//! Field-by-field checks of the tool paths in Settings, written for people
//! setting the app up for the first time. Each check says what is wrong in
//! plain words and, when the right file or folder can be found nearby (the
//! folder instead of the program, a folder extracted inside another folder,
//! the viewer app instead of its command-line tool), suggests it as a fix.
//! Checks only look at files; they never run a tool.
use deadlock_project::Settings;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// The path points at the right kind of file or folder.
    Ok,
    /// Works, but something should be changed (a long path, an untested copy).
    Warning,
    /// The app cannot use this path; the build will fail until it is fixed.
    Problem,
    /// An optional field that is empty.
    Optional,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FieldCheck {
    /// The Settings key (`csdk`, `resourcecompiler`, ...).
    pub field: String,
    pub state: State,
    pub message: String,
    /// A corrected value the user can apply with one click.
    pub fix: Option<String>,
}

/// Material compiler inside Reduced CSDK 12. The SDK has four copies (bin,
/// bin_cs2, bin_server, bin_tools) with different particles.dll builds;
/// bin_cs2 is the tested one.
pub const CSDK_COMPILER: &str = "game/bin_cs2/win64/resourcecompiler.exe";
pub const CSDK_OTHER_COMPILERS: [&str; 3] = [
    "game/bin/win64/resourcecompiler.exe",
    "game/bin_tools/win64/resourcecompiler.exe",
    "game/bin_server/win64/resourcecompiler.exe",
];
const CSDK_CONFIG: &str = "game/citadel/gameinfo.gi";
const DEADLOCK_PAK: &str = "game/citadel/pak01_dir.vpk";
const TOOLSET_COMPILER: &str = "game/bin/win64/resourcecompiler.exe";
const TOOLSET_GAME: &str = "game/csgo";
/// Folder paths longer than this risk Windows' 260-character limit for the
/// compiler's generated texture names (see deadlock-materials native_paths).
pub const LONG_CSDK_PATH: usize = 60;

pub const EXAMPLE_DEADLOCK: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Deadlock";
pub const EXAMPLE_PAK: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\Deadlock\game\citadel\pak01_dir.vpk";
pub const EXAMPLE_CSDK: &str = r"C:\CSDK12";
pub const EXAMPLE_COMPILER: &str = r"C:\CSDK12\game\bin_cs2\win64\resourcecompiler.exe";
pub const EXAMPLE_S2V: &str = r"C:\Tools\Source2Viewer\Source2Viewer-CLI.exe";
pub const EXAMPLE_TOOLSET: &str = r"C:\DeadlockModdingTools\DMBTools-1\toolset";
pub const EXAMPLE_DEPOTDOWNLOADER: &str = r"C:\Tools\DepotDownloader\DepotDownloader.exe";

fn text(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}
fn file_named(path: &Path, name: &str) -> bool {
    path.is_file()
        && path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
}
fn is_zip(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("zip"))
}
fn same(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| text(p).trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}
/// `path` and up to `levels` of its parents, nearest first.
fn ancestors(path: &Path, levels: usize) -> Vec<PathBuf> {
    path.ancestors().take(levels + 1).map(Path::to_path_buf).collect()
}
/// Child folders up to `depth` levels below `path` (sorted, at most 64 per level).
fn descendants(path: &Path, depth: usize) -> Vec<PathBuf> {
    let mut found = vec![];
    let mut level = vec![path.to_path_buf()];
    for _ in 0..depth {
        let mut next = vec![];
        for folder in &level {
            let Ok(entries) = std::fs::read_dir(folder) else { continue };
            let mut children: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            children.sort();
            children.truncate(64);
            next.extend(children);
        }
        found.extend(next.iter().cloned());
        level = next;
    }
    found
}
/// The folder near `path` (itself, a parent, or a folder inside it) that
/// contains `marker`.
fn root_with(path: &Path, marker: &str, up: usize, down: usize) -> Option<PathBuf> {
    ancestors(path, up)
        .into_iter()
        .chain(descendants(path, down))
        .find(|p| p.join(marker).is_file())
}
/// A file called `name` at `path`, inside it, or next to it.
fn file_near(path: &Path, name: &str, down: usize) -> Option<PathBuf> {
    if file_named(path, name) {
        return Some(path.to_path_buf());
    }
    let start = if path.is_dir() { path.to_path_buf() } else { path.parent()?.to_path_buf() };
    std::iter::once(start.clone())
        .chain(descendants(&start, down))
        .map(|p| p.join(name))
        .find(|p| p.is_file())
}

fn check(field: &str, state: State, message: impl Into<String>, fix: Option<PathBuf>) -> FieldCheck {
    FieldCheck { field: field.into(), state, message: message.into(), fix: fix.map(|p| text(&p)) }
}
fn ok(field: &str, message: impl Into<String>) -> FieldCheck {
    check(field, State::Ok, message, None)
}

fn workspace(settings: &Settings) -> FieldCheck {
    let value = settings.workspace.trim();
    if value.is_empty() {
        return check("workspace", State::Problem, r"Choose a folder where the app can keep project files, for example C:\DeadlockModBuilder\workspaces. Click Detect tools to use the default folder.", None);
    }
    let path = Path::new(value);
    if path.is_file() {
        return check("workspace", State::Problem, "This is a file. Choose a folder instead.", path.parent().map(Path::to_path_buf));
    }
    if text(path).to_lowercase().contains("onedrive") {
        return check("workspace", State::Warning, "This folder is inside OneDrive. Syncing can lock files during a build; a folder outside OneDrive is safer.", None);
    }
    ok("workspace", if path.is_dir() { "Folder found." } else { "The folder will be created on the first build." })
}

fn deadlock(settings: &Settings) -> FieldCheck {
    let value = settings.deadlock.trim();
    if value.is_empty() {
        return check("deadlock", State::Problem, format!("Choose the Deadlock game folder (the one that contains the game folder), for example {EXAMPLE_DEADLOCK}. Click Detect tools to find it through Steam."), None);
    }
    let path = Path::new(value);
    if path.join(DEADLOCK_PAK).is_file() {
        return ok("deadlock", "Deadlock installation found.");
    }
    match root_with(path, DEADLOCK_PAK, 4, 2) {
        Some(root) => check("deadlock", State::Problem, format!("This is not the main Deadlock folder. The right folder is {}.", text(&root)), Some(root)),
        None if path.exists() => check("deadlock", State::Problem, format!(r"No game\citadel\pak01_dir.vpk in this folder. Choose the Deadlock folder from Steam, for example {EXAMPLE_DEADLOCK}."), None),
        None => check("deadlock", State::Problem, "This folder does not exist. Click Detect tools, or in Steam right-click Deadlock → Manage → Browse local files and copy that folder.", None),
    }
}

fn pak(settings: &Settings) -> FieldCheck {
    let value = settings.pak.trim();
    let from_game = || Some(Path::new(settings.deadlock.trim()).join(DEADLOCK_PAK)).filter(|p| !settings.deadlock.trim().is_empty() && p.is_file());
    if value.is_empty() {
        return check("pak", State::Problem, format!("Choose pak01_dir.vpk from the game, for example {EXAMPLE_PAK}."), from_game());
    }
    let path = Path::new(value);
    if file_named(path, "pak01_dir.vpk") {
        return ok("pak", "Game archive found.");
    }
    if path.is_dir() {
        let found = root_with(path, DEADLOCK_PAK, 3, 1).map(|r| r.join(DEADLOCK_PAK))
            .or_else(|| Some(path.join("pak01_dir.vpk")).filter(|p| p.is_file()));
        return check("pak", State::Problem, "This is a folder. Choose the file pak01_dir.vpk itself.", found.or_else(from_game));
    }
    if path.is_file() {
        let sibling = path.with_file_name("pak01_dir.vpk");
        return check("pak", State::Problem, "This is one of the numbered archive parts. Choose pak01_dir.vpk, the index file next to it.", Some(sibling).filter(|p| p.is_file()).or_else(from_game));
    }
    check("pak", State::Problem, format!("The file does not exist. Choose pak01_dir.vpk, for example {EXAMPLE_PAK}."), from_game())
}

/// Reduced CSDK 12 root: contains game\citadel\gameinfo.gi and the bin_cs2 compiler.
pub fn csdk_root(path: &Path) -> Option<PathBuf> {
    let complete = |p: &Path| p.join(CSDK_CONFIG).is_file() && p.join(CSDK_COMPILER).is_file();
    ancestors(path, 5).into_iter().chain(descendants(path, 2)).find(|p| complete(p))
}

fn csdk(settings: &Settings) -> FieldCheck {
    let value = settings.csdk.trim();
    if value.is_empty() {
        return check("csdk", State::Problem, format!("Choose the extracted Reduced_CSDK_12 folder (it contains the folders game and content), for example {EXAMPLE_CSDK}."), None);
    }
    let path = Path::new(value);
    if is_zip(path) {
        return check("csdk", State::Problem, "This is the downloaded ZIP. Extract it first (right-click → Extract All), then choose the extracted Reduced_CSDK_12 folder.", None);
    }
    let Some(root) = csdk_root(path) else {
        return check("csdk", State::Problem, if path.exists() {
            r"This is not a complete Reduced CSDK 12: game\citadel\gameinfo.gi or game\bin_cs2\win64\resourcecompiler.exe is missing. Extract the whole ZIP again into a new, empty folder such as C:\CSDK12."
        } else { r"This folder does not exist. Choose the extracted Reduced_CSDK_12 folder, for example C:\CSDK12." }, None);
    };
    if !same(&root, path) {
        let why = if root.starts_with(path) { "The SDK was extracted inside another folder of the same name." } else { "This is a folder inside the SDK." };
        return check("csdk", State::Problem, format!("{why} The SDK itself is {}.", text(&root)), Some(root));
    }
    if !settings.deadlock.trim().is_empty() && root.starts_with(Path::new(settings.deadlock.trim())) {
        return check("csdk", State::Problem, r"The SDK is inside the Deadlock game folder. Extract it to its own folder, for example C:\CSDK12.", None);
    }
    if text(&root).len() > LONG_CSDK_PATH {
        return check("csdk", State::Warning, format!(r"Found, but the folder path is long ({} characters). Long paths can make material compiles fail; move the folder to C:\CSDK12 if they do.", text(&root).len()), None);
    }
    if text(&root).to_lowercase().contains("onedrive") {
        return check("csdk", State::Warning, r"Found, but it is inside OneDrive. Syncing can lock compiler files; C:\CSDK12 is safer.", None);
    }
    ok("csdk", "Reduced CSDK 12 found.")
}

fn resourcecompiler(settings: &Settings) -> FieldCheck {
    let value = settings.resourcecompiler.trim();
    let root = csdk_root(Path::new(settings.csdk.trim())).filter(|_| !settings.csdk.trim().is_empty());
    let expected = root.as_ref().map(|r| r.join(CSDK_COMPILER));
    if value.is_empty() {
        return check("resourcecompiler", State::Problem, format!(r"Choose resourcecompiler.exe in the SDK's game\bin_cs2\win64 folder, for example {EXAMPLE_COMPILER}."), expected);
    }
    let path = Path::new(value);
    if path.is_dir() {
        let found = expected.clone().or_else(|| file_near(path, "resourcecompiler.exe", 3));
        return check("resourcecompiler", State::Problem, "This is a folder. Choose the file resourcecompiler.exe itself.", found);
    }
    if !file_named(path, "resourcecompiler.exe") {
        return check("resourcecompiler", State::Problem, if path.exists() { "This is not resourcecompiler.exe." } else { "This file does not exist." }, expected);
    }
    if let Some(expected) = &expected {
        if same(path, expected) {
            return ok("resourcecompiler", "Material compiler found (bin_cs2, the tested copy).");
        }
        let root = root.as_ref().expect("expected comes from root");
        if path.starts_with(root) {
            return check("resourcecompiler", State::Warning, r"This is one of the SDK's other compiler copies. Use the one in game\bin_cs2\win64, the copy the app is tested with.", Some(expected.clone()));
        }
        let toolset = !settings.cswin.trim().is_empty() && path.starts_with(Path::new(settings.cswin.trim()));
        return check("resourcecompiler", State::Problem, if toolset {
            "This is the model compiler from the model toolset. The material compiler must be the one inside the Community SDK; the model compiler goes under Advanced configuration."
        } else { "This compiler is not inside the Community SDK folder above. Use the SDK's own compiler." }, Some(expected.clone()));
    }
    check("resourcecompiler", State::Warning, "Found. Set the Community SDK folder too, so the app can confirm this compiler belongs to it.", None)
}

fn source2viewer(settings: &Settings) -> FieldCheck {
    let value = settings.source2viewer.trim();
    if value.is_empty() {
        return check("source2viewer", State::Problem, format!("Choose Source2Viewer-CLI.exe from the Windows x64 command-line download (cli-windows-x64.zip), for example {EXAMPLE_S2V}."), None);
    }
    let path = Path::new(value);
    if file_named(path, "Source2Viewer-CLI.exe") {
        return ok("source2viewer", "Source 2 Viewer command-line tool found.");
    }
    if is_zip(path) {
        return check("source2viewer", State::Problem, "This is the downloaded ZIP. Extract it first, then choose Source2Viewer-CLI.exe inside it.", None);
    }
    let near = file_near(path, "Source2Viewer-CLI.exe", 2)
        .or_else(|| path.parent().and_then(|p| file_near(p, "Source2Viewer-CLI.exe", 2)));
    if file_named(path, "Source2Viewer.exe") {
        return check("source2viewer", State::Problem, "This is the Source 2 Viewer app. The builder needs its command-line tool, Source2Viewer-CLI.exe, from the cli-windows-x64 download.", near);
    }
    check("source2viewer", State::Problem, if path.is_dir() { "This is a folder. Choose the file Source2Viewer-CLI.exe itself." } else if path.exists() { "This is not Source2Viewer-CLI.exe." } else { "This file does not exist." }, near)
}

/// A downloaded model toolset: `<root>\game\bin\win64\resourcecompiler.exe`
/// and `<root>\game\csgo`. An acquisition folder holds it in `toolset`.
pub fn toolset_root(path: &Path) -> Option<PathBuf> {
    let complete = |p: &Path| p.join(TOOLSET_COMPILER).is_file() && p.join(TOOLSET_GAME).join("gameinfo.gi").is_file();
    ancestors(path, 5).into_iter().chain(descendants(path, 1)).find(|p| complete(p))
}

/// The model compiler: the downloaded toolset. A model compile with the
/// Community SDK's compiler stops with a ParticleFloatType_t schema mismatch.
fn model_compiler(settings: &Settings) -> Vec<FieldCheck> {
    let root_text = settings.cswin.trim();
    let not_set = "The model compiler is not set up. Open Set up model compiler…, get the files, then click Use this toolset in settings. Without it, models are compiled with the Community SDK, which stops with a schema mismatch.";
    if settings.model_backend == "csdk" {
        return vec![check("cswin", State::Problem, "Model compiler backend is set to CSDK. The Community SDK cannot compile hero models (it stops with a schema mismatch). Set the backend to auto and set up the model toolset.", None)];
    }
    if root_text.is_empty() {
        return vec![check("cswin", State::Problem, not_set, None)];
    }
    let root_path = Path::new(root_text);
    if csdk_root(root_path).is_some_and(|r| same(&r, root_path)) {
        return vec![check("cswin", State::Problem, "This is the Community SDK, not the model toolset. Choose the toolset folder you downloaded with Set up model compiler… (it contains game\\csgo).", None)];
    }
    let Some(root) = toolset_root(root_path) else {
        return vec![check("cswin", State::Problem, format!(r"No model toolset here: game\bin\win64\resourcecompiler.exe or game\csgo\gameinfo.gi is missing. The downloaded toolset is the toolset folder inside your acquisition folder, for example {EXAMPLE_TOOLSET}."), None)];
    };
    if !same(&root, root_path) {
        return vec![check("cswin", State::Problem, format!("The toolset itself is {}.", text(&root)), Some(root))];
    }
    let mut out = vec![ok("cswin", "Model toolset found.")];
    let compiler = root.join(TOOLSET_COMPILER);
    let chosen = settings.cswin_resourcecompiler.trim();
    if chosen.is_empty() || !same(Path::new(chosen), &compiler) {
        out.push(check("cswin_resourcecompiler", State::Problem, if chosen.is_empty() { "Choose the toolset's own compiler." } else { "The model compiler must be the toolset's own compiler, not another copy." }, Some(compiler)));
    } else {
        out.push(ok("cswin_resourcecompiler", "Model compiler found."));
    }
    let game = root.join(TOOLSET_GAME);
    let chosen = settings.model_game.trim();
    if chosen.is_empty() || !same(Path::new(chosen), &game) {
        out.push(check("model_game", State::Problem, r"The model compiler game folder must be the toolset's game\csgo folder.", Some(game)));
    } else {
        out.push(ok("model_game", "Model game folder found."));
    }
    out
}

fn optional_file(field: &str, value: &str, name: &str, what: &str) -> FieldCheck {
    let value = value.trim();
    if value.is_empty() {
        return check(field, State::Optional, "Optional; empty.", None);
    }
    let path = Path::new(value);
    if file_named(path, name) {
        return ok(field, format!("{what} found."));
    }
    let near = file_near(path, name, 2);
    check(field, if near.is_some() { State::Problem } else { State::Warning },
        if path.is_dir() { format!("This is a folder. Choose {name} itself.") } else if path.exists() { format!("This is not {name}.") } else { "This file does not exist.".into() }, near)
}
fn optional_folder(field: &str, value: &str) -> FieldCheck {
    let value = value.trim();
    if value.is_empty() {
        check(field, State::Optional, "Optional; empty.", None)
    } else if Path::new(value).is_dir() {
        ok(field, "Folder found.")
    } else {
        check(field, State::Warning, "This folder does not exist.", None)
    }
}

/// Every path field of Settings, in the order the Settings page shows them.
pub fn field_checks(settings: &Settings) -> Vec<FieldCheck> {
    let mut out = vec![workspace(settings), deadlock(settings), pak(settings), source2viewer(settings), resourcecompiler(settings), csdk(settings)];
    out.push(optional_folder("steam", &settings.steam));
    out.extend(model_compiler(settings));
    out.push(optional_folder("optional_cs2", &settings.optional_cs2));
    out.push(optional_file("vpk_tool", &settings.vpk_tool, "vpk.exe", "VPK tool"));
    out.push(optional_file("blender", &settings.blender, "blender.exe", "Blender"));
    out
}

/// Apply the suggested fixes to `settings` (Detect tools and the Fix all button).
pub fn apply_fixes(settings: &Settings) -> Settings {
    let mut result = settings.clone();
    // An earlier detection may have put the Community SDK into the model
    // compiler fields; clear them so Set up model compiler… can fill them.
    let sdk = csdk_root(Path::new(result.csdk.trim())).filter(|_| !result.csdk.trim().is_empty());
    let in_sdk = |value: &str| sdk.as_ref().is_some_and(|r| !value.trim().is_empty() && Path::new(value.trim()).starts_with(r));
    if (result.cswin.trim().is_empty() || in_sdk(&result.cswin)) && (in_sdk(&result.cswin_resourcecompiler) || in_sdk(&result.model_game) || result.model_backend == "csdk") {
        result.cswin.clear();
        result.cswin_resourcecompiler.clear();
        result.model_game.clear();
        result.model_backend = "auto".into();
    }
    // A fix can make another one possible (the CSDK root, then its compiler).
    for _ in 0..3 {
        let checks = field_checks(&result);
        let mut changed = false;
        for check in checks {
            let Some(fix) = check.fix else { continue };
            let slot = match check.field.as_str() {
                "deadlock" => &mut result.deadlock,
                "pak" => &mut result.pak,
                "csdk" => &mut result.csdk,
                "resourcecompiler" => &mut result.resourcecompiler,
                "source2viewer" => &mut result.source2viewer,
                "cswin" => &mut result.cswin,
                "cswin_resourcecompiler" => &mut result.cswin_resourcecompiler,
                "model_game" => &mut result.model_game,
                "workspace" => &mut result.workspace,
                "vpk_tool" => &mut result.vpk_tool,
                "blender" => &mut result.blender,
                _ => continue,
            };
            if *slot != fix {
                *slot = fix;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    result
}

/// Free space a build should have on the workspace drive.
pub const BUILD_FREE_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// Whether Deadlock is running (it locks the installed addon VPKs).
pub fn game_running() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        return std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq deadlock.exe", "/NH", "/FO", "CSV"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("\"deadlock.exe\""))
            .unwrap_or(false);
    }
    #[allow(unreachable_code)]
    false
}

/// Free bytes on the drive holding `path` (its nearest existing folder).
pub fn free_space(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.is_dir())?;
    fs2::available_space(existing).ok()
}

/// Common causes of failed builds, checked right before Build starts. The
/// user can build anyway; each warning says what to do.
pub fn prebuild_warnings_with(settings: &Settings, running: bool, free: Option<u64>) -> Vec<String> {
    let mut out = vec![];
    if running {
        out.push("Deadlock is running. Close the game first: it locks the installed mods, so installing the new VPK will fail.".into());
    }
    if let Some(free) = free.filter(|f| *f < BUILD_FREE_BYTES) {
        out.push(format!("Only {:.1} GB is free on the workspace drive. A build needs about 10 GB; clear build data in Settings → Storage or free some space.", free as f64 / 1073741824.));
    }
    let synced: Vec<&str> = [("workspace", &settings.workspace), ("Community SDK", &settings.csdk), ("model toolset", &settings.cswin)]
        .into_iter()
        .filter(|(_, path)| path.to_ascii_lowercase().contains("onedrive"))
        .map(|(name, _)| name)
        .collect();
    if !synced.is_empty() {
        out.push(format!("The {} {} inside OneDrive. Syncing can lock files during the build; pause OneDrive syncing or move {} out of OneDrive if the build fails.",
            synced.join(" and "), if synced.len() == 1 { "is" } else { "are" }, if synced.len() == 1 { "it" } else { "them" }));
    }
    out
}

/// `prebuild_warnings_with` for this PC.
pub fn prebuild_warnings(settings: &Settings) -> Vec<String> {
    prebuild_warnings_with(settings, game_running(), free_space(Path::new(&settings.workspace)))
}

/// DepotDownloader.exe from what the user chose: the program, its folder, or
/// the folder it was extracted into.
pub fn depotdownloader(path: &Path) -> std::result::Result<PathBuf, String> {
    let shown = text(path);
    if shown.trim().is_empty() {
        return Err(format!("Choose DepotDownloader.exe. Download DepotDownloader-windows-x64.zip, extract it (right-click → Extract All), then select the DepotDownloader.exe inside, for example {EXAMPLE_DEPOTDOWNLOADER}."));
    }
    if is_zip(path) {
        return Err(format!("{shown} is the downloaded ZIP. Extract it first (right-click → Extract All), then select the DepotDownloader.exe inside the extracted folder."));
    }
    if let Some(found) = file_near(path, "DepotDownloader.exe", 2) {
        return Ok(found);
    }
    Err(if path.exists() {
        format!("DepotDownloader.exe was not found at {shown}. Select the DepotDownloader.exe inside the extracted DepotDownloader-windows-x64 folder, for example {EXAMPLE_DEPOTDOWNLOADER}.")
    } else {
        format!("{shown} does not exist. Select the DepotDownloader.exe inside the extracted DepotDownloader-windows-x64 folder, for example {EXAMPLE_DEPOTDOWNLOADER}.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }
    fn state(checks: &[FieldCheck], field: &str) -> (State, Option<String>) {
        let c = checks.iter().find(|c| c.field == field).unwrap();
        (c.state, c.fix.clone())
    }

    #[test]
    fn a_nested_sdk_extraction_and_the_wrong_compiler_copy_are_corrected() {
        let t = tempfile::tempdir().unwrap();
        let outer = t.path().join("Reduced_CSDK_12");
        let inner = outer.join("Reduced_CSDK_12");
        for rel in [CSDK_CONFIG, CSDK_COMPILER, CSDK_OTHER_COMPILERS[0]] { touch(&inner, rel); }
        let mut s = Settings { csdk: text(&outer), resourcecompiler: text(&inner.join(CSDK_OTHER_COMPILERS[0])), ..Default::default() };
        let checks = field_checks(&s);
        assert_eq!(state(&checks, "csdk"), (State::Problem, Some(text(&inner))));
        assert_eq!(state(&checks, "resourcecompiler"), (State::Warning, Some(text(&inner.join(CSDK_COMPILER)))));
        s = apply_fixes(&s);
        assert_eq!(s.csdk, text(&inner));
        assert_eq!(s.resourcecompiler, text(&inner.join(CSDK_COMPILER)));
        // Choosing a folder inside the SDK points back to the SDK itself.
        let picked = Settings { csdk: text(&inner.join("game/bin_cs2")), ..Default::default() };
        assert_eq!(state(&field_checks(&picked), "csdk").1, Some(text(&inner)));
    }

    #[test]
    fn the_model_toolset_compiler_in_the_material_field_and_a_missing_toolset_are_problems() {
        let t = tempfile::tempdir().unwrap();
        let sdk = t.path().join("CSDK12");
        for rel in [CSDK_CONFIG, CSDK_COMPILER] { touch(&sdk, rel); }
        let acquisition = t.path().join("DMBTools-1");
        let toolset = acquisition.join("toolset");
        touch(&toolset, TOOLSET_COMPILER);
        touch(&toolset, "game/csgo/gameinfo.gi");
        let crossed = Settings { csdk: text(&sdk), resourcecompiler: text(&toolset.join(TOOLSET_COMPILER)), cswin: text(&toolset), model_backend: "auto".into(), ..Default::default() };
        let checks = field_checks(&crossed);
        let (s, fix) = state(&checks, "resourcecompiler");
        assert_eq!((s, fix), (State::Problem, Some(text(&sdk.join(CSDK_COMPILER)))));
        assert!(checks.iter().find(|c| c.field == "resourcecompiler").unwrap().message.contains("Advanced configuration"));
        // No toolset selected: the CSDK fallback is reported, not hidden.
        let none = Settings { csdk: text(&sdk), model_backend: "auto".into(), ..Default::default() };
        assert_eq!(state(&field_checks(&none), "cswin").0, State::Problem);
        // The acquisition folder points to its toolset; the compiler and game follow.
        let fixed = apply_fixes(&Settings { cswin: text(&acquisition), model_backend: "auto".into(), ..Default::default() });
        assert_eq!(fixed.cswin, text(&toolset));
        assert_eq!(fixed.cswin_resourcecompiler, text(&toolset.join(TOOLSET_COMPILER)));
        assert_eq!(fixed.model_game, text(&toolset.join(TOOLSET_GAME)));
        assert!(field_checks(&fixed).iter().filter(|c| c.field.starts_with("cswin") || c.field == "model_game").all(|c| c.state == State::Ok));
        // The CSDK chosen as the model toolset is named as such.
        let sdk_as_model = Settings { cswin: text(&sdk), model_backend: "auto".into(), ..Default::default() };
        assert!(field_checks(&sdk_as_model).iter().find(|c| c.field == "cswin").unwrap().message.contains("Community SDK"));
    }

    #[test]
    fn viewer_app_folders_zips_and_archive_parts_point_to_the_right_file() {
        let t = tempfile::tempdir().unwrap();
        let tools = t.path().join("Program Installers");
        touch(&tools, "Source2Viewer.exe");
        touch(&tools, "cli-windows-x64/Source2Viewer-CLI.exe");
        touch(&tools, "DepotDownloader-windows-x64/DepotDownloader.exe");
        let game = t.path().join("Deadlock");
        touch(&game, DEADLOCK_PAK);
        touch(&game, "game/citadel/pak01_000.vpk");
        let s = Settings {
            source2viewer: text(&tools.join("Source2Viewer.exe")),
            deadlock: text(&game.join("game")),
            pak: text(&game.join("game/citadel/pak01_000.vpk")),
            ..Default::default()
        };
        let checks = field_checks(&s);
        assert_eq!(state(&checks, "source2viewer"), (State::Problem, Some(text(&tools.join("cli-windows-x64/Source2Viewer-CLI.exe")))));
        assert_eq!(state(&checks, "deadlock"), (State::Problem, Some(text(&game))));
        assert_eq!(state(&checks, "pak"), (State::Problem, Some(text(&game.join(DEADLOCK_PAK)))));
        assert_eq!(depotdownloader(&tools.join("DepotDownloader-windows-x64")).unwrap(), tools.join("DepotDownloader-windows-x64").join("DepotDownloader.exe"));
        assert!(depotdownloader(&tools.join("DepotDownloader-windows-x64.zip")).unwrap_err().contains("Extract"));
        assert!(depotdownloader(Path::new("")).unwrap_err().contains("DepotDownloader-windows-x64.zip"));
    }

    #[test]
    fn prebuild_warnings_name_the_game_low_space_and_onedrive_folders() {
        let clean = Settings { workspace: r"C:\DMB\work".into(), csdk: r"C:\CSDK12".into(), ..Default::default() };
        assert!(prebuild_warnings_with(&clean, false, Some(50 << 30)).is_empty());
        let synced = Settings { workspace: r"C:\Users\a\OneDrive\work".into(), csdk: r"C:\Users\a\OneDrive\Desktop\CSDK".into(), ..Default::default() };
        let warnings = prebuild_warnings_with(&synced, true, Some(3 << 30));
        assert_eq!(warnings.len(), 3);
        assert!(warnings[0].contains("Deadlock is running"));
        assert!(warnings[1].contains("3.0 GB"));
        assert!(warnings[2].contains("workspace and Community SDK are inside OneDrive"));
        assert!(free_space(Path::new(r"C:\definitely\missing\folder")).is_some() || !cfg!(windows));
    }

    #[test]
    fn empty_optional_fields_are_not_problems() {
        let checks = field_checks(&Settings::default());
        for field in ["steam", "optional_cs2", "vpk_tool", "blender"] {
            assert_eq!(state(&checks, field).0, State::Optional, "{field}");
        }
        for field in ["workspace", "deadlock", "pak", "csdk", "resourcecompiler", "source2viewer", "cswin"] {
            assert_eq!(state(&checks, field).0, State::Problem, "{field}");
        }
    }
}
