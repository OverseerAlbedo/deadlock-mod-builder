//! One saved project taken through the app's own pipeline (optional
//! preparation, Analyze, build) on a copy, for checking a toolset change end to
//! end (run with --ignored). Driven only by environment variables:
//!
//! - `DMB_E2E_SETTINGS`: a settings.json to copy (only read).
//! - `DMB_E2E_PROJECT`: a saved project (only read).
//! - `DMB_E2E_SCRATCH`: a new or empty folder for everything the run writes:
//!   the settings copy and app state, a new workspace, the project copy and
//!   `e2e-report.json`.
//! - `DMB_E2E_OVERRIDES` (optional): a JSON object of settings fields to
//!   change in the copy, for example `{"cswin":"","cswin_resourcecompiler":"",
//!   "model_game":"","model_backend":"auto"}` to compile models with the
//!   Community SDK.
//! - `DMB_E2E_PROJECT_OVERRIDES` (optional): a JSON object merged into the
//!   project copy, for example `{"customization":{"secondary_motion":
//!   {"enabled":false}}}` for Preserve original physics.
//! - `DMB_E2E_PREPARE` (optional, `1`): prepare the character again from the
//!   project's saved source and options before Analyze, as the app asks after a
//!   game update changed the hero. Choices the user had made after preparing
//!   (mesh mapping, confirmed materials, reviewed LODs) are carried over when
//!   they still apply.
//!
//! The copy gets a new project ID, so its compiler folders are new
//! `dmb_<id>` folders next to (never replacing) the user's own. Its prepared
//! files are copied into the new workspace. VPK output stays in the scratch
//! workspace and nothing is installed. The run refuses a scratch folder inside
//! the user's workspace, project folder, Community SDK, model toolset or game,
//! and checks afterwards that the game's addons folder is unchanged. It lists
//! every compiler folder it created; deleting them is left to the user.
use deadlock_project::{Project, Settings};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase()
}
fn inside(path: &Path, root: &str) -> bool {
    !root.trim().is_empty() && {
        let (path, root) = (key(path), key(Path::new(root.trim())));
        path == root || path.starts_with(&format!("{root}/"))
    }
}
/// Every string in `value`, with mutable access.
fn strings(value: &mut Value, visit: &mut dyn FnMut(&mut String)) {
    match value {
        Value::String(text) => visit(text),
        Value::Array(items) => items.iter_mut().for_each(|v| strings(v, visit)),
        Value::Object(map) => map.values_mut().for_each(|v| strings(v, visit)),
        _ => {}
    }
}
fn copy_tree(from: &Path, to: &Path) {
    if from.is_file() {
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(from, to).unwrap();
        return;
    }
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        copy_tree(&entry.path(), &to.join(entry.file_name()));
    }
}
/// File name → (size, modification time) of every entry of `folder`, recursively.
fn listing(folder: &Path) -> BTreeMap<String, (u64, Option<std::time::SystemTime>)> {
    let mut out = BTreeMap::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).into_iter().flatten().flatten() {
            let path = entry.path();
            let meta = entry.metadata().ok();
            out.insert(key(&path), (meta.as_ref().map_or(0, |m| m.len()), meta.and_then(|m| m.modified().ok())));
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    out
}
/// The `*_addons` folders of a toolset root (Community SDK or model toolset).
fn addon_parents(root: &str) -> Vec<PathBuf> {
    let mut out = vec![];
    if root.trim().is_empty() {
        return out;
    }
    for side in ["content", "game"] {
        for entry in fs::read_dir(Path::new(root.trim()).join(side)).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().ends_with("_addons") && entry.path().is_dir() {
                out.push(entry.path());
            }
        }
    }
    out
}
fn children(parents: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = parents
        .iter()
        .flat_map(|p| fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path().to_string_lossy().into_owned()))
        .collect();
    out.sort();
    out
}

#[test]
#[ignore = "needs the real tools, the game and a saved project"]
fn a_project_copy_builds_end_to_end_with_the_chosen_tools() {
    let (Ok(settings_file), Ok(project_file), Ok(scratch)) = (
        std::env::var("DMB_E2E_SETTINGS"),
        std::env::var("DMB_E2E_PROJECT"),
        std::env::var("DMB_E2E_SCRATCH"),
    ) else {
        eprintln!("DMB_E2E_SETTINGS, DMB_E2E_PROJECT and DMB_E2E_SCRATCH are not set; skipped.");
        return;
    };
    let scratch = PathBuf::from(scratch);
    let original: Settings = serde_json::from_str(&fs::read_to_string(&settings_file).unwrap()).unwrap();
    let mut settings = original.clone();
    if let Some(overrides) = std::env::var("DMB_E2E_OVERRIDES").ok().filter(|v| !v.trim().is_empty()) {
        let mut value = serde_json::to_value(&settings).unwrap();
        for (field, new) in serde_json::from_str::<serde_json::Map<String, Value>>(&overrides).unwrap() {
            assert!(value.get(&field).is_some(), "unknown settings field {field}");
            value[&field] = new;
        }
        settings = serde_json::from_value(value).unwrap();
    }
    // The scratch folder must be new ground: never inside anything of the user's.
    let project_dir = Path::new(&project_file).parent().unwrap().to_path_buf();
    for (what, root) in [
        ("workspace", original.workspace.as_str()),
        ("project folder", &project_dir.to_string_lossy()),
        ("Community SDK", original.csdk.as_str()),
        ("model toolset", original.cswin.as_str()),
        ("Deadlock", original.deadlock.as_str()),
        ("settings folder", &Path::new(&settings_file).parent().unwrap().to_string_lossy()),
    ] {
        assert!(!inside(&scratch, root), "DMB_E2E_SCRATCH is inside the {what}; choose a separate folder");
    }
    assert!(!scratch.exists() || fs::read_dir(&scratch).unwrap().next().is_none(), "DMB_E2E_SCRATCH must be new or empty");
    fs::create_dir_all(&scratch).unwrap();
    let workspace = scratch.join("workspace");
    let state = scratch.join("state");
    settings.workspace = workspace.to_string_lossy().into_owned();
    settings.last_project.clear();
    settings.clear_build_data_after_build = false;

    // The project copy: a new ID, its prepared files copied into the new workspace.
    let mut project = Project::load(Path::new(&project_file)).unwrap();
    let old_id = project.id.clone();
    let new_id = uuid::Uuid::new_v4().simple().to_string();
    let old_root = key(&Path::new(&original.workspace).join(&old_id));
    let new_root = workspace.join(&new_id);
    let mut value = serde_json::to_value(&project).unwrap();
    let mut copied = BTreeMap::new();
    strings(&mut value, &mut |text| {
        let normalized = text.replace('\\', "/").to_lowercase();
        if let Some(rest) = normalized.strip_prefix(&format!("{old_root}/")) {
            let original_rest = &text.replace('\\', "/")[text.len() - rest.len()..];
            // Copy the referenced folder (humanoid_preparation/<run>) once.
            let parts: Vec<&str> = original_rest.split('/').collect();
            let unit = parts[..parts.len().min(2)].join("/");
            copied.entry(unit.clone()).or_insert_with(|| {
                let from = Path::new(&original.workspace).join(&old_id).join(&unit);
                copy_tree(&from, &new_root.join(&unit));
                from
            });
            *text = new_root.join(original_rest).to_string_lossy().into_owned();
        }
    });
    if let Some(overrides) = std::env::var("DMB_E2E_PROJECT_OVERRIDES").ok().filter(|v| !v.trim().is_empty()) {
        fn merge(into: &mut Value, from: Value) {
            match (into, from) {
                (Value::Object(into), Value::Object(from)) => {
                    for (k, v) in from {
                        merge(into.entry(k).or_insert(Value::Null), v);
                    }
                }
                (into, from) => *into = from,
            }
        }
        merge(&mut value, serde_json::from_str(&overrides).unwrap());
    }
    project = serde_json::from_value(value).unwrap();
    let leftover = serde_json::to_string(&project).unwrap().replace("\\\\", "/").to_lowercase();
    assert!(!leftover.contains(&old_root), "the copy still points into the user's workspace");
    project.id = new_id.clone();
    project.name = format!("{} e2e", project.name.chars().take(37).collect::<String>());
    project.install = false;
    let copy_path = scratch.join("project.dmb.json");
    project.save(&copy_path).unwrap();
    let project = Project::load(&copy_path).unwrap();

    let mut addons_parents: Vec<PathBuf> = [settings.csdk.as_str(), settings.cswin.as_str(), original.csdk.as_str(), original.cswin.as_str()]
        .into_iter()
        .flat_map(addon_parents)
        .collect();
    addons_parents.sort_by_key(|p| key(p));
    addons_parents.dedup_by_key(|p| key(p));
    let before_addons = children(&addons_parents);
    let game_addons = Path::new(&original.deadlock).join("game/citadel/addons");
    let before_game = listing(&game_addons);

    let app = deadlock_desktop::app::Application::new(state.clone()).unwrap();
    app.save_settings(&settings).unwrap();
    let started = Instant::now();
    let mut report = serde_json::json!({
        "project": project.name, "id": new_id, "copied_from_workspace": copied.keys().collect::<Vec<_>>(),
        "settings": {"csdk": settings.csdk, "resourcecompiler": settings.resourcecompiler, "cswin": settings.cswin,
            "cswin_resourcecompiler": settings.cswin_resourcecompiler, "model_game": settings.model_game,
            "model_backend": settings.model_backend, "source2viewer": settings.source2viewer},
    });
    let outcome = (|| -> Result<Value, String> {
        let mut project = project.clone();
        // Materials the user had confirmed on the Materials page, by prepared
        // name (its hash is the material's identity across preparations).
        let confirmed: Vec<String> = serde_json::to_value(&project.materials)
            .unwrap()
            .as_object()
            .map(|m| m.iter().filter(|(_, c)| c["confirmed"] == true).map(|(k, _)| k.clone()).collect())
            .unwrap_or_default();
        let prepare = std::env::var("DMB_E2E_PREPARE").as_deref() == Ok("1");
        let reviewed_lods = project.review_lods;
        let mut restored_all = false;
        if prepare {
            let step = Instant::now();
            // As Model preparation's Prepare again: the same source and saved
            // choices, confirmed (the review the click stands for).
            let source = project.preparation_source.clone();
            let rotation = std::env::var("DMB_E2E_ROTATION")
                .ok()
                .and_then(|v| serde_json::from_str::<[f64; 3]>(&v).ok())
                .unwrap_or([0.0; 3]);
            let analysis = app.preparation_inspect(project.clone(), source, rotation).map_err(|e| format!("prepare (import): {e}"))?;
            let mut options = project.preparation_options.clone();
            options.values.insert("reviewed".into(), true.into());
            if options.values.get("mapping_anchors").and_then(Value::as_object).is_some_and(|m| !m.is_empty()) {
                options.values.insert("follow_motion_reviewed".into(), true.into());
            }
            project.preparation_options = options.clone();
            let issues = app.preparation_review(&project, &analysis, &options).map_err(|e| format!("prepare (review): {e}"))?;
            if !issues.is_empty() {
                return Err(format!("prepare (review): {}", serde_json::to_string(&issues).unwrap_or_default()));
            }
            let result = app
                .preparation_convert(project.clone(), analysis, options, false, None)
                .map_err(|e| format!("prepare (convert): {e}"))?;
            let saved_meshes = project.meshes.clone();
            project = app.preparation_accept(project, result).map_err(|e| format!("prepare (accept): {e}"))?;
            // Mesh mapping as the user had chosen it (Mesh mapping keeps a
            // choice whose mesh still exists in the prepared model).
            let names: std::collections::BTreeSet<String> = app
                .inspect_project(project.clone())
                .ok()
                .and_then(|i| serde_json::to_value(&i).ok())
                .and_then(|v| v["report"]["geometry"]["meshes"].as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|m| m["name"].as_str().map(str::to_owned))
                .collect();
            restored_all = true;
            for (target, mapping) in saved_meshes {
                let chosen = serde_json::to_value(&mapping).unwrap();
                let usable = |name: &Value| name.as_str().is_some_and(|n| n.starts_with('@') || names.contains(n));
                if chosen.as_array().map_or(usable(&chosen), |all| all.iter().all(usable)) {
                    project.meshes.insert(target, mapping);
                } else {
                    restored_all = false;
                }
            }
            project.save(&copy_path).unwrap();
            println!("prepared in {:.0} s", step.elapsed().as_secs_f64());
        }
        let step = Instant::now();
        let inspection = app.inspect_project(project.clone()).map_err(|e| format!("Analyze: {e}"))?;
        let analyze_seconds = step.elapsed().as_secs_f64();
        let mut project = inspection.project.clone();
        if prepare {
            // Preparing again asks to confirm the materials again; the copy
            // confirms the ones the user had confirmed, nothing else.
            // A prepared name ends in the material's 8-digit hash, which stays
            // when a newer app spells the readable part differently.
            let identity = |name: &str| name.strip_suffix(".vmat").and_then(|n| n.rsplit_once('_')).map(|(_, hash)| hash.to_owned());
            let hashes: std::collections::BTreeSet<String> = confirmed.iter().filter_map(|n| identity(n)).collect();
            let mut materials = serde_json::to_value(&project.materials).unwrap();
            for (name, choice) in materials.as_object_mut().unwrap() {
                if confirmed.contains(name) || identity(name).is_some_and(|h| h.len() == 8 && hashes.contains(&h)) {
                    choice["confirmed"] = true.into();
                }
            }
            project.materials = serde_json::from_value(materials).unwrap();
            // Preparing again also asks to look at the LODs again; the user's
            // earlier "reviewed" stands when the whole mapping was restored.
            if restored_all {
                project.review_lods = reviewed_lods;
            }
        }
        let step = Instant::now();
        let built = app.build_project(project, true, false, Some(copy_path.clone())).map_err(|e| format!("Build: {e}"))?;
        Ok(serde_json::json!({
            "analyze_seconds": analyze_seconds,
            "build_seconds": step.elapsed().as_secs_f64(),
            "result": serde_json::to_value(&built.result).unwrap_or(Value::Null),
        }))
    })();
    let entries: Vec<String> = app.log_entries().into_iter().map(|e| e.message).filter(|m| !m.starts_with("Step time")).collect();
    for line in &entries[entries.len().saturating_sub(60)..] {
        println!("{line}");
    }
    let after_addons = children(&addons_parents);
    let removed: Vec<&String> = before_addons.iter().filter(|p| !after_addons.contains(p)).collect();
    // Only this copy's folders (another run at the same time may add its own).
    let created: Vec<String> = after_addons.iter().filter(|p| !before_addons.contains(p) && p.contains(&new_id[..16])).cloned().collect();
    report["seconds"] = serde_json::json!(started.elapsed().as_secs_f64());
    report["created_compiler_folders"] = serde_json::json!(created);
    report["log"] = serde_json::json!(entries);
    match &outcome {
        Ok(value) => {
            report["passed"] = true.into();
            report["build"] = value.clone();
        }
        Err(error) => {
            report["passed"] = false.into();
            report["error"] = error.clone().into();
        }
    }
    fs::write(scratch.join("e2e-report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("created compiler folders:");
    for folder in &created {
        println!("  {folder}");
    }
    assert_eq!(listing(&game_addons), before_game, "the game's addons folder changed");
    assert!(removed.is_empty(), "existing compiler folders disappeared: {removed:?}");
    if let Err(error) = outcome {
        panic!("{error}");
    }
}
