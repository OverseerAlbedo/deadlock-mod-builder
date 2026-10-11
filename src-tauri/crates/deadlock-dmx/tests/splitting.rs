use deadlock_core::{safety, CancellationToken};
use deadlock_dmx::split::{self, Layout, MeshMapping};
use deadlock_dmx::{Dmx, Rewrite};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/split")
}
fn run(name: &str, destination: &Path) -> split::SplitReport {
    let folder = fixtures().join(name);
    let expected: Value =
        serde_json::from_slice(&fs::read(folder.join("expected.json")).unwrap()).unwrap();
    let target = expected["report"]["target"].as_u64().unwrap() as usize;
    let mesh = expected["report"]["groups"][0]["source_mesh"]
        .as_str()
        .unwrap();
    split::split_file(
        &folder.join("source.dmx"),
        &[mesh.into()],
        destination,
        target,
        &CancellationToken::new(),
    )
    .unwrap()
}

#[test]
fn all_piece_bytes_reports_cache_identity_and_reuse_match_python() {
    for name in [
        "normal-6",
        "normal-9",
        "loose-many",
        "loose-none",
        "unicode",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let report = run(name, temp.path());
        let directory = report.directory.as_ref().unwrap();
        let folder = fixtures().join(name);
        let expected: Value =
            serde_json::from_slice(&fs::read(folder.join("expected.json")).unwrap()).unwrap();
        assert_eq!(
            directory.file_name().unwrap().to_str().unwrap(),
            expected["cache_key"].as_str().unwrap(),
            "{name}"
        );
        let mut actual = serde_json::to_value(&report).unwrap();
        actual.as_object_mut().unwrap().remove("source_file");
        actual.as_object_mut().unwrap().remove("directory");
        assert_eq!(actual, expected["report"], "{name}");
        for p in &report.groups[0].pieces {
            assert_eq!(
                fs::read(directory.join(&p.file)).unwrap(),
                fs::read(folder.join(&p.file)).unwrap(),
                "{name} {}",
                p.name
            );
            assert_eq!(
                fs::read(directory.join(&p.corner_map)).unwrap(),
                fs::read(folder.join(&p.corner_map)).unwrap()
            );
        }
        assert_eq!(
            serde_json::to_value(run(name, temp.path())).unwrap(),
            serde_json::to_value(report).unwrap()
        );
    }
}

#[test]
fn validator_rejects_geometry_weight_material_provenance_and_coverage_changes() {
    let temp = tempfile::tempdir().unwrap();
    let report = run("normal-6", temp.path());
    let source = Dmx::load(&fixtures().join("normal-6/source.dmx")).unwrap();
    let directory = report.directory.as_ref().unwrap();
    let cancel = CancellationToken::new();
    for mode in ["delete", "duplicate", "reverse", "algorithm", "loose"] {
        let mut bad = report.clone();
        match mode {
            "delete" => {
                bad.groups[0].pieces.pop();
            }
            "duplicate" => {
                let p = bad.groups[0].pieces[0].clone();
                bad.groups[0].pieces.push(p);
            }
            "reverse" => bad.groups[0].pieces.reverse(),
            "algorithm" => bad.algorithm = 2,
            "loose" => bad.groups[0].pieces[1].loose_positions.push(5),
            _ => unreachable!(),
        };
        assert!(
            split::validate_bundle(&source, &bad, directory, &cancel).is_err(),
            "{mode}"
        );
    }
    let path = directory.join(&report.groups[0].pieces[0].file);
    let raw = fs::read(&path).unwrap();
    let dmx = Dmx::parse(raw.clone()).unwrap();
    for stream in [
        "position$0",
        "normal$0",
        "texcoord$0",
        "texcoord$1",
        "tangent$0",
        "color$0",
        "blendweights$0",
        "blendindices$0",
    ] {
        let a = dmx.elements[6]
            .attributes
            .iter()
            .find(|a| a.name == stream)
            .unwrap();
        let mut bytes = dmx.raw_attribute(a).to_vec();
        bytes[4] ^= 1;
        let mut edit = Rewrite::default();
        edit.attributes.insert((6, stream.into()), bytes);
        let changed = dmx.rewrite_binary9(&edit).unwrap();
        fs::write(&path, changed).unwrap();
        let mut bad = report.clone();
        bad.groups[0].pieces[0].sha256 = safety::digest(&path).unwrap();
        assert!(
            split::validate_bundle(&source, &bad, directory, &cancel).is_err(),
            "{stream}"
        );
    }
    let mut edit = Rewrite::default();
    edit.attributes
        .insert((7, "material".into()), 12_i32.to_le_bytes().to_vec());
    fs::write(&path, dmx.rewrite_binary9(&edit).unwrap()).unwrap();
    let mut bad = report.clone();
    bad.groups[0].pieces[0].sha256 = safety::digest(&path).unwrap();
    assert!(split::validate_bundle(&source, &bad, directory, &cancel).is_err());
    fs::write(&path, raw).unwrap();
    let mut bad = report.clone();
    bad.groups[0].pieces[0].file = "../outside.dmx".into();
    assert!(split::validate_bundle(&source, &bad, directory, &cancel).is_err());
}

#[test]
fn cancellation_invalid_layout_and_cache_corruption_do_not_publish() {
    let temp = tempfile::tempdir().unwrap();
    let source = fixtures().join("normal-6/source.dmx");
    let destination = temp.path().join("generated");
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(split::split_file(&source, &["body".into()], &destination, 6, &cancel).is_err());
    assert!(!destination.exists());
    let source_dmx = Dmx::load(&source).unwrap();
    let mut malformed = source_dmx.clone();
    malformed.elements[10].name = "wrong".into();
    assert!(Layout::new(&malformed, "body", &CancellationToken::new()).is_err());
    let mut edit = Rewrite::default();
    let a = source_dmx.elements[6]
        .attributes
        .iter()
        .find(|a| a.name == "position$0")
        .unwrap();
    let mut bytes = source_dmx.raw_attribute(a).to_vec();
    bytes[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
    edit.attributes.insert((6, "position$0".into()), bytes);
    let bad = Dmx::parse(source_dmx.rewrite_binary9(&edit).unwrap()).unwrap();
    assert!(Layout::new(&bad, "body", &CancellationToken::new()).is_err());
    let report = run("normal-6", temp.path());
    fs::write(
        report
            .directory
            .as_ref()
            .unwrap()
            .join(&report.groups[0].pieces[0].file),
        b"broken",
    )
    .unwrap();
    assert!(split::split_file(
        &source,
        &["body".into()],
        temp.path(),
        6,
        &CancellationToken::new()
    )
    .is_err());
    for value in [0, 2, 524288, 600000] {
        assert!(split::validate_target(value).is_err());
    }
    assert!(split::validate_target(400000).is_ok());
}

#[test]
fn polygon_too_large_and_impossible_loose_capacity_fail_without_data_loss() {
    let temp = tempfile::tempdir().unwrap();
    let source = fixtures().join("normal-6/source.dmx");
    let before = fs::read(&source).unwrap();
    assert!(split::split_file(
        &source,
        &["body".into()],
        temp.path(),
        3,
        &CancellationToken::new()
    )
    .is_err());
    assert_eq!(fs::read(&source).unwrap(), before);
    let dmx = Dmx::parse(before).unwrap();
    let mut edit = Rewrite::default();
    edit.attributes
        .insert((8, "faces".into()), 0_i32.to_le_bytes().to_vec());
    let source = temp.path().join("one-face.dmx");
    fs::write(&source, dmx.rewrite_binary9(&edit).unwrap()).unwrap();
    let destination = temp.path().join("pieces");
    assert!(split::split_file(
        &source,
        &["body".into()],
        &destination,
        6,
        &CancellationToken::new()
    )
    .is_err());
    assert!(fs::read_dir(destination).unwrap().next().is_none());
}

#[test]
fn mappings_expand_one_to_many_preserving_keep_disable_and_order() {
    let temp = tempfile::tempdir().unwrap();
    let report = run("normal-6", temp.path());
    let inputs = BTreeMap::from([
        ("body".into(), MeshMapping::One("body".into())),
        (
            "lod".into(),
            MeshMapping::Many(vec!["body".into(), "gun".into()]),
        ),
        ("keep".into(), MeshMapping::One("@keep".into())),
        ("off".into(), MeshMapping::One("@disable".into())),
    ]);
    let result = split::expand_mapping(&inputs, &report).unwrap();
    assert_eq!(
        result["body"],
        MeshMapping::Many(vec!["body_01".into(), "body_02".into()])
    );
    assert_eq!(
        result["lod"],
        MeshMapping::Many(vec!["body_01".into(), "body_02".into(), "gun".into()])
    );
    assert_eq!(result["keep"], inputs["keep"]);
    assert_eq!(result["off"], inputs["off"]);
}
