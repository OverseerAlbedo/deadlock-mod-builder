//! Separate owned compiler staging and frozen-output model identity validation.
//! Source headers remain authoritative; native loader migration is never simulated.
use crate::{
    compilation,
    inspect::{InspectionReport, ModelPipeline, OriginalInspection},
    invalid,
    motion::{self, MotionService, ValidatedMotion},
    sources,
    workspace::WorkspaceLease,
};
use deadlock_core::{safety, CoreError, Result};
use deadlock_model::{ag2, modeldoc};
use deadlock_project::{AnimationPolicy, Project};
use deadlock_resource::{binary, container, Value};
use deadlock_skeleton::{
    bindings, extensions, source_bones::SkeletonReference, transforms, Skeleton,
};
use deadlock_tools::{compiler, toolchain};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
#[cfg(test)]
mod capture_tests;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FormatEvidence {
    Native(compiler::FormatProbeReport),
    Offline {
        target: Option<u32>,
        evidence: String,
        native: bool,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerPlan {
    pub compiler: String,
    pub command: Vec<String>,
    pub content: PathBuf,
    pub game: PathBuf,
    pub temporary_model: PathBuf,
    pub original_source_sha256: String,
    pub temporary_source_sha256: String,
    pub changes: Vec<String>,
    pub ag2: Vec<ag2::Reference>,
    pub source_file_sha256: BTreeMap<String, String>,
    pub missing_compiled_dependencies: Vec<String>,
    pub animation_policy: AnimationPolicy,
    pub compiled: bool,
    pub offline: bool,
    pub format: modeldoc::MigrationPlan,
    pub related_model_formats: BTreeMap<String, modeldoc::MigrationPlan>,
    pub compiler_format_evidence: FormatEvidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile_error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deployed_resources: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compilation: Option<compiler::CompileResult>,
}
fn relative(root: &Path, file: &Path) -> Result<String> {
    file.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map_err(|_| invalid("Compiler file escaped its owned root"))
}
fn copy(source: &Path, destination: &Path) -> Result<()> {
    safety::require_native_path(destination)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| CoreError::io("Create compiler destination", e))?;
    }
    fs::copy(source, destination).map_err(|e| CoreError::io("Stage compiler file", e))?;
    Ok(())
}
impl ModelPipeline<'_> {
    pub fn prepare_cswin(
        &self,
        p: &Project,
        content: &Path,
        game: &Path,
        offline: bool,
    ) -> Result<CompilerPlan> {
        self.clock.measure("Prepare model compiler and ModelDoc",None,|span|{
 let root=self.root(p)?;let report=root.join("compiler-plan.json");safety::write_json(&report,&serde_json::json!({"compiled":false,"offline":offline,"state":"preparing"}))?;
 let selected=if offline{None}else{Some(toolchain::select(self.settings,true)?)};
 let(content_out,game_out,rc,target_game)=if let Some(toolset)=&selected{let paths=toolset.paths(&p.id)?;(paths.content,paths.game,paths.compiler,paths.target_game)}else{(root.join("compiler-preview-content"),root.join("compiler-preview-game"),PathBuf::from("<Selected model resourcecompiler.exe>"),PathBuf::from("<Selected toolset>/game/<target>"))};
 compilation::clear_owned(p,&[content_out.clone(),game_out.clone()],self.cancel)?;
 let hashes=self.clock.measure("Stage model sources and compiled dependencies",Some(span.id()),|_|{
 let mut hashes=BTreeMap::new();for file in sources::files(content,self.cancel)?{self.cancel.check()?;let extension=file.extension().and_then(|e|e.to_str()).unwrap_or("").to_ascii_lowercase();if ["vmdl","dmx","smd","fbx","obj","vanim","vphys","vagrp"].contains(&extension.as_str()){let rel=relative(content,&file)?;let destination=safety::inside(&content_out,&rel)?;safety::require_native_path(&destination)?;let sha=safety::copy_verified(&file,&destination,self.cancel).map_err(|_|invalid("Staged model source differs from inspected bytes"))?;hashes.insert(rel,sha);}}
 for file in sources::files(game,self.cancel)?{self.cancel.check()?;let rel=relative(game,&file)?;if rel.ends_with("_c")&&rel!=format!("{}_c",p.source_model()){copy(&file,&safety::inside(&game_out,&rel)?)?;}}Ok(hashes)})?;
 let source=safety::inside(content,p.source_model())?;let text=sources::read_text(&source)?;modeldoc::migration_plan(&text,None).map_err(|e|invalid(e.to_string()))?;
 let evidence=if let Some(toolset)=&selected{FormatEvidence::Native(compiler::detect_target(toolset,self.runner,&content_out,&game_out,&root.join("compiler-format.json"),&self.settings.model_format,self.clock,Some(span.id()),self.cancel)?)}else{FormatEvidence::Offline{target:None,evidence:"offline_not_probed".into(),native:false}};
 let target=match &evidence{FormatEvidence::Native(v)=>v.target,FormatEvidence::Offline{target,..}=>*target};
 let migration=(||->Result<_>{let migration=modeldoc::migration_plan(&text,target).map_err(|e|invalid(e.to_string()))?;let mut related=BTreeMap::new();for rel in hashes.keys().filter(|p|p.to_ascii_lowercase().ends_with(".vmdl")){related.insert(rel.clone(),modeldoc::migration_plan(&sources::read_text(&safety::inside(&content_out,rel)?)?,target).map_err(|e|invalid(e.to_string()))?);}Ok((migration,related))})();
 let(migration,related)=match migration{Ok(value)=>value,Err(e)=>{safety::write_json(&report,&serde_json::json!({"compiled":false,"offline":offline,"state":"source_format_rejected","error":e.to_string(),"compiler_format_evidence":evidence}))?;return Err(e)}};
 let(upgraded,changes)=ag2::compiler_source(&text,p.animation_policy,target).map_err(|e|invalid(e.to_string()))?;let temporary=safety::inside(&content_out,p.source_model())?;safety::atomic_write(&temporary,upgraded.as_bytes())?;
 let nodes=ag2::source_nodes(&upgraded).map_err(|e|invalid(e.to_string()))?;let mut missing=vec![];for n in &nodes{let name=format!("{}_c",n.filename);if !safety::inside(&game_out,&name)?.is_file(){missing.push(name);}}
 let plan=CompilerPlan{compiler:"selected local AG2 toolset".into(),command:vec![rc.to_string_lossy().into_owned(),"-f".into(),"-i".into(),temporary.to_string_lossy().into_owned(),"-game".into(),target_game.to_string_lossy().into_owned()],content:content_out,game:game_out,temporary_model:temporary.clone(),original_source_sha256:safety::digest_with_cancel(&source,self.cancel)?,temporary_source_sha256:safety::digest_with_cancel(&temporary,self.cancel)?,changes,ag2:nodes,source_file_sha256:hashes,missing_compiled_dependencies:missing,animation_policy:p.animation_policy,compiled:false,offline,format:migration,related_model_formats:related,compiler_format_evidence:evidence,compile_error:None,deployed_resources:vec![],compilation:None};
 safety::write_json(&report,&plan)?;if !offline&&!plan.missing_compiled_dependencies.is_empty(){return Err(invalid(format!("Original compiled AG2 dependencies are required. Select the complete Deadlock pak01_dir.vpk:\n{}",plan.missing_compiled_dependencies.join("\n"))))}
 (self.log)(&format!("Temporary ModelDoc{} prepared; original source revision retained",plan.format.temporary_file_format));Ok(plan)})
    }
    pub fn compile_cswin(&self, p: &Project, content: &Path, game: &Path) -> Result<CompilerPlan> {
        self.clock
            .measure("Compile model with selected toolset", None, |span| {
                let mut plan = self.prepare_cswin(p, content, game, false)?;
                let output = safety::inside(&plan.game, &format!("{}_c", p.source_model()))?;
                let report = self.root(p)?.join("compiler-plan.json");
                let result = compiler::compile_resource(
                    self.runner,
                    Path::new(&plan.command[0]),
                    &plan.temporary_model,
                    &output,
                    Some(Path::new(&plan.command[5])),
                    self.clock,
                    Some(span.id()),
                    self.cancel,
                );
                match result {
                    Ok(result) => plan.compilation = Some(result),
                    Err(error) => {
                        plan.compile_error = Some(error.to_string());
                        safety::write_json(&report, &plan)?;
                        return Err(error);
                    }
                }
                let _barrier = self.runner.quiescent()?;
                self.cancel.check()?;
                self.clock.measure(
                    "Deploy freshly compiled model resources",
                    Some(span.id()),
                    |_| {
                        for file in sources::files(&plan.game, self.cancel)? {
                            self.cancel.check()?;
                            let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("");
                            if !["vmdl_c", "vmesh_c", "vphys_c", "vanim_c", "vagrp_c"]
                                .contains(&extension)
                            {
                                continue;
                            }
                            let rel = relative(&plan.game, &file)?;
                            let destination = safety::inside(game, &rel)?;
                            if rel == format!("{}_c", p.source_model())
                                || !destination.exists()
                                || safety::digest_with_cancel(&destination, self.cancel)?
                                    != safety::digest_with_cancel(&file, self.cancel)?
                            {
                                safety::atomic_write(
                                    &destination,
                                    motion::read_resource(&file, 512 * 1024 * 1024, self.cancel)?,
                                )?;
                                plan.deployed_resources.push(rel);
                            }
                        }
                        Ok(())
                    },
                )?;
                plan.format.native_migration_verified =
                    self.runner.native_execution() && cfg!(windows);
                plan.compiled = true;
                safety::write_json(&report, &plan)?;
                Ok(plan)
            })
    }
    pub fn verify_ag2(
        &self,
        expected: &[ag2::Reference],
        output: &Path,
    ) -> Result<ag2::Verification> {
        self.clock
            .measure("Verify animation graph references", None, |_| {
                let raw = motion::read_resource(output, 512 * 1024 * 1024, self.cancel)?;
                verify_ag2_bytes(expected, &raw)
            })
    }
}
pub fn verify_ag2_bytes(expected: &[ag2::Reference], raw: &[u8]) -> Result<ag2::Verification> {
    let layout = container::layout(raw).map_err(|e| invalid(e.to_string()))?;
    let data = binary::decode(
        container::payload(
            raw,
            container::unique(&layout.blocks, "DATA").map_err(|e| invalid(e.to_string()))?,
        )
        .map_err(|e| invalid(e.to_string()))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let nodes = ag2::from_data(&data.root).map_err(|e| invalid(e.to_string()))?;
    let refs = deadlock_vpk::resource_references(raw).map_err(crate::vpk_error)?;
    ag2::verify(expected, &refs, Some(&nodes)).map_err(|e| invalid(e.to_string()))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledStructure {
    #[serde(flatten)]
    pub meshes: bindings::MeshAudit,
    pub compiled_bindings: bindings::BindingAudit,
    pub mesh_mapping_passed: bool,
    pub compiled_material_references: Vec<String>,
    pub ag2_graph_identities_preserved: bool,
}
/// Structural proof only; caller still owes original/extension/cloth authority and immutable snapshots.
pub fn compiled_structure(
    raw: &[u8],
    reference: &Skeleton,
    expected_ag2: &[ag2::Reference],
    mappings: &deadlock_model::Mappings,
    materials: &[String],
) -> Result<CompiledStructure> {
    compiled_structure_with_custom_bones(
        raw,
        reference,
        expected_ag2,
        mappings,
        materials,
        &BTreeSet::new(),
        false,
    )
}
fn compiled_structure_with_custom_bones(
    raw: &[u8],
    reference: &Skeleton,
    expected_ag2: &[ag2::Reference],
    mappings: &deadlock_model::Mappings,
    materials: &[String],
    custom: &BTreeSet<String>,
    generated: bool,
) -> Result<CompiledStructure> {
    verify_ag2_bytes(expected_ag2, raw)?;
    let layout = container::layout(raw).map_err(|e| invalid(e.to_string()))?;
    let read = |name: &str| -> Result<Vec<Value>> {
        container::read_blocks(raw, name)
            .map(|v| v.into_iter().map(|d| d.root).collect())
            .map_err(|e| invalid(e.to_string()))
    };
    let data = read("DATA")?;
    let ctrl = read("CTRL")?;
    let mdats = read("MDAT")?;
    let agrps = read("AGRP")?;
    if data.len() != 1 || ctrl.len() != 1 {
        return Err(invalid(
            "Compiled model requires unique DATA and CTRL blocks",
        ));
    }
    let meshes = bindings::audit_meshes(&data[0], &ctrl[0], &mdats, &layout.blocks, reference)
        .map_err(|e| invalid(e.to_string()))?;
    let checker=if generated {bindings::audit_bindings_with_generated_bones}else{bindings::audit_bindings_with_custom_bones};
    let compiled_bindings = checker(
        &data[0],
        &ctrl[0],
        &mdats,
        &agrps,
        &layout.blocks,
        custom,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let expected: BTreeSet<_> = deadlock_model::expanded_mappings(mappings)
        .map_err(|e| invalid(e.to_string()))?
        .into_iter()
        .filter(|(_, target)| target != "@disable")
        .map(|(name, _)| name)
        .collect();
    let actual: Vec<_> = meshes
        .meshes
        .iter()
        .map(|m| {
            m.name
                .clone()
                .ok_or_else(|| invalid("Compiled mesh has no reviewed identity"))
        })
        .collect::<Result<_>>()?;
    if actual.len() != expected.len() || actual.iter().cloned().collect::<BTreeSet<_>>() != expected
    {
        return Err(invalid(
            "Compiled mesh set differs from reviewed replacement mapping",
        ));
    }
    let refs = deadlock_vpk::resource_references(raw).map_err(crate::vpk_error)?;
    let material_refs: BTreeSet<_> = refs
        .iter()
        .map(|r| r.strip_suffix("_c").unwrap_or(r))
        .filter(|r| r.ends_with(".vmat"))
        .map(str::to_string)
        .collect();
    if !materials.iter().any(|m| material_refs.contains(m)) {
        return Err(invalid(
            "Compiled model contains no replacement material references",
        ));
    }
    Ok(CompiledStructure {
        meshes,
        compiled_bindings,
        mesh_mapping_passed: true,
        compiled_material_references: material_refs.into_iter().collect(),
        ag2_graph_identities_preserved: true,
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledValidation {
    #[serde(flatten)]
    pub structure: CompiledStructure,
    pub runtime_motion: ValidatedMotion,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skeleton_extension: Option<deadlock_cloth::ExtensionProof>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_scale_audit: Option<transforms::ScaleAudit>,
    pub compiled_sha256: String,
    pub original_sha256: String,
    pub compiled_model: PathBuf,
}
#[derive(Debug, Serialize)]
pub struct OriginalValidation {
    pub original: OriginalInspection,
    pub compiler_plan: CompilerPlan,
    pub compiled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiled_model: Option<PathBuf>,
    pub replacement_dmx_used: bool,
    pub packaged: bool,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_motion: Option<OriginalMotionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolchain_evidence: Option<toolchain::CompileReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_evidence: Option<deadlock_tools::validation::ValidationEvidence>,
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum OriginalMotionResult {
    Compared(deadlock_cloth::MotionValidation),
    Unavailable { passed: bool, reason: String },
}
impl ModelPipeline<'_> {
    pub fn validate_compiled(
        &self,
        p: &Project,
        output: &Path,
        report: &InspectionReport,
    ) -> Result<CompiledValidation> {
        self.clock
            .measure("Validate compiled skeleton and meshes", None, |span| {
                self.cancel.check()?;
                report.verify_sources(self.cancel)?;
                if report.source_identity.blocked {
                    return Err(invalid(
                        "Selected source skeleton did not pass its precompile authority gate",
                    ));
                }
                if safety::digest_with_cancel(Path::new(&p.dmx), self.cancel)?
                    != report.source_sha256
                {
                    return Err(invalid("Replacement source changed after analysis"));
                }
                let original = report.original.compiled_bytes(self.cancel)?;
                let original_sha = report
                    .original
                    .compiled_sha256
                    .as_deref()
                    .ok_or_else(|| invalid("Original compiled authority unavailable"))?;
                let original_data = container::read_blocks(&original, "DATA")
                    .map_err(|e| invalid(e.to_string()))?;
                if original_data.len() != 1 {
                    return Err(invalid("Original model must have unique DATA block"));
                }
                let original_skeleton = Skeleton::from_value(
                    original_data[0]
                        .root
                        .get("m_modelSkeleton")
                        .ok_or_else(|| invalid("Original model skeleton missing"))?,
                )
                .map_err(|e| invalid(e.to_string()))?;
                let reference: SkeletonReference = if p.skeleton_reference.is_empty() {
                    SkeletonReference {
                        sha256: original_sha.into(),
                        skeleton: original_skeleton.clone(),
                        scale_policy: Some(transforms::SCALE_POLICY.into()),
                    }
                } else {
                    serde_json::from_slice(&motion::read_resource(
                        Path::new(&p.skeleton_reference),
                        16 * 1024 * 1024,
                        self.cancel,
                    )?)?
                };
                if reference.sha256 != original_sha {
                    return Err(invalid(
                        "Skeleton reference hash differs from selected original compiled hero",
                    ));
                }
                // A SHA label is not authority for edited reference transforms.
                deadlock_skeleton::check_transfer(&reference.skeleton, &original_skeleton)
                    .map_err(|e| invalid(e.to_string()))?;
                transforms::audit_scale_preservation(
                    &reference.skeleton,
                    &original_skeleton,
                    false,
                )
                .map_err(|e| invalid(e.to_string()))?;
                let raw = motion::read_resource(output, 512 * 1024 * 1024, self.cancel)?;
                // Bind every gate to this exact immutable snapshot; the path
                // is hashed again after readers exit before accepting output.
                let compiled_sha = crate::inspect::hash(&raw);
                let parsed =
                    container::read_blocks(&raw, "DATA").map_err(|e| invalid(e.to_string()))?;
                if parsed.len() != 1 {
                    return Err(invalid("Candidate model must have unique DATA block"));
                }
                let actual = Skeleton::from_value(
                    parsed[0]
                        .root
                        .get("m_modelSkeleton")
                        .ok_or_else(|| invalid("Candidate model skeleton missing"))?,
                )
                .map_err(|e| invalid(e.to_string()))?;
                let original_names: BTreeSet<_> = reference.skeleton.names.iter().collect();
                let actual_names: BTreeSet<_> = actual.names.iter().collect();
                let extended = original_names.len() < actual_names.len()
                    && original_names.is_subset(&actual_names);
                let source = report.source_binding();
                let directory = self.root(p)?.join("runtime-motion");
                let service = MotionService {
                    directory: &directory,
                    reader_root: self.reader_root,
                    runner: self.runner,
                    cancel: self.cancel,
                    clock: self.clock,
                    log: self.log,
                };
                // Native extension/solver and independent full-precision proofs precede use of
                // the complete candidate as the mesh-binding reference.
                let skeleton_extension = if extended && report.original.tested_physics.is_none() && report.secondary_motion.is_none() {
                    Some(
                        deadlock_cloth::audit_skeleton_extension(&original, &raw, source.as_ref())
                            .map_err(|e| invalid(e.to_string()))?,
                    )
                } else {
                    None
                };
                let runtime_motion = if let Some(setup)=&report.secondary_motion {
                    self.validate_secondary(p,&original,&raw,setup,report.generated_contract.as_ref().ok_or_else(||invalid("Generated secondary motion needs a completed native build. Rebuild first."))?,source.as_ref().ok_or_else(||invalid("Generated source binding missing."))?)?
                } else { service.validate(
                    p,
                    &original,
                    &raw,
                    report.original.tested_physics.as_ref(),
                    source.as_ref(),
                )? };
                let custom: BTreeSet<String> = skeleton_extension
                    .as_ref()
                    .and_then(|proof| proof.source_binding.as_ref())
                    .map(|proof| proof.source_bones_checked.iter().cloned().collect())
                    .unwrap_or_default();
                let custom = if let ValidatedMotion::Generated{audit,..}=&runtime_motion { audit.custom_bones.clone() } else { custom };
                let structure = self.clock.measure(
                    "Verify compiled mesh, animation and material identities",
                    Some(span.id()),
                    |_| {
                        compiled_structure_with_custom_bones(
                            &raw,
                            if extended {
                                &actual
                            } else {
                                &reference.skeleton
                            },
                            &report.original.ag2,
                            &report.mappings,
                            &report.dmx.materials,
                            &custom,
                            matches!(runtime_motion,ValidatedMotion::Generated{..}),
                        )
                    },
                )?;
                let scale = if let Some(policy) = &reference.scale_policy {
                    if policy != transforms::SCALE_POLICY {
                        return Err(invalid(
                            "Unknown target bone-scale policy; prepare source again",
                        ));
                    }
                    let projection = if extended {
                        extensions::project_skeleton(&actual, &reference.skeleton.names)
                            .map_err(|e| invalid(e.to_string()))?
                    } else {
                        actual.clone()
                    };
                    Some(
                        transforms::audit_scale_preservation(
                            &projection,
                            &reference.skeleton,
                            false,
                        )
                        .map_err(|e| invalid(e.to_string()))?,
                    )
                } else {
                    None
                };
                let _barrier = self.runner.quiescent()?;
                self.cancel.check()?;
                if safety::digest_with_cancel(output, self.cancel)? != compiled_sha
                    || runtime_motion.resource_sha256() != compiled_sha
                {
                    return Err(invalid(
                        "Compiled output changed during identity validation",
                    ));
                }
                report.verify_sources(self.cancel)?;
                if safety::digest_with_cancel(Path::new(&p.dmx), self.cancel)?
                    != report.source_sha256
                {
                    return Err(invalid("Replacement source changed during validation"));
                }
                let result = CompiledValidation {
                    structure,
                    runtime_motion,
                    skeleton_extension,
                    target_scale_audit: scale,
                    compiled_sha256: compiled_sha,
                    original_sha256: original_sha.into(),
                    compiled_model: output.into(),
                };
                safety::write_json(&self.root(p)?.join("compiled-validation.json"), &result)?;
                (self.log)(
                    "Compiled skeleton, mesh remaps, animation and material identities passed",
                );
                Ok(result)
            })
    }
    pub fn validate_original(&self, p: &Project, compile: bool) -> Result<OriginalValidation> {
        self.clock.measure("Validate original model",None,|span|{
 let root=self.root(p)?;let _lease=WorkspaceLease::acquire_for_runner(&root,self.runner)?;safety::write_json(&root.join("original-validation.json"),&serde_json::json!({"state":"validating","compiled":false,"packaged":false,"installed":false}))?;
 let audit=self.inspect_original(p,None)?;let content=root.join("original-test-content");let game=root.join("original-test-game");compilation::clear_owned(p,&[content.clone(),game.clone()],self.cancel)?;
 self.clock.measure("Stage original-only sources",Some(span.id()),|_|{safety::copy_tree(&crate::sources::base_dir(self.settings,p)?,&content,self.cancel)?;safety::atomic_write(&safety::inside(&content,p.source_model())?,audit.verified_source(self.cancel)?.as_bytes())?;self.sources().support_resources(p,&content,&game)?;Ok(())})?;
 let plan=if compile{self.compile_cswin(p,&content,&game)?}else{self.prepare_cswin(p,&content,&game,true)?};
 let mut result=OriginalValidation{original:audit,compiler_plan:plan,compiled:compile,compiled_model:None,replacement_dmx_used:false,packaged:false,installed:false,runtime_motion:None,toolchain_evidence:None,validation_evidence:None};
 if compile{let output=safety::inside(&game,&format!("{}_c",p.source_model()))?;let raw=motion::read_resource(&output,512*1024*1024,self.cancel)?;let compiled_sha=crate::inspect::hash(&raw);self.clock.measure("Verify original AG2 graph identifiers",Some(span.id()),|_|verify_ag2_bytes(&result.original.ag2,&raw))?;
 let comparison=match result.original.compiled_bytes(self.cancel){Ok(original)=>match deadlock_cloth::validate_motion(&original,&raw,None){Ok(value)=>OriginalMotionResult::Compared(value),Err(error)=>OriginalMotionResult::Unavailable{passed:false,reason:error.to_string()}},Err(CoreError::Cancelled)=>return Err(CoreError::Cancelled),Err(error)=>OriginalMotionResult::Unavailable{passed:false,reason:error.to_string()}};
 if !matches!(&comparison,OriginalMotionResult::Compared(proof) if proof.passed){(self.log)("Original AG2 compile succeeded, but runtime cloth preservation did not pass. See original-validation.json.");}result.runtime_motion=Some(comparison);
 let _barrier=self.runner.quiescent()?;self.cancel.check()?;if safety::digest_with_cancel(&output,self.cancel)?!=compiled_sha{return Err(invalid("Original compiled output changed during validation"));}
 result.toolchain_evidence=Some(toolchain::write_compile_receipt(self.settings,p,&root.join("toolchain-validation.json"),&output,&result.original.ag2,self.runner,self.cancel)?);
 let nodes=result.original.ag2.iter().map(|n|deadlock_tools::validation::NodeEvidence{class:n.class.clone(),name:n.name.clone(),filename:n.filename.clone()}).collect::<Vec<_>>();let format_native=matches!(&result.compiler_plan.compiler_format_evidence,FormatEvidence::Native(p)if p.native);let native=self.runner.native_execution()&&cfg!(windows);
 result.validation_evidence=Some(deadlock_tools::validation::ValidationEvidence{native,kind:if native{"native_original_compile"}else{"non_native_or_test_process_compile"}.into(),compiled:true,compiled_model:output.clone(),compiled_sha256:compiled_sha.clone(),ag2_nodes_expected:nodes.clone(),ag2_nodes_checked:nodes,ag2_output_checked:!result.original.ag2.is_empty(),format_target:result.compiler_plan.format.compiler_target,format_native,native_migration_verified:result.compiler_plan.format.native_migration_verified});
 if result.toolchain_evidence.as_ref().is_none_or(|receipt|receipt.compiled_sha256!=compiled_sha)||safety::digest_with_cancel(&output,self.cancel)?!=compiled_sha{return Err(invalid("Original compiled output changed while writing validation evidence"));}
 result.compiled_model=Some(output);
 }
 safety::write_json(&root.join("original-validation.json"),&result)?;Ok(result)
 })
    }
}
