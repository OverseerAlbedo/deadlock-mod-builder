//! Fresh, isolated cloth sources for native review. Never feeds publication gates.
use crate::{
    inspect::{hash, map_error, ModelPipeline, OriginalInspection},
    invalid,
    prepare::copy_file_verified,
    sources,
    workspace::WorkspaceLease,
};
use deadlock_cloth_authoring::{automatic, inventory, physics, proxy_source, surface};
use deadlock_core::{safety, CoreError, Result};
use deadlock_dmx::Dmx;
use deadlock_model::{self as model, ag2};
use deadlock_project::{ModelInputMode, Project};
use deadlock_skeleton::{
    source_identity::{self, SourceContract},
    Skeleton,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
pub const POLICY: &str = "native-direct-cloth-workspace-v1";
fn author_error(error: deadlock_cloth_authoring::Error) -> CoreError {
    match error {
        deadlock_cloth_authoring::Error::Core(e) => e,
        deadlock_cloth_authoring::Error::Dmx(e) => e.into_core(),
        other => invalid(other.to_string()),
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct SkirtReport {
    #[serde(flatten)]
    pub analysis: physics::PhysicsAnalysis,
    pub kind: String,
    pub status: String,
    pub original_model: String,
    pub selected_meshes: BTreeSet<String>,
    pub original_solver: inventory::Inventory,
    pub native_compiler_run: bool,
    pub published: bool,
    pub installed: bool,
    pub analysis_fingerprint_algorithm: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ClothAnalysis {
    Skirt(SkirtReport),
    Automatic(automatic::AutomaticReport),
}
impl ClothAnalysis {
    pub fn source_sha256(&self) -> &str {
        match self {
            Self::Skirt(r) => &r.analysis.source_sha256,
            Self::Automatic(r) => &r.source_sha256,
        }
    }
    pub fn original_sha256(&self) -> &str {
        match self {
            Self::Skirt(r) => r.analysis.original_sha256.as_deref().unwrap_or_default(),
            Self::Automatic(r) => &r.original_sha256,
        }
    }
    fn verified(&self) -> bool {
        match self {
            Self::Skirt(r) => r.analysis.original_snapshot_verified == Some(true),
            Self::Automatic(r) => r.original_snapshot_verified,
        }
    }
    fn blockers(&self) -> &[String] {
        match self {
            Self::Skirt(r) => &r.analysis.blockers,
            Self::Automatic(r) => &r.blockers,
        }
    }
    fn warnings(&self) -> &[String] {
        match self {
            Self::Skirt(r) => &r.analysis.warnings,
            Self::Automatic(r) => &r.warnings,
        }
    }
    fn overlaps(&self) -> usize {
        match self {
            Self::Skirt(r) => r.analysis.summary.overlapping_position_count,
            Self::Automatic(r) => r.summary.overlapping_position_count,
        }
    }
    fn proxy(&self) -> proxy_source::Analysis {
        match self {
            Self::Skirt(r) => proxy_source::Analysis {
                units: r.analysis.units.clone(),
                millimeters_per_unit: r.analysis.millimeters_per_unit,
                collision_disabled: false,
                leg_colliders: r
                    .analysis
                    .leg_colliders
                    .iter()
                    .map(|c| proxy_source::ColliderProposal {
                        bone: c.bone.clone(),
                        tip_bone: c.tip_bone.clone(),
                        endpoints_local: c.endpoints_local,
                        endpoints_world: c.endpoints_world,
                        radius_local: c.radius_local,
                        radius_world: c.radius_world,
                    })
                    .collect(),
            },
            Self::Automatic(r) => proxy_source::Analysis {
                units: r.units.clone(),
                millimeters_per_unit: r.millimeters_per_unit,
                collision_disabled: r.collision_disabled,
                leg_colliders: r
                    .leg_colliders
                    .iter()
                    .map(|c| proxy_source::ColliderProposal {
                        bone: c.bone.clone(),
                        tip_bone: c.tip_bone.clone(),
                        endpoints_local: c.endpoints_local,
                        endpoints_world: c.endpoints_world,
                        radius_local: c.radius_local,
                        radius_world: c.radius_world,
                    })
                    .collect(),
            },
        }
    }
}
#[derive(Debug, Serialize)]
pub struct AnalysisResult {
    pub kind: String,
    pub analysis: ClothAnalysis,
    pub report_path: PathBuf,
}
struct Inputs {
    result: AnalysisResult,
    dmx: Dmx,
    original: OriginalInspection,
    skeleton: Skeleton,
    selection: surface::Selections,
    extension: Option<SourceContract>,
}
pub struct ClothAuthoringService<'a, 'b> {
    pub model: &'a ModelPipeline<'b>,
}
impl ClothAuthoringService<'_, '_> {
    /// Owns the current project lease. Saved analysis JSON is never trusted.
    pub fn analyze(&self, project: &Project) -> Result<AnalysisResult> {
        let root = self.model.root(project)?;
        let _lease = WorkspaceLease::acquire_for_runner(&root, self.model.runner)?;
        self.model
            .clock
            .measure("Analyze skirt physics", None, |_| {
                Ok(self.inspect_inputs(project, false)?.result)
            })
    }
    fn inspect_inputs(&self, project: &Project, automatic: bool) -> Result<Inputs> {
        if !project.customization.model_enabled() {
            return Err(invalid(
                "Enable the replacement model before analyzing cloth.",
            ));
        }
        let source = Path::new(&project.dmx);
        if !source.is_file()
            || source
                .extension()
                .and_then(|s| s.to_str())
                .is_none_or(|s| !s.eq_ignore_ascii_case("dmx"))
        {
            return Err(invalid(
                "Choose an existing game-ready rigged DMX before analyzing cloth.",
            ));
        }
        let root = self.model.root(project)?;
        let destination = root.join(if automatic {
            "automatic-cloth-analysis.json"
        } else {
            "skirt-physics-analysis.json"
        });
        safety::write_json(
            &destination,
            &serde_json::json!({"kind":if automatic{"automatic_cloth"}else{"skirt_analysis"},"status":"checking","runtime_ready":false,"published":false}),
        )?;
        let result = (|| {
            let dmx = self
                .model
                .clock
                .measure("Read cloth source DMX", None, |_| {
                    Dmx::load_with_cancel(source, self.model.cancel)
                        .map_err(deadlock_dmx::DmxError::into_core)
                })?;
            let sha = hash(dmx.bytes());
            let original = self.model.inspect_original(project, None)?;
            let audit = original.source_audit();
            let snapshot = original.compiled_snapshot.as_deref().ok_or_else(|| {
                invalid("Select the current original compiled model before setting up cloth.")
            })?;
            let (analysis, skeleton, selection, extension) = if automatic {
                let options = project
                    .customization
                    .cloth_setup
                    .clone()
                    .unwrap_or_default();
                let geometry = physics::original(
                    snapshot,
                    original.compiled_sha256.as_deref().unwrap_or_default(),
                    &audit,
                    self.model.cancel,
                )
                .map_err(author_error)?;
                let analysis = self.model.clock.measure(
                    "Select cloth surface and propose leg collision",
                    None,
                    |_| {
                        automatic::analyze(&dmx, &options, &geometry, &audit, self.model.cancel)
                            .map_err(author_error)
                    },
                )?;
                (
                    ClothAnalysis::Automatic(analysis.report),
                    analysis.skeleton,
                    analysis.selection,
                    None,
                )
            } else {
                if audit.runtime_motion.cloth_nodes == 0 {
                    return Err(invalid("Skirt analysis currently requires an original hero with audited existing cloth."));
                }
                let meshes: BTreeSet<_> = dmx
                    .elements
                    .iter()
                    .filter(|e| e.kind == "DmeMesh")
                    .map(|e| e.name.clone())
                    .collect();
                let selected = if project.meshes.is_empty() {
                    meshes.clone()
                } else {
                    model::selected_meshes(&project.meshes).map_err(map_error)?
                };
                if !selected.is_subset(&meshes) {
                    return Err(invalid(
                        "A selected replacement mesh is missing from the DMX; review its mapping.",
                    ));
                }
                let extension=self.model.clock.measure("Verify selected custom cloth bone source",None,|_|source_identity::source_contract(&dmx,&audit,&selected,&sha).map_err(map_error))?.ok_or_else(||invalid("No selected weighted custom bone branches were found with a verified original skeleton. Choose the rigged DMX retaining its skirt bones."))?;
                let report = self.model.clock.measure(
                    "Analyze skirt bone chains and leg collision",
                    None,
                    |_| {
                        physics::analyze(
                            &dmx,
                            &audit,
                            Some(snapshot),
                            Some(&extension),
                            &physics::AnalysisOptions::default(),
                            self.model.cancel,
                        )
                        .map_err(author_error)
                    },
                )?;
                let raw = original.compiled_bytes(self.model.cancel)?;
                let solver =
                    self.model
                        .clock
                        .measure("Inventory original cloth solver", None, |_| {
                            inventory::solver_inventory(&raw).map_err(author_error)
                        })?;
                let mut report = SkirtReport {
                    analysis: report,
                    kind: "skirt_analysis".into(),
                    status: "analyzed".into(),
                    original_model: project.original_model.clone(),
                    selected_meshes: selected,
                    original_solver: solver,
                    native_compiler_run: false,
                    published: false,
                    installed: false,
                    analysis_fingerprint_algorithm: "rust-sorted-json-v1-sha256".into(),
                };
                report.analysis.analysis_sha256 = None;
                let mut value = serde_json::to_value(&report)?;
                value.as_object_mut().unwrap().remove("analysis_sha256");
                report.analysis.analysis_sha256 = Some(hash(&serde_json::to_vec(&value)?));
                (
                    ClothAnalysis::Skirt(report),
                    extension.skeleton.clone(),
                    BTreeMap::new(),
                    Some(extension),
                )
            };
            self.fresh(source, snapshot, &analysis)?;
            safety::write_json(&destination, &analysis)?;
            for warning in analysis.warnings() {
                (self.model.log)(&format!("Cloth setup review: {warning}"));
            }
            for blocker in analysis.blockers() {
                (self.model.log)(&format!("Cloth physics needs validation: {blocker}"));
            }
            (self.model.log)(&format!(
                "Cloth setup analysis saved: {}",
                destination.display()
            ));
            Ok(Inputs {
                result: AnalysisResult {
                    kind: if automatic {
                        "automatic_cloth".into()
                    } else {
                        "skirt_analysis".into()
                    },
                    analysis,
                    report_path: destination.clone(),
                },
                dmx,
                original,
                skeleton,
                selection,
                extension,
            })
        })();
        if let Err(error) = &result {
            let _ = safety::write_json(
                &destination,
                &serde_json::json!({"kind":if automatic{"automatic_cloth"}else{"skirt_analysis"},"status":if matches!(error,CoreError::Cancelled){"cancelled"}else{"failed"},"runtime_ready":false,"published":false,"error":error.to_string()}),
            );
        }
        result
    }
    fn fresh(&self, source: &Path, original: &Path, analysis: &ClothAnalysis) -> Result<()> {
        self.model.cancel.check()?;
        if safety::digest_with_cancel(source, self.model.cancel)? != analysis.source_sha256() {
            return Err(invalid(
                "Cloth source changed during analysis; create the workspace again.",
            ));
        }
        if safety::digest_with_cancel(original, self.model.cancel)? != analysis.original_sha256() {
            return Err(invalid(
                "Original hero changed during cloth analysis; analyze again.",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Serialize)]
pub struct PreviewCopy {
    pub path: PathBuf,
    pub relative: String,
    pub sha256: String,
    pub removed_ag2_nodes: Vec<ag2::Reference>,
    pub source_references: Vec<String>,
    pub material_references: BTreeSet<String>,
    pub missing_staged_references: BTreeSet<String>,
}
#[derive(Debug, Serialize)]
pub struct GeometryPreviews {
    pub scope: String,
    pub runtime_ready: bool,
    pub native_compiled: bool,
    pub ag2_compatibility_verified: bool,
    pub original_solver_preservation_verified: bool,
    pub full_authored_model_changed: bool,
    pub dependency_scope: String,
    pub before: PreviewCopy,
    pub cloth: PreviewCopy,
    pub missing_staged_references: BTreeSet<String>,
}
impl ClothAuthoringService<'_, '_> {
    fn preview_copies(
        &self,
        content: &Path,
        directory: &str,
        before: &str,
        authored: &str,
        materials: BTreeSet<String>,
    ) -> Result<GeometryPreviews> {
        let make = |filename: &str, text: &str| -> Result<PreviewCopy> {
            self.model.cancel.check()?;
            let relative = format!("{directory}/{filename}");
            let destination = safety::inside(content, &relative)?;
            if destination.exists() {
                return Err(invalid("Cloth preview would replace an existing asset."));
            }
            let removed = ag2::source_nodes(text).map_err(map_error)?;
            let preview = ag2::preview_source(text).map_err(map_error)?;
            let references = model::source_references(&preview).map_err(map_error)?;
            let all: BTreeSet<_> = references
                .iter()
                .cloned()
                .chain(materials.iter().cloned())
                .collect();
            let mut missing = BTreeSet::new();
            for reference in all {
                if !safety::inside(content, &reference)?.is_file() {
                    missing.insert(reference);
                }
            }
            safety::atomic_write(&destination, preview.as_bytes())?;
            Ok(PreviewCopy {
                path: destination,
                relative,
                sha256: hash(preview.as_bytes()),
                removed_ag2_nodes: removed,
                source_references: references,
                material_references: materials.clone(),
                missing_staged_references: missing,
            })
        };
        let before = make("before_cloth_preview.vmdl", before)?;
        let cloth = make("cloth_preview.vmdl", authored)?;
        let missing = before
            .missing_staged_references
            .union(&cloth.missing_staged_references)
            .cloned()
            .collect::<BTreeSet<_>>();
        if !missing.is_empty() {
            (self.model.log)(&format!(
                "Cloth preview has unstaged dependencies retained for review:\n{}",
                missing.iter().cloned().collect::<Vec<_>>().join("\n")
            ));
        }
        Ok(GeometryPreviews{scope:"geometry_and_cloth_only".into(),runtime_ready:false,native_compiled:false,ag2_compatibility_verified:false,original_solver_preservation_verified:false,full_authored_model_changed:false,dependency_scope:"Direct ModelDoc references and staged material sources; not a recursive compiled-resource dependency audit.".into(),before,cloth,missing_staged_references:missing})
    }
}
#[derive(Debug, Serialize)]
pub struct AuthoringReceipt {
    pub kind: String,
    pub policy: String,
    pub status: String,
    pub parent_project: String,
    pub workspace_project: String,
    pub original_model: String,
    pub source_sha256: String,
    pub original_sha256: String,
    pub runtime_ready: bool,
    pub native_compiler_run: bool,
    pub build_authorized: bool,
    pub published: bool,
    pub installed: bool,
    pub source_modified: bool,
    pub project_modified: bool,
    pub automatic_surface_setup: bool,
    pub excluded_non_model_extras: BTreeMap<String, usize>,
    pub analysis: ClothAnalysis,
    pub surface: surface::SurfaceReceipt,
    pub model_source: proxy_source::Receipt,
    pub workspace_path: PathBuf,
    pub model_path: PathBuf,
    pub geometry_previews: GeometryPreviews,
    pub preview_model_path: PathBuf,
    pub preview_before_model_path: PathBuf,
    pub report_path: PathBuf,
    pub content_path: PathBuf,
    pub game_path: PathBuf,
    pub model_relative: String,
    pub original_reference: String,
    pub original_solver_preservation_verified: bool,
    pub collider_fit_approved: bool,
    pub automatic_split: Option<deadlock_dmx::split::SplitReport>,
    pub pending_gates: Vec<String>,
    pub files: BTreeMap<String, String>,
    pub receipt_sha256: String,
    pub receipt_fingerprint_algorithm: String,
}
impl ClothAuthoringService<'_, '_> {
    /// Creates a separate unpublishable project root, retaining active inputs.
    pub fn create_workspace(&self, project: &Project, automatic: bool) -> Result<AuthoringReceipt> {
        project.validate_action().map_err(map_error)?;
        if !project.customization.model_enabled() {
            return Err(invalid(
                "Enable the replacement model before creating a cloth workspace.",
            ));
        }
        if project.model_input_mode != ModelInputMode::Prepared {
            return Err(invalid("Cloth authoring needs a game-ready rigged DMX."));
        }
        if project.customization.active()
            && project
                .customization
                .clothing
                .as_ref()
                .is_some_and(|v| v.enabled == Some(true))
        {
            return Err(invalid(
                "Use a DMX that includes the garment, then disable Include an added garment.",
            ));
        }
        let parent_root = self.model.root(project)?;
        let _parent_lease = WorkspaceLease::acquire_for_runner(&parent_root, self.model.runner)?;
        self.model.clock.measure("Create cloth review workspace",None,|span|{
            let mut inputs=self.inspect_inputs(project,automatic)?;
            let source=inputs.original.verified_source(self.model.cancel)?;
            proxy_source::supported_source_format(&source).map_err(author_error)?;
            if let (ClothAnalysis::Skirt(report),Some(extension))=(&inputs.result.analysis,&inputs.extension){
                inputs.selection=surface::selection_from_analysis(&inputs.dmx,&report.analysis,extension).map_err(author_error)?;
            }
            let analysis=&inputs.result.analysis;let proxy_analysis=analysis.proxy();
            if !analysis.verified() || (proxy_analysis.leg_colliders.is_empty() && !(automatic && proxy_analysis.collision_disabled)){return Err(invalid("Cloth authoring needs a verified original and measured leg colliders; review analysis."))}
            let blocking=analysis.blockers().iter().filter(|reason|!reason.starts_with("Proposed leg capsules overlap ")).cloned().collect::<Vec<_>>();
            if !blocking.is_empty(){return Err(invalid(format!("Cloth setup needs attention:\n{}",blocking.join("\n"))))}
            let original_path=inputs.original.compiled_snapshot.as_deref().ok_or_else(||invalid("Original snapshot is required for cloth authoring."))?;
            self.fresh(Path::new(&project.dmx),original_path,analysis)?;
            let mut draft=project.clone();draft.id=uuid::Uuid::new_v4().simple().to_string();draft.install=false;draft.build_vpk=false;draft.compile_model=false;
            if let Some(physics)=&mut draft.customization.physics{physics.enabled=Some(false);}
            if let Some(motion)=&mut draft.customization.secondary_motion{motion.enabled=Some(false);}
            let mut excluded=BTreeMap::new();
            for (kind,rows) in [("icons",&mut draft.customization.icons),("animations",&mut draft.customization.animations),("effects",&mut draft.customization.effects),("audio",&mut draft.customization.audio)]{
                let count=if project.customization.active(){rows.as_deref().unwrap_or_default().iter().filter(|v|v.enabled.unwrap_or(true)).count()}else{0};excluded.insert(kind.to_string(),count);*rows=Some(vec![]);
            }
            draft.imported_source=self.model.sources().ensure_base(project)?.to_str().ok_or_else(||invalid("Original source path is not Unicode."))?.to_string();
            draft.original_compiled=original_path.to_str().ok_or_else(||invalid("Original snapshot path is not Unicode."))?.to_string();
            let root=self.model.root(&draft)?;let _draft_lease=WorkspaceLease::acquire_for_runner(&root, self.model.runner)?;let report_path=root.join("cloth-authoring.json");
            let mut initial=serde_json::json!({"kind":"skirt_authoring","policy":POLICY,"status":"authoring","parent_project":project.id,"workspace_project":draft.id,"original_model":project.original_model,"source_sha256":analysis.source_sha256(),"original_sha256":analysis.original_sha256(),"runtime_ready":false,"native_compiler_run":false,"build_authorized":false,"published":false,"installed":false,"source_modified":false,"project_modified":false,"automatic_surface_setup":automatic,"excluded_non_model_extras":excluded});
            safety::write_json(&report_path,&initial)?;
            let result=(||{
                let surface=self.model.clock.measure("Author garment cloth mask and surface proxy",Some(span.id()),|_|surface::author_direct_cloth(&inputs.dmx,&inputs.selection,&root.join("cloth-inputs"),analysis.source_sha256(),inputs.original.runtime().map_or(0,|r|r.cloth_nodes),self.model.cancel,Some(&deadlock_cloth_authoring::motion::MotionProfile::default())).map_err(author_error))?;
                draft.dmx=surface.render_path.to_str().ok_or_else(||invalid("Authored DMX path is not Unicode."))?.to_string();draft.expected_dmx_sha256=surface.render_sha256.clone();
                let inspection=self.model.inspect(&mut draft)?;
                if inspection.needs_review{return Err(invalid("Review mesh mappings, materials and LOD choices with Analyze before creating the cloth workspace."))}
                if inspection.original.compiled_sha256.as_deref()!=Some(analysis.original_sha256()){return Err(invalid("Original hero changed before cloth source assembly."))}
                let prepared=self.model.prepare(&mut draft,&inspection,true)?;
                let directory=format!("models/builder_cloth/{}",&draft.id[..8]);let asset=format!("{directory}/proxy.dmx");
                copy_file_verified(&surface.proxy_path,&safety::inside(&prepared.content,&asset)?,self.model.cancel)?;
                let model_path=safety::inside(&prepared.content,draft.source_model())?;let before=sources::read_text(&model_path)?;
                let selected=model::selected_meshes(&inspection.source_mappings).map_err(map_error)?;
                if inputs.selection.keys().any(|name|!selected.contains(name)){return Err(invalid("A chosen cloth mesh is excluded by replacement mapping. Include it in Analyze first."))}
                let (authored,source_receipt)=self.model.clock.measure("Author native proxy and collider source nodes",Some(span.id()),|_|proxy_source::author_direct_proxy(&before,&asset,&proxy_analysis,&inputs.skeleton,"builder_skirt",automatic && proxy_analysis.collision_disabled).map_err(author_error))?;
                safety::atomic_write(&root.join("reference-before-cloth.vmdl"),before.as_bytes())?;safety::atomic_write(&model_path,authored.as_bytes())?;
                let previews=self.model.clock.measure("Create paired native geometry previews",Some(span.id()),|_|self.preview_copies(&prepared.content,&directory,&before,&authored,inspection.materials.keys().cloned().collect()))?;
                self.fresh(Path::new(&project.dmx),original_path,analysis)?;
                if safety::digest_with_cancel(&surface.proxy_path,self.model.cancel)?!=surface.proxy_sha256 || safety::digest_with_cancel(&surface.render_path,self.model.cancel)?!=surface.render_sha256{return Err(invalid("Authored cloth DMX changed during assembly."))}
                copy_file_verified(original_path,&root.join("reference-original.vmdl_c"),self.model.cancel)?;
                if safety::digest_with_cancel(&root.join("reference-original.vmdl_c"),self.model.cancel)?!=analysis.original_sha256(){return Err(invalid("Original hero changed while its reference was copied."))}
                safety::write_json(&root.join("skirt-analysis.json"),analysis)?;
                safety::atomic_write(&root.join("START_HERE.md"),guide(project,draft.source_model(),analysis,&previews).as_bytes())?;
                let files=self.model.clock.measure("Fingerprint cloth review workspace",Some(span.id()),|_|{
                    let mut result=BTreeMap::new();for file in sources::files(&root,self.model.cancel)?{
                        // The operation lease is ephemeral, never an authored input.
                        if file==report_path || file==root.join("build.lock"){continue;}
                        let relative=file.strip_prefix(&root).map_err(map_error)?.to_str().ok_or_else(||invalid("Cloth workspace filename is not Unicode."))?.replace('\\',"/");result.insert(relative,safety::digest_with_cancel(&file,self.model.cancel)?);
                    }Ok(result)
                })?;
                self.fresh(Path::new(&project.dmx),original_path,analysis)?;
                let mut receipt=AuthoringReceipt{kind:"skirt_authoring".into(),policy:POLICY.into(),status:"authored_for_native_review".into(),parent_project:project.id.clone(),workspace_project:draft.id.clone(),original_model:project.original_model.clone(),source_sha256:analysis.source_sha256().into(),original_sha256:analysis.original_sha256().into(),runtime_ready:false,native_compiler_run:false,build_authorized:false,published:false,installed:false,source_modified:false,project_modified:false,automatic_surface_setup:automatic,excluded_non_model_extras:excluded,analysis:analysis.clone(),surface,model_source:source_receipt,workspace_path:root.clone(),model_path,preview_model_path:previews.cloth.path.clone(),preview_before_model_path:previews.before.path.clone(),geometry_previews:previews,report_path:report_path.clone(),content_path:prepared.content,game_path:prepared.game,model_relative:draft.source_model().into(),original_reference:"reference-original.vmdl_c".into(),original_solver_preservation_verified:false,collider_fit_approved:false,automatic_split:inspection.automatic_split,pending_gates:vec!["Native compile and generated-bone/render-binding audit.".into(),"Animated leg capsule fit and visible garment contact.".into(),"Combined original hair and new cloth solver preservation.".into(),"Deadlock in-game validation.".into()],files,receipt_sha256:String::new(),receipt_fingerprint_algorithm:"rust-sorted-json-v1-sha256".into()};
                let mut value=serde_json::to_value(&receipt)?;value.as_object_mut().unwrap().remove("receipt_sha256");receipt.receipt_sha256=hash(&serde_json::to_vec(&value)?);self.model.cancel.check()?;safety::write_json(&report_path,&receipt)?;
                (self.model.log)(&format!("Cloth review workspace created: {}",root.display()));(self.model.log)("Native validation required. Original hair/new cloth coexistence is not yet verified; no VPK was published.");Ok(receipt)
            })();
            if let Err(error)=&result{initial["status"]=serde_json::json!(if matches!(error,CoreError::Cancelled){"cancelled"}else{"failed"});initial["error"]=serde_json::json!(error.to_string());let _=safety::write_json(&report_path,&initial);}
            result
        })
    }
}
fn guide(
    project: &Project,
    model: &str,
    analysis: &ClothAnalysis,
    previews: &GeometryPreviews,
) -> String {
    let collision = if analysis.proxy().leg_colliders.is_empty() {
        "**no body collision** (the garment can pass through the body)"
    } else {
        "bone-bound leg capsule proposals"
    };
    let automatic=match analysis{ClothAnalysis::Automatic(report)=>format!("\nAutomatic setup selected the **{} 10% attachment band**. Pins retain their existing character weights; the remaining selected surface receives a cloth mask. This candidate uses native generated controls, not the accepted R12 fitted-bone solver. Normal Build does not consume this workspace automatically.\n",report.options.attachment.as_deref().unwrap_or("top")),_=>String::new()};
    let missing = if previews.missing_staged_references.is_empty() {
        "Direct references and material sources are staged. Native compilation may still need additional toolset or nested resources.".into()
    } else {
        format!(
            "Resolve these retained dependencies in the test content root:\n\n{}",
            previews
                .missing_staged_references
                .iter()
                .map(|s| format!("- `{s}`"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    format!("# Cloth test workspace\n\nThis workspace contains **{}** with a generated cloth proxy, an explicit render cloth mask and {collision}. It is a native authoring test, not an installable mod.\n{automatic}\nThe proxy includes explicit animation-goal, drag and contraction-limit maps. Support is stronger near anchors and weaker down each physical panel. Native output must prove these maps reached the solver; animation playback must qualify the result. Original geometry, weights, pins and openings remain unchanged.\n\n1. Copy `prepared-content` into a **separate test content root** mounted by your ModelDoc toolset. Retain relative paths.\n2. Open `{}` and compile the **before-cloth baseline**. Check geometry and legacy animation.\n3. Open `{}` and compile the **cloth preview**. The cloth nodes are already authored. Enable simulation and compare the waist anchor, both legs, rapid motion and rest pose in the same animation. Do not add an old strip fixture or change binding switches.\n4. Review capsule position and radius proposals. Analysis found **{} overlapping garment positions at rest**. Whole-triangle contact and animated fit remain unverified.\n5. Keep both preview sources, resulting VMDL_C files and compiler logs together with this report.\n\nBoth preview copies omit external AG2 references for geometry testing and retain legacy animations. They do not qualify the game's AG2 animation behavior. Full authored source `{model}` retains its AG2 references and is never replaced by a preview in normal Build.\n\n{missing}\n\nThe source may omit the original compiled hair solver. Successful cloth compilation does not establish combined hair preservation or AG2 compatibility. Normal Build still refuses an unvalidated changed solver. Do not install this candidate or copy its PHYS over the original.\n\nDirect binding lets the native compiler assign generated cloth bones to enabled positions; masks exclude body, equipment and pins. Proxy subdivision preserves rest surfaces without making the render mesh denser or promising no clipping.\n\nIcons, replacement animation clips, effects and audio are excluded. Original hero animation definitions and model appearance/equipment choices remain. Active inputs and the project are unchanged.\n\nThis operation ran no native compiler, published no VPK and installed nothing.\n",if project.hero_name.is_empty(){&project.name}else{&project.hero_name},previews.before.relative,previews.cloth.relative,analysis.overlaps())
}
