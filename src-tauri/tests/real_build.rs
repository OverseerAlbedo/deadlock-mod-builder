//! A real project built end to end into a separate workspace (run with
//! --ignored): DMB_SETTINGS is the app's settings.json, DMB_PROJECT a saved
//! project, DMB_BUILD_WORKSPACE an empty folder. The copy gets its own project
//! ID and compiles only: no VPK, nothing installed. DMB_BUILD_STATE (optional)
//! keeps the app's state folder (timing reports, tool logs) for inspection.
#[test]
#[ignore = "needs the real tools, game and a prepared project"]
fn a_real_project_compiles_with_the_current_game_files() {
    let (Ok(settings), Ok(project), Ok(workspace)) = (std::env::var("DMB_SETTINGS"), std::env::var("DMB_PROJECT"), std::env::var("DMB_BUILD_WORKSPACE")) else { return };
    let temp = tempfile::tempdir().unwrap();
    let state = std::env::var_os("DMB_BUILD_STATE").map_or_else(|| temp.path().join("state"), std::path::PathBuf::from);
    let app = deadlock_desktop::app::Application::new(state).unwrap();
    let mut settings: deadlock_project::Settings = serde_json::from_str(&std::fs::read_to_string(settings).unwrap()).unwrap();
    settings.workspace = workspace;
    app.save_settings(&settings).unwrap();
    let mut project = deadlock_project::Project::load(std::path::Path::new(&project)).unwrap();
    project.id = "0123456789abcdef0123456789abcdef".into();
    project.build_vpk = false;
    project.install = false;
    let inspection = app.inspect_project(project.clone()).unwrap();
    println!("inspected: {:?}", serde_json::to_value(&inspection).ok().and_then(|v| v.get("warnings").cloned()));
    let built = app.build_project(project, true, false, Some(temp.path().join("copy.dmb.json")));
    let entries: Vec<_> = app.log_entries().into_iter().filter(|e| !e.message.starts_with("Step time")).collect();
    for entry in &entries[entries.len().saturating_sub(40)..] {
        println!("{}", entry.message);
    }
    let built = built.unwrap();
    println!("compiled: {}", serde_json::to_value(&built.result).unwrap()["compiled"]);
}
