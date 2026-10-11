use deadlock_resource::{
    binary, cloth, constraints, container, independent, text::Document, Kind, Value,
};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn manifest() -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(fixtures().join("cloth-manifest.json")).unwrap()).unwrap()
}

#[test]
fn all_python_constraint_cases_preserve_fail_pending_pass_decisions() {
    for case in manifest()["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let bytes = std::fs::read(fixtures().join(format!("cloth-{name}.kv3"))).unwrap();
        let physics = binary::decode(&bytes).unwrap().root;
        let rods = serde_json::to_value(cloth::audit_rods(&physics)).unwrap();
        let constraint = serde_json::to_value(constraints::audit(&physics)).unwrap();
        assert_eq!(rods["status"], case["rods"]["status"], "{name} rod audit");
        assert_eq!(
            constraint["status"], case["constraints"]["status"],
            "{name}: {constraint}"
        );
        for key in [
            "checked_rod_capsule_pairs",
            "mask_excluded_rod_capsule_pairs",
            "conflicting_rod_capsule_pairs",
            "uncertain_rod_capsule_pairs",
            "distinct_rods",
            "distinct_dynamic_nodes",
            "distinct_anchors",
            "mixed_mask_conservative_superset",
        ] {
            if let Some(expected) = case["constraints"]["evidence"].get(key) {
                assert_eq!(&constraint["evidence"][key], expected, "{name} {key}");
            }
        }
        assert_eq!(constraint["evidence"]["build_authorized"], false);
        assert_eq!(constraint["evidence"]["compiled_resources_modified"], false);
    }
}

#[test]
fn native_r12_structure_controls_rods_and_constraint_decisions_match() {
    for case in manifest()["native"].as_array().unwrap() {
        let path = case["path"].as_str().unwrap();
        let raw = std::fs::read(root().join(path)).unwrap();
        let blocks = container::layout(&raw).unwrap().blocks;
        let read = |name: &str| {
            binary::decode(
                container::payload(&raw, container::unique(&blocks, name).unwrap()).unwrap(),
            )
            .unwrap()
            .root
        };
        let physics = read("PHYS");
        let data = read("DATA");
        let solver = physics.get("m_pFeModel").unwrap();
        cloth::self_contained(solver).unwrap();
        assert_eq!(
            serde_json::to_value(cloth::structure(&physics).unwrap()).unwrap(),
            case["structure"],
            "{path}"
        );
        let controls = solver
            .get("m_CtrlName")
            .unwrap()
            .array()
            .unwrap()
            .iter()
            .map(|v| v.string().unwrap().to_owned())
            .collect::<Vec<_>>();
        let names = data
            .get("m_modelSkeleton")
            .unwrap()
            .get("m_boneName")
            .unwrap()
            .array()
            .unwrap()
            .iter()
            .map(|v| v.string().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(cloth::control_bindings(&controls, &names, solver).unwrap())
                .unwrap(),
            case["control_bindings"]
        );
        let rods = serde_json::to_value(cloth::audit_rods(&physics)).unwrap();
        assert_eq!(rods["status"], case["rods"]["status"]);
        for (key, value) in rods["evidence"].as_object().unwrap() {
            if let Some(expected) = case["rods"]["evidence"].get(key) {
                assert_eq!(value, expected, "{path} rods {key}");
            }
        }
        let constraints = serde_json::to_value(constraints::audit(&physics)).unwrap();
        assert_eq!(
            constraints["status"], case["constraints"]["status"],
            "{path} {constraints}"
        );
        for key in [
            "boundary_rod_count",
            "checked_rod_capsule_pairs",
            "mask_excluded_rod_capsule_pairs",
            "conflicting_rod_capsule_pairs",
            "uncertain_rod_capsule_pairs",
            "mixed_mask_conservative_superset",
        ] {
            assert_eq!(
                constraints["evidence"][key], case["constraints"]["evidence"][key],
                "{path} {key}"
            );
        }
    }
}

#[test]
fn text_numeric_verifier_distinguishes_storage_width_without_tolerance() {
    let doc =
        Document::parse("{ f = 0.1 i = 18446744073709551615 blob = #[00ff] s = \"0.1\" }").unwrap();
    let mut expected = Value::new(Kind::Object(vec![
        ("f".into(), Value::new(Kind::F32(0.1))),
        ("i".into(), Value::new(Kind::U64(u64::MAX))),
        ("blob".into(), Value::new(Kind::Blob(vec![0, 255]))),
        ("s".into(), Value::new(Kind::String("0.1".into()))),
    ]));
    assert_eq!(
        independent::verify(&doc.root, &expected)
            .unwrap()
            .float32_checked,
        1
    );
    expected.get_mut("f").unwrap().kind = Kind::F64(0.1f32 as f64);
    assert!(independent::verify(&doc.root, &expected).is_err());
    expected.get_mut("f").unwrap().kind = Kind::F64(0.1);
    expected.get_mut("s").unwrap().kind = Kind::String("0.100000".into());
    assert!(independent::verify(&doc.root, &expected).is_err());
}

#[cfg(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
#[test]
fn independent_vrf_reader_verifies_every_data_phys_value_of_rust_encoded_resource() {
    use sha2::{Digest, Sha256};
    let root = root();
    let rid = if cfg!(target_os = "windows") {
        "win-x64"
    } else {
        "linux-x64"
    };
    let manifest_raw =
        std::fs::read(root.join("resources/native/cloth-reader/manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_raw).unwrap();
    let entry = manifest["platforms"][rid]["entrypoint"].as_str().unwrap();
    let exe = root
        .join("resources/native/cloth-reader")
        .join(entry);
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&exe).unwrap())),
        manifest["platforms"][rid]["files"][entry].as_str().unwrap()
    );
    let source =
        root.join("physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c");
    let raw = std::fs::read(&source).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        cloth::BALANCED_REFERENCE_SHA256
    );
    let layout = container::layout(&raw).unwrap();
    let mut replacements = std::collections::BTreeMap::new();
    for name in ["PHYS", "DATA"] {
        let parsed = binary::decode(
            container::payload(&raw, container::unique(&layout.blocks, name).unwrap()).unwrap(),
        )
        .unwrap();
        replacements.insert(
            name.into(),
            binary::encode(&parsed.root, parsed.format_guid).unwrap(),
        );
    }
    let encoded = container::rebuild(&raw, &replacements).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let model = temp.path().join("rust-encoded.vmdl_c");
    std::fs::write(&model, &encoded).unwrap();
    let layout = container::layout(&encoded).unwrap();
    for name in ["PHYS", "DATA"] {
        let output = std::process::Command::new(&exe)
            .args(["-i", model.to_str().unwrap(), "-b", name])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        let docs = independent::parse_cli_blocks(&text).unwrap();
        assert_eq!(docs.len(), 1);
        let expected = binary::decode(
            container::payload(&encoded, container::unique(&layout.blocks, name).unwrap()).unwrap(),
        )
        .unwrap();
        let proof = independent::verify(&docs[0].root, &expected.root).unwrap();
        assert!(!proof.numeric_tolerance_applied);
        assert!(proof.values_checked > 1000);
    }
    assert_eq!(std::fs::read(source).unwrap(), raw);
}
