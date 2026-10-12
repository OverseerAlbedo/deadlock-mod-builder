//! Fresh, typed inspection receipts. JSON receipts are evidence, never build authority.
use crate::{
    invalid,
    motion::{read_resource, MotionService, TestedContract},
    sources::{inventory, read_text, SourceService},
    vpk_error,
};
use deadlock_core::{
    process::ToolExecutor, safety, text::casefold, CancellationToken, CoreError, Result,
    TimingRecorder,
};
use deadlock_dmx::{limits, split, Dmx, DmxSummary, GeometryCounts};
use deadlock_model::{self as model, ag2, Mappings};
use deadlock_project::{AnimationPolicy, LodStrategy, MeshMapping, Project, Settings};
use deadlock_skeleton::{
    extensions::SourceBinding,
    source_bones, source_identity,
    source_skeleton::{RuntimeMotionSourceAudit, SourceAuditReport},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub struct ModelPipeline<'a> {
    pub settings: &'a Settings,
    pub runner: &'a dyn ToolExecutor,
    pub cancel: &'a CancellationToken,
    pub clock: &'a TimingRecorder,
    pub log: &'a dyn Fn(&str),
    pub reader_root: &'a Path,
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RuntimeInspection {
    Audited(deadlock_cloth::RuntimeAudit),
    Unavailable { status: String, reason: String },
}
#[derive(Debug, Serialize)]
pub struct OriginalInspection {
    pub origin: String,
    pub original_model: String,
    pub hero_root: String,
    pub source_sha256: String,
    pub compiled_available: bool,
    pub references: Vec<String>,
    pub ag2: Vec<ag2::Reference>,
    pub compiled_sha256: Option<String>,
    pub compiled_snapshot: Option<PathBuf>,
    pub source_view: PathBuf,
    pub animation_policy: AnimationPolicy,
    pub runtime_motion: RuntimeInspection,
    pub tested_physics: Option<TestedContract>,
    pub layout: Option<deadlock_vpk::ResourceLayout>,
    source_view_sha256: String,
}
impl OriginalInspection {
    pub fn runtime(&self) -> Option<&deadlock_cloth::RuntimeAudit> {
        match &self.runtime_motion {
            RuntimeInspection::Audited(r) => Some(r),
            _ => None,
        }
    }
    pub fn source_audit(&self) -> SourceAuditReport {
        SourceAuditReport {
            compiled_sha256: self.compiled_sha256.clone(),
            runtime_motion: match self.runtime() {
                Some(r) => RuntimeMotionSourceAudit {
                    status: r.status.clone(),
                    resource_sha256: Some(r.resource_sha256.clone()),
                    cloth_nodes: r.cloth_nodes,
                    controls: r.controls.clone(),
                    skeleton: Some(r.skeleton.clone()),
                },
                None => RuntimeMotionSourceAudit {
                    status: "unavailable".into(),
                    ..Default::default()
                },
            },
        }
    }
    pub fn compiled_bytes(&self, cancel: &CancellationToken) -> Result<Vec<u8>> {
        let path=self.compiled_snapshot.as_deref().ok_or_else(||invalid("Select the current game VPK or original compiled model before validating compiled output."))?;
        let raw = read_resource(path, 512 * 1024 * 1024, cancel)?;
        if self.compiled_sha256.as_deref() != Some(hash(&raw).as_str()) {
            return Err(invalid(
                "Original compiled snapshot changed after inspection.",
            ));
        }
        Ok(raw)
    }
    pub fn verified_source(&self, cancel: &CancellationToken) -> Result<String> {
        cancel.check()?;
        let source = read_text(&self.source_view)?;
        if hash(source.as_bytes()) != self.source_view_sha256 {
            return Err(invalid(
                "Original reconciled source changed after inspection.",
            ));
        }
        Ok(source)
    }
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum SourceBoneInspection {
    Original(source_bones::SourceBoneAudit),
    Extension(source_bones::TemplateContractAudit),
}
#[derive(Debug, Serialize)]
pub struct InspectionReport {
    #[serde(skip_serializing_if="Option::is_none")]
    pub secondary_motion: Option<deadlock_cloth_authoring::secondary::Setup>,
    #[serde(skip)] pub generated_contract: Option<crate::secondary_motion::NativeContract>,
    pub project: String,
    pub hero_root: String,
    pub model: model::Analysis,
    pub dmx: DmxSummary,
    pub geometry: GeometryCounts,
    pub source_materials: Option<deadlock_materials::MaterialAudit>,
    pub source_bones: Option<SourceBoneInspection>,
    pub source_identity: source_identity::Preflight,
    pub vertex_preflight: limits::Preflight,
    pub automatic_split: Option<split::SplitReport>,
    pub source_mappings: Mappings,
    pub project_mappings: Mappings,
    pub path_rebases: Vec<String>,
    pub effective_sources: PathBuf,
    pub mappings: Mappings,
    pub unresolved_meshes: Vec<String>,
    pub materials: deadlock_materials::Plans,
    pub warnings: Vec<String>,
    pub unskinned_meshes: Vec<String>,
    pub original: OriginalInspection,
    pub needs_review: bool,
    pub source_sha256: String,
    #[serde(skip)]
    pub(crate) project_sha256: String,
    #[serde(skip)]
    pub(crate) input_hashes: BTreeMap<PathBuf, String>,
    #[serde(skip)]
    pub(crate) effective_hashes: deadlock_vpk::Hashes,
}
impl InspectionReport {
    pub fn source_binding(&self) -> Option<SourceBinding> {
        self.source_identity
            .extension
            .as_ref()
            .map(|e| SourceBinding {
                original_sha256: e.original_sha256.clone(),
                source_sha256: e.source_sha256.clone(),
                skeleton: e.skeleton.clone(),
                extra_names: e.extra_names.clone(),
                required_extra_names: e.required_extra_names.clone(),
            })
    }
    pub fn verify_inputs(&self, project: &Project, cancel: &CancellationToken) -> Result<()> {
        cancel.check()?;
        if project_hash(project)? != self.project_sha256 {
            return Err(invalid("Project choices changed after analysis. Analyze the current project before preparing output."));
        }
        self.verify_sources(cancel)
    }
    pub fn verify_sources(&self, cancel: &CancellationToken) -> Result<()> {
        for (path, expected) in &self.input_hashes {
            deadlock_vpk::check_unlinked_path(path, false).map_err(vpk_error)?;
            if safety::digest_with_cancel(path, cancel)? != *expected {
                return Err(invalid(format!(
                    "Source input changed after analysis: {}",
                    path.display()
                )));
            }
        }
        if inventory(&self.effective_sources, cancel)? != self.effective_hashes {
            return Err(invalid("Effective source tree changed after analysis."));
        }
        self.original.verified_source(cancel)?;
        if self.original.compiled_available {
            self.original.compiled_bytes(cancel)?;
        }
        Ok(())
    }
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn project_hash(project: &Project) -> Result<String> {
    Ok(hash(&serde_json::to_vec(project)?))
}
pub(crate) fn map_error(error: impl std::fmt::Display) -> CoreError {
    invalid(error.to_string())
}
pub(crate) fn material_error(error: deadlock_materials::Error) -> CoreError {
    match error {
        deadlock_materials::Error::Core(e) => e,
        deadlock_materials::Error::Vpk(e) => vpk_error(e),
        other => map_error(other),
    }
}

impl<'a> ModelPipeline<'a> {
    pub fn root(&self, p: &Project) -> Result<PathBuf> {
        crate::workspace::root(self.settings, p)
    }
    pub fn sources(&self) -> SourceService<'a> {
        SourceService {
            settings: self.settings,
            runner: self.runner,
            cancel: self.cancel,
            clock: self.clock,
            log: self.log,
        }
    }
    pub fn motion<'b>(&'b self, directory: &'b Path) -> MotionService<'b> {
        MotionService {
            directory,
            reader_root: self.reader_root,
            runner: self.runner,
            cancel: self.cancel,
            clock: self.clock,
            log: self.log,
        }
    }
    pub fn inspect_original(&self, p: &Project, base: Option<&Path>) -> Result<OriginalInspection> {
        self.clock
            .measure("Analyze original model", None, |parent| {
                self.cancel.check()?;
                let base = match base {
                    Some(p) => p.to_owned(),
                    None => self.sources().ensure_base(p)?,
                };
                let source_path = safety::inside(&base, p.source_model())?;
                let source_bytes = read_resource(&source_path, 128 * 1024 * 1024, self.cancel)?;
                let mut source = std::str::from_utf8(&source_bytes)
                    .map_err(map_error)?
                    .trim_start_matches('\u{feff}')
                    .to_string();
                let source_sha256 = hash(&source_bytes);
                let root = self.root(p)?;
                let mut nodes = ag2::source_nodes(&source).map_err(map_error)?;
                let (bytes, origin) =
                    if !self.settings.pak.is_empty() && Path::new(&self.settings.pak).is_file() {
                        let raw = self.clock.measure(
                            "Read original compiled model from VPK",
                            Some(parent.id()),
                            |_| {
                                deadlock_vpk::Vpk::open(&self.settings.pak)
                                    .and_then(|v| v.read(&format!("{}_c", p.source_model())))
                                    .map_err(vpk_error)
                            },
                        )?;
                        (Some(raw), "Original game VPK (read-only)")
                    } else if !p.original_compiled.is_empty() {
                        (
                            Some(read_resource(
                                Path::new(&p.original_compiled),
                                512 * 1024 * 1024,
                                self.cancel,
                            )?),
                            "Imported original compiled model (read-only)",
                        )
                    } else {
                        (None, "No original compiled resource configured")
                    };
                let mut report = OriginalInspection {
                    origin: origin.into(),
                    original_model: p.original_model.clone(),
                    hero_root: p.hero_root(),
                    source_sha256,
                    compiled_available: bytes.is_some(),
                    references: vec![],
                    ag2: vec![],
                    compiled_sha256: None,
                    compiled_snapshot: None,
                    source_view: safety::inside(
                        &root.join("original-source-view"),
                        p.source_model(),
                    )?,
                    animation_policy: p.animation_policy,
                    runtime_motion: RuntimeInspection::Unavailable {
                        status: "unavailable".into(),
                        reason: "No original compiled resource configured".into(),
                    },
                    tested_physics: None,
                    layout: None,
                    source_view_sha256: String::new(),
                };
                if let Some(raw) = bytes {
                    let layout = deadlock_vpk::resource_blocks(&raw).map_err(vpk_error)?;
                    let refs = deadlock_vpk::resource_references(&raw).map_err(vpk_error)?;
                    let existing: BTreeSet<_> =
                        nodes.iter().map(|n| casefold(&n.filename)).collect();
                    let missing = refs
                        .iter()
                        .map(|r| casefold(r.strip_suffix("_c").unwrap_or(r)))
                        .any(|r| {
                            (r.ends_with(".vnmskel") || r.ends_with(".vnmgraph"))
                                && !existing.contains(&r)
                        });
                    let recovered = if missing {
                        let data = deadlock_resource::container::read_blocks(&raw, "DATA")
                            .map_err(map_error)?;
                        if data.len() != 1 {
                            return Err(invalid(
                                "Original AG2 recovery requires exactly one DATA block.",
                            ));
                        }
                        ag2::from_data(&data[0].root).map_err(map_error)?
                    } else {
                        vec![]
                    };
                    (source, nodes) =
                        ag2::reconcile(&source, Some(&refs), &recovered).map_err(map_error)?;
                    let snapshot = safety::inside(
                        &root.join("original-compiled"),
                        &format!("{}_c", p.source_model()),
                    )?;
                    safety::atomic_write(&snapshot, &raw)?;
                    let directory = root.join("runtime-motion");
                    report.runtime_motion =
                        RuntimeInspection::Audited(self.motion(&directory).audit(&raw)?);
                    report.compiled_sha256 = Some(hash(&raw));
                    report.compiled_snapshot = Some(snapshot);
                    report.references = refs;
                    report.layout = Some(layout);
                }
                report.ag2 = nodes;
                report.source_view_sha256 = hash(source.as_bytes());
                safety::atomic_write(&report.source_view, source.as_bytes())?;
                safety::write_json(&root.join("original-inspection.json"), &report)?;
                (self.log)(&format!(
                    "Original model audited: {} AG2/skeleton nodes; replacement DMX not required",
                    report.ag2.len()
                ));
                Ok(report)
            })
    }
    pub fn inspect(&self, p: &mut Project) -> Result<InspectionReport> {
        self.clock.measure("Analyze replacement",None,|parent|{
            let path_rebases=self.clock.measure("Resolve project asset paths",Some(parent.id()),|_|p.rebase_assets("","").map_err(map_error))?;
            p.validate_action().map_err(map_error)?;
            if !p.customization.model_enabled(){return Err(invalid("Use the component pipeline for an assets-only project."));}
            let required_props=deadlock_preparation::service::required_prop_meshes(p).map_err(map_error)?;
            let initial_inputs=self.fingerprint_inputs(p,&p.materials)?;
            let base=self.sources().ensure_base(p)?;let mut original=self.inspect_original(p,Some(&base))?;let root=self.root(p)?;
            deadlock_vpk::check_unlinked_path(Path::new(&p.dmx),false).map_err(vpk_error)?;
            let dmx=self.clock.measure("Read and parse replacement DMX",Some(parent.id()),|_|Dmx::load_with_cancel(Path::new(&p.dmx),self.cancel).map_err(deadlock_dmx::DmxError::into_core))?;
            let source_sha256=hash(dmx.bytes());
            if !p.expected_dmx_sha256.is_empty()&&p.expected_dmx_sha256!=source_sha256{return Err(invalid("Replacement DMX differs from the imported package hash. Import the revised source package or explicitly review the new source hash."));}
            let summary=self.clock.measure("Analyze DMX meshes, weights and bones",Some(parent.id()),|_|dmx.summary_with_cancel(self.cancel).map_err(deadlock_dmx::DmxError::into_core))?;
            let geometry=self.clock.measure("Count model geometry",Some(parent.id()),|_|dmx.geometry_counts_with_cancel(self.cancel).map_err(deadlock_dmx::DmxError::into_core))?;
            // The game may load another model of this hero (Rem: familiar_wip.vmdl);
            // a readable name and a successful build do not show that.
            let mut target_warning=None;
            if !self.settings.pak.is_empty()&&Path::new(&self.settings.pak).is_file(){
                let archive=deadlock_vpk::Vpk::open(&self.settings.pak).map_err(vpk_error)?;
                target_warning=deadlock_components::heroes::GameModels::read(&archive).ok().flatten().and_then(|game|game.target_warning(p.source_model()).ok().flatten());
                for material in &summary.materials{
                    self.cancel.check()?;let source=safety::inside(&base,material)?;
                    if !p.source_overlay.is_empty()&&safety::inside(Path::new(&p.source_overlay),material)?.is_file(){continue;}
                    if !source.exists()&&archive.entries.contains_key(&casefold(&format!("{material}_c"))){self.sources().decompile(Path::new(&self.settings.pak),&format!("{material}_c"),&base,Some(parent.id()))?;if !source.is_file(){return Err(invalid(format!("Original material could not be decompiled: {material}")));}}
                }
            }
            let effective=self.clock.measure("Prepare effective source overlay",Some(parent.id()),|_|deadlock_materials::effective_sources(&base,optional_path(&p.source_overlay),&root.join("effective-sources"),self.cancel).map_err(material_error))?;
            let source_materials=if !p.source_overlay.is_empty(){Some(deadlock_materials::validate_materials(&summary,&effective,optional_path(&p.material_mapping),optional_path(&p.material_remap)).map_err(material_error)?)}else{None};
            let model=model::analyze(&original.verified_source(self.cancel)?).map_err(map_error)?;
            let names:Vec<_>=summary.meshes.iter().map(|m|m.name.clone()).collect();let name_set=names.iter().cloned().collect();
            let (mappings,_)=self.clock.measure("Resolve mesh and LOD mappings",Some(parent.id()),|_|model::suggest_mappings(&model,&names,&p.meshes,p.lod_strategy,p.auto_map).map_err(map_error))?;
            let project_mappings=mappings.clone();let mut mappings=model::equipment::apply_mappings(&model,&mappings,&p.customization,&name_set).map_err(map_error)?;
            let unresolved_meshes=model.meshes.iter().filter(|m|!mappings.contains_key(&m.name)).map(|m|m.name.clone()).collect::<Vec<_>>();
            let selected=model::selected_meshes(&mappings).map_err(map_error)?;
            if let Some(name)=required_props.iter().find(|n|!selected.contains(*n)){return Err(invalid(format!("Map prepared attached prop mesh before building: {name}")));}
            if let Some(name)=selected.iter().find(|n|!name_set.contains(*n)){return Err(invalid(format!("Replacement DMX does not contain mapped mesh: {name}")));}
            let source_audit=original.source_audit();
            let source_identity=self.clock.measure("Validate source skeleton identity",Some(parent.id()),|_|source_identity::preflight(&dmx,&source_audit,&selected,Some(&source_sha256)).map_err(map_error))?;
            safety::write_json(&root.join("source-skeleton-identity.json"),&source_identity)?;
            if source_identity.blocked{return Err(invalid(format!("Source skeleton additions have no valid source authority: {}",source_identity.identity.errors.join("; "))));}
            let source_bones=if !p.mesh_template.is_empty(){
                let template=read_text(Path::new(&p.mesh_template))?;let hierarchy=model::template_bones(&template).map_err(map_error)?;
                let reference=optional_path(&p.skeleton_reference).map(|f|read_resource(f,32*1024*1024,self.cancel).and_then(|raw|serde_json::from_slice::<source_bones::SkeletonReference>(&raw).map_err(CoreError::from))).transpose()?;
                // The template describes the whole input DMX, even before the
                // user has mapped any replacement mesh. Discover its additive
                // skeleton independently of the selected-mesh build contract;
                // otherwise prepared secondary bones hit the original-only
                // gate and prevent the user from reaching mesh review.
                let template_extension = if source_identity.extension.is_none() {
                    source_identity::source_contract(&dmx, &source_audit, &name_set, &source_sha256)
                        .map_err(map_error)?
                } else {
                    None
                };
                let extension = source_identity.extension.as_ref().or(template_extension.as_ref());
                Some(if let Some(extension) = extension {
                    SourceBoneInspection::Extension(source_bones::template_contract(
                        &dmx, &hierarchy, &source_audit, extension, reference.as_ref(),
                    ).map_err(map_error)?)
                } else {
                    SourceBoneInspection::Original(source_bones::source_bones(
                        &dmx, &hierarchy, reference.as_ref(), original.compiled_sha256.as_deref(),
                    ).map_err(map_error)?)
                })
            }else{None};
            let binding=source_identity.extension.as_ref().map(|e|SourceBinding{original_sha256:e.original_sha256.clone(),source_sha256:e.source_sha256.clone(),skeleton:e.skeleton.clone(),extra_names:e.extra_names.clone(),required_extra_names:e.required_extra_names.clone()});
            if crate::motion::selection(p)?.is_some(){let directory=root.join("runtime-motion");original.tested_physics=self.motion(&directory).inspect_tested(p,&original.compiled_bytes(self.cancel)?,binding.as_ref())?;safety::write_json(&root.join("tested-physics-inspection.json"),&original.tested_physics)?;}
            // The source binding intentionally covers only selected replacement
            // meshes. A first inspection must reach mesh review before it can
            // decide whether those meshes use the preserved secondary rig.
            // Do not substitute the whole-template audit as build authority.
            let secondary_waiting_for_mesh_mapping = deadlock_cloth_authoring::secondary::active(p)
                && binding.is_none() && !unresolved_meshes.is_empty();
            if deadlock_cloth_authoring::secondary::active(p) && deadlock_preparation::service::moving_parts_changed(p) {
                return Err(invalid(deadlock_preparation::service::MOVING_PARTS_CHANGED));
            }
            let secondary_motion = if secondary_waiting_for_mesh_mapping { None } else {
                deadlock_cloth_authoring::secondary::setup(p,&dmx,binding.as_ref(),self.cancel).map_err(map_error)?
            };
            let plans=self.clock.measure("Plan material and texture mappings",Some(parent.id()),|_|deadlock_materials::plan(&summary.materials,&effective,if p.textures.is_empty(){Path::new("__no_textures__")}else{Path::new(&p.textures)},&p.materials,self.cancel).map_err(material_error))?;
            let materials=deadlock_materials::apply_materials(&plans,&p.customization).map_err(material_error)?;
            let source_mappings=mappings.clone();let mut vertex_preflight=limits::preflight(&summary,&selected);let mut automatic_split=None;
            if p.auto_split{
                let counts:BTreeMap<_,_>=geometry.meshes.iter().map(|m|(&m.name,m)).collect();
                let candidates:Vec<_>=summary.meshes.iter().filter(|m|selected.contains(&m.name)&&m.has_skin_weights&&(m.vertices>p.split_target as usize||counts[&m.name].triangles.saturating_mul(3)>p.split_target as u64)).map(|m|m.name.clone()).collect();
                if !candidates.is_empty(){
                    let report=self.clock.measure("Split and verify oversized meshes",Some(parent.id()),|_|split::split_file(Path::new(&p.dmx),&candidates,&root.join("split-dmx"),p.split_target as usize,self.cancel).map_err(deadlock_dmx::DmxError::into_core))?;
                    mappings=expand_mapping(&mappings,&report)?;
                    vertex_preflight=limits::preflight(&summary,&selected.iter().filter(|n|!candidates.contains(n)).cloned().collect());vertex_preflight.automatic_subdivision=true;automatic_split=Some(report);
                }
            }
            if automatic_split.is_none(){self.clock.skip_step("Split oversized meshes",Some(parent.id()),if p.auto_split{"No selected mesh requires subdivision"}else{"Automatic subdivision is disabled"})?;}
            let mut warnings=vertex_preflight.warnings.clone();
            warnings.extend(target_warning);
            if secondary_waiting_for_mesh_mapping {
                warnings.push("Complete Mesh mapping, then Analyze again to check automatic motion. Your saved physics-bone choices are retained; the selected replacement meshes must include the moving parts.".into());
            }
            if let Some(e)=&source_identity.extension{warnings.push(format!("Custom skeleton: {} weighted or ancestor bones require source and compiled binding checks; motion needs an in-game check.",e.required_extra_names.len()));}
            else if source_identity.identity.required&&!source_identity.identity.passed&&!selected.is_empty(){warnings.push("Source skeleton differs from the original. Final compiled identity, binds and cloth still require validation.".into());}
            if original.runtime().is_some_and(|r|r.cloth_nodes>0){warnings.push("Original runtime cloth is present; source ModelDoc alone does not retain its compiled simulation.".into());}
            for(name,previous)in &p.meshes{let missing=previous.targets().map_err(map_error)?.into_iter().filter(|n|!name_set.contains(*n)).collect::<Vec<_>>();if !missing.is_empty(){warnings.push(format!("Previously mapped mesh(es) missing for {name}: {}",missing.join(", ")));}}
            let unused_meshes:Vec<_>=names.iter().filter(|n|!selected.contains(*n)).cloned().collect();
            if !unused_meshes.is_empty()&&!selected.is_empty(){warnings.push(format!("Not used by any render node, so missing in game: {}. Tick them in Mesh mapping (or use Add to the main node).",unused_meshes.join(", ")));}
            if geometry.source_triangles>500000{warnings.push(format!("Replacement source has {} triangles. No new LODs are generated; choose a LOD strategy explicitly.",geometry.source_triangles));}
            let unskinned_meshes:Vec<_>=summary.meshes.iter().filter(|m|selected.contains(&m.name)&&!m.has_skin_weights).map(|m|m.name.clone()).collect();
            if !unskinned_meshes.is_empty()&&!summary.bones.is_empty(){warnings.push(format!("Replacement meshes have no skin weights: {}",unskinned_meshes.join(", ")));}
            if model.lod_groups>1{warnings.push(lod_warning(p.lod_strategy).into());}
            if !model.ag2.is_empty(){warnings.push("AG2 uses the selected native model toolset. Original graph identifiers are verified after compilation; CSDK compiles materials.".into());}
            if p.animation_policy==AnimationPolicy::DisableLegacy{warnings.push("Legacy animation nodes will be disabled only in the compiler copy; check sequences in game.".into());}
            let needs_review=!unresolved_meshes.is_empty()||materials.values().any(|c|c.mode.as_deref()==Some("generate")&&(!c.confirmed.unwrap_or(false)||c.ambiguous.as_ref().is_some_and(|v|!v.is_empty())))||p.lod_strategy==LodStrategy::Choose||((model.lod_groups>1||geometry.source_triangles>500000)&&!p.review_lods)||(!unskinned_meshes.is_empty()&&!summary.bones.is_empty()&&!p.allow_unskinned);
            let input_hashes=self.fingerprint_inputs(p,&materials)?;
            for(path,sha)in &initial_inputs{if input_hashes.get(path)!=Some(sha){return Err(invalid(format!("Source input changed during analysis: {}",path.display())));}}
            if input_hashes.get(Path::new(&p.dmx))!=Some(&source_sha256){return Err(invalid("Replacement DMX changed during analysis."));}
            let effective_hashes=inventory(&effective,self.cancel)?;
            let report=InspectionReport{secondary_motion,generated_contract:None,project:p.name.clone(),hero_root:p.hero_root(),model,dmx:summary,geometry,source_materials,source_bones,source_identity,vertex_preflight,automatic_split,source_mappings,project_mappings,path_rebases,effective_sources:effective,mappings,unresolved_meshes,materials,warnings,unskinned_meshes,original,needs_review,source_sha256,project_sha256:project_hash(p)?,input_hashes,effective_hashes};
            self.cancel.check()?;safety::write_json(&root.join("inspection.json"),&report)?;
            for warning in &report.warnings{(self.log)(warning);}
            (self.log)(&format!("Replacement DMX parsed: {} meshes, {} materials, {} bones",names.len(),report.dmx.materials.len(),report.dmx.bones.len()));Ok(report)
        })
    }
    fn fingerprint_inputs(
        &self,
        p: &Project,
        plans: &deadlock_materials::Plans,
    ) -> Result<BTreeMap<PathBuf, String>> {
        let mut paths = vec![
            p.dmx.clone(),
            p.mesh_template.clone(),
            p.skeleton_reference.clone(),
            p.material_mapping.clone(),
            p.material_remap.clone(),
            p.preparation_report.clone(),
        ];
        for material in plans.values() {
            if let Some(textures) = &material.textures {
                paths.extend(textures.values().filter(|v| !v.is_empty()).cloned());
            }
        }
        if let Some(reference) = crate::motion::selection(p)? {
            paths.push(reference.to_string_lossy().into_owned());
        }
        let mut hashes = BTreeMap::new();
        for path in paths.into_iter().filter(|p| !p.is_empty()) {
            let path = PathBuf::from(path);
            deadlock_vpk::check_unlinked_path(&path, false).map_err(vpk_error)?;
            hashes.insert(
                path.clone(),
                safety::digest_with_cancel(&path, self.cancel)?,
            );
        }
        Ok(hashes)
    }
}
pub(crate) fn optional_path(value: &str) -> Option<&Path> {
    (!value.is_empty()).then(|| Path::new(value))
}
fn expand_mapping(mappings: &Mappings, report: &split::SplitReport) -> Result<Mappings> {
    let input = mappings
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                match v {
                    MeshMapping::One(s) => split::MeshMapping::One(s.clone()),
                    MeshMapping::Many(s) => split::MeshMapping::Many(s.clone()),
                },
            )
        })
        .collect();
    Ok(split::expand_mapping(&input, report)
        .map_err(map_error)?
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                match v {
                    split::MeshMapping::One(s) => MeshMapping::One(s),
                    split::MeshMapping::Many(s) => MeshMapping::Many(s),
                },
            )
        })
        .collect())
}
fn lod_warning(strategy: LodStrategy) -> &'static str {
    match strategy {
        LodStrategy::Reuse => {
            "Reusing full-detail geometry at lower LODs increases rendering cost."
        }
        LodStrategy::Choose => "Choose a LOD strategy before building.",
        LodStrategy::Single => {
            "Single LOD retains full detail at all distances; no simplification is performed."
        }
        LodStrategy::Preserve => {
            "Original lower LODs may restore original appearance at a distance."
        }
        LodStrategy::Disable => "Disabled lower LOD meshes may disappear at a distance.",
        LodStrategy::Custom => "Check every custom LOD mapping for appearance and performance.",
    }
}
