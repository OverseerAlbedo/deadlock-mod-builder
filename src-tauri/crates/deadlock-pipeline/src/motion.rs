//! Compose typed motion gates with the independently pinned native reader.
//! Validation operates on immutable snapshots before generated output is replaced.
use crate::{invalid, vpk_error};
use deadlock_cloth::{
    MotionValidation, RestorationReceipt, RuntimeAudit, TestedAudit, TestedSelection,
    TransferReceipt,
};
use deadlock_core::{
    process::{ProcessResult, ToolExecutor},
    safety, CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_project::Project;
use deadlock_resource::{
    binary, container,
    independent::{self, NumericProof},
};
use deadlock_skeleton::extensions::SourceBinding;
use deadlock_tools::cloth_reader::{self, ReaderIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndependentAudit {
    pub schema: u32,
    pub resource_sha256: String,
    pub reader: ReaderIdentity,
    pub blocks: BTreeMap<String, NumericProof>,
    pub processes: Vec<ProcessResult>,
    pub stdout_sha256: BTreeMap<String, String>,
    pub numeric_tolerance_applied: bool,
    pub in_game_motion_tested: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestedContract {
    pub reference: PathBuf,
    pub source_sha256: String,
    pub selection: TestedSelection,
    pub independent_reference: IndependentAudit,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ValidatedMotion {
    Generated { audit: deadlock_cloth::generated::Audit, independent: IndependentAudit },
    Preserve {
        audit: MotionValidation,
        independent: IndependentAudit,
    },
    Tested {
        audit: TestedAudit,
        independent: IndependentAudit,
    },
}
impl ValidatedMotion {
    pub fn resource_sha256(&self) -> &str {
        match self {
            Self::Generated { audit, .. } => &audit.resource_sha256,
            Self::Preserve { audit, .. } => &audit.rebuilt.resource_sha256,
            Self::Tested { audit, .. } => &audit.motion.rebuilt.resource_sha256,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Restoration {
    Generated { audit: deadlock_cloth::generated::Audit, native_model_backup: PathBuf },
    Preserve {
        receipt: RestorationReceipt,
        native_model_backup: PathBuf,
    },
    Tested {
        receipt: TransferReceipt,
        native_model_backup: PathBuf,
    },
}
pub struct MotionService<'a> {
    pub directory: &'a Path,
    pub reader_root: &'a Path,
    pub runner: &'a dyn ToolExecutor,
    pub cancel: &'a CancellationToken,
    pub clock: &'a TimingRecorder,
    pub log: &'a dyn Fn(&str),
}
impl MotionService<'_> {
    pub fn audit(&self, raw: &[u8]) -> Result<RuntimeAudit> {
        self.clock
            .measure("Audit native runtime motion", None, |_| {
                self.cancel.check()?;
                let audit =
                    deadlock_cloth::audit_resource(raw).map_err(|e| invalid(e.to_string()))?;
                self.cancel.check()?;
                Ok(audit)
            })
    }
    pub fn independent(&self, raw: &[u8]) -> Result<IndependentAudit> {
        self.clock.measure("Independently verify native cloth values",None,|parent|{
            self.cancel.check()?;
            let reader=self.clock.measure("Verify full-precision reader identity",Some(parent.id()),|_|cloth_reader::identity(self.reader_root,self.cancel))?;
            let sha=format!("{:x}",Sha256::digest(raw));
            let layout=container::layout(raw).map_err(|e|invalid(e.to_string()))?;
            let blocks:Vec<_>=["PHYS","DATA"].into_iter().filter(|name|layout.blocks.iter().any(|b|b.name==*name)).collect();
            if !blocks.contains(&"DATA"){return Err(invalid("Compiled model has no DATA block for independent validation."));}
            for name in &blocks{container::unique(&layout.blocks,name).map_err(|e|invalid(e.to_string()))?;}
            let key=format!("{:x}",Sha256::digest(format!("independent-v2:{sha}:{}:{}",reader.manifest_sha256,reader.executable_sha256).as_bytes()));
            fs::create_dir_all(self.directory).map_err(|e|CoreError::io("Create motion evidence folder",e))?;
            let cache=self.directory.join(format!("{key}.json"));
            if cache.is_file(){
                let reused=(||->Result<Option<IndependentAudit>>{
                    let mut saved:IndependentAudit=serde_json::from_slice(&read_resource(&cache,8*1024*1024,self.cancel)?).map_err(|e|invalid(e.to_string()))?;
                    if saved.schema!=2||saved.resource_sha256!=sha||saved.reader.manifest_sha256!=reader.manifest_sha256||saved.reader.executable_sha256!=reader.executable_sha256||saved.numeric_tolerance_applied||saved.blocks.len()!=blocks.len()||saved.processes.len()!=blocks.len()||saved.stdout_sha256.len()!=blocks.len(){return Ok(None)}
                    // Recheck immutable raw evidence against current typed bytes.
                    // Success booleans from an old JSON receipt are never enough.
                    for(name,process)in blocks.iter().zip(&saved.processes){
                        self.cancel.check()?;
                        deadlock_vpk::check_unlinked_path(&process.stdout_path,false).map_err(vpk_error)?;
                        let digest=safety::digest_with_cancel(&process.stdout_path,self.cancel)?;
                        if saved.stdout_sha256.get(*name)!=Some(&digest){return Ok(None)}
                        let proof=self.clock.measure(format!("Recheck cached {name} values"),Some(parent.id()),|_|verify_output(raw,&layout,name,process,self.cancel))?;
                        saved.blocks.insert((*name).into(),proof);
                    }
                    saved.reader=reader.clone();
                    saved.in_game_motion_tested=false;
                    Ok(Some(saved))
                })();
                match reused{
                    Ok(Some(saved))=>{self.cancel.check()?;self.clock.start_step("Reuse full-precision audit",Some(parent.id()))?.cached()?;return Ok(saved)},
                    Err(CoreError::Cancelled)=>return Err(CoreError::Cancelled),
                    _=>(self.log)("Cached full-precision evidence is incomplete or changed; reading the native snapshot again"),
                }
            }
            let temp=tempfile::Builder::new().prefix("independent-").tempdir_in(self.directory).map_err(|e|CoreError::io("Create native-reader snapshot",e))?;
            let snapshot=temp.path().join(format!("{sha}.vmdl_c"));safety::atomic_write(&snapshot,raw)?;
            let mut proofs=BTreeMap::new();let mut processes=Vec::new();let mut stdout_sha256=BTreeMap::new();
            let outcome=(||->Result<()>{
                for name in blocks{
                    self.cancel.check()?;
                    (self.log)(&format!("Reading full-precision {name}; complete native output is retained in the operation logs"));
                    let process=self.clock.measure(format!("Read full-precision {name}"),Some(parent.id()),|_|self.runner.run(&reader.command(&snapshot,name)?))?;
                    processes.push(process);
                    let process=processes.last().ok_or_else(||invalid("Missing reader process result"))?;
                    let proof=self.clock.measure(format!("Compare every typed {name} value"),Some(parent.id()),|_|verify_output(raw,&layout,name,process,self.cancel))?;
                    stdout_sha256.insert(name.into(),safety::digest_with_cancel(&process.stdout_path,self.cancel)?);
                    proofs.insert(name.into(),proof);
                }
                if safety::digest_with_cancel(&snapshot,self.cancel)?!=sha{return Err(invalid("Independent reader snapshot changed during validation."));}
                let fresh=cloth_reader::identity(self.reader_root,self.cancel)?;
                if fresh.manifest_sha256!=reader.manifest_sha256||fresh.executable_sha256!=reader.executable_sha256{return Err(invalid("Independent reader changed during validation."));}
                Ok(())
            })();
            if let Err(error)=outcome{
                let evidence=cache.with_extension("failure.json");
                let report=serde_json::json!({"schema":2,"status":"failed","resource_sha256":sha,"reader":reader,"processes":processes,"reason":error.to_string(),"numeric_tolerance_applied":false});
                if let Err(write)=safety::write_json(&evidence,&report){(self.log)(&format!("Could not finalize native reader failure evidence: {write}"));}
                (self.log)(&format!("Native cloth validation failed. Diagnostic report: {}",evidence.display()));
                return Err(error);
            }
            let audit=IndependentAudit{schema:2,resource_sha256:sha,reader,blocks:proofs,processes,stdout_sha256,numeric_tolerance_applied:false,in_game_motion_tested:false};
            safety::write_json(&cache,&audit)?;Ok(audit)
        })
    }
    pub fn inspect_tested(
        &self,
        project: &Project,
        original: &[u8],
        source: Option<&SourceBinding>,
    ) -> Result<Option<TestedContract>> {
        let Some(reference) = selection(project)? else {
            return Ok(None);
        };
        self.clock
            .measure("Qualify tested cloth reference", None, |_| {
                let raw = read_resource(&reference, 256 * 1024 * 1024, self.cancel)?;
                let selection = deadlock_cloth::inspect_tested(original, &raw, source)
                    .map_err(|e| invalid(e.to_string()))?;
                let independent_reference = self.independent(&raw)?;
                Ok(Some(TestedContract {
                    reference,
                    source_sha256: safety::digest_with_cancel(
                        Path::new(&project.dmx),
                        self.cancel,
                    )?,
                    selection,
                    independent_reference,
                }))
            })
    }
    fn reference(&self, project: &Project, contract: &TestedContract) -> Result<Vec<u8>> {
        let path = selection(project)?.ok_or_else(|| {
            invalid("Tested cloth selection was disabled after analysis. Analyze again.")
        })?;
        let raw = read_resource(&path, 256 * 1024 * 1024, self.cancel)?;
        if format!("{:x}", Sha256::digest(&raw))
            != contract.selection.qualification.reference_sha256
        {
            return Err(invalid(
                "Tested cloth reference changed. Analyze and rebuild.",
            ));
        }
        Ok(raw)
    }
    pub fn validate(
        &self,
        project: &Project,
        original: &[u8],
        candidate: &[u8],
        tested: Option<&TestedContract>,
        source: Option<&SourceBinding>,
    ) -> Result<ValidatedMotion> {
        self.clock
            .measure("Validate compiled runtime motion", None, |_| {
                self.cancel.check()?;
                if deadlock_cloth_authoring::secondary::active(project) { return Err(invalid("Generated secondary motion requires its fresh native build contract.")); }
                let result = if let Some(contract) = tested {
                    let reference = self.reference(project, contract)?;
                    let audit = deadlock_cloth::validate_tested(
                        original,
                        &reference,
                        candidate,
                        &contract.selection,
                        source,
                    )
                    .map_err(|e| invalid(e.to_string()))?;
                    let independent = self.independent(candidate)?;
                    ValidatedMotion::Tested { audit, independent }
                } else {
                    if selection(project)?.is_some() {
                        return Err(invalid(
                            "Tested cloth needs a qualified build contract. Analyze and rebuild.",
                        ));
                    }
                    let audit = deadlock_cloth::validate_motion(original, candidate, source)
                        .map_err(|e| invalid(e.to_string()))?;
                    if !audit.passed {
                        return Err(invalid(format!(
                            "Runtime cloth validation failed: {}",
                            audit.errors.join("; ")
                        )));
                    }
                    let independent = self.independent(candidate)?;
                    ValidatedMotion::Preserve { audit, independent }
                };
                self.cancel.check()?;
                Ok(result)
            })
    }
    pub fn restore(
        &self,
        project: &Project,
        original: &[u8],
        output: &Path,
        tested: Option<&TestedContract>,
        source: Option<&SourceBinding>,
    ) -> Result<(Restoration, ValidatedMotion)> {
        self.clock.measure("Restore or import runtime cloth",None,|_|{
            let candidate=read_resource(output,512*1024*1024,self.cancel)?;
            let candidate_sha=format!("{:x}",Sha256::digest(&candidate));
            let native_model_backup=self.directory.join("native-model").join(format!("{candidate_sha}.vmdl_c"));
            self.clock.measure("Preserve native compiler output",None,|_|{
                if native_model_backup.exists(){if safety::digest_with_cancel(&native_model_backup,self.cancel)?!=candidate_sha{return Err(invalid("Native compiler backup changed."));}}
                else{safety::atomic_write(&native_model_backup,&candidate)?;}
                Ok(())
            })?;
            if let Some(source)=source{if safety::digest_with_cancel(Path::new(&project.dmx),self.cancel)?!=source.source_sha256{return Err(invalid("Custom source changed after inspection. Analyze again."));}}
            let (changed,restoration)=if let Some(contract)=tested{
                if safety::digest_with_cancel(Path::new(&project.dmx),self.cancel)?!=contract.source_sha256{return Err(invalid("Replacement DMX changed after tested-cloth analysis. Analyze again."));}
                let reference=self.reference(project,contract)?;
                let (raw,receipt)=deadlock_cloth::apply_tested(original,&reference,&candidate,&contract.selection,source).map_err(|e|invalid(e.to_string()))?;
                (raw,Restoration::Tested{receipt,native_model_backup:native_model_backup.clone()})
            }else{
                if selection(project)?.is_some(){return Err(invalid("Tested cloth has no qualified analysis contract."));}
                let(raw,receipt)=deadlock_cloth::restore(original,&candidate,source).map_err(|e|invalid(e.to_string()))?;
                (raw,Restoration::Preserve{receipt,native_model_backup:native_model_backup.clone()})
            };
            let audit=self.validate(project,original,&changed,tested,source)?;
            // Native readers have finished before acquiring the process barrier.
            // Running the reader while holding this barrier would deadlock Runner.
            let _barrier=self.runner.quiescent()?;
            self.cancel.check()?;
            if safety::digest_with_cancel(output,self.cancel)?!=candidate_sha{return Err(invalid("Compiled output changed during motion validation; no replacement committed."));}
            if let Some(source)=source{if safety::digest_with_cancel(Path::new(&project.dmx),self.cancel)?!=source.source_sha256{return Err(invalid("Custom source changed during motion validation."));}}
            if let Some(contract)=tested{
                if safety::digest_with_cancel(Path::new(&project.dmx),self.cancel)?!=contract.source_sha256{return Err(invalid("Replacement DMX changed during motion validation."));}
                let reference=self.reference(project,contract)?;
                if format!("{:x}",Sha256::digest(&reference))!=contract.selection.qualification.reference_sha256{return Err(invalid("Tested reference changed during motion validation."));}
            }
            if changed!=candidate{safety::atomic_write(output,&changed)?;}
            if safety::digest_with_cancel(output,self.cancel)?!=audit.resource_sha256(){return Err(invalid("Compiled model changed after motion validation."));}
            Ok((restoration,audit))
        })
    }
}
fn verify_output(
    raw: &[u8],
    layout: &deadlock_vpk::ResourceLayout,
    name: &str,
    process: &ProcessResult,
    cancel: &CancellationToken,
) -> Result<NumericProof> {
    if !process.success || process.code != Some(0) {
        return Err(invalid(format!(
            "Native cloth reader failed; full output: {}",
            process.stdout_path.display()
        )));
    }
    let text = process.read_stdout_utf8(512 * 1024 * 1024)?;
    if text.len() as u64 != process.stdout_bytes {
        return Err(invalid(
            "Native reader output length changed after capture.",
        ));
    }
    let decoded = independent::parse_cli_blocks(&text).map_err(|e| {
        invalid(format!(
            "{e}\nFull reader output: {}",
            process.stdout_path.display()
        ))
    })?;
    if decoded.len() != 1 {
        return Err(invalid(format!(
            "Expected exactly one independent {name} block; full output: {}",
            process.stdout_path.display()
        )));
    }
    let block = container::unique(&layout.blocks, name).map_err(|e| invalid(e.to_string()))?;
    let expected =
        binary::decode(container::payload(raw, block).map_err(|e| invalid(e.to_string()))?)
            .map_err(|e| invalid(e.to_string()))?;
    cancel.check()?;
    let proof = independent::verify(&decoded[0].root, &expected.root).map_err(|e| {
        invalid(format!(
            "{e}\nFull reader output: {}",
            process.stdout_path.display()
        ))
    })?;
    cancel.check()?;
    Ok(proof)
}
pub fn selection(project: &Project) -> Result<Option<PathBuf>> {
    if !project.customization.active() {
        return Ok(None);
    }
    let Some(config) = project
        .customization
        .physics
        .as_ref()
        .filter(|v| v.enabled.unwrap_or(false))
    else {
        return Ok(None);
    };
    if !project.customization.model_enabled() {
        return Err(invalid(
            "Include a replacement model to use tested cloth physics.",
        ));
    }
    if config.hero_path.as_deref() != Some(&project.original_model) {
        return Err(invalid(
            "Tested cloth is not selected for this hero. Select its tested model again in Motion.",
        ));
    }
    let source = config.source.as_deref().unwrap_or("").trim();
    if source.is_empty() {
        return Err(invalid(
            "Choose a tested compiled .vmdl_c model, or turn off tested cloth physics.",
        ));
    }
    let path = Path::new(source);
    if !path.is_file()
        || !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("vmdl_c"))
    {
        return Err(invalid(
            "Tested cloth requires an existing compiled .vmdl_c model.",
        ));
    }
    deadlock_vpk::check_unlinked_path(path, false).map_err(vpk_error)?;
    Ok(Some(path.canonicalize().map_err(|e| {
        CoreError::io("Resolve tested cloth reference", e)
    })?))
}
pub fn read_resource(path: &Path, limit: u64, cancel: &CancellationToken) -> Result<Vec<u8>> {
    cancel.check()?;
    deadlock_vpk::check_unlinked_path(path, false).map_err(vpk_error)?;
    let file =
        fs::File::open(path).map_err(|e| CoreError::io(format!("Read {}", path.display()), e))?;
    let metadata = file
        .metadata()
        .map_err(|e| CoreError::io("Inspect compiled resource", e))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(invalid(
            "Compiled resource exceeds the supported validation limit.",
        ));
    }
    let mut raw = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut raw)
        .map_err(|e| CoreError::io("Read compiled resource", e))?;
    if raw.len() as u64 > limit {
        return Err(invalid(
            "Compiled resource grew beyond the validation limit.",
        ));
    }
    cancel.check()?;
    Ok(raw)
}
