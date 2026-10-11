//! Validation uses a complete private copy; original tools and project settings are never modified.
use crate::{
    acquisition::{self, AcquisitionPlan, IntegrityReceipt},
    binary_audit, manual_setup,
};
use deadlock_core::{
    process::ToolExecutor,
    safety::{self, digest_with_cancel},
    CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_project::{AnimationPolicy, Project, Settings};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct ValidationOptions {
    pub toolroot: PathBuf,
    pub manifests: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeEvidence {
    pub class: String,
    pub name: String,
    pub filename: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationEvidence {
    pub native: bool,
    pub kind: String,
    pub compiled: bool,
    pub compiled_model: PathBuf,
    pub compiled_sha256: String,
    pub ag2_nodes_expected: Vec<NodeEvidence>,
    pub ag2_nodes_checked: Vec<NodeEvidence>,
    pub ag2_output_checked: bool,
    pub format_target: Option<u32>,
    pub format_native: bool,
    pub native_migration_verified: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeChecks {
    pub native_modeldoc41: bool,
    pub original_compiled: bool,
    pub gameplay_animgraph2: bool,
    pub ui_animgraph2: bool,
    pub nm_skeleton: bool,
}
impl NativeChecks {
    pub fn all_passed(&self) -> bool {
        self.native_modeldoc41
            && self.original_compiled
            && self.gameplay_animgraph2
            && self.ui_animgraph2
            && self.nm_skeleton
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationReport {
    pub schema: u32,
    pub kind: String,
    pub compatible: bool,
    pub downloaded: bool,
    pub toolset_modified: bool,
    pub source_toolset: PathBuf,
    pub hero: String,
    pub plan_digest: String,
    pub recipe_sha256: String,
    pub report_directory: PathBuf,
    pub validation_copy_directory: PathBuf,
    pub validation_copy_removed: bool,
    pub native_test: String,
    pub integrity: Option<IntegrityReceipt>,
    pub static_audit: Option<serde_json::Value>,
    pub native_result: Option<ValidationEvidence>,
    pub checks: Option<NativeChecks>,
    pub source_reverification: Option<IntegrityReceipt>,
    pub error: Option<String>,
    pub cleanup_error: Option<String>,
    pub scope: String,
}
pub struct OwnedToolsetCopy {
    pub settings: Settings,
    pub project: Project,
    pub report: ValidationReport,
    plan: AcquisitionPlan,
    copy_owned: bool,
}
fn fail(message: impl Into<String>) -> CoreError {
    CoreError::Integrity(message.into())
}
impl OwnedToolsetCopy {
    fn persist(&self) -> Result<()> {
        safety::write_json(
            &self.report.report_directory.join("validation.json"),
            &self.report,
        )
    }
    fn remove_copy(&mut self) {
        if self.copy_owned {
            match fs::remove_dir_all(&self.report.validation_copy_directory) {
                Ok(()) => {
                    self.report.validation_copy_removed = true;
                    self.copy_owned = false;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.report.validation_copy_removed = true;
                    self.copy_owned = false;
                }
                Err(error) => {
                    self.report.validation_copy_removed = false;
                    self.report.cleanup_error = Some(error.to_string());
                }
            }
        }
    }
    /// Only the actual native pipeline should construct this evidence after its deep checks.
    pub fn finish(
        mut self,
        evidence: ValidationEvidence,
        executor: &dyn ToolExecutor,
        timing: &TimingRecorder,
        parent: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<ValidationReport> {
        let result=timing.measure("Finalize original-toolset validation",parent,|_|{
            let _barrier=executor.quiescent()?;
            let checks=native_checks(&evidence,executor.native_execution()&&cfg!(windows),cancel)?;
            self.report.source_reverification=Some(acquisition::validate_files(&self.plan,&self.report.source_toolset,cancel)?);
            self.report.compatible=checks.all_passed();self.report.native_test=if self.report.compatible{"passed"}else{"not certified"}.into();self.report.checks=Some(checks);self.report.native_result=Some(evidence);
            self.remove_copy();self.persist()?;
            if executor.native_execution()&&cfg!(windows)&&!self.report.compatible{return Err(fail("Native validation did not establish every required ModelDoc41/gameplay/UI/NmSkeleton check. See the saved report."));}
            Ok(self.report.clone())
        });
        if let Err(error) = &result {
            self.report.compatible = false;
            self.report.error = Some(error.to_string());
            match executor.quiescent() {
                Ok(_barrier) => self.remove_copy(),
                Err(cleanup) => {
                    let _ = self.retain_for_recovery(cleanup);
                }
            }
            let _ = self.persist();
        }
        result
    }
    pub fn record_failure(&mut self, error: impl std::fmt::Display) -> Result<()> {
        self.report.compatible = false;
        self.report.error = Some(error.to_string());
        self.persist()
    }
    /// Preserve a native test tree when descendant quiescence cannot be proven.
    /// The report records why manual recovery is needed; Drop will not remove it.
    pub fn retain_for_recovery(&mut self, reason: impl std::fmt::Display) -> Result<()> {
        self.copy_owned = false;
        self.report.validation_copy_removed = false;
        self.report.cleanup_error = Some(format!(
            "Private copy retained because tool quiescence was not established: {reason}"
        ));
        self.persist()
    }
}
impl Drop for OwnedToolsetCopy {
    fn drop(&mut self) {
        self.remove_copy();
        let _ = self.persist();
    }
}
pub fn native_checks(
    evidence: &ValidationEvidence,
    real_native: bool,
    cancel: &CancellationToken,
) -> Result<NativeChecks> {
    let identities = |nodes: &[NodeEvidence]| {
        nodes
            .iter()
            .filter(|n| !n.filename.is_empty())
            .cloned()
            .collect::<BTreeSet<_>>()
    };
    let expected = identities(&evidence.ag2_nodes_expected);
    let checked = identities(&evidence.ag2_nodes_checked);
    let deep = evidence.ag2_output_checked && expected == checked;
    let native = real_native
        && evidence.native
        && evidence.kind == "native_original_compile"
        && evidence.format_native
        && evidence.format_target == Some(41)
        && evidence.native_migration_verified;
    let output = evidence.compiled_model.is_file()
        && digest_with_cancel(&evidence.compiled_model, cancel)? == evidence.compiled_sha256;
    Ok(NativeChecks {
        native_modeldoc41: native,
        original_compiled: evidence.compiled && output,
        gameplay_animgraph2: deep && expected.iter().any(|n| n.class == "DefaultAnimGraph2"),
        ui_animgraph2: deep
            && expected
                .iter()
                .any(|n| n.class == "AnimGraph2" && deadlock_core::text::casefold(&n.name) == "ui"),
        nm_skeleton: deep && expected.iter().any(|n| n.class == "NmSkeletonReference"),
    })
}
/// Run read-only integrity/static checks, then create a private complete native-test copy.
/// The caller invokes validate_original with returned settings/project and calls finish.
/// For a static-only non-Windows action use acquisition::validate_files + binary_audit::scan_tree directly.
pub fn prepare_existing_copy(
    settings: &Settings,
    project: &Project,
    options: &ValidationOptions,
    timing: &TimingRecorder,
    parent: Option<&str>,
    cancel: &CancellationToken,
) -> Result<OwnedToolsetCopy> {
    timing.measure("Prepare independent toolset validation",parent,|span|{
        cancel.check()?;acquisition::no_links(&options.toolroot)?;acquisition::no_links(&options.manifests)?;
        let source=safety::resolve_path(&options.toolroot)?;if !source.is_dir(){return Err(fail("Existing toolset folder is missing."));}if settings.workspace.is_empty(){return Err(fail("Select a builder workspace outside the acquired toolset."));}
        let recipe=acquisition::recipe()?;let prepared=acquisition::prepared_plan(&recipe,&options.manifests,&source,cancel)?;
        let workspace=Path::new(&settings.workspace).join("v");manual_setup::require_separate(&workspace,&[&source,&options.manifests,Path::new(&settings.deadlock),Path::new(&settings.csdk),Path::new(&settings.cswin),Path::new(&settings.optional_cs2)])?;
        safety::require_native_path(&workspace.join("00000000/acquisition-plan.json"))?;fs::create_dir_all(&workspace).map_err(|e|CoreError::io("Create validation workspace",e))?;
        let directory=loop{let candidate=workspace.join(&uuid::Uuid::new_v4().simple().to_string()[..8]);match fs::create_dir(&candidate){Ok(())=>break candidate,Err(e)if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,Err(e)=>return Err(CoreError::io("Create private validation workspace",e))}};let clone=directory.join("t");
        safety::atomic_write(&directory.join("acquisition-plan.json"),prepared.plan_text)?;
        let mut next_settings=settings.clone();next_settings.cswin=clone.to_string_lossy().into_owned();next_settings.model_backend="cswin".into();next_settings.model_format="auto".into();next_settings.cswin_resourcecompiler=clone.join("game/bin/win64/resourcecompiler.exe").to_string_lossy().into_owned();next_settings.model_game=clone.join("game/csgo").to_string_lossy().into_owned();next_settings.workspace=directory.to_string_lossy().into_owned();
        let mut next_project=project.clone();next_project.id=uuid::Uuid::new_v4().simple().to_string();next_project.imported_source.clear();next_project.original_compiled.clear();next_project.animation_policy=AnimationPolicy::Preserve;next_project.auto_decompile=true;
        let mut copy=OwnedToolsetCopy{settings:next_settings,project:next_project,plan:prepared.plan,copy_owned:false,report:ValidationReport{schema:1,kind:"manual_existing_toolset_validation".into(),compatible:false,downloaded:false,toolset_modified:false,source_toolset:source.clone(),hero:project.original_model.clone(),plan_digest:prepared.plan_digest,recipe_sha256:acquisition::recipe_sha256(),report_directory:directory,validation_copy_directory:clone.clone(),validation_copy_removed:false,native_test:"not run".into(),integrity:None,static_audit:None,native_result:None,checks:None,source_reverification:None,error:None,cleanup_error:None,scope:"Pinned selected files and original selected hero only; replacement/live-game tests are separate.".into()}};
        let result:Result<()>=(||{
            copy.report.integrity=Some(timing.measure("Verify acquired source toolset",Some(span.id()),|_|acquisition::validate_files(&copy.plan,&source,cancel))?);
            let audit=timing.measure("Audit static ModelDoc classes",Some(span.id()),|_|binary_audit::scan_tree(&source,cancel,|_|{}))?;
            let missing:Vec<_>=binary_audit::CLASSES.iter().filter(|(name,_)|audit["class_evidence_by_module"][name].as_array().is_none_or(|v|v.is_empty())).map(|(name,_)|*name).collect();copy.report.static_audit=Some(audit.clone());
            if !missing.is_empty(){return Err(fail(format!("Required ModelDoc classes were not found: {}. Reverify the complete pinned tool family; do not copy individual DLLs.",missing.join(", "))));}
            if audit["scan_complete_without_errors"]!=true{return Err(fail("Static ModelDoc scan was incomplete; inspect the saved errors before native testing."));}
            if project.original_model.is_empty()||!Path::new(&settings.pak).is_file()||!Path::new(&settings.source2viewer).is_file(){return Err(fail("Select the original hero, complete Deadlock archive, and Source2Viewer CLI for the native original/AG2 test."));}
            let need=copy.plan.installed_bytes.checked_add(1<<30).ok_or_else(||fail("Validation size overflow"))?;let free=fs2::available_space(&copy.report.report_directory).map_err(|e|CoreError::io("Check validation free space",e))?;if free<need{return Err(fail(format!("Validation needs at least {need} free bytes for an independent toolset copy plus initial output headroom.")));}
            for item in &copy.plan.files{safety::require_native_path(&safety::inside(&clone,&item.path)?)?;}
            fs::create_dir(&clone).map_err(|e|CoreError::io("Create private tool copy",e))?;copy.copy_owned=true;
            timing.measure("Copy complete selected toolset",Some(span.id()),|_|{for item in &copy.plan.files{cancel.check()?;let input=safety::inside(&source,&item.path)?;acquisition::no_links(&input)?;safety::atomic_copy(&input,&safety::inside(&clone,&item.path)?,cancel)?;}Ok(())})?;
            timing.measure("Verify independent toolset copy",Some(span.id()),|_|acquisition::validate_files(&copy.plan,&clone,cancel))?;
            copy.report.source_reverification=Some(timing.measure("Reverify unchanged source toolset",Some(span.id()),|_|acquisition::validate_files(&copy.plan,&source,cancel))?);copy.persist()?;Ok(())
        })();
        if let Err(error)=result{copy.report.error=Some(error.to_string());copy.remove_copy();copy.persist()?;return Err(error);}Ok(copy)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_certification_requires_all_roles_matching_identities_and_output_hash() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("original.vmdl_c");
        fs::write(&output, b"compiled fixture").unwrap();
        let cancel = CancellationToken::new();
        let nodes = vec![
            NodeEvidence {
                class: "DefaultAnimGraph2".into(),
                name: "".into(),
                filename: "graphs/game.vnmgraph".into(),
            },
            NodeEvidence {
                class: "AnimGraph2".into(),
                name: "UI".into(),
                filename: "graphs/ui.vnmgraph".into(),
            },
            NodeEvidence {
                class: "NmSkeletonReference".into(),
                name: "".into(),
                filename: "models/hero.vnmskel".into(),
            },
        ];
        let mut evidence = ValidationEvidence {
            native: true,
            kind: "native_original_compile".into(),
            compiled: true,
            compiled_sha256: digest_with_cancel(&output, &cancel).unwrap(),
            compiled_model: output.clone(),
            ag2_nodes_expected: nodes.clone(),
            ag2_nodes_checked: nodes,
            ag2_output_checked: true,
            format_target: Some(41),
            format_native: true,
            native_migration_verified: true,
        };
        assert!(native_checks(&evidence, true, &cancel)
            .unwrap()
            .all_passed());
        assert!(!native_checks(&evidence, false, &cancel)
            .unwrap()
            .all_passed());
        evidence.ag2_nodes_checked[1].filename = "graphs/wrong.vnmgraph".into();
        assert!(!native_checks(&evidence, true, &cancel)
            .unwrap()
            .all_passed());
        evidence.ag2_nodes_checked = evidence.ag2_nodes_expected.clone();
        fs::write(output, b"changed after compile").unwrap();
        assert!(!native_checks(&evidence, true, &cancel)
            .unwrap()
            .all_passed());
    }
}
