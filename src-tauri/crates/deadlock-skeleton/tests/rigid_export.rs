//! The unskinned model-22 DMX of a rigid ability object.
use deadlock_dmx::{Dmx, Value};
use deadlock_skeleton::export::{axis_system, export_rigid_bytes, PreparedFace, PreparedMesh};
use std::collections::BTreeMap;

/// A unit square of two triangles plus a third triangle with another material.
fn mesh() -> PreparedMesh {
    let positions = vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.], [0., 0., 1.]];
    let face = |v: [usize; 3], start: usize, material: &str| PreparedFace { vertices: v.to_vec(), corners: (start..start + 3).collect(), material: material.into() };
    PreparedMesh {
        name: "object".into(),
        positions,
        normals: vec![[0., 0., 1.]; 9],
        uvs: vec![vec![[0., 0.], [1., 0.], [1., 1.], [0., 0.], [1., 1.], [0., 1.], [0., 0.], [1., 0.], [0., 1.]]],
        colors: vec![],
        tangents: vec![],
        weights: vec![],
        faces: vec![face([0, 1, 2], 0, "models/x/a.vmat"), face([0, 2, 3], 3, "models/x/a.vmat"), face([0, 1, 4], 6, "models/x/b.vmat")],
        extra: BTreeMap::new(),
    }
}

#[test]
fn a_rigid_object_is_written_without_joints_with_the_original_axis_system() {
    let raw = export_rigid_bytes(&[mesh()], Some((3, 1, 0))).unwrap();
    let dmx = Dmx::parse(raw.clone()).unwrap();
    assert_eq!(dmx.geometry_counts().unwrap().source_triangles, 3);
    assert_eq!(axis_system(&dmx), Some((3, 1, 0)));
    let kinds: Vec<&str> = dmx.elements.iter().map(|e| e.kind.as_str()).collect();
    assert!(!kinds.contains(&"DmeJoint"), "no skeleton: {kinds:?}");
    assert_eq!(kinds.iter().filter(|k| **k == "DmeFaceSet").count(), 2, "one face set per material");
    let state = dmx.elements.iter().find(|e| e.kind == "DmeVertexData").unwrap();
    assert!(matches!(state.get("jointCount"), Some(Value::Integer(0))));
    assert!(state.get("texcoord$0").is_some() && state.get("jointWeights").is_none());
    // The same input writes the same bytes.
    assert_eq!(export_rigid_bytes(&[mesh()], Some((3, 1, 0))).unwrap(), raw);
    // Without an axis system none is written.
    let plain = Dmx::parse(export_rigid_bytes(&[mesh()], None).unwrap()).unwrap();
    assert_eq!(axis_system(&plain), None);
}

#[test]
fn a_one_bone_object_is_bound_wholly_to_its_bone_with_the_original_axis_system() {
    use deadlock_skeleton::{export::{export_bytes, export_bytes_with_axis, PreparedReport}, Skeleton};
    // One bone turned half a turn about (1, 0, 1) / √2, at the origin (a placed object's bone).
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let skeleton = Skeleton { names: vec!["joint1".into()], parents: vec![-1], positions: vec![[0.; 3]], rotations: vec![[h, 0., h, 0.]], scales: vec![1.], flags: vec![] };
    let bound = PreparedMesh { weights: vec![vec![("joint1".to_owned(), 1.)]; 5], ..mesh() };
    let report = PreparedReport { meshes: vec![bound], extra: BTreeMap::new() };
    let (raw, audit) = export_bytes_with_axis(&report, &skeleton, Some((3, 1, 0))).unwrap();
    assert!(audit.skinning.numeric_checks_passed);
    assert_eq!(audit.dmx.bones, ["joint1"]);
    let dmx = Dmx::parse(raw).unwrap();
    assert_eq!(axis_system(&dmx), Some((3, 1, 0)));
    assert_eq!(dmx.elements.iter().filter(|e| e.kind == "DmeJoint").count(), 1);
    // Without an axis system it writes exactly what export_bytes writes.
    assert_eq!(export_bytes_with_axis(&report, &skeleton, None).unwrap().0, export_bytes(&report, &skeleton).unwrap().0);
    // A weight to another bone is refused.
    let stray = PreparedReport { meshes: vec![PreparedMesh { weights: vec![vec![("other".to_owned(), 1.)]; 5], ..mesh() }], extra: BTreeMap::new() };
    assert!(export_bytes_with_axis(&stray, &skeleton, None).is_err());
}

#[test]
fn a_rigid_object_needs_geometry_and_no_weights() {
    assert!(export_rigid_bytes(&[], None).is_err());
    assert!(export_rigid_bytes(&[PreparedMesh { normals: vec![], ..mesh() }], None).is_err());
    assert!(export_rigid_bytes(&[PreparedMesh { weights: vec![vec![("root".into(), 1.)]; 5], ..mesh() }], None).is_err());
    let mut bad = mesh();
    bad.faces[0].vertices[0] = 9;
    assert!(export_rigid_bytes(&[bad], None).is_err());
}
