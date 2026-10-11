use deadlock_core::{safety, CancellationToken, TimingStatus};
use deadlock_desktop::app::Application;
use deadlock_project::{Project, Settings};
use std::path::{Path, PathBuf};
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/deadlock-components/tests/fixtures/icons")
        .join(name)
}

#[test]
fn preview_command_returns_real_bounded_png_and_measured_steps() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let png = app.preview_image(&fixture("oriented.jpg"), 10).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let (image, _) =
        deadlock_components::icons::decode_image(&png, &CancellationToken::new()).unwrap();
    assert_eq!(image.dimensions(), (7, 10));
    let times = app.get_timings().unwrap();
    assert_eq!(times.status, TimingStatus::Succeeded);
    assert!(times
        .steps
        .iter()
        .any(|s| s.name == "Decode, orient and resize thumbnail" && s.inclusive_ns > 0));
    assert!(app.preview_image(&fixture("source.png"), 4096).is_err());
    assert!(!app.is_busy());
    assert_eq!(app.get_timings().unwrap().status, TimingStatus::Failed);
}

#[test]
fn catalog_uses_current_game_vpk_and_reports_unavailable_optional_animation_without_guessing() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let tree = temp.path().join("tree");
    let hero = "models/heroes_staging/hornet_v3/hornet.vmdl_c";
    let icon = "panorama/images/heroes/vindicta_png.vtex_c";
    // Empty typed-resource stand-in is intentional: the catalog must retain an
    // explicit animation-unavailable reason while discovering a valid image.
    safety::atomic_write(&tree.join(hero), b"not a model").unwrap();
    safety::atomic_write(
        &tree.join(icon),
        &std::fs::read(fixture("embedded.vtex_c")).unwrap(),
    )
    .unwrap();
    let pak = temp.path().join("pak18_dir.vpk");
    deadlock_vpk::write_vpk(
        &tree,
        &[hero.into(), icon.into()],
        &pak,
        None,
        2,
        &mut || Ok(()),
    )
    .unwrap();
    let mut settings = Settings::default();
    settings.pak = pak.display().to_string();
    app.save_settings(&settings).unwrap();
    let mut project = Project::default();
    project.original_model = hero.into();
    let catalog = app.discover_components(project.clone()).unwrap();
    assert_eq!(catalog.icons.len(), 1);
    assert_eq!(catalog.icons[0].target, icon);
    assert!(catalog.icons[0].supported);
    assert!(catalog.animation_unavailable.is_some());
    assert!(app.discover_variants(project.clone()).unwrap().is_empty());
    project.original_model = "models/heroes/haze/haze.vmdl".into();
    assert!(app.discover_components(project).is_err());
}

#[cfg(unix)]
#[test]
fn thumbnail_input_does_not_follow_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    let link = temp.path().join("link.png");
    std::os::unix::fs::symlink(fixture("source.png"), &link).unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    assert!(app.preview_image(&link, 64).is_err());
}

#[test]
fn extras_previews_are_real_read_only_and_bound_to_the_current_character_catalog() {
    use deadlock_desktop::catalogs::ExtraPreviewRequest;
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let tree=temp.path().join("tree");
    let hero="models/heroes_staging/hornet_v3/hornet.vmdl_c";
    let icon="panorama/images/heroes/vindicta_png.vtex_c";
    let sound="sounds/heroes/hornet/voice.vsnd_c";
    let effect="particles/heroes/hornet/card.vpcf_c";
    let root=fixture("source.png").parent().unwrap().parent().unwrap().to_path_buf();
    for (target,raw) in [(hero,deadlock_vpk::Vpk::open(root.join("components_dir.vpk")).unwrap().read(hero).unwrap()),(icon,std::fs::read(fixture("embedded.vtex_c")).unwrap()),(sound,std::fs::read(root.join("sound-data.bin")).unwrap()),(effect,std::fs::read(root.join("particle.bin")).unwrap())]{safety::atomic_write(&tree.join(target),&raw).unwrap();}
    let pak=temp.path().join("pak18_dir.vpk");
    deadlock_vpk::write_vpk(&tree,&[hero.into(),icon.into(),sound.into(),effect.into()],&pak,None,2,&mut||Ok(())).unwrap();
    let settings=Settings{pak:pak.display().to_string(),..Default::default()};app.save_settings(&settings).unwrap();
    let mut project=Project::default();project.original_model=hero.into();
    let catalog=app.discover_extras(project).unwrap();assert!(!catalog.includes_animations);assert!(catalog.animations.is_empty());
    let before=serde_json::to_value(app.get_timings().unwrap()).unwrap();let original_pak=std::fs::read(&pak).unwrap();
    let make=|target:&str,kind:&str|ExtraPreviewRequest{hero_path:hero.into(),target:target.into(),kind:kind.into(),source:None,fit:None,audio_fit:None,max_dimension:Some(64)};
    let unpack=|raw:Vec<u8>|{let len=u32::from_le_bytes(raw[..4].try_into().unwrap()) as usize;let head:serde_json::Value=serde_json::from_slice(&raw[4..4+len]).unwrap();(head,raw[4+len..].to_vec())};
    let (head,png)=unpack(app.preview_extra(make(icon,"icons")).unwrap());assert_eq!(head["mime_type"],"image/png");assert!(png.starts_with(b"\x89PNG"));
    let (head,wave)=unpack(app.preview_extra(make(sound,"audio")).unwrap());assert_eq!(head["mime_type"],"audio/wav");assert_eq!(&wave[..4],b"RIFF");
    let (head,data)=unpack(app.preview_extra(make(effect,"effects")).unwrap());assert!(data.is_empty());assert!(head["details"]["groups"].is_object());
    let mut replacement=make(sound,"audio");replacement.source=Some(root.join("samples.wav").display().to_string());replacement.audio_fit=Some("pad".into());let (_,wave)=unpack(app.preview_extra(replacement).unwrap());assert_eq!(&wave[..4],b"RIFF");
    assert!(!app.is_busy());assert_eq!(serde_json::to_value(app.get_timings().unwrap()).unwrap(),before);assert_eq!(std::fs::read(&pak).unwrap(),original_pak);
    assert!(app.preview_extra(make(icon,"audio")).is_err());assert!(app.preview_extra(make("panorama/images/other.vtex_c","icons")).is_err());
    let mut other=make(icon,"icons");other.hero_path="models/heroes_wip/wraith/wraith.vmdl".into();assert!(app.preview_extra(other).is_err());
    app.save_settings(&Settings{pak:temp.path().join("other_dir.vpk").display().to_string(),..settings}).unwrap();assert!(app.preview_extra(make(icon,"icons")).is_err());
}
