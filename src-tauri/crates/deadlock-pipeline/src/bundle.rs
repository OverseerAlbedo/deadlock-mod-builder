//! Each alternate form completes its normal build and retains a distinct model,
//! motion and component contract. Only identical shared bytes may be merged.
use crate::{
    invalid,
    package::{self, PackageService, PublishedPackage, ResourceValidator},
    vpk_error, workspace,
};
use deadlock_core::{
    process::ToolExecutor, safety, text::casefold, CancellationToken, CoreError, Result,
    TimingRecorder,
};
use deadlock_project::Project;
use deadlock_vpk::{Hashes, SnapshotOptions, Vpk};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
pub const MAX_MEMBERS: usize = 17;
pub const MAX_FILES: usize = 100_000;
pub const MAX_RECEIPT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_ORIGINAL_RESOURCES: usize = 20_000;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub model: String,
    pub project: PathBuf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationPhase {
    Assembled,
    Install,
}
/// Concrete implementations use their normal typed BuildManifest as Contract.
/// No default validation or success flag can authorize a member.
pub trait BundleMemberBuilder {
    type Contract: Clone + Serialize + DeserializeOwned;
    fn preflight(&self, project: &Project) -> Result<()>;
    fn build(&self, project: &mut Project) -> Result<MemberArtifact<Self::Contract>>;
    fn validate_member(
        &self,
        saved: &Project,
        current: &Project,
        root: &Path,
        files: &Hashes,
        current_game: &Vpk,
        contract: &Self::Contract,
        phase: ValidationPhase,
    ) -> Result<()>;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemberArtifact<C> {
    pub package: PublishedPackage,
    pub original_model_sha256: String,
    pub rebuilt_model_sha256: Option<String>,
    pub contract: C,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemberRecord<C> {
    pub project: Project,
    pub source_project_id: String,
    pub selection: Option<Selection>,
    pub files: Hashes,
    pub original_model_sha256: String,
    pub rebuilt_model_sha256: Option<String>,
    pub contract: C,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BundleContract<C> {
    pub schema: u32,
    pub selection: Vec<Selection>,
    pub members: Vec<MemberRecord<C>>,
    pub original_resources: Hashes,
    pub in_game_validated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BundleManifest<C> {
    pub builder_version: String,
    pub project: Project,
    pub package: PublishedPackage,
    pub warnings: Vec<String>,
    pub bundle: BundleContract<C>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BundleResult<C> {
    pub package: PublishedPackage,
    pub installed: Option<PathBuf>,
    pub forms: usize,
    pub manifest_path: PathBuf,
    pub manifest: BundleManifest<C>,
}
pub struct BundleService<'a, B: BundleMemberBuilder> {
    pub settings: &'a deadlock_project::Settings,
    pub runner: &'a dyn ToolExecutor,
    pub builder: &'a B,
    pub cancel: &'a CancellationToken,
    pub clock: &'a TimingRecorder,
    pub log: &'a dyn Fn(&str),
}
fn hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn compiled(path: &str) -> Result<String> {
    deadlock_components::compiled(path)
}
pub fn active(project: &Project) -> bool {
    project.customization.active()
        && project.customization.equipment.as_ref().is_some_and(|e| {
            e.enabled.unwrap_or(false)
                && e.variants
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|r| r.enabled.unwrap_or(true))
        })
}
pub fn selection(project: &Project) -> Result<Vec<Selection>> {
    if !active(project) {
        return Ok(vec![]);
    }
    let mut output = vec![];
    for row in project
        .customization
        .equipment
        .as_ref()
        .and_then(|e| e.variants.as_deref())
        .unwrap_or_default()
        .iter()
        .filter(|r| r.enabled.unwrap_or(true))
    {
        let model = row
            .model
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                invalid("Choose an alternate model and its prepared project before bundling forms.")
            })?;
        let path = row
            .project
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                invalid("Choose an alternate model and its prepared project before bundling forms.")
            })?;
        output.push(Selection {
            model: compiled(model)?,
            project: safety::resolve_path(Path::new(path))?,
        });
    }
    Ok(output)
}
fn strip_variants(project: &Project) -> Project {
    let mut project = project.clone();
    if let Some(equipment) = &mut project.customization.equipment {
        equipment.variants = None;
    }
    project.install = false;
    project
}
fn read_project(path: &Path, cancel: &CancellationToken) -> Result<Project> {
    let bytes = crate::motion::read_resource(path, MAX_RECEIPT_BYTES, cancel)?;
    Project::load_from_bytes(path, &bytes).map_err(|e| {
        invalid(format!(
            "Could not load alternate-form project {}: {e}",
            path.display()
        ))
    })
}
fn validate_inventory(files: &Hashes) -> Result<()> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(invalid(
            "A bundled form has no bounded compiled file inventory.",
        ));
    }
    let mut seen = BTreeSet::new();
    for (path, sha) in files {
        safety::relative_asset(path)?;
        if !path.ends_with("_c")
            || sha.len() != 64
            || !sha
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
            || !seen.insert(casefold(path))
        {
            return Err(invalid(
                "Bundled file inventory has invalid fingerprints or case-colliding paths.",
            ));
        }
    }
    Ok(())
}
fn exact_archive(archive: &Vpk, files: &Hashes, cancel: &CancellationToken) -> Result<()> {
    validate_inventory(files)?;
    if archive.entries.values().any(|e| e.archive != 0x7fff) {
        return Err(invalid(
            "Alternate forms require verified single-file VPKs.",
        ));
    }
    if archive.entries.keys().cloned().collect::<BTreeSet<_>>()
        != files.keys().map(|p| casefold(p)).collect()
    {
        return Err(invalid(
            "Bundle archive differs from its exact reviewed inventory.",
        ));
    }
    let mut check = || cancel.check().map_err(|_| deadlock_vpk::Error::Cancelled);
    archive
        .verify_files(Some(files), &mut check)
        .map_err(vpk_error)?;
    if archive.version == 2 {
        archive.verify_v2_hashes(&mut check).map_err(vpk_error)?;
    }
    Ok(())
}
fn lookup_sha<'a>(files: &'a Hashes, path: &str) -> Option<&'a String> {
    let key = casefold(path);
    files
        .iter()
        .find(|(name, _)| casefold(name) == key)
        .map(|(_, sha)| sha)
}
fn current_component_selection(project: &Project) -> Result<BTreeMap<String, Vec<(String, bool)>>> {
    let mut output = BTreeMap::new();
    for (kind, rows) in [
        ("icons", &project.customization.icons),
        ("animations", &project.customization.animations),
        ("effects", &project.customization.effects),
        ("audio", &project.customization.audio),
    ] {
        let mut selected = vec![];
        if project.customization.active() {
            for row in rows
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter(|r| r.enabled.unwrap_or(true))
            {
                selected.push((
                    casefold(&safety::relative_asset(
                        row.target.as_deref().unwrap_or_default(),
                    )?),
                    row.allow_shared.unwrap_or(false),
                ));
            }
        }
        selected.sort();
        output.insert(kind.into(), selected);
    }
    Ok(output)
}
struct PrivateTree<'a>(PathBuf, &'a dyn ToolExecutor);
impl<'a> PrivateTree<'a> {
    fn new(root: &Path, prefix: &str, runner: &'a dyn ToolExecutor) -> Self {
        Self(
            root.join(format!("{prefix}-{}", uuid::Uuid::new_v4().simple())),
            runner,
        )
    }
}
impl Drop for PrivateTree<'_> {
    fn drop(&mut self) {
        if let Ok(_guard) = self.1.completed_tree_guard() {
            let _ = deadlock_vpk::remove_snapshot(&self.0);
        }
    }
}
struct PendingArchive<'a>(PathBuf, &'a dyn ToolExecutor);
impl Drop for PendingArchive<'_> {
    fn drop(&mut self) {
        let Ok(_guard) = self.1.completed_tree_guard() else {
            return;
        };
        let _ = fs::remove_file(&self.0);
        if let (Some(parent), Some(stem)) =
            (self.0.parent(), self.0.file_stem().and_then(|v| v.to_str()))
        {
            let prefix = format!("{}_", stem.strip_suffix("_dir").unwrap_or(stem));
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with(&prefix) && name.ends_with(".vpk") {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}
struct Outputs {
    root: PathBuf,
    files: Hashes,
    cases: BTreeMap<String, String>,
}
impl Outputs {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: Hashes::new(),
            cases: BTreeMap::new(),
        }
    }
    fn add(&mut self, path: &str, raw: &[u8], sha: &str) -> Result<()> {
        safety::relative_asset(path)?;
        if hash(raw) != sha {
            return Err(invalid(format!(
                "Form output changed during bundle assembly: {path}"
            )));
        }
        if let Some(previous) = self.cases.get(&casefold(path)) {
            if self.files[previous] != sha {
                return Err(invalid(format!("Alternate forms produce different replacements for the same asset: {path}. Use matching shared materials/textures/icons or separate mods.")));
            }
            return Ok(());
        }
        if self.files.len() >= MAX_FILES {
            return Err(invalid(
                "Character bundle exceeds its compiled-resource limit.",
            ));
        }
        safety::atomic_write(&safety::inside(&self.root, path)?, raw)?;
        self.files.insert(path.into(), sha.into());
        self.cases.insert(casefold(path), path.into());
        Ok(())
    }
}
fn original_resources(
    originals: &Vpk,
    models: &[String],
    output_files: &Hashes,
    read_output: impl Fn(&str) -> Result<Vec<u8>>,
    cancel: &CancellationToken,
) -> Result<Hashes> {
    let mut pending = models.to_vec();
    let cases = output_files
        .keys()
        .map(|p| casefold(p))
        .collect::<BTreeSet<_>>();
    for path in output_files.keys() {
        cancel.check()?;
        pending.extend(deadlock_vpk::resource_references(&read_output(path)?).map_err(vpk_error)?);
    }
    let mut result = Hashes::new();
    while let Some(path) = pending.pop() {
        cancel.check()?;
        let path = compiled(&path)?;
        let Some(entry) = originals.entries.get(&casefold(&path)) else {
            if !cases.contains(&casefold(&path)) {
                return Err(invalid(format!(
                    "Character bundle references missing game resource: {path}"
                )));
            }
            continue;
        };
        if result.contains_key(&entry.path) {
            continue;
        }
        if result.len() >= MAX_ORIGINAL_RESOURCES {
            return Err(invalid(
                "Bundle dependency validation exceeds 20,000 original resources.",
            ));
        }
        let raw = originals.read(&entry.path).map_err(vpk_error)?;
        result.insert(entry.path.clone(), hash(&raw));
        pending.extend(deadlock_vpk::resource_references(&raw).map_err(vpk_error)?);
    }
    Ok(result)
}
impl<B: BundleMemberBuilder> BundleService<'_, B> {
    fn load_members(
        &self,
        project: &Project,
        originals: &Vpk,
    ) -> Result<(Vec<Project>, Vec<Selection>)> {
        project
            .validate_action()
            .map_err(|e| invalid(e.to_string()))?;
        if !project.build_vpk {
            return Err(invalid("Enable Build VPK before bundling alternate forms."));
        }
        if project.customization.model_enabled() && !project.compile_model {
            return Err(invalid(
                "Enable Compile model to bundle the main character with alternate forms.",
            ));
        }
        let rows = selection(project)?;
        if rows.is_empty() || rows.len() + 1 > MAX_MEMBERS {
            return Err(invalid(
                "Choose between one and sixteen alternate-form projects.",
            ));
        }
        let mut members = vec![project.clone()];
        let mut identifiers = BTreeSet::from([project.id.clone()]);
        let mut targets = BTreeSet::from([casefold(&compiled(&project.original_model)?)]);
        let mut paths = BTreeSet::new();
        for row in &rows {
            self.cancel.check()?;
            if !paths.insert(row.project.clone()) {
                return Err(invalid(
                    "Alternate-form projects must have unique existing paths.",
                ));
            }
            let child = read_project(&row.project, self.cancel)?;
            if active(&child) {
                return Err(invalid(
                    "Alternate-form projects cannot recursively include other forms.",
                ));
            }
            if !identifiers.insert(child.id.clone()) {
                return Err(invalid(
                    "Every form requires a separate project workspace identity.",
                ));
            }
            let target = compiled(&child.original_model)?;
            if casefold(&target) != casefold(&row.model) {
                return Err(invalid(
                    "Linked project targets a different model than the selected alternate form.",
                ));
            }
            if !targets.insert(casefold(&target)) {
                return Err(invalid(
                    "Character bundle cannot contain the same model twice.",
                ));
            }
            deadlock_components::variants::validate_variant_project(project, &child, originals)?;
            if !child.compile_model || !child.build_vpk {
                return Err(invalid(
                    "Enable Compile model and Build VPK in every alternate-form project.",
                ));
            }
            members.push(child);
        }
        // Resolve every preparation handoff before any member can compile.
        for member in &members {
            self.cancel.check()?;
            self.builder.preflight(member)?;
        }
        Ok((members, rows))
    }
    pub fn build(
        &self,
        project: &Project,
        overwrite_output: bool,
        overwrite_install: bool,
    ) -> Result<BundleResult<B::Contract>> {
        self.clock
            .measure("Build alternate model bundle", None, |parent| {
                if self.settings.pak.is_empty() {
                    return Err(invalid(
                        "Select the current Deadlock VPK before bundling alternate forms.",
                    ));
                }
                let originals = Vpk::open(&self.settings.pak).map_err(vpk_error)?;
                let (members, selection) = self.clock.measure(
                    "Validate every bundle project and preparation handoff",
                    Some(parent.id()),
                    |_| self.load_members(project, &originals),
                )?;
                let root = workspace::root(self.settings, project)?;
                let output = root
                    .join("output")
                    .join(format!("pak{:02}_dir.vpk", project.vpk_number));
                deadlock_vpk::check_unlinked_path(&output, true).map_err(vpk_error)?;
                if output.exists() && !overwrite_output {
                    return Err(invalid(format!(
                        "Confirm replacement before building: {}",
                        output.display()
                    )));
                }
                fs::create_dir_all(&root)
                    .map_err(|e| CoreError::io("Create bundle workspace", e))?;
                let temporary = PrivateTree::new(&root, "bundle", self.runner);
                let mut outputs = Outputs::new(temporary.0.join("merged"));
                let snapshot_dir = temporary.0.join("snapshot");
                let pending = PendingArchive(
                    root.join(format!("pending_{}_dir.vpk", uuid::Uuid::new_v4().simple())),
                    self.runner,
                );
                let mut model_hashes = Hashes::new();
                for member in &members {
                    let target = compiled(&member.original_model)?;
                    model_hashes.insert(
                        target.clone(),
                        hash(&originals.read(&target).map_err(vpk_error)?),
                    );
                }
                let mut records = vec![];
                let mut warnings = vec![];
                for (number, source_project) in members.iter().enumerate() {
                    self.cancel.check()?;
                    let mut child = strip_variants(source_project);
                    child.id = uuid::Uuid::new_v4().simple().to_string();
                    let isolated_id = child.id.clone();
                    (self.log)(&format!(
                        "Building form {}/{}: {}",
                        number + 1,
                        members.len(),
                        if child.hero_name.is_empty() {
                            &child.original_model
                        } else {
                            &child.hero_name
                        }
                    ));
                    let artifact = self.clock.measure(
                        format!("Build form {}: {}", number + 1, child.original_model),
                        Some(parent.id()),
                        |_| self.builder.build(&mut child),
                    )?;
                    child
                        .validate_action()
                        .map_err(|e| invalid(e.to_string()))?;
                    if child.id != isolated_id
                        || active(&child)
                        || child.install
                        || compiled(&child.original_model)?
                            != compiled(&source_project.original_model)?
                    {
                        return Err(invalid(
                            "Member builder changed its isolated project authority.",
                        ));
                    }
                    validate_inventory(&artifact.package.files)?;
                    let target = compiled(&child.original_model)?;
                    if artifact.original_model_sha256 != model_hashes[&target] {
                        return Err(invalid(
                            "A form was built against a different original game model.",
                        ));
                    }
                    if child.customization.model_enabled() {
                        if artifact.rebuilt_model_sha256.is_none()
                            || artifact.rebuilt_model_sha256.as_ref()
                                != lookup_sha(&artifact.package.files, &target)
                        {
                            return Err(invalid(
                                "A form lacks matching rebuilt runtime-model authority.",
                            ));
                        }
                    } else if artifact.rebuilt_model_sha256.is_some() {
                        return Err(invalid(
                            "Assets-only form unexpectedly claims a rebuilt model.",
                        ));
                    }
                    let private = temporary.0.join(format!("member-{number}_dir.vpk"));
                    safety::atomic_copy(&artifact.package.path, &private, self.cancel)?;
                    if safety::digest_with_cancel(&private, self.cancel)? != artifact.package.sha256
                    {
                        return Err(invalid(
                            "Form archive differs from its completed build receipt.",
                        ));
                    }
                    let archive = Vpk::open(&private).map_err(vpk_error)?;
                    exact_archive(&archive, &artifact.package.files, self.cancel)?;
                    for (path, sha) in &artifact.package.files {
                        self.cancel.check()?;
                        outputs.add(path, &archive.read(path).map_err(vpk_error)?, sha)?;
                    }
                    warnings.extend(artifact.warnings);
                    records.push(MemberRecord {
                        project: child,
                        source_project_id: source_project.id.clone(),
                        selection: if number == 0 {
                            None
                        } else {
                            Some(selection[number - 1].clone())
                        },
                        files: artifact.package.files,
                        original_model_sha256: artifact.original_model_sha256,
                        rebuilt_model_sha256: artifact.rebuilt_model_sha256,
                        contract: artifact.contract,
                    });
                }
                let models = model_hashes.keys().cloned().collect::<Vec<_>>();
                let fingerprints = self.clock.measure(
                    "Fingerprint original bundle dependency closure",
                    Some(parent.id()),
                    |_| {
                        original_resources(
                            &originals,
                            &models,
                            &outputs.files,
                            |p| {
                                crate::motion::read_resource(
                                    &safety::inside(&outputs.root, p)?,
                                    deadlock_vpk::MAX_READ_BYTES,
                                    self.cancel,
                                )
                            },
                            self.cancel,
                        )
                    },
                )?;
                for (path, sha) in &model_hashes {
                    if lookup_sha(&fingerprints, path) != Some(sha) {
                        return Err(invalid("An original form changed during compilation."));
                    }
                }
                let snapshot = self.clock.measure(
                    "Freeze and verify bundle snapshot",
                    Some(parent.id()),
                    |_| {
                        let _barrier = self.runner.quiescent()?;
                        let mut check = || {
                            self.cancel
                                .check()
                                .map_err(|_| deadlock_vpk::Error::Cancelled)
                        };
                        let mut log = |line: &str| (self.log)(line);
                        deadlock_vpk::create_snapshot(
                            &outputs.root,
                            &snapshot_dir,
                            &SnapshotOptions {
                                required: outputs.files.keys().cloned().collect(),
                                ..Default::default()
                            },
                            &mut check,
                            &mut log,
                        )
                        .map_err(vpk_error)
                    },
                )?;
                if snapshot.files != outputs.files {
                    return Err(invalid(
                        "Bundle outputs changed before immutable packaging snapshot.",
                    ));
                }
                let bundle = BundleContract {
                    schema: 1,
                    selection,
                    members: records,
                    original_resources: fingerprints,
                    in_game_validated: false,
                };
                let gate = BundleGate {
                    service: self,
                    contract: &bundle,
                    files: &snapshot.files,
                    expected_sha: None,
                };
                let packer = PackageService {
                    settings: self.settings,
                    runner: self.runner,
                    cancel: self.cancel,
                    clock: self.clock,
                    validator: &gate,
                    log: self.log,
                };
                let packaging =
                    packer.pack(&snapshot_dir, &snapshot.files, &pending.0, snapshot.clone())?;
                let sha256 = safety::digest_with_cancel(&pending.0, self.cancel)?;
                let archive = Vpk::open(&pending.0).map_err(vpk_error)?;
                self.clock.measure(
                    "Validate every form in the assembled bundle",
                    Some(parent.id()),
                    |_| {
                        self.verify_contract(
                            project,
                            &archive,
                            &originals,
                            &bundle,
                            &snapshot.files,
                            ValidationPhase::Assembled,
                        )
                    },
                )?;
                let package = PublishedPackage {
                    path: output.clone(),
                    sha256,
                    files: snapshot.files.clone(),
                    packaging,
                };
                let mut unique = BTreeSet::new();
                warnings.retain(|w| unique.insert(w.clone()));
                let manifest = BundleManifest {
                    builder_version: env!("CARGO_PKG_VERSION").into(),
                    project: project.clone(),
                    package: package.clone(),
                    warnings,
                    bundle,
                };
                let manifest_path = root.join("build-manifest.json");
                // Serialization/size is checked before any published archive changes.
                let manifest_bytes =
                    serde_json::to_vec_pretty(&manifest).map_err(|e| invalid(e.to_string()))?;
                if manifest_bytes.len() as u64 > MAX_RECEIPT_BYTES {
                    return Err(invalid(
                        "Bundle manifest exceeds the supported 64 MiB limit.",
                    ));
                }
                self.clock.measure(
                    "Publish alternate bundle and manifest",
                    Some(parent.id()),
                    |_| {
                        let _barrier = self.runner.quiescent()?;
                        if safety::digest_with_cancel(&pending.0, self.cancel)? != package.sha256 {
                            return Err(invalid(
                                "Assembled bundle changed after validation; publication blocked.",
                            ));
                        }
                        package::publish_copy(
                            &pending.0,
                            &output,
                            &root.join("backups"),
                            overwrite_output,
                            self.cancel,
                        )?;
                        if safety::digest_with_cancel(&output, self.cancel)? != package.sha256 {
                            return Err(invalid(
                                "Published bundle differs from its validated archive.",
                            ));
                        }
                        safety::atomic_write(&manifest_path, &manifest_bytes)?;
                        safety::write_json(&root.join("packaging-snapshot.json"), &snapshot)?;
                        Ok(())
                    },
                )?;
                let installed = if project.install {
                    let gate = BundleGate {
                        service: self,
                        contract: &manifest.bundle,
                        files: &manifest.package.files,
                        expected_sha: Some(&manifest.package.sha256),
                    };
                    let packer = PackageService {
                        settings: self.settings,
                        runner: self.runner,
                        cancel: self.cancel,
                        clock: self.clock,
                        validator: &gate,
                        log: self.log,
                    };
                    Some(packer.install(project, &output, overwrite_install)?)
                } else {
                    self.clock
                        .skip_step("Install", Some(parent.id()), "Installation disabled")?;
                    None
                };
                self.clock.measure(
                    "Clean bundle assembly and snapshot",
                    Some(parent.id()),
                    |_| {
                        let _guard = self.runner.completed_tree_guard()?;
                        deadlock_vpk::remove_snapshot(&temporary.0).map_err(vpk_error)
                    },
                )?;
                (self.log)(&format!(
                    "Character bundle created with {} independently validated forms",
                    members.len()
                ));
                Ok(BundleResult {
                    package,
                    installed,
                    forms: members.len(),
                    manifest_path,
                    manifest,
                })
            })
    }
    pub fn verify_install(
        &self,
        project: &Project,
        archive: &Vpk,
        current_game: &Vpk,
        manifest: &BundleManifest<B::Contract>,
        outer_sha: &str,
    ) -> Result<()> {
        self.clock.measure("Validate alternate model bundle installation",None,|_|{
            if manifest.package.sha256!=outer_sha||manifest.project.id!=project.id||casefold(&compiled(&manifest.project.original_model)?)!=casefold(&compiled(&project.original_model)?){return Err(invalid("Archive does not match this selected character bundle. Rebuild before installing."));}
            self.verify_contract(project,archive,current_game,&manifest.bundle,&manifest.package.files,ValidationPhase::Install)
        })
    }
    pub fn read_manifest(&self, path: &Path) -> Result<BundleManifest<B::Contract>> {
        let raw = crate::motion::read_resource(path, MAX_RECEIPT_BYTES, self.cancel)?;
        serde_json::from_slice(&raw).map_err(|e| {
            invalid(format!(
                "Could not read complete bounded character bundle receipt: {e}"
            ))
        })
    }
    fn verify_contract(
        &self,
        project: &Project,
        archive: &Vpk,
        originals: &Vpk,
        bundle: &BundleContract<B::Contract>,
        files: &Hashes,
        phase: ValidationPhase,
    ) -> Result<()> {
        project
            .validate_action()
            .map_err(|e| invalid(e.to_string()))?;
        let selected = selection(project)?;
        if bundle.schema != 1
            || bundle.selection != selected
            || selected.is_empty()
            || bundle.members.len() != selected.len() + 1
            || bundle.members.len() > MAX_MEMBERS
        {
            return Err(invalid(
                "Bundle member contract does not match the selected forms.",
            ));
        }
        exact_archive(archive, files, self.cancel)?;
        validate_inventory(&bundle.original_resources)?;
        if bundle.original_resources.len() > MAX_ORIGINAL_RESOURCES {
            return Err(invalid(
                "Bundle original-resource contract exceeds 20,000 resources.",
            ));
        }
        let models = bundle
            .members
            .iter()
            .map(|r| compiled(&r.project.original_model))
            .collect::<Result<Vec<_>>>()?;
        let actual_originals = self.clock.measure(
            "Recheck current-game bundle dependency closure",
            None,
            |_| {
                original_resources(
                    originals,
                    &models,
                    files,
                    |p| archive.read(p).map_err(vpk_error),
                    self.cancel,
                )
            },
        )?;
        if actual_originals != bundle.original_resources {
            return Err(invalid("A game asset used by this bundle changed or its dependency contract is incomplete. Rebuild against this installation."));
        }
        let file_cases = files
            .keys()
            .map(|p| (casefold(p), p.clone()))
            .collect::<BTreeMap<_, _>>();
        let temporary = PrivateTree::new(
            &workspace::root(self.settings, project)?,
            "bundle-check",
            self.runner,
        );
        let mut union = Hashes::new();
        let mut targets = BTreeSet::new();
        let mut identifiers = BTreeSet::new();
        let mut source_identifiers = BTreeSet::new();
        for (number, record) in bundle.members.iter().enumerate() {
            self.cancel.check()?;
            let child = &record.project;
            child
                .validate_action()
                .map_err(|e| invalid(e.to_string()))?;
            if active(child)
                || child.install
                || !identifiers.insert(child.id.clone())
                || !source_identifiers.insert(record.source_project_id.clone())
            {
                return Err(invalid(
                    "Bundled forms require distinct nonrecursive build/source identities.",
                ));
            }
            let target = compiled(&child.original_model)?;
            if !targets.insert(casefold(&target))
                || !originals.entries.contains_key(&casefold(&target))
            {
                return Err(invalid(
                    "Bundle contains duplicated or missing model targets.",
                ));
            }
            let current = if number == 0 {
                if casefold(&target) != casefold(&compiled(&project.original_model)?)
                    || record.selection.is_some()
                    || record.source_project_id != project.id
                {
                    return Err(invalid("Bundle primary form does not match this project."));
                }
                project.clone()
            } else {
                let row = &selected[number - 1];
                if record.selection.as_ref() != Some(row)
                    || casefold(&target) != casefold(&row.model)
                {
                    return Err(invalid("Alternate forms differ from selected projects."));
                }
                deadlock_components::variants::validate_variant_project(project, child, originals)?;
                let current = read_project(&row.project, self.cancel)?;
                if current.id != record.source_project_id
                    || active(&current)
                    || casefold(&compiled(&current.original_model)?) != casefold(&target)
                {
                    return Err(invalid(
                        "Linked alternate project changed identity or model. Rebuild this bundle.",
                    ));
                }
                current
            };
            if current.customization.model_enabled() != child.customization.model_enabled()
                || current_component_selection(&current)? != current_component_selection(child)?
            {
                return Err(invalid("A form's component selection changed after this bundle was built. Rebuild before installing."));
            }
            validate_inventory(&record.files)?;
            for (path, sha) in &record.files {
                let canonical = file_cases.get(&casefold(path)).ok_or_else(|| {
                    invalid("A bundled form resource is absent from assembled output.")
                })?;
                if files[canonical] != *sha {
                    return Err(invalid(format!(
                        "Bundled form bytes differ from validated output: {path}"
                    )));
                }
                union.insert(canonical.clone(), sha.clone());
            }
            if lookup_sha(&bundle.original_resources, &target)
                != Some(&record.original_model_sha256)
            {
                return Err(invalid(
                    "Form original-model identity differs from the current bundle authority.",
                ));
            }
            if child.customization.model_enabled() {
                if record.rebuilt_model_sha256.is_none()
                    || record.rebuilt_model_sha256.as_ref() != lookup_sha(&record.files, &target)
                {
                    return Err(invalid(
                        "Bundled form lacks its independently validated replacement model.",
                    ));
                }
            } else if record.rebuilt_model_sha256.is_some() {
                return Err(invalid(
                    "Assets-only form unexpectedly claims a rebuilt model.",
                ));
            }
            let member_source = temporary.0.join(number.to_string()).join("source");
            let member_frozen = temporary.0.join(number.to_string()).join("frozen");
            self.clock.measure(
                format!("Extract and freeze assembled form {}", number + 1),
                None,
                |_| {
                    for (path, sha) in &record.files {
                        self.cancel.check()?;
                        let raw = archive.read(path).map_err(vpk_error)?;
                        if hash(&raw) != *sha {
                            return Err(invalid(
                                "Assembled form bytes changed during verification.",
                            ));
                        }
                        safety::atomic_write(&safety::inside(&member_source, path)?, &raw)?;
                    }
                    let mut check = || {
                        self.cancel
                            .check()
                            .map_err(|_| deadlock_vpk::Error::Cancelled)
                    };
                    let mut log = |line: &str| (self.log)(line);
                    let snapshot = deadlock_vpk::create_snapshot(
                        &member_source,
                        &member_frozen,
                        &SnapshotOptions {
                            required: record.files.keys().cloned().collect(),
                            ..Default::default()
                        },
                        &mut check,
                        &mut log,
                    )
                    .map_err(vpk_error)?;
                    if snapshot.files != record.files {
                        return Err(invalid(
                            "Extracted form differs from its immutable receipt.",
                        ));
                    }
                    Ok(())
                },
            )?;
            self.clock.measure(
                format!(
                    "Validate assembled form {} model, motion and components",
                    number + 1
                ),
                None,
                |_| {
                    self.builder.validate_member(
                        child,
                        &current,
                        &member_frozen,
                        &record.files,
                        originals,
                        &record.contract,
                        phase,
                    )
                },
            )?;
            // Verify callbacks cannot modify the bytes subsequently authorized.
            for (path, sha) in &record.files {
                self.cancel.check()?;
                if safety::digest_with_cancel(&safety::inside(&member_frozen, path)?, self.cancel)?
                    != *sha
                {
                    return Err(invalid("Form validator modified frozen member resources."));
                }
            }
        }
        if union != *files {
            return Err(invalid(
                "Bundle contains files outside its independently validated forms.",
            ));
        }
        self.clock
            .measure("Clean assembled-form validation snapshots", None, |_| {
                let _guard = self.runner.completed_tree_guard()?;
                deadlock_vpk::remove_snapshot(&temporary.0).map_err(vpk_error)
            })?;
        Ok(())
    }
}
struct BundleGate<'a, 'b, B: BundleMemberBuilder> {
    service: &'a BundleService<'b, B>,
    contract: &'a BundleContract<B::Contract>,
    files: &'a Hashes,
    expected_sha: Option<&'a str>,
}
impl<B: BundleMemberBuilder> ResourceValidator for BundleGate<'_, '_, B> {
    fn validate_frozen(
        &self,
        _: &Project,
        _: &Path,
        files: &Hashes,
        _: &CancellationToken,
        _: &TimingRecorder,
    ) -> Result<()> {
        if files != self.files {
            return Err(invalid("Bundle frozen inventory changed."));
        }
        // Bundle publication always validates the actual assembled archive.
        Err(invalid(
            "Bundle validation requires its assembled archive boundary.",
        ))
    }
    fn validate_install(
        &self,
        project: &Project,
        candidate: &Vpk,
        current_game: &Vpk,
        archive_sha: &str,
        _: &CancellationToken,
        _: &TimingRecorder,
    ) -> Result<()> {
        if self.expected_sha != Some(archive_sha) {
            return Err(invalid(
                "Assembled bundle hash changed before installation.",
            ));
        }
        self.service.verify_contract(
            project,
            candidate,
            current_game,
            self.contract,
            self.files,
            ValidationPhase::Install,
        )
    }
}
