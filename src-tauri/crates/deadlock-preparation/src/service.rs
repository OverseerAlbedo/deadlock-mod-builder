//! Hash-bound import, prepare, correction, resume and handoff workflow.
use crate::{
    Result,
    algorithms::{self, Bone, Suggestions},
    blender::{BlenderBridge, Request},
    digest, invalid, path_text,
    provenance::{self, ClothingSelection, GeometryRecord, PropRecord, PropSelection},
    source, write_json, secondary_bones,
};
use deadlock_project::{MaterialChoices, ModelInputMode, PreparationOptions, Project, asset_paths};
use deadlock_skeleton::{
    Skeleton,
    export::{self, PreparedReport},
    transforms::{self, PreparedBone, ScaleAudit},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub const REST_POLICY: &str = "anatomical-rest-alignment-v1";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshInfo {
    pub name: String,
    pub vertices: u64,
    pub polygons: u64,
    pub materials: Vec<String>,
    pub disconnected_components: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Morph {
    pub name: String,
    pub mesh: String,
    pub shape: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Inspection {
    pub armatures: u32,
    pub bones: Vec<Bone>,
    pub vertices: u64,
    pub bounds: [[f64; 3]; 2],
    pub meshes: Vec<MeshInfo>,
    pub morphs: Vec<Morph>,
    pub weighted_bones: Vec<String>,
    pub suggested_mode: ModelInputMode,
    pub humanoid: serde_json::Value,
    #[serde(default)]
    pub source_helpers: Vec<serde_json::Value>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub status: String,
    /// Which scene objects were kept, set aside or repaired, and why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<serde_json::Value>,
    /// Objects in the source that look like a held gun.
    #[serde(default)]
    pub weapons: Vec<crate::weapon::Candidate>,
    /// Result of adding breast bones for breast motion, when requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breast_bones: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInput {
    pub received: String,
    pub resolved: String,
    pub cwd: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClothingRecord {
    pub selection: ClothingSelection,
    pub source: String,
    pub source_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analysis {
    pub source: String,
    pub source_sha256: String,
    pub source_original: String,
    pub source_input: SourceInput,
    pub root: String,
    pub target_model: String,
    pub target_sha256: String,
    pub target_skeleton: Skeleton,
    pub target: Vec<PreparedBone>,
    pub target_scale_audit: ScaleAudit,
    pub analysis: Inspection,
    pub suggestions: Suggestions,
    pub rotation: [f64; 3],
    #[serde(default)]
    pub geometry_customization: GeometryRecord,
    pub clothing: Option<ClothingRecord>,
    /// The original model's gun (hero gun meshes, weapon bones, or a weapon model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_reference: Option<crate::weapon::Reference>,
    /// Automatic gun choice for a fresh inspection (the user can change it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_suggestion: Option<crate::weapon::Choice>,
    /// Breast bones added in Blender; conversion repeats this request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breast_generation: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyAudit {
    pub policy: String,
    #[serde(flatten)]
    pub values: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedResponse {
    pub status: String,
    pub meshes: Vec<export::PreparedMesh>,
    pub materials: BTreeMap<String, MaterialChoices>,
    pub warnings: Vec<String>,
    pub blend: String,
    pub target: Vec<PreparedBone>,
    pub rest_alignment: PolicyAudit,
    #[serde(flatten)]
    pub algorithm_audits: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReceipt {
    pub target_scale_audit: ScaleAudit,
    #[serde(flatten)]
    pub audits: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    #[serde(flatten)]
    pub prepared: PreparedResponse,
    pub export_validation: ValidationReceipt,
    pub target_model: String,
    pub target_sha256: String,
    pub analysis_path: String,
    pub options: PreparationOptions,
    pub dmx_sha256: String,
    pub blender_operation_completed: bool,
    pub native_compiler_validated: bool,
    pub in_game_validated: bool,
    pub clothing_selection: Option<ClothingSelection>,
    #[serde(default)]
    pub geometry_customization: GeometryRecord,
    #[serde(default)]
    pub attached_prop_meshes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparationResult {
    pub dmx: String,
    pub blend: String,
    pub report: String,
    pub textures: String,
    pub target_model: String,
    pub warnings: Vec<String>,
}
fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn same_hash(path: &Path, expected: &str, message: &str) -> Result<()> {
    if digest(path)? != expected {
        return Err(invalid(message));
    }
    Ok(())
}
/// The application supplies an already extracted, validated original skeleton
/// and its compiled file. This service never chooses a different hero implicitly.
#[derive(Clone)]
pub struct PreparationTimings {
    pub recorder: deadlock_core::TimingRecorder,
    pub parent_id: Option<String>,
}
pub struct PreparationService {
    pub project_root: PathBuf,
    pub bridge: BlenderBridge,
    pub timing: Option<PreparationTimings>,
    pub cancel: deadlock_core::CancellationToken,
    /// Measured from the decompiled original model by the application.
    pub weapon_reference: Option<crate::weapon::Reference>,
}
impl PreparationService {
    pub fn new(project_root: PathBuf, bridge: BlenderBridge) -> Self {
        Self {
            project_root,
            bridge,
            timing: None,
            cancel: deadlock_core::CancellationToken::new(),
            weapon_reference: None,
        }
    }
    pub fn with_weapon_reference(mut self, reference: Option<crate::weapon::Reference>) -> Self {
        self.weapon_reference = reference;
        self
    }
    pub fn with_cancel(mut self, cancel: deadlock_core::CancellationToken) -> Self {
        self.cancel = cancel;
        self
    }
    pub fn with_timing(
        mut self,
        recorder: deadlock_core::TimingRecorder,
        parent_id: Option<String>,
    ) -> Self {
        self.timing = Some(PreparationTimings {
            recorder,
            parent_id,
        });
        self
    }
    fn step<T>(&self, name: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        self.cancel.check()?;
        let operation = || {
            let result = operation()?;
            self.cancel.check()?;
            Ok(result)
        };
        let Some(timing) = &self.timing else {
            return operation();
        };
        let guard = timing
            .recorder
            .start_step(name, timing.parent_id.as_deref())?;
        match operation() {
            Ok(value) => {
                guard.success()?;
                Ok(value)
            }
            Err(error) => {
                let status = if matches!(
                    error,
                    crate::Error::Core(deadlock_core::CoreError::Cancelled)
                ) {
                    deadlock_core::TimingStatus::Cancelled
                } else {
                    deadlock_core::TimingStatus::Failed
                };
                guard.finish(status, Some(error.to_string()))?;
                Err(error)
            }
        }
    }

    pub fn inspect(
        &self,
        project: &Project,
        input: &str,
        rotation: [f64; 3],
        compiled: &Path,
        skeleton: Skeleton,
    ) -> Result<Analysis> {
        project.validate_action()?;
        let source = self.step("Validate preparation source", || {
            source::resolve_source(input)
        })?;
        let geometry = provenance::customization_selection(project)?;
        let clothing = provenance::clothing_selection(project)?;
        let garment = if let Some(c) = &clothing {
            if c.mode != "weighted" {
                return Err(invalid(
                    "New simulated cloth requires separately validated Source 2 simulation data. Choose clothing that follows the body.",
                ));
            }
            let garment = source::resolve_source(&c.source)?;
            if garment == source {
                return Err(invalid(
                    "The clothing file is the same as the character source. Turn off Add clothing if it is already included.",
                ));
            }
            Some(garment)
        } else {
            None
        };
        self.step("Verify Blender launch", || {
            crate::blender::validate(&self.bridge.executable, self.bridge.executor.as_ref())
        })?;
        let root = asset_paths::resolve(&self.project_root)?
            .join("humanoid_preparation")
            .join(uuid::Uuid::new_v4().simple().to_string());
        let local = self.step("Stage character source and dependencies", || {
            source::stage_source_with_cancel(
                input,
                &root.join("source"),
                Some(&self.bridge),
                &self.cancel,
            )
        })?;
        let clothing = if let (Some(mut selection), Some(garment)) = (clothing, garment) {
            let local = self.step("Stage fitted garment and dependencies", || {
                source::stage_source_with_cancel(
                    &path_text(&garment)?,
                    &root.join("clothing"),
                    Some(&self.bridge),
                    &self.cancel,
                )
            })?;
            selection.source_files = provenance::source_proof(&garment, &local)?;
            Some(ClothingRecord {
                selection,
                source: path_text(&local)?,
                source_sha256: digest(&local)?,
            })
        } else {
            None
        };
        let (target, target_scale_audit) = self.step("Derive exact target binds", || {
            transforms::prepare_target(&skeleton).map_err(|e| invalid(e.to_string()))
        })?;
        let mut props = Vec::new();
        for (index, prop) in geometry.props.into_iter().enumerate() {
            if !target
                .iter()
                .any(|b| Some(b.name.as_str()) == prop.bone.as_deref())
            {
                return Err(invalid("Choose an existing target bone for the prop."));
            }
            let prop_source = source::resolve_source(prop.source.as_deref().unwrap_or_default())?;
            if prop_source == source {
                return Err(invalid(
                    "The attached prop source is the whole character. Choose a separate rigid prop file.",
                ));
            }
            let local = self.step(
                &format!("Stage rigid prop {} and dependencies", index + 1),
                || {
                    source::stage_source_with_cancel(
                        &path_text(&prop_source)?,
                        &root.join("props").join(index.to_string()),
                        Some(&self.bridge),
                        &self.cancel,
                    )
                },
            )?;
            let info: Inspection = self.step(
                &format!("Inspect rigid prop {} in Blender", index + 1),
                || {
                    self.bridge.inspect(
                        &local,
                        &local.parent().unwrap().join("analysis"),
                        [0.; 3],
                        None,
                        None,
                        None,
                    )
                },
            )?;
            if !info.bones.is_empty() || !info.morphs.is_empty() {
                return Err(invalid(
                    "An attached prop must be rigid geometry without its own skeleton or shape keys.",
                ));
            }
            props.push(PropRecord {
                selection: PropSelection {
                    choice: prop,
                    mode: Some("rigid".into()),
                    source_files: provenance::source_proof(&prop_source, &local)?,
                },
                source: path_text(&local)?,
                source_sha256: digest(&local)?,
                analysis: info,
            });
        }
        let scene = project.preparation_options.values.get("scene").cloned();
        let mut rotation = rotation;
        let mut info: Inspection = self.step("Inspect source geometry in Blender", || {
            self.bridge.inspect(
                &local,
                &root.join("analysis"),
                rotation,
                clothing.as_ref().map(|c| Path::new(&c.source)),
                scene.clone(),
                None,
            )
        })?;
        // Sources exported lying down (Y-up without axis conversion, or on
        // their side) are turned upright automatically when the user did not
        // choose a rotation. Recognition is repeated on the turned source.
        if rotation == [0.; 3] && !info.bones.is_empty() {
            let named = crate::automap::roles(&info.bones, &info.weighted_bones, &Default::default())?;
            if let Some(turn) = crate::structure::orientation(&info.bones, &info.weighted_bones, &named.roles) {
                rotation = turn.rotation;
                info = self.step("Inspect upright source geometry in Blender", || {
                    self.bridge.inspect(
                        &local,
                        &root.join("analysis-upright"),
                        rotation,
                        clothing.as_ref().map(|c| Path::new(&c.source)),
                        scene.clone(),
                        None,
                    )
                })?;
                info.warnings.push(format!(
                    "The source was not standing upright, so it was rotated {}° (X, Y, Z) before inspection. Check the mapping preview; set Source orientation yourself to override this.",
                    rotation.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(", ")
                ));
            }
        }
        // Breast motion on a rig without breast bones: Blender adds them where
        // the chest has breasts (a flat chest gets none), and conversion repeats
        // the same request. The user's file is never changed.
        let mut breast_generation = None;
        if project.preparation_options.values.get("breast_motion").and_then(|v| v.as_bool()) == Some(true)
            && !info.bones.is_empty()
        {
            let named = crate::automap::roles(&info.bones, &info.weighted_bones, &Default::default())?;
            let existing = secondary_bones::breast_named_bones(&info.bones, &info.weighted_bones);
            if secondary_bones::breast_bones(&info.bones, &info.weighted_bones, &named.roles).is_empty() && !existing.is_empty() {
                // Breast bones that carry other parts (a ribbon or cape strip) are
                // not single breasts, but the rig has them: never add more.
                info.breast_bones = Some(serde_json::json!({
                    "policy": "generated-breast-bones-v1", "status": "present",
                    "reason": format!("this rig already has breast bones ({}).", existing.join(", ")),
                }));
            } else if secondary_bones::breast_bones(&info.bones, &info.weighted_bones, &named.roles).is_empty() {
                if let Some(request) = secondary_bones::breast_request(&info.bones, &named.roles) {
                    let added: Inspection = self.step("Add breast bones in Blender", || {
                        self.bridge.inspect(
                            &local,
                            &root.join("analysis-breasts"),
                            rotation,
                            clothing.as_ref().map(|c| Path::new(&c.source)),
                            scene.clone(),
                            Some(&request),
                        )
                    })?;
                    let receipt = added.breast_bones.clone().unwrap_or_default();
                    if receipt.get("status").and_then(|v| v.as_str()) == Some("added") {
                        let warnings = info.warnings.clone();
                        info = added;
                        info.warnings = warnings;
                        info.warnings.push("Breast motion: added two breast bones where the chest has breasts. Check the bounce in game.".into());
                        breast_generation = Some(request);
                    } else {
                        info.breast_bones = Some(receipt.clone());
                        info.warnings.push(format!(
                            "Breast motion: no breast bones were added. {}",
                            receipt.get("reason").and_then(|v| v.as_str()).unwrap_or("The chest shape could not be measured.")
                        ));
                    }
                }
            }
        }
        let bones: Vec<_> = target.iter().map(Bone::from).collect();
        let mut complete = PreparationOptions::default();
        complete.values.insert("complete_mapping".into(), true.into());
        let suggestions = crate::automap::suggest(&info.bones, &info.weighted_bones, &bones, &complete)?;
        if project.model_input_mode != ModelInputMode::Weapon {
            // Only objects that plausibly are a held gun (named, hand-held or on a
            // weapon bone) are offered; rigid hairpins and badges are not guns.
            let roles = suggestions.source.as_ref().map(|s| s.roles.clone()).unwrap_or_default();
            info.weapons.retain(|c| crate::weapon::score(c, &info.bones, &roles) >= 2.5);
        }
        let analysis = Analysis {
            source: path_text(&local)?,
            source_sha256: digest(&local)?,
            source_original: path_text(&source)?,
            source_input: SourceInput {
                received: input.into(),
                resolved: path_text(&source)?,
                cwd: path_text(&std::env::current_dir()?)?,
            },
            root: path_text(&root)?,
            target_model: project.original_model.clone(),
            target_sha256: digest(compiled)?,
            target_skeleton: skeleton,
            target,
            target_scale_audit,
            analysis: info,
            suggestions,
            rotation,
            geometry_customization: GeometryRecord {
                props,
                face: geometry.face,
            },
            clothing,
            weapon_reference: self.weapon_reference.clone(),
            weapon_suggestion: None,
            breast_generation,
        };
        let mut analysis = analysis;
        analysis.weapon_suggestion = weapon_suggestion(project, &analysis);
        self.step("Save preparation analysis", || {
            write_json(&root.join("analysis.json"), &analysis)
        })?;
        Ok(analysis)
    }
    pub fn prepare(
        &self,
        project: &Project,
        analysis: &Analysis,
        mut options: PreparationOptions,
        correction: bool,
        compiled: &Path,
    ) -> Result<PreparationResult> {
        self.step("Validate humanoid review", || {
            validate_review(project, analysis, &options)
        })?;
        let original_options = options.clone();
        let preservation = secondary_bones::selection(&analysis.analysis.bones, &analysis.analysis.weighted_bones,
            &analysis.target.iter().map(Bone::from).collect::<Vec<_>>(), &options)?;
        let mut prepared_target = analysis.target.clone();
        let (root, allowed) = self.step("Verify preparation input provenance", || {
            let root = asset_paths::resolve(Path::new(&analysis.root))?;
            let allowed = asset_paths::resolve(&self.project_root.join("humanoid_preparation"))?;
            if !root.starts_with(&allowed) {
                return Err(invalid(
                    "Preparation workspace belongs to a different project.",
                ));
            }
            if project.original_model != analysis.target_model {
                return Err(invalid("Target hero changed; analyze preparation again."));
            }
            same_hash(
                compiled,
                &analysis.target_sha256,
                "Original hero changed; analyze preparation again.",
            )?;
            same_hash(
                Path::new(&analysis.source),
                &analysis.source_sha256,
                "Workspace source changed; import it again.",
            )?;
            provenance::check_customization_selection(project, &analysis.geometry_customization)?;
            for prop in &analysis.geometry_customization.props {
                same_hash(
                    Path::new(&prop.source),
                    &prop.source_sha256,
                    "Workspace prop changed; import it again.",
                )?;
            }
            provenance::check_clothing_selection(
                project,
                analysis.clothing.as_ref().map(|c| &c.selection),
            )?;
            if let Some(clothing) = &analysis.clothing {
                same_hash(
                    Path::new(&clothing.source),
                    &clothing.source_sha256,
                    "Workspace garment changed; import it again.",
                )?;
            }
            if analysis.target_scale_audit.policy != transforms::SCALE_POLICY {
                return Err(invalid(
                    "Analysis predates exact target bone scale preservation; import again.",
                ));
            }
            let (expected, _) = transforms::prepare_target(&analysis.target_skeleton)
                .map_err(|e| invalid(e.to_string()))?;
            if serde_json::to_value(&expected)? != serde_json::to_value(&analysis.target)? {
                return Err(invalid(
                    "Target bind transforms changed in the preparation analysis. Import again.",
                ));
            }
            Ok((root, allowed))
        })?;
        let destination = root.join(format!("prepared-{}", uuid::Uuid::new_v4().simple()));
        let mut source = analysis.source.clone();
        let mut mode = project.model_input_mode;
        let mut rotation = analysis.rotation;
        if correction {
            let previous: Receipt = load(Path::new(&project.preparation_report))?;
            crate::fitting::check_unchanged(&previous.options, &options)?;
            if previous.prepared.rest_alignment.policy != crate::weapon::POLICY {
                crate::fitting::check_policy(&previous.options, &previous.prepared.rest_alignment.policy)?;
            }
            if preservation.is_some() {
                let previous: Receipt = load(Path::new(&project.preparation_report))?;
                let previous_plan = secondary_bones::selection(&analysis.analysis.bones, &analysis.analysis.weighted_bones,
                    &analysis.target.iter().map(Bone::from).collect::<Vec<_>>(), &previous.options)?;
                if previous_plan != preservation {
                    return Err(invalid("Physics bone choices changed. Convert from the original source before re-exporting corrections."));
                }
                secondary_bones::export_skeleton(&analysis.target_skeleton, &analysis.target, &previous.prepared.target,
                    &analysis.analysis.bones, preservation.as_ref())?;
                prepared_target = previous.prepared.target;
            } else {
                let previous: Receipt = load(Path::new(&project.preparation_report))?;
                if previous.prepared.target.len() != analysis.target.len() {
                    return Err(invalid("Physics bone choices changed. Convert from the original source before re-exporting corrections."));
                }
            }
            let blend = asset_paths::resolve(Path::new(&project.preparation_blend))?;
            if !blend.starts_with(&allowed) || !blend.is_file() {
                return Err(invalid(
                    "Choose the workspace prepared.blend for corrections.",
                ));
            }
            source = path_text(&blend)?;
            if mode != ModelInputMode::Weapon {
                mode = ModelInputMode::RiggedHumanoid;
            }
            rotation = [0.; 3];
            let mapping: BTreeMap<_, _> = prepared_target
                .iter()
                .map(|b| (b.name.clone(), b.name.clone()))
                .collect();
            options
                .values
                .insert("mapping".into(), serde_json::to_value(mapping)?);
            if mode != ModelInputMode::Weapon {
                let target_roles = options
                    .values
                    .get("target_roles")
                    .cloned()
                    .ok_or_else(|| invalid("Review target roles before correcting."))?;
                options.values.insert("source_roles".into(), target_roles);
            }
            // The editable output already has the target rig. Source helper
            // anchors must never be applied a second time on correction export.
            options.values.remove("mapping_anchors");
            options.values.remove("follow_motion_reviewed");
        }
        let weapon_plan = if correction { None } else { weapon_plan(project, analysis, &options)? };
        let props = if correction {
            vec![]
        } else {
            analysis
                .geometry_customization
                .props
                .iter()
                .map(|p| {
                    let mut choice = p.selection.choice.clone();
                    choice.source = Some(p.source.clone());
                    choice
                })
                .collect()
        };
        let request = Request::Prepare {
            source,
            mode,
            target: prepared_target,
            hero_root: project.hero_root(),
            options: options.clone(),
            rotation,
            correction,
            garment_source: if correction {
                None
            } else {
                analysis.clothing.as_ref().map(|c| c.source.clone())
            },
            face: if correction {
                None
            } else {
                analysis.geometry_customization.face.clone()
            },
            props,
            weapon: weapon_plan.clone(),
            breast_bones: if correction { None } else { analysis.breast_generation.clone() },
        };
        let mut report: PreparedResponse = self.step(
            if correction {
                "Re-export corrected Blender scene"
            } else {
                "Prepare geometry in Blender"
            },
            || self.bridge.execute(&request, &destination),
        )?;
        if project.model_input_mode == ModelInputMode::Weapon {
            if report.rest_alignment.policy != crate::weapon::POLICY {
                return Err(invalid("Blender did not return a weapon fit. Convert the source again."));
            }
        } else {
            crate::fitting::check_policy(&options, &report.rest_alignment.policy)?;
        }
        if let Some(plan) = &weapon_plan {
            // Record which original meshes the fitted gun replaces for the hand-off.
            let mut audit = report.algorithm_audits.get("weapon").cloned().unwrap_or(serde_json::Value::Null);
            if let Some(object) = audit.as_object_mut() {
                object.insert("replace".into(), serde_json::to_value(&plan.replace)?);
            }
            report.algorithm_audits.insert("weapon".into(), audit);
        } else if correction {
            // Corrections keep the previous gun fit and its mesh mapping.
            if let Ok(previous) = load::<Receipt>(Path::new(&project.preparation_report)) {
                if let Some(weapon) = previous.prepared.algorithm_audits.get("weapon") {
                    report.algorithm_audits.insert("weapon".into(), weapon.clone());
                }
            }
        }
        if let Some(plan) = &preservation {
            let expected = serde_json::to_value(plan)?;
            if !correction && report.algorithm_audits.get("preserved_physics_bones") != Some(&expected) {
                return Err(invalid("Blender physics bindings changed since inspection. Inspect the original source again."));
            }
            report.algorithm_audits.insert("preserved_physics_bones".into(), expected);
        }
        if correction { options = original_options; }
        let exported_skeleton = secondary_bones::export_skeleton(&analysis.target_skeleton, &analysis.target,
            &report.target, &analysis.analysis.bones, preservation.as_ref())?;
        let dmx = destination.join("prepared.dmx");
        let exported = self.step("Export and validate prepared DMX", || {
            export::export_dmx(
                &PreparedReport {
                    meshes: report.meshes.clone(),
                    extra: BTreeMap::new(),
                },
                &exported_skeleton,
                &dmx,
            )
            .map_err(|e| invalid(e.to_string()))
        })?;
        let(receipt,report_path)=self.step("Assemble and verify preparation handoff",||{
        let template = mesh_template(&analysis.target, &report.meshes)?;
        deadlock_core::safety::atomic_write(
            &destination.join("mesh_mapping.vmdl"),
            template.as_bytes(),
        )?;
        write_json(
            &destination.join("skeleton_reference.json"),
            &serde_json::json!({"sha256":analysis.target_sha256,"skeleton":analysis.target_skeleton,"scale_policy":transforms::SCALE_POLICY}),
        )?;
        let mut attached: Vec<_> = report
            .meshes
            .iter()
            .filter(|m| {
                m.extra
                    .get("attachment")
                    .is_some_and(|a| !a.is_null() && a != &serde_json::Value::Bool(false))
            })
            .map(|m| m.name.clone())
            .collect();
        if correction && !analysis.geometry_customization.props.is_empty() {
            let previous: Receipt = load(Path::new(&project.preparation_report))?;
            attached = previous.attached_prop_meshes;
            if attached
                .iter()
                .any(|name| !report.meshes.iter().any(|mesh| &mesh.name == name))
            {
                return Err(invalid(
                    "The corrected scene removed an attached prop. Reconfigure it and prepare from source again.",
                ));
            }
        }
        let mut export_validation: ValidationReceipt =
            serde_json::from_value(serde_json::to_value(exported)?)?;
        // The compile template/reference retains only original hero authority.
        // Additional DMX bones are checked here, then discovered and qualified
        // independently by the existing custom-bone build contract.
        let required = report.target.iter().map(|b| (b.name.clone(), b.parent.clone())).collect();
        let reference = deadlock_skeleton::source_bones::SkeletonReference {
            sha256: analysis.target_sha256.clone(),
            skeleton: exported_skeleton.clone(),
            scale_policy: Some(transforms::SCALE_POLICY.into()),
        };
        let parsed =
            deadlock_dmx::Dmx::parse(fs::read(&dmx)?).map_err(|e| invalid(e.to_string()))?;
        let target_bones = deadlock_skeleton::source_bones::source_bones(
            &parsed,
            &required,
            Some(&reference),
            Some(&analysis.target_sha256),
        )
        .map_err(|e| invalid(e.to_string()))?;
        export_validation
            .audits
            .insert("target_bones".into(), serde_json::to_value(target_bones)?);
        let receipt = Receipt {
            prepared: report,
            export_validation,
            target_model: project.original_model.clone(),
            target_sha256: analysis.target_sha256.clone(),
            analysis_path: "../analysis.json".into(),
            options,
            dmx_sha256: digest(&dmx)?,
            blender_operation_completed: true,
            native_compiler_validated: false,
            in_game_validated: false,
            clothing_selection: analysis.clothing.as_ref().map(|c| c.selection.clone()),
            geometry_customization: analysis.geometry_customization.clone(),
            attached_prop_meshes: attached,
        };
        let report_path = destination.join("preparation-report.json");
            Ok((receipt,report_path))
        })?;
        self.step("Save preparation provenance", || {
            write_json(&report_path, &receipt)
        })?;
        Ok(PreparationResult {
            dmx: path_text(&dmx)?,
            blend: receipt.prepared.blend,
            report: path_text(&report_path)?,
            textures: path_text(&destination.join("textures"))?,
            target_model: project.original_model.clone(),
            warnings: receipt.prepared.warnings,
        })
    }
}
pub fn review_issues(
    project: &Project,
    analysis: &Analysis,
    options: &PreparationOptions,
) -> Result<Vec<algorithms::ReviewIssue>> {
    let issue = |tab: &str, message: String| algorithms::ReviewIssue { tab: tab.into(), message, scope: None, roles: vec![], bones: vec![] };
    if project.model_input_mode == ModelInputMode::Weapon {
        // A separate weapon model needs a chosen gun and a computable fit only.
        return Ok(match weapon_plan(project, analysis, options) {
            Ok(Some(_)) => vec![],
            Ok(None) => vec![issue("weapon", "Choose the gun to use for this weapon model in the Weapon tab.".into())],
            Err(error) => vec![issue("weapon", error.to_string())],
        });
    }
    let target: Vec<_> = analysis.target.iter().map(Bone::from).collect();
    let mode = serde_json::to_value(project.model_input_mode)?;
    let suggested = serde_json::to_value(analysis.analysis.suggested_mode)?;
    let mut issues = algorithms::review_issues(
        &target,
        &analysis.analysis.bones,
        &analysis.analysis.weighted_bones,
        suggested.as_str().unwrap(),
        mode.as_str().unwrap(),
        options,
    );
    if let Err(error) = weapon_plan(project, analysis, options) {
        issues.push(issue("weapon", error.to_string()));
    }
    let shape_mode = options.values.get("shape_key_mode");
    if let Err(error) = crate::fitting::mode(options) {
        issues.push(algorithms::ReviewIssue {
            tab: "model fitting".into(), message: error.to_string(),
            scope: None, roles: vec![], bones: vec![],
        });
    }
    let selected = shape_mode.and_then(serde_json::Value::as_str).unwrap_or("");
    let invalid_mode = shape_mode.is_some_and(|value| !value.is_string())
        || !matches!(selected, "" | "current_static" | "basis_static");
    if invalid_mode
        || (!analysis.analysis.morphs.is_empty()
            && analysis.geometry_customization.face.is_none()
            && selected.is_empty())
    {
        issues.push(algorithms::ReviewIssue {
            tab: "shape keys".into(),
            message: if invalid_mode {
                "Choose a supported Shape key handling option in Conversion review.".into()
            } else {
                format!("This source has {} shape keys. In Conversion review, choose Keep the saved shape (static) or Use the base shape (static). Animated morphs are not exported; the original source keeps its shape keys.", analysis.analysis.morphs.len())
            },
            scope: None,
            roles: vec![],
            bones: vec![],
        });
    }
    Ok(issues)
}
/// Default gun handling for a fresh inspection.
pub fn weapon_suggestion(project: &Project, analysis: &Analysis) -> Option<crate::weapon::Choice> {
    let candidates = &analysis.analysis.weapons;
    if candidates.is_empty() {
        return None;
    }
    let roles = analysis.suggestions.source.as_ref().map(|s| s.roles.clone()).unwrap_or_default();
    let weapon_model = project.model_input_mode == ModelInputMode::Weapon;
    let best = crate::weapon::best_candidate(candidates, &analysis.analysis.bones, &roles)
        .or_else(|| weapon_model.then(|| candidates.iter().max_by_key(|c| c.vertices)).flatten())?;
    let reference = analysis.weapon_reference.as_ref();
    let action = if weapon_model || reference.is_some_and(|r| !r.meshes.is_empty()) {
        "replace"
    } else if reference.is_some_and(|r| r.notes.iter().any(|n| n.contains("separate model"))) {
        "remove"
    } else {
        "keep"
    };
    Some(crate::weapon::Choice { enabled: true, action: action.into(), objects: best.objects.clone(), ..Default::default() })
}

/// Body scale used by "keep the gun's size relative to the body".
fn body_scale(analysis: &Analysis, options: &PreparationOptions) -> Option<f64> {
    let roles = |key: &str| -> BTreeMap<String, String> {
        options.values.get(key).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    };
    let (source_roles, target_roles) = (roles("source_roles"), roles("target_roles"));
    let height = |roles: &BTreeMap<String, String>, position: &dyn Fn(&str) -> Option<[f64; 3]>| -> Option<f64> {
        let head = position(roles.get("head")?)?;
        let foot = position(roles.get("foot_L")?)?[2].min(position(roles.get("foot_R")?)?[2]);
        Some(head[2] - foot).filter(|h| *h > 1e-6)
    };
    let source = height(&source_roles, &|n| analysis.analysis.bones.iter().find(|b| b.name == n).map(|b| b.position))?;
    let target = height(&target_roles, &|n| analysis.target.iter().find(|b| b.name == n).map(|b| b.position))?;
    Some(target / source)
}

/// The gun fit requested in options.weapon, or None when the gun is not customized.
pub fn weapon_plan(project: &Project, analysis: &Analysis, options: &PreparationOptions) -> Result<Option<crate::weapon::Plan>> {
    let choice = crate::weapon::choice(options)?;
    let weapon_model = project.model_input_mode == ModelInputMode::Weapon;
    if !weapon_model && (!choice.enabled || choice.action == "keep" || choice.action.is_empty()) {
        return Ok(None);
    }
    if choice.objects.is_empty() {
        if weapon_model {
            return Ok(None);
        }
        return Err(invalid("Choose which object in the source is the gun (Weapon tab)."));
    }
    let wanted: std::collections::BTreeSet<_> = choice.objects.iter().collect();
    let candidate = analysis
        .analysis
        .weapons
        .iter()
        .find(|c| c.objects.iter().collect::<std::collections::BTreeSet<_>>() == wanted)
        .ok_or_else(|| invalid("The chosen gun is no longer in the inspected source. Inspect the source again and choose the gun in the Weapon tab."))?;
    if choice.action == "remove" && !weapon_model {
        return Ok(Some(crate::weapon::removal(candidate)));
    }
    let reference = analysis
        .weapon_reference
        .as_ref()
        .ok_or_else(|| invalid("The hero's original gun has not been measured. Inspect the source again."))?;
    if !weapon_model && reference.meshes.is_empty() {
        return Err(invalid(
            "This hero's gun is a separate model, so it cannot be replaced from the character file. Use Equipment → Weapon model to create a weapon project for it, and choose 'Leave it out' here.",
        ));
    }
    let source_roles: BTreeMap<String, String> = options.values.get("source_roles").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let target: Vec<Bone> = analysis
        .target
        .iter()
        .map(|b| {
            let mut bone = Bone::from(b);
            bone.details.insert("matrix".into(), serde_json::to_value(b.matrix).unwrap_or_default());
            bone
        })
        .collect();
    let fit = crate::weapon::fit(candidate, reference, &choice, &analysis.analysis.bones, &source_roles, &target, body_scale(analysis, options))?;
    Ok(Some(crate::weapon::plan(&fit, candidate, reference, if weapon_model { "weapon_model" } else { "replace" })))
}

/// Fit preview for the Weapon tab (no Blender run).
pub fn weapon_preview(project: &Project, analysis: &Analysis, options: &PreparationOptions) -> Result<crate::weapon::Preview> {
    let choice = crate::weapon::choice(options)?;
    let wanted: std::collections::BTreeSet<_> = choice.objects.iter().collect();
    let candidate = analysis
        .analysis
        .weapons
        .iter()
        .find(|c| c.objects.iter().collect::<std::collections::BTreeSet<_>>() == wanted)
        .ok_or_else(|| invalid("Choose which object in the source is the gun."))?;
    let reference = analysis.weapon_reference.as_ref().ok_or_else(|| invalid("The hero's original gun has not been measured. Inspect the source again."))?;
    let _ = project;
    let source_roles: BTreeMap<String, String> = options.values.get("source_roles").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let target: Vec<Bone> = analysis
        .target
        .iter()
        .map(|b| {
            let mut bone = Bone::from(b);
            bone.details.insert("matrix".into(), serde_json::to_value(b.matrix).unwrap_or_default());
            bone
        })
        .collect();
    let fit = crate::weapon::fit(candidate, reference, &choice, &analysis.analysis.bones, &source_roles, &target, body_scale(analysis, options))?;
    Ok(crate::weapon::preview(&fit, candidate, reference))
}

pub fn validate_review(
    project: &Project,
    analysis: &Analysis,
    options: &PreparationOptions,
) -> Result<()> {
    let issues = review_issues(project, analysis, options)?;
    if !issues.is_empty() {
        return Err(invalid(format!(
            "Humanoid review needs attention before Blender starts:\n{}",
            issues
                .into_iter()
                .map(|i| i.message)
                .collect::<Vec<_>>()
                .join("\n")
        )));
    }
    if options
        .values
        .get("reviewed")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(invalid(
            "Confirm bone mapping/landmarks and orientation before preparation.",
        ));
    }
    Ok(())
}
// Keep literal UTF-8 names identical to the binary DMX string table.
fn kv_string(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization")
}
pub fn mesh_template(bones: &[PreparedBone], meshes: &[export::PreparedMesh]) -> Result<String> {
    fn node(bone: &PreparedBone, bones: &[PreparedBone], depth: usize) -> Result<String> {
        if depth > bones.len() {
            return Err(invalid("Cyclic target hierarchy."));
        }
        let children = bones
            .iter()
            .filter(|b| b.parent.as_deref() == Some(&bone.name))
            .map(|b| node(b, bones, depth + 1))
            .collect::<Result<Vec<_>>>()?;
        Ok(format!(
            "{{ _class = \"Bone\" name = {} do_not_discard = true children = [ {} ] }}",
            kv_string(&bone.name),
            children.join(" ")
        ))
    }
    let roots = bones
        .iter()
        .filter(|b| b.parent.is_none())
        .map(|b| node(b, bones, 0))
        .collect::<Result<Vec<_>>>()?;
    let render=meshes.iter().map(|m|{let name=kv_string(&m.name);Ok(format!("{{ _class = \"RenderMeshFile\" name = {name} import_scale = 1 import_filter = {{ exclude_by_default = true exception_list = [ {name} ] }} }}"))}).collect::<Result<Vec<_>>>()?;
    Ok(format!(
        "{{ rootNode = {{ children = [ {{ _class = \"Skeleton\" children = [ {} ] }} {{ _class = \"BoneMarkupList\" bone_cull_type = \"None\" }} {} ] }} }}",
        roots.join(" "),
        render.join(" ")
    ))
}
pub fn handoff(project: &Project, result: &PreparationResult) -> Result<Project> {
    if result.target_model != project.original_model {
        return Err(invalid("Prepared result targets another hero."));
    }
    let report: Receipt = load(Path::new(&result.report))?;
    let same_hero_preparation = load::<Receipt>(Path::new(&project.preparation_report)).ok()
        .is_some_and(|old| old.target_model == report.target_model && old.target_sha256 == report.target_sha256);
    let mut p = project.clone();
    p.dmx = result.dmx.clone();
    p.preparation_blend = result.blend.clone();
    p.preparation_report = result.report.clone();
    p.textures = result.textures.clone();
    let parent = Path::new(&result.dmx)
        .parent()
        .ok_or_else(|| invalid("Prepared DMX has no parent."))?;
    p.source_root = path_text(parent)?;
    p.source_overlay.clear();
    p.material_mapping.clear();
    p.material_remap.clear();
    p.expected_dmx_sha256.clear();
    p.mesh_template = path_text(&parent.join("mesh_mapping.vmdl"))?;
    p.skeleton_reference = path_text(&parent.join("skeleton_reference.json"))?;
    p.materials = report.prepared.materials;
    p.meshes.clear();
    p.review_lods = false;
    if same_hero_preparation {
        let available: std::collections::BTreeSet<_> = report.prepared.meshes.iter().map(|m| m.name.as_str()).collect();
        for (name, mapping) in &project.meshes {
            if mapping.targets().is_ok_and(|targets| targets.iter().all(|n| available.contains(n))) {
                p.meshes.insert(name.clone(), mapping.clone());
            }
        }
        p.review_lods = project.review_lods && p.meshes.len() == project.meshes.len();
        for (name, settings) in &project.materials {
            if p.materials.contains_key(name) { p.materials.insert(name.clone(), settings.clone()); }
        }
        if !project.textures.is_empty() { p.textures = project.textures.clone(); }
    }
    if let Some(weapon) = report.prepared.algorithm_audits.get("weapon").filter(|w| !w.is_null()) {
        let meshes: Vec<String> = serde_json::from_value(weapon.get("meshes").cloned().unwrap_or_default()).unwrap_or_default();
        let replace: Vec<String> = serde_json::from_value(weapon.get("replace").cloned().unwrap_or_default()).unwrap_or_default();
        p.meshes.extend(crate::weapon::mesh_mappings(&meshes, &replace));
    }
    p.preparation_options = report.options;
    if p.preparation_options.values.get("automatic_secondary_motion").and_then(|v|v.as_bool())==Some(true)
        && p.preparation_options.values.get("preserve_physics_bones").and_then(|v|v.as_bool())==Some(true) {
        let roots:Vec<String>=serde_json::from_value(p.preparation_options.values.get("physics_roots").cloned().unwrap_or_default())?;
        if !roots.is_empty() {
            let mut choice=project.customization.secondary_motion.clone().unwrap_or_default();
            let same_roots=project.preparation_options.values.get("physics_roots")==p.preparation_options.values.get("physics_roots");
            if !same_hero_preparation || !same_roots {choice=Default::default();}
            choice.enabled=Some(choice.enabled.unwrap_or(true));
            choice.preset.get_or_insert_with(||"automatic".into());
            if choice.enabled==Some(true){
                p.customization.enabled=Some(true);p.customization.model_enabled=Some(true);
                if let Some(physics)=&mut p.customization.physics{physics.enabled=Some(false);}
            }
            p.customization.secondary_motion=Some(choice);
        }
    }
    let analysis_path = asset_paths::resolve(
        &Path::new(&result.report)
            .parent()
            .unwrap()
            .join(report.analysis_path),
    )?;
    let analysis: Analysis = load(&analysis_path)?;
    p.preparation_source = path_text(
        &analysis_path.parent().unwrap().join("source").join(
            Path::new(&analysis.source)
                .file_name()
                .ok_or_else(|| invalid("Analysis source filename missing."))?,
        ),
    )?;
    Ok(p)
}
fn prior_geometry(project: &Project) -> Option<GeometryRecord> {
    let value: serde_json::Value = load(Path::new(&project.preparation_report)).ok()?;
    serde_json::from_value(value.get("geometry_customization")?.clone()).ok()
}
fn kept_choices(o: &PreparationOptions) -> serde_json::Value {
    if o.values.get("preserve_physics_bones").and_then(|v| v.as_bool()) != Some(true) { return serde_json::Value::Null; }
    let roots = o.values.get("physics_roots").and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|n| n.as_str()).collect::<std::collections::BTreeSet<_>>());
    serde_json::json!(roots)
}
/// Moving-part choices (including breast motion) changed after the accepted
/// conversion: the project's model does not have the bones they need yet.
pub fn moving_parts_changed(project: &Project) -> bool {
    !project.preparation_report.is_empty()
        && load::<Receipt>(Path::new(&project.preparation_report))
            .is_ok_and(|receipt| kept_choices(&receipt.options) != kept_choices(&project.preparation_options))
}
pub const MOVING_PARTS_CHANGED: &str = "Moving parts or breast motion changed after this model was prepared, so the model in use does not have their bones yet. In Model preparation, convert again and click Use prepared model, then Analyze.";
pub fn check_handoff(project: &Project) -> Result<()> {
    let selection = provenance::customization_selection(project)?;
    if project.model_input_mode == ModelInputMode::Prepared
        && selection.props.is_empty()
        && selection.face.is_none()
    {
        if prior_geometry(project).is_none_or(|g| g.props.is_empty() && g.face.is_none()) {
            return Ok(());
        }
    }
    if project.preparation_report.is_empty() {
        return Err(invalid(
            "Prepare this humanoid and accept its review before Analyze / Build.",
        ));
    }
    let receipt: Receipt = load(Path::new(&project.preparation_report))?;
    crate::fitting::check_unchanged(&receipt.options, &project.preparation_options)?;
    if receipt.prepared.rest_alignment.policy != crate::weapon::POLICY {
        crate::fitting::check_policy(&receipt.options, &receipt.prepared.rest_alignment.policy)?;
    }
    if kept_choices(&receipt.options) != kept_choices(&project.preparation_options) {
        return Err(invalid("Physics bone choices changed. Convert the original source and choose Use prepared model before building."));
    }
    provenance::check_clothing_selection(project, receipt.clothing_selection.as_ref())?;
    provenance::check_customization_selection(project, &receipt.geometry_customization)?;
    if ![REST_POLICY, crate::following::POLICY, crate::fitting::PROPORTIONS_POLICY, crate::weapon::POLICY]
        .contains(&receipt.prepared.rest_alignment.policy.as_str())
    {
        return Err(invalid(
            "This humanoid uses an older bone-axis conversion. Import and prepare again from the original source.",
        ));
    }
    if receipt.export_validation.target_scale_audit.policy != transforms::SCALE_POLICY {
        return Err(invalid(
            "This humanoid predates exact bone-scale preservation. Import and prepare again.",
        ));
    }
    if receipt.target_model != project.original_model {
        return Err(invalid("Prepared target changed. Revalidate preparation."));
    }
    same_hash(
        Path::new(&project.dmx),
        &receipt.dmx_sha256,
        "Prepared model changed. Revalidate preparation before building.",
    )
}
pub fn resume(project: &Project) -> Result<(Analysis, PreparationResult, Receipt)> {
    let path = Path::new(&project.preparation_report);
    let report: Receipt = load(path)?;
    if report.target_model != project.original_model {
        return Err(invalid("Target hero changed; import and analyze again."));
    }
    let analysis_path = asset_paths::resolve(
        &path
            .parent()
            .unwrap_or(Path::new("."))
            .join(&report.analysis_path),
    )?;
    let mut analysis: Analysis = load(&analysis_path)?;
    if analysis.target_model != project.original_model {
        return Err(invalid("Target hero changed; import and analyze again."));
    }
    let root = analysis_path.parent().unwrap();
    analysis.root = path_text(root)?;
    fn moved(root: &Path, path: &str) -> Result<String> {
        path_text(
            &root.join(
                Path::new(path)
                    .file_name()
                    .ok_or_else(|| invalid("Source filename missing."))?,
            ),
        )
    }
    analysis.source = moved(&root.join("source"), &analysis.source)?;
    if let Some(c) = &mut analysis.clothing {
        c.source = moved(&root.join("clothing"), &c.source)?;
    }
    for (i, prop) in analysis.geometry_customization.props.iter_mut().enumerate() {
        prop.source = moved(&root.join("props").join(i.to_string()), &prop.source)?;
    }
    let result = PreparationResult {
        dmx: project.dmx.clone(),
        blend: project.preparation_blend.clone(),
        report: path_text(path)?,
        textures: project.textures.clone(),
        target_model: report.target_model.clone(),
        warnings: report.prepared.warnings.clone(),
    };
    Ok((analysis, result, report))
}
pub fn required_prop_meshes(project: &Project) -> Result<Vec<String>> {
    let selection = provenance::customization_selection(project)?;
    if selection.props.is_empty()
        && selection.face.is_none()
        && prior_geometry(project).is_none_or(|g| g.props.is_empty() && g.face.is_none())
    {
        return Ok(vec![]);
    }
    check_handoff(project)?;
    let receipt: Receipt = load(Path::new(&project.preparation_report))?;
    if (!selection.props.is_empty() && receipt.attached_prop_meshes.is_empty())
        || receipt.attached_prop_meshes.iter().any(String::is_empty)
    {
        return Err(invalid(
            "Preparation receipt has no valid attached prop meshes. Prepare again before building.",
        ));
    }
    Ok(receipt.attached_prop_meshes)
}
