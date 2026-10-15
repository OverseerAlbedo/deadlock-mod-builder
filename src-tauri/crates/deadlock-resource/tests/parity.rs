use deadlock_resource::{
    binary, container,
    text::{field_edit, Document, Edit},
    Annotation, Kind, Value,
};
use sha2::{Digest, Sha256};
mod common;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn manifest() -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(fixtures().join("manifest.json")).unwrap()).unwrap()
}

#[test]
fn python_binary_encoder_and_compression_parity() {
    for row in manifest()["fixtures"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let source = std::fs::read(fixtures().join(format!("{name}.kv3"))).unwrap();
        let document = binary::decode(&source).unwrap_or_else(|e| panic!("{name}: {e}"));
        let encoded = binary::encode(&document.root, document.format_guid).unwrap();
        assert_eq!(
            encoded,
            std::fs::read(fixtures().join(format!("{name}.v1"))).unwrap(),
            "{name}"
        );
        assert!(binary::decode(&encoded)
            .unwrap()
            .root
            .canonical_eq(&document.root));
    }
}

#[test]
fn accepted_r12_native_every_supported_ordinal_matches_python() {
    // The native resources are the compiled Vindicta references: private assets, skipped with a note when absent.
    for resource in manifest()["native"].as_array().unwrap() {
        let name = resource["path"].as_str().unwrap();
        let Some(file) = common::private_path(name) else { continue };
        let raw = std::fs::read(&file).unwrap();
        assert_eq!(hash(&raw), resource["sha256"].as_str().unwrap());
        let layout = container::layout(&raw).unwrap();
        for row in resource["blocks"].as_array().unwrap() {
            let ordinal = row["ordinal"].as_u64().unwrap() as usize;
            let block = &layout.blocks[ordinal];
            let payload = container::payload(&raw, block).unwrap();
            assert_eq!(hash(payload), row["sha256"].as_str().unwrap());
            let document = binary::decode(payload)
                .unwrap_or_else(|e| panic!("{name} {} #{ordinal}: {e}", block.name));
            let encoded = binary::encode(&document.root, document.format_guid).unwrap();
            assert_eq!(
                hash(&encoded),
                row["encoded_sha256"].as_str().unwrap(),
                "{name} {} #{ordinal}",
                block.name
            );
        }
        assert_eq!(
            hash(&std::fs::read(&file).unwrap()),
            resource["sha256"].as_str().unwrap()
        );
    }
}

#[test]
fn malformed_headers_footer_trailing_nonfinite_and_annotations_fail_closed() {
    let good = std::fs::read(fixtures().join("all-values.kv3")).unwrap();
    for end in 0..good.len() {
        assert!(binary::decode(&good[..end]).is_err(), "truncation at {end}");
    }
    let mut bad = good.clone();
    bad.push(0);
    assert!(binary::decode(&bad).is_err());
    let mut bad = good.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(binary::decode(&bad).is_err());
    let mut bad = good.clone();
    bad[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(binary::decode(&bad).is_err());
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(binary::encode(&Value::new(Kind::F64(value)), [0; 16]).is_err());
    }
    assert!(Annotation::try_from(6).is_err());
    let blob = std::fs::read(fixtures().join("v5-chained-blobs.kv3")).unwrap();
    for offset in [60, 68, blob.len() - 4] {
        let mut bad = blob.clone();
        bad[offset..offset + 4].copy_from_slice(&3u32.to_le_bytes());
        assert!(binary::decode(&bad).is_err(), "offset {offset}");
    }
    let zstd = std::fs::read(fixtures().join("v5-scalars-2.kv3")).unwrap();
    for cut in 1..=4 {
        assert!(binary::decode(&zstd[..zstd.len() - cut]).is_err());
    }
}

#[test]
fn native_empty_animation_blob_has_a_required_footer() {
    let raw = include_bytes!("fixtures/item-crouch-idle-empty-blob.vnmclip_c");
    assert_eq!(
        hash(raw),
        "180340722ac5764ab3cdfe8ca4f4a3fbd7bc48d44bb6fd583df77e767a01d1ab"
    );
    let layout = container::layout(raw).unwrap();
    let data = container::payload(raw, container::unique(&layout.blocks, "DATA").unwrap()).unwrap();
    let document = binary::decode(data).unwrap();
    assert_eq!(
        document
            .root
            .get("m_nNumFrames")
            .unwrap()
            .integer()
            .unwrap(),
        1
    );
    assert!(
        matches!(&document.root.get("m_compressedPoseData").unwrap().kind, Kind::Blob(bytes) if bytes.is_empty())
    );
    let encoded = binary::encode(&document.root, document.format_guid).unwrap();
    assert!(binary::decode(&encoded)
        .unwrap()
        .root
        .canonical_eq(&document.root));
    // Consume and validate the marker, never allow arbitrary trailing bytes or
    // an absent/corrupt marker merely because the referenced blob is empty.
    for cut in 1..=4 {
        assert!(binary::decode(&data[..data.len() - cut]).is_err());
    }
    let mut bad = data.to_vec();
    *bad.last_mut().unwrap() ^= 1;
    assert!(binary::decode(&bad).is_err());
    let mut bad = data.to_vec();
    bad.push(0);
    assert!(binary::decode(&bad).is_err());
}

#[test]
fn empty_blob_footer_is_checked_for_uncompressed_and_zstd_streams() {
    let raw = include_bytes!("fixtures/item-crouch-idle-empty-blob.vnmclip_c");
    let layout = container::layout(raw).unwrap();
    let data = container::payload(raw, container::unique(&layout.blocks, "DATA").unwrap()).unwrap();
    let expected = binary::decode(data).unwrap();
    let word = |at| u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
    let packed0 = word(76);
    let packed1 = word(84);
    let stream0 = lz4_flex::block::decompress(&data[120..120 + packed0], word(72)).unwrap();
    let stream1 =
        lz4_flex::block::decompress(&data[120 + packed0..120 + packed0 + packed1], word(80))
            .unwrap();
    for method in [0u32, 2] {
        let (first, second, blob) = if method == 0 {
            (stream0.clone(), stream1.clone(), vec![])
        } else {
            (
                zstd::bulk::compress(&stream0, 1).unwrap(),
                zstd::bulk::compress(&stream1, 1).unwrap(),
                zstd::bulk::compress(&[], 1).unwrap(),
            )
        };
        let mut other = data[..120].to_vec();
        other[20..24].copy_from_slice(&method.to_le_bytes());
        other[24..28].fill(0);
        for (at, n) in [
            (52, first.len() + second.len() + blob.len()),
            (76, first.len()),
            (84, second.len()),
        ] {
            other[at..at + 4].copy_from_slice(&(n as u32).to_le_bytes());
        }
        other.extend(first);
        other.extend(second);
        other.extend(blob);
        other.extend(0xffeedd00u32.to_le_bytes());
        assert!(binary::decode(&other)
            .unwrap()
            .root
            .canonical_eq(&expected.root));
        let mut bad = other.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(binary::decode(&bad).is_err());
        assert!(binary::decode(&other[..other.len() - 4]).is_err());
        other.push(0);
        assert!(binary::decode(&other).is_err());
    }
}

#[test]
fn exact_numeric_domain_order_and_annotations_are_not_tolerated() {
    let mut a = Value::new(Kind::Object(vec![("x".into(), Value::new(Kind::F32(0.1)))]));
    let b = Value::new(Kind::Object(vec![(
        "x".into(),
        Value::new(Kind::F64(0.1f32 as f64)),
    )]));
    assert!(a.canonical_eq(&b));
    a.get_mut("x").unwrap().kind = Kind::F64(0.1);
    assert!(!a.canonical_eq(&b));
    assert!(!Value::new(Kind::F64(-0.0)).canonical_eq(&Value::new(Kind::F64(0.0))));
    let mut annotated = b.clone();
    annotated.annotation = Annotation::Resource;
    assert!(!b.canonical_eq(&annotated));
}

#[test]
fn lossless_text_edits_match_python_and_preserve_opaque_content() {
    let source = std::fs::read_to_string(fixtures().join("lossless.vmdl")).unwrap();
    let doc = Document::parse(source.clone()).unwrap();
    let nodes = doc.objects(Some("RenderMeshFile"));
    assert_eq!(nodes.len(), 1);
    let node = nodes[0];
    let output = doc
        .apply(&[
            field_edit(node, "filename", "新しい model.dmx").unwrap(),
            field_edit(node, "added", "value").unwrap(),
        ])
        .unwrap();
    assert_eq!(
        output,
        std::fs::read_to_string(fixtures().join("lossless-edited.vmdl")).unwrap()
    );
    assert_eq!(doc.apply(&[]).unwrap(), source);
    assert!(doc
        .apply(&[
            Edit {
                start: 0,
                end: 5,
                replacement: String::new()
            },
            Edit {
                start: 2,
                end: 7,
                replacement: String::new()
            }
        ])
        .is_err());
    for invalid in [
        "{ x=1 x=2 }",
        "{ blob=#[ 0 ] }",
        "{ x=[ }",
        "{ x=\"unterminated }",
        "{} trailing",
    ] {
        assert!(Document::parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn container_rebuild_matches_untouched_payloads_and_rejects_trailing_data() {
    // The accepted Vindicta reference is a private asset; the test skips with a note without it.
    let Some(raw) = common::private_asset("physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c") else { return };
    let layout = container::layout(&raw).unwrap();
    let phys = container::unique(&layout.blocks, "PHYS").unwrap();
    let mut replacements = BTreeMap::new();
    replacements.insert(
        "PHYS".into(),
        container::payload(&raw, phys).unwrap().to_vec(),
    );
    let rebuilt = container::rebuild(&raw, &replacements).unwrap();
    let next = container::layout(&rebuilt).unwrap();
    assert_eq!(layout.blocks.len(), next.blocks.len());
    for (a, b) in layout.blocks.iter().zip(&next.blocks) {
        assert_eq!(a.name, b.name);
        assert_eq!(
            container::payload(&raw, a).unwrap(),
            container::payload(&rebuilt, b).unwrap()
        );
    }
    let mut invalid = raw;
    invalid.push(0);
    assert!(container::layout(&invalid).is_err());
}
#[test]
fn canonical_hash_uses_exact_typed_equality_domain() {
    use deadlock_resource::value::canonical_sha256;
    let pairs = [
        (Kind::I64(3), Kind::U64(3)),
        (Kind::F32(0.94), Kind::F64(0.94_f32 as f64)),
        (Kind::F32(-0.0), Kind::F64(-0.0)),
    ];
    for (a, b) in pairs {
        let (a, b) = (Value::new(a), Value::new(b));
        assert!(a.canonical_eq(&b));
        assert_eq!(canonical_sha256(&a).unwrap(), canonical_sha256(&b).unwrap());
    }
    let a = Value::new(Kind::F64(0.0));
    let b = Value::new(Kind::F64(-0.0));
    assert_ne!(canonical_sha256(&a).unwrap(), canonical_sha256(&b).unwrap());
    let a = Value::new(Kind::Blob(b"abc".to_vec()));
    let b = Value::new(Kind::String("abc".into()));
    assert_ne!(canonical_sha256(&a).unwrap(), canonical_sha256(&b).unwrap());
    let mut b = a.clone();
    b.annotation = Annotation::Resource;
    assert_ne!(canonical_sha256(&a).unwrap(), canonical_sha256(&b).unwrap());
}
