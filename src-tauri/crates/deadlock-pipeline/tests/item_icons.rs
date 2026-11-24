//! An item icon (Extras → Items) through the real asset-only build, its
//! manifest check and install, without a native compiler.
use deadlock_core::{
    process::{Runner, RunnerOptions},
    safety, CancellationToken, TimingRecorder,
};
use deadlock_pipeline::{build::BuildService, inspect::ModelPipeline};
use deadlock_project::{customization::ResourceReplacement, Project, Settings};
use deadlock_vpk::{write_vpk, Vpk};
use std::{fs, path::Path};

const MAGAZINE: &str = "panorama/images/items/weapon/basic_magazine_psd.vtex_c";

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../deadlock-components/tests/fixtures").join(name)
}
/// A Panorama RGBA texture that is replaced without the resource compiler.
fn texture(size: u16) -> Vec<u8> {
    let mut data = vec![0u8; 40 + 24 + 1024 + 128];
    data[..2].copy_from_slice(&1u16.to_le_bytes());
    data[2..4].copy_from_slice(&395u16.to_le_bytes());
    data[20..22].copy_from_slice(&size.to_le_bytes());
    data[22..24].copy_from_slice(&size.to_le_bytes());
    data[24..26].copy_from_slice(&1u16.to_le_bytes());
    data[26] = 4;
    data[27] = 1;
    data[32..36].copy_from_slice(&8u32.to_le_bytes());
    data[36..40].copy_from_slice(&2u32.to_le_bytes());
    for (at, kind, offset, length) in [(40, 1u32, 20u32, 1024u32), (52, 3, 1032, 128)] {
        data[at..at + 4].copy_from_slice(&kind.to_le_bytes());
        data[at + 4..at + 8].copy_from_slice(&offset.to_le_bytes());
        data[at + 8..at + 12].copy_from_slice(&length.to_le_bytes());
    }
    data[1090..1092].copy_from_slice(&size.to_le_bytes());
    data[1092..1094].copy_from_slice(&size.to_le_bytes());
    let mut raw = vec![];
    raw.extend((28 + data.len() as u32).to_le_bytes());
    raw.extend(12u16.to_le_bytes());
    raw.extend(1u16.to_le_bytes());
    raw.extend(8u32.to_le_bytes());
    raw.extend(1u32.to_le_bytes());
    raw.extend(b"DATA");
    raw.extend(8u32.to_le_bytes());
    raw.extend((data.len() as u32).to_le_bytes());
    raw.extend(data);
    raw.extend(vec![200u8; usize::from(size) * usize::from(size) * 4]);
    raw
}

#[test]
fn an_item_icon_builds_and_installs_as_an_asset_only_mod() {
    let temp = tempfile::tempdir().unwrap();
    let token = CancellationToken::new();
    // The hero fixture plus one sold item and its picture.
    let tree = temp.path().join("game-source");
    let base = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let mut paths = vec![];
    for entry in base.entries.values() {
        safety::atomic_write(&tree.join(&entry.path), base.read(&entry.path).unwrap()).unwrap();
        paths.push(entry.path.clone());
    }
    for (path, raw) in [
        ("scripts/abilities.vdata", br#"{ upgrade_clip_size = { m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_WeaponMod" m_iItemTier = "EModTier_1" m_strShopIconLarge = "file://{images}/items/weapon/basic_magazine.psd" } }"#.to_vec()),
        (MAGAZINE, texture(64)),
    ] {
        safety::atomic_write(&tree.join(path), raw).unwrap();
        paths.push(path.into());
    }
    let pak = temp.path().join("pak42_dir.vpk");
    write_vpk(&tree, &paths, &pak, None, 2, &mut || Ok(())).unwrap();
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("workspace").display().to_string();
    settings.deadlock = temp.path().join("deadlock").display().to_string();
    settings.pak = pak.display().to_string();
    safety::atomic_copy(&pak, &Path::new(&settings.deadlock).join("game/citadel/pak01_dir.vpk"), &token).unwrap();
    let clock = TimingRecorder::new("item icon build");
    let runner = Runner::new(temp.path().join("logs"), token.clone(), RunnerOptions::default()).unwrap();
    let mut project = Project::default();
    project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    project.install = false;
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.item_icons = Some(vec![ResourceReplacement {
        enabled: Some(true),
        source: Some(fixture("icons/profile.png").display().to_string()),
        target: Some(MAGAZINE.into()),
        fit: Some("contain".into()),
        ..Default::default()
    }]);
    let service = BuildService {
        model: ModelPipeline { settings: &settings, runner: &runner, cancel: &token, clock: &clock, log: &|_| {}, reader_root: temp.path() },
    };
    let result = service.build(&mut project, false, false).unwrap();
    let output = result.vpk.unwrap();
    let built = Vpk::open(&output).unwrap();
    assert_eq!(built.entries.keys().collect::<Vec<_>>(), [MAGAZINE], "an asset-only VPK holds exactly the item icon");
    assert!(service.read_manifest(&project).unwrap().customization.unwrap().item_icons.entries.len() == 1);
    service.install(&mut project, &output, false).unwrap();
    let installed = Path::new(&settings.deadlock).join("game/citadel/addons").join(output.file_name().unwrap());
    assert_eq!(fs::read(&installed).unwrap(), fs::read(&output).unwrap());
    // Changing the saved item icon after the build stops a reinstall.
    project.customization.item_icons = None;
    assert!(service.install(&mut project, &output, true).is_err());
}
