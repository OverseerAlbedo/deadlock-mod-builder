//! Review gates and plans for gun customization in hero and weapon-model projects.
use deadlock_preparation::{service::{self, Analysis}, weapon};
use deadlock_project::{ModelInputMode, PreparationOptions, Project};
use serde_json::{json, Value};

fn pistol(grip: [f64; 3], forward: [f64; 3], up: [f64; 3], size: f64) -> Vec<[f64; 3]> {
    let right = [forward[1] * up[2] - forward[2] * up[1], forward[2] * up[0] - forward[0] * up[2], forward[0] * up[1] - forward[1] * up[0]];
    let at = |f: f64, u: f64, r: f64| [0, 1, 2].map(|i| grip[i] + forward[i] * f * size + up[i] * u * size + right[i] * r * size);
    let mut points = vec![];
    for i in 0..=20 {
        for (a, b) in [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)] {
            points.push(at(-0.06 + 0.24 * f64::from(i) / 20., 0.05 + 0.02 * a, 0.015 * b));
        }
    }
    for i in 0..=10 {
        for (a, b) in [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)] {
            points.push(at(0.025 * a, -0.08 + 0.1 * f64::from(i) / 10., 0.015 * b));
        }
    }
    points
}

fn analysis(gun_meshes: bool) -> Analysis {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/reference.json")).unwrap();
    let suggestions = fixture["cases"][1]["expected"].clone();
    let sample = pistol([-0.9, 0., 1.5], [0., -1., 0.], [0., 0., 1.], 1.);
    let mut inspection = fixture["inspection"].clone();
    inspection["weapons"] = json!([{
        "id": "Pistol", "objects": ["Pistol"], "vertices": sample.len(), "bounds": [[-1., -0.3, 1.3], [-0.8, 0.1, 1.6]],
        "sample": sample, "attach_bone": "mixamorig:RightHand", "attach_kind": "bone_parent", "keywords": ["pistol"], "elongation": 3.
    }]);
    let reference = if gun_meshes {
        let points = pistol([-0.9, 0.3, 1.5], [1., 0., 0.], [0., 0., 1.], 1.2);
        json!({"policy": weapon::POLICY, "source": "meshes", "separate_model": false, "meshes": ["hero_gun", "hero_gun_lod1"],
               "primary": ["hero_gun"], "model_meshes": ["hero_body", "hero_gun", "hero_gun_lod1"], "points": points,
               "weights": vec![vec![("hand_R", 1.0)]; points.len()], "bones": ["hand_R"]})
    } else {
        json!({"policy": weapon::POLICY, "source": "skeleton", "separate_model": false, "model_meshes": ["hero_body"],
               "notes": ["This hero model has no gun mesh of its own; its gun is a separate model."]})
    };
    serde_json::from_value(json!({
        "source": "C:/workspace/source.blend", "source_sha256": "a".repeat(64), "source_original": "C:/source.blend",
        "source_input": {"received": "", "resolved": "", "cwd": ""}, "root": "C:/workspace", "target_model": "models/heroes/test.vmdl",
        "target_sha256": "b".repeat(64), "target_skeleton": fixture["skeleton"], "target": fixture["target"],
        "target_scale_audit": fixture["scale_audit"], "analysis": inspection, "suggestions": suggestions, "rotation": [0, 0, 0],
        "geometry_customization": {"props": [], "face": null}, "clothing": null, "weapon_reference": reference,
    }))
    .unwrap()
}

fn project(mode: ModelInputMode) -> Project {
    let mut project = Project::default();
    project.original_model = "models/heroes/test.vmdl".into();
    project.model_input_mode = mode;
    project
}

fn options(analysis: &Analysis, weapon: Value) -> PreparationOptions {
    let s = &analysis.suggestions;
    serde_json::from_value(json!({
        "reviewed": true, "source_roles": s.source.as_ref().unwrap().roles, "target_roles": s.target.roles,
        "mapping": s.mapping, "weapon": weapon,
    }))
    .unwrap()
}

fn weapon_issues(project: &Project, analysis: &Analysis, options: &PreparationOptions) -> Vec<String> {
    service::review_issues(project, analysis, options).unwrap().into_iter().filter(|i| i.tab == "weapon").map(|i| i.message).collect()
}

#[test]
fn hero_gun_meshes_are_replaced_and_suggested_by_default() {
    let analysis = analysis(true);
    let hero = project(ModelInputMode::RiggedHumanoid);
    let suggestion = service::weapon_suggestion(&hero, &analysis).unwrap();
    assert_eq!((suggestion.action.as_str(), suggestion.objects.clone()), ("replace", vec!["Pistol".to_string()]));
    let choices = options(&analysis, serde_json::to_value(&suggestion).unwrap());
    assert!(weapon_issues(&hero, &analysis, &choices).is_empty());
    let plan = service::weapon_plan(&hero, &analysis, &choices).unwrap().unwrap();
    assert_eq!(plan.action, "replace");
    assert_eq!(plan.replace, vec!["hero_gun".to_string(), "hero_gun_lod1".to_string()]);
    assert!(matches!(plan.skin, weapon::Skin::Rigid { ref bone } if bone == "hand_R"));
    let preview = service::weapon_preview(&hero, &analysis, &choices).unwrap();
    assert_eq!(preview.fitted.len(), analysis.analysis.weapons[0].sample.len());
    // Keeping the gun in the hand needs no plan at all.
    let keep = options(&analysis, json!({"enabled": true, "action": "keep", "objects": ["Pistol"]}));
    assert!(service::weapon_plan(&hero, &analysis, &keep).unwrap().is_none());
    // A stale gun choice is a review issue, not a Blender failure.
    let stale = options(&analysis, json!({"enabled": true, "action": "replace", "objects": ["Gone"]}));
    assert!(weapon_issues(&hero, &analysis, &stale)[0].contains("no longer in the inspected source"));
}

#[test]
fn separate_hero_guns_point_to_a_weapon_project() {
    let analysis = analysis(false);
    let hero = project(ModelInputMode::RiggedHumanoid);
    assert_eq!(service::weapon_suggestion(&hero, &analysis).unwrap().action, "remove");
    let replace = options(&analysis, json!({"enabled": true, "action": "replace", "objects": ["Pistol"]}));
    assert!(weapon_issues(&hero, &analysis, &replace)[0].contains("Weapon model"));
    let remove = options(&analysis, json!({"enabled": true, "action": "remove", "objects": ["Pistol"]}));
    assert!(weapon_issues(&hero, &analysis, &remove).is_empty());
    assert_eq!(service::weapon_plan(&hero, &analysis, &remove).unwrap().unwrap().action, "remove");
}

#[test]
fn weapon_model_projects_only_need_a_gun_and_a_measurable_fit() {
    let mut analysis = analysis(true);
    analysis.weapon_reference.as_mut().unwrap().separate_model = true;
    let gun_project = project(ModelInputMode::Weapon);
    let nothing = options(&analysis, Value::Null);
    let issues = service::review_issues(&gun_project, &analysis, &nothing).unwrap();
    assert_eq!(issues.len(), 1);
    assert!(issues[0].message.contains("Choose the gun"));
    let suggestion = service::weapon_suggestion(&gun_project, &analysis).unwrap();
    let choices = options(&analysis, serde_json::to_value(&suggestion).unwrap());
    assert!(service::review_issues(&gun_project, &analysis, &choices).unwrap().is_empty());
    let plan = service::weapon_plan(&gun_project, &analysis, &choices).unwrap().unwrap();
    assert_eq!(plan.action, "weapon_model");
    assert_eq!(plan.replace, vec!["hero_body".to_string(), "hero_gun".to_string(), "hero_gun_lod1".to_string()]);
}

#[test]
fn weapon_model_accepts_an_unrigged_gun_only_file_without_names() {
    // A gun-only weapon source: one object called "Cube", no rig, no weapon
    // words, lying along X. Blender offers the whole file as one candidate.
    let mut analysis = analysis(true);
    analysis.weapon_reference.as_mut().unwrap().separate_model = true;
    let sample = pistol([0.2, 0., 0.1], [1., 0., 0.], [0., 0., 1.], 1.);
    analysis.analysis.bones.clear();
    analysis.analysis.weighted_bones.clear();
    analysis.suggestions.source = None;
    analysis.analysis.weapons = serde_json::from_value(json!([{
        "id": "Cube", "objects": ["Cube"], "vertices": sample.len(), "bounds": [[0.1, -0.1, 0.], [0.5, 0.1, 0.2]],
        "sample": sample, "attach_bone": null, "attach_kind": "whole_file", "keywords": [], "elongation": 0.
    }]))
    .unwrap();
    let gun_project = project(ModelInputMode::Weapon);
    let suggestion = service::weapon_suggestion(&gun_project, &analysis).unwrap();
    assert_eq!((suggestion.action.as_str(), suggestion.objects.clone()), ("replace", vec!["Cube".to_string()]));
    let choices: PreparationOptions = serde_json::from_value(json!({"reviewed": true, "weapon": suggestion})).unwrap();
    assert!(service::review_issues(&gun_project, &analysis, &choices).unwrap().is_empty());
    let plan = service::weapon_plan(&gun_project, &analysis, &choices).unwrap().unwrap();
    assert_eq!(plan.action, "weapon_model");
    // Without a hand bone the grip is the handle hanging below the barrel and
    // the muzzle is the thinner front end.
    let preview = service::weapon_preview(&gun_project, &analysis, &choices).unwrap();
    let source = &preview.fit.source;
    assert!(source.forward[0] > 0.99, "barrel {:?}", source.forward);
    assert!(source.up[2] > 0.95, "up {:?}", source.up);
    assert!(source.grip[2] < 0.1 && (source.grip[0] - 0.2).abs() < 0.05, "grip {:?}", source.grip);
    // A character project still offers nothing for an unnamed loose object.
    assert!(service::weapon_suggestion(&project(ModelInputMode::RiggedHumanoid), &analysis).is_none());
}

#[test]
fn the_gun_file_gun_is_suggested_and_fitted_by_its_own_handle() {
    let analysis = analysis(true);
    let hero = project(ModelInputMode::RiggedHumanoid);
    // The gun file's pistol was moved so its grip sits in the right hand.
    let sample = pistol([-0.9, 0., 1.5], [0., -1., 0.], [0., 0., 1.], 1.);
    let mut gun_file: weapon::Candidate = serde_json::from_value(json!({
        "id": "gun_file_Cube", "objects": ["gun_file_Cube"], "vertices": sample.len(), "bounds": [[-1., -0.3, 1.3], [-0.8, 0.1, 1.6]],
        "sample": sample, "attach_bone": "mixamorig:RightHand", "attach_kind": "placed", "keywords": ["gun"], "elongation": 3.,
        "gun_file": true
    }))
    .unwrap();
    let mut with_file = analysis.clone();
    with_file.analysis.weapons.push(gun_file.clone());
    // Preferred over the character file's own pistol.
    let suggestion = service::weapon_suggestion(&hero, &with_file).unwrap();
    assert_eq!((suggestion.action.as_str(), suggestion.objects.clone()), ("replace", vec!["gun_file_Cube".to_string()]));
    let choices = options(&with_file, serde_json::to_value(&suggestion).unwrap());
    assert!(weapon_issues(&hero, &with_file, &choices).is_empty());
    assert_eq!(service::weapon_plan(&hero, &with_file, &choices).unwrap().unwrap().objects, vec!["gun_file_Cube".to_string()]);
    // The hand is where the grip was put, not extra evidence: the fit is the
    // same as for the same gun lying anywhere else.
    let placed = service::weapon_preview(&hero, &with_file, &choices).unwrap().fit.source;
    let measured = weapon::measure(&gun_file.sample, None, None).unwrap();
    assert_eq!(placed.grip, measured.grip);
    gun_file.attach_kind = "attached".into();
    with_file.analysis.weapons[1] = gun_file;
    let held = service::weapon_preview(&hero, &with_file, &choices).unwrap().fit.source;
    assert_ne!(held.grip, measured.grip, "a gun already in the hand uses the hand");
}
