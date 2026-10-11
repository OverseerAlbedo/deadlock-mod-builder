//! Gun customization: reading the hero's original gun, measuring guns and fitting.
use deadlock_preparation::{algorithms::Bone, weapon};
use deadlock_skeleton::{export::{export_dmx, PreparedReport}, transforms::{prepare_target, PreparedBone}, Skeleton};
use serde_json::{json, Value};
use std::collections::BTreeMap;

type V = [f64; 3];

fn hero() -> (Skeleton, Vec<PreparedBone>) {
    let value: Value = serde_json::from_str(include_str!("fixtures/daki-rig.json")).unwrap();
    let target: Vec<PreparedBone> = serde_json::from_value(value["target"].clone()).unwrap();
    let index: BTreeMap<_, _> = target.iter().enumerate().map(|(i, b)| (b.name.clone(), i as i32)).collect();
    let skeleton = Skeleton {
        names: target.iter().map(|b| b.name.clone()).collect(),
        parents: target.iter().map(|b| b.parent.as_ref().map_or(-1, |p| index[p])).collect(),
        positions: target.iter().map(|b| b.local_position).collect(),
        rotations: target.iter().map(|b| b.local_rotation).collect(),
        scales: target.iter().map(|b| b.local_scale).collect(),
        flags: vec![0; target.len()],
    };
    let (target, _) = prepare_target(&skeleton).unwrap();
    (skeleton, target)
}
fn bones(target: &[PreparedBone]) -> Vec<Bone> {
    target
        .iter()
        .map(|b| {
            let mut bone = Bone::from(b);
            bone.details.insert("matrix".into(), serde_json::to_value(b.matrix).unwrap());
            bone
        })
        .collect()
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn dist(a: V, b: V) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
/// Box corners: centre, axis lengths along forward/up/right.
fn cuboid(centre: V, f: V, u: V, size: V) -> Vec<V> {
    let r = cross(f, u);
    let mut out = vec![];
    for a in [-0.5, 0.5] {
        for b in [-0.5, 0.5] {
            for c in [-0.5, 0.5] {
                out.push(add(add(add(centre, mul(f, a * size[0])), mul(u, b * size[1])), mul(r, c * size[2])));
            }
        }
    }
    out
}
/// Gun parts: (corners, bone). Slide/barrel on `slide_bone`, grip and magazine on `grip_bone`.
fn gun_parts(grip: V, f: V, u: V, scale: f64, slide_bone: &str, grip_bone: &str, long_barrel: bool) -> Vec<(Vec<V>, String)> {
    let barrel = if long_barrel { 0.55 } else { 0.24 };
    vec![
        (cuboid(add(add(grip, mul(f, (barrel / 2. - 0.06) * scale)), mul(u, 0.05 * scale)), f, u, [barrel * scale, 0.045 * scale, 0.035 * scale]), slide_bone.into()),
        (cuboid(add(grip, mul(u, -0.03 * scale)), f, u, [0.05 * scale, 0.12 * scale, 0.032 * scale]), grip_bone.into()),
    ]
}
fn mesh(name: &str, parts: &[(Vec<V>, String)]) -> Value {
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    let (mut positions, mut weights, mut faces, mut normals, mut uvs) = (vec![], vec![], vec![], vec![], vec![]);
    let mut corner = 0;
    for (points, bone) in parts {
        let base = positions.len();
        for p in points {
            positions.push(*p);
            weights.push(vec![(bone.clone(), 1.0)]);
        }
        for q in quads {
            faces.push(json!({"vertices": q.map(|i| base + i), "corners": [corner, corner + 1, corner + 2, corner + 3], "material": "materials/test/gun.vmat"}));
            for _ in 0..4 {
                normals.push([0., 0., 1.]);
                uvs.push([0.5, 0.5]);
            }
            corner += 4;
        }
    }
    json!({"name": name, "positions": positions, "normals": normals, "uvs": [uvs], "colors": [], "weights": weights, "faces": faces})
}

fn hero_gun_frame(target: &[Bone]) -> (V, V, V) {
    let find = |n: &str| target.iter().find(|b| b.name == n).unwrap().position;
    let grip = find("prop_hand_R");
    let muzzle = find("muzzle_fx");
    let d = [muzzle[0] - grip[0], muzzle[1] - grip[1], muzzle[2] - grip[2]];
    let n = dot(d, d).sqrt();
    let f = mul(d, 1. / n);
    let helper = [1., 0., 0.];
    let u = add(helper, mul(f, -dot(f, helper)));
    let u = mul(u, 1. / dot(u, u).sqrt());
    (grip, f, u)
}

#[test]
fn hero_gun_meshes_are_read_from_the_original_model_with_import_filters_and_lods() {
    let (skeleton, target) = hero();
    let bones = bones(&target);
    let (grip, f, u) = hero_gun_frame(&bones);
    let body = mesh("body", &[(cuboid([0., 0., 60.], [1., 0., 0.], [0., 0., 1.], [10., 30., 10.]), "pelvis".into())]);
    let gun = mesh("gun", &gun_parts(grip, f, u, 25., "weapon", "prop_hand_R", false));
    let report: PreparedReport = serde_json::from_value(json!({"meshes": [body, gun]})).unwrap();
    let base = tempfile::tempdir().unwrap();
    let file = base.path().join("models/heroes_wip/test/test_model.dmx");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    export_dmx(&report, &skeleton, &file).unwrap();
    let vmdl = r#"<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:modeldoc29:version{3cec427c-1b0e-4d48-a90a-0436f33a6041} -->
{ rootNode = { _class = "RootNode" children = [ { _class = "RenderMeshList" children = [
  { _class = "RenderMeshFile" name = "test_body" filename = "models/heroes_wip/test/test_model.dmx" import_filter = { exclude_by_default = true exception_list = [ "body" ] } },
  { _class = "RenderMeshFile" name = "test_gun" filename = "models/heroes_wip/test/test_model.dmx" import_filter = { exclude_by_default = true exception_list = [ "gun" ] } },
  { _class = "RenderMeshFile" name = "test_gun_LOD1" filename = "models/heroes_wip/test/test_model.dmx" import_filter = { exclude_by_default = true exception_list = [ "gun" ] } } ] } ] } }"#;
    let reference = weapon::reference_from_sources(base.path(), vmdl, &target, false, &deadlock_core::CancellationToken::new()).unwrap();
    assert_eq!(reference.source, "meshes");
    assert_eq!(reference.primary, vec!["test_gun".to_string()]);
    assert_eq!(reference.meshes, vec!["test_gun".to_string(), "test_gun_LOD1".to_string()]);
    assert_eq!(reference.model_meshes.len(), 3);
    assert!(reference.bones.contains(&"weapon".to_string()) && reference.bones.contains(&"prop_hand_R".to_string()));
    // Every measured point lies on the exported gun.
    assert!(reference.points.iter().all(|p| dist(*p, grip) < 25. * 0.6), "gun points away from the weapon bones");
    // A separate weapon model counts every render mesh as the weapon.
    let separate = weapon::reference_from_sources(base.path(), vmdl, &target, true, &deadlock_core::CancellationToken::new()).unwrap();
    assert_eq!(separate.primary.len(), 2);
}

#[test]
fn a_differently_shaped_gun_in_the_hand_is_fitted_onto_the_hero_gun() {
    let (_, target) = hero();
    let bones = bones(&target);
    let (grip, f, u) = hero_gun_frame(&bones);
    // Hero pistol: short barrel, 25 units; two bones (slide moves for recoil).
    let hero_parts = gun_parts(grip, f, u, 25., "weapon", "prop_hand_R", false);
    let mut points = vec![];
    let mut weights = vec![];
    for (corners, bone) in &hero_parts {
        // Densify each box so measuring sees volume, not only corners.
        for a in 0..8 {
            for b in 0..8 {
                let pa = corners[a];
                let pb = corners[b];
                for t in [0., 0.25, 0.5, 0.75] {
                    points.push(add(mul(pa, 1. - t), mul(pb, t)));
                    weights.push(vec![(bone.clone(), 1.)]);
                }
            }
        }
    }
    let reference = weapon::Reference {
        policy: weapon::POLICY.into(),
        source: "meshes".into(),
        meshes: vec!["test_gun".into()],
        points,
        weights,
        ..Default::default()
    };
    // Source: a long-barrel gun in metres, held in the right hand pointing -Y.
    let hand: V = [-0.74, 0., 1.45];
    let source_parts = gun_parts(hand, [0., -1., 0.], [0., 0., 1.], 1.0, "src", "src", true);
    let mut sample = vec![];
    for (corners, _) in &source_parts {
        for a in 0..8 {
            for b in 0..8 {
                for t in [0., 0.25, 0.5, 0.75] {
                    sample.push(add(mul(corners[a], 1. - t), mul(corners[b], t)));
                }
            }
        }
    }
    let source: Vec<Bone> = serde_json::from_value(json!([{"name":"hand_R","parent":null,"position":hand}])).unwrap();
    let roles = BTreeMap::from([("hand_R".to_string(), "hand_R".to_string())]);
    let candidate = weapon::Candidate {
        id: "Rifle".into(), objects: vec!["Rifle".into()], vertices: sample.len() as u64,
        bounds: [[-0.8, -0.6, 1.3], [-0.7, 0.1, 1.6]], sample, attach_bone: Some("hand_R".into()),
        attach_kind: "bone_parent".into(), keywords: vec![], elongation: 4.,
    };
    let fit = weapon::fit(&candidate, &reference, &weapon::Choice::default(), &source, &roles, &bones, None).unwrap();
    // The barrel lines up with the hero barrel, the grip sits at the hero grip.
    assert!(dot(fit.target.forward, f) > 0.97, "target barrel {:?} vs {:?}", fit.target.forward, f);
    let m = fit.matrix;
    let apply = |p: V| [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3]);
    let fitted_forward = {
        let a = apply(hand);
        let b = apply(add(hand, [0., -1., 0.]));
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        mul(d, 1. / dot(d, d).sqrt())
    };
    assert!(dot(fitted_forward, f) > 0.97, "fitted barrel {fitted_forward:?}");
    let up_fitted = {
        let a = apply(hand);
        let b = apply(add(hand, [0., 0., 1.]));
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        mul(d, 1. / dot(d, d).sqrt())
    };
    assert!(dot(up_fitted, fit.target.up) > 0.9, "gun upside down: {up_fitted:?} vs {:?}", fit.target.up);
    assert!(dist(apply(hand), fit.target.grip) < 1e-6);
    // Two weapon bones in the original -> weights are copied, not one rigid bone.
    assert_eq!(fit.skin, "transfer");
    assert_eq!(fit.skin_bones, vec!["prop_hand_R".to_string(), "weapon".to_string()]);
    // The longer gun is scaled to the original length; user scale multiplies it.
    let bigger = weapon::fit(&candidate, &reference, &weapon::Choice { scale: 1.5, ..Default::default() }, &source, &roles, &bones, None).unwrap();
    assert!((bigger.scale / fit.scale - 1.5).abs() < 1e-9);
    // Flip and roll are exact rotations about the fitted gun.
    let flipped = weapon::fit(&candidate, &reference, &weapon::Choice { flip: true, ..Default::default() }, &source, &roles, &bones, None).unwrap();
    let fm = flipped.matrix;
    let flip_forward = [0, 1, 2].map(|i| fm[i][1] * -1.);
    assert!(dot(flip_forward, f) < -0.97 * flipped.scale);
    let rigid = weapon::fit(&candidate, &reference, &weapon::Choice { skin: "rigid".into(), ..Default::default() }, &source, &roles, &bones, None).unwrap();
    assert_eq!(rigid.skin, "rigid");
    let plan = weapon::plan(&fit, &candidate, &reference, "replace");
    assert_eq!(plan.replace, vec!["test_gun".to_string()]);
    assert!(matches!(plan.skin, weapon::Skin::Transfer { .. }));
}

#[test]
fn heroes_without_gun_meshes_use_their_weapon_and_muzzle_bones() {
    let (_, target) = hero();
    let bones = bones(&target);
    let reference = weapon::Reference { policy: weapon::POLICY.into(), source: "skeleton".into(), ..Default::default() };
    let frame = weapon::target_frame(&reference, &bones).unwrap();
    let find = |n: &str| bones.iter().find(|b| b.name == n).unwrap().position;
    assert!(dist(frame.grip, find("prop_hand_R")) < 1e-9);
    assert!(dist(frame.muzzle, find("muzzle_fx")) < 1e-9);
    assert_eq!(frame.confidence, "low");
    assert_eq!(weapon::weapon_root(&bones).as_deref(), Some("weapon"));
    // TangTang-style rigs carry a weapon_bone under the hand.
    let value: Value = serde_json::from_str(include_str!("fixtures/tangtang-rig.json")).unwrap();
    let other: Vec<Bone> = serde_json::from_value(value["target"].clone()).unwrap();
    assert_eq!(weapon::weapon_root(&other).as_deref(), Some("weapon_bone"));
    let weapon_bones = weapon::weapon_bones(&other);
    assert!(weapon_bones.contains("muzzle_fx") && !weapon_bones.contains("hand_R"));
}

#[test]
fn choices_are_validated_and_mesh_mappings_replace_every_gun_lod() {
    let mut options = deadlock_project::PreparationOptions::default();
    options.values.insert("weapon".into(), json!({"enabled": true, "action": "explode"}));
    assert!(weapon::choice(&options).is_err());
    options.values.insert("weapon".into(), json!({"enabled": true, "action": "replace", "scale": 0}));
    assert!(weapon::choice(&options).is_err());
    options.values.insert("weapon".into(), json!({"enabled": true, "action": "replace", "objects": ["Gun"]}));
    let choice = weapon::choice(&options).unwrap();
    assert_eq!(choice.scale, 1.);
    let mappings = weapon::mesh_mappings(&["weapon_Gun".into(), "weapon_Mag".into()], &["gun".into(), "gun_lod1".into()]);
    assert_eq!(mappings.len(), 2);
    assert_eq!(serde_json::to_value(&mappings["gun_lod1"]).unwrap(), json!(["weapon_Gun", "weapon_Mag"]));
}
