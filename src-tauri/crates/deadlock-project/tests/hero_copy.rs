//! "Make a copy for another hero": the field table covers every field, the copy carries the
//! character's choices, keeps nothing that names the old hero's files, and leaves the
//! original untouched.
use deadlock_project::customization::*;
use deadlock_project::hero_copy::{copy_for_hero, copy_name, moved_material, Carry, CUSTOMIZATION_FIELDS, PREPARATION_KEYS, PROJECT_FIELDS};
use deadlock_project::{Customization, Project};
use serde_json::json;
use std::collections::BTreeSet;

const OLD: &str = "models/heroes_wip/geist/geist.vmdl";
const NEW: &str = "models/heroes_staging/haze/haze.vmdl";

fn names(table: &[(&str, Carry)]) -> BTreeSet<String> {
    table.iter().map(|(name, _)| (*name).to_owned()).collect()
}
fn keys(value: serde_json::Value) -> BTreeSet<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn every_project_field_is_in_the_table() {
    // `copied_report` is left out of saved files while empty: set it so it is listed.
    let mut project = Project::default();
    project.copied_report = "report.json".into();
    assert_eq!(keys(serde_json::to_value(&project).unwrap()), names(PROJECT_FIELDS));
}

#[test]
fn every_customization_field_is_in_the_table() {
    // Every field named (no `..`): a new field stops this test from building until it is classed.
    let full = Customization {
        schema: Some(1), enabled: Some(true), model_enabled: Some(true), hero_path: Some(String::new()),
        icons: Some(vec![]), item_icons: Some(vec![]), animations: Some(vec![]), effects: Some(vec![]), audio: Some(vec![]),
        clothing: Some(Clothing::default()), physics: Some(Physics::default()), cloth_setup: Some(ClothSetup::default()),
        secondary_motion: Some(SecondaryMotion::default()), appearance: Some(Appearance::default()), equipment: Some(Equipment::default()),
        face: Some(Face::default()), face_layers: Some(FaceLayers::default()), hero_materials: Some(HeroMaterials::default()),
        effect_edits: Some(vec![]), ability_objects: Some(vec![]), item_names: Some(vec![]), extensions: Default::default(),
    };
    let mut expected = names(CUSTOMIZATION_FIELDS);
    expected.remove("extensions");
    assert_eq!(keys(serde_json::to_value(&full).unwrap()), expected);
}

#[test]
fn preparation_keys_are_listed_once() {
    assert_eq!(names(PREPARATION_KEYS).len(), PREPARATION_KEYS.len());
}

fn source() -> Project {
    let mut project = Project::default();
    project.name = "My character".into();
    project.hero_name = "Lady Geist".into();
    project.original_model = OLD.into();
    project.preparation_source = "D:/Characters/sample/sample.fbx".into();
    project.preparation_report = "D:/Workspace/prepared-1/preparation-report.json".into();
    project.preparation_blend = "D:/Workspace/prepared-1/prepared.blend".into();
    project.dmx = "D:/Workspace/prepared-1/prepared.dmx".into();
    project.textures = "D:/Workspace/prepared-1/textures".into();
    project.source_root = "D:/Workspace/prepared-1".into();
    project.mesh_template = "D:/Workspace/prepared-1/mesh_mapping.vmdl".into();
    project.skeleton_reference = "D:/Workspace/prepared-1/skeleton_reference.json".into();
    project.vpk_number = 41;
    project.review_lods = true;
    project.meshes.insert("body".into(), deadlock_project::MeshMapping::One("geist_body".into()));
    let material: MaterialChoices = serde_json::from_value(json!({"preset":"skin","textures":{"color":"D:/Characters/sample/skin.png"},"confirmed":true})).unwrap();
    project.materials.insert("models/heroes_wip/geist/materials/prepared/skin_0ae57f18.vmat".into(), material);
    project.preparation_options = serde_json::from_value(json!({
        "source_roles": {"pelvis": "Hips", "head": "Head"}, "target_roles": {"pelvis": "pelvis"},
        "mapping": {"Hips": "pelvis"}, "mapping_anchors": {"Skirt": "pelvis"}, "physics_roots": ["Hair_01", "Skirt_01"],
        "physics_kinds": {"Hair_01": "hair"}, "fit_mode": "preserve_proportions", "fit_floor": "legs", "weapon": {"enabled": true},
        "weapon_source": "D:/Characters/sample/gun.fbx", "reviewed": true, "future_key": 3
    })).unwrap();
    project.customization = serde_json::from_value(json!({
        "enabled": true, "model_enabled": true, "hero_path": OLD,
        "audio": [{"enabled": true, "source": "D:/a.wav", "target": "sounds/vo/geist/a.vsnd_c"}, {"enabled": true, "silence": true, "target": "sounds/vo/geist/b.vsnd_c"}],
        "effects": [{"enabled": true, "source": "D:/e.vpcf_c", "target": "particles/abilities/geist/e.vpcf_c"}],
        "icons": [{"enabled": true, "source": "D:/i.png", "target": "panorama/images/heroes/geist_card.vtex_c"}],
        "item_icons": [{"enabled": true, "source": "D:/item.png", "target": "panorama/images/items/weapon/a.vtex_c"}],
        "hero_materials": {"enabled": true, "materials": [{"enabled": true, "target": "models/heroes_wip/geist/materials/geist_body.vmat_c", "brightness": 0.5}]},
        "appearance": {"enabled": true, "materials": [{"enabled": true, "target": "models/heroes_wip/geist/materials/prepared/skin_0ae57f18.vmat", "hue": 20.0}]},
        "secondary_motion": {"enabled": true, "preset": "automatic", "tuning": {"hair": {"stiffness": 0.3}}},
        "face_layers": {"blush": true},
        "equipment": {"enabled": true, "meshes": [{"target": "geist_hat", "action": "hide"}]},
        "physics": {"enabled": false, "source": "D:/cloth.vmdl_c", "hero_path": OLD},
        "ability_objects": [{"target": "models/abilities/sample_dagger_model.vmdl", "source": "D:/shape.fbx"}],
        "item_names": [{"item": "upgrade_sample", "name": "New Name"}],
        "some_future": {"x": 1}
    })).unwrap();
    project
}

#[test]
fn carries_the_characters_choices_to_the_new_hero() {
    let original = source();
    let copy = copy_for_hero(&original, "Haze", NEW, None);
    let p = &copy.project;
    assert_ne!(p.id, original.id);
    assert_eq!(p.name, "My character Haze");
    assert_eq!((p.hero_name.as_str(), p.original_model.as_str()), ("Haze", NEW));
    assert_eq!(p.preparation_source, original.preparation_source);
    // Material choices moved to the new hero's folder, with the same identity.
    assert_eq!(p.materials.keys().collect::<Vec<_>>(), ["models/heroes_staging/haze/materials/prepared/skin_0ae57f18.vmat"]);
    assert_eq!(p.materials.values().next(), original.materials.values().next());
    let rows = p.customization.appearance.as_ref().unwrap().materials.as_ref().unwrap();
    assert_eq!(rows[0].target.as_deref(), Some("models/heroes_staging/haze/materials/prepared/skin_0ae57f18.vmat"));
    assert_eq!(rows[0].hue, Some(20.0));
    let options = &p.preparation_options.values;
    for key in ["source_roles", "physics_roots", "physics_kinds", "fit_mode", "fit_floor", "weapon_source"] {
        assert_eq!(options.get(key), original.preparation_options.values.get(key), "{key}");
    }
    assert_eq!(p.customization.secondary_motion, original.customization.secondary_motion);
    assert_eq!(p.customization.item_icons, original.customization.item_icons);
    assert_eq!(p.customization.item_names, original.customization.item_names, "item names are the same for every hero");
    assert_eq!(p.customization.face_layers, original.customization.face_layers);
    // The first preparation reads the original's report, never the original's outputs.
    assert_eq!(p.copied_report, original.preparation_report);
}

#[test]
fn keeps_nothing_that_names_the_old_heros_files() {
    let original = source();
    let p = copy_for_hero(&original, "Haze", NEW, None).project;
    let c = &p.customization;
    assert!(c.audio.is_none() && c.effects.is_none() && c.icons.is_none() && c.animations.is_none());
    assert!(c.hero_materials.is_none() && c.effect_edits.is_none() && c.equipment.is_none() && c.physics.is_none());
    assert!(c.ability_objects.is_none(), "ability objects name the old hero's models");
    assert!(c.hero_path.is_none() && c.extensions.is_empty());
    for key in ["target_roles", "mapping", "mapping_anchors", "weapon", "reviewed", "future_key"] {
        assert!(!p.preparation_options.values.contains_key(key), "{key}");
    }
    assert!(p.meshes.is_empty() && p.dmx.is_empty() && p.preparation_report.is_empty() && p.preparation_blend.is_empty());
    assert!(p.textures.is_empty() && p.source_root.is_empty() && p.mesh_template.is_empty() && p.skeleton_reference.is_empty());
    assert_eq!(p.vpk_number, Project::default().vpk_number);
    assert!(!p.review_lods);
    // Nowhere in the copy (but the report it reads once) is the old hero named.
    let mut shown = p;
    shown.copied_report.clear();
    let text = serde_json::to_string(&shown).unwrap();
    assert!(!text.contains("geist"), "{text}");
}

#[test]
fn leaves_the_original_untouched_and_says_what_it_did() {
    let original = source();
    let before = serde_json::to_string(&original).unwrap();
    let copy = copy_for_hero(&original, "Haze", NEW, None);
    assert_eq!(serde_json::to_string(&original).unwrap(), before);
    let report = copy.report;
    assert_eq!(report.from_hero, "Lady Geist");
    for line in ["Your model file", "Your gun file", "Body roles on your model's bones (2)", "Moving parts (2 groups)", "Material images and settings (1 material)",
        "Color and surface changes of your model's materials (1)", "Physics fine-tuning and preset", "1 item icon", "Face expression and face details"] {
        assert!(report.carried.iter().any(|l| l == line), "carried {line}: {:?}", report.carried);
    }
    for line in ["2 sounds", "1 effect file", "1 hero icon", "1 hero color", "Equipment choices on the hero's meshes and bones (1)", "The tested cloth setup", "The gun fit", "Settings this version does not know"] {
        assert!(report.not_carried.iter().any(|l| l == line), "not carried {line}: {:?}", report.not_carried);
    }
    assert_eq!(report.remade, ["Joint matches", "Mesh mapping", "The prepared model", "VPK slot"]);
}

#[test]
fn clips_taken_from_another_hero_are_dropped_with_the_other_animation_rows() {
    // A taken row names this hero's clip and the other hero's clip: both belong to the old pair.
    let mut original = source();
    original.customization.animations = Some(vec![serde_json::from_value(json!({
        "enabled": true, "target": "models/heroes_wip/geist/clips/dash_ground.vnmclip_c",
        "taken": {"hero": "models/heroes_staging/haze/haze.vmdl", "clip": "models/heroes_staging/haze/clips/dash_ground.vnmclip_c", "arms": "own"}
    })).unwrap()]);
    original.customization.validate().unwrap();
    let copy = copy_for_hero(&original, "Haze", NEW, None);
    assert!(copy.project.customization.animations.is_none());
    assert!(copy.report.not_carried.iter().any(|l| l == "1 animation clip"), "{:?}", copy.report.not_carried);
}

#[test]
fn takes_the_users_own_model_file_and_keeps_a_chosen_texture_folder() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("sample.fbx");
    std::fs::write(&file, b"model").unwrap();
    let mut original = source();
    original.textures = "D:/Characters/sample/textures".into();
    let copy = copy_for_hero(&original, "Haze", NEW, Some(file.to_string_lossy().into_owned()));
    assert_eq!(copy.project.preparation_source, file.to_string_lossy());
    assert_eq!(copy.project.textures, "D:/Characters/sample/textures");
    // A file that is gone is not used.
    let copy = copy_for_hero(&original, "Haze", NEW, Some("D:/gone/sample.fbx".into()));
    assert_eq!(copy.project.preparation_source, original.preparation_source);
}

#[test]
fn names_and_paths() {
    assert_eq!(copy_name("my_character", "Mo & Krill"), "my_character Mo Krill");
    assert_eq!(copy_name("", "Haze"), "Character Haze");
    assert!(copy_name(&"a".repeat(60), "Seven").len() <= 41);
    assert!(copy_name(&"a".repeat(60), "Seven").ends_with(" Seven"));
    assert_eq!(moved_material("models/heroes_wip/geist/materials/prepared/a_12345678.vmat", OLD, NEW), "models/heroes_staging/haze/materials/prepared/a_12345678.vmat");
    assert_eq!(moved_material("materials/other.vmat", OLD, NEW), "materials/other.vmat");
}
