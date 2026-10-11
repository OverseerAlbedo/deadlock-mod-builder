//! Reversible swap of the user's chosen installed mod, with durable recovery.
//! Never writes pak01_dir.vpk or modifies gameinfo/Steam launch settings.
use crate::native::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema: u32,
    target: PathBuf,
    backup: PathBuf,
    original_sha256: String,
    known_test_hashes: Vec<String>,
}
fn checked(path: &Path) -> Result<()> {
    deadlock_vpk::check_unlinked_path(path, true)?;
    Ok(())
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    checked(path)?;
    let temporary = path.with_file_name(format!(
        "{}.dmb-streamer-pending",
        path.file_name().ok_or("Invalid output")?.to_string_lossy()
    ));
    checked(&temporary)?;
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    if let Err(e) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(e.into());
    }
    Ok(())
}
fn read_state(dir: &Path) -> Result<State> {
    checked(dir)?;
    let state: State = serde_json::from_slice(&fs::read(dir.join("install-state.json"))?)?;
    require(state.schema == 1, "Unknown install state")?;
    Ok(state)
}
fn target_allowed(target: &Path) -> Result<()> {
    checked(target)?;
    let name = target
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or("Missing mod filename")?
        .to_ascii_lowercase();
    require(
        name.ends_with(".vpk") && name != "pak01_dir.vpk",
        "Select a mod VPK; the original game archive is never replaced",
    )?;
    let addons = target.parent().ok_or("No mod directory")?;
    require(
        addons.file_name().and_then(|x| x.to_str()) == Some("addons"),
        "Choose the installed mod from Deadlock/game/citadel/addons",
    )?;
    require(
        addons
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            == Some("citadel"),
        "Target is outside Deadlock's citadel addons directory",
    )?;
    Ok(())
}
fn game_closed() -> Result<()> {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist.exe")
            .args(["/FI", "IMAGENAME eq citadel.exe", "/FO", "CSV", "/NH"])
            .output()?;
        require(
            output.status.success(),
            "Could not check whether Deadlock is running",
        )?;
        require(
            !String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains("citadel.exe"),
            "Close Deadlock before changing the playback test",
        )?;
    }
    Ok(())
}
pub fn install(target: &Path, variant: &Path, dir: &Path) -> Result<serde_json::Value> {
    game_closed()?;
    target_allowed(target)?;
    checked(variant)?;
    checked(dir)?;
    let target = target.canonicalize()?;
    let candidate = fs::read(variant)?;
    let report: serde_json::Value = serde_json::from_slice(&fs::read(
        variant
            .parent()
            .ok_or("No test directory")?
            .join("build-report.json"),
    )?)?;
    let label = variant
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid variant")?;
    require(
        ["animated", "fixed"].contains(&label),
        "Unknown test comparison",
    )?;
    require(
        report["variants"][label]["sha256"].as_str() == Some(&hash(&candidate)),
        "Test package differs from its verified build",
    )?;
    let expected_base = report["base_mod_sha256"]
        .as_str()
        .ok_or("Test report lacks the original mod identity")?;
    let archive = deadlock_vpk::Vpk::open(variant)?;
    archive.verify_files(None, &mut || Ok(()))?;
    archive.verify_v2_hashes(&mut || Ok(()))?;
    let model = archive.read(crate::animation::MODEL)?;
    let model_data = data(&model, "DATA")?;
    let skeletons = arr(&model_data.root, "m_vecNmSkeletonRefs")?;
    if matches!(
        report["test"].as_str(),
        Some(
            "one-streamer-native-playback-primary-r2"
                | "one-streamer-native-playback-primary-r3"
                | "one-streamer-native-playback-primary-r4"
        )
    ) {
        require(
            skeletons.len() == 1,
            "Primary test has unexpected skeleton references",
        )?;
        let raw = archive.read(&format!("{}_c", skeletons[0].string()?))?;
        let skel = data(&raw, "DATA")?;
        let bones = arr(&skel.root, "m_boneIDs")?;
        let original = report["original_primary_bones"]
            .as_u64()
            .ok_or("Missing primary test bone count")? as usize;
        require(
            bones.len() == original + 7
                && bones[original..]
                    .iter()
                    .zip(crate::animation::names())
                    .all(|(a, b)| a.string().ok() == Some(b.as_str())),
            "Primary test lacks the left-streamer chain",
        )?;
        let model_skel =
            deadlock_skeleton::Skeleton::from_value(field(&model_data.root, "m_modelSkeleton")?)?;
        if matches!(
            report["test"].as_str(),
            Some(
                "one-streamer-native-playback-primary-r3"
                    | "one-streamer-native-playback-primary-r4"
            )
        ) {
            crate::disconnect::verify_detached(&model)?;
        }
        let selected = crate::animation::selected(&model_skel)?;
        require(
            selected
                .iter()
                .all(|&j| model_skel.flags[j] & 0x400008 == 0),
            "Test streamer physics is still enabled",
        )?;
    } else {
        require(
            report["test"].as_str() == Some("one-streamer-native-playback-r1")
                && skeletons
                    .iter()
                    .any(|v| v.string().ok() == Some(crate::animation::SECONDARY)),
            "Selected file is not a verified streamer test",
        )?;
    }
    let old = fs::read(&target)?;
    let prior = hash(&old);
    let new_hash = hash(&candidate);
    fs::create_dir_all(dir)?;
    let state_path = dir.join("install-state.json");
    let mut state = if state_path.exists() {
        let state = read_state(dir)?;
        require(
            state.original_sha256 == expected_base,
            "This package was built from a different original mod",
        )?;
        require(
            state.target == target,
            "This test session belongs to another installed mod",
        )?;
        require(
            prior == state.original_sha256 || state.known_test_hashes.contains(&prior),
            "Your installed VPK changed after this test. Nothing was replaced",
        )?;
        require(
            hash(&fs::read(&state.backup)?) == state.original_sha256,
            "Original backup changed; nothing replaced",
        )?;
        state
    } else {
        require(
            prior == expected_base,
            "Your mod changed since the test was built; nothing installed",
        )?;
        crate::probe(&target)?;
        let backup = dir.join("original-mod.vpk");
        require(!backup.exists(), "An untracked backup already exists")?;
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&backup)?;
        f.write_all(&old)?;
        f.sync_all()?;
        State {
            schema: 1,
            target: target.clone(),
            backup: backup.canonicalize()?,
            original_sha256: prior.clone(),
            known_test_hashes: vec![],
        }
    };
    if !state.known_test_hashes.contains(&new_hash) {
        state.known_test_hashes.push(new_hash.clone());
    }
    // Record the recovery hashes before replacing anything. A power loss leaves
    // either an original or known candidate, both restorable by this state.
    atomic(&state_path, &serde_json::to_vec_pretty(&state)?)?;
    require(
        hash(&fs::read(&target)?) == prior,
        "Mod changed during installation; stopped",
    )?;
    atomic(&target, &candidate)?;
    require(
        hash(&fs::read(&target)?) == new_hash,
        "Installed test hash mismatch",
    )?;
    Ok(
        serde_json::json!({"installed":true,"target":target,"sha256":new_hash,"original_backup":state.backup,"restore_available":true}),
    )
}
pub fn restore(dir: &Path) -> Result<serde_json::Value> {
    game_closed()?;
    let state = read_state(dir)?;
    target_allowed(&state.target)?;
    checked(&state.backup)?;
    let old = fs::read(&state.target)?;
    let current = hash(&old);
    require(current==state.original_sha256 || state.known_test_hashes.contains(&current),"Installed VPK was changed by something else. Restore stopped to protect it; your original backup remains in the test folder")?;
    let backup = fs::read(&state.backup)?;
    require(
        hash(&backup) == state.original_sha256,
        "Original backup hash mismatch",
    )?;
    if current != state.original_sha256 {
        atomic(&state.target, &backup)?;
    }
    require(
        hash(&fs::read(&state.target)?) == state.original_sha256,
        "Restored file hash mismatch",
    )?;
    Ok(serde_json::json!({"restored":true,"target":state.target,"sha256":state.original_sha256}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_restores_exact_original_and_refuses_external_changes() {
        let dir = tempfile::tempdir().unwrap();
        let addons = dir.path().join("game/citadel/addons");
        fs::create_dir_all(&addons).unwrap();
        let target = addons.join("pak04_dir.vpk");
        let session = dir.path().join("session");
        fs::create_dir(&session).unwrap();
        let backup = session.join("original-mod.vpk");
        fs::write(&backup, b"original").unwrap();
        fs::write(&target, b"test").unwrap();
        let state = State {
            schema: 1,
            target: target.clone(),
            backup,
            original_sha256: hash(b"original"),
            known_test_hashes: vec![hash(b"test")],
        };
        atomic(
            &session.join("install-state.json"),
            &serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        restore(&session).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        restore(&session).unwrap();
        fs::write(&target, b"user's subsequent mod").unwrap();
        assert!(restore(&session).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"user's subsequent mod");
    }
    #[test]
    fn original_game_archive_is_never_an_install_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pak01_dir.vpk");
        assert!(target_allowed(&path).is_err());
    }
}
