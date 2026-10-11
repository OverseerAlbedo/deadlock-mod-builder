use deadlock_core::{CancellationToken, TimingRecorder};
use deadlock_dmx::Dmx;
use deadlock_preview::{from_dmx, BufferSlice, Metadata};
use serde_json::Value;
use std::path::Path;
fn floats(data: &[u8], slice: &BufferSlice) -> Vec<f64> {
    data[slice.offset..slice.offset + slice.count * 4]
        .chunks_exact(4)
        .map(|x| f32::from_le_bytes(x.try_into().unwrap()) as f64)
        .collect()
}
fn numbers(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| {
            row.as_array()
                .map(|a| a.iter().map(|x| x.as_f64().unwrap()).collect())
                .unwrap_or_else(|| vec![row.as_f64().unwrap()])
        })
        .collect()
}
#[test]
fn agrees_with_reference_bind_rest_geometry() {
    for name in [
        "basic",
        "scaled",
        "translated",
        "unskinned",
        "generated_normals",
        "concave",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}.dmx"));
        let before = std::fs::read(&path).unwrap();
        let dmx = Dmx::load(&path).unwrap();
        let preview = from_dmx(
            &dmx,
            path.display().to_string(),
            &CancellationToken::default(),
        )
        .unwrap();
        let expected: Value =
            serde_json::from_slice(&std::fs::read(path.with_extension("json")).unwrap()).unwrap();
        for (mesh, reference) in preview
            .metadata
            .meshes
            .iter()
            .zip(expected["meshes"].as_array().unwrap())
        {
            assert_eq!(mesh.name, reference["name"]);
            assert_eq!(mesh.material, reference["material"]);
            for (label, slice) in [
                ("positions", &mesh.positions),
                ("normals", &mesh.normals),
                ("uvs", mesh.uvs.as_ref().unwrap()),
            ] {
                let actual = floats(&preview.data, slice);
                let expected = numbers(&reference[label]);
                assert_eq!(actual.len(), expected.len(), "{name} {label}");
                for (a, b) in actual.iter().zip(expected) {
                    assert!((a - b).abs() < 1e-6, "{name} {label}: {a} != {b}");
                }
            }
            if name == "unskinned" {
                assert!(mesh.skin.is_none());
            } else {
                let skin = mesh
                    .skin
                    .as_ref()
                    .expect("source skin weights are retained");
                let packed = floats(&preview.data, &skin.data);
                assert_eq!(packed.len(), skin.width * skin.height * 4);
                for vertex in 0..mesh.vertex_count {
                    let mut values = std::collections::BTreeMap::new();
                    for slot in 0..skin.influences {
                        let offset = (vertex * skin.influences + slot) * 2;
                        if packed[offset + 1] != 0. {
                            values.insert(
                                skin.joint_names[packed[offset] as usize].as_str(),
                                packed[offset + 1],
                            );
                        }
                    }
                    assert_eq!(values.get("pelvis"), Some(&(0.4f32 as f64)));
                    assert_eq!(values.get("spine"), Some(&(0.6f32 as f64)));
                    assert_eq!(values.len(), 2);
                }
            }
            let indices: Vec<u32> = preview.data
                [mesh.indices.offset..mesh.indices.offset + mesh.indices.count * 4]
                .chunks_exact(4)
                .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
                .collect();
            let expected: Vec<u32> = reference["indices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as u32)
                .collect();
            assert_eq!(indices, expected, "{name} triangulation");
        }
        assert_eq!(
            preview.metadata.bones.len(),
            expected["bones"].as_array().unwrap().len()
        );
        for (bone, reference) in preview
            .metadata
            .bones
            .iter()
            .zip(expected["bones"].as_array().unwrap())
        {
            assert_eq!(bone.name, reference["name"]);
            assert_eq!(bone.parent as i64, reference["parent"].as_i64().unwrap());
            for (a, b) in bone
                .position
                .iter()
                .zip(reference["position"].as_array().unwrap())
            {
                assert!(
                    (a - b.as_f64().unwrap()).abs() < 1e-9,
                    "{name} bone {}",
                    bone.name
                );
            }
        }
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "preview modified input"
        );
        let packet = preview.packet(&CancellationToken::default()).unwrap();
        assert_eq!(&packet[..4], b"DMBP");
        let jsonlen = u32::from_le_bytes(packet[8..12].try_into().unwrap()) as usize;
        let metadata: Metadata = serde_json::from_slice(&packet[16..16 + jsonlen]).unwrap();
        assert_eq!(metadata.source_sha256, preview.metadata.source_sha256);
        assert_eq!(&packet[16 + ((jsonlen + 3) & !3)..], preview.data);
    }
}
#[test]
fn cancellation_has_no_partial_packet() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/basic.dmx");
    let token = CancellationToken::default();
    token.cancel();
    let clock = TimingRecorder::new("preview");
    assert!(matches!(
        deadlock_preview::load_packet(&path, &token, &clock),
        Err(deadlock_core::CoreError::Cancelled)
    ));
}
