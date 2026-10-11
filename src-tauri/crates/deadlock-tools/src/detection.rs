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
    for name in ["CSDK12", "REDUCED_CSDK12"] {
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
    ]);
    for root in &roots {
        csdk.extend([
            root.clone(),
            root.join("Reduced_CSDK_12"),
            root.join("Reduced CSDK12"),
            root.join("CSDK12"),
        ]);
    }
    for candidate in csdk {
        cancel.check()?;
        for suffix in [
            "game/bin/win64/resourcecompiler.exe",
            "game/bin_tools/win64/resourcecompiler.exe",
        ] {
            let rc = candidate.join(suffix);
            if rc.is_file() {
                if result.csdk.is_empty() {
                    result.csdk = candidate.to_string_lossy().into_owned();
                }
                if result.resourcecompiler.is_empty() && candidate == Path::new(&result.csdk) {
                    result.resourcecompiler = rc.to_string_lossy().into_owned();
                }
                let vpk = rc.with_file_name("vpk.exe");
                if vpk.is_file() && result.vpk_tool.is_empty() {
                    result.vpk_tool = vpk.to_string_lossy().into_owned();
                }
            }
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
    if result.cswin_resourcecompiler.is_empty() {
        let preferred = if !result.cswin.is_empty() && result.model_backend != "csdk" {
            Some(safety::resolve_path(Path::new(&result.cswin))?)
        } else {
            None
        };
        if let Some(candidate) = available.into_iter().find(|c| {
            (result.model_backend == "auto" || c.backend == result.model_backend)
                && preferred.as_ref().is_none_or(|p| *p == c.root)
        }) {
            result.model_backend = candidate.backend.clone();
            if candidate.backend == "cswin" {
                result.cswin = candidate.root.to_string_lossy().into_owned();
            }
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
                    .map(|r| r.join(exe))
                    .chain([cwd.join("tools").join(exe)])
                    .find(|p| p.is_file())
            });
            if let Some(found) = found {
                *field = found.to_string_lossy().into_owned();
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
    Ok(result)
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
        Ok(tool) => format!("model files found ({}); see project test", tool.backend),
        Err(_) => "model setup needed (full CS2 optional)".into(),
    }
}
