//! Connect prepared secondary proxies to native compilation and frozen-output gates.
use crate::{
    inspect::{InspectionReport, ModelPipeline},
    invalid,
    motion::{read_resource, Restoration, ValidatedMotion},
};
use deadlock_cloth_authoring::{proxy_source, secondary::Setup};
use deadlock_core::{safety, Result};
use deadlock_project::Project;
use deadlock_skeleton::extensions::SourceBinding;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeContract {
    pub policy: String,
    pub source_sha256: String,
    pub proxy_sha256: String,
    pub native_sha256: String,
    pub native_snapshot: PathBuf,
    #[serde(default)]
    pub motion_plan_sha256:String,
}
/// The bones the cloth solver moves (all bones, and the weighted ones). Parts
/// set to stay still are left out: they have no simulated points.
fn selected(s: &Setup) -> (BTreeSet<String>, BTreeSet<String>) {
    (
        s.simulated_groups()
            .flat_map(|g| g.bones.iter().cloned())
            .collect(),
        s.simulated_groups()
            .flat_map(|g| g.weighted_bones.iter().cloned())
            .collect(),
    )
}
impl ModelPipeline<'_> {
    pub(crate) fn author_secondary(
        &self,
        p: &Project,
        report: &InspectionReport,
        content: &Path,
        model_path: &Path,
        before: &str,
    ) -> Result<String> {
        let Some(setup) = &report.secondary_motion else {
            return Ok(before.into());
        };
        self.clock.measure("Author secondary motion",None,|_|{
            let binding=report.source_binding().ok_or_else(||invalid("Secondary rig source authority is missing."))?;
            let asset=format!("{}/dmx/dmb_secondary_proxy.dmx",p.hero_root());
            if safety::inside(content,&asset)?.exists(){return Err(invalid("Secondary proxy path conflicts with an existing source asset."));}
            safety::atomic_write(&safety::inside(content,&asset)?,&setup.proxy)?;
            // Leg colliders measured at Analyze (none for the TangTang presets).
            let analysis=proxy_source::Analysis{units:proxy_source::UNITS.into(),millimeters_per_unit:25.4,leg_colliders:setup.body_colliders.clone(),collision_disabled:setup.body_colliders.is_empty()};
            let (authored,_)=proxy_source::author_direct_proxy(before,&asset,&analysis,&binding.skeleton,"dmb_secondary",true).map_err(|e|invalid(e.to_string()))?;
            let (authored,_)=proxy_source::configure_secondary_proxy(&authored,"dmb_secondary_proxy").map_err(|e|invalid(e.to_string()))?;
            safety::write_json(&self.root(p)?.join("secondary-motion-source.json"),setup)?;
            (self.log)(&format!("Secondary motion: {} groups, {} proxy points; original render DMX and binds retained. Compiling {}",setup.groups.len(),setup.proxy_positions,model_path.display()));
            Ok(authored)
        })
    }
    pub(crate) fn restore_secondary(
        &self,
        p: &Project,
        report: &InspectionReport,
        output: &Path,
    ) -> Result<(NativeContract, Restoration, ValidatedMotion)> {
        self.clock
            .measure("Validate generated secondary motion", None, |_| {
                let setup = report
                    .secondary_motion
                    .as_ref()
                    .ok_or_else(|| invalid("Missing generated source plan."))?;
                let source = report
                    .source_binding()
                    .ok_or_else(|| invalid("Missing generated source binding."))?;
                let original = report.original.compiled_bytes(self.cancel)?;
                let native = read_resource(output, 512 * 1024 * 1024, self.cancel)?;
                let sha = crate::inspect::hash(&native);
                let directory = self.root(p)?.join("runtime-motion");
                let snapshot = directory
                    .join("generated-native")
                    .join(format!("{sha}.vmdl_c"));
                if snapshot.exists() {
                    if safety::digest_with_cancel(&snapshot, self.cancel)? != sha {
                        return Err(invalid("Native secondary backup changed."));
                    }
                } else {
                    safety::atomic_write(&snapshot, &native)?;
                }
                let contract = NativeContract {
                    policy: deadlock_cloth_authoring::secondary::POLICY.into(),
                    source_sha256: setup.source_sha256.clone(),
                    proxy_sha256: setup.proxy_sha256.clone(),
                    native_sha256: sha.clone(),
                    native_snapshot: snapshot.clone(),
                    motion_plan_sha256:crate::inspect::hash(&serde_json::to_vec(setup)?),
                };
                if setup.requires_animation_limits() {
                    deadlock_cloth::generated::require_animation_limit_records(&native)
                        .map_err(|e| invalid(e.to_string()))?;
                }
                let (selected, weighted) = selected(setup);
                let (changed, audit) = deadlock_cloth::generated::apply_with_fabric(
                    &original, &native, &source, &selected, &weighted, &setup.roll_lock_chains, &setup.fabric_stability(),
                )
                .map_err(|e| invalid(e.to_string()))?;
                if audit.sheet_stability.bend_limit_rods>0 || audit.sheet_stability.seam_limit_rods>0 {
                    (self.log)(&format!("Long garment: {} seam separation limits, {} fold limits and {} local support diagonals; existing cloth movement maps retained.",
                        audit.sheet_stability.seam_limit_rods,audit.sheet_stability.bend_limit_rods,audit.sheet_stability.rigid_support_rods));
                }
                if !audit.sheet_stability.surface_frames.is_empty() {
                    (self.log)(&format!("Long garment: {} bone rotations follow the connected fabric surface; streamer behavior retained.",
                        audit.sheet_stability.surface_frames.len()));
                }
                if !audit.sheet_stability.attachments.bones.is_empty() {
                    (self.log)(&format!("Upper garment support: {} source-evidenced hood / upper-back controls restrained; free straps, streamers and lower skirt settings retained.",
                        audit.sheet_stability.attachments.bones.len()));
                }
                if !audit.original_identity.original_aliases.is_empty() {
                    (self.log)(&format!(
                        "Restored {} original cloth bone names and separated {} generated proxy names; original bones, accessory bindings and simulation settings verified.",
                        audit.original_identity.original_aliases.len(),
                        audit.original_identity.control_renames.iter().filter(|r| r.derived_proxy).count()
                    ));
                }
                if !audit.bind_repair.corrected_bones.is_empty() {
                    (self.log)(&format!(
                        "Reconciled native rest transforms for {} secondary bones; source, animation, mesh and solver checks passed.",
                        audit.bind_repair.corrected_bones.len()
                    ));
                }
                if !audit.source_rotation_import.bones.is_empty() {
                    (self.log)(&format!(
                        "Verified native rotation conversion for {} secondary bones; source, animation, mesh and physics rest checks passed.",
                        audit.source_rotation_import.bones.len()
                    ));
                }
                let independent = self.motion(&directory).independent(&changed)?;
                let runtime = ValidatedMotion::Generated {
                    audit: audit.clone(),
                    independent,
                };
                let restoration = Restoration::Generated {
                    audit,
                    native_model_backup: snapshot,
                };
                let _barrier = self.runner.quiescent()?;
                self.cancel.check()?;
                report.verify_sources(self.cancel)?;
                if safety::digest_with_cancel(output, self.cancel)? != sha {
                    return Err(invalid(
                        "Native output changed during secondary motion validation.",
                    ));
                }
                safety::atomic_write(output, &changed)?;
                safety::write_json(&directory.join("generated-contract.json"), &contract)?;
                Ok((contract, restoration, runtime))
            })
    }
    pub(crate) fn validate_secondary(
        &self,
        p: &Project,
        original: &[u8],
        candidate: &[u8],
        setup: &Setup,
        contract: &NativeContract,
        source: &SourceBinding,
    ) -> Result<ValidatedMotion> {
        if contract.policy != setup.policy
            || contract.source_sha256 != setup.source_sha256
            || contract.proxy_sha256 != setup.proxy_sha256
            || source.source_sha256 != setup.source_sha256
            || contract.motion_plan_sha256 != crate::inspect::hash(&serde_json::to_vec(setup)?)
        {
            return Err(invalid(
                "Secondary motion settings or source changed. Rebuild this project.",
            ));
        }
        // The saved path cannot select another external reference at installation.
        let expected = self
            .root(p)?
            .join("runtime-motion/generated-native")
            .join(format!("{}.vmdl_c", contract.native_sha256));
        if contract.native_snapshot != expected {
            return Err(invalid(
                "Secondary native snapshot is outside its owned build location.",
            ));
        }
        let native = read_resource(&expected, 512 * 1024 * 1024, self.cancel)?;
        if crate::inspect::hash(&native) != contract.native_sha256 {
            return Err(invalid(
                "Secondary native snapshot is missing or changed. Rebuild the project.",
            ));
        }
        if setup.requires_animation_limits() {
            deadlock_cloth::generated::require_animation_limit_records(&native)
                .map_err(|e| invalid(e.to_string()))?;
        }
        let (selected, weighted) = selected(setup);
        let audit = deadlock_cloth::generated::validate_with_fabric(
            original, &native, candidate, source, &selected, &weighted, &setup.roll_lock_chains, &setup.fabric_stability(),
        )
        .map_err(|e| invalid(e.to_string()))?;
        let directory = self.root(p)?.join("runtime-motion");
        let independent = self.motion(&directory).independent(candidate)?;
        Ok(ValidatedMotion::Generated { audit, independent })
    }
}
