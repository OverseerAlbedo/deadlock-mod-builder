//! Captured Windows compiler data + the independently verified, user-accepted
//! anti-twist result. Transform tests do not stand in for a new game test.
use deadlock_cloth::generated::roll_lock::{self, Chain};
use deadlock_resource::{binary, Kind, Value};
use deadlock_skeleton::Skeleton;
use std::{collections::BTreeMap, fs};
mod common;
/// The captured anti-twist data (compiled TangTang cloth on Haze: private assets), or `None` with a note.
fn inputs() -> Option<(Value, Skeleton, Vec<Chain>, serde_json::Value)> {
    let root = common::private_path("fixtures/anti-twist/phys.bin")?.parent().unwrap().to_path_buf();
    let physics = binary::decode(&fs::read(root.join("phys.bin")).unwrap())
        .unwrap()
        .root;
    let data = binary::decode(&fs::read(root.join("data.bin")).unwrap())
        .unwrap()
        .root;
    let sk = Skeleton::from_value(data.get("m_modelSkeleton").unwrap()).unwrap();
    let chains = ["L", "R"]
        .into_iter()
        .map(|side| Chain {
            anchor: "spine_3".into(),
            per_joint_reference: false,
            bones: (1..=7)
                .map(|n| format!("dmb_secondary_spring_base_{side}_a_{n:02}_jnt"))
                .collect(),
        })
        .collect();
    Some((
        physics.get("m_pFeModel").unwrap().clone(),
        sk,
        chains,
        serde_json::from_slice(&fs::read(root.join("accepted-plan.json")).unwrap()).unwrap(),
    ))
}
fn array_mut<'a>(v: &'a mut Value, key: &str) -> &'a mut Vec<Value> {
    match &mut v.get_mut(key).unwrap().kind {
        Kind::Array(a) => a,
        _ => panic!(),
    }
}
fn num(v: &Value) -> f64 {
    match v.kind {
        Kind::F32(n) => n as f64,
        Kind::F64(n) => n,
        _ => panic!(),
    }
}
#[test]
fn reproduces_only_the_fourteen_accepted_orientation_rows() {
    let Some((fe, sk, chains, plan)) = inputs() else { return };
    let patch = roll_lock::apply(&fe, &sk, &chains).unwrap();
    assert_eq!(patch.proof.chains.len(), 2);
    assert_eq!(patch.proof.chains[0].reference, [91, 92]);
    assert_eq!(patch.proof.chains[1].reference, [94, 95]);
    assert!(patch
        .proof
        .chains
        .iter()
        .all(|c| c.minimum_pole_angle_degrees > 89.));
    for (key, value) in fe.object().unwrap() {
        if key != "m_NodeBases" {
            assert!(value.canonical_eq(patch.fe.get(key).unwrap()), "{key}");
        }
    }
    let changed = plan["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["after"]["nNode"].as_u64().unwrap() as i128, v))
        .collect::<BTreeMap<_, _>>();
    let before = fe.get("m_NodeBases").unwrap().array().unwrap();
    let after = patch.fe.get("m_NodeBases").unwrap().array().unwrap();
    let mut count = 0;
    for (a, b) in before.iter().zip(after) {
        let node = a.get("nNode").unwrap().integer().unwrap();
        if let Some(expected) = changed.get(&node) {
            count += 1;
            for key in ["nNodeX0", "nNodeX1", "nNodeY0", "nNodeY1"] {
                assert_eq!(
                    b.get(key).unwrap().integer().unwrap(),
                    expected["after"][key].as_i64().unwrap() as i128
                );
            }
            let q = b.get("qAdjust").unwrap().array().unwrap();
            let want = expected["after"]["qAdjust"].as_array().unwrap();
            let dot = q
                .iter()
                .zip(want)
                .map(|(x, y)| num(x) * y.as_f64().unwrap())
                .sum::<f64>()
                .abs();
            assert!((1. - dot).abs() < 2e-7);
        } else {
            assert!(a.canonical_eq(b));
        }
    }
    assert_eq!(count, 14);
}
#[test]
fn arbitrary_names_translation_and_scale_keep_the_same_attachment_choice() {
    let Some((fe, mut sk, mut chains, _)) = inputs() else { return };
    let baseline = roll_lock::apply(&fe, &sk, &chains).unwrap();
    let mut renamed = fe.clone();
    let names = fe.get("m_CtrlName").unwrap().array().unwrap();
    let table = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.string().unwrap().to_owned(), format!("control_{i:04}")))
        .collect::<BTreeMap<_, _>>();
    for v in array_mut(&mut renamed, "m_CtrlName") {
        v.kind = Kind::String(table[v.string().unwrap()].clone());
    }
    for n in &mut sk.names {
        if let Some(new) = table.get(n) {
            *n = new.clone();
        }
    }
    for c in &mut chains {
        c.anchor = table[&c.anchor].clone();
        for n in &mut c.bones {
            *n = table[n].clone();
        }
    }
    for p in array_mut(&mut renamed, "m_InitPose") {
        if let Kind::Array(a) = &mut p.kind {
            for k in 0..3 {
                a[k] = Value::new(Kind::F64(num(&a[k]) * 2.5 + [11., -7., 3.][k]));
            }
        }
    }
    let patch = roll_lock::apply(&renamed, &sk, &chains).unwrap();
    assert_eq!(
        baseline
            .proof
            .chains
            .iter()
            .map(|c| c.reference)
            .collect::<Vec<_>>(),
        patch
            .proof
            .chains
            .iter()
            .map(|c| c.reference)
            .collect::<Vec<_>>()
    );
    let a = baseline.fe.get("m_NodeBases").unwrap().array().unwrap();
    let b = patch.fe.get("m_NodeBases").unwrap().array().unwrap();
    for (a, b) in a.iter().zip(b) {
        for key in ["nNode", "nNodeX0", "nNodeX1", "nNodeY0", "nNodeY1"] {
            assert!(a.get(key).unwrap().canonical_eq(b.get(key).unwrap()));
        }
    }
}
#[test]
fn unrelated_attachments_owners_disconnected_or_overlapping_chains_are_rejected() {
    let Some((fe, sk, chains, _)) = inputs() else { return };
    let mut bad = chains.clone();
    bad[0].anchor = "head".into();
    assert!(roll_lock::apply(&fe, &sk, &bad).is_err());
    let mut bad = chains.clone();
    bad[0].bones[3] = chains[1].bones[3].clone();
    assert!(roll_lock::apply(&fe, &sk, &bad).is_err());
    let mut bad = chains.clone();
    bad.push(chains[0].clone());
    assert!(roll_lock::apply(&fe, &sk, &bad).is_err());
    let mut wrong = fe.clone();
    array_mut(&mut wrong, "m_SkelParents")[208] = Value::new(Kind::I64(1107));
    assert!(roll_lock::apply(&wrong, &sk, &chains).is_err());
    let mut disconnected = fe.clone();
    *disconnected.get_mut("m_Rods").unwrap() = Value::new(Kind::Array(vec![]));
    assert!(roll_lock::apply(&disconnected, &sk, &chains).is_err());
}

#[test]
fn curved_strand_can_choose_well_conditioned_references_per_joint() {
    let Some((mut fe, sk, mut chains, _)) = inputs() else { return };
    chains.truncate(1);
    let base = roll_lock::apply(&fe, &sk, &chains).unwrap();
    let reference = base.proof.chains[0].reference;
    let poses = fe.get("m_InitPose").unwrap().array().unwrap();
    let p = |i: usize| {
        let a = poses[i].array().unwrap();
        [num(&a[0]), num(&a[1]), num(&a[2])]
    };
    let origin = p(reference[0]);
    let x = p(reference[1]);
    let x = std::array::from_fn::<_, 3, _>(|i| x[i] - origin[i]);
    let len = x.iter().map(|v| v * v).sum::<f64>().sqrt();
    let x = x.map(|v| v / len);
    // Trace segments perpendicular to each of a fixed triangle's three edges.
    // No one reference can serve the entire curve, yet each segment has an
    // independently suitable reference. This does not relax the 60° gate.
    let nodes = fe.get("m_CtrlName").unwrap().array().unwrap();
    let reverse = fe.get("m_ReverseOffsets").unwrap().array().unwrap();
    let points = chains[0]
        .bones
        .iter()
        .map(|name| {
            let n = nodes
                .iter()
                .position(|n| n.string().unwrap() == name)
                .unwrap();
            reverse
                .iter()
                .find(|r| r.get("nBoneCtrl").unwrap().integer().unwrap() == n as i128)
                .unwrap()
                .get("nTargetNode")
                .unwrap()
                .integer()
                .unwrap() as usize
        })
        .collect::<Vec<_>>();
    let start = p(points[0]);
    let parents = fe.get("m_SkelParents").unwrap().array().unwrap();
    let fixed_parent = parents[reference[0]].integer().unwrap();
    let static_count = fe.get("m_nRotLockStaticNodes").unwrap().integer().unwrap() as usize;
    let mut linked = std::collections::BTreeSet::new();
    let mut todo = vec![points[0]];
    let rods = fe.get("m_Rods").unwrap().array().unwrap();
    while let Some(at) = todo.pop() {
        if !linked.insert(at) {
            continue;
        }
        for rod in rods {
            let n = rod.get("nNode").unwrap().array().unwrap();
            let a = n[0].integer().unwrap() as usize;
            let b = n[1].integer().unwrap() as usize;
            if a == at {
                todo.push(b);
            } else if b == at {
                todo.push(a);
            }
        }
    }
    let third = linked
        .into_iter()
        .find(|&i| {
            i < static_count
                && !reference.contains(&i)
                && parents[i].integer().unwrap() == fixed_parent
        })
        .unwrap();
    let other = p(third);
    let along = std::array::from_fn::<_, 3, _>(|i| other[i] - origin[i]);
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let normal = cross(x, along);
    let mut z = cross(normal, x);
    let len = z.iter().map(|v| v * v).sum::<f64>().sqrt();
    z = z.map(|v| v / len);
    // An exact equilateral static reference makes this synthetic curve
    // independent of the aspect ratio in the accepted streamer fixture.
    let Kind::Array(p) = &mut array_mut(&mut fe, "m_InitPose")[third].kind else {
        panic!()
    };
    for k in 0..3 {
        p[k] = Value::new(Kind::F64(origin[k] + 0.5 * x[k] + 0.75f64.sqrt() * z[k]));
    }
    let Kind::Array(p) = &mut array_mut(&mut fe, "m_InitPose")[reference[1]].kind else {
        panic!()
    };
    for k in 0..3 {
        p[k] = Value::new(Kind::F64(origin[k] + x[k]));
    }
    let mut at = start;
    for (j, &point) in points.iter().enumerate() {
        if j > 0 {
            let angle = std::f64::consts::PI / 6. + (j - 1) as f64 * std::f64::consts::PI / 3.;
            for k in 0..3 {
                at[k] += 2. * (angle.cos() * x[k] + angle.sin() * z[k]);
            }
        }
        let Kind::Array(p) = &mut array_mut(&mut fe, "m_InitPose")[point].kind else {
            panic!()
        };
        for k in 0..3 {
            p[k] = Value::new(Kind::F64(at[k]));
        }
    }
    assert!(roll_lock::apply(&fe, &sk, &chains).is_err());
    chains[0].per_joint_reference = true;
    let patch = roll_lock::apply(&fe, &sk, &chains).unwrap();
    assert!(patch.proof.chains[0].minimum_pole_angle_degrees >= 60.);
    assert!(patch.proof.chains[0]
        .joint_references
        .windows(2)
        .any(|r| r[0] != r[1]));
}
