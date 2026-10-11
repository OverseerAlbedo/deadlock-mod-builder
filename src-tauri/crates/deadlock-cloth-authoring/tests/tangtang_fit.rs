//! Real-input regressions; no Source 2 compiler or game execution is implied.
use deadlock_cloth_authoring::{secondary, surface};
use deadlock_core::CancellationToken;
use deadlock_dmx::{Dmx, Rewrite};
use deadlock_project::{customization::SecondaryMotion, Project};
use deadlock_skeleton::extensions::SourceBinding;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn binding(d: &Dmx) -> SourceBinding {
    let mut skeleton = surface::binds(d).unwrap().skeleton;
    skeleton.flags = vec![64; skeleton.names.len()];
    let extras = skeleton
        .names
        .iter()
        .filter(|n| n.starts_with("dmb_secondary_"))
        .cloned()
        .collect::<Vec<_>>();
    SourceBinding {
        source_sha256: format!("{:x}", Sha256::digest(d.bytes())),
        original_sha256: "0".repeat(64),
        skeleton,
        extra_names: extras.clone(),
        required_extra_names: extras,
    }
}
fn project(preset: &str) -> Project {
    let mut p = Project::default();
    p.original_model = "models/heroes_staging/haze/haze.vmdl".into();
    p.customization.enabled = Some(true);
    p.customization.secondary_motion = Some(SecondaryMotion {
        enabled: Some(true),
        preset: Some(preset.into()),
        ..Default::default()
    });
    p.preparation_options
        .values
        .insert("preserve_physics_bones".into(), true.into());
    let saved: serde_json::Value =
        serde_json::from_str(include_str!("../resources/tangtang-r2-groups.json")).unwrap();
    let roots = saved["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["source_root"].clone())
        .collect::<Vec<_>>();
    p.preparation_options
        .values
        .insert("physics_roots".into(), roots.into());
    p
}
fn mutate(d: &Dmx, index: usize, name: &str, raw: Vec<u8>) -> Dmx {
    let mut edits = Rewrite::default();
    edits.attributes.insert((index, name.into()), raw);
    Dmx::parse(d.rewrite_binary9(&edits).unwrap()).unwrap()
}

#[test]
#[ignore = "requires DMB_TANG_DMX, DMB_TANG_LAPTOP_DMX and independent DMB_TANG_FIT_ORACLE"]
fn actual_laptop_conversion_refits_both_presets_and_preserves_tuning() {
    let old = Dmx::load(Path::new(&std::env::var("DMB_TANG_DMX").unwrap())).unwrap();
    let d = Dmx::load(Path::new(&std::env::var("DMB_TANG_LAPTOP_DMX").unwrap())).unwrap();
    let oracle: serde_json::Value =
        serde_json::from_slice(&fs::read(std::env::var("DMB_TANG_FIT_ORACLE").unwrap()).unwrap())
            .unwrap();
    let before = d.bytes().to_vec();
    let cancel = CancellationToken::new();
    for preset in ["tangtang_r2", "tangtang_reference"] {
        let p = project(preset);
        let accepted = secondary::setup(&p, &old, Some(&binding(&old)), &cancel)
            .unwrap()
            .unwrap();
        assert!(accepted.preset_fit.is_none());
        let actual = secondary::setup(&p, &d, Some(&binding(&d)), &cancel)
            .unwrap()
            .unwrap();
        let receipt = actual.preset_fit.as_ref().unwrap();
        assert_eq!(receipt.groups_rebound, 38);
        assert!(receipt.max_group_position_residual_inches < 4e-6);
        assert!(receipt.max_group_basis_residual < 2e-7);
        assert_eq!(
            (actual.proxy_positions, actual.dynamic_positions),
            (900, 786)
        );
        assert!(!actual.render_source_modified);
        let a = Dmx::parse(accepted.proxy).unwrap();
        let b = Dmx::parse(actual.proxy.clone()).unwrap();
        assert_eq!(
            surface::binds(&b).unwrap().skeleton,
            surface::binds(&d).unwrap().skeleton
        );
        assert_eq!(a.elements.len(), b.elements.len());
        for (aa, bb) in a.elements.iter().zip(&b.elements) {
            assert_eq!((&aa.kind, &aa.name, aa.uuid), (&bb.kind, &bb.name, bb.uuid));
            for (x, y) in aa.attributes.iter().zip(&bb.attributes) {
                assert_eq!((&x.name, x.type_code), (&y.name, y.type_code));
                let changed = (aa.kind == "DmeTransform"
                    && matches!(x.name.as_str(), "position" | "orientation"))
                    || (aa.kind == "DmeVertexData"
                        && matches!(x.name.as_str(), "position$0" | "normal$0"));
                if !changed {
                    assert_eq!(
                        a.raw_attribute(x),
                        b.raw_attribute(y),
                        "{} {}",
                        aa.name,
                        x.name
                    );
                }
                if aa.kind == "DmeVertexData"
                    && matches!(x.name.as_str(), "position$0" | "normal$0")
                {
                    let expected: Vec<f64> =
                        serde_json::from_value(oracle[preset][&x.name].clone()).unwrap();
                    let observed = b.raw_attribute(y)[4..]
                        .chunks_exact(4)
                        .map(|v| f32::from_le_bytes(v.try_into().unwrap()) as f64)
                        .collect::<Vec<_>>();
                    assert_eq!(expected.len(), observed.len());
                    for (l, r) in observed.iter().zip(&expected) {
                        assert!((l - r).abs() < 1e-5, "{} {}: {l} vs {r}", preset, x.name);
                    }
                }
            }
        }
        if let Ok(dir) = std::env::var("DMB_TANG_FIT_OUTPUT") {
            fs::create_dir_all(&dir).unwrap();
            fs::write(Path::new(&dir).join(format!("{preset}.dmx")), &actual.proxy).unwrap();
            fs::write(
                Path::new(&dir).join(format!("{preset}.json")),
                serde_json::to_vec_pretty(&actual).unwrap(),
            )
            .unwrap();
        }
    }
    assert_eq!(d.bytes(), before);
}

#[test]
#[ignore = "requires DMB_TANG_LAPTOP_DMX"]
fn refit_refuses_changed_weights_internal_bones_body_binds_and_stale_analysis() {
    let d = Dmx::load(Path::new(&std::env::var("DMB_TANG_LAPTOP_DMX").unwrap())).unwrap();
    let p = project("tangtang_reference");
    let cancel = CancellationToken::new();
    let mut bad = binding(&d);
    bad.source_sha256 = "0".repeat(64);
    assert!(secondary::setup(&p, &d, Some(&bad), &cancel)
        .unwrap_err()
        .to_string()
        .contains("changed after analysis"));
    let state = d
        .elements
        .iter()
        .position(|e| e.kind == "DmeVertexData" && e.get("blendweights$0").is_some())
        .unwrap();
    let attr = d.elements[state]
        .attributes
        .iter()
        .find(|a| a.name == "blendweights$0")
        .unwrap();
    let mut weights = d.raw_attribute(attr).to_vec();
    // Change valid, normalized skinning so this exercises model identity,
    // rather than merely triggering the existing weight-sum validator.
    let i = weights[4..]
        .chunks_exact(16)
        .position(|b| {
            f32::from_le_bytes(b[..4].try_into().unwrap()) > 0.01
                && f32::from_le_bytes(b[4..8].try_into().unwrap()) > 0.01
        })
        .unwrap()
        * 16
        + 4;
    let value = f32::from_le_bytes(weights[i..i + 4].try_into().unwrap());
    let other = f32::from_le_bytes(weights[i + 4..i + 8].try_into().unwrap());
    weights[i..i + 4].copy_from_slice(&(value - 0.001).to_le_bytes());
    weights[i + 4..i + 8].copy_from_slice(&(other + 0.001).to_le_bytes());
    let bad = mutate(&d, state, "blendweights$0", weights);
    let err = secondary::setup(&p, &bad, Some(&binding(&bad)), &cancel).unwrap_err();
    assert!(err.to_string().contains("different topology"), "{err}");
    for (bone, message) in [
        ("dmb_secondary_HB_1_0", "changed shape"),
        ("pelvis", "body binds"),
    ] {
        let i = d
            .elements
            .iter()
            .position(|e| e.kind == "DmeTransform" && e.name == bone)
            .unwrap();
        let a = d.elements[i]
            .attributes
            .iter()
            .find(|a| a.name == "position")
            .unwrap();
        let mut raw = d.raw_attribute(a).to_vec();
        let value = f32::from_le_bytes(raw[..4].try_into().unwrap());
        raw[..4].copy_from_slice(&(value + 0.1).to_le_bytes());
        let bad = mutate(&d, i, "position", raw);
        let err = secondary::setup(&p, &bad, Some(&binding(&bad)), &cancel).unwrap_err();
        assert!(err.to_string().contains(message), "{err}");
    }
    let mut wrong = p;
    wrong.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    assert!(secondary::setup(&wrong, &d, Some(&binding(&d)), &cancel).is_err());
    cancel.cancel();
    assert!(secondary::setup(
        &project("tangtang_reference"),
        &d,
        Some(&binding(&d)),
        &cancel
    )
    .is_err());
}
