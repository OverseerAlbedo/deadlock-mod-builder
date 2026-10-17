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
    assert!(groups.iter().all(|g| !g.source_bones.iter().any(|b| b.contains("Finger") || b.starts_with("Pouch") || b.starts_with("HairPin") || b.starts_with("HairLoop"))));
    // Breasts move only when breast motion is on, as one weighted bone each.
    let breasts: Vec<String> = serde_json::from_value(data["breast_bones"].clone()).unwrap();
    assert_eq!(secondary::breast_bones(&source, &weighted, &roles), breasts);
    assert_eq!(secondary::candidates_with(&source, &weighted, &roles, &roles, Some(false)).unwrap(), groups);
    let with: Vec<String> = serde_json::from_value(data["expected_roots_with_breast_motion"].clone()).unwrap();
    let moving = secondary::candidates_with(&source, &weighted, &roles, &roles, Some(true)).unwrap();
    assert_eq!(moving.iter().map(|g| g.root.clone()).collect::<Vec<_>>(), with);
    let left = moving.iter().find(|g| g.root == "Bone_Breast_L_00").unwrap();
    assert_eq!((left.source_anchor.as_str(), left.source_bones.len()), ("Bip001_Spine1", 1));
    // Without breast names the mirrored pair in front of the chest is found by position.
    let renamed: BTreeMap<String, String> = serde_json::from_value(data["renamed_breast_bones"].clone()).unwrap();
    let rename = |n: &String| renamed.get(n).cloned().unwrap_or_else(|| n.clone());
    let generic: Vec<Bone> = source.iter().map(|b| Bone { name: rename(&b.name), ..b.clone() }).collect();
    let generic_weighted: Vec<String> = weighted.iter().map(rename).collect();
    assert_eq!(secondary::breast_bones(&generic, &generic_weighted, &roles), breasts.iter().map(rename).collect::<Vec<_>>());
    // Ribbons on the chest are a mirrored pair in front of the chest too, but
    // their names make them ribbons (SiViolet's L/R_ribbonG); renamed without
    // a part word the same bones count as breasts.
    let ribbons: BTreeMap<String, String> = serde_json::from_value(data["chest_ribbons"].clone()).unwrap();
    assert!(ribbons.keys().all(|r| !breasts.contains(r)));
    let deco = |n: &String| ribbons.get(n).cloned().unwrap_or_else(|| n.clone());
    let decorated: Vec<Bone> = source.iter().map(|b| Bone { name: deco(&b.name), ..b.clone() }).collect();
    let mut with_deco = breasts.clone();
    with_deco.extend(ribbons.values().cloned());
    with_deco.sort();
    assert_eq!(secondary::breast_bones(&decorated, &weighted.iter().map(deco).collect::<Vec<_>>(), &roles), with_deco);
    // Butts are found by name and as a low mirrored pair behind the hips; they
    // never count as breasts and move only with butt motion on.
    let butts: Vec<String> = serde_json::from_value(data["butt_bones"].clone()).unwrap();
    assert_eq!(secondary::butt_bones(&source, &weighted, &roles), butts);
    assert!(butts.iter().all(|b| !breasts.contains(b)));
    let with_butts: Vec<String> = serde_json::from_value(data["expected_roots_with_butt_motion"].clone()).unwrap();
    let moving = secondary::candidates_motion(&source, &weighted, &roles, &roles, None, Some(true)).unwrap();
    assert_eq!(secondary::candidates_motion(&source, &weighted, &roles, &roles, None, Some(false)).unwrap(), groups);
    assert_eq!(moving.iter().map(|g| g.root.clone()).collect::<Vec<_>>(), with_butts);
    assert_eq!(moving.iter().find(|g| g.root == "Butt_L_00").unwrap().source_bones.len(), 2);
    for name in data["butt_names"]["butt"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        assert!(secondary::named_butt(name) && !secondary::named_breast(name), "{name}");
    }
    for name in data["butt_names"]["not_butt"].as_array().unwrap() {
        assert!(!secondary::named_butt(name.as_str().unwrap()), "{name}");
    }
}

#[test]
fn saved_butt_groups_stay_on_the_hips_while_butt_motion_is_off() {
    let data: Value = serde_json::from_str(include_str!("fixtures/hanging-chains.json")).unwrap();
    let source: Vec<Bone> = serde_json::from_value(data["bones"].clone()).unwrap();
    let weighted: Vec<String> = serde_json::from_value(data["weighted"].clone()).unwrap();
    let roles = data["roles"].clone();
    let role_names: Vec<String> = serde_json::from_value::<BTreeMap<String, String>>(roles.clone()).unwrap().into_values().collect();
    let target: Vec<Bone> = source.iter().filter(|b| role_names.contains(&b.name)).cloned().collect();
    let with_butts = data["expected_roots_with_butt_motion"].clone();
    let mut options = deadlock_project::PreparationOptions::default();
    options.values.insert("preserve_physics_bones".into(), true.into());
    options.values.insert("physics_roots".into(), with_butts);
    options.values.insert("source_roles".into(), roles.clone());
    options.values.insert("target_roles".into(), roles);
    // Off (new models get this at their first inspection): the butt roots are
    // dropped, not reported as stale.
    options.values.insert("butt_motion".into(), false.into());
    let plan = secondary::selection(&source, &weighted, &target, &options).unwrap().unwrap();
    let expected: Vec<String> = serde_json::from_value(data["expected_roots"].clone()).unwrap();
    assert_eq!(plan.groups.iter().map(|g| g.root.clone()).collect::<Vec<_>>(), expected);
    options.values.insert("butt_motion".into(), true.into());
    let plan = secondary::selection(&source, &weighted, &target, &options).unwrap().unwrap();
    assert_eq!(plan.groups.len(), expected.len() + 4);
    options.values.insert("butt_motion".into(), "yes".into());
    assert!(secondary::selection(&source, &weighted, &target, &options).unwrap_err().to_string().contains("Butt motion must be on or off"));
}

#[test]
fn charms_hang_from_the_gun_and_follow_its_fit_or_leave_with_it() {
    use deadlock_preparation::weapon;
    let data: Value = serde_json::from_str(include_str!("fixtures/hanging-chains.json")).unwrap();
    let source: Vec<Bone> = serde_json::from_value(data["bones"].clone()).unwrap();
    let weighted: Vec<String> = serde_json::from_value(data["weighted"].clone()).unwrap();
    let roles: BTreeMap<String, String> = serde_json::from_value(data["roles"].clone()).unwrap();
    let gun = data["gun"].as_str().unwrap();
    let charms: Vec<String> = serde_json::from_value(data["gun_charm_roots"].clone()).unwrap();
    let expected: Vec<String> = serde_json::from_value(data["expected_roots"].clone()).unwrap();
    let groups = secondary::candidates(&source, &weighted, &roles, &roles).unwrap();
    let on_gun: Vec<_> = groups.iter().filter(|g| secondary::on_gun(&g.root, &source, &roles, gun)).map(|g| g.root.clone()).collect();
    assert_eq!(on_gun, charms);
    // The charm keeps all its links; slide, magazine, a sideways fin and a tag on the hand stay put.
    let link = groups.iter().find(|g| g.root == "Pistol_Link_00").unwrap();
    assert_eq!((link.source_anchor.as_str(), link.source_bones.len()), ("Bip001_L_Hand", 3));
    assert!(groups.iter().all(|g| !g.source_bones.iter().any(|b| ["Pistol_Slide", "Pistol_Mag", "Pistol_Fin", "Glove_Tag"].iter().any(|p| b.starts_with(p)))));
    // Without the inspection's gun marks only the named parts move.
    let unmarked: Vec<Bone> = source.iter().map(|b| { let mut b = b.clone(); b.details.remove("weapon"); b }).collect();
    let named: Vec<_> = secondary::candidates(&unmarked, &weighted, &roles, &roles).unwrap().into_iter().map(|g| g.root).collect();
    assert_eq!(named, expected.iter().filter(|r| !charms.contains(r)).cloned().collect::<Vec<_>>());
    // Rigid accessory words lose to charm words; charm names do not need a gun.
    assert!(!secondary::rigid_named("Earring_Dangle_00") && secondary::rigid_named("Earring_00"));
    assert!(secondary::charm_named("Key_Chain_00") && secondary::charm_named("KeyRing") && secondary::charm_named("キーホルダー") && !secondary::charm_named("Keystone"));
    // A replaced gun carries its charms; a removed gun takes them along.
    let mut options = PreparationOptions::default();
    options.values.insert("preserve_physics_bones".into(), json!(true));
    options.values.insert("physics_roots".into(), json!(expected));
    options.values.insert("source_roles".into(), json!(roles));
    options.values.insert("target_roles".into(), json!(roles));
    let mut target: Vec<Bone> = serde_json::from_value(json!([{"name":"weapon","parent":"Bip001_L_Hand","position":[0.5,0,1.09]}])).unwrap();
    target.extend(source.iter().filter(|b| roles.values().any(|r| r == &b.name)).cloned());
    let plan = weapon::Plan {
        policy: weapon::POLICY.into(), objects: vec![gun.into()], action: "replace".into(),
        matrix: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]],
        skin: weapon::Skin::Rigid { bone: "weapon".into() }, mesh_prefix: "weapon_".into(), replace: vec![], anchor: "weapon".into(), ..Default::default()
    };
    let kept = secondary::selection_for(&source, &weighted, &target, &options, Some(&plan)).unwrap().unwrap();
    assert_eq!(kept.groups.len(), expected.len());
    for group in &kept.groups {
        assert_eq!(group.target_anchor == "weapon", charms.contains(&group.root), "{}", group.root);
    }
    // Without a gun choice the charms stay on the hand, as when the gun is kept in the hand.
    let hand = secondary::selection(&source, &weighted, &target, &options).unwrap().unwrap();
    assert!(hand.groups.iter().filter(|g| charms.contains(&g.root)).all(|g| g.target_anchor == "Bip001_L_Hand"));
    // Another candidate's plan does not move them.
    let other = weapon::Plan { objects: vec!["Sword".into()], ..plan.clone() };
    assert_eq!(secondary::selection_for(&source, &weighted, &target, &options, Some(&other)).unwrap(), Some(hand.clone()));
    let removed = weapon::Plan { action: "remove".into(), anchor: String::new(), ..plan.clone() };
    let left = secondary::selection_for(&source, &weighted, &target, &options, Some(&removed)).unwrap().unwrap();
    assert_eq!(left.groups.iter().map(|g| g.root.clone()).collect::<Vec<_>>(), expected.iter().filter(|r| !charms.contains(r)).cloned().collect::<Vec<_>>());
    let mut only = options.clone();
    only.values.insert("physics_roots".into(), json!(charms));
    assert_eq!(secondary::selection_for(&source, &weighted, &target, &only, Some(&removed)).unwrap(), None);
    let unanchored = weapon::Plan { anchor: String::new(), ..plan };
    assert!(secondary::selection_for(&source, &weighted, &target, &options, Some(&unanchored)).is_err());
}

#[test]
fn inspection_marks_the_bones_of_offered_guns() {
    use deadlock_preparation::weapon;
    let mut bones: Vec<Bone> = serde_json::from_value(json!([
        {"name":"hand_R","parent":null,"position":[0,0,0]},
        {"name":"Gun","parent":"hand_R","position":[0,0,0]},
        {"name":"Charm_0","parent":"Gun","position":[0,0,-0.02]},
        {"name":"Hair","parent":null,"position":[0,0,1]}
    ])).unwrap();
    let candidate: weapon::Candidate = serde_json::from_value(json!({"id":"Pistol","objects":["Pistol"],"vertices":40,"bounds":[[0,0,0],[1,1,1]],"sample":[],
        "attach_bone":null,"attach_kind":"skinned","bones":["Gun","Charm_0"]})).unwrap();
    weapon::mark_gun_bones(&mut bones, &[candidate.clone(), candidate]);
    assert_eq!(weapon::gun_marks(&bones), BTreeMap::from([("Charm_0".to_string(), vec!["Pistol".to_string()]), ("Gun".to_string(), vec!["Pistol".to_string()])]));
    assert!(bones[3].details.get("weapon").is_none());
}

#[test]
fn breast_bones_carrying_other_parts_still_count_as_existing_breasts() {
    // MMD-style rig (as in Yvonne): 胸上2.L/R carry the breast skin and also
    // hold ribbon and cape strips, so they are not single breasts, but the rig
    // must never be given extra breast bones.
    let source: Vec<Bone> = serde_json::from_value(json!([
        {"name":"pelvis","parent":null,"position":[0,0,0.92]},
        {"name":"chest","parent":"pelvis","position":[0,0,1.19]},
        {"name":"head","parent":"chest","position":[0,0,1.47]},
        {"name":"胸上2.L","parent":"chest","position":[0.06,-0.05,1.25]},
        {"name":"胸先.L","parent":"胸上2.L","position":[0.07,-0.1,1.24]},
        {"name":"ribbon_L_01","parent":"胸上2.L","position":[0.08,-0.14,1.28]},
        {"name":"ribbon_L_02","parent":"ribbon_L_01","position":[0.08,-0.15,1.23]},
        {"name":"胸上2.R","parent":"chest","position":[-0.06,-0.05,1.25]},
        {"name":"ribbon_R_01","parent":"胸上2.R","position":[-0.08,-0.14,1.28]},
        {"name":"ribbon_R_02","parent":"ribbon_R_01","position":[-0.08,-0.15,1.23]},
        {"name":"breast_L_01_jnt","parent":"chest","position":[0.04,-0.05,1.25]}
    ])).unwrap();
    let weighted: Vec<String> = ["pelvis","chest","head","胸上2.L","ribbon_L_01","ribbon_L_02","胸上2.R","ribbon_R_01","ribbon_R_02"].map(String::from).to_vec();
    let roles = BTreeMap::from([("pelvis".into(),"pelvis".into()),("chest".into(),"chest".into()),("head".into(),"head".into())]);
    assert!(secondary::breast_bones(&source, &weighted, &roles).is_empty());
    // The unweighted breast_L_01_jnt does not count; the weighted 胸上2 bones do.
    assert_eq!(secondary::breast_named_bones(&source, &weighted), vec!["胸上2.L".to_string(), "胸上2.R".to_string()]);
    assert!(secondary::breast_request(&source, &roles).is_some());
}

/// The bone layout of a friend's build (one model on Shiv), read from its VPK in
/// hero space (+X forward, Z up). Its non-English names were stripped by the
/// compiler, so only positions identify the parts: two breast chains on the upper
/// chest (a tip bone 4.4 in from the root), two butt pairs just behind and below
/// the pelvis, and boot bones under the feet. The butt must never be a breast.
#[test]
fn a_real_layout_keeps_breasts_on_the_chest_and_butts_on_the_hips() {
    let bone = |name: &str, parent: Option<&str>, p: [f64; 3]| serde_json::json!({"name": name, "parent": parent, "position": p});
    let mut bones = vec![
        bone("pelvis", None, [0., 0., 61.5]), bone("spine_0", Some("pelvis"), [0.3, 0., 62.6]),
        bone("spine_1", Some("spine_0"), [-0.7, 0., 70.4]), bone("spine_2", Some("spine_1"), [-2., 0., 76.5]),
        bone("spine_3", Some("spine_2"), [-1.3, 0., 81.8]), bone("neck_0", Some("spine_3"), [-0.6, 0., 85.1]),
        bone("head", Some("neck_0"), [1.5, 0., 89.3]),
    ];
    for (side, y) in [("L", 1.), ("R", -1.)] {
        bones.extend([
            bone(&format!("B2_{side}"), Some("spine_3"), [8.5, 3.1 * y, 77.5]), bone(&format!("B_{side}"), Some(&format!("B2_{side}")), [8.4, 3.4 * y, 73.1]),
            bone(&format!("U1_{side}"), Some("pelvis"), [-0.6, 5.2 * y, 59.0]), bone(&format!("U1t_{side}"), Some(&format!("U1_{side}")), [0.1, 5.2 * y, 58.3]),
            bone(&format!("U2_{side}"), Some("pelvis"), [-1.0, 5.5 * y, 57.2]), bone(&format!("U2t_{side}"), Some(&format!("U2_{side}")), [-0.3, 5.5 * y, 56.6]),
            bone(&format!("ankle_{side}"), Some("pelvis"), [-1., 14.7 * y, 5.]), bone(&format!("ball_{side}"), Some(&format!("ankle_{side}")), [3., 14.7 * y, 1.7]),
            // A boot strap hanging backwards from the ankle, longer than the toe.
            bone(&format!("Boots_{side}"), Some(&format!("ankle_{side}")), [-8., 14.7 * y, 4.]),
        ]);
    }
    let source: Vec<Bone> = serde_json::from_value(serde_json::Value::Array(bones)).unwrap();
    let weighted: Vec<String> = source.iter().map(|b| b.name.clone()).filter(|n| !n.starts_with("ankle") && !n.starts_with("ball") && !n.starts_with("Boots")).collect();
    let roles: BTreeMap<String, String> = [("pelvis", "pelvis"), ("spine", "spine_0"), ("chest", "spine_2"), ("neck", "neck_0"), ("head", "head"), ("foot_L", "ankle_L"), ("foot_R", "ankle_R")]
        .into_iter().map(|(a, b)| (a.to_owned(), b.to_owned())).collect();
    assert_eq!(secondary::breast_bones(&source, &weighted, &roles), vec!["B2_L".to_string(), "B2_R".to_string()]);
    assert_eq!(secondary::butt_bones(&source, &weighted, &roles), ["U1_L", "U1_R", "U2_L", "U2_R"].map(String::from).to_vec());
}
