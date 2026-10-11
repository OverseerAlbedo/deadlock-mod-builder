//! Exact Python byte oracles for masks, subdivision, overlays and motion maps.
use deadlock_cloth_authoring::{
    motion::{self, MotionProfile},
    surface::{self, Selections, SurfaceOptions},
};
use deadlock_core::{safety, CancellationToken};
use deadlock_dmx::Dmx;
use serde_json::Value;
use std::{fs, path::PathBuf};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/surface")
}
fn close(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            let tolerance = if path.contains("rotation_error_degrees") {
                2e-6
            } else {
                1e-10 * (1. + b.abs())
            };
            assert!((a - b).abs() <= tolerance, "{path}: {a}!={b}");
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                close(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect::<std::collections::BTreeSet<_>>(),
                "{path}"
            );
            for (k, a) in a {
                close(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn rendered_proxy_and_motion_bytes_and_all_gate_decisions_match_python() {
    let cases: Vec<Value> =
        serde_json::from_slice(&fs::read(root().join("oracle.json")).unwrap()).unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let d = Dmx::load(&root().join(format!("{name}.dmx"))).unwrap();
        let selected: Selections = serde_json::from_value(case["selection"].clone()).unwrap();
        let mut options = SurfaceOptions::default();
        options.max_edge_inches = case["options"]["max_edge_inches"].as_f64().unwrap();
        if let Some(count) = case["options"]["max_proxy_vertices"].as_u64() {
            options.max_proxy_vertices = count as usize;
        }
        let motion: Option<MotionProfile> = serde_json::from_value(case["motion"].clone()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("authored");
        let result = surface::author_direct_cloth_with_options(
            &d,
            &selected,
            &dest,
            case["source_sha256"].as_str().unwrap(),
            &options,
            &CancellationToken::new(),
            motion.as_ref(),
        );
        assert_eq!(
            result.is_ok(),
            case["passed"].as_bool().unwrap(),
            "{name}: {result:?}"
        );
        if let Ok(report) = result {
            for (label, path) in [
                ("render", &report.render_path),
                ("proxy", &report.proxy_path),
            ] {
                let expected = root().join(format!("{name}.{label}.dmx"));
                let actual = fs::read(path).unwrap();
                let oracle = fs::read(&expected).unwrap();
                let first = actual.iter().zip(&oracle).position(|(a, b)| a != b);
                assert_eq!(
                    safety::digest(path).unwrap(),
                    safety::digest(&expected).unwrap(),
                    "{name}.{label}, first different byte {first:?}, lengths {}/{}",
                    actual.len(),
                    oracle.len()
                );
            }
            let mut actual = serde_json::to_value(report).unwrap();
            actual.as_object_mut().unwrap().remove("render_path");
            actual.as_object_mut().unwrap().remove("proxy_path");
            close(&actual, &case["receipt"], name);
        } else {
            assert!(!dest.exists(), "Failed {name} wrote authoring files");
        }
    }
}
#[test]
fn stale_hash_cancel_nonempty_destination_and_existing_maps_do_not_overwrite_inputs() {
    let path = root().join("base.dmx");
    let source = Dmx::load(&path).unwrap();
    let hash = safety::digest(&path).unwrap();
    let selected = Selections::from([(
        "garment".into(),
        surface::Selection {
            enabled: (3..9).collect(),
            pinned: vec![0, 1, 2],
        },
    )]);
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("output");
    let token = CancellationToken::new();
    assert!(
        surface::author_direct_cloth(&source, &selected, &dest, "wrong", 0, &token, None).is_err()
    );
    assert!(!dest.exists());
    token.cancel();
    assert!(
        surface::author_direct_cloth(&source, &selected, &dest, &hash, 0, &token, None).is_err()
    );
    assert!(!dest.exists());
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("previous"), b"KEEP").unwrap();
    assert!(surface::author_direct_cloth(
        &source,
        &selected,
        &dest,
        &hash,
        0,
        &CancellationToken::new(),
        None
    )
    .is_err());
    assert_eq!(fs::read(dest.join("previous")).unwrap(), b"KEEP");
    assert_eq!(safety::digest(&path).unwrap(), hash);
    let marked = Dmx::load(&root().join("base.render.dmx")).unwrap();
    assert!(surface::mesh_data(&marked).is_err());
    let proxy = fs::read(root().join("motion.proxy.dmx")).unwrap();
    assert!(motion::author_motion_maps(
        &proxy,
        &MotionProfile::default(),
        &CancellationToken::new()
    )
    .is_err());
}
#[test]
fn malformed_motion_profile_and_mask_indices_fail_without_tolerance() {
    let proxy = fs::read(root().join("base.proxy.dmx")).unwrap();
    for value in [0., -0.1, 1.01, f64::NAN, f64::INFINITY] {
        let profile = MotionProfile {
            drag: value,
            ..Default::default()
        };
        assert!(motion::author_motion_maps(&proxy, &profile, &CancellationToken::new()).is_err());
    }
    let d = Dmx::parse(proxy).unwrap();
    let state = d
        .elements
        .iter()
        .position(|e| e.kind == "DmeVertexData")
        .unwrap();
    let mut changes = deadlock_dmx::Rewrite::default();
    let a = d.elements[state]
        .attributes
        .iter()
        .find(|a| a.name == "cloth_enable$0Indices")
        .unwrap();
    let mut raw = d.raw_attribute(a).to_vec();
    raw[4..8].copy_from_slice(&999999i32.to_le_bytes());
    changes
        .attributes
        .insert((state, "cloth_enable$0Indices".into()), raw);
    let changed = d.rewrite_binary9(&changes).unwrap();
    assert!(motion::author_motion_maps(
        &changed,
        &MotionProfile::default(),
        &CancellationToken::new()
    )
    .is_err());
}
