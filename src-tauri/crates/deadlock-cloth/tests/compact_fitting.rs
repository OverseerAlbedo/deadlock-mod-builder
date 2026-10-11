//! Captured native tables, not a synthetic stand-in for compact fitting.
use deadlock_cloth::generated::fitting;
use deadlock_resource::{binary, Kind, Value};
use deadlock_skeleton::Skeleton;
use std::{collections::BTreeSet, fs, path::Path};

fn inputs() -> (Value, Vec<String>, BTreeSet<String>, BTreeSet<String>) {
    captured("compact")
}
fn captured(folder: &str) -> (Value, Vec<String>, BTreeSet<String>, BTreeSet<String>) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(folder);
    let physics = binary::decode(&fs::read(dir.join("phys.bin")).unwrap())
        .unwrap()
        .root;
    let data = binary::decode(&fs::read(dir.join("data.bin")).unwrap())
        .unwrap()
        .root;
    let skeleton = Skeleton::from_value(data.get("m_modelSkeleton").unwrap()).unwrap();
    let groups: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("groups.json")).unwrap()).unwrap();
    (
        physics.get("m_pFeModel").unwrap().clone(),
        skeleton.names,
        serde_json::from_value(groups["selected"].clone()).unwrap(),
        serde_json::from_value(groups["weighted"].clone()).unwrap(),
    )
}
#[test]
fn captured_ribbon_accepts_connected_separate_position_supports() {
    let (fe, names, selected, weighted) = captured("compact-ribbon");
    let before = format!("{fe:?}");
    let proof = fitting::audit(&fe, &names, &selected, &weighted).unwrap();
    assert_eq!(proof.compact_fitted_bones.len(), 262);
    assert_eq!(proof.separate_position_support_bones.len(), 14);
    assert_eq!(weighted.len(), 199);
    assert!(weighted.is_subset(&proof.fitted_bones()));
    assert!(proof.matrix_fitted_bones.is_empty());
    assert!(proof.max_compact_rotation_residual < 1e-5);
    assert!(proof.max_compact_offset_residual_inches < 1e-4);
    assert_eq!(before, format!("{fe:?}"));
}
fn first_mut<'a>(v: &'a mut Value, key: &str) -> &'a mut Value {
    match &mut v.get_mut(key).unwrap().kind {
        Kind::Array(a) => &mut a[0],
        _ => panic!(),
    }
}
fn first_index(v: &Value, key: &str, index: &str) -> usize {
    v.get(key).unwrap().array().unwrap()[0]
        .get(index)
        .unwrap()
        .integer()
        .unwrap() as usize
}
fn array_mut<'a>(v: &'a mut Value, key: &str) -> &'a mut Vec<Value> {
    match &mut v.get_mut(key).unwrap().kind {
        Kind::Array(a) => a,
        _ => panic!(),
    }
}
#[test]
fn separate_support_rejects_broken_ownership_constraints_and_rest_transforms() {
    let (native, names, selected, weighted) = captured("compact-ribbon");
    let fe_names = native.get("m_CtrlName").unwrap().array().unwrap();
    let node = fe_names
        .iter()
        .position(|v| v.string().unwrap() == "dmb_secondary_spring_base_L_a_01_jnt___")
        .unwrap();
    let index = |key: &str, field: &str, value: usize| {
        native
            .get(key)
            .unwrap()
            .array()
            .unwrap()
            .iter()
            .position(|v| v.get(field).unwrap().integer().unwrap() == value as i128)
            .unwrap()
    };
    let reverse = index("m_ReverseOffsets", "nBoneCtrl", node);
    let base = index("m_NodeBases", "nNode", node);
    let point = native.get("m_ReverseOffsets").unwrap().array().unwrap()[reverse]
        .get("nTargetNode")
        .unwrap()
        .integer()
        .unwrap() as usize;
    let ctrl = index("m_CtrlOffsets", "nCtrlChild", point);
    let basis_point = native.get("m_NodeBases").unwrap().array().unwrap()[base]
        .get("nNodeX0")
        .unwrap()
        .integer()
        .unwrap();
    let rod = native
        .get("m_Rods")
        .unwrap()
        .array()
        .unwrap()
        .iter()
        .position(|r| {
            let ends = r.get("nNode").unwrap().array().unwrap();
            let ends: Vec<_> = ends.iter().map(|v| v.integer().unwrap()).collect();
            ends.contains(&(point as i128)) && ends.contains(&basis_point)
        })
        .unwrap();
    for case in 0..20 {
        let mut fe = native.clone();
        match case {
            0..=2 => {
                array_mut(&mut fe, "m_ReverseOffsets")[reverse]
                    .get_mut("nTargetNode")
                    .unwrap()
                    .kind = Kind::I64(match case {
                    0 => 0,
                    1 => node as i64,
                    _ => fe_names.len() as i64,
                })
            }
            3 => array_mut(&mut fe, "m_NodeInvMasses")[point].kind = Kind::F32(0.),
            4 => {
                array_mut(&mut fe, "m_CtrlOffsets").remove(ctrl);
            }
            5 => {
                let a = array_mut(&mut fe, "m_CtrlOffsets");
                a.push(a[ctrl].clone());
            }
            6 => {
                array_mut(&mut fe, "m_CtrlOffsets")[ctrl]
                    .get_mut("nCtrlParent")
                    .unwrap()
                    .kind = Kind::I64(node as i64 - 1)
            }
            7 => array_mut(&mut fe, "m_SkelParents")[point].kind = Kind::I64(node as i64 - 1),
            8 => {
                array_mut(&mut array_mut(&mut fe, "m_CtrlOffsets")[ctrl], "vOffset")[0].kind =
                    Kind::F32(0.)
            }
            9 => {
                array_mut(&mut fe, "m_Rods").remove(rod);
            }
            10 => {
                array_mut(&mut fe, "m_Rods")[rod]
                    .get_mut("flRelaxationFactor")
                    .unwrap()
                    .kind = Kind::F32(0.)
            }
            11 => {
                array_mut(&mut fe, "m_Rods")[rod]
                    .get_mut("flMaxDist")
                    .unwrap()
                    .kind = Kind::F32(100.)
            }
            12 => {
                for k in ["flMinDist", "flMaxDist"] {
                    array_mut(&mut fe, "m_Rods")[rod].get_mut(k).unwrap().kind = Kind::F32(100.);
                }
            }
            13 => {
                array_mut(&mut fe, "m_Rods")[rod]
                    .get_mut("flWeight0")
                    .unwrap()
                    .kind = Kind::F32(0.)
            }
            14 => {
                let a = array_mut(&mut fe, "m_Rods");
                a.push(a[rod].clone());
            }
            15 => array_mut(&mut fe, "m_Rods").clear(),
            16 => {
                for (key, row) in [("m_ReverseOffsets", reverse), ("m_CtrlOffsets", ctrl)] {
                    array_mut(&mut array_mut(&mut fe, key)[row], "vOffset")[0].kind = Kind::F32(0.);
                }
            }
            17 => {
                array_mut(&mut array_mut(&mut fe, "m_NodeBases")[base], "qAdjust")[0].kind =
                    Kind::F32(0.707)
            }
            18 => {
                if let Kind::Array(a) = &mut array_mut(&mut fe, "m_InitPose")[point].kind {
                    a[0].kind = Kind::F32(f32::NAN);
                }
            }
            _ => {
                array_mut(&mut fe, "m_Rods")[rod]
                    .get_mut("flRelaxationFactor")
                    .unwrap()
                    .kind = Kind::F32(f32::NAN)
            }
        }
        assert!(
            fitting::audit(&fe, &names, &selected, &weighted).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn captured_tangtang_native_output_qualifies_every_compact_joint() {
    let (fe, names, selected, weighted) = inputs();
    assert_eq!(weighted.len(), 199);
    assert!(fe.get("m_FitMatrices").unwrap().array().unwrap().is_empty());
    assert!(fe.get("m_FitWeights").unwrap().array().unwrap().is_empty());
    let before = format!("{fe:?}");
    let proof = fitting::audit(&fe, &names, &selected, &weighted).unwrap();
    assert_eq!(proof.compact_fitted_bones.len(), 262);
    assert!(proof.separate_position_support_bones.is_empty());
    assert!(proof.matrix_fitted_bones.is_empty());
    assert!(weighted.is_subset(&proof.fitted_bones()));
    assert!(proof.max_compact_rotation_residual < 2.2e-7);
    assert!(proof.max_compact_offset_residual_inches < 1.1e-7);
    assert_eq!(before, format!("{fe:?}"));
}
#[test]
fn compact_fitting_rejects_missing_bad_duplicate_static_and_unrelated_support() {
    let (native, names, selected, weighted) = inputs();
    let point = first_index(&native, "m_ReverseOffsets", "nTargetNode");
    let bone = first_index(&native, "m_NodeBases", "nNode");
    for case in 0..16 {
        let mut fe = native.clone();
        match case {
            0 => fe.get_mut("m_NodeBases").unwrap().kind = Kind::Array(vec![]),
            1 => fe.get_mut("m_ReverseOffsets").unwrap().kind = Kind::Array(vec![]),
            2 => {
                first_mut(&mut fe, "m_NodeBases")
                    .get_mut("nNodeX0")
                    .unwrap()
                    .kind = Kind::I64(0)
            }
            3 => {
                first_mut(&mut fe, "m_NodeBases")
                    .get_mut("nNodeY1")
                    .unwrap()
                    .kind = Kind::I64(65535)
            }
            4 => {
                first_mut(&mut fe, "m_ReverseOffsets")
                    .get_mut("nTargetNode")
                    .unwrap()
                    .kind = Kind::I64(800)
            }
            5 => {
                first_mut(&mut fe, "m_ReverseOffsets")
                    .get_mut("nBoneCtrl")
                    .unwrap()
                    .kind = Kind::I64(0)
            }
            6 | 7 => {
                let key = if case == 6 {
                    "m_NodeBases"
                } else {
                    "m_ReverseOffsets"
                };
                if let Kind::Array(a) = &mut fe.get_mut(key).unwrap().kind {
                    a.push(a[0].clone());
                }
            }
            8 => {
                if let Kind::Array(a) = &mut first_mut(&mut fe, "m_NodeBases")
                    .get_mut("qAdjust")
                    .unwrap()
                    .kind
                {
                    a[0].kind = Kind::F32(0.70710236);
                }
            }
            9 => {
                if let Kind::Array(a) = &mut first_mut(&mut fe, "m_ReverseOffsets")
                    .get_mut("vOffset")
                    .unwrap()
                    .kind
                {
                    a[0].kind = Kind::F32(1.0);
                }
            }
            10 => {
                if let Kind::Array(a) = &mut fe.get_mut("m_InitPose").unwrap().kind {
                    a[point + 1] = a[point].clone();
                    a[point + 2] = a[point].clone();
                }
            }
            11 => {
                if let Kind::Array(a) = &mut fe.get_mut("m_NodeInvMasses").unwrap().kind {
                    a[point].kind = Kind::F32(0.);
                }
            }
            12 => {
                if let Kind::Array(a) = &mut first_mut(&mut fe, "m_NodeBases")
                    .get_mut("qAdjust")
                    .unwrap()
                    .kind
                {
                    a[0].kind = Kind::F32(f32::NAN);
                }
            }
            13 => {
                first_mut(&mut fe, "m_NodeBases")
                    .get_mut("nNodeX0")
                    .unwrap()
                    .kind = Kind::I64(bone as i64)
            }
            14 => fe.get_mut("m_nFirstPositionDrivenNode").unwrap().kind = Kind::I64(1000),
            _ => {
                if let Kind::Array(a) = &mut fe.get_mut("m_CtrlName").unwrap().kind {
                    a[bone].kind = Kind::String("pelvis".into());
                }
            }
        }
        assert!(
            fitting::audit(&fe, &names, &selected, &weighted).is_err(),
            "case {case}"
        );
    }
    let mut unselected = selected.clone();
    unselected.remove("dmb_secondary_HB_0_0");
    let subset = weighted.intersection(&unselected).cloned().collect();
    let err = fitting::audit(&native, &names, &unselected, &subset).unwrap_err();
    assert!(err.to_string().contains("unselected or body joint"));
}
#[test]
fn matrix_and_compact_fits_can_coexist_but_cannot_drive_the_same_joint() {
    let (mut fe, names, selected, weighted) = inputs();
    let bone = first_index(&fe, "m_NodeBases", "nNode");
    let point = first_index(&fe, "m_ReverseOffsets", "nTargetNode");
    let val = |kind| Value {
        kind,
        annotation: deadlock_resource::Annotation::Unspecified,
    };
    let integer = |n: usize| val(Kind::I64(n as i64));
    let weights = (point..point + 3)
        .map(|n| {
            val(Kind::Object(vec![
                ("nNode".into(), integer(n)),
                ("flWeight".into(), val(Kind::F32(1. / 3.))),
            ]))
        })
        .collect();
    fe.get_mut("m_FitWeights").unwrap().kind = Kind::Array(weights);
    fe.get_mut("m_FitMatrices").unwrap().kind = Kind::Array(vec![val(Kind::Object(vec![
        ("nNode".into(), integer(bone)),
        ("nEnd".into(), integer(3)),
        ("nBeginDynamic".into(), integer(0)),
    ]))]);
    assert!(fitting::audit(&fe, &names, &selected, &weighted).is_err());
    for key in ["m_NodeBases", "m_ReverseOffsets"] {
        if let Kind::Array(a) = &mut fe.get_mut(key).unwrap().kind {
            a.remove(0);
        }
    }
    let proof = fitting::audit(&fe, &names, &selected, &weighted).unwrap();
    assert_eq!(proof.compact_fitted_bones.len(), 261);
    assert_eq!(proof.matrix_fitted_bones.len(), 1);
}
