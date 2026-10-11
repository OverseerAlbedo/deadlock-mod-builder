//! bpy-generated evidence validates ordinary skinning, not Source 2 simulation.
use deadlock_preparation::{algorithms::{self, Bone}, secondary_bones::{self as secondary, PreservationPlan}};
use deadlock_project::PreparationOptions;
use deadlock_skeleton::{Skeleton, export::{self, PreparedReport}, transforms::PreparedBone, source_bones::{self, SkeletonReference}, source_identity, source_skeleton::{SourceAuditReport, RuntimeMotionSourceAudit}};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn fixture() -> Value { serde_json::from_str(include_str!("fixtures/secondary-bones.json")).unwrap() }
#[test]
fn named_weighted_chain_without_bodies_is_preserved_but_names_and_coincident_pivots_are_insufficient(){
    let mut source:Vec<Bone>=serde_json::from_value(json!([
        {"name":"pelvis","parent":null,"position":[0,0,0]},
        {"name":"head","parent":"pelvis","position":[0,0,5]},
        {"name":"hair_root","parent":"head","position":[0,0,4]},
        {"name":"hair_tip","parent":"hair_root","position":[0,0,2]},
        {"name":"rear_support","parent":"pelvis","position":[0,0,1]}
    ])).unwrap();
    let weighted=vec!["hair_root".into(),"hair_tip".into(),"rear_support".into()];
    let roles=BTreeMap::from([("pelvis".into(),"pelvis".into()),("head".into(),"head".into())]);
    let groups=secondary::candidates(&source,&weighted,&roles,&roles).unwrap();assert_eq!(groups.len(),1);assert_eq!(groups[0].root,"hair_root");
    source[3].position=source[2].position;assert!(secondary::candidates(&source,&weighted,&roles,&roles).unwrap().is_empty());
}
fn skeleton(target: &[PreparedBone]) -> Skeleton {
    Skeleton {
        names: target.iter().map(|b| b.name.clone()).collect(),
        parents: target.iter().map(|b| b.parent.as_ref().map_or(-1, |p| target.iter().position(|t| &t.name == p).unwrap() as i32)).collect(),
        positions: target.iter().map(|b| b.local_position).collect(),
        rotations: target.iter().map(|b| b.local_rotation).collect(),
        scales: target.iter().map(|b| b.local_scale).collect(),
        flags: vec![0x40; target.len()],
    }
}
fn validate(data: &Value) -> Value {
    let source: Vec<Bone> = serde_json::from_value(data["inspection"]["bones"].clone()).unwrap();
    let weighted: Vec<String> = serde_json::from_value(data["inspection"]["weighted_bones"].clone()).unwrap();
    let target: Vec<PreparedBone> = serde_json::from_value(data["target"].clone()).unwrap();
    let options: PreparationOptions = serde_json::from_value(data["options"].clone()).unwrap();
    let target_bones = target.iter().map(Bone::from).collect::<Vec<_>>();
    let plan = secondary::selection(&source, &weighted, &target_bones, &options).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&plan).unwrap(), data["report"]["preserved_physics_bones"]);
    assert!(algorithms::review_issues(&target_bones, &source, &weighted, "rigged_humanoid", "rigged_humanoid", &options).is_empty());
    let prepared: Vec<PreparedBone> = serde_json::from_value(data["report"]["target"].clone()).unwrap();
    let original = skeleton(&target);
    let combined = secondary::export_skeleton(&original, &target, &prepared, &source, Some(&plan)).unwrap();
    let report: PreparedReport = serde_json::from_value(data["report"].clone()).unwrap();
    let (bytes, audit) = export::export_bytes(&report, &combined).unwrap();
    let sha = format!("{:x}", Sha256::digest(&bytes));
    let dmx = deadlock_dmx::Dmx::parse(bytes).unwrap();
    let authority = SourceAuditReport { compiled_sha256: Some("a".repeat(64)), runtime_motion: RuntimeMotionSourceAudit {status: "audited".into(), resource_sha256: Some("a".repeat(64)), skeleton: Some(original.clone()), ..Default::default()} };
    let selected = report.meshes.iter().map(|m| m.name.clone()).collect();
    let contract = source_identity::source_contract(&dmx, &authority, &selected, &sha).unwrap().unwrap();
    assert!(!contract.custom_cloth_solver_created && !contract.native_compiler_validated);
    assert!(contract.original_names_and_parents_preserved && !contract.weights_modified && !contract.source_modified);
    let required = target.iter().map(|b| (b.name.clone(), b.parent.clone())).collect();
    let reference = SkeletonReference {sha256: "a".repeat(64), skeleton: original.clone(), scale_policy: Some(deadlock_skeleton::transforms::SCALE_POLICY.into())};
    let template = source_bones::template_contract(&dmx, &required, &authority, &contract, Some(&reference)).unwrap();
    // Original binds, appended ancestry, and exact matrix consistency remain
    // independent checks. Any one violation must stop export.
    for variant in 0..4 {
        let mut bad = prepared.clone();
        match variant {
            0 => bad[0].local_scale += 0.01,
            1 => bad[target.len()].parent = Some("head".into()),
            2 => bad[target.len()].matrix[0][3] += 0.01,
            _ => { bad.pop(); },
        }
        assert!(secondary::export_skeleton(&original, &target, &bad, &source, Some(&plan)).is_err());
    }
    assert!(secondary::export_skeleton(&original, &target, &prepared, &source, None).is_err());
    json!({"added_bones": plan.exported_bones.len(), "groups": plan.groups.len(), "dmx": audit, "template": template, "new_physics": false})
}

#[test]
fn actual_bpy_result_exports_with_unchanged_hero_and_authorized_ordinary_bones() {
    let audit = validate(&fixture());
    assert_eq!(audit["added_bones"], 3);
}

#[test]
fn binding_properties_not_names_exclude_body_colliders_and_reject_stale_choices() {
    let data = fixture();
    let source: Vec<Bone> = serde_json::from_value(data["inspection"]["bones"].clone()).unwrap();
    let weighted: Vec<String> = serde_json::from_value(data["inspection"]["weighted_bones"].clone()).unwrap();
    let target: Vec<Bone> = serde_json::from_value(data["target"].clone()).unwrap();
    let mut options: PreparationOptions = serde_json::from_value(data["options"].clone()).unwrap();
    let plan = secondary::selection(&source, &weighted, &target, &options).unwrap().unwrap();
    assert_eq!(plan.groups[0].source_bones, ["appendage", "helper", "tip"]);
    assert!(!plan.exported_bones.contains_key("upperarm_L"));
    for roots in [json!([]), json!(["upperarm_L"]), json!(["tail_named_without_binding"]), json!(["appendage", "appendage"])] {
        options.values.insert("physics_roots".into(), roots);
        assert!(secondary::selection(&source, &weighted, &target, &options).is_err());
    }
    options.values.insert("physics_roots".into(), json!(["appendage"]));
    let mut missing = source.clone();
    for b in &mut missing { b.details.remove("physics"); }
    assert!(secondary::selection(&missing, &weighted, &target, &options).is_err());
    let before = options.clone();
    let body = secondary::body_options(&options, Some(&plan));
    assert!(body.values["mapping"].get("appendage").is_none());
    assert_eq!(options, before);
    options.values.insert("preserve_physics_bones".into(), json!(false));
    assert!(secondary::selection(&source, &weighted, &target, &options).unwrap().is_none());
    let mut collision = target.clone();
    let mut bone = collision[0].clone(); bone.name = plan.exported_bones["appendage"].clone(); collision.push(bone);
    options.values.insert("preserve_physics_bones".into(), json!(true));
    assert!(secondary::selection(&source, &weighted, &collision, &options).is_err());
}

#[test]
fn optional_full_asset_bpy_outputs_pass_native_dmx_and_build_template_contracts() {
    let Ok(directory) = std::env::var("DMB_SECONDARY_NATIVE_INPUT") else { return; };
    let directory = Path::new(&directory);
    let load = |path: &Path| -> Value {serde_json::from_slice(&fs::read(path).unwrap()).unwrap()};
    let mut audits = BTreeMap::new();
    for label in ["one", "all"] {
        let report = load(&directory.join(label).join("result.json"));
        let plan: PreservationPlan = serde_json::from_value(report["preserved_physics_bones"].clone()).unwrap();
        let mut options = load(&directory.join("options.json"));
        options["preserve_physics_bones"] = json!(true);
        options["physics_roots"] = json!(plan.groups.iter().map(|g| &g.root).collect::<Vec<_>>());
        audits.insert(label, validate(&json!({"inspection":load(&directory.join("inspection.json")), "target":load(&directory.join("target.json")), "options":options, "report":report})));
    }
    fs::write(directory.join("native-export.json"), serde_json::to_vec_pretty(&audits).unwrap()).unwrap();
}

#[test]
fn robes_and_unnamed_hanging_chains_on_the_torso_move_but_limb_and_short_chains_do_not() {
    let data: Value = serde_json::from_str(include_str!("fixtures/hanging-chains.json")).unwrap();
    let source: Vec<Bone> = serde_json::from_value(data["bones"].clone()).unwrap();
    let weighted: Vec<String> = serde_json::from_value(data["weighted"].clone()).unwrap();
    let roles: BTreeMap<String, String> = serde_json::from_value(data["roles"].clone()).unwrap();
    let groups = secondary::candidates(&source, &weighted, &roles, &roles).unwrap();
    let roots: Vec<_> = groups.iter().map(|g| g.root.clone()).collect();
    let expected: Vec<String> = serde_json::from_value(data["expected_roots"].clone()).unwrap();
    assert_eq!(roots, expected);
    let strap = groups.iter().find(|g| g.root == "Strap_00").unwrap();
    assert_eq!((strap.source_anchor.as_str(), strap.source_bones.len()), ("Bip001_Pelvis", 4));
    assert!(groups.iter().all(|g| !g.source_bones.iter().any(|b| b.contains("Finger") || b.starts_with("Pouch"))));
}
