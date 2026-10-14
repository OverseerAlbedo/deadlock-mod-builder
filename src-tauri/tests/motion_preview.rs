//! The movement preview of a real project (run with --ignored; DMB_SETTINGS is
//! the app's settings.json, DMB_PROJECT a prepared project).
#[test]
#[ignore = "needs the real tools and a prepared project"]
fn a_prepared_project_gets_idle_run_and_jump_clips() {
    let (Ok(settings), Ok(project)) = (std::env::var("DMB_SETTINGS"), std::env::var("DMB_PROJECT")) else { return };
    let settings: deadlock_project::Settings = serde_json::from_str(&std::fs::read_to_string(settings).unwrap()).unwrap();
    let project = deadlock_project::Project::load(std::path::Path::new(&project)).unwrap();
    let packet = deadlock_pipeline::motion_preview::build(&settings, &project, &deadlock_core::CancellationToken::new()).unwrap();
    let length = u32::from_le_bytes(packet[..4].try_into().unwrap()) as usize;
    let header: serde_json::Value = serde_json::from_slice(&packet[4..4 + length]).unwrap();
    let bones = header["bones"].as_array().unwrap().len();
    let floats = (packet.len() - (4 + length).div_ceil(4) * 4) / 4;
    println!("{bones} bones, {} MB", packet.len() / 1_000_000);
    let mut total = 0;
    for clip in header["clips"].as_array().unwrap() {
        println!("{} {:?} {} frames", clip["name"], clip["sources"], clip["frames"]);
        total += clip["frames"].as_u64().unwrap() as usize;
    }
    assert_eq!(floats, total * bones * 12);
    let names: Vec<&str> = header["clips"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["idle", "run", "jump"]);
}

/// Data for viewing the movement preview outside the app (run with --ignored):
/// a copy of the project is analyzed in DMB_BUILD_WORKSPACE, then the preview
/// scene, the movement packet and the analyzed moving bones are written to
/// DMB_HARNESS_OUT (scene.bin, motion.bin, nodes.json).
#[test]
#[ignore = "needs the real tools and a prepared project"]
fn export_movement_preview_data() {
    let (Ok(settings), Ok(project), Ok(workspace), Ok(out)) = (std::env::var("DMB_SETTINGS"), std::env::var("DMB_PROJECT"), std::env::var("DMB_BUILD_WORKSPACE"), std::env::var("DMB_HARNESS_OUT")) else { return };
    let temp = tempfile::tempdir().unwrap();
    let app = deadlock_desktop::app::Application::new(temp.path().join("state")).unwrap();
    let mut settings: deadlock_project::Settings = serde_json::from_str(&std::fs::read_to_string(settings).unwrap()).unwrap();
    settings.workspace = workspace;
    app.save_settings(&settings).unwrap();
    let mut project = deadlock_project::Project::load(std::path::Path::new(&project)).unwrap();
    project.id = "0123456789abcdef0123456789abcdef".into();
    let inspection = serde_json::to_value(app.inspect_project(project.clone()).unwrap()).unwrap();
    let nodes = &inspection["report"]["secondary_motion"]["preview_nodes"];
    println!("{} moving bones", nodes.as_array().map_or(0, Vec::len));
    let out = std::path::Path::new(&out);
    std::fs::write(out.join("nodes.json"), serde_json::to_vec(nodes).unwrap()).unwrap();
    std::fs::write(out.join("motion.bin"), app.motion_preview(project.clone()).unwrap()).unwrap();
    std::fs::write(out.join("scene.bin"), app.preview_dmx(std::path::Path::new(&project.dmx)).unwrap()).unwrap();
}


#[test]
#[ignore = "debug"]
fn print_landing_candidates() {
    let (Ok(settings), Ok(project)) = (std::env::var("DMB_SETTINGS"), std::env::var("DMB_PROJECT")) else { return };
    let settings: deadlock_project::Settings = serde_json::from_str(&std::fs::read_to_string(settings).unwrap()).unwrap();
    let project = deadlock_project::Project::load(std::path::Path::new(&project)).unwrap();
    let base = deadlock_pipeline::sources::base_dir(&settings, &project).unwrap();
    let model = base.join(project.source_model());
    for (name, parts) in deadlock_pipeline::motion_preview::clips_for(&base, model.parent().unwrap()) {
        for (k, part) in parts.iter().enumerate() {
            for file in part.candidates.iter().take(4) {
                let info = deadlock_preparation::stance::frames(file, 30., 1000).map(|(len, poses)| {
                    let z: Vec<f64> = poses.iter().map(|p| p.get("pelvis").map_or(f64::NAN, |m| m[2][3])).collect();
                    let top: Vec<f64> = poses.iter().map(|p| p.values().map(|m| m[2][3]).fold(f64::MIN, f64::max)).collect();
                    format!("{len:.2}s {} frames pelvis {:.1}..{:.1} top {:.1}", poses.len(), z.iter().cloned().fold(f64::MAX, f64::min), z.iter().cloned().fold(f64::MIN, f64::max), top[0])
                });
                println!("{name} part {k}: {:<34} {}", file.file_name().unwrap().to_string_lossy(), info.unwrap_or_else(|| "unreadable".into()));
            }
        }
    }
}
