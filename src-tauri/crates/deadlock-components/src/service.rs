//! Independent component staging, native compilation and frozen/install revalidation.
use crate::{
    CancellationToken, Result, compiled, fail, icons, motion, read, read_file,
    resources::{self, ResourceEntry, ResourceKind, ResourceReceipt},
    sha,
};
use deadlock_core::{TimingRecorder, process::ToolExecutor, safety, text::casefold};
use deadlock_project::{ModelInputMode, Project, Settings, customization::ResourceReplacement};
use deadlock_tools::{compiler, toolchain};
use deadlock_vpk::{Hashes, Vpk, resource_references};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClothingReadiness {
    pub ready: bool,
    pub mode: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentReceipt {
    pub schema: u32,
    pub hero_path: String,
    pub model_enabled: bool,
    pub icons: icons::IconReceipt,
    pub animations: motion::AnimationReceipt,
    pub effects: ResourceReceipt,
    pub audio: ResourceReceipt,
    pub files: Hashes,
    pub original_resources: Hashes,
    pub prepared_content: PathBuf,
    pub clothing: Option<ClothingReadiness>,
    pub native_validated: bool,
    pub in_game_validated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInspection {
    pub model_enabled: bool,
    pub icons: Vec<icons::IconEntry>,
    pub animations: Vec<motion::AnimationEntry>,
    pub effects: Vec<ResourceEntry>,
    pub audio: Vec<ResourceEntry>,
    pub clothing: Option<ClothingReadiness>,
    pub native_compile_required: bool,
    pub warnings: Vec<String>,
    pub native_validated: bool,
    pub in_game_validated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationDiscovery {
    #[serde(flatten)]
    pub row: motion::AnimationRow,
    pub path: String,
    pub supported: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentCatalog {
    pub hero_path: String,
    pub icons: Vec<icons::IconDiscovery>,
    pub animations: Vec<AnimationDiscovery>,
    pub effects: Vec<resources::CatalogRow>,
    pub audio: Vec<resources::CatalogRow>,
    pub variants: Vec<crate::variants::Variant>,
    pub animation_unavailable: Option<String>,
    pub warnings: Vec<String>,
    pub includes_animations: bool,
}
pub struct ComponentService<'a> {
    pub settings: &'a Settings,
    pub runner: &'a dyn ToolExecutor,
    pub cancel: &'a CancellationToken,
    pub clock: &'a TimingRecorder,
    pub log: &'a dyn Fn(&str),
}
#[derive(Default)]
struct Selection {
    icons: Vec<ResourceReplacement>,
    animations: Vec<ResourceReplacement>,
    effects: Vec<ResourceReplacement>,
    audio: Vec<ResourceReplacement>,
    clothing: Option<ClothingReadiness>,
}
impl Selection {
    fn any(&self) -> bool {
        !self.icons.is_empty()
            || !self.animations.is_empty()
            || !self.effects.is_empty()
            || !self.audio.is_empty()
    }
    fn rows(&self) -> impl Iterator<Item = &ResourceReplacement> {
        self.icons
            .iter()
            .chain(&self.animations)
            .chain(&self.effects)
            .chain(&self.audio)
    }
}
pub fn clothing_readiness(mode: &str) -> Result<ClothingReadiness> {
    if !["preserve", "weighted", "simulated"].contains(&mode) {
        return Err(fail("Unknown clothing motion choice."));
    }
    let ready = mode != "simulated";
    Ok(ClothingReadiness{ready,mode:mode.into(),message:if ready{"Existing cloth preservation remains enabled. Weighted garments move with the model skeleton; they do not gain independent simulation."}else{"New dynamic cloth authoring is not ready for packaging. It needs a Source 2 cloth proxy, pinned vertices, solver controls, render bindings and collision shapes, followed by native compiler and in-game validation. Use weighted clothing while this exporter is being developed."}.into()})
}
fn selection(project: &Project) -> Result<Selection> {
    project.customization.validate().map_err(fail)?;
    let value = &project.customization;
    let mut chosen = Selection::default();
    if value.active() {
        for (destination, source) in [
            (&mut chosen.icons, &value.icons),
            (&mut chosen.animations, &value.animations),
            (&mut chosen.effects, &value.effects),
            (&mut chosen.audio, &value.audio),
        ] {
            destination.extend(
                source
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter(|row| row.enabled.unwrap_or(true))
                    .cloned(),
            );
        }
    }
    for row in chosen.rows() {
        if row.source.as_deref().unwrap_or_default().trim().is_empty() {
            return Err(fail(
                "Add a source file for each enabled character replacement, or remove its empty row.",
            ));
        }
        if row.target.as_deref().unwrap_or_default().trim().is_empty() {
            return Err(fail(
                "Choose which hero asset to replace. Find character assets first.",
            ));
        }
    }
    if value.active() {
        let equipment = value.equipment.as_ref();
        if value
            .physics
            .as_ref()
            .is_some_and(|p| p.enabled.unwrap_or(false))
            && equipment.is_some_and(|e| {
                e.variants
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|v| v.enabled.unwrap_or(true))
            })
        {
            return Err(fail(
                "Build tested cloth physics for one replacement model at a time. Disable alternate model projects for this build.",
            ));
        }
        if let Some(garment) = value
            .clothing
            .as_ref()
            .filter(|c| c.enabled.unwrap_or(false))
        {
            if !value.model_enabled() {
                return Err(fail(
                    "Include a replacement model to add clothing. Clothing is prepared with that model.",
                ));
            }
            let readiness = clothing_readiness(garment.mode.as_deref().unwrap_or("weighted"))?;
            if !readiness.ready {
                return Err(fail(&readiness.message));
            }
            if project.model_input_mode == ModelInputMode::Prepared {
                return Err(fail(
                    "Use Prepare with your Blender, GLB, glTF or FBX model to add this garment. If clothing is already included in your DMX, turn off Add clothing.",
                ));
            }
            deadlock_preparation::service::check_handoff(project).map_err(fail)?;
            chosen.clothing = Some(readiness);
        }
        let geometry = equipment.is_some_and(|e| {
            e.enabled.unwrap_or(false)
                && e.props
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|p| p.enabled.unwrap_or(true))
        }) || value.face.as_ref().is_some_and(|f| {
            f.enabled.unwrap_or(false) && f.weights.as_ref().is_some_and(|w| !w.is_empty())
        });
        let mesh = equipment.is_some_and(|e| {
            e.enabled.unwrap_or(false) && e.meshes.as_ref().is_some_and(|m| !m.is_empty())
        });
        let appearance = value.appearance.as_ref().is_some_and(|a| {
            a.enabled.unwrap_or(false)
                && a.materials
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|m| m.enabled.unwrap_or(true))
        });
        if geometry || mesh || appearance {
            if !value.model_enabled() {
                return Err(fail(
                    "Include a replacement model to use Appearance, Equipment or a static facial expression.",
                ));
            }
            if geometry {
                deadlock_preparation::service::check_handoff(project).map_err(fail)?;
            }
        }
    }
    if !value.model_enabled() && !chosen.any() {
        return Err(fail(
            "Add an icon, animation, effect or audio replacement, or include a replacement model before building.",
        ));
    }
    Ok(chosen)
}
pub fn input_paths(project: &Project) -> Result<Vec<PathBuf>> {
    let value = &project.customization;
    let mut paths = vec![];
    for rows in [
        &value.icons,
        &value.animations,
        &value.effects,
        &value.audio,
    ] {
        paths.extend(
            rows.as_deref()
                .unwrap_or_default()
                .iter()
                .filter_map(|r| r.source.as_ref())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
    }
    if let Some(source) = value
        .clothing
        .as_ref()
        .and_then(|v| v.source.as_ref())
        .filter(|s| !s.is_empty())
    {
        paths.push(source.into());
    }
    if let Some(source) = value
        .physics
        .as_ref()
        .and_then(|v| v.source.as_ref())
        .filter(|s| !s.is_empty())
    {
        paths.push(source.into());
    }
    if let Some(e) = &value.equipment {
        paths.extend(
            e.props
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter_map(|p| p.source.as_ref())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
        paths.extend(
            e.variants
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.project.as_ref())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
    }
    for path in paths.clone() {
        if let Some(dependency) = motion::source_dependency(&path)? {
            paths.push(dependency);
        }
    }
    Ok(paths)
}
fn targets(receipt: &ComponentReceipt) -> Result<Vec<String>> {
    let targets: Vec<String> = receipt
        .icons
        .entries
        .iter()
        .map(|e| e.target.clone())
        .chain(
            receipt
                .animations
                .replacements
                .iter()
                .map(|e| e.target.clone()),
        )
        .chain(receipt.effects.entries.iter().map(|e| e.target.clone()))
        .chain(receipt.audio.entries.iter().map(|e| e.target.clone()))
        .collect();
    let mut seen = BTreeSet::new();
    for target in &targets {
        safety::relative_asset(target)?;
        if !seen.insert(casefold(target)) {
            return Err(fail(
                "Character component target is duplicated across categories.",
            ));
        }
    }
    Ok(targets)
}
fn original_hashes(receipt: &ComponentReceipt) -> Result<Hashes> {
    let mut result = Hashes::new();
    for (target, hash) in receipt
        .icons
        .entries
        .iter()
        .map(|e| (&e.target, &e.original_sha256))
        .chain(
            receipt
                .animations
                .replacements
                .iter()
                .map(|e| (&e.target, &e.original_sha256)),
        )
        .chain(
            receipt
                .effects
                .entries
                .iter()
                .map(|e| (&e.target, &e.original_sha256)),
        )
        .chain(
            receipt
                .audio
                .entries
                .iter()
                .map(|e| (&e.target, &e.original_sha256)),
        )
    {
        if result.insert(target.clone(), hash.clone()).is_some() {
            return Err(fail("Duplicate character component target."));
        }
    }
    Ok(result)
}

fn verify_audio_choices(project: &Project, receipt: &ComponentReceipt) -> Result<()> {
    let selected = selection(project)?;
    if selected.audio.len() != receipt.audio.entries.len() {
        return Err(fail("Sound replacements changed. Rebuild before installing."));
    }
    for row in &selected.audio {
        let entry = receipt.audio.entries.iter().find(|entry| casefold(&entry.target)==casefold(row.target.as_deref().unwrap_or_default()))
            .ok_or_else(|| fail("Sound destination changed. Rebuild before installing."))?;
        if row.volume_percent.unwrap_or(100) != entry.volume_percent.unwrap_or(100)
            || row.audio_fit.as_deref().unwrap_or("exact") != entry.audio_fit.as_deref().unwrap_or("exact") {
            return Err(fail("Replacement sound volume or timing changed. Rebuild before installing."));
        }
    }
    Ok(())
}
fn closure(archive: &Vpk, targets: &[String], cancel: &CancellationToken) -> Result<Hashes> {
    let mut pending = targets.to_vec();
    let mut hashes = Hashes::new();
    while let Some(path) = pending.pop() {
        cancel.check()?;
        let wanted = compiled(&path)?;
        let entry = archive.entries.get(&casefold(&wanted)).ok_or_else(|| {
            fail(format!(
                "Character component dependency is missing from this game VPK: {wanted}"
            ))
        })?;
        if hashes.contains_key(&entry.path) {
            continue;
        }
        if hashes.len() >= 20000 {
            return Err(fail(
                "Character component dependency graph exceeds 20,000 resources.",
            ));
        }
        let raw = read(archive, &entry.path)?;
        hashes.insert(entry.path.clone(), sha(&raw));
        pending.extend(resource_references(&raw).map_err(fail)?);
    }
    Ok(hashes)
}
fn inventory(root: &Path, cancel: &CancellationToken) -> Result<Hashes> {
    fn walk(root: &Path, path: &Path, cancel: &CancellationToken, out: &mut Hashes) -> Result<()> {
        deadlock_vpk::check_unlinked_path(path, false).map_err(fail)?;
        for entry in fs::read_dir(path).map_err(fail)? {
            cancel.check()?;
            let entry = entry.map_err(fail)?;
            let path = entry.path();
            deadlock_vpk::check_unlinked_path(&path, false).map_err(fail)?;
            if path.is_dir() {
                walk(root, &path, cancel, out)?;
            } else if path.is_file() {
                if path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with("_c"))
                {
                    let relative = path
                        .strip_prefix(root)
                        .map_err(fail)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.insert(relative, safety::digest_with_cancel(&path, cancel)?);
                }
            } else {
                return Err(fail("Special file in component output tree."));
            }
        }
        Ok(())
    }
    let mut out = Hashes::new();
    if root.exists() {
        walk(root, root, cancel, &mut out)?;
    }
    Ok(out)
}
fn verify_outputs(
    archive: &Vpk,
    receipt: &mut ComponentReceipt,
    game: &Path,
    cancel: &CancellationToken,
    expected: Option<&Hashes>,
) -> Result<Hashes> {
    let targets = targets(receipt)?;
    let target_keys: BTreeSet<_> = targets.iter().map(|p| casefold(p)).collect();
    if let Some(expected) = expected {
        if expected.keys().cloned().collect::<BTreeSet<_>>() != targets.iter().cloned().collect() {
            return Err(fail(
                "Component receipt file allowlist differs from its selected entries.",
            ));
        }
    }
    for target in &targets {
        cancel.check()?;
        let raw = read_file(&safety::inside(game, target)?)?;
        if expected.is_some_and(|m| m.get(target) != Some(&sha(&raw))) {
            return Err(fail(format!(
                "Character component changed after compilation: {target}"
            )));
        }
        for dependency in resource_references(&raw).map_err(fail)? {
            let path = casefold(&compiled(&dependency)?);
            if !archive.entries.contains_key(&path) && !target_keys.contains(&path) {
                return Err(fail(format!(
                    "Character component references an unprovided game asset: {dependency}"
                )));
            }
        }
    }
    icons::verify_compiled(&mut receipt.icons, game, cancel)?;
    motion::verify_compiled(archive, &receipt.animations, game, cancel)?;
    resources::verify_compiled(archive, &receipt.effects, game, cancel)?;
    resources::verify_compiled(archive, &receipt.audio, game, cancel)?;
    targets
        .into_iter()
        .map(|p| {
            Ok((
                p.clone(),
                safety::digest_with_cancel(&safety::inside(game, &p)?, cancel)?,
            ))
        })
        .collect()
}
pub fn verify_snapshot(
    archive: &Vpk,
    receipt: &ComponentReceipt,
    directory: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    if !archive
        .entries
        .contains_key(&casefold(&compiled(&receipt.hero_path)?))
    {
        return Err(fail(
            "The selected hero is missing from this game installation.",
        ));
    }
    let current = closure(archive, &targets(receipt)?, cancel)?;
    if current != receipt.original_resources {
        return Err(fail(
            "Original character component dependencies changed or the receipt closure is incomplete. Rebuild against this installation.",
        ));
    }
    for (path, hash) in original_hashes(receipt)? {
        if current.get(&path) != Some(&hash) {
            return Err(fail(format!(
                "Character component original fingerprints disagree: {path}"
            )));
        }
    }
    // Re-run hero target ownership from actual current resources, never only saved receipt flags.
    let icon_allowed: std::collections::BTreeMap<_,_> = icons::discover(archive, &receipt.hero_path, cancel)?
        .into_iter()
        .map(|row| (casefold(&row.target),row.shared))
        .collect();
    if receipt
        .icons
        .entries
        .iter()
        .any(|e| !icon_allowed.contains_key(&casefold(&e.target)))
    {
        return Err(fail("An icon no longer belongs to the selected hero."));
    }
    for item in &receipt.icons.entries {
        if icon_allowed[&casefold(&item.target)] && !item.allow_shared {
            return Err(fail("A shared icon replacement no longer has explicit shared-resource confirmation."));
        }
        let original = read(archive, &item.target)?;
        if icons::texture_metadata(&original)? != item.original_metadata {
            return Err(fail(
                "Saved icon metadata differs from the actual original game resource.",
            ));
        }
    }
    if !receipt.animations.replacements.is_empty() {
        let allowed: BTreeSet<_> = motion::discover(archive, &receipt.hero_path, cancel, |_| {})?
            .into_iter()
            .map(|row| casefold(&row.target))
            .collect();
        if receipt
            .animations
            .replacements
            .iter()
            .any(|e| !allowed.contains(&casefold(&e.target)))
        {
            return Err(fail("An animation no longer belongs to the selected hero."));
        }
    }
    if receipt.effects.hero_path != receipt.hero_path
        || receipt.audio.hero_path != receipt.hero_path
    {
        return Err(fail("Component hero identities disagree."));
    }
    let mut fresh = receipt.clone();
    verify_outputs(archive, &mut fresh, directory, cancel, Some(&receipt.files))?;
    Ok(())
}
struct OwnedMounts<'a> {
    paths: Vec<PathBuf>,
    executor: Option<&'a dyn ToolExecutor>,
}
impl Drop for OwnedMounts<'_> {
    fn drop(&mut self) {
        // Do not delete a compiler tree until ownership establishes quiescence.
        let _barrier = if let Some(executor) = self.executor {
            match executor.quiescent() {
                Ok(guard) => Some(guard),
                Err(_) => return,
            }
        } else {
            None
        };
        for path in self.paths.iter().rev() {
            let _ = fs::remove_dir_all(path);
        }
    }
}
impl<'a> ComponentService<'a> {
    pub fn discover(&self, project: &Project) -> Result<ComponentCatalog> {
        self.discover_selected(project, true)
    }
    pub fn discover_extras(&self, project: &Project) -> Result<ComponentCatalog> {
        self.discover_selected(project, false)
    }
    fn discover_selected(&self, project: &Project, include_motion: bool) -> Result<ComponentCatalog> {
        self.clock
            .measure("Discover character components", None, |span| {
                let archive = self.archive(project)?;
                let (icons,icon_warnings) =
                    self.clock
                        .measure("Discover character images", Some(span.id()), |_| {
                            icons::discover_with_warnings(&archive, &project.original_model, self.cancel)
                        })?;
                let mut catalog = ComponentCatalog {
                    hero_path: project.original_model.clone(),
                    icons,
                    animations: vec![],
                    effects: vec![],
                    audio: vec![],
                    variants: vec![],
                    animation_unavailable: None,
                    warnings: icon_warnings,
                    includes_animations: include_motion,
                };
                if include_motion {
                match self.clock.measure(
                    "Discover character animations",
                    Some(span.id()),
                    |_| {
                        let result = motion::discover(
                            &archive,
                            &project.original_model,
                            self.cancel,
                            self.log,
                        );
                        self.cancel.check()?;
                        Ok(result)
                    },
                )? {
                    Ok(rows) => {
                        catalog.animations = rows
                            .into_iter()
                            .map(|row| AnimationDiscovery {
                                path: row.target.clone(),
                                row,
                                supported: true,
                            })
                            .collect()
                    }
                    Err(error) => {
                        self.cancel.check()?;
                        catalog.animation_unavailable = Some(error.to_string());
                        catalog.warnings.push(error.to_string());
                    }
                }
                }
                for kind in [ResourceKind::Effects, ResourceKind::Audio] {
                    match self.clock.measure(
                        format!("Discover character {}", kind.name()),
                        Some(span.id()),
                        |_| {
                            let result = resources::discover(
                                &archive,
                                &project.original_model,
                                kind,
                                self.cancel,
                            );
                            self.cancel.check()?;
                            Ok(result)
                        },
                    )? {
                        Ok(rows) => {
                            if kind == ResourceKind::Effects {
                                catalog.effects = rows;
                            } else {
                                catalog.audio = rows;
                            }
                        }
                        Err(error) => {
                            self.cancel.check()?;
                            catalog
                                .warnings
                                .push(format!("{} discovery: {error}", kind.name()));
                        }
                    }
                }
                catalog.variants =
                    self.clock
                        .measure("Discover alternate forms", Some(span.id()), |_| {
                            crate::variants::discover_variants(&archive, &project.original_model)
                        })?;
                Ok(catalog)
            })
    }
    fn archive(&self, project: &Project) -> Result<Vpk> {
        project.validate(true).map_err(fail)?;
        let path = Path::new(&self.settings.pak);
        if self.settings.pak.is_empty() || !path.is_file() {
            return Err(fail(
                "Choose the current Deadlock pak01_dir.vpk in Setup to find and verify character extras.",
            ));
        }
        let archive = Vpk::open(path).map_err(fail)?;
        if !archive
            .entries
            .contains_key(&casefold(&compiled(&project.original_model)?))
        {
            return Err(fail("The selected hero is missing from this game VPK."));
        }
        Ok(archive)
    }
    fn root(&self, project: &Project) -> Result<PathBuf> {
        project.validate(true).map_err(fail)?;
        if self.settings.workspace.is_empty() {
            return Err(fail("Select a workspace for character components."));
        }
        deadlock_vpk::check_unlinked_path(Path::new(&self.settings.workspace), true)
            .map_err(fail)?;
        let root = safety::inside(Path::new(&self.settings.workspace), &project.id)?;
        for installation in [
            &self.settings.deadlock,
            &self.settings.csdk,
            &self.settings.cswin,
        ] {
            if !installation.is_empty()
                && root.starts_with(safety::resolve_path(Path::new(installation))?)
            {
                return Err(fail(
                    "Workspace must be outside the Deadlock, CSDK and selected model-tool installations.",
                ));
            }
        }
        fs::create_dir_all(&root).map_err(fail)?;
        Ok(root)
    }
    fn folders(&self, project: &Project, label: &str) -> Result<(PathBuf, PathBuf)> {
        let root = self
            .root(project)?
            .join(format!("{label}-{}", uuid::Uuid::new_v4().simple()));
        deadlock_vpk::check_unlinked_path(&root, true).map_err(fail)?;
        let content = root.join("content");
        let game = root.join("game");
        for path in [&content, &game] {
            fs::create_dir_all(path).map_err(fail)?;
        }
        Ok((content, game))
    }
    fn stage(
        &self,
        project: &Project,
        archive: &Vpk,
        selected: &Selection,
        content: &Path,
        game: &Path,
        parent: Option<&str>,
    ) -> Result<ComponentReceipt> {
        self.clock
            .measure("Stage selected character assets", parent, |span| {
                let icons =
                    self.clock
                        .measure("Stage character images", Some(span.id()), |_| {
                            icons::stage(
                                archive,
                                &selected.icons,
                                content,
                                &project.original_model,
                                self.cancel,
                            )
                        })?;
                let animations =
                    self.clock
                        .measure("Stage character animations", Some(span.id()), |_| {
                            motion::stage(
                                archive,
                                &project.original_model,
                                &selected.animations,
                                content,
                                game,
                                self.cancel,
                                false,
                            )
                        })?;
                let effects =
                    self.clock
                        .measure("Stage character effects", Some(span.id()), |_| {
                            resources::stage(
                                archive,
                                &project.original_model,
                                &selected.effects,
                                game,
                                ResourceKind::Effects,
                                self.cancel,
                            )
                        })?;
                let audio = self
                    .clock
                    .measure("Stage character audio", Some(span.id()), |_| {
                        resources::stage(
                            archive,
                            &project.original_model,
                            &selected.audio,
                            game,
                            ResourceKind::Audio,
                            self.cancel,
                        )
                    })?;
                Ok(ComponentReceipt {
                    schema: 1,
                    hero_path: project.original_model.clone(),
                    model_enabled: project.customization.model_enabled(),
                    icons,
                    animations,
                    effects,
                    audio,
                    files: Hashes::new(),
                    original_resources: Hashes::new(),
                    prepared_content: content.into(),
                    clothing: selected.clothing.clone(),
                    native_validated: false,
                    in_game_validated: false,
                })
            })
    }
    pub fn inspect(&self, project: &Project) -> Result<ComponentInspection> {
        self.clock.measure("Analyze character components",None,|span|{let selected=selection(project)?;let mut result=ComponentInspection{model_enabled:project.customization.model_enabled(),icons:vec![],animations:vec![],effects:vec![],audio:vec![],clothing:selected.clothing.clone(),native_compile_required:false,warnings:vec![],native_validated:false,in_game_validated:false};if selected.any(){let archive=self.archive(project)?;let(content,game)=self.folders(project,"customization-analysis")?;let receipt=self.stage(project,&archive,&selected,&content,&game,Some(span.id()))?;result.native_compile_required=!receipt.animations.compile_sources.is_empty()||receipt.icons.entries.iter().any(|e|e.method=="native_vtex");result.icons=receipt.icons.entries;result.animations=receipt.animations.replacements;result.effects=receipt.effects.entries;result.audio=receipt.audio.entries;if !result.animations.is_empty(){result.warnings.push("Animation replacement changes bone poses only. Original timing, events, root motion and skeleton remain required; test movement in game.".into());}if result.native_compile_required{result.warnings.push("Selected image or animation sources need the configured native compiler. Output contracts are checked before packaging.".into());}}
 safety::write_json(&self.root(project)?.join("customization-analysis.json"),&result)?;Ok(result)})
    }
    fn mount(
        &self,
        root: &Path,
        target: &str,
        label: &str,
    ) -> Result<(OwnedMounts<'_>, PathBuf, PathBuf)> {
        let addon = format!("dmb_{}_{}", uuid::Uuid::new_v4().simple(), label);
        let content = safety::inside(root, &format!("content/{target}_addons/{addon}"))?;
        let game = safety::inside(root, &format!("game/{target}_addons/{addon}"))?;
        let mut owner = OwnedMounts {
            paths: vec![],
            executor: Some(self.runner),
        };
        for path in [&content, &game] {
            deadlock_vpk::check_unlinked_path(path, true).map_err(fail)?;
            safety::require_native_path(path)?;
            fs::create_dir_all(path.parent().unwrap()).map_err(fail)?;
            fs::create_dir(path).map_err(fail)?;
            owner.paths.push(path.clone());
        }
        Ok((owner, content, game))
    }
    fn native_icons(
        &self,
        receipt: &mut ComponentReceipt,
        content: &Path,
        game: &Path,
        parent: Option<&str>,
    ) -> Result<()> {
        self.clock
            .measure("Compile character images", parent, |span| {
                let native = receipt
                    .icons
                    .entries
                    .iter()
                    .any(|e| e.method == "native_vtex");
                if !native {
                    icons::compile_entries(
                        &mut receipt.icons,
                        content,
                        game,
                        &mut |_, _| Err(fail("Unexpected native image job")),
                        self.cancel,
                    )?;
                    return Ok(());
                }
                let root = safety::resolve_path(Path::new(&self.settings.csdk))?;
                let target_game = deadlock_tools::toolchain::sdk_game(self.settings)?;
                if self.settings.csdk.is_empty() || !root.is_dir() {
                    return Err(fail(
                        "Choose the Reduced CSDK12 folder in Setup to compile these image formats.",
                    ));
                }
                if !self.settings.deadlock.is_empty()
                    && root.starts_with(safety::resolve_path(Path::new(&self.settings.deadlock))?)
                {
                    return Err(fail(
                        "CSDK must be separate from the original Deadlock installation.",
                    ));
                }
                let (_owner, mounted_content, mounted_game) =
                    self.mount(&root, "citadel", "custom")?;
                for item in &receipt.icons.entries {
                    let mut names = vec![item.prepared_image.as_str(), item.source_vtex.as_str()];
                    if let Some(resource) = &item.prepared_resource {
                        names.push(resource);
                    }
                    for name in names.into_iter().filter(|s| !s.is_empty()) {
                        let output = safety::inside(&mounted_content, name)?;
                        safety::require_native_path(&output)?;
                        safety::atomic_copy(&safety::inside(content, name)?, &output, self.cancel)?;
                    }
                }
                icons::compile_entries(
                    &mut receipt.icons,
                    &mounted_content,
                    &mounted_game,
                    &mut |source, output| {
                        compiler::compile_resource(
                            self.runner,
                            Path::new(&self.settings.resourcecompiler),
                            source,
                            output,
                            Some(&target_game),
                            self.clock,
                            Some(span.id()),
                            self.cancel,
                        )?;
                        Ok(())
                    },
                    self.cancel,
                )?;
                let targets: BTreeSet<_> = receipt
                    .icons
                    .entries
                    .iter()
                    .map(|e| e.target.clone())
                    .collect();
                if inventory(&mounted_game, self.cancel)?
                    .keys()
                    .any(|p| !targets.contains(p))
                {
                    return Err(fail("Image compiler produced unreviewed extra resources."));
                }
                let _barrier = self.runner.quiescent()?;
                for target in targets {
                    safety::atomic_copy(
                        &safety::inside(&mounted_game, &target)?,
                        &safety::inside(game, &target)?,
                        self.cancel,
                    )?;
                }
                Ok(())
            })
    }
    fn native_animations(
        &self,
        receipt: &ComponentReceipt,
        archive: &Vpk,
        content: &Path,
        game: &Path,
        parent: Option<&str>,
    ) -> Result<()> {
        if receipt.animations.compile_sources.is_empty() {
            self.clock.skip_step(
                "Compile character animations",
                parent,
                "No animation source requires compilation",
            )?;
            return Ok(());
        }
        self.clock.measure("Compile character animations",parent,|span|{let tool=toolchain::select(self.settings,true)?;let target=tool.game.file_name().and_then(|v|v.to_str()).ok_or_else(||fail("Invalid model game name"))?;let(_owner,mounted_content,mounted_game)=self.mount(&tool.root,target,"custommotion")?;
 fn copy_sources(source:&Path,root:&Path,destination:&Path,cancel:&CancellationToken)->Result<()>{for entry in fs::read_dir(source).map_err(fail)?{cancel.check()?;let path=entry.map_err(fail)?.path();deadlock_vpk::check_unlinked_path(&path,false).map_err(fail)?;if path.is_dir(){copy_sources(&path,root,destination,cancel)?;}else if path.extension().and_then(|x|x.to_str()).is_some_and(|x|["vnmclip","dmx"].contains(&x.to_ascii_lowercase().as_str())){let relative=path.strip_prefix(root).map_err(fail)?.to_string_lossy().replace('\\',"/");let output=safety::inside(destination,&relative)?;safety::require_native_path(&output)?;safety::atomic_copy(&path,&output,cancel)?;}}Ok(())}copy_sources(content,content,&mounted_content,self.cancel)?;
 let targets:BTreeSet<_>=receipt.animations.compile_sources.iter().map(|s|format!("{s}_c")).collect();let dependencies:Hashes=receipt.original_resources.iter().filter(|(p,_)|!targets.contains(*p)).map(|(p,h)|(p.clone(),h.clone())).collect();for(path,expected)in &dependencies{self.cancel.check()?;let raw=read(archive,path)?;if sha(&raw)!=*expected{return Err(fail("Original animation dependencies changed during compilation."));}let output=safety::inside(&mounted_game,path)?;safety::require_native_path(&output)?;safety::atomic_write(&output,raw)?;}
 for source in &receipt.animations.compile_sources{compiler::compile_resource(self.runner,&tool.compiler,&safety::inside(&mounted_content,source)?,&safety::inside(&mounted_game,&format!("{source}_c"))?,Some(&tool.game),self.clock,Some(span.id()),self.cancel)?;}
 let _barrier=self.runner.quiescent()?;let after=inventory(&mounted_game,self.cancel)?;if after.iter().any(|(p,h)|!targets.contains(p)&&dependencies.get(p)!=Some(h))||dependencies.iter().any(|(p,h)|after.get(p)!=Some(h)){return Err(fail("Animation compiler changed unreviewed resources or dependencies; no replacements were packaged."));}for target in targets{safety::atomic_copy(&safety::inside(&mounted_game,&target)?,&safety::inside(game,&target)?,self.cancel)?;}Ok(())})
    }
    pub fn compile(
        &self,
        project: &Project,
        destination: &Path,
    ) -> Result<Option<ComponentReceipt>> {
        let selected = selection(project)?;
        if !selected.any() {
            self.clock.skip_step(
                "Build optional character components",
                None,
                "No character assets selected",
            )?;
            return Ok(None);
        }
        self.clock
            .measure("Build optional character components", None, |span| {
                let archive = self.archive(project)?;
                let (content, game) = self.folders(project, "customization-build")?;
                (self.log)("Checking selected character assets");
                let mut receipt = self.stage(
                    project,
                    &archive,
                    &selected,
                    &content,
                    &game,
                    Some(span.id()),
                )?;
                receipt.original_resources = self.clock.measure(
                    "Hash character asset dependencies",
                    Some(span.id()),
                    |_| closure(&archive, &targets(&receipt)?, self.cancel),
                )?;
                for (path, hash) in original_hashes(&receipt)? {
                    if receipt.original_resources.get(&path) != Some(&hash) {
                        return Err(fail(
                            "A game asset changed while importing character extras. Analyze again.",
                        ));
                    }
                }
                self.native_icons(&mut receipt, &content, &game, Some(span.id()))?;
                self.native_animations(&receipt, &archive, &content, &game, Some(span.id()))?;
                self.clock
                    .measure("Verify and stage character assets", Some(span.id()), |_| {
                        let _barrier = self.runner.quiescent()?;
                        receipt.files =
                            verify_outputs(&archive, &mut receipt, &game, self.cancel, None)?;
                        for (target, expected) in &receipt.files {
                            let output = safety::inside(destination, target)?;
                            safety::atomic_copy(
                                &safety::inside(&game, target)?,
                                &output,
                                self.cancel,
                            )?;
                            if safety::digest_with_cancel(&output, self.cancel)? != *expected {
                                return Err(fail(
                                    "Character component changed while staging for packaging.",
                                ));
                            }
                        }
                        Ok(())
                    })?;
                safety::write_json(
                    &self.root(project)?.join("customization-build.json"),
                    &receipt,
                )?;
                (self.log)(&format!(
                    "{} character asset replacements verified",
                    receipt.files.len()
                ));
                Ok(Some(receipt))
            })
    }
    pub fn verify_snapshot(
        &self,
        project: &Project,
        receipt: &ComponentReceipt,
        directory: &Path,
    ) -> Result<()> {
        self.clock
            .measure("Verify frozen character components", None, |_| {
                verify_audio_choices(project, receipt)?;
                if casefold(&compiled(&project.original_model)?)
                    != casefold(&compiled(&receipt.hero_path)?)
                {
                    return Err(fail("Component receipt is for a different hero."));
                }
                let archive = self.archive(project)?;
                verify_snapshot(&archive, receipt, directory, self.cancel)
            })
    }
    pub fn verify_install(
        &self,
        project: &Project,
        receipt: &ComponentReceipt,
        candidate: &Vpk,
        current_game: &Vpk,
    ) -> Result<()> {
        verify_audio_choices(project, receipt)?;
        self.clock.measure("Validate installed character components",None,|_|{let selected=selection(project)?;let wanted:BTreeSet<_>=selected.rows().map(|r|casefold(r.target.as_deref().unwrap_or_default())).collect();if wanted!=receipt.files.keys().map(|p|casefold(p)).collect()||casefold(&compiled(&project.original_model)?)!=casefold(&compiled(&receipt.hero_path)?)||receipt.model_enabled!=project.customization.model_enabled(){return Err(fail("Character extras changed since this VPK was built. Rebuild before installing."));}if !receipt.model_enabled&&candidate.entries.keys().cloned().collect::<BTreeSet<_>>()!=wanted{return Err(fail("An asset-only VPK must contain exactly its reviewed character extras."));}let root=self.root(project)?.join(format!("install-customization-{}",uuid::Uuid::new_v4().simple()));deadlock_vpk::check_unlinked_path(&root,true).map_err(fail)?;fs::create_dir_all(&root).map_err(fail)?;let _owner=OwnedMounts{paths:vec![root.clone()],executor:None};for(path,hash)in &receipt.files{self.cancel.check()?;let raw=read(candidate,path)?;if sha(&raw)!=*hash{return Err(fail("Character component package hash differs from its build receipt."));}safety::atomic_write(&safety::inside(&root,path)?,raw)?;}verify_snapshot(current_game,receipt,&root,self.cancel)})
    }
}
