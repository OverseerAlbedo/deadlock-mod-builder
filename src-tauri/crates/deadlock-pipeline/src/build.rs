//! Build and install transactions backed by fresh resource validation.
use crate::{
    compilation::CompilerService,
    inspect::{InspectionReport, ModelPipeline},
    invalid,
    motion::{Restoration, TestedContract, ValidatedMotion},
    package::{PackageService, PublishedPackage, ResourceValidator},
    workspace::WorkspaceLease,
};
use deadlock_core::{safety, CancellationToken, CoreError, Result, TimingRecorder};
use deadlock_project::{Ag2Mode, Project};
use deadlock_vpk::{Hashes, Vpk};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub prepared: Option<PathBuf>,
    pub compiled: bool,
    pub vpk: Option<PathBuf>,
    pub installed: Option<PathBuf>,
    pub manifest: Option<PathBuf>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildManifest {
    pub schema: u32,
    pub builder_version: String,
    pub project: Project,
    pub package: PublishedPackage,
    pub original_model_sha256: Option<String>,
    pub runtime_motion: Option<ValidatedMotion>,
    pub motion_restoration: Option<Restoration>,
    pub tested_contract: Option<TestedContract>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub generated_contract: Option<crate::secondary_motion::NativeContract>,
    pub source_binding: Option<deadlock_skeleton::extensions::SourceBinding>,
    pub customization: Option<deadlock_components::service::ComponentReceipt>,
    pub warnings: Vec<String>,
}
pub struct BuildService<'a> {
    pub model: ModelPipeline<'a>,
}
impl BuildService<'_> {
    pub(crate) fn components(&self) -> deadlock_components::service::ComponentService<'_> {
        deadlock_components::service::ComponentService {
            settings: self.model.settings,
            runner: self.model.runner,
            cancel: self.model.cancel,
            clock: self.model.clock,
            log: self.model.log,
        }
    }
    pub fn build(
        &self,
        p: &mut Project,
        overwrite_output: bool,
        overwrite_install: bool,
    ) -> Result<BuildResult> {
        let root = self.model.root(p)?;
        let lease = WorkspaceLease::acquire_for_runner(&root, self.model.runner)?;
        let result = self.build_owned(p, overwrite_output, overwrite_install);
        let cleanup = self
            .model
            .clock
            .measure("Release build workspace", None, |_| lease.release());
        if let Err(error) = &cleanup {
            (self.model.log)(&error.to_string());
        }
        match (result, cleanup) {
            (Err(e), _) => Err(e),
            (Ok(_), Err(e)) => Err(e),
            (Ok(v), Ok(_)) => Ok(v),
        }
    }
    fn build_owned(
        &self,
        p: &mut Project,
        overwrite_output: bool,
        overwrite_install: bool,
    ) -> Result<BuildResult> {
        let root = self.model.root(p)?;
        let clock = self.model.clock;
        let cancel = self.model.cancel;
        p.validate_action().map_err(|e| invalid(e.to_string()))?;
        if crate::bundle::active(p) {
            return self.build_bundle_owned(p, overwrite_output, overwrite_install);
        }
        deadlock_preparation::service::check_handoff(p).map_err(|error| match error {
            deadlock_preparation::Error::Core(error) => error,
            other => invalid(other.to_string()),
        })?;
        if p.build_vpk && !p.compile_model && p.customization.model_enabled() {
            return Err(invalid("Enable Compile model when Build VPK is selected; stale compiled files are never packaged."));
        }
        let output = root
            .join("output")
            .join(format!("pak{:02}_dir.vpk", p.vpk_number));
        if p.build_vpk && output.exists() && !overwrite_output {
            return Err(invalid(format!(
                "Confirm replacement of the existing output before building: {}",
                output.display()
            )));
        }
        let components = self.components();
        let mut report = None;
        let mut restoration = None;
        let mut motion = None;
        let mut support = Hashes::new();
        let mut required = BTreeSet::new();
        let (content, game, compiled) = if p.customization.model_enabled() {
            let mut inspection = self.model.inspect(p)?;
            if p.compile_model && !inspection.vertex_preflight.errors.is_empty() {
                return Err(invalid(format!(
                    "Model compilation stopped by vertex preflight:\n{}",
                    inspection.vertex_preflight.errors.join("\n")
                )));
            }
            if inspection.needs_review {
                return Err(invalid(format!("Review mesh mappings, materials, LODs and unskinned meshes before building. Inspection: {}",root.join("inspection.json").display())));
            }
            let cswin = p.ag2_mode == Ag2Mode::Cswin
                || (p.ag2_mode == Ag2Mode::Auto && !inspection.model.ag2.is_empty());
            if cswin && p.compile_model {
                deadlock_tools::toolchain::select(self.model.settings, true)?;
            }
            let prepared = self.model.prepare(p, &inspection, false)?;
            let secondary_sources = if let Some(setup)=&inspection.secondary_motion {
                let model=safety::inside(&prepared.content,p.source_model())?;
                let proxy=safety::inside(&prepared.content,&format!("{}/dmx/dmb_secondary_proxy.dmx",p.hero_root()))?;
                let proxy_sha=safety::digest_with_cancel(&proxy,cancel)?;
                if proxy_sha!=setup.proxy_sha256{return Err(invalid("Generated proxy changed before compilation."));}
                Some((model.clone(),safety::digest_with_cancel(&model,cancel)?,proxy,proxy_sha))
            } else {None};
            if !p.compile_model {
                clock.skip_step("Compile model", None, "Compile model is disabled")?;
                clock.skip_step("Package VPK", None, "Compilation is disabled")?;
                clock.skip_step("Install", None, "No compiled package")?;
                return Ok(BuildResult {
                    prepared: Some(prepared.content),
                    compiled: false,
                    vpk: None,
                    installed: None,
                    manifest: None,
                });
            }
            support =
                self.model
                    .sources()
                    .support_resources(p, &prepared.content, &prepared.game)?;
            let mut targets = prepared
                .generated_materials
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            for material in inspection.materials.keys() {
                if safety::inside(&prepared.content, material)?.is_file() {
                    targets.insert(material.clone());
                }
            }
            let compiler = CompilerService {
                settings: self.model.settings,
                runner: self.model.runner,
                cancel,
                clock,
                log: self.model.log,
            };
            clock.measure("Build materials", None, |span| {
                for material in &targets {
                    cancel.check()?;
                    compiler.compile_material(
                        p,
                        material,
                        &prepared.content,
                        &prepared.game,
                        Some(span.id()),
                    )?;
                }
                Ok(())
            })?;
            let output = safety::inside(&prepared.game, &format!("{}_c", p.source_model()))?;
            if p.ag2_mode == Ag2Mode::Adapter {
                compiler.adapter(p, &prepared.content, &prepared.game)?;
            } else if cswin {
                self.model
                    .compile_cswin(p, &prepared.content, &prepared.game)?;
            } else {
                let target_game = deadlock_tools::toolchain::sdk_game(self.model.settings)?;
                deadlock_tools::compiler::compile_resource(
                    self.model.runner,
                    Path::new(&self.model.settings.resourcecompiler),
                    &safety::inside(&prepared.content, p.source_model())?,
                    &output,
                    Some(&target_game),
                    clock,
                    None,
                    cancel,
                )?;
            }
            if let Some((model,model_sha,proxy,proxy_sha))=&secondary_sources {
                if safety::digest_with_cancel(model,cancel)?!=*model_sha||safety::digest_with_cancel(proxy,cancel)?!=*proxy_sha{return Err(invalid("Secondary source changed during native compilation. Rebuild without editing its managed sources."));}
            }
            if inspection.original.compiled_available || p.build_vpk {
                let original = inspection.original.compiled_bytes(cancel)?;
                let source = inspection.source_binding();
                let directory = root.join("runtime-motion");
                let (restored, _) = if inspection.secondary_motion.is_some() {
                    let (contract, restored, validated)=self.model.restore_secondary(p,&inspection,&output)?;
                    inspection.generated_contract=Some(contract);
                    (restored,validated)
                } else { self.model.motion(&directory).restore(
                    p,
                    &original,
                    &output,
                    inspection.original.tested_physics.as_ref(),
                    source.as_ref(),
                )? };
                restoration = Some(restored);
                let verified = self.model.validate_compiled(p, &output, &inspection)?;
                motion = Some(verified.runtime_motion);
            } else {
                self.model.verify_ag2(&inspection.original.ag2, &output)?;
                clock.skip_step(
                    "Restore and validate runtime motion",
                    None,
                    "Source-only diagnostic compile; original compiled authority unavailable",
                )?;
                (self.model.log)("Diagnostic compilation completed. Runtime motion is unverified and this output cannot be packaged or installed.");
            }
            required.insert(format!("{}_c", p.source_model()));
            required.extend(targets.into_iter().map(|m| format!("{m}_c")));
            report = Some(inspection);
            (prepared.content, prepared.game, true)
        } else {
            let content = root.join("components-content");
            let game = root.join("components-game");
            crate::compilation::clear_owned(p, &[content.clone(), game.clone()], cancel)?;
            clock.skip_step("Compile model", None, "Assets-only project")?;
            (content, game, false)
        };
        let extras = if p.customization.active() {
            components.compile(p, &game)?
        } else {
            clock.skip_step("Build optional components", None, "No components selected")?;
            None
        };
        if let Some(extra) = &extras {
            required.extend(extra.files.keys().cloned());
        }
        if !p.build_vpk {
            clock.skip_step("Package VPK", None, "VPK output is disabled")?;
            clock.skip_step("Install", None, "No VPK requested")?;
            return Ok(BuildResult {
                prepared: Some(content),
                compiled,
                vpk: None,
                installed: None,
                manifest: None,
            });
        }
        let gate = BuildGate {
            service: self,
            report: report.as_ref(),
            extras: extras.as_ref(),
            expected_package: None,
        };
        let package = PackageService {
            settings: self.model.settings,
            runner: self.model.runner,
            cancel,
            clock,
            validator: &gate,
            log: self.model.log,
        }
        .package(p, &game, support, required, overwrite_output)?;
        let manifest = BuildManifest {
            schema: 1,
            builder_version: env!("CARGO_PKG_VERSION").into(),
            project: p.clone(),
            package,
            original_model_sha256: report
                .as_ref()
                .and_then(|r| r.original.compiled_sha256.clone()),
            runtime_motion: motion,
            motion_restoration: restoration,
            tested_contract: report
                .as_ref()
                .and_then(|r| r.original.tested_physics.clone()),
            generated_contract: report.as_ref().and_then(|r|r.generated_contract.clone()),
            source_binding: report.as_ref().and_then(|r| r.source_binding()),
            customization: extras,
            warnings: report
                .as_ref()
                .map(|r| r.warnings.clone())
                .unwrap_or_default(),
        };
        let manifest_path = root.join("build-manifest.json");
        clock.measure("Save verified build manifest", None, |_| {
            safety::write_json(&manifest_path, &manifest)
        })?;
        let installed = if p.install {
            let gate = BuildGate {
                service: self,
                report: report.as_ref(),
                extras: manifest.customization.as_ref(),
                expected_package: Some(&manifest.package),
            };
            Some(
                PackageService {
                    settings: self.model.settings,
                    runner: self.model.runner,
                    cancel,
                    clock,
                    validator: &gate,
                    log: self.model.log,
                }
                .install(p, &manifest.package.path, overwrite_install)?,
            )
        } else {
            clock.skip_step("Install", None, "Installation is disabled")?;
            None
        };
        Ok(BuildResult {
            prepared: Some(content),
            compiled,
            vpk: Some(manifest.package.path),
            installed,
            manifest: Some(manifest_path),
        })
    }
    pub fn read_manifest(&self, p: &Project) -> Result<BuildManifest> {
        let raw = crate::motion::read_resource(
            &self.model.root(p)?.join("build-manifest.json"),
            64 * 1024 * 1024,
            self.model.cancel,
        )?;
        let manifest: BuildManifest = serde_json::from_slice(&raw)?;
        if manifest.schema != 1
            || manifest.project.id != p.id
            || manifest.project.original_model != p.original_model
            || manifest.project.customization != p.customization
        {
            return Err(invalid("This package's saved project or component selection differs from the current project. Rebuild first."));
        }
        validate_component_receipt(p, manifest.customization.as_ref(), &manifest.package.files)?;
        Ok(manifest)
    }
    pub fn install(&self, p: &mut Project, source: &Path, overwrite: bool) -> Result<PathBuf> {
        let root = self.model.root(p)?;
        let _lease = WorkspaceLease::acquire_for_runner(&root, self.model.runner)?;
        if crate::bundle::active(p) {
            return self.install_bundle_owned(p, source, overwrite);
        }
        let manifest = self.read_manifest(p)?;
        let mut settings = self.model.settings.clone();
        settings.pak = Path::new(&settings.deadlock)
            .join("game/citadel/pak01_dir.vpk")
            .to_string_lossy()
            .into_owned();
        let current = BuildService {
            model: ModelPipeline {
                settings: &settings,
                runner: self.model.runner,
                cancel: self.model.cancel,
                clock: self.model.clock,
                log: self.model.log,
                reader_root: self.model.reader_root,
            },
        };
        let mut report = if p.customization.model_enabled() {
            Some(current.model.inspect(p)?)
        } else {
            None
        };
        if let Some(report) = &mut report {
            report.generated_contract=manifest.generated_contract.clone();
            if manifest.original_model_sha256 != report.original.compiled_sha256 {
                return Err(invalid("This VPK was built against a different original game model. Rebuild against the current game before installing."));
            }
        }
        let gate = BuildGate {
            service: &current,
            report: report.as_ref(),
            extras: manifest.customization.as_ref(),
            expected_package: Some(&manifest.package),
        };
        PackageService {
            settings: &settings,
            runner: self.model.runner,
            cancel: self.model.cancel,
            clock: self.model.clock,
            validator: &gate,
            log: self.model.log,
        }
        .install(p, source, overwrite)
    }
}
pub(crate) struct BuildGate<'a, 'b> {
    pub service: &'a BuildService<'b>,
    pub report: Option<&'a InspectionReport>,
    pub extras: Option<&'a deadlock_components::service::ComponentReceipt>,
    pub expected_package: Option<&'a PublishedPackage>,
}
impl ResourceValidator for BuildGate<'_, '_> {
    fn validate_frozen(
        &self,
        p: &Project,
        root: &Path,
        files: &Hashes,
        cancel: &CancellationToken,
        _clock: &TimingRecorder,
    ) -> Result<()> {
        cancel.check()?;
        validate_component_receipt(p, self.extras, files)?;
        if let Some(report) = self.report {
            let path = format!("{}_c", p.source_model());
            let expected = files
                .get(&path)
                .ok_or_else(|| invalid("Replacement model is absent from frozen package."))?;
            let validated =
                self.service
                    .model
                    .validate_compiled(p, &safety::inside(root, &path)?, report)?;
            if &validated.compiled_sha256 != expected {
                return Err(invalid(
                    "Frozen model differs from validated package bytes.",
                ));
            }
        } else if p.customization.model_enabled() {
            return Err(invalid(
                "Model package requires a fresh original and source contract.",
            ));
        }
        if let Some(extras) = self.extras {
            self.service.components().verify_snapshot(p, extras, root)?;
        }
        for (path, hash) in files {
            if safety::digest_with_cancel(&safety::inside(root, path)?, cancel)? != *hash {
                return Err(invalid("Frozen package changed during validation."));
            }
        }
        Ok(())
    }
    fn validate_install(
        &self,
        p: &Project,
        candidate: &Vpk,
        current_game: &Vpk,
        sha: &str,
        cancel: &CancellationToken,
        clock: &TimingRecorder,
    ) -> Result<()> {
        let expected = self.expected_package.ok_or_else(|| {
            invalid("Installation requires a verified build manifest. Rebuild this project first.")
        })?;
        validate_component_receipt(p, self.extras, &expected.files)?;
        if expected.sha256 != sha {
            return Err(invalid(
                "Selected archive differs from the verified build manifest. Rebuild first.",
            ));
        }
        let mut check = || cancel.check().map_err(|_| deadlock_vpk::Error::Cancelled);
        candidate
            .verify_files(Some(&expected.files), &mut check)
            .map_err(crate::vpk_error)?;
        let directory = tempfile::Builder::new()
            .prefix("install-contract-")
            .tempdir_in(self.service.model.root(p)?)
            .map_err(|e| CoreError::io("Create installation validation snapshot", e))?;
        for path in expected.files.keys() {
            cancel.check()?;
            safety::atomic_write(
                &safety::inside(directory.path(), path)?,
                candidate.read(path).map_err(crate::vpk_error)?,
            )?;
        }
        if let Some(report) = self.report {
            let original = current_game
                .read(&format!("{}_c", p.source_model()))
                .map_err(crate::vpk_error)?;
            if report.original.compiled_sha256.as_deref()
                != Some(crate::inspect::hash(&original).as_str())
            {
                return Err(invalid("Current game model changed since this build's authority was inspected. Rebuild against the current game."));
            }
        }
        self.validate_frozen(p, directory.path(), &expected.files, cancel, clock)?;
        if let Some(extras) = self.extras {
            self.service
                .components()
                .verify_install(p, extras, candidate, current_game)?;
        }
        crate::package::validate_dependencies(
            directory.path(),
            &expected.files,
            Some(current_game),
            cancel,
        )
    }
}

/// A receipt is mandatory authority, not an optional UI detail. Bind it to the
/// live project before any model reader or component validator is invoked.
fn validate_component_receipt(
    project: &Project,
    receipt: Option<&deadlock_components::service::ComponentReceipt>,
    files: &Hashes,
) -> Result<()> {
    use deadlock_core::text::casefold;
    let choices = &project.customization;
    let mut selected = BTreeSet::new();
    if choices.active() {
        for rows in [
            &choices.icons,
            &choices.animations,
            &choices.effects,
            &choices.audio,
        ] {
            for row in rows
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter(|r| r.enabled.unwrap_or(true))
            {
                let target = row
                    .target
                    .as_deref()
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| {
                        invalid("Selected component is missing its reviewed target. Rebuild first.")
                    })?;
                safety::relative_asset(target)?;
                if !selected.insert(casefold(target)) {
                    return Err(invalid("Selected component targets are duplicated."));
                }
            }
        }
    }
    let package_files = files
        .keys()
        .map(|path| casefold(path))
        .collect::<BTreeSet<_>>();
    if package_files.len() != files.len() {
        return Err(invalid(
            "Package files have duplicate or case-colliding targets.",
        ));
    }
    let component_path = |path: &str| {
        path.ends_with(".vnmclip_c")
            || path.ends_with(".vpcf_c")
            || path.ends_with(".vsnd_c")
            || (path.starts_with("panorama/images/") && path.ends_with(".vtex_c"))
    };
    let packaged_components = package_files
        .iter()
        .filter(|p| component_path(p))
        .cloned()
        .collect::<BTreeSet<_>>();
    if packaged_components != selected {
        return Err(invalid("Packaged character components differ from the current project's exact selected targets. Rebuild first."));
    }
    if !choices.model_enabled() && (selected.is_empty() || package_files != selected) {
        return Err(invalid(
            "An asset-only VPK must contain exactly its reviewed character extras.",
        ));
    }
    match receipt {
        None if !selected.is_empty() || !choices.model_enabled() => Err(invalid(
            "Selected character components require their verified build receipt. Rebuild first.",
        )),
        None => Ok(()),
        Some(receipt) => {
            let targets = receipt
                .files
                .keys()
                .map(|path| casefold(path))
                .collect::<BTreeSet<_>>();
            if receipt.schema != 1
                || targets.len() != receipt.files.len()
                || targets != selected
                || receipt.model_enabled != choices.model_enabled()
                || casefold(receipt.hero_path.trim_end_matches("_c"))
                    != casefold(project.source_model())
            {
                return Err(invalid("Character component receipt differs from the current project's exact selected targets. Rebuild first."));
            }
            for (path, expected) in &receipt.files {
                let actual = files
                    .iter()
                    .find(|(target, _)| casefold(target) == casefold(path))
                    .map(|(_, hash)| hash);
                if actual != Some(expected) {
                    return Err(invalid(
                        "Character component package hashes differ from the verified receipt.",
                    ));
                }
            }
            Ok(())
        }
    }
}
