use deadlock_cloth_authoring::proxy_source::*;
use deadlock_resource::text::Document;
use deadlock_skeleton::Skeleton;
use serde::Deserialize;
use std::{fs, path::Path};
fn text(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}
#[derive(Deserialize)]
struct Inputs {
    skeleton: Skeleton,
    analysis: Analysis,
}
fn inputs() -> Inputs {
    serde_json::from_str(&text("proxy-inputs.json")).unwrap()
}
#[test]
fn native_source_authoring_matches_python_and_preserves_unicode_offsets() {
    let input = inputs();
    let names: Vec<String> = serde_json::from_str(&text("proxy-cases.json")).unwrap();
    for name in names {
        let source = text(&format!("{name}-before.vmdl"));
        let (output, receipt) = author_direct_proxy(
            &source,
            "models/custom/garment_proxy.dmx",
            &input.analysis,
            &input.skeleton,
            "builder_skirt",
            false,
        )
        .unwrap();
        assert_eq!(output, text(&format!("{name}-after.vmdl")), "{name}");
        let expected: serde_json::Value =
            serde_json::from_str(&text(&format!("{name}-receipt.json"))).unwrap();
        let mut actual = serde_json::to_value(receipt).unwrap();
        let mut expected = expected;
        // Proposal world coordinates are f64 matrix diagnostics, never serialized
        // bind authorization. Source capsule fields still match byte-for-byte.
        for value in [&mut actual, &mut expected] {
            value["capsules"][0]
                .as_object_mut()
                .unwrap()
                .remove("endpoints_world");
        }
        assert_eq!(actual, expected, "{name}");
    }
    let (output, receipt) =
        configure_joint_driven_proxy(&text("empty28-after.vmdl"), "builder_skirt_proxy").unwrap();
    assert_eq!(output, text("joint-after.vmdl"));
    assert_eq!(
        serde_json::to_value(receipt).unwrap(),
        serde_json::from_str::<serde_json::Value>(&text("joint-receipt.json")).unwrap()
    );
}
#[test]
fn native_authoring_refuses_stale_binds_ambiguous_containers_and_hidden_switches() {
    let mut input = inputs();
    let source = text("empty40-before.vmdl");
    input.analysis.leg_colliders[0].endpoints_world[0][0] += 0.01;
    assert!(author_direct_proxy(
        &source,
        "models/proxy.dmx",
        &input.analysis,
        &input.skeleton,
        "test",
        false
    )
    .is_err());
    input = inputs();
    input.analysis.leg_colliders[0].radius_local = 0.;
    assert!(author_direct_proxy(
        &source,
        "models/proxy.dmx",
        &input.analysis,
        &input.skeleton,
        "test",
        false
    )
    .is_err());
    input = inputs();
    for path in ["../proxy.dmx", "models/p.vmdl"] {
        assert!(author_direct_proxy(
            &source,
            path,
            &input.analysis,
            &input.skeleton,
            "test",
            false
        )
        .is_err());
    }
    for source in [
        text("empty40-after.vmdl"),
        source.replace(
            "children = [",
            "children = [{ _class=\"Softbody\" children=[] },{ _class=\"Softbody\" children=[] },",
        ),
    ] {
        assert!(author_direct_proxy(
            &source,
            "models/custom/garment_proxy.dmx",
            &input.analysis,
            &input.skeleton,
            "builder_skirt",
            false
        )
        .is_err());
    }
    let quoted = text("empty28-after.vmdl")
        .replace("back_solve_joints = false", "back_solve_joints = \"false\"");
    assert!(configure_joint_driven_proxy(&quoted, "builder_skirt_proxy").is_err());
    assert!(
        configure_joint_driven_proxy(&text("empty40-after.vmdl"), "builder_skirt_proxy").is_err()
    );
}
#[test]
fn explicit_movement_only_preview_keeps_runtime_authority_false() {
    let mut input = inputs();
    input.analysis.leg_colliders.clear();
    input.analysis.collision_disabled = true;
    for version in [28, 40, 41] {
        let source = text(&format!("empty{version}-before.vmdl"));
        let (output, receipt) = author_direct_proxy(
            &source,
            "models/proxy.dmx",
            &input.analysis,
            &input.skeleton,
            "builder_skirt",
            true,
        )
        .unwrap();
        assert!(Document::parse(&output)
            .unwrap()
            .objects(Some("ClothShapeCapsule"))
            .is_empty());
        assert!(
            !receipt.runtime_ready
                && !receipt.original_solver_preservation_verified
                && !receipt.native_compiled
        );
    }
    assert!(author_direct_proxy(
        &text("empty40-before.vmdl"),
        "models/proxy.dmx",
        &input.analysis,
        &input.skeleton,
        "builder_skirt",
        false
    )
    .is_err());
}
