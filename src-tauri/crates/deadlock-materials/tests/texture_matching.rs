use deadlock_core::CancellationToken;
use deadlock_materials::{plan, plan_with_bindings, texture_candidates, Plans};
use serde_json::json;
use std::{fs, path::Path};

fn images(root: &Path, names: &[&str]) {
    for name in names {
        let file = root.join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, b"fixture image bytes").unwrap();
    }
}
#[test]
fn prepared_hashes_actor_prefixes_number_padding_and_nested_folders_match() {
    let temp = tempfile::tempdir().unwrap();
    images(
        temp.path(),
        &[
            "textures/T_actor_tangtang_cloth_02_D.png",
            "textures/T_actor_tangtang_cloth_01_D.png",
            "other tex/T_actor_tangtang_cloth_02_N.png",
            "textures/T_actor_tangtang_body_01_D.png",
            "textures/T_actor_tangtang_cloth_02_ORM.png",
            "textures/T_actor_tangtang_cloth_02_M.png",
            "textures/T_actor_tangtang_nobody_01_D.png",
        ],
    );
    let paths = texture_candidates(
        "models/heroes_staging/haze/materials/prepared/cloth2_e034ebe4.vmat",
        temp.path(),
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(paths["color"].len(), 1);
    assert!(paths["color"][0].ends_with("T_actor_tangtang_cloth_02_D.png"));
    assert_eq!(paths["normal"].len(), 1);
    assert!(paths["roughness"].is_empty());
    assert!(paths["metalness"].is_empty());
    let body = texture_candidates(
        "models/test/materials/prepared/body_1234abcd.vmat",
        temp.path(),
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(body["color"].len(), 1);
    assert!(body["color"][0].ends_with("T_actor_tangtang_body_01_D.png"));
}
#[test]
fn unknown_numeric_materials_and_multiple_variants_are_never_guessed() {
    let temp = tempfile::tempdir().unwrap();
    images(
        temp.path(),
        &["iris_01_D.png", "iris_02_D.png", "body_01_D.png"],
    );
    let iris = "models/test/materials/prepared/iris_1234abcd.vmat".to_string();
    let opaque = "models/test/materials/prepared/_1_e8d63bad.vmat".to_string();
    let result = plan(
        &[iris.clone(), opaque.clone()],
        temp.path(),
        temp.path(),
        &Plans::new(),
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(result[&iris].ambiguous.as_ref().unwrap(), &["color"]);
    assert!(!result[&iris]
        .textures
        .as_ref()
        .unwrap()
        .contains_key("color"));
    assert_eq!(result[&iris].confirmed, Some(false));
    assert!(result[&opaque].textures.as_ref().unwrap().is_empty());
    // Do not reinterpret arbitrary hashes outside the prepared namespace.
    assert!(texture_candidates(
        "models/test/iris_1234abcd.vmat",
        temp.path(),
        &CancellationToken::new()
    )
    .unwrap()["color"]
        .is_empty());
}
#[test]
fn recovered_connections_outrank_names_but_keep_manual_choices_and_explicit_none() {
    let temp = tempfile::tempdir().unwrap();
    images(
        temp.path(),
        &[
            "body_01_D.png",
            "linked.png",
            "manual.png",
            "normal.png",
            "other.png",
        ],
    );
    let material = "models/test/materials/prepared/body_1234abcd.vmat";
    let bindings: Plans = serde_json::from_value(json!({material:{"textures":{"color":temp.path().join("linked.png"),"normal":temp.path().join("normal.png")}}})).unwrap();
    let automatic = plan_with_bindings(
        &[material.into()],
        temp.path(),
        temp.path(),
        &Plans::new(),
        &bindings,
        &CancellationToken::new(),
    )
    .unwrap();
    assert!(automatic[material].textures.as_ref().unwrap()["color"].ends_with("linked.png"));
    assert_eq!(automatic[material].confirmed, Some(false));
    let current: Plans = serde_json::from_value(json!({material:{"textures":{"color":temp.path().join("manual.png"),"normal":""},"color":[0.2,0.3,0.4],"confirmed":true,"future_note":"preserve"}})).unwrap();
    let planned = plan_with_bindings(
        &[material.into()],
        temp.path(),
        temp.path(),
        &current,
        &bindings,
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(planned[material].textures, current[material].textures);
    assert_eq!(planned[material].color, current[material].color);
    assert_eq!(planned[material].confirmed, Some(true));
    assert_eq!(planned[material].extensions["future_note"], "preserve");
    let ambiguous: Plans = serde_json::from_value(json!({material:{"candidates":{"color":[temp.path().join("linked.png"),temp.path().join("other.png")]}}})).unwrap();
    let planned = plan_with_bindings(
        &[material.into()],
        temp.path(),
        temp.path(),
        &Plans::new(),
        &ambiguous,
        &CancellationToken::new(),
    )
    .unwrap();
    assert!(planned[material].textures.as_ref().unwrap().is_empty());
    assert_eq!(planned[material].ambiguous.as_ref().unwrap(), &["color"]);
}
#[test]
fn exact_exported_texture_identity_wins_and_new_assignments_renew_review() {
    let temp = tempfile::tempdir().unwrap();
    images(
        temp.path(),
        &["body_1234abcd_color.png", "T_actor_body_01_D.png"],
    );
    let material = "models/test/materials/prepared/body_1234abcd.vmat";
    let current: Plans =
        serde_json::from_value(json!({material:{"textures":{},"confirmed":true}})).unwrap();
    let planned = plan(
        &[material.into()],
        temp.path(),
        temp.path(),
        &current,
        &CancellationToken::new(),
    )
    .unwrap();
    assert!(
        planned[material].textures.as_ref().unwrap()["color"].ends_with("body_1234abcd_color.png")
    );
    assert_eq!(planned[material].confirmed, Some(false));
    assert!(planned[material].ambiguous.as_ref().unwrap().is_empty());
}
