//! Schema-1 project persistence and portable paths. No build approval is inferred
//! from a saved project; readiness validation belongs to the processing pipeline.
pub mod asset_paths;
pub mod customization;
pub mod presets;

use asset_paths::{decode, encode, is_absolute, relative_asset, relative_to, resolve, text};
pub use customization::{Customization, MaterialChoices, PreparationOptions};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Validation(String),
    #[error("Could not access project files: {0}")]
    Io(#[from] std::io::Error),
    #[error("Could not read project data: {0}")]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
macro_rules! named_enum {
    ($name:ident, $default:ident, [$($variant:ident => $text:literal),* $(,)?]) => {
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
        pub enum $name { $(#[serde(rename=$text)] $variant,)* }
        impl Default for $name { fn default() -> Self { Self::$default } }
    }
}
named_enum!(ModelInputMode, Prepared, [Prepared=>"prepared", RiggedHumanoid=>"rigged_humanoid", UnriggedHumanoid=>"unrigged_humanoid", Weapon=>"weapon"]);
named_enum!(LodStrategy, Reuse, [Choose=>"choose", Single=>"single", Reuse=>"reuse", Preserve=>"preserve", Disable=>"disable", Custom=>"custom"]);
named_enum!(Ag2Mode, Auto, [Auto=>"auto", Cswin=>"cswin", Preserve=>"preserve", Adapter=>"adapter"]);
named_enum!(AnimationPolicy, Preserve, [Preserve=>"preserve", DisableLegacy=>"disable_legacy"]);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub steam: String,
    pub deadlock: String,
    pub pak: String,
    pub csdk: String,
    pub resourcecompiler: String,
    pub cswin: String,
    pub cswin_resourcecompiler: String,
    pub model_backend: String,
    pub model_game: String,
    pub model_format: String,
    pub optional_cs2: String,
    pub source2viewer: String,
    pub vpk_tool: String,
    pub vpk_backend: String,
    pub ag2_adapter: String,
    pub workspace: String,
    pub last_project: String,
    pub blender: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            steam: String::new(),
            deadlock: String::new(),
            pak: String::new(),
            csdk: String::new(),
            resourcecompiler: String::new(),
            cswin: String::new(),
            cswin_resourcecompiler: String::new(),
            model_backend: "auto".into(),
            model_game: String::new(),
            model_format: "auto".into(),
            optional_cs2: String::new(),
            source2viewer: String::new(),
            vpk_tool: String::new(),
            vpk_backend: "builtin".into(),
            ag2_adapter: String::new(),
            workspace: String::new(),
            last_project: String::new(),
            blender: String::new(),
        }
    }
}
impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        atomic_json(path, self)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MeshMapping {
    One(String),
    Many(Vec<String>),
}
impl MeshMapping {
    pub fn targets(&self) -> Result<Vec<&str>> {
        match self {
            Self::One(v) if v == "@keep" || v == "@disable" => Ok(Vec::new()),
            Self::One(v) if !v.is_empty() => Ok(vec![v.as_str()]),
            Self::Many(v) if !v.is_empty() && v.iter().all(|s| !s.is_empty() && s != "@keep" && s != "@disable") && v.iter().collect::<BTreeSet<_>>().len() == v.len() => Ok(v.iter().map(String::as_str).collect()),
            _ => Err(Error::Validation("Mesh mapping must be a mesh name, unique nonempty mesh list, Keep Original or Disable.".into())),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Project {
    pub model_input_mode: ModelInputMode,
    pub preparation_source: String,
    pub preparation_blend: String,
    pub preparation_report: String,
    pub preparation_options: PreparationOptions,
    pub customization: Customization,
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub hero_name: String,
    pub original_model: String,
    pub dmx: String,
    pub textures: String,
    pub source_root: String,
    pub imported_source: String,
    pub original_compiled: String,
    pub source_overlay: String,
    pub mesh_template: String,
    pub material_mapping: String,
    pub material_remap: String,
    pub skeleton_reference: String,
    pub expected_dmx_sha256: String,
    pub meshes: BTreeMap<String, MeshMapping>,
    pub materials: BTreeMap<String, MaterialChoices>,
    pub lod_strategy: LodStrategy,
    pub ag2_mode: Ag2Mode,
    pub animation_policy: AnimationPolicy,
    pub vpk_number: u32,
    pub auto_decompile: bool,
    pub auto_map: bool,
    pub generate_materials: bool,
    pub compile_model: bool,
    pub build_vpk: bool,
    pub install: bool,
    pub review_lods: bool,
    pub allow_unskinned: bool,
    pub auto_split: bool,
    pub split_target: u32,
    #[serde(skip)]
    resolved_source: String,
    #[serde(skip)]
    resolved_textures: String,
    #[serde(skip)]
    pub rebase_log: Vec<String>,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            model_input_mode: ModelInputMode::default(),
            preparation_source: String::new(),
            preparation_blend: String::new(),
            preparation_report: String::new(),
            preparation_options: PreparationOptions::default(),
            customization: Customization::default(),
            schema: 1,
            id: uuid::Uuid::new_v4().simple().to_string(),
            name: "my_character".into(),
            hero_name: String::new(),
            original_model: String::new(),
            dmx: String::new(),
            textures: String::new(),
            source_root: String::new(),
            imported_source: String::new(),
            original_compiled: String::new(),
            source_overlay: String::new(),
            mesh_template: String::new(),
            material_mapping: String::new(),
            material_remap: String::new(),
            skeleton_reference: String::new(),
            expected_dmx_sha256: String::new(),
            meshes: BTreeMap::new(),
            materials: BTreeMap::new(),
            lod_strategy: LodStrategy::default(),
            ag2_mode: Ag2Mode::default(),
            animation_policy: AnimationPolicy::default(),
            vpk_number: 18,
            auto_decompile: true,
            auto_map: true,
            generate_materials: true,
            compile_model: true,
            build_vpk: true,
            install: true,
            review_lods: false,
            allow_unskinned: false,
            auto_split: false,
            split_target: 400000,
            resolved_source: String::new(),
            resolved_textures: String::new(),
            rebase_log: Vec::new(),
        }
    }
}
impl Project {
    pub fn hero_root(&self) -> String {
        if self.original_model.is_empty() {
            return String::new();
        }
        self.original_model
            .rsplit_once('/')
            .map(|(p, _)| p.to_owned())
            .unwrap_or_else(|| ".".into())
    }
    pub fn source_model(&self) -> &str {
        self.original_model
            .strip_suffix("_c")
            .unwrap_or(&self.original_model)
    }
    pub fn validate_draft(&self) -> Result<()> {
        self.validate(false)
    }
    pub fn validate_action(&self) -> Result<()> {
        self.validate(true)
    }
    pub fn validate(&self, require_original: bool) -> Result<()> {
        self.customization.validate()?;
        if self.schema != 1 {
            return Err(Error::Validation(
                "This project uses an unsupported profile version.".into(),
            ));
        }
        if self.id.len() != 32
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Validation("Invalid project identifier.".into()));
        }
        if !(1..=41).contains(&self.name.len())
            || !self.name.as_bytes()[0].is_ascii_alphabetic()
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b' ')
        {
            return Err(Error::Validation(
                "Project name must start with a letter and contain 1–41 letters, numbers, spaces or underscores.".into(),
            ));
        }
        if self.original_model.is_empty() {
            if require_original {
                return Err(Error::Validation("Select a hero / original model before preparing, analyzing or building this project.".into()));
            }
        } else {
            relative_asset(&self.original_model)?;
            if !self.original_model.ends_with(".vmdl") && !self.original_model.ends_with(".vmdl_c")
            {
                return Err(Error::Validation(
                    "Select an original VMDL hero resource.".into(),
                ));
            }
        }
        if !(1..=99).contains(&self.vpk_number) {
            return Err(Error::Validation(
                "Addon VPK number must be 1–99. Installation only targets game/citadel/addons."
                    .into(),
            ));
        }
        if !(3..524288).contains(&self.split_target) {
            return Err(Error::Validation("Split target must be an integer from 3 through 524287; Valve hard limit is fixed at 524288.".into()));
        }
        self.expanded_mappings()?;
        Ok(())
    }
    pub fn expanded_mappings(&self) -> Result<BTreeMap<String, String>> {
        let mut result = BTreeMap::new();
        for (name, value) in &self.meshes {
            let targets = value.targets()?;
            let rows: Vec<_> = if targets.len() > 1 {
                targets
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (format!("{name}__part{:02}", i + 1), t.to_string()))
                    .collect()
            } else {
                vec![(
                    name.clone(),
                    targets
                        .first()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| match value {
                            MeshMapping::One(s) => s.clone(),
                            _ => unreachable!(),
                        }),
                )]
            };
            for (output, target) in rows {
                if result.contains_key(&output)
                    || (&output != name && self.meshes.contains_key(&output))
                {
                    return Err(Error::Validation(format!(
                        "Split mesh name collides with an original render node: {output}"
                    )));
                }
                result.insert(output, target);
            }
        }
        Ok(result)
    }
    fn paths_mut(&mut self) -> [(&'static str, &mut String); 13] {
        [
            ("preparation_source", &mut self.preparation_source),
            ("preparation_blend", &mut self.preparation_blend),
            ("preparation_report", &mut self.preparation_report),
            ("dmx", &mut self.dmx),
            ("textures", &mut self.textures),
            ("source_root", &mut self.source_root),
            ("imported_source", &mut self.imported_source),
            ("original_compiled", &mut self.original_compiled),
            ("source_overlay", &mut self.source_overlay),
            ("mesh_template", &mut self.mesh_template),
            ("material_mapping", &mut self.material_mapping),
            ("material_remap", &mut self.material_remap),
            ("skeleton_reference", &mut self.skeleton_reference),
        ]
    }
    pub fn rebase_assets(&mut self, old_source: &str, old_textures: &str) -> Result<Vec<String>> {
        let current_source = if self.dmx.is_empty() {
            self.source_root.clone()
        } else {
            text(
                Path::new(&self.dmx)
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new(".")),
            )?
        };
        let old_source = if !old_source.is_empty() {
            old_source
        } else if !self.resolved_source.is_empty() {
            &self.resolved_source
        } else {
            &self.source_root
        }
        .to_owned();
        let mut old_textures = if old_textures.is_empty() {
            self.resolved_textures.clone()
        } else {
            old_textures.to_owned()
        };
        let mut messages = Vec::new();
        if let Some(rel) = relative_to(&self.textures, &old_source)? {
            if old_source != current_source {
                let previous = self.textures.clone();
                self.textures = text(&Path::new(&current_source).join(rel))?;
                messages.push(format!(
                    "Rebased textures root: {previous} → {}",
                    self.textures
                ));
                if old_textures.is_empty() {
                    old_textures = previous;
                }
            }
        }
        let roots = [
            (old_textures, self.textures.clone()),
            (old_source, current_source.clone()),
        ];
        let fix = |value: &str, label: &str, messages: &mut Vec<String>| -> Result<String> {
            if value.is_empty() {
                return Ok(value.into());
            }
            let mut new = if let Some(suffix) = value.strip_prefix("@textures/") {
                Some(asset_paths::under(&self.textures, suffix)?)
            } else if let Some(suffix) = value.strip_prefix("@source/") {
                Some(asset_paths::under(&current_source, suffix)?)
            } else {
                None
            };
            if new.is_none() {
                for (old, current) in &roots {
                    if let Some(rel) = relative_to(value, old)? {
                        if old != current && !current.is_empty() {
                            new = Some(text(&Path::new(current).join(rel))?);
                            break;
                        }
                    }
                }
                if new.is_none() && !Path::new(value).exists() {
                    new = asset_paths::recover(value, &[&self.textures, &current_source])?;
                }
            }
            if let Some(new) = new {
                if new != value {
                    messages.push(format!("Rebased {label}: {value} → {new}"));
                }
                Ok(new)
            } else {
                Ok(value.into())
            }
        };
        // Compute replacements before mutating fields borrowed by the resolver.
        let package: Vec<_> = [
            ("source_overlay", &self.source_overlay),
            ("mesh_template", &self.mesh_template),
            ("material_mapping", &self.material_mapping),
            ("material_remap", &self.material_remap),
            ("skeleton_reference", &self.skeleton_reference),
        ]
        .into_iter()
        .map(|(key, value)| Ok((key, fix(value, key, &mut messages)?)))
        .collect::<Result<_>>()?;
        let mut materials = self.materials.clone();
        for (name, material) in &mut materials {
            if let Some(textures) = &mut material.textures {
                for (channel, value) in textures {
                    *value = fix(value, &format!("{name} / {channel}"), &mut messages)?;
                }
            }
            material.candidates = None;
        }
        self.materials = materials;
        for (key, value) in package {
            match key {
                "source_overlay" => self.source_overlay = value,
                "mesh_template" => self.mesh_template = value,
                "material_mapping" => self.material_mapping = value,
                "material_remap" => self.material_remap = value,
                "skeleton_reference" => self.skeleton_reference = value,
                _ => unreachable!(),
            }
        }
        self.source_root = current_source.clone();
        self.resolved_source = current_source;
        self.resolved_textures = self.textures.clone();
        self.rebase_log.extend(messages.clone());
        Ok(messages)
    }
    pub fn save(&mut self, path: &Path) -> Result<()> {
        self.validate_draft()?;
        self.rebase_assets("", "")?;
        let mut encoded = self.clone();
        let profile = resolve(path)?;
        let directory = profile.parent().unwrap_or_else(|| Path::new("."));
        for (_, value) in encoded.paths_mut() {
            *value = encode(value, directory)?;
        }
        encoded
            .customization
            .map_sources(|value| encode(value, directory))?;
        for material in encoded.materials.values_mut() {
            material.remove_computed();
            if let Some(textures) = &mut material.textures {
                for value in textures.values_mut() {
                    if let Some(rel) = relative_to(value, &self.textures)? {
                        *value = format!("@textures/{rel}");
                    } else if let Some(rel) = relative_to(value, &self.source_root)? {
                        *value = format!("@source/{rel}");
                    } else {
                        *value = encode(value, directory)?;
                    }
                }
            }
        }
        let mut value = serde_json::to_value(encoded)?;
        value
            .as_object_mut()
            .expect("Project serializes an object")
            .insert("hero_root".into(), self.hero_root().into());
        atomic_json(path, &value)
    }
    pub fn load(path: &Path) -> Result<Self> {
        Self::load_from_bytes(path, &fs::read(path)?)
    }
    /// Decode an already bounded immutable snapshot while resolving relative
    /// assets against the authored project path. Never rereads the project.
    pub fn load_from_bytes(path: &Path, bytes: &[u8]) -> Result<Self> {
        let mut project: Self = serde_json::from_slice(bytes)?;
        let profile = resolve(path)?;
        let directory = profile.parent().unwrap_or_else(|| Path::new("."));
        project
            .customization
            .map_sources(|value| decode(value, directory))?;
        let mut messages = Vec::new();
        for (key, value) in project.paths_mut() {
            let original = value.clone();
            let mut resolved = decode(value, directory)?;
            if !value.is_empty() && is_absolute(value) && !Path::new(value).exists() {
                if let Some(rebased) = asset_paths::recover(value, &[&text(directory)?])? {
                    messages.push(format!("Rebased {key}: {value} → {rebased}"));
                    resolved = rebased;
                }
            }
            *value = if original.is_empty() {
                original
            } else {
                resolved
            };
        }
        project.rebase_log.extend(messages);
        for material in project.materials.values_mut() {
            if let Some(textures) = &mut material.textures {
                for value in textures.values_mut() {
                    if !value.starts_with("@source/") && !value.starts_with("@textures/") {
                        *value = decode(value, directory)?;
                    }
                }
            }
        }
        project.rebase_assets("", "")?;
        project.validate_draft()?;
        Ok(project)
    }
    pub fn change_hero(&mut self, name: String, path: String) {
        if self.original_model != path {
            self.customization.clear_targets();
            self.meshes.clear();
            self.materials.clear();
            self.review_lods = false;
            self.allow_unskinned = false;
            self.imported_source.clear();
            self.original_compiled.clear();
            self.preparation_report.clear();
            self.preparation_blend.clear();
            self.preparation_options = PreparationOptions::default();
        }
        self.hero_name = name;
        self.original_model = path;
    }
}
/// Same-directory temporary write + sync + atomic replacement; failure keeps the
/// previous document. No user-visible file is opened/truncated before validation.
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(".builder-{}", uuid::Uuid::new_v4().simple()));
    let operation = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        // std::fs::rename uses MoveFileExW(REPLACE_EXISTING) on Windows.
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    operation
}
