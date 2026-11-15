//! Finite local discovery. Existing explicit compiler choices are preserved.
use deadlock_core::{safety, CancellationToken, CoreError, Result};
use deadlock_project::Settings;
use regex::Regex;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
pub fn steam_libraries(steam: &Path) -> Result<Vec<PathBuf>> {
    let mut found = vec![steam.to_path_buf()];
    let file = steam.join("steamapps/libraryfolders.vdf");
    if file.is_file() {
        let raw = fs::read(file).map_err(|e| CoreError::io("Read Steam libraries", e))?;
        let text = String::from_utf8_lossy(&raw);
        for pattern in [
            r#""path"\s*"((?:\\.|[^"\\])*)""#,
            r#""\d+"\s*"([A-Za-z]:[^"\n]*)""#,
        ] {
            for capture in Regex::new(pattern)
                .expect("constant regex")
                .captures_iter(&text)
            {
                found.push(PathBuf::from(capture[1].replace("\\\\", "\\")));
            }
        }
    }
    let mut seen = HashSet::new();
    found.retain(|p| seen.insert(p.clone()));
    Ok(found)
}
fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
}
fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|value| {
        std::env::split_paths(&value)
            .map(|p| p.join(name))
            .find(|p| p.is_file())
    })
}
pub fn detect(
    settings: &Settings,
    extra_roots: &[PathBuf],
    cancel: &CancellationToken,
) -> Result<Settings> {
    let mut result = settings.clone();
    let mut roots = extra_roots.to_vec();
    let mut steam = if result.steam.is_empty() {
        vec![]
    } else {
        vec![PathBuf::from(&result.steam)]
    };
    #[cfg(windows)]
    {
        use winreg::{
            enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
            RegKey,
        };
        for (hive, key, value) in [
            (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
            (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\WOW6432Node\Valve\Steam",
                "InstallPath",
            ),
        ] {
            if let Ok(key) = RegKey::predef(hive).open_subkey(key) {
                if let Ok(value) = key.get_value::<String, _>(value) {
                    steam.push(PathBuf::from(value));
                }
            }
        }
    }
    for name in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(value) = std::env::var_os(name) {
            steam.push(PathBuf::from(value).join("Steam"));
        }
    }
    steam.extend(roots.iter().cloned());
    let install = Regex::new(r#""installdir"\s*"([^"\\/]+)""#).expect("constant regex");
    for location in &steam {
        cancel.check()?;
        if !location.join("steamapps").is_dir() {
            continue;
        }
        if result.steam.is_empty() {
            result.steam = location.to_string_lossy().into_owned();
        }
        for library in steam_libraries(location)? {
            let app = library.join("steamapps/appmanifest_1422450.acf");
            let name = if app.is_file() {
                let raw =
                    fs::read(app).map_err(|e| CoreError::io("Read Deadlock app manifest", e))?;
                let text = String::from_utf8_lossy(&raw);
                install
                    .captures(&text)
                    .map(|c| c[1].to_owned())
                    .unwrap_or_else(|| "Deadlock".into())
            } else {
                "Deadlock".into()
            };
            let game = library.join("steamapps/common").join(name);
            let pak = game.join("game/citadel/pak01_dir.vpk");
            if pak.is_file() && result.pak.is_empty() {
                result.deadlock = game.to_string_lossy().into_owned();
                result.pak = pak.to_string_lossy().into_owned();
            }
        }
    }
    if !result.pak.is_empty() && result.deadlock.is_empty() {
        let pak = Path::new(&result.pak);
        if let Some(parent) = pak.parent() {
            if parent
                .file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("citadel"))
            {
                if let Some(game) = parent.parent() {
                    if game
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("game"))
                    {
                        if let Some(root) = game.parent() {
                            result.deadlock = root.to_string_lossy().into_owned();
                        }
                    }
                }
            }
        }
    }
    let mut csdk = if result.csdk.is_empty() {
        vec![]
    } else {
        vec![PathBuf::from(&result.csdk)]
    };
    for name in ["CSDK13", "REDUCED_CSDK13", "CSDK12", "REDUCED_CSDK12"] {
        if let Some(value) = std::env::var_os(name) {
            csdk.push(PathBuf::from(value));
        }
    }
    let cwd = std::env::current_dir().map_err(|e| CoreError::io("Read working directory", e))?;
    roots.extend([
        cwd.clone(),
        home().join("Downloads"),
        PathBuf::from("C:/"),
        PathBuf::from("D:/"),
        PathBuf::from("C:/Tools"),
        PathBuf::from("D:/Tools"),
        PathBuf::from("C:/DeadlockModdingTools"),
        PathBuf::from("D:/DeadlockModdingTools"),
        home().join("Desktop"),
        home().join("OneDrive/Desktop"),
        home().join("Documents"),
    ]);
    // Tools are often unpacked one level down (Desktop\DeadlockModdingTools\...).
    let nested: Vec<PathBuf> = roots
        .iter()
        .filter(|r| r.is_dir())
        .flat_map(|r| fs::read_dir(r).into_iter().flatten().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).take(48).collect::<Vec<_>>())
        .collect();
    roots.extend(nested);
    for root in &roots {
        csdk.extend([
            root.clone(),
            root.join("Reduced_CSDK_13"),
            root.join("Reduced CSDK13"),
            root.join("CSDK13"),
            root.join("Reduced_CSDK_13/Reduced_CSDK_13"),
            root.join("Reduced_CSDK_12"),
            root.join("Reduced CSDK12"),
            root.join("CSDK12"),
            root.join("Reduced_CSDK_12/Reduced_CSDK_12"),
        ]);
    }
    // Only complete SDK folders (game\citadel\gameinfo.gi and the layout's
    // compiler), so a model toolset or a Deadlock installation is never taken
    // for one. A Community SDK already chosen is kept; otherwise the current
    // layout (Reduced CSDK 13) wins over Reduced CSDK 12.
    let mut sdks: Vec<(PathBuf, crate::setup_checks::SdkLayout)> = vec![];
    let mut seen_sdk = HashSet::new();
    for candidate in csdk {
        cancel.check()?;
        if let Some(layout) = crate::setup_checks::sdk_layout(&candidate) {
            let key = candidate.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase();
            if seen_sdk.insert(key) {
                sdks.push((candidate, layout));
            }
        }
    }
    if result.csdk.is_empty() {
        let preferred = sdks
            .iter()
            .find(|(_, layout)| layout.compiles_models())
            .or_else(|| sdks.first());
        if let Some((root, _)) = preferred {
            result.csdk = root.to_string_lossy().into_owned();
        }
    }
    if result.resourcecompiler.is_empty() && !result.csdk.is_empty() {
        let root = PathBuf::from(&result.csdk);
        if let Some(layout) = crate::setup_checks::sdk_layout(&root) {
            let rc = root.join(layout.compiler());
            if rc.is_file() {
                result.resourcecompiler = rc.to_string_lossy().into_owned();
            }
        }
    }
    for (root, layout) in &sdks {
        let vpk = root.join(layout.compiler()).with_file_name("vpk.exe");
        if vpk.is_file() && result.vpk_tool.is_empty() {
            result.vpk_tool = vpk.to_string_lossy().into_owned();
        }
    }
    let mut model_roots = if result.cswin.is_empty() {
        vec![]
    } else {
        vec![PathBuf::from(&result.cswin)]
    };
    if let Some(value) = std::env::var_os("CSWIN64") {
        model_roots.push(PathBuf::from(value));
    }
    for root in &roots {
        model_roots.extend([
            root.clone(),
            root.join("CSWin64"),
            root.join("Tools/CSWin64"),
            root.join("modding/CSWin64"),
        ]);
    }
    for location in &steam {
        cancel.check()?;
        for library in steam_libraries(location)? {
            let full = library.join("steamapps/common/Counter-Strike Global Offensive");
            if full.is_dir() && result.optional_cs2.is_empty() {
                result.optional_cs2 = full.to_string_lossy().into_owned();
            }
        }
    }
    let available = crate::toolchain::candidates(
        &result,
        &model_roots
            .into_iter()
            .filter(|r| r.join("game/csgo").is_dir())
            .collect::<Vec<_>>(),
    )?;
    // With the backend on auto, a Reduced CSDK 13 compiles the models:
    // Detect fills no toolset fields then, so no Counter-Strike tools folder
    // is put next to it.
    let sdk_models = result.model_backend == "auto" && crate::toolchain::sdk_compiles_models(&result);
    if result.cswin_resourcecompiler.is_empty() && !sdk_models {
        let preferred = if !result.cswin.is_empty() && result.model_backend != "csdk" {
            Some(safety::resolve_path(Path::new(&result.cswin))?)
        } else {
            None
        };
        // Only a downloaded model toolset fills these fields. Reduced CSDK 13
        // needs none (auto compiles with it first), and Reduced CSDK 12
        // cannot compile hero models. The backend is never written: auto
        // stays auto, so a Reduced CSDK 13 chosen later takes over.
        if let Some(candidate) = available.into_iter().find(|c| {
            c.backend == "cswin"
                && (result.model_backend == "auto" || c.backend == result.model_backend)
                && preferred.as_ref().is_none_or(|p| *p == c.root)
        }) {
            result.cswin = candidate.root.to_string_lossy().into_owned();
            result.cswin_resourcecompiler = candidate.compiler.to_string_lossy().into_owned();
            if result.model_game.is_empty() {
                result.model_game = candidate.game.to_string_lossy().into_owned();
            }
        }
    }
    for (field, exe) in [
        (&mut result.source2viewer, "Source2Viewer-CLI.exe"),
        (&mut result.vpk_tool, "vpk.exe"),
    ] {
        if field.is_empty() {
            let found = find_on_path(exe).or_else(|| {
                roots
                    .iter()
                    .flat_map(|r| [r.join(exe), r.join("cli-windows-x64").join(exe)])
                    .chain([cwd.join("tools").join(exe)])
                    .find(|p| p.is_file())
            });
            if let Some(found) = found {
                *field = found.to_string_lossy().into_owned();
            }
        }
    }
    if result.blender.is_empty() {
        for base in ["ProgramFiles", "ProgramFiles(x86)"] {
            let Some(base) = std::env::var_os(base) else { continue };
            let folder = PathBuf::from(base).join("Blender Foundation");
            let found: Vec<PathBuf> = fs::read_dir(&folder).into_iter().flatten().filter_map(|e| e.ok()).map(|e| e.path().join("blender.exe")).filter(|p| p.is_file()).collect();
            // By version number, highest supported first (not by folder name order).
            if let Some(newest) = deadlock_core::blender_version::choose_install(found) {
                result.blender = newest.to_string_lossy().into_owned();
                break;
            }
        }
    }
    if result.workspace.is_empty() {
        result.workspace = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home)
            .join("DeadlockModBuilder/workspaces")
            .to_string_lossy()
            .into_owned();
    }
    // Correct what was found or typed: nested SDK folders, a folder instead of
    // the program, the acquisition folder instead of its toolset.
    Ok(crate::setup_checks::apply_fixes(&result))
}
pub fn status(settings: &Settings) -> BTreeMap<String, bool> {
    [
        (
            "Deadlock",
            !settings.deadlock.is_empty() && Path::new(&settings.deadlock).is_dir(),
        ),
        (
            "Original VPK",
            !settings.pak.is_empty() && Path::new(&settings.pak).is_file(),
        ),
        (
            "CSDK",
            !settings.csdk.is_empty() && Path::new(&settings.csdk).is_dir(),
        ),
        (
            "Resource compiler",
            !settings.resourcecompiler.is_empty()
                && Path::new(&settings.resourcecompiler).is_file(),
        ),
        (
            "Source 2 Viewer CLI",
            !settings.source2viewer.is_empty() && Path::new(&settings.source2viewer).is_file(),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect()
}
pub fn model_status(settings: &Settings) -> String {
    match crate::toolchain::select(settings, true) {
        Ok(tool) if tool.backend == "csdk" && crate::toolchain::sdk_compiles_models(settings) =>"Models are compiled with the Community SDK (Reduced CSDK 13); no separate model toolset is needed. Test my setup checks it.".into(),
        Ok(tool) => format!("Model compiler found ({}). Test it once with your hero in Set up model compiler… (Settings → Advanced configuration → Downloaded model toolset).", tool.backend),
        Err(error) => error.to_string().trim_start_matches("Integrity check failed: ").to_owned(),
    }
}
