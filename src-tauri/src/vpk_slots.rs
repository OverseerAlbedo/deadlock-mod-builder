//! Filename-only addon slot discovery. Scanning never creates folders or opens VPKs.
use crate::app::{AppError, Application, Result};
use deadlock_core::CancellationToken;
use deadlock_project::Project;
use serde::Serialize;
use std::{collections::BTreeSet, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct VpkSlots {
    pub configured_slot: u32,
    /// Another VPK of that number is in the game's addons folder now.
    pub configured_slot_occupied: bool,
    pub available_slot: Option<u32>,
    /// Slots used in the game's addons folder (scanned at every call).
    pub occupied_slots: Vec<u32>,
    /// Slots of this project's own earlier builds in its output folder; they
    /// are this project's files, not another mod's, so they do not count as
    /// occupied (Build asks before replacing an output).
    pub previous_outputs: Vec<u32>,
    pub scanned_folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VpkSlotChoice {
    pub slot: u32,
    pub filename: String,
    pub occupied_slots: Vec<u32>,
    pub previous_outputs: Vec<u32>,
    pub scanned_folders: Vec<String>,
}

// Preserve the legacy anchored filename grammar: pakN[N], optionally _dir or
// exactly three chunk digits, followed by .vpk (ASCII case insensitive).
fn filename_slot(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_prefix("pak")?.strip_suffix(".vpk")?;
    let digits = if let Some((digits, suffix)) = stem.split_once('_') {
        if suffix != "dir" && !(suffix.len() == 3 && suffix.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        digits
    } else {
        stem
    };
    if !(1..=2).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Slot numbers of the VPK files in one folder (read fresh every time).
fn slots_in(folder: &PathBuf, cancel: &CancellationToken) -> Result<BTreeSet<u32>> {
    let mut slots = BTreeSet::new();
    let text = folder.to_str().ok_or_else(|| {
        AppError::validation(
            "Could not check VPK slots",
            "The folder path cannot be represented losslessly.",
        )
    })?;
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(slots),
        Err(error) => {
            return Err(AppError::io(
                &format!("Could not check VPK slots in {text}"),
                error,
            ))
        }
    };
    for entry in entries {
        cancel.check()?;
        let entry = entry.map_err(|error| {
            AppError::io(&format!("Could not check VPK slots in {text}"), error)
        })?;
        // Non-Unicode names cannot match this ASCII-only filename grammar.
        if let Some(slot) = entry.file_name().to_str().and_then(filename_slot) {
            slots.insert(slot);
        }
    }
    Ok(slots)
}

/// `folders` occupy slots (the game's addons); `output` is this project's own
/// output folder, reported separately.
fn scan(folders: Vec<PathBuf>, output: Option<PathBuf>, preferred: u32, cancel: &CancellationToken) -> Result<VpkSlots> {
    let mut occupied = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut scanned = Vec::new();
    let mut previous = BTreeSet::new();
    if let Some(folder) = output {
        cancel.check()?;
        previous = slots_in(&folder, cancel)?;
        if let Some(text) = folder.to_str() {
            scanned.push(text.to_owned());
        }
        seen.insert(folder);
    }
    for folder in folders {
        cancel.check()?;
        if !seen.insert(folder.clone()) {
            continue;
        }
        let text = folder.to_str().ok_or_else(|| {
            AppError::validation(
                "Could not check VPK slots",
                "The folder path cannot be represented losslessly.",
            )
        })?;
        scanned.push(text.to_owned());
        occupied.extend(slots_in(&folder, cancel)?);
    }
    // pak01 remains reserved by the native app; this helper never chooses it.
    let available_slot = if (2..=99).contains(&preferred) && !occupied.contains(&preferred) {
        Some(preferred)
    } else {
        (2..=99).find(|slot| !occupied.contains(slot))
    };
    Ok(VpkSlots {
        configured_slot: preferred,
        configured_slot_occupied: occupied.contains(&preferred),
        available_slot,
        occupied_slots: occupied.into_iter().collect(),
        previous_outputs: previous.into_iter().collect(),
        scanned_folders: scanned,
    })
}

impl Application {
    fn scan_vpk_slots(&self, project: &Project, cancel: &CancellationToken) -> Result<VpkSlots> {
        project
            .validate_draft()
            .map_err(|e| AppError::validation("Project choices are invalid", e))?;
        let settings = self.settings()?;
        if settings.deadlock.is_empty() {
            return Err(AppError::validation(
                "Select Deadlock in Setup to find a free VPK slot",
                "No Deadlock folder is configured.",
            ));
        }
        let folders = vec![PathBuf::from(&settings.deadlock).join("game/citadel/addons")];
        let output = (!settings.workspace.is_empty()).then(|| {
            PathBuf::from(&settings.workspace)
                .join(&project.id)
                .join("output")
        });
        scan(folders, output, project.vpk_number, cancel)
    }

    /// Snapshot availability only. Existing build/install overwrite gates remain
    /// authoritative if another process creates a file after this scan.
    pub fn get_vpk_slots(&self, project: Project) -> Result<VpkSlots> {
        let (slots, _) = self.operation("Find free VPK slot", |clock, cancel| {
            self.step(clock, "Scan addon and project output filenames", || {
                self.scan_vpk_slots(&project, cancel)
            })
        })?;
        Ok(slots)
    }

    /// Explicit selection only: builds and rebuilds retain their saved number.
    pub fn choose_free_vpk_slot(&self, project: Project) -> Result<VpkSlotChoice> {
        let (choice, _) = self.operation("Find free VPK slot", |clock, cancel| {
            let slots = self.step(clock, "Scan addon and project output filenames", || self.scan_vpk_slots(&project, cancel))?;
            self.step(clock, "Choose free addon VPK slot", || {
                cancel.check()?;
                let slot = slots.available_slot.ok_or_else(|| AppError::validation(
                    "All addon VPK slots 02–99 are occupied",
                    "Free a slot, or choose an existing number manually and confirm replacement when building. Slot 01 is reserved."))?;
                Ok(VpkSlotChoice { slot, filename: format!("pak{slot:02}_dir.vpk"),
                    occupied_slots: slots.occupied_slots, previous_outputs: slots.previous_outputs, scanned_folders: slots.scanned_folders })
            })
        })?;
        Ok(choice)
    }
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    use std::sync::Arc;
    type State<'a> = tauri::State<'a, Arc<Application>>;
    #[tauri::command]
    pub async fn get_vpk_slots(state: State<'_>, project: Project) -> Result<VpkSlots> {
        let app = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.get_vpk_slots(project))
            .await
            .map_err(|e| AppError::new("internal", "The VPK slot worker stopped", e))?
    }
    #[tauri::command]
    pub async fn choose_free_vpk_slot(state: State<'_>, project: Project) -> Result<VpkSlotChoice> {
        let app = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.choose_free_vpk_slot(project))
            .await
            .map_err(|e| AppError::new("internal", "The VPK slot worker stopped", e))?
    }
}
