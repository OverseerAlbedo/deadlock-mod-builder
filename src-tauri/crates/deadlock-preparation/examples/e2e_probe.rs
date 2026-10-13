//! Development aid: plan a Blender preparation job from a real inspection and a
//! captured hero target, then validate Blender's result with the native exporter.
//!
//!   e2e_probe plan INSPECTION TARGET_FIXTURE WORKSPACE MODE[hero|weapon] ACTION[replace|remove|keep|none] FIT[match_target|preserve_proportions]
//!   e2e_probe validate INSPECTION TARGET_FIXTURE OPTIONS REPORT OUTPUT.dmx
//!   e2e_probe orient INSPECTION      (prints the upright rotation or null)
//!   e2e_probe hand INSPECTION        (prints {"hand": right hand, "left_hand": left hand} that hold a gun file)
//!
//! The hero gun reference here is synthetic (built at the hero's weapon bones);
//! production measures the decompiled original gun meshes instead.
use deadlock_preparation::{algorithms::Bone, automap, secondary_bones, service::{Inspection, PreparedResponse}, structure, weapon};
use deadlock_project::PreparationOptions;
use deadlock_skeleton::{export::{export_dmx, PreparedReport}, transforms::{prepare_target, PreparedBone}, Skeleton};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    serde_json::from_slice(&fs::read(path).expect("read input")).expect("decode input")
}
fn skeleton(target: &[PreparedBone]) -> Skeleton {
    let index: BTreeMap<_, _> = target.iter().enumerate().map(|(i, b)| (b.name.clone(), i as i32)).collect();
    Skeleton {
        names: target.iter().map(|b| b.name.clone()).collect(),
        parents: target.iter().map(|b| b.parent.as_ref().map_or(-1, |p| index[p])).collect(),
        positions: target.iter().map(|b| b.local_position).collect(),
        rotations: target.iter().map(|b| b.local_rotation).collect(),
        scales: target.iter().map(|b| b.local_scale).collect(),
        flags: vec![0; target.len()],
    }
}
fn canonical(fixture: &str) -> (Skeleton, Vec<PreparedBone>) {
    let value: Value = read(fixture);
    let target: Vec<PreparedBone> = serde_json::from_value(value["target"].clone()).expect("PreparedBone target");
    let skeleton = skeleton(&target);
    let (target, _) = prepare_target(&skeleton).expect("canonical target");
    (skeleton, target)
}
fn bones_with_matrix(target: &[PreparedBone]) -> Vec<Bone> {
    target
        .iter()
        .map(|b| {
            let mut bone = Bone::from(b);
            bone.details.insert("matrix".into(), serde_json::to_value(b.matrix).unwrap());
            bone
        })
        .collect()
}
/// A pistol-shaped point cloud at the hero's weapon bones, split over two bones.
fn synthetic_reference(target: &[Bone]) -> weapon::Reference {
    let find = |n: &str| target.iter().find(|b| b.name == n).map(|b| b.position);
    let muzzle = find("muzzle_fx").expect("muzzle bone");
    let grip = find("prop_hand_R").or_else(|| find("weapon_bone")).expect("grip bone");
    let d: Vec<f64> = (0..3).map(|i| muzzle[i] - grip[i]).collect();
    let reach = (d.iter().map(|v| v * v).sum::<f64>()).sqrt();
    let forward = [d[0] / reach, d[1] / reach, d[2] / reach];
    let helper = if forward[0].abs() < 0.9 { [1., 0., 0.] } else { [0., 1., 0.] };
    let dot = forward[0] * helper[0] + forward[1] * helper[1] + forward[2] * helper[2];
    let mut up = [helper[0] - forward[0] * dot, helper[1] - forward[1] * dot, helper[2] - forward[2] * dot];
    let n = (up.iter().map(|v| v * v).sum::<f64>()).sqrt();
    up = [up[0] / n, up[1] / n, up[2] / n];
    let right = [forward[1] * up[2] - forward[2] * up[1], forward[2] * up[0] - forward[0] * up[2], forward[0] * up[1] - forward[1] * up[0]];
    let size = reach / 0.18;
    let at = |f: f64, u: f64, r: f64| [0, 1, 2].map(|i| grip[i] + forward[i] * f * size + up[i] * u * size + right[i] * r * size);
    let (mut points, mut weights) = (vec![], vec![]);
    let root = weapon::weapon_root(target).unwrap();
    for i in 0..=30 {
        for (a, b) in [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)] {
            points.push(at(-0.06 + 0.24 * i as f64 / 30., 0.05 + 0.02 * a, 0.015 * b));
            weights.push(vec![(root.clone(), 1.)]);
        }
    }
    for i in 0..=12 {
        for (a, b) in [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)] {
            points.push(at(0.025 * a, -0.08 + 0.1 * i as f64 / 12., 0.015 * b));
            weights.push(vec![(root.clone(), 1.)]);
        }
    }
    weapon::Reference {
        policy: weapon::POLICY.into(),
        source: "meshes".into(),
        meshes: vec!["hero_gun".into(), "hero_gun_lod1".into()],
        primary: vec!["hero_gun".into()],
        model_meshes: vec!["hero_body".into(), "hero_gun".into(), "hero_gun_lod1".into()],
        points,
        weights,
        bones: vec![root],
        ..Default::default()
    }
}
/// A dual-wield hero (Haze): a pistol in each hand on that hand's weapon bone.
fn synthetic_dual_reference(target: &[Bone]) -> weapon::Reference {
    let find = |n: &str| target.iter().find(|b| b.name == n).map(|b| b.position).expect("hand bone");
    let gun = |hand: [f64; 3], bone: &str| -> (Vec<[f64; 3]>, Vec<Vec<(String, f64)>>) {
        let (mut points, mut weights) = (vec![], vec![]);
        // Barrel along -Y (forward), grip below the hand.
        for i in 0..=30 {
            for (a, b) in [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)] {
                points.push([hand[0] + 0.6 * b, hand[1] - 2.0 - 7.0 * i as f64 / 30., hand[2] + 1.5 + 0.6 * a]);
                weights.push(vec![(bone.to_string(), 1.)]);
            }
        }
        for i in 0..=12 {
            for (a, b) in [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)] {
                points.push([hand[0] + 0.5 * b, hand[1] - 1.0 + 0.8 * a, hand[2] + 1.5 - 3.0 * i as f64 / 12.]);
                weights.push(vec![(bone.to_string(), 1.)]);
            }
        }
        (points, weights)
    };
    let (right_points, right_weights) = gun(find("hand_R"), "weaponHand_R");
    let (left_points, left_weights) = gun(find("hand_L"), "weaponHand_L");
    let side = |name: &str, points: &[[f64; 3]], weights: &[Vec<(String, f64)>], bone: &str| weapon::Side {
        side: name.into(),
        meshes: vec![format!("hero_gun_{name}"), format!("hero_gun_{name}_lod1")],
        primary: vec![format!("hero_gun_{name}")],
        points: points.to_vec(),
        weights: weights.to_vec(),
        bones: vec![bone.into()],
        shape: Default::default(),
    };
    weapon::Reference {
        policy: weapon::POLICY.into(),
        source: "meshes".into(),
        meshes: ["right", "left"].iter().flat_map(|s| [format!("hero_gun_{s}"), format!("hero_gun_{s}_lod1")]).collect(),
        primary: vec!["hero_gun_right".into(), "hero_gun_left".into()],
        model_meshes: vec!["hero_body".into(), "hero_gun_right".into(), "hero_gun_left".into()],
        points: [right_points.clone(), left_points.clone()].concat(),
        weights: [right_weights.clone(), left_weights.clone()].concat(),
        bones: vec!["weaponHand_L".into(), "weaponHand_R".into()],
        sides: vec![side("right", &right_points, &right_weights, "weaponHand_R"), side("left", &left_points, &left_weights, "weaponHand_L")],
        ..Default::default()
    }
}
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("plan") => {
            let mut inspection: Inspection = read(&a[1]);
            let (_, target) = canonical(&a[2]);
            let workspace = Path::new(&a[3]);
            let mode = a[4].as_str();
            let action = a[5].as_str();
            let fit_mode = a.get(6).map(String::as_str).unwrap_or("match_target");
            let target_bones = bones_with_matrix(&target);
            let mut complete = PreparationOptions::default();
            complete.values.insert("complete_mapping".into(), true.into());
            let suggestions = automap::suggest(&inspection.bones, &inspection.weighted_bones, &target_bones, &complete).expect("suggest");
            let source_roles = suggestions.source.as_ref().map(|s| s.roles.clone()).unwrap_or_default();
            if mode != "weapon" {
                // As the service does: the offered guns mark their bones, so charms on them are found.
                let offered: Vec<_> = inspection.weapons.iter().filter(|c| c.gun_file || weapon::score(c, &inspection.bones, &source_roles) >= 2.5).cloned().collect();
                weapon::mark_gun_bones(&mut inspection.bones, &offered);
            }
            let mut options = json!({
                "reviewed": true, "follow_motion_reviewed": true, "fit_mode": fit_mode, "arm_skinning": "smooth_twist",
                "source_roles": source_roles, "target_roles": suggestions.target.roles, "mapping": suggestions.mapping,
                "mapping_anchors": suggestions.assistance.as_ref().map(|a| a.anchors.clone()).unwrap_or_default(),
            });
            if std::env::var_os("DMB_PROBE_PHYSICS").is_some() {
                // Same default as the UI: keep every detected moving part.
                // DMB_PROBE_BREASTS: breast motion on, as when the UI checkbox is ticked.
                let breasts = std::env::var_os("DMB_PROBE_BREASTS").map(|_| true);
                let groups = secondary_bones::candidates_with(&inspection.bones, &inspection.weighted_bones, &source_roles, &suggestions.target.roles, breasts).expect("moving parts");
                if breasts.is_some() { options["breast_motion"] = true.into(); }
                eprintln!("moving parts: {:?}", groups.iter().map(|g| (&g.root, &g.source_anchor, g.source_bones.len())).collect::<Vec<_>>());
                if !groups.is_empty() {
                    options["preserve_physics_bones"] = true.into();
                    options["automatic_secondary_motion"] = true.into();
                    options["physics_roots"] = groups.iter().map(|g| g.root.clone()).collect::<Vec<_>>().into();
                }
            }
            let mut plan = None;
            if action != "none" {
                // Same choice and plan as the app: the gun file first (each hand of a pair), then a clear candidate.
                // DMB_PROBE_DUAL: a dual-wield hero (a gun in each hand); DMB_PROBE_LEFT=keep|copy changes its left hand.
                let reference = if mode == "weapon" {
                    weapon::Reference { separate_model: true, ..synthetic_reference(&target_bones) }
                } else if std::env::var_os("DMB_PROBE_DUAL").is_some() {
                    synthetic_dual_reference(&target_bones)
                } else {
                    synthetic_reference(&target_bones)
                };
                let offered: Vec<_> = if mode == "weapon" {
                    inspection.weapons.clone()
                } else {
                    inspection.weapons.iter().filter(|c| c.gun_file || weapon::score(c, &inspection.bones, &source_roles) >= 2.5).cloned().collect()
                };
                let cx = weapon::Context { candidates: &offered, reference: Some(&reference), source: &inspection.bones, roles: &source_roles, target: &target_bones, body_scale: None, weapon_model: mode == "weapon" };
                let mut choice = weapon::suggest_choice(&cx).expect("gun candidate");
                choice.action = action.into();
                match std::env::var("DMB_PROBE_LEFT").as_deref() {
                    Ok("keep") => choice.left.keep = true,
                    Ok("copy") => choice.left.objects.clear(),
                    _ => {}
                }
                // DMB_PROBE_MERGE=0: keep the gun's parts as separate meshes.
                if std::env::var("DMB_PROBE_MERGE").as_deref() == Ok("0") {
                    choice.merge = false;
                }
                options["weapon"] = serde_json::to_value(&choice).unwrap();
                plan = weapon::plan_choice(&choice, &cx).expect("weapon plan");
                for hand in plan.iter().flat_map(weapon::Plan::hands) {
                    eprintln!("gun {} {:?} -> replaces {:?}, anchor {}, copy {}, merge {}", hand.side, hand.objects.len(), hand.replace, hand.anchor, hand.copy, hand.merge);
                }
            }
            if let Some(shape) = a.get(7).filter(|s| !s.is_empty()) {
                options["shape_key_mode"] = shape.as_str().into();
            }
            let inspect_job: Value = read(&Path::new(&a[1]).with_file_name("job.json").to_string_lossy());
            let job = json!({
                "operation": "prepare", "source": inspection_source(&a[1]), "mode": if mode == "weapon" { "weapon" } else { "rigged_humanoid" },
                "target": target, "hero_root": "models/heroes_wip/test", "options": options, "rotation": inspect_job["rotation"].clone(),
                "correction": false, "weapon": plan, "gun_bones": weapon::gun_marks(&inspection.bones), "breast_bones": inspect_job["breast_bones"].clone(),
                "weapon_source": inspect_job["weapon_source"].clone(),
                "result": workspace.join("result.json"), "workspace": workspace,
            });
            fs::create_dir_all(workspace).unwrap();
            fs::write(workspace.join("job.json"), serde_json::to_vec_pretty(&job).unwrap()).unwrap();
            fs::write(workspace.join("options.json"), serde_json::to_vec_pretty(&options).unwrap()).unwrap();
            println!("{}", workspace.join("job.json").display());
        }
        Some("orient") => {
            let inspection: Inspection = read(&a[1]);
            let named = automap::roles(&inspection.bones, &inspection.weighted_bones, &Default::default()).expect("roles");
            match structure::orientation(&inspection.bones, &inspection.weighted_bones, &named.roles) {
                Some(turn) => println!("{}", serde_json::to_string(&turn.rotation).unwrap()),
                None => println!("null"),
            }
        }
        Some("hand") => {
            // The bones the service asks Blender to put the gun file in (the left hand holds a pair's left gun).
            let inspection: Inspection = read(&a[1]);
            let roles = if inspection.bones.is_empty() {
                BTreeMap::new()
            } else {
                automap::roles(&inspection.bones, &inspection.weighted_bones, &Default::default()).expect("roles").roles
            };
            println!("{}", serde_json::to_string(&json!({"hand": roles.get("hand_R"), "left_hand": roles.get("hand_L")})).unwrap());
        }
        Some("validate") => {
            let mut inspection: Inspection = read(&a[1]);
            let (skeleton, target) = canonical(&a[2]);
            let options: PreparationOptions = read(&a[3]);
            let report: PreparedResponse = read(&a[4]);
            // The planned job beside the options holds the gun plan and the gun-bone marks.
            let job: Value = read(&Path::new(&a[3]).with_file_name("job.json").to_string_lossy());
            let gun: Option<weapon::Plan> = serde_json::from_value(job["weapon"].clone()).expect("weapon plan");
            let marks: BTreeMap<String, Vec<String>> = serde_json::from_value(job.get("gun_bones").cloned().unwrap_or(json!({}))).expect("gun bones");
            for bone in &mut inspection.bones {
                if let Some(ids) = marks.get(&bone.name) {
                    bone.details.insert("weapon".into(), json!(ids));
                }
            }
            let plan = secondary_bones::selection_for(&inspection.bones, &inspection.weighted_bones, &target.iter().map(Bone::from).collect::<Vec<_>>(), &options, gun.as_ref()).expect("secondary plan");
            if let Some(plan) = &plan {
                let preserved: Option<secondary_bones::PreservationPlan> = serde_json::from_value(report.algorithm_audits.get("preserved_physics_bones").cloned().unwrap_or(Value::Null)).expect("preserved bones");
                assert_eq!(preserved.as_ref(), Some(plan), "Blender and the native selection disagree");
                eprintln!("moving parts: {:?}", plan.groups.iter().map(|g| (&g.root, &g.target_anchor)).collect::<Vec<_>>());
            }
            let extended = secondary_bones::export_skeleton(&skeleton, &target, &report.target, &inspection.bones, plan.as_ref()).expect("unchanged original binds");
            let receipt = export_dmx(&PreparedReport { meshes: report.meshes.clone(), extra: BTreeMap::new() }, &extended, Path::new(&a[5])).expect("native export and bind checks");
            println!("validated {} meshes, {} bones, policy {}; skinning {}", report.meshes.len(), extended.names.len(), report.rest_alignment.policy, serde_json::to_string(&receipt.skinning).unwrap());
        }
        _ => panic!("usage: plan|validate ..."),
    }
}
fn inspection_source(path: &str) -> String {
    // The probe stores the source path beside the inspection as job.json.
    let job: Value = read(&Path::new(path).with_file_name("job.json").to_string_lossy());
    job["source"].as_str().unwrap().to_string()
}
