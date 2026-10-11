//! Portable hero-bound outfit choices are inputs, never execution settings or
//! transferable preparation/build approval.
use crate::customization::{Clothing, EquipmentMesh, Face, NumericChoice, Prop};
use crate::{
    asset_paths, atomic_json, Customization, Error, LodStrategy, MaterialChoices, MeshMapping,
    ModelInputMode, PreparationOptions, Project, Result,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
pub const FORMAT: &str = "deadlock-character-outfit";
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
macro_rules! choice_fields {
    ($($field:ident : $ty:ty),* $(,)?)=>{
        #[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
        #[serde(deny_unknown_fields)]
        pub struct OutfitChoices{$(pub $field:$ty,)*}
        impl OutfitChoices{
            fn capture(p:&Project)->Self{Self{$($field:p.$field.clone(),)*}}
            fn apply(&self,p:&mut Project){$(p.$field=self.$field.clone();)*}
        }
    }
}
choice_fields! {
    model_input_mode:ModelInputMode,preparation_source:String,preparation_options:PreparationOptions,dmx:String,
    textures:String,source_root:String,imported_source:String,original_compiled:String,source_overlay:String,
    mesh_template:String,material_mapping:String,material_remap:String,skeleton_reference:String,
    expected_dmx_sha256:String,meshes:BTreeMap<String,MeshMapping>,materials:BTreeMap<String,MaterialChoices>,
    customization:Customization,lod_strategy:LodStrategy,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutfitPreset {
    pub format: String,
    pub schema: u32,
    pub name: String,
    pub original_model: String,
    #[serde(default)]
    pub hero_name: String,
    pub choices: OutfitChoices,
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn hero(value: &str) -> String {
    deadlock_core::text::casefold(value.strip_suffix("_c").unwrap_or(value))
}
fn finite_materials(materials: &BTreeMap<String, MaterialChoices>) -> bool {
    materials.values().all(|m| {
        m.color.is_none_or(|c| c.into_iter().all(f64::is_finite))
            && [&m.roughness, &m.metalness]
                .into_iter()
                .all(|n| !matches!(n,Some(NumericChoice::Value(v))if !v.is_finite()))
    })
}
impl OutfitPreset {
    pub fn validate(&self) -> Result<()> {
        if self.format != FORMAT || self.schema != 1 {
            return Err(invalid("Unsupported character outfit preset format."));
        }
        if self.name.trim().is_empty()
            || self.name.trim().chars().count() > 100
            || self.name.contains('\0')
        {
            return Err(invalid(
                "Give the outfit a name between 1 and 100 characters.",
            ));
        }
        let mut p = Project::default();
        p.original_model = self.original_model.clone();
        self.choices.apply(&mut p);
        p.validate_action()?;
        for (_, path) in p.paths_mut() {
            if path.contains('\0') {
                return Err(invalid("Invalid outfit source path."));
            }
        }
        if self.choices.expected_dmx_sha256.contains('\0') {
            return Err(invalid("Invalid outfit source fingerprint."));
        }
        if !finite_materials(&self.choices.materials) {
            return Err(invalid("Outfit choices must contain finite JSON values."));
        }
        let encoded = serde_json::to_value(self)?;
        fn separators(value: &serde_json::Value) -> usize {
            match value {
                serde_json::Value::Object(rows) => {
                    rows.len()
                        + rows.len().saturating_sub(1)
                        + rows.values().map(separators).sum::<usize>()
                }
                serde_json::Value::Array(rows) => {
                    rows.len().saturating_sub(1) + rows.iter().map(separators).sum::<usize>()
                }
                _ => 0,
            }
        }
        if serde_json::to_vec(&encoded)?
            .len()
            .saturating_add(separators(&encoded))
            > MAX_BYTES
        {
            return Err(invalid("Outfit preset exceeds the 4 MiB choice limit."));
        }
        Ok(())
    }
    pub fn capture(project: &Project, name: &str) -> Result<Self> {
        project.validate_action()?;
        let mut choices = OutfitChoices::capture(project);
        for m in choices.materials.values_mut() {
            m.remove_computed();
        }
        let preset = Self {
            format: FORMAT.into(),
            schema: 1,
            name: if name.is_empty() {
                project.name.replace('_', " ")
            } else {
                name.into()
            },
            original_model: project.original_model.clone(),
            hero_name: project.hero_name.clone(),
            choices,
        };
        preset.validate()?;
        Ok(preset)
    }
    fn map_paths(&mut self, mut transform: impl FnMut(&str) -> Result<String>) -> Result<()> {
        let c = &mut self.choices;
        for value in [
            &mut c.preparation_source,
            &mut c.dmx,
            &mut c.textures,
            &mut c.source_root,
            &mut c.imported_source,
            &mut c.original_compiled,
            &mut c.source_overlay,
            &mut c.mesh_template,
            &mut c.material_mapping,
            &mut c.material_remap,
            &mut c.skeleton_reference,
        ] {
            if !value.is_empty() {
                *value = transform(value)?;
            }
        }
        c.customization.map_sources(&mut transform)?;
        for m in c.materials.values_mut() {
            if let Some(textures) = &mut m.textures {
                for value in textures.values_mut() {
                    if !value.is_empty()
                        && !value.starts_with("@source/")
                        && !value.starts_with("@textures/")
                    {
                        *value = transform(value)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn apply(&self, project: &Project) -> Result<Project> {
        self.validate()?;
        project.validate_action()?;
        if hero(&project.original_model) != hero(&self.original_model) {
            return Err(invalid(format!(
                "This outfit belongs to a different hero. Select its original hero before loading it: {}",
                if self.hero_name.is_empty() {
                    &self.original_model
                } else {
                    &self.hero_name
                }
            )));
        }
        let choices = &self.choices;
        let changed = project.model_input_mode != choices.model_input_mode
            || project.preparation_source != choices.preparation_source
            || project.preparation_options != choices.preparation_options
            || project.dmx != choices.dmx
            || project.source_overlay != choices.source_overlay
            || project.mesh_template != choices.mesh_template
            || project.material_mapping != choices.material_mapping
            || project.material_remap != choices.material_remap
            || project.skeleton_reference != choices.skeleton_reference
            || project.expected_dmx_sha256 != choices.expected_dmx_sha256
            || project.imported_source != choices.imported_source
            || project.original_compiled != choices.original_compiled
            || geometry(&project.customization) != geometry(&choices.customization);
        let mapping_changed = project.meshes != choices.meshes
            || mesh_choices(&project.customization) != mesh_choices(&choices.customization);
        let mut result = project.clone();
        choices.apply(&mut result);
        for m in result.materials.values_mut() {
            m.remove_computed();
        }
        if changed {
            result.preparation_report.clear();
            result.preparation_blend.clear();
            result.review_lods = false;
        }
        if mapping_changed {
            result.review_lods = false;
        }
        result.validate_action()?;
        Ok(result)
    }
    pub fn save(project: &Project, path: &Path, name: &str) -> Result<()> {
        let path = asset_paths::resolve(path)?;
        let mut preset = Self::capture(project, name)?;
        let mut sources = Vec::new();
        preset.map_paths(|value| {
            sources.push(value.to_owned());
            Ok(value.into())
        })?;
        for value in sources {
            if asset_paths::resolve(Path::new(&value))? == path {
                return Err(invalid(
                    "Save the outfit under a new filename; this path is one of its source assets.",
                ));
            }
        }
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        preset.map_paths(|value| asset_paths::encode(value, directory))?;
        preset.validate()?;
        atomic_json(&path, &preset)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let path = asset_paths::resolve(path)?;
        if !path.is_file() || path.metadata()?.len() > MAX_BYTES as u64 {
            return Err(invalid("Choose an outfit preset smaller than 4 MiB."));
        }
        let data = fs::read(&path)?;
        if data.len() > MAX_BYTES {
            return Err(invalid("Choose an outfit preset smaller than 4 MiB."));
        }
        let data = data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&data);
        let mut result: Self = serde_json::from_slice(data)?;
        result.validate()?;
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        result.map_paths(|value| asset_paths::decode(value, directory))?;
        result.validate()?;
        Ok(result)
    }
}
#[derive(PartialEq)]
struct Geometry<'a> {
    clothing: Option<&'a Clothing>,
    face: Option<&'a Face>,
    props: Vec<&'a Prop>,
}
fn geometry(state: &Customization) -> Geometry<'_> {
    if !state.active() {
        return Geometry {
            clothing: None,
            face: None,
            props: Vec::new(),
        };
    }
    Geometry {
        clothing: state
            .clothing
            .as_ref()
            .filter(|c| c.enabled.unwrap_or(false)),
        face: state.face.as_ref().filter(|f| f.enabled.unwrap_or(false)),
        props: state
            .equipment
            .as_ref()
            .filter(|e| e.enabled.unwrap_or(false))
            .and_then(|e| e.props.as_ref())
            .map(|p| p.iter().filter(|p| p.enabled.unwrap_or(true)).collect())
            .unwrap_or_default(),
    }
}
fn mesh_choices(state: &Customization) -> &[EquipmentMesh] {
    if !state.active() {
        return &[];
    }
    state
        .equipment
        .as_ref()
        .filter(|e| e.enabled.unwrap_or(false))
        .and_then(|e| e.meshes.as_deref())
        .unwrap_or_default()
}
