//! The Motion page's clip list: its own read-only command, kept per game
//! archive and hero, never an operation (no lease), and exactly the rows the
//! build accepts as targets. One test, because the kept list is shared by the
//! whole process.
use deadlock_desktop::{app::Application, clips};
use deadlock_project::{Project, Settings};
use std::path::Path;

const HERO: &str = "models/heroes_wip/test/test.vmdl_c";

#[test]
fn the_motion_list_is_its_own_lease_free_kept_read() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let mut project = Project::default();
    project.original_model = HERO.into();
    let error = app.motion_catalog(project.clone(), false).unwrap_err();
    assert!(error.to_string().contains(clips::CLIPS_NO_GAME), "{error}");

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/deadlock-components/tests/fixtures/components_dir.vpk");
    let pak = temp.path().join("pak01_dir.vpk");
    std::fs::copy(&fixture, &pak).unwrap();
    let mut settings = Settings::default();
    settings.pak = pak.display().to_string();
    app.save_settings(&settings).unwrap();

    let before = app.get_timings().map(|t| t.run_id);
    let list = app.motion_catalog(project.clone(), false).unwrap();
    assert_eq!(app.get_timings().map(|t| t.run_id), before, "no operation ran");
    assert_eq!(list.hero_path, HERO);
    let archive = deadlock_vpk::Vpk::open(&pak).unwrap();
    let allowed = deadlock_components::motion::discover(&archive, HERO, &deadlock_core::CancellationToken::new(), |_| {}).unwrap();
    assert_eq!(list.animations.iter().map(|a| a.row.target.clone()).collect::<Vec<_>>(), allowed.iter().map(|r| r.target.clone()).collect::<Vec<_>>(), "exactly the build's targets");
    let json = serde_json::to_value(&list).unwrap();
    for key in ["target", "label", "frames", "duration", "tracks", "events", "name", "group", "graph_named", "path", "supported"] {
        assert!(json["animations"][0].get(key).is_some(), "{key}");
    }
    // Kept: a second call reads nothing (the archive file may even be gone for a moment).
    assert_eq!(serde_json::to_value(app.motion_catalog(project.clone(), false).unwrap()).unwrap(), json);
    let both = std::thread::scope(|s| {
        let a = s.spawn(|| app.motion_catalog(project.clone(), true));
        let b = s.spawn(|| app.motion_catalog(project.clone(), false));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert!(both.0.is_ok() && both.1.is_ok(), "two reads at once both succeed");
    // Another hero is its own read (and an unknown one says so).
    let mut other = project.clone();
    other.original_model = "models/heroes_wip/none/none.vmdl_c".into();
    assert!(app.motion_catalog(other, false).is_err());
}
