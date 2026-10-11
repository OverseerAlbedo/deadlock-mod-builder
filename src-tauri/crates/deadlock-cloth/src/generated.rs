//! Fresh native secondary solver qualification. This mode explicitly replaces
//! original FE cloth, while preserving original rigid-body physics exactly.
//! Custom compiler spellings stay together. Original cloth identities are
//! restored with separately namespaced generated controls before qualification.
use crate::*;
use deadlock_resource::{cloth, constraints, Kind};
use deadlock_skeleton::{
    extensions::{self, SourceBinding},
    transforms,
};
use std::collections::{BTreeMap, BTreeSet};
pub mod fitting;
pub mod roll_lock;
pub mod bind_repair;
pub mod identity;
pub mod rotation_import;
pub mod sheet_stability;
pub mod sheet_frames;
pub mod attachment_limits;
pub const POLICY: &str = "generated-secondary-replacement-v2";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Audit {
    pub policy: String,
    pub resource_sha256: String,
    pub original_sha256: String,
    pub solver_sha256: String,
    pub source_binding: extensions::SourceBindingAudit,
    pub compiler_aliases: BTreeMap<String, String>,
    pub custom_bones: BTreeSet<String>,
    pub fitted_bones: Vec<String>,
    #[serde(default)]
    pub fitting: fitting::Proof,
    #[serde(default)]
    pub roll_lock: roll_lock::Proof,
    #[serde(default)]
    pub bind_repair: bind_repair::Proof,
    #[serde(default)]
    pub original_identity: identity::Proof,
    #[serde(default)]
    pub source_rotation_import: rotation_import::Proof,
    #[serde(default)]
    pub sheet_stability: sheet_stability::Proof,
    pub structure: cloth::StructureProof,
    pub compiled_bindings: bindings::BindingAudit,
    pub capacity: CapacityProof,
    pub constraints: constraints::Audit,
    pub original_rigid_body_physics_preserved: bool,
    pub original_cloth_solver_preserved: bool,
    pub in_game_motion_tested: bool,
}
fn spelling(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'_' {
                b as char
            } else {
                '_'
            }
        })
        .collect()
}
fn number(v: &Value) -> Result<f64> {
    let n = match v.kind {
        Kind::F32(n) => n as f64,
        Kind::F64(n) => n,
        Kind::I64(n) => n as f64,
        Kind::U64(n) => n as f64,
        _ => return Err(invalid("Expected native numeric value")),
    };
    if !n.is_finite() {
        return Err(invalid("Nonfinite native solver value"));
    }
    Ok(n)
}
fn usize_field(v: &Value, k: &str) -> Result<usize> {
    usize::try_from(field(v, k)?.integer()?)
        .map_err(|_| invalid(format!("Invalid solver index {k}")))
}
fn mapped_source(
    actual: &Skeleton,
    original: &Skeleton,
    source: &SourceBinding,
) -> Result<(SourceBinding, BTreeMap<String, String>)> {
    let present = actual.names.iter().collect::<BTreeSet<_>>();
    let extra = source.extra_names.iter().collect::<BTreeSet<_>>();
    let mut mapped = source.clone();
    let mut aliases = BTreeMap::new();
    let mut potential = BTreeMap::<String, usize>::new();
    for n in &source.skeleton.names {
        *potential.entry(spelling(n)).or_default() += 1;
    }
    for n in &source.extra_names {
        if !present.contains(n) {
            let s = spelling(n);
            if !present.contains(&s)
                || potential[&s] != 1
                || original.names.contains(&s)
                || source.skeleton.names.contains(&s)
            {
                return Err(invalid(format!("Missing or ambiguous generated bone: {n}")));
            }
            aliases.insert(n.clone(), s);
        }
    }
    for n in &mut mapped.skeleton.names {
        if extra.contains(n) {
            if let Some(s) = aliases.get(n) {
                *n = s.clone();
            }
        }
    }
    for n in mapped
        .extra_names
        .iter_mut()
        .chain(&mut mapped.required_extra_names)
    {
        if let Some(s) = aliases.get(n) {
            *n = s.clone();
        }
    }
    Ok((mapped, aliases))
}
/// Presence gate for an authored travel-limited preset. This supplements the
/// complete qualification checks; it does not certify per-node limit values.
pub fn require_animation_limit_records(native: &[u8]) -> Result<()> {
    let layout = container::layout(native)?;
    let physics = decode_unique(native, &layout.blocks, "PHYS")?;
    let fe = field(&physics.root, "m_pFeModel")?;
    let mut count = 0;
    for name in ["m_AnimStrayRadii", "m_SimdAnimStrayRadii"] {
        if let Some(value) = fe.get(name) {
            let rows = value.array()?;
            for row in rows {
                row.object()?;
            }
            count += rows.len();
        }
    }
    if count == 0 {
        return Err(invalid("The native compiler did not emit the authored hair/streamer travel limits. Keep the generated native model for inspection; this unbounded result cannot be installed as protected secondary motion."));
    }
    Ok(())
}
pub fn qualify(
    original: &[u8],
    candidate: &[u8],
    source: &SourceBinding,
    selected: &BTreeSet<String>,
    weighted: &BTreeSet<String>,
) -> Result<Audit> {
    qualify_locked(original, candidate, source, selected, weighted, &BTreeMap::new())
}
fn qualify_locked(
    original: &[u8], candidate: &[u8], source: &SourceBinding,
    selected: &BTreeSet<String>, weighted: &BTreeSet<String>, locked: &BTreeMap<usize, Value>,
) -> Result<Audit> {
    if selected.is_empty()
        || weighted.is_empty()
        || !weighted.is_subset(selected)
        || !selected.iter().all(|n| source.extra_names.contains(n))
    {
        return Err(invalid(
            "Generated solver has no verified selected source branches.",
        ));
    }
    let decoded = Decoded::read(candidate)?;
    let actual = skeleton(&decoded.data.root)?;
    let original_layout = container::layout(original)?;
    let original_data = decode_unique(original, &original_layout.blocks, "DATA")?;
    let original_sk = skeleton(&original_data.root)?;
    let projected = extensions::project_skeleton(&actual, &original_sk.names)?;
    deadlock_skeleton::check_transfer(&projected, &original_sk)?;
    transforms::audit_scale_preservation(&projected, &original_sk, false)?;
    let (mapped, aliases) = mapped_source(&actual, &original_sk, source)?;
    let raw_source = mapped;
    let (mapped, mut source_rotation_import) = rotation_import::reference(
        &actual, &original_sk, &raw_source, &decoded.animations,
    )?;
    let binding = extensions::source_binding(&actual, &original_sk, &mapped, &hash(original))?;
    let custom = binding
        .source_bones_checked
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let selected = selected
        .iter()
        .map(|n| aliases.get(n).unwrap_or(n).clone())
        .collect::<BTreeSet<_>>();
    let weighted = weighted
        .iter()
        .map(|n| aliases.get(n).unwrap_or(n).clone())
        .collect::<BTreeSet<_>>();
    for (n, f) in actual.names.iter().zip(&actual.flags) {
        if custom.contains(n) && *f & !(extensions::ORDINARY_FLAGS | cloth::MOTION_MASK as u32) != 0
        {
            return Err(invalid(format!("Unsupported secondary bone flags: {n}")));
        }
        if custom.contains(n) && !selected.contains(n) && *f & cloth::MOTION_MASK as u32 != 0 {
            return Err(invalid(format!(
                "Unselected source bone acquired simulation: {n}"
            )));
        }
    }
    let fe = field(&decoded.physics.root, "m_pFeModel")?;
    cloth::self_contained(fe)?;
    let structure = cloth::structure(&decoded.physics.root)?;
    let summary = runtime::summarize(&decoded.physics.root, &decoded.data.root, hash(candidate))?;
    let names = strings(field(fe, "m_CtrlName")?)?;
    let static_ = structure.static_nodes;
    let fitting = fitting::audit_with_roll_lock(fe, &actual.names, &selected, &weighted, locked)?;
    let fitted = fitting.fitted_bones();
    for (i, name) in names.iter().enumerate() {
        if i >= static_ && actual.names.contains(name) && !selected.contains(name) {
            return Err(invalid(format!(
                "Native solver dynamically moves a body or unselected bone: {name}"
            )));
        }
    }
    for name in &weighted {
        let i = actual.names.iter().position(|n| n == name).unwrap();
        if actual.flags[i] & cloth::MOTION_MASK as u32 != cloth::MOTION_MASK as u32 {
            return Err(invalid(format!(
                "Native secondary motion flags missing: {name}"
            )));
        }
    }
    let bindings = bindings::audit_bindings_with_generated_bones(
        &decoded.data.root,
        &decoded.ctrl.root,
        &decoded.meshes,
        &decoded.animations,
        &decoded.layout.blocks,
        &custom,
    )?;
    let capacity = capacity::audit(
        &decoded.data.root,
        &decoded.ctrl.root,
        &decoded.meshes,
        &decoded.layout.blocks,
        &decoded.physics.root,
    );
    let constraints = constraints::audit(&decoded.physics.root);
    if capacity.status != cloth::Status::Passed || constraints.status != cloth::Status::Passed {
        return Err(invalid(
            "Generated secondary solver failed native capacity or rest-constraint validation.",
        ));
    }
    rotation_import::audit(&actual, &original_sk, &raw_source, &decoded.physics.root, &mut source_rotation_import)?;
    Ok(Audit {
        policy: POLICY.into(),
        resource_sha256: hash(candidate),
        original_sha256: hash(original),
        solver_sha256: summary
            .solver_sha256
            .ok_or_else(|| invalid("Generated native solver is missing."))?,
        source_binding: binding,
        compiler_aliases: aliases,
        custom_bones: custom,
        fitted_bones: fitted.into_iter().collect(),
        fitting,
        roll_lock: roll_lock::Proof::default(),
        sheet_stability: sheet_stability::Proof::default(),
        bind_repair: bind_repair::Proof::default(),
        original_identity: identity::Proof::default(),
        source_rotation_import,
        structure,
        compiled_bindings: bindings,
        capacity,
        constraints,
        original_rigid_body_physics_preserved: false,
        original_cloth_solver_preserved: false,
        in_game_motion_tested: false,
    })
}
/// Keep the whole native FE value unchanged; restore only the original body block.
fn apply_qualified(
    original: &[u8],
    native: &[u8],
    source: &SourceBinding,
    selected: &BTreeSet<String>,
    weighted: &BTreeSet<String>,
) -> Result<(Vec<u8>, Audit)> {
    qualify(original, native, source, selected, weighted)?;
    let decoded = Decoded::read(native)?;
    let layout = container::layout(original)?;
    let mut physics = decode_unique(original, &layout.blocks, "PHYS")?;
    tested::require_body_bones(&physics.root, &skeleton(&decoded.data.root)?)?;
    if physics.format_guid != decoded.physics.format_guid {
        return Err(invalid(
            "Original and generated physics formats differ; native migration is required.",
        ));
    }
    *field_mut(&mut physics.root, "m_pFeModel")? =
        field(&decoded.physics.root, "m_pFeModel")?.clone();
    let result = container::rebuild(
        native,
        &BTreeMap::from([(
            "PHYS".into(),
            binary::encode(&physics.root, physics.format_guid)?,
        )]),
    )?;
    unchanged_blocks(native, &result, &["PHYS"])?;
    let audit = validate_qualified(original, native, &result, source, selected, weighted)?;
    Ok((result, audit))
}
fn validate_qualified(
    original: &[u8],
    native: &[u8],
    candidate: &[u8],
    source: &SourceBinding,
    selected: &BTreeSet<String>,
    weighted: &BTreeSet<String>,
) -> Result<Audit> {
    let mut audit = qualify(original, candidate, source, selected, weighted)?;
    let expected = qualify(original, native, source, selected, weighted)?;
    if audit.solver_sha256 != expected.solver_sha256 {
        return Err(invalid(
            "Generated secondary solver changed after native compilation.",
        ));
    }
    unchanged_blocks(native, candidate, &["PHYS"])?;
    let a = Decoded::read(candidate)?;
    let layout = container::layout(original)?;
    let body = decode_unique(original, &layout.blocks, "PHYS")?;
    if !cloth::same_body_physics(&a.physics.root, &body.root)? {
        return Err(invalid(
            "Original rigid-body physics changed around the generated solver.",
        ));
    }
    audit.original_rigid_body_physics_preserved = true;
    Ok(audit)
}

fn planned_roll_lock(native: &[u8], source: &SourceBinding, chains: &[roll_lock::Chain],
    weighted: &BTreeSet<String>, aliases: &BTreeMap<String,String>) -> Result<roll_lock::Patch> {
    let decoded=Decoded::read(native)?;
    let actual=skeleton(&decoded.data.root)?;
    for chain in chains {
        if source.extra_names.contains(&chain.anchor) || chain.bones.iter().any(|n|!weighted.contains(n)) {
            return Err(invalid("Strand selection includes a body bone or unweighted joint."));
        }
    }
    let chains=chains.iter().map(|c|roll_lock::Chain{anchor:c.anchor.clone(),bones:c.bones.iter().map(|n|aliases.get(n).unwrap_or(n).clone()).collect(),per_joint_reference:c.per_joint_reference}).collect::<Vec<_>>();
    roll_lock::apply(field(&decoded.physics.root,"m_pFeModel")?,&actual,&chains)
}
fn planned_sheet(fe: &Value, plan: &sheet_stability::Plan, weighted: &BTreeSet<String>, aliases: &BTreeMap<String,String>) -> Result<(Value,sheet_stability::Proof)> {
    if plan.bones.iter().any(|n|!weighted.contains(n)) {return Err(invalid("Fabric selection includes an unweighted or body bone."));}
    if plan.attachments.bones.iter().any(|n|!weighted.contains(&n.bone)) {return Err(invalid("Attachment selection includes an unweighted or body bone."));}
    let name=|n:&String|aliases.get(n).unwrap_or(n).clone();
    sheet_stability::apply(fe,&sheet_stability::Plan {bones:plan.bones.iter().map(name).collect(),
        hinges:plan.hinges.iter().map(|h|h.each_ref().map(name)).collect(),
        seams:plan.seams.iter().map(|s|s.each_ref().map(name)).collect(),
        frames:plan.frames.iter().map(|f|sheet_frames::Frame {bone:name(&f.bone),
            along:f.along.each_ref().map(name),across:f.across.each_ref().map(name)}).collect(),
        attachments:attachment_limits::Plan {bones:plan.attachments.bones.iter().map(|r|attachment_limits::Restraint {
            bone:name(&r.bone),anchor:name(&r.anchor),radius_inches:r.radius_inches,region:r.region.clone()}).collect()}})
}
/// Native FE remains the authority. Only the deterministic, source-selected
/// strand orientation transform is allowed between that snapshot and output.
fn apply_locked_qualified(original: &[u8], native: &[u8], source: &SourceBinding,
    selected: &BTreeSet<String>, weighted: &BTreeSet<String>, chains: &[roll_lock::Chain], sheet: &sheet_stability::Plan) -> Result<(Vec<u8>,Audit)> {
    if chains.is_empty() && sheet.bones.is_empty() && sheet.attachments.bones.is_empty(){return apply_qualified(original,native,source,selected,weighted);}
    let (baseline,audit)=apply_qualified(original,native,source,selected,weighted)?;
    let patch=planned_roll_lock(native,source,chains,weighted,&audit.compiler_aliases)?;
    let mut decoded=Decoded::read(&baseline)?;
    *field_mut(&mut decoded.physics.root,"m_pFeModel")?=planned_sheet(&patch.fe,sheet,weighted,&audit.compiler_aliases)?.0;
    let result=container::rebuild(&baseline,&BTreeMap::from([("PHYS".into(),binary::encode(&decoded.physics.root,decoded.physics.format_guid)?)]))?;
    let audit=validate_locked_qualified(original,native,&result,source,selected,weighted,chains,sheet)?;
    Ok((result,audit))
}
fn validate_locked_qualified(original: &[u8], native: &[u8], candidate: &[u8], source: &SourceBinding,
    selected: &BTreeSet<String>, weighted: &BTreeSet<String>, chains: &[roll_lock::Chain], sheet: &sheet_stability::Plan) -> Result<Audit> {
    if chains.is_empty() && sheet.bones.is_empty() && sheet.attachments.bones.is_empty(){return validate_qualified(original,native,candidate,source,selected,weighted);}
    let native_audit=qualify(original,native,source,selected,weighted)?;
    let patch=planned_roll_lock(native,source,chains,weighted,&native_audit.compiler_aliases)?;
    let (expected,sheet_proof)=planned_sheet(&patch.fe,sheet,weighted,&native_audit.compiler_aliases)?;
    let decoded=Decoded::read(candidate)?;
    if !field(&decoded.physics.root,"m_pFeModel")?.canonical_eq(&expected) {
        return Err(invalid("Generated solver differs from its immutable native snapshot and verified strand transform."));
    }
    let mut audit=qualify_locked(original,candidate,source,selected,weighted,&patch.bases)?;
    unchanged_blocks(native,candidate,&["PHYS"])?;
    let layout=container::layout(original)?;
    let body=decode_unique(original,&layout.blocks,"PHYS")?;
    if !cloth::same_body_physics(&decoded.physics.root,&body.root)?{return Err(invalid("Original body physics changed around the strand transform."));}
    audit.roll_lock=patch.proof;
    audit.sheet_stability=sheet_proof;
    audit.original_rigid_body_physics_preserved=true;
    Ok(audit)
}

pub fn apply(original: &[u8], native: &[u8], source: &SourceBinding,
    selected: &BTreeSet<String>, weighted: &BTreeSet<String>) -> Result<(Vec<u8>,Audit)> {
    apply_with_roll_lock(original,native,source,selected,weighted,&[])
}
pub fn validate(original: &[u8], native: &[u8], candidate: &[u8], source: &SourceBinding,
    selected: &BTreeSet<String>, weighted: &BTreeSet<String>) -> Result<Audit> {
    validate_with_roll_lock(original,native,candidate,source,selected,weighted,&[])
}
/// Reconciliation is deterministic from the immutable native snapshot and
/// selected source. Already valid rigs retain their exact compiled DATA bytes.
pub fn apply_with_roll_lock(original: &[u8], native: &[u8], source: &SourceBinding,
    selected: &BTreeSet<String>, weighted: &BTreeSet<String>, chains: &[roll_lock::Chain]) -> Result<(Vec<u8>,Audit)> {
    apply_with_fabric(original,native,source,selected,weighted,chains,&Default::default())
}
pub fn apply_with_fabric(original: &[u8], native: &[u8], source: &SourceBinding,
    selected: &BTreeSet<String>, weighted: &BTreeSet<String>, chains: &[roll_lock::Chain], sheet: &sheet_stability::Plan) -> Result<(Vec<u8>,Audit)> {
    let (canonical,identity)=identity::prepare(original,native,source)?;
    let (prepared,proof)=bind_repair::prepare(original,&canonical,source)?;
    let (output,mut audit)=apply_locked_qualified(original,&prepared,source,selected,weighted,chains,sheet)?;
    audit.bind_repair=proof;
    audit.original_identity=identity;
    Ok((output,audit))
}
pub fn validate_with_roll_lock(original: &[u8], native: &[u8], candidate: &[u8], source: &SourceBinding,
    selected: &BTreeSet<String>, weighted: &BTreeSet<String>, chains: &[roll_lock::Chain]) -> Result<Audit> {
    validate_with_fabric(original,native,candidate,source,selected,weighted,chains,&Default::default())
}
pub fn validate_with_fabric(original: &[u8], native: &[u8], candidate: &[u8], source: &SourceBinding,
    selected: &BTreeSet<String>, weighted: &BTreeSet<String>, chains: &[roll_lock::Chain], sheet: &sheet_stability::Plan) -> Result<Audit> {
    let (canonical,identity)=identity::prepare(original,native,source)?;
    let (prepared,proof)=bind_repair::prepare(original,&canonical,source)?;
    let mut audit=validate_locked_qualified(original,&prepared,candidate,source,selected,weighted,chains,sheet)?;
    audit.bind_repair=proof;
    audit.original_identity=identity;
    Ok(audit)
}
