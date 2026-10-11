//! Concrete bundle members use the same complete BuildService and fresh compiled
//! validation as standalone projects; no alternate-form shortcut can publish.
use crate::{
    build::{BuildManifest, BuildResult, BuildService},
    bundle::{self, BundleMemberBuilder, BundleService, MemberArtifact, ValidationPhase},
    inspect::ModelPipeline,
    invalid,
    package::{PackageService, ResourceValidator},
    vpk_error,
};
use deadlock_core::{safety, CancellationToken, Result, TimingRecorder};
use deadlock_project::Project;
use deadlock_vpk::{Hashes, Vpk};
use std::path::{Path, PathBuf};
impl BundleMemberBuilder for BuildService<'_> {
    type Contract = BuildManifest;
    fn preflight(&self, project: &Project) -> Result<()> {
        project
            .validate_action()
            .map_err(|e| invalid(e.to_string()))?;
        if project.customization.model_enabled() {
            deadlock_preparation::service::check_handoff(project).map_err(|error| match error {
                deadlock_preparation::Error::Core(error) => error,
                other => invalid(other.to_string()),
            })?;
        }
        Ok(())
    }
    fn build(&self, project: &mut Project) -> Result<MemberArtifact<BuildManifest>> {
        let result = BuildService::build(self, project, false, false)?;
        let path = result.vpk.ok_or_else(|| {
            invalid("A form did not finish its normal VPK build; bundle was not published.")
        })?;
        let manifest = self.read_manifest(project)?;
        if path != manifest.package.path {
            return Err(invalid(
                "Form build returned a different archive than its manifest.",
            ));
        }
        let original_model_sha256 = match &manifest.original_model_sha256 {
            Some(hash) => hash.clone(),
            None => {
                if project.customization.model_enabled() {
                    return Err(invalid("Form lacks authoritative original model evidence."));
                }
                let original = Vpk::open(&self.model.settings.pak).map_err(vpk_error)?;
                crate::inspect::hash(
                    &original
                        .read(&deadlock_components::compiled(&project.original_model)?)
                        .map_err(vpk_error)?,
                )
            }
        };
        let rebuilt_model_sha256 = manifest
            .runtime_motion
            .as_ref()
            .map(|m| m.resource_sha256().to_owned());
        Ok(MemberArtifact {
            package: manifest.package.clone(),
            original_model_sha256,
            rebuilt_model_sha256,
            warnings: manifest.warnings.clone(),
            contract: manifest,
        })
    }
    fn validate_member(
        &self,
        saved: &Project,
        _current: &Project,
        root: &Path,
        files: &Hashes,
        current_game: &Vpk,
        contract: &BuildManifest,
        _phase: ValidationPhase,
    ) -> Result<()> {
        self.model.cancel.check()?;
        if contract.schema != 1 || contract.project != *saved || contract.package.files != *files {
            return Err(invalid(
                "Bundled form manifest does not match its independently built project and files.",
            ));
        }
        let mut settings = self.model.settings.clone();
        settings.pak = current_game.path.to_string_lossy().into_owned();
        let service = BuildService {
            model: ModelPipeline {
                settings: &settings,
                runner: self.model.runner,
                cancel: self.model.cancel,
                clock: self.model.clock,
                log: self.model.log,
                reader_root: self.model.reader_root,
            },
        };
        if saved.customization.model_enabled() {
            let target = deadlock_components::compiled(&saved.original_model)?;
            let current_original = current_game.read(&target).map_err(vpk_error)?;
            if contract.original_model_sha256.as_deref()
                != Some(crate::inspect::hash(&current_original).as_str())
            {
                return Err(invalid(
                    "Bundled form's original model changed; rebuild against this installation.",
                ));
            }
            let prior = contract
                .runtime_motion
                .as_ref()
                .ok_or_else(|| invalid("Bundled model has no original runtime-motion proof."))?;
            let candidate = safety::inside(root, &target)?;
            if safety::digest_with_cancel(&candidate, self.model.cancel)? != prior.resource_sha256()
            {
                return Err(invalid(
                    "Bundled compiled model differs from its validated member bytes.",
                ));
            }
            let mut checked = saved.clone();
            let mut report = service.model.inspect(&mut checked)?;
            report.generated_contract=contract.generated_contract.clone();
            if report.needs_review {
                return Err(invalid("Bundled form needs fresh mapping/material review. Open and Analyze its own project before rebuilding."));
            }
            let expected_reference = contract
                .tested_contract
                .as_ref()
                .map(|c| &c.selection.qualification.reference_sha256);
            let actual_reference = report
                .original
                .tested_physics
                .as_ref()
                .map(|c| &c.selection.qualification.reference_sha256);
            if expected_reference != actual_reference {
                return Err(invalid(
                    "Bundled form's tested cloth reference changed; rebuild this bundle.",
                ));
            }
            let validated = service
                .model
                .validate_compiled(&checked, &candidate, &report)?;
            if validated.compiled_sha256 != prior.resource_sha256() {
                return Err(invalid(
                    "Assembled form failed fresh compiled identity validation.",
                ));
            }
        } else if contract.runtime_motion.is_some() {
            return Err(invalid(
                "Assets-only member has unexpected model authority.",
            ));
        }
        if let Some(extras) = &contract.customization {
            for (path, sha) in &extras.files {
                if files.get(path) != Some(sha) {
                    return Err(invalid(
                        "Bundled component is outside its member's output contract.",
                    ));
                }
            }
            service.components().verify_snapshot(saved, extras, root)?;
        } else if saved.customization.active()
            && [
                &saved.customization.icons,
                &saved.customization.animations,
                &saved.customization.effects,
                &saved.customization.audio,
            ]
            .into_iter()
            .any(|rows| {
                rows.as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|r| r.enabled.unwrap_or(true))
            })
        {
            return Err(invalid(
                "Bundled form's selected components lack validated receipts.",
            ));
        }
        crate::package::validate_dependencies(root, files, Some(current_game), self.model.cancel)
    }
}
impl BuildService<'_> {
    /// Caller already holds the primary project lease. Child builds acquire only
    /// their fresh isolated identities, so the main workspace lock never nests.
    pub(crate) fn build_bundle_owned(
        &self,
        project: &Project,
        overwrite_output: bool,
        overwrite_install: bool,
    ) -> Result<BuildResult> {
        let bundle = BundleService {
            settings: self.model.settings,
            runner: self.model.runner,
            builder: self,
            cancel: self.model.cancel,
            clock: self.model.clock,
            log: self.model.log,
        }
        .build(project, overwrite_output, overwrite_install)?;
        Ok(BuildResult {
            prepared: Some(self.model.root(project)?),
            compiled: true,
            vpk: Some(bundle.package.path),
            installed: bundle.installed,
            manifest: Some(bundle.manifest_path),
        })
    }
    pub(crate) fn install_bundle_owned(
        &self,
        project: &Project,
        source: &Path,
        overwrite: bool,
    ) -> Result<PathBuf> {
        let service = BundleService {
            settings: self.model.settings,
            runner: self.model.runner,
            builder: self,
            cancel: self.model.cancel,
            clock: self.model.clock,
            log: self.model.log,
        };
        let manifest =
            service.read_manifest(&self.model.root(project)?.join("build-manifest.json"))?;
        let gate = InstallGate {
            service: &service,
            manifest: &manifest,
        };
        PackageService {
            settings: self.model.settings,
            runner: self.model.runner,
            cancel: self.model.cancel,
            clock: self.model.clock,
            validator: &gate,
            log: self.model.log,
        }
        .install(project, source, overwrite)
    }
}
struct InstallGate<'a, 'b> {
    service: &'a BundleService<'b, BuildService<'b>>,
    manifest: &'a bundle::BundleManifest<BuildManifest>,
}
impl ResourceValidator for InstallGate<'_, '_> {
    fn validate_frozen(
        &self,
        _: &Project,
        _: &Path,
        _: &Hashes,
        _: &CancellationToken,
        _: &TimingRecorder,
    ) -> Result<()> {
        Err(invalid(
            "Existing bundle installation requires its actual archive.",
        ))
    }
    fn validate_install(
        &self,
        project: &Project,
        candidate: &Vpk,
        current_game: &Vpk,
        sha: &str,
        _: &CancellationToken,
        _: &TimingRecorder,
    ) -> Result<()> {
        self.service
            .verify_install(project, candidate, current_game, self.manifest, sha)
    }
}
