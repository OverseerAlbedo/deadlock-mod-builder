use deadlock_dmx::{Dmx, Rewrite, Value};
use serde_json::Value as Json;
use std::{fs, path::PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn reference_summaries_geometry_and_extraction_match_all_encodings() {
    for name in ["basic-v4", "basic-v5", "basic-v9", "unicode-v9", "split-v9"] {
        let source = Dmx::load(&fixture(&format!("{name}.dmx"))).unwrap();
        let expected: Json =
            serde_json::from_slice(&fs::read(fixture(&format!("{name}.json"))).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(source.summary().unwrap()).unwrap(),
            expected["summary"],
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(source.geometry_counts().unwrap()).unwrap(),
            expected["geometry"],
            "{name}"
        );
        for (i, mesh) in source.summary().unwrap().meshes.iter().enumerate() {
            assert_eq!(
                source.single_mesh(&mesh.name).unwrap(),
                fs::read(fixture(&format!("{name}-mesh-{i}.dmx"))).unwrap(),
                "{name} {}",
                mesh.name
            );
        }
    }
}

#[test]
fn raw_rewrite_preserves_every_type_and_matches_reference_edit() {
    for name in ["basic-v9", "split-v9", "unicode-v9", "all-types-v9"] {
        let raw = fs::read(fixture(&format!("{name}.dmx"))).unwrap();
        let source = Dmx::parse(raw.clone()).unwrap();
        assert_eq!(source.rewrite_binary9(&Rewrite::default()).unwrap(), raw);
    }
    let source = Dmx::load(&fixture("split-v9.dmx")).unwrap();
    let mut changes = Rewrite::default();
    for i in [4, 5, 10] {
        changes.names.insert(i, "衣装-piece".into());
    }
    changes
        .identities
        .insert(5, std::array::from_fn(|i| i as u8));
    assert_eq!(
        source.rewrite_binary9(&changes).unwrap(),
        fs::read(fixture("split-v9-rewritten.dmx")).unwrap()
    );
    let types = Dmx::load(&fixture("all-types-v9.dmx")).unwrap();
    assert_eq!(types.elements[0].attributes.len(), 32);
    assert!(matches!(types.elements[0].get("scalar_3"),Some(Value::Float(x)) if *x == -0.125));
    assert!(matches!(
        types.elements[0].get("array_3"),
        Some(Value::Packed {
            base: 3,
            count: 2,
            stride: 4,
            ..
        })
    ));
    assert_eq!(
        types
            .value_bytes(types.elements[0].get("scalar_6").unwrap())
            .unwrap(),
        [0, 128, 255]
    );
    assert_eq!(
        types.elements[0].get("scalar_5").unwrap().as_str(),
        Some("文字列")
    );
}

#[test]
fn every_truncated_prefix_rejected_without_panic() {
    let raw = fs::read(fixture("all-types-v9.dmx")).unwrap();
    for end in 0..raw.len() {
        assert!(Dmx::parse(raw[..end].to_vec()).is_err(), "prefix {end}");
    }
}

#[test]
fn invalid_header_counts_pool_types_refs_and_tail_fail() {
    let raw = fs::read(fixture("basic-v9.dmx")).unwrap();
    let source = Dmx::parse(raw.clone()).unwrap();
    let header = raw.iter().position(|b| *b == 0).unwrap() + 1;
    let mut cases = Vec::new();
    let mut bad = raw.clone();
    bad[header..header + 4].copy_from_slice(&1_i32.to_le_bytes());
    cases.push(bad);
    for count in [-1, 20_000_001, i32::MAX] {
        let mut bad = raw.clone();
        bad[header + 4..header + 8].copy_from_slice(&count.to_le_bytes());
        cases.push(bad);
    }
    let mut bad = raw.clone();
    bad[header + 8] = 255;
    cases.push(bad);
    let attr = &source.elements[0].attributes[0];
    let mut bad = raw.clone();
    bad[attr.raw.start - 1] = 255;
    cases.push(bad);
    for index in [-2_i32, 9999] {
        let mut bad = raw.clone();
        bad[attr.raw.start + 4..attr.raw.start + 8].copy_from_slice(&index.to_le_bytes());
        cases.push(bad);
    }
    let mut bad = raw.clone();
    bad.extend(b"garbage");
    cases.push(bad);
    for (i, bad) in cases.into_iter().enumerate() {
        assert!(Dmx::parse(bad).is_err(), "case {i}");
    }
    let mut allowed = raw;
    allowed.extend(b"\0\n\r\t ");
    assert!(Dmx::parse(allowed).is_ok());
}

#[test]
fn rewrite_rejects_invalid_intent_and_does_not_touch_source() {
    let source = Dmx::load(&fixture("split-v9.dmx")).unwrap();
    let original = source.bytes().to_vec();
    let mut edits = Rewrite::default();
    edits
        .attributes
        .insert((0, "does-not-exist".into()), vec![]);
    assert!(source.rewrite_binary9(&edits).is_err());
    let mut edits = Rewrite::default();
    edits.names.insert(0, "bad\0name".into());
    assert!(source.rewrite_binary9(&edits).is_err());
    let mut edits = Rewrite::default();
    edits
        .attributes
        .insert((5, "bindState".into()), 99999_i32.to_le_bytes().to_vec());
    assert!(source.rewrite_binary9(&edits).is_err());
    assert!(source.single_mesh("absent").is_err());
    assert_eq!(source.bytes(), original);
    assert!(Dmx::load(&fixture("basic-v4.dmx"))
        .unwrap()
        .rewrite_binary9(&Rewrite::default())
        .is_err());
}

#[test]
fn duplicate_meshes_fail_summary_and_bad_polygons_fail_geometry() {
    let mut source = Dmx::load(&fixture("basic-v9.dmx")).unwrap();
    let names: Vec<_> = source
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "DmeMesh")
        .map(|(i, _)| i)
        .collect();
    source.elements[names[1]].name = source.elements[names[0]].name.clone();
    assert!(source.summary().is_err());
    let source = Dmx::load(&fixture("split-v9.dmx")).unwrap();
    let mut edits = Rewrite::default();
    edits.attributes.insert(
        (7, "faces".into()),
        [1_i32.to_le_bytes(), (-1_i32).to_le_bytes()].concat(),
    );
    let changed = Dmx::parse(source.rewrite_binary9(&edits).unwrap()).unwrap();
    assert!(changed.geometry_counts().is_err());
}

#[test]
fn unicode_path_load_uses_native_paths() {
    let folder = std::env::temp_dir().join(format!("dmb-dmx-{}-衣装", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("模型 оружие.dmx");
    fs::copy(fixture("unicode-v9.dmx"), &path).unwrap();
    let loaded = Dmx::load(&path).unwrap();
    assert_eq!(loaded.summary().unwrap().meshes[0].name, "衣装");
    fs::remove_file(path).unwrap();
    fs::remove_dir(folder).unwrap();
}

#[test]
fn mistyped_face_reference_and_cancelled_large_work_are_errors() {
    let raw = fs::read(fixture("split-v9.dmx")).unwrap();
    let source = Dmx::parse(raw.clone()).unwrap();
    let attribute = source.elements[5]
        .attributes
        .iter()
        .find(|a| a.name == "faceSets")
        .unwrap();
    let mut malformed = raw;
    malformed[attribute.raw.start - 1] = 34;
    malformed[attribute.raw.start + 4..attribute.raw.start + 8]
        .copy_from_slice(&99999_i32.to_le_bytes());
    let parsed = Dmx::parse(malformed).unwrap();
    assert!(parsed.geometry_counts().is_err());
    let cancel = deadlock_core::CancellationToken::new();
    cancel.cancel();
    assert!(Dmx::parse_with_cancel(source.bytes().to_vec(), &cancel).is_err());
    assert!(matches!(
        source.summary_with_cancel(&cancel).unwrap_err().into_core(),
        deadlock_core::CoreError::Cancelled
    ));
    assert!(matches!(
        source
            .geometry_counts_with_cancel(&cancel)
            .unwrap_err()
            .into_core(),
        deadlock_core::CoreError::Cancelled
    ));
    assert!(Dmx::load_with_cancel(&fixture("split-v9.dmx"), &cancel).is_err());
}

#[test]
fn malformed_bit_mutations_do_not_panic_in_source_analysis() {
    let raw = fs::read(fixture("split-v9.dmx")).unwrap();
    for offset in 0..raw.len() {
        let mut malformed = raw.clone();
        malformed[offset] ^= 0xff;
        if let Ok(source) = Dmx::parse(malformed) {
            let _ = source.summary();
            let _ = source.geometry_counts();
        }
    }
}
