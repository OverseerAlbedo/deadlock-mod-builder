use deadlock_project::{asset_paths, customization::Customization, Project, Settings};
use serde_json::{json, Value};
use std::{fs, path::Path};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/reference.json")).unwrap()
}
fn strings(value: &mut Value, transform: &impl Fn(&str) -> String) {
    match value {
        Value::String(s) => *s = transform(s),
        Value::Array(v) => {
            for x in v {
                strings(x, transform);
            }
        }
        Value::Object(v) => {
            for x in v.values_mut() {
                strings(x, transform);
            }
        }
        _ => {}
    }
}
fn unpack(value: &Value, root: &Path) -> Value {
    let mut result = value.clone();
    strings(&mut result, &|s| {
        s.replace("${ROOT}", root.to_str().unwrap())
    });
    result
}
fn normalize(value: &mut Value, root: &Path) {
    strings(value, &|s| s.replace(root.to_str().unwrap(), "${ROOT}"));
}
fn same(a: &Value, b: &Value, location: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{location}"),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect::<std::collections::BTreeSet<_>>(),
                "{location} keys"
            );
            for (key, a) in a {
                same(a, &b[key], &format!("{location}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{location} length");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                same(a, b, &format!("{location}[{i}]"));
            }
        }
        _ => assert_eq!(a, b, "{location}"),
    }
}
#[test]
#[cfg(not(windows))]
fn python_reference_persistence_and_rebasing() {
    let golden = fixture();
    for case in golden["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        for file in golden["files"].as_array().unwrap() {
            let path = root.join(file.as_str().unwrap());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }
        let input = unpack(case, root);
        let name = input["name"].as_str().unwrap();
        let mut output = match input["operation"].as_str().unwrap() {
            "clear_targets" => {
                let mut state: Customization =
                    serde_json::from_value(input["customization"].clone()).unwrap();
                state.clear_targets();
                serde_json::to_value(state).unwrap()
            }
            "settings" => {
                let path = root.join("settings.json");
                fs::write(&path, serde_json::to_vec(&input["settings"]).unwrap()).unwrap();
                serde_json::to_value(Settings::load(&path).unwrap()).unwrap()
            }
            operation => {
                let mut project: Project = serde_json::from_value(input["project"].clone())
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
                match operation {
                    "save" => {
                        let path = root.join(input["path"].as_str().unwrap());
                        project
                            .save(&path)
                            .unwrap_or_else(|e| panic!("{name}: {e}"));
                        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
                    }
                    "load" => {
                        let path = root.join(input["path"].as_str().unwrap());
                        fs::create_dir_all(path.parent().unwrap()).unwrap();
                        fs::write(&path, serde_json::to_vec(&input["project"]).unwrap()).unwrap();
                        serde_json::to_value(
                            Project::load(&path).unwrap_or_else(|e| panic!("{name}: {e}")),
                        )
                        .unwrap()
                    }
                    "rebase" => {
                        if input["initial_rebase"].as_bool() == Some(true) {
                            project.rebase_assets("", "").unwrap();
                        }
                        if let Some(v) = input["changes"]["dmx"].as_str() {
                            project.dmx = v.into();
                        }
                        if let Some(v) = input["changes"]["textures"].as_str() {
                            project.textures = v.into();
                        }
                        project
                            .rebase_assets("", "")
                            .unwrap_or_else(|e| panic!("{name}: {e}"));
                        serde_json::to_value(project).unwrap()
                    }
                    _ => unreachable!(),
                }
            }
        };
        normalize(&mut output, root);
        same(&output, &case["expected"], name);
    }
}
#[test]
fn python_reference_rejected_profiles_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let golden = fixture();
    for case in golden["invalid"].as_array().unwrap() {
        let case = unpack(case, temporary.path());
        let result = serde_json::from_value::<Project>(case["project"].clone())
            .map_err(|e| e.to_string())
            .and_then(|mut p| {
                if case["operation"] == "rebase" {
                    p.rebase_assets("", "")
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                } else {
                    p.validate(case["action"].as_bool().unwrap_or(false))
                        .map_err(|e| e.to_string())
                }
            });
        if case["name"] == "field_name" {
            // Intentional alpha.2 UX change requested by the user: display
            // names may contain spaces and capitals. Keep this legacy oracle
            // case visible as an explicit difference rather than changing it.
            assert!(result.is_ok(), "friendly display name should be accepted");
            continue;
        }
        assert!(
            result.is_err(),
            "accepted reference-invalid profile: {}",
            case["name"]
        );
    }
}
#[test]
#[cfg(not(windows))]
fn python_reference_lexical_paths() {
    for case in fixture()["path_cases"].as_array().unwrap() {
        let args = case["arguments"].as_array().unwrap();
        let a = args[0].as_str().unwrap();
        let got = match case["operation"].as_str().unwrap() {
            "absolute" => json!(asset_paths::is_absolute(a)),
            "relative_asset" => json!(asset_paths::relative_asset(a).unwrap()),
            "relative_to" => json!(asset_paths::relative_to(a, args[1].as_str().unwrap()).unwrap()),
            _ => unreachable!(),
        };
        assert_eq!(got, case["expected"], "{case}");
    }
}
#[test]
fn draft_save_is_atomic_and_action_still_requires_hero() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("draft.dmb.json");
    let mut p = Project::default();
    p.save(&path).unwrap();
    let before = fs::read(&path).unwrap();
    let loaded = Project::load(&path).unwrap();
    assert_eq!(loaded.id, p.id);
    assert!(loaded.validate_draft().is_ok());
    assert!(loaded.validate_action().is_err());
    p.original_model = "../escape.vmdl".into();
    assert!(p.save(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
}
#[test]
fn absent_vs_null_preserved_and_invalid_optional_null_rejected() {
    let mut p:Project=serde_json::from_value(json!({"materials":{"a.vmat":{"roughness":null,"metalness":0.0,"future":null}},"preparation_options":{"landmarks":{"head":[]},"reviewed":null},"customization":{"future":null}})).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("draft.json");
    p.save(&path).unwrap();
    let v: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(v["materials"]["a.vmat"]["roughness"].is_null());
    assert!(v["materials"]["a.vmat"].get("roughness").is_some());
    assert!(v["materials"]["a.vmat"].get("color").is_none());
    assert!(v["preparation_options"]["reviewed"].is_null());
    assert!(v["customization"].get("future").is_some());
    assert!(serde_json::from_value::<Project>(json!({"customization":{"enabled":null}})).is_err());
}
#[test]
fn missing_recovery_requires_unique_directory_anchor() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let first = root.join("one/package");
    let second = root.join("two/package");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("thing.dmx"), b"x").unwrap();
    fs::write(second.join("thing.dmx"), b"y").unwrap();
    let first = first.to_str().unwrap();
    let second = second.to_str().unwrap();
    assert!(
        asset_paths::recover("C:/old/package/thing.dmx", &[first, second])
            .unwrap()
            .is_none()
    );
    assert!(asset_paths::recover("/unrelated/thing.dmx", &[first])
        .unwrap()
        .is_none());
    assert!(asset_paths::recover("C:/old/package/thing.dmx", &[first])
        .unwrap()
        .is_some());
}
#[test]
fn portable_sources_move_without_old_folder() {
    let temporary = tempfile::tempdir().unwrap();
    let old = temporary.path().join("original");
    let new = temporary.path().join("moved");
    fs::create_dir_all(old.join("textures")).unwrap();
    fs::write(old.join("modèle with spaces.dmx"), b"original").unwrap();
    fs::write(old.join("textures/body.png"), b"image").unwrap();
    let mut p:Project=serde_json::from_value(json!({"dmx":old.join("modèle with spaces.dmx").to_str().unwrap(),"textures":old.join("textures").to_str().unwrap(),"materials":{"a.vmat":{"textures":{"color":old.join("textures/body.png").to_str().unwrap()}}}})).unwrap();
    p.save(&old.join("project.json")).unwrap();
    fs::rename(&old, &new).unwrap();
    let loaded = Project::load(&new.join("project.json")).unwrap();
    // Loaded paths are resolved native paths (Windows separators and long names).
    let resolved = |p: &Path| asset_paths::text(&asset_paths::resolve(p).unwrap()).unwrap();
    assert_eq!(loaded.dmx, resolved(&new.join("modèle with spaces.dmx")));
    assert_eq!(
        loaded.materials["a.vmat"].textures.as_ref().unwrap()["color"],
        resolved(&new.join("textures").join("body.png"))
    );
}
#[cfg(unix)]
#[test]
fn symlinks_and_non_unicode_filesystem_paths_never_lose_information() {
    use std::ffi::OsString;
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let temporary = tempfile::tempdir().unwrap();
    let real = temporary.path().join("real/nested");
    fs::create_dir_all(&real).unwrap();
    let alias = temporary.path().join("alias");
    symlink(&real, &alias).unwrap();
    let actual = asset_paths::resolve(&alias.join("../missing.dmx")).unwrap();
    assert_eq!(actual, temporary.path().join("real/missing.dmx"));
    assert!(asset_paths::text(Path::new(&OsString::from_vec(vec![b'x', 0xff]))).is_err());
}
