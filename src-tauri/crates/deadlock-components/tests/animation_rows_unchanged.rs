//! Animation rows without a made clip (`made` absent) load, validate, stage
//! and save exactly as before clips could be made from an animation: the
//! digests below hold what that code wrote (the staged digest with each
//! source path cut to its file name, so it does not depend on where the
//! checkout is) and must never change.
use deadlock_components::motion;
use deadlock_core::CancellationToken;
use deadlock_project::customization::{Customization, ResourceReplacement};
use deadlock_vpk::Vpk;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MODEL: &str = "models/heroes_wip/test/test.vmdl_c";
const TARGET: &str = "models/heroes_wip/test/idle.vnmclip_c";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Saved rows, as a project file holds them, through load and save.
fn saved_rows_digest() -> String {
    let text = serde_json::json!({
        "enabled": true,
        "animations": [
            {"enabled": true, "source": "C:/clips/mine.vnmclip_c", "target": TARGET, "allow_shared": false},
            {"enabled": false, "source": "C:/clips/other.vnmclip", "target": "models/heroes_wip/test/run.vnmclip_c", "original_sha256": "a".repeat(64)},
            {"source": "", "target": ""}
        ]
    })
    .to_string();
    let customization: Customization = serde_json::from_str(&text).unwrap();
    customization.validate().unwrap();
    hex(serde_json::to_string(&customization).unwrap().as_bytes())
}

/// The receipt and staged bytes of a compiled and an authored row.
fn staged_digest() -> String {
    let archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let cancel = CancellationToken::new();
    let dir = tempfile::tempdir().unwrap();
    let compiled = dir.path().join("mine.vnmclip_c");
    fs::write(&compiled, fs::read(fixture("clip-replaced.bin")).unwrap()).unwrap();
    let mut parts = String::new();
    for row in [
        ResourceReplacement { enabled: Some(true), source: Some(compiled.to_string_lossy().into()), target: Some(TARGET.into()), ..Default::default() },
        ResourceReplacement { source: Some(fixture("action.vnmclip").to_string_lossy().into()), target: Some(TARGET.into()), ..Default::default() },
    ] {
        let content = dir.path().join("content");
        let game = dir.path().join("game");
        let _ = fs::remove_dir_all(&content);
        let _ = fs::remove_dir_all(&game);
        let receipt = motion::stage(&archive, MODEL, &[row], &content, &game, &cancel, false).unwrap();
        // Where the checkout and this run's folders are is not part of the
        // contract: each source path keeps only its file name.
        let mut shown = receipt.clone();
        for entry in &mut shown.replacements {
            let name = entry.source.to_string_lossy().rsplit(['\\', '/']).next().unwrap().to_string();
            entry.source = PathBuf::from(name);
        }
        let json = serde_json::to_string(&shown).unwrap();
        for folder in [env!("CARGO_MANIFEST_DIR").to_string(), dir.path().to_string_lossy().into_owned()] {
            for form in [folder.replace('/', "\\"), folder.replace('\\', "/")] {
                assert!(!json.contains(&form) && !json.contains(&form.replace('\\', "\\\\")), "a local folder in the digest: {json}");
            }
        }
        parts.push_str(&json);
        for file in receipt.files.iter().chain(&receipt.compile_sources) {
            let place = if receipt.files.contains(file) { game.join(file) } else { content.join(file) };
            parts.push_str(&hex(&fs::read(place).unwrap()));
        }
    }
    hex(parts.as_bytes())
}

#[test]
fn rows_without_a_made_clip_save_and_stage_exactly_as_before() {
    let saved = saved_rows_digest();
    let staged = staged_digest();
    println!("saved {saved}\nstaged {staged}");
    assert_eq!(saved, "beced6d5885eb2f7d992730d7bfc54a2024ec1d0323f768701a29937af6465bf");
    assert_eq!(staged, "24c94d9571a802c3d2697dfc2a3b54f827b0886331435d05c64552ad000f4f3a");
}
