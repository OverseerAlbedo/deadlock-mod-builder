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
    /// A model compiler backend the Settings page offers as a button
    /// (`auto` for a toolset setup whose Community SDK compiles models).
    /// Never applied by Fix all or Detect tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// On the `csdk` check: the layout of the chosen SDK folder, when it is
    /// exactly an SDK (the sidebar's Notifications show the move to Reduced
    /// CSDK 13 for the older one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<SdkLayout>,
}

/// Material compiler inside Reduced CSDK 12 (the older layout). That SDK has
/// four copies (bin, bin_cs2, bin_server, bin_tools) with different
/// particles.dll builds; bin_cs2 is the tested one.
pub const CSDK_COMPILER: &str = "game/bin_cs2/win64/resourcecompiler.exe";
pub const CSDK_OTHER_COMPILERS: [&str; 3] = [
    "game/bin/win64/resourcecompiler.exe",
    "game/bin_tools/win64/resourcecompiler.exe",
    "game/bin_server/win64/resourcecompiler.exe",
];
/// Compiler of the current layout (Reduced CSDK 13, November 2026): no
/// bin_cs2, one bin set plus bin_server, which differs only in its active
/// particles.dll. Both compiled materials, textures and an AG2 hero model
/// byte for byte alike; bin is the tested one.
pub const CURRENT_CSDK_COMPILER: &str = "game/bin/win64/resourcecompiler.exe";
pub const CURRENT_CSDK_OTHER_COMPILERS: [&str; 1] = ["game/bin_server/win64/resourcecompiler.exe"];
const CSDK_CONFIG: &str = "game/citadel/gameinfo.gi";
const CSDK_CONTENT: &str = "content/citadel";
const DEADLOCK_PAK: &str = "game/citadel/pak01_dir.vpk";
const TOOLSET_COMPILER: &str = "game/bin/win64/resourcecompiler.exe";
const TOOLSET_GAME: &str = "game/csgo";
/// Folder paths longer than this risk Windows' 260-character limit for the
/// compiler's generated texture names (see deadlock-materials native_paths).
pub const LONG_CSDK_PATH: usize = 60;

pub const EXAMPLE_DEADLOCK: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Deadlock";
pub const EXAMPLE_PAK: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\Deadlock\game\citadel\pak01_dir.vpk";
pub const EXAMPLE_CSDK: &str = r"C:\CSDK13";
pub const EXAMPLE_COMPILER: &str = r"C:\CSDK13\game\bin\win64\resourcecompiler.exe";
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
    FieldCheck { field: field.into(), state, message: message.into(), fix: fix.map(|p| text(&p)), backend: None, layout: None }
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

/// The two shapes of the Community SDK. Told apart by what is in the folder,
/// not by its name, so a later SDK with the same shape is accepted too.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SdkLayout {
    /// Reduced CSDK 12: four compiler copies, bin_cs2 the tested one.
    /// Compiles materials only (hero models stop with a schema mismatch).
    Older,
    /// Reduced CSDK 13 and later: one bin set (plus bin_server). Compiles
    /// materials and hero models with AG2 nodes.
    Current,
}
impl SdkLayout {
    /// The compiler the app uses, relative to the SDK folder.
    pub fn compiler(self) -> &'static str {
        match self {
            SdkLayout::Older => CSDK_COMPILER,
            SdkLayout::Current => CURRENT_CSDK_COMPILER,
        }
    }
    /// The SDK's other compiler copies, relative to the SDK folder.
    pub fn other_compilers(self) -> &'static [&'static str] {
        match self {
            SdkLayout::Older => &CSDK_OTHER_COMPILERS,
            SdkLayout::Current => &CURRENT_CSDK_OTHER_COMPILERS,
        }
    }
    /// The compiler's folder as shown to players.
    pub fn compiler_folder(self) -> &'static str {
        match self {
            SdkLayout::Older => r"game\bin_cs2\win64",
            SdkLayout::Current => r"game\bin\win64",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            SdkLayout::Older => "Reduced CSDK 12",
            SdkLayout::Current => "Reduced CSDK 13",
        }
    }
    /// Whether its compiler builds hero models (for an AG2 hero it gives the
    /// same skeleton, AG2 references, attachments and cloth as the model
    /// toolset's compile).
    pub fn compiles_models(self) -> bool {
        self == SdkLayout::Current
    }
}

/// The layout of the SDK whose root is exactly `root`, if it is one. A
/// model toolset (game\csgo) or a Deadlock installation (pak01_dir.vpk) is
/// never taken for the current layout, whose compiler path they share.
pub fn sdk_layout(root: &Path) -> Option<SdkLayout> {
    if !root.join(CSDK_CONFIG).is_file() {
        return None;
    }
    if root.join(CSDK_COMPILER).is_file() {
        return Some(SdkLayout::Older);
    }
    let current = root.join(CURRENT_CSDK_COMPILER).is_file()
        && root.join(CSDK_CONTENT).is_dir()
        && !root.join(TOOLSET_GAME).exists()
        && !root.join(DEADLOCK_PAK).exists();
    current.then_some(SdkLayout::Current)
}

/// Community SDK root near `path` (itself, a parent, or a folder up to two
/// levels inside it): either layout of [`sdk_layout`].
pub fn csdk_root(path: &Path) -> Option<PathBuf> {
    ancestors(path, 5).into_iter().chain(descendants(path, 2)).find(|p| sdk_layout(p).is_some())
}

/// The layout of the SDK chosen in Settings, when that field is exactly an
/// SDK folder.
pub fn settings_sdk_layout(settings: &Settings) -> Option<SdkLayout> {
    let value = settings.csdk.trim();
    if value.is_empty() {
        return None;
    }
    sdk_layout(Path::new(value))
}

fn csdk(settings: &Settings) -> FieldCheck {
    let value = settings.csdk.trim();
    if value.is_empty() {
        return check("csdk", State::Problem, format!("Choose the extracted Reduced_CSDK_13 folder (it contains the folders game and content), for example {EXAMPLE_CSDK}."), None);
    }
    let path = Path::new(value);
    if is_zip(path) {
        return check("csdk", State::Problem, "This is the downloaded ZIP. Extract it first (right-click → Extract All), then choose the extracted Reduced_CSDK_13 folder.", None);
    }
    let Some(root) = csdk_root(path) else {
        return check("csdk", State::Problem, if path.exists() {
            r"This is not a complete Community SDK: game\citadel\gameinfo.gi or game\bin\win64\resourcecompiler.exe is missing. Extract the whole Reduced CSDK 13 ZIP again into a new, empty folder such as C:\CSDK13."
        } else { r"This folder does not exist. Choose the extracted Reduced_CSDK_13 folder, for example C:\CSDK13." }, None);
    };
    if !same(&root, path) {
        let why = if root.starts_with(path) { "The SDK was extracted inside another folder of the same name." } else { "This is a folder inside the SDK." };
        return check("csdk", State::Problem, format!("{why} The SDK itself is {}.", text(&root)), Some(root));
    }
    let layout = sdk_layout(&root).expect("csdk_root returns SDK folders");
    let name = layout.name();
    let found = if !settings.deadlock.trim().is_empty() && root.starts_with(Path::new(settings.deadlock.trim())) {
        check("csdk", State::Problem, format!(r"The SDK is inside the Deadlock game folder. Extract it to its own folder, for example {EXAMPLE_CSDK}."), None)
    } else if text(&root).len() > LONG_CSDK_PATH {
        check("csdk", State::Warning, format!(r"{name} found, but the folder path is long ({} characters). Long paths can make material compiles fail; move the folder to {EXAMPLE_CSDK} if they do.", text(&root).len()), None)
    } else if text(&root).to_lowercase().contains("onedrive") {
        check("csdk", State::Warning, format!(r"{name} found, but it is inside OneDrive. Syncing can lock compiler files; {EXAMPLE_CSDK} is safer."), None)
    } else {
        match layout {
            // The sidebar's Notifications show the steps to move over (from `layout`).
            SdkLayout::Older => check("csdk", State::Warning, "Reduced CSDK 12 found. Reduced CSDK 13 replaces it and also compiles hero models.", None),
            SdkLayout::Current => ok("csdk", "Reduced CSDK 13 found. It compiles materials and hero models."),
        }
    };
    FieldCheck { layout: Some(layout), ..found }
}

fn resourcecompiler(settings: &Settings) -> FieldCheck {
    let value = settings.resourcecompiler.trim();
    let root = csdk_root(Path::new(settings.csdk.trim())).filter(|_| !settings.csdk.trim().is_empty());
    let layout = root.as_deref().and_then(sdk_layout);
    let expected = root.as_ref().zip(layout).map(|(r, l)| r.join(l.compiler()));
    if value.is_empty() {
        let folder = layout.map_or(r"game\bin\win64", SdkLayout::compiler_folder);
        let example = match layout {
            Some(SdkLayout::Older) => r"C:\CSDK12\game\bin_cs2\win64\resourcecompiler.exe",
            _ => EXAMPLE_COMPILER,
        };
        return check("resourcecompiler", State::Problem, format!(r"Choose resourcecompiler.exe in the SDK's {folder} folder, for example {example}."), expected);
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
        let layout = layout.expect("expected comes from the layout");
        if same(path, expected) {
            return ok("resourcecompiler", match layout {
                SdkLayout::Older => "Material compiler found (bin_cs2, the tested copy).",
                SdkLayout::Current => r"Material compiler found (game\bin\win64, the tested copy).",
            });
        }
        let root = root.as_ref().expect("expected comes from root");
        if path.starts_with(root) {
            return check("resourcecompiler", State::Warning, format!("This is one of the SDK's other compiler copies. Use the one in {}, the copy the app is tested with.", layout.compiler_folder()), Some(expected.clone()));
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
/// What `path` is when it is not a model toolset, for the folders people
/// choose by mistake: a Community SDK folder (any level of it) or Deadlock's
/// own game files (downloaded from Deadlock's depot instead of the toolset).
pub fn toolset_mistake(path: &Path) -> Option<&'static str> {
    match csdk_root(path).as_deref().and_then(sdk_layout) {
        Some(SdkLayout::Older) => return Some("This folder is part of the Community SDK (Reduced CSDK 12), which only compiles materials. The model compiler is a separate toolset from Counter-Strike 2's Workshop Tools: on Set up model compiler…, click Fetch manifests and plan, then Download; the app selects the toolset folder for you. Reduced CSDK 13 compiles models too, without a separate toolset."),
        Some(SdkLayout::Current) => return Some("This folder is part of the Community SDK (Reduced CSDK 13), which compiles hero models itself. Choose it under Community SDK, leave Model compiler toolset empty and keep the model compiler backend on auto."),
        None => {}
    }
    let deadlock = |p: &Path| p.join("citadel").is_dir() && !p.join("csgo").is_dir();
    if ancestors(path, 2).iter().chain(&[path.join("game")]).any(|p| deadlock(p)) {
        return Some(r"This is Deadlock's game content (game\citadel), not the model toolset. The toolset comes from Counter-Strike 2's Workshop Tools (app 730); the app downloads only the files it needs with Fetch manifests and plan, then Download on Set up model compiler….");
    }
    None
}

/// The model compiler, following `toolchain::select`: with the backend on
/// auto, a Community SDK of the current layout (Reduced CSDK 13), which
/// compiles hero models itself, else the downloaded toolset. A model compile
/// with Reduced CSDK 12's compiler stops with a ParticleFloatType_t schema
/// mismatch.
fn model_compiler(settings: &Settings) -> Vec<FieldCheck> {
    let sdk_models = settings_sdk_layout(settings).is_some_and(SdkLayout::compiles_models);
    if settings.model_backend == "csdk" {
        if sdk_models {
            return vec![ok("cswin", "Models are compiled with Reduced CSDK 13 (model compiler backend: csdk); no separate model toolset is needed.")];
        }
        return vec![check("cswin", State::Problem, "Model compiler backend is set to CSDK, but this Community SDK cannot compile hero models (Reduced CSDK 12 stops with a schema mismatch). Set the backend to auto, then use Reduced CSDK 13 or set up the model toolset.", None)];
    }
    // Auto compiles with the SDK; whatever the toolset fields hold is unused.
    if sdk_models && settings.model_backend == "auto" {
        return vec![ok("cswin", "Models are compiled with Reduced CSDK 13; no separate model toolset is needed.")];
    }
    let mut out = toolset_checks(settings);
    // A toolset chosen on purpose (backend cswin) next to a Reduced CSDK 13:
    // offer the switch to auto. Only the Settings page button applies it.
    if settings.model_backend == "cswin" && sdk_models {
        if let Some(first) = out.iter_mut().find(|c| c.field == "cswin") {
            if first.state == State::Ok {
                first.message = "Hero models are compiled by your downloaded model toolset. Reduced CSDK 13 compiles them too, so the toolset is no longer needed.".into();
            }
            first.backend = Some("auto".into());
        }
    }
    out
}

/// The downloaded model toolset's fields, when the toolset compiles the models.
fn toolset_checks(settings: &Settings) -> Vec<FieldCheck> {
    let root_text = settings.cswin.trim();
    if root_text.is_empty() {
        let older = settings_sdk_layout(settings) == Some(SdkLayout::Older);
        return vec![check("cswin", State::Problem, if older {
            "The model compiler is not set up. Reduced CSDK 12 cannot compile hero models; Reduced CSDK 13 replaces it and compiles materials and the hero model, so choose it as the Community SDK. An already downloaded model toolset can still be chosen under Advanced configuration."
        } else if settings.model_backend == "cswin" && settings_sdk_layout(settings) == Some(SdkLayout::Current) {
            "The model compiler is not set up. The backend is set to cswin, but no model toolset is chosen; Reduced CSDK 13 compiles the hero model, so the backend can go back to auto."
        } else {
            crate::toolchain::MODEL_TOOLSET_NOT_SET
        }, None)];
    }
    let root_path = Path::new(root_text);
    if let Some(layout) = csdk_root(root_path).filter(|r| same(r, root_path)).as_deref().and_then(sdk_layout) {
        return vec![check("cswin", State::Problem, if layout.compiles_models() {
            "This is the Community SDK (Reduced CSDK 13), not the model toolset. It compiles models itself: choose it under Community SDK and leave Model compiler toolset empty."
        } else {
            "This is the Community SDK, not the model toolset. Choose the toolset folder you downloaded with Set up model compiler… (it contains game\\csgo)."
        }, None)];
    }
    let Some(root) = toolset_root(root_path) else {
        let why = toolset_mistake(root_path).map(str::to_owned).unwrap_or_else(|| format!(r"No model toolset here: game\bin\win64\resourcecompiler.exe or game\csgo\gameinfo.gi is missing. The downloaded toolset is the toolset folder inside your acquisition folder, for example {EXAMPLE_TOOLSET}."));
        return vec![check("cswin", State::Problem, why, None)];
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

/// Blender: nearly every model needs it, but an empty path is only a warning
/// (a prepared DMX builds without it), so it never reopens the setup guide.
fn blender(settings: &Settings) -> FieldCheck {
    if settings.blender.trim().is_empty() {
        return check("blender", State::Warning, "Blender is not set. It is needed to prepare Blender, FBX and glTF models; Detect tools finds an installed Blender.", None);
    }
    optional_file("blender", &settings.blender, "blender.exe", "Blender")
}

/// The `major.minor` version of the Blender at `executable`, read from the
/// version folder Blender keeps beside its program (`5.3\scripts`), so no
/// Blender has to start.
pub fn blender_version_folder(executable: &Path) -> Option<String> {
    let parent = executable.parent()?;
    let mut versions: Vec<(u32, u32, String)> = std::fs::read_dir(parent).ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("scripts").is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let (major, minor) = name.split_once('.')?;
            Some((major.parse().ok()?, minor.parse().ok()?, name))
        })
        .collect();
    versions.sort();
    versions.pop().map(|(_, _, name)| name)
}
/// The folder of a Blender's user extensions, as Blender finds it:
/// `BLENDER_USER_EXTENSIONS`, then `BLENDER_USER_RESOURCES\extensions`, then
/// a portable Blender's `portable\extensions` beside the program, then
/// `%APPDATA%\Blender Foundation\Blender\<version>\extensions`.
pub fn blender_extensions_folder(executable: &Path, version: &str, env: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    let set = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(folder) = set("BLENDER_USER_EXTENSIONS") {
        return Some(folder);
    }
    if let Some(folder) = set("BLENDER_USER_RESOURCES") {
        return Some(folder.join("extensions"));
    }
    let portable = executable.parent()?.join("portable");
    if portable.is_dir() {
        return Some(portable.join("extensions"));
    }
    Some(set("APPDATA")?.join("Blender Foundation").join("Blender").join(version).join("extensions"))
}
/// The MMD Tools extension in any repository folder (`blender_org`,
/// `user_default`, ...) of `extensions`.
pub fn mmd_tools_folder(extensions: &Path) -> Option<PathBuf> {
    let mut repos: Vec<PathBuf> = std::fs::read_dir(extensions).ok()?
        .filter_map(|e| e.ok())
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path().join("mmd_tools"))
        .filter(|p| p.join("blender_manifest.toml").is_file())
        .collect();
    repos.sort();
    repos.into_iter().next()
}
/// MMD Tools for the chosen Blender: only MMD models (.pmx, .pmd) need it, so
/// this is a note (found, or Optional), never a Warning or a Problem. A hint:
/// preparation itself turns the add-on on and says so when it cannot.
fn mmd_tools(settings: &Settings, env: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<FieldCheck> {
    let executable = Path::new(settings.blender.trim());
    if settings.blender.trim().is_empty() || !file_named(executable, "blender.exe") {
        return None;
    }
    let version = blender_version_folder(executable)?;
    let folder = blender_extensions_folder(executable, &version, env)?;
    Some(match mmd_tools_folder(&folder) {
        Some(_) => ok("mmd_tools", format!("MMD Tools found for Blender {version}: MMD models (.pmx, .pmd) can be prepared.")),
        None => check("mmd_tools", State::Optional, format!("{MMD_TOOLS_MISSING} {version}. Only MMD models (.pmx, .pmd) need it: in Blender {version}, open Edit → Preferences → Get Extensions, search for MMD Tools and click Install."), None),
    })
}
/// Start of the note when the add-on is missing (the Settings page and its tests).
pub const MMD_TOOLS_MISSING: &str = "MMD Tools is not installed for Blender";

/// Every path field of Settings, in the order the Settings page shows them.
pub fn field_checks(settings: &Settings) -> Vec<FieldCheck> {
    field_checks_with(settings, &|name| std::env::var_os(name))
}
fn field_checks_with(settings: &Settings, env: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Vec<FieldCheck> {
    let mut out = vec![workspace(settings), deadlock(settings), pak(settings), blender(settings)];
    out.extend(mmd_tools(settings, env));
    out.extend([source2viewer(settings), csdk(settings), resourcecompiler(settings)]);
    out.push(optional_folder("steam", &settings.steam));
    out.extend(model_compiler(settings));
    out.push(optional_folder("optional_cs2", &settings.optional_cs2));
    out.push(optional_file("vpk_tool", &settings.vpk_tool, "vpk.exe", "VPK tool"));
    out
}

/// Apply the suggested fixes to `settings` (Detect tools and the Fix all button).
pub fn apply_fixes(settings: &Settings) -> Settings {
    let mut result = settings.clone();
    // An earlier detection may have put the Community SDK into the model
    // compiler fields; clear them so Set up model compiler… can fill them.
    // A Reduced CSDK 13 compiles models itself: its csdk backend is kept.
    let sdk = csdk_root(Path::new(result.csdk.trim())).filter(|_| !result.csdk.trim().is_empty());
    let sdk_models = sdk.as_deref().and_then(sdk_layout).is_some_and(SdkLayout::compiles_models);
    let in_sdk = |value: &str| sdk.as_ref().is_some_and(|r| !value.trim().is_empty() && Path::new(value.trim()).starts_with(r));
    if (result.cswin.trim().is_empty() || in_sdk(&result.cswin)) && (in_sdk(&result.cswin_resourcecompiler) || in_sdk(&result.model_game) || result.model_backend == "csdk") {
        result.cswin.clear();
        result.cswin_resourcecompiler.clear();
        result.model_game.clear();
        if !sdk_models {
            result.model_backend = "auto".into();
        }
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

    /// Reduced CSDK 13's shape: one bin set plus bin_server, no bin_cs2.
    fn current_sdk(root: &Path) {
        for rel in [CSDK_CONFIG, CURRENT_CSDK_COMPILER, CURRENT_CSDK_OTHER_COMPILERS[0], "content/citadel/x.txt"] { touch(root, rel); }
    }

    #[test]
    fn a_nested_csdk_13_extraction_and_its_bin_server_copy_are_corrected() {
        let t = tempfile::tempdir().unwrap();
        let outer = t.path().join("Reduced_CSDK_13");
        let inner = outer.join("Reduced_CSDK_13");
        current_sdk(&inner);
        assert_eq!(sdk_layout(&inner), Some(SdkLayout::Current));
        let mut s = Settings { csdk: text(&outer), resourcecompiler: text(&inner.join(CURRENT_CSDK_OTHER_COMPILERS[0])), ..Default::default() };
        let checks = field_checks(&s);
        assert_eq!(state(&checks, "csdk"), (State::Problem, Some(text(&inner))));
        assert_eq!(state(&checks, "resourcecompiler"), (State::Warning, Some(text(&inner.join(CURRENT_CSDK_COMPILER)))));
        assert!(checks.iter().find(|c| c.field == "resourcecompiler").unwrap().message.contains(r"game\bin\win64"));
        s = apply_fixes(&s);
        assert_eq!(s.csdk, text(&inner));
        assert_eq!(s.resourcecompiler, text(&inner.join(CURRENT_CSDK_COMPILER)));
        let checks = field_checks(&s);
        let csdk = checks.iter().find(|c| c.field == "csdk").unwrap();
        // Ok, or a Warning for the long temporary path; never a Problem.
        assert_ne!(csdk.state, State::Problem);
        assert!(csdk.message.starts_with("Reduced CSDK 13 found"), "{}", csdk.message);
        assert_eq!(csdk.layout, Some(SdkLayout::Current));
        assert_eq!(state(&checks, "resourcecompiler").0, State::Ok);
        // A folder inside the SDK points back to it.
        let picked = Settings { csdk: text(&inner.join("game/bin")), ..Default::default() };
        assert_eq!(state(&field_checks(&picked), "csdk").1, Some(text(&inner)));
    }

    #[test]
    fn a_working_csdk_12_is_a_warning_without_a_fix_and_says_csdk_13_replaced_it() {
        let t = tempfile::tempdir().unwrap();
        let sdk = t.path().join("CSDK12");
        for rel in [CSDK_CONFIG, CSDK_COMPILER, CSDK_OTHER_COMPILERS[0]] { touch(&sdk, rel); }
        let s = Settings { csdk: text(&sdk), resourcecompiler: text(&sdk.join(CSDK_COMPILER)), ..Default::default() };
        let checks = field_checks(&s);
        let csdk = checks.iter().find(|c| c.field == "csdk").unwrap();
        // A Warning (never a Problem): it still compiles materials.
        assert_eq!((csdk.state, csdk.fix.as_deref(), csdk.layout), (State::Warning, None, Some(SdkLayout::Older)));
        assert_eq!(serde_json::to_value(csdk).unwrap()["layout"], "older");
        assert_eq!(apply_fixes(&s).csdk, s.csdk);
        // Without a toolset, the model compiler says CSDK 13 replaces it.
        let model = checks.iter().find(|c| c.field == "cswin").unwrap();
        assert_eq!(model.state, State::Problem);
        assert!(model.message.starts_with("The model compiler is not set up.") && model.message.contains("Reduced CSDK 12 cannot compile hero models") && model.message.contains("Reduced CSDK 13 replaces it"), "{}", model.message);
        assert!(csdk.message.starts_with("Reduced CSDK 12 found.") && csdk.message.contains("Reduced CSDK 13"), "{}", csdk.message);
        assert_eq!(state(&checks, "resourcecompiler").0, State::Ok);
        // bin is one of CSDK 12's other copies, not its compiler.
        let other = Settings { resourcecompiler: text(&sdk.join(CSDK_OTHER_COMPILERS[0])), ..s.clone() };
        assert_eq!(state(&field_checks(&other), "resourcecompiler"), (State::Warning, Some(text(&sdk.join(CSDK_COMPILER)))));
    }

    #[test]
    fn model_toolsets_and_game_installs_are_never_a_current_layout_sdk() {
        let t = tempfile::tempdir().unwrap();
        // A model toolset shares the compiler path; a stray citadel config
        // and content folder do not make it an SDK.
        let toolset = t.path().join("toolset");
        current_sdk(&toolset);
        touch(&toolset, "game/csgo/gameinfo.gi");
        assert_eq!(sdk_layout(&toolset), None);
        // Neither is Deadlock's own installation.
        let game = t.path().join("Deadlock");
        current_sdk(&game);
        touch(&game, DEADLOCK_PAK);
        assert_eq!(sdk_layout(&game), None);
        // Nor a folder without content\citadel.
        let partial = t.path().join("partial");
        for rel in [CSDK_CONFIG, CURRENT_CSDK_COMPILER] { touch(&partial, rel); }
        assert_eq!(sdk_layout(&partial), None);
        assert_eq!(csdk_root(&partial), None);
    }

    #[test]
    fn csdk_13_compiles_models_without_a_toolset_and_csdk_12_does_not() {
        let t = tempfile::tempdir().unwrap();
        let current = t.path().join("CSDK13");
        current_sdk(&current);
        let older = t.path().join("CSDK12");
        for rel in [CSDK_CONFIG, CSDK_COMPILER] { touch(&older, rel); }
        let cswin = |csdk: &Path, backend: &str| {
            let s = Settings { csdk: text(csdk), model_backend: backend.into(), ..Default::default() };
            field_checks(&s).into_iter().find(|c| c.field == "cswin").unwrap()
        };
        let auto = cswin(&current, "auto");
        assert_eq!(auto.state, State::Ok);
        assert!(auto.message.contains("no separate model toolset is needed"), "{}", auto.message);
        assert_eq!(cswin(&current, "csdk").state, State::Ok);
        assert_eq!(cswin(&older, "auto").state, State::Problem);
        let explicit = cswin(&older, "csdk");
        assert_eq!(explicit.state, State::Problem);
        assert!(explicit.message.contains("schema mismatch"));
        // Fix all keeps the csdk backend for CSDK 13 and clears the SDK's
        // own paths from the model toolset fields; CSDK 12 goes back to auto.
        let crossed = |csdk: &Path| Settings {
            csdk: text(csdk),
            cswin_resourcecompiler: text(&csdk.join("game/bin/win64/resourcecompiler.exe")),
            model_game: text(&csdk.join("game/citadel")),
            model_backend: "csdk".into(),
            ..Default::default()
        };
        let fixed = apply_fixes(&crossed(&current));
        assert_eq!((fixed.model_backend.as_str(), fixed.cswin_resourcecompiler.as_str(), fixed.model_game.as_str()), ("csdk", "", ""));
        assert_eq!(apply_fixes(&crossed(&older)).model_backend, "auto");
        // CSDK 13 chosen as the model toolset says to choose it as the SDK.
        let s = Settings { cswin: text(&current), model_backend: "auto".into(), ..Default::default() };
        let message = field_checks(&s).into_iter().find(|c| c.field == "cswin").unwrap().message;
        assert!(message.contains("Reduced CSDK 13") && message.contains("Community SDK"), "{message}");
        assert!(toolset_mistake(&current.join("game")).unwrap().contains("compiles hero models itself"));
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
        // So is a folder inside it (issue #2: Reduced_CSDK_12\game), and
        // Deadlock's own game files downloaded instead of the toolset.
        let message = |path: &Path| {
            let s = Settings { cswin: text(path), model_backend: "auto".into(), ..Default::default() };
            field_checks(&s).into_iter().find(|c| c.field == "cswin").unwrap().message
        };
        assert!(message(&sdk.join("game")).contains("part of the Community SDK"));
        let deadlock = t.path().join("ModelToolset");
        touch(&deadlock, "game/citadel/pak01_dir.vpk");
        touch(&deadlock, "game/core/panorama/x.txt");
        for picked in [deadlock.clone(), deadlock.join("game")] {
            assert!(message(&picked).contains("Deadlock's game content"), "{}", picked.display());
        }
        assert_eq!(toolset_mistake(&toolset), None);
        assert!(message(&t.path().join("empty")).contains("No model toolset here"));
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
        for field in ["steam", "optional_cs2", "vpk_tool"] {
            assert_eq!(state(&checks, field).0, State::Optional, "{field}");
        }
        // An empty Blender is a warning: needed by most models, never a Problem.
        let blender = checks.iter().find(|c| c.field == "blender").unwrap();
        assert_eq!((blender.state, blender.fix.as_deref()), (State::Warning, None));
        assert!(blender.message.starts_with("Blender is not set.") && blender.message.contains("FBX"), "{}", blender.message);
        for field in ["workspace", "deadlock", "pak", "csdk", "resourcecompiler", "source2viewer", "cswin"] {
            assert_eq!(state(&checks, field).0, State::Problem, "{field}");
        }
    }

    /// A new user's empty settings lead to Reduced CSDK 13 only: no message
    /// names the downloaded toolset or the tools it needs.
    #[test]
    fn empty_settings_never_point_to_the_model_toolset() {
        for check in field_checks(&Settings::default()) {
            for word in ["Set up model compiler", "DepotDownloader", "Counter-Strike", "CSDK 12", "toolset"] {
                assert!(!check.message.contains(word), "{}: {}", check.field, check.message);
            }
            assert_eq!((check.backend.as_deref(), check.layout), (None, None), "{}", check.field);
        }
        let model = field_checks(&Settings::default()).into_iter().find(|c| c.field == "cswin").unwrap();
        assert_eq!(model.message, crate::toolchain::MODEL_TOOLSET_NOT_SET);
        assert!(model.message.starts_with("The model compiler is not set up.") && model.message.contains("Reduced CSDK 13"));
    }

    /// A toolset chosen on purpose (backend cswin) next to a Reduced CSDK 13
    /// gets a one-click switch to auto; Fix all and a CSDK 12 never offer it.
    #[test]
    fn an_explicit_toolset_beside_csdk_13_is_offered_auto_but_never_switched() {
        let t = tempfile::tempdir().unwrap();
        let current = t.path().join("CSDK13");
        current_sdk(&current);
        let older = t.path().join("CSDK12");
        for rel in [CSDK_CONFIG, CSDK_COMPILER] { touch(&older, rel); }
        let toolset = t.path().join("DMBTools-1").join("toolset");
        touch(&toolset, TOOLSET_COMPILER);
        touch(&toolset, "game/csgo/gameinfo.gi");
        let setup = |sdk: &Path, backend: &str| Settings {
            csdk: text(sdk), resourcecompiler: text(&sdk.join(sdk_layout(sdk).unwrap().compiler())),
            cswin: text(&toolset), cswin_resourcecompiler: text(&toolset.join(TOOLSET_COMPILER)), model_game: text(&toolset.join(TOOLSET_GAME)),
            model_backend: backend.into(), ..Default::default()
        };
        let pinned = setup(&current, "cswin");
        let checks = field_checks(&pinned);
        let model = checks.iter().find(|c| c.field == "cswin").unwrap();
        assert_eq!((model.state, model.backend.as_deref()), (State::Ok, Some("auto")));
        assert!(model.message.contains("Reduced CSDK 13 compiles them too"), "{}", model.message);
        assert_eq!(state(&checks, "cswin_resourcecompiler").0, State::Ok);
        let json = serde_json::to_value(model).unwrap();
        assert_eq!(json["backend"], "auto");
        // Fix all leaves the backend and the toolset as they are.
        assert_eq!(apply_fixes(&pinned), pinned);
        // Only checks with a suggestion serialize the field.
        assert!(serde_json::to_value(&checks[0]).unwrap().get("backend").is_none());
        // With Reduced CSDK 12 the toolset is still needed: no suggestion.
        let older_pinned = setup(&older, "cswin");
        assert!(field_checks(&older_pinned).iter().all(|c| c.backend.is_none()));
        assert_eq!(field_checks(&older_pinned).into_iter().find(|c| c.field == "cswin").unwrap().message, "Model toolset found.");
        // Auto with CSDK 13 compiles with the SDK; the toolset fields are unused.
        let auto = setup(&current, "auto");
        let model = field_checks(&auto).into_iter().find(|c| c.field == "cswin").unwrap();
        assert_eq!((model.state, model.backend), (State::Ok, None));
        assert!(model.message.starts_with("Models are compiled with Reduced CSDK 13"), "{}", model.message);
        // An explicit cswin backend with no toolset is a Problem with the same suggestion.
        let empty = Settings { cswin: String::new(), cswin_resourcecompiler: String::new(), model_game: String::new(), ..pinned.clone() };
        let model = field_checks(&empty).into_iter().find(|c| c.field == "cswin").unwrap();
        assert_eq!((model.state, model.backend.as_deref()), (State::Problem, Some("auto")));
        assert!(model.message.starts_with("The model compiler is not set up."), "{}", model.message);
        assert_eq!(apply_fixes(&empty).model_backend, "cswin");
    }

    /// A Blender install: the program and its `5.3\scripts` version folder.
    fn blender_install(root: &Path, version: &str) -> PathBuf {
        touch(root, "blender.exe");
        touch(root, &format!("{version}/scripts/startup/x.py"));
        // A folder that only looks like a version is not taken.
        fs::create_dir_all(root.join("9.9")).unwrap();
        root.join("blender.exe")
    }
    fn mmd_checks(blender: &Path, env: &[(&str, &Path)]) -> Option<FieldCheck> {
        let env: Vec<(String, std::ffi::OsString)> = env.iter().map(|(k, v)| (k.to_string(), v.as_os_str().to_owned())).collect();
        let lookup = move |name: &str| env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        let settings = Settings { blender: text(blender), ..Default::default() };
        field_checks_with(&settings, &lookup).into_iter().find(|c| c.field == "mmd_tools")
    }

    #[test]
    fn mmd_tools_is_found_in_the_installed_blenders_extension_folder() {
        let t = tempfile::tempdir().unwrap();
        let exe = blender_install(&t.path().join("Blender 5.2"), "5.2");
        assert_eq!(blender_version_folder(&exe).as_deref(), Some("5.2"));
        let appdata = t.path().join("AppData");
        // Missing: a note, never a Warning or a Problem, and no fix.
        let missing = mmd_checks(&exe, &[("APPDATA", &appdata)]).unwrap();
        assert_eq!((missing.state, missing.fix.as_deref()), (State::Optional, None));
        assert!(missing.message.starts_with("MMD Tools is not installed for Blender 5.2."), "{}", missing.message);
        assert!(missing.message.contains("Get Extensions") && missing.message.contains(".pmx"), "{}", missing.message);
        // Another Blender version's copy does not count.
        touch(&appdata, "Blender Foundation/Blender/5.3/extensions/blender_org/mmd_tools/blender_manifest.toml");
        assert_eq!(mmd_checks(&exe, &[("APPDATA", &appdata)]).unwrap().state, State::Optional);
        // Installed from the online repository or from disk (user_default).
        touch(&appdata, "Blender Foundation/Blender/5.2/extensions/user_default/mmd_tools/blender_manifest.toml");
        let found = mmd_checks(&exe, &[("APPDATA", &appdata)]).unwrap();
        assert_eq!(found.state, State::Ok);
        assert!(found.message.starts_with("MMD Tools found for Blender 5.2"), "{}", found.message);
        // Blender's own folder variables come first.
        let own = t.path().join("own");
        assert_eq!(mmd_checks(&exe, &[("APPDATA", &appdata), ("BLENDER_USER_RESOURCES", &own)]).unwrap().state, State::Optional);
        touch(&own, "extensions/blender_org/mmd_tools/blender_manifest.toml");
        assert_eq!(mmd_checks(&exe, &[("APPDATA", &appdata), ("BLENDER_USER_RESOURCES", &own)]).unwrap().state, State::Ok);
    }

    #[test]
    fn a_portable_blender_keeps_its_extensions_beside_the_program() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("blender-5.3.0-windows-x64");
        let exe = blender_install(&root, "5.3");
        let appdata = t.path().join("AppData");
        touch(&appdata, "Blender Foundation/Blender/5.3/extensions/blender_org/mmd_tools/blender_manifest.toml");
        fs::create_dir_all(root.join("portable")).unwrap();
        let check = mmd_checks(&exe, &[("APPDATA", &appdata)]).unwrap();
        assert_eq!(check.state, State::Optional, "{}", check.message);
        assert!(check.message.starts_with("MMD Tools is not installed for Blender 5.3."), "{}", check.message);
        touch(&root, "portable/extensions/blender_org/mmd_tools/blender_manifest.toml");
        assert_eq!(mmd_checks(&exe, &[("APPDATA", &appdata)]).unwrap().state, State::Ok);
    }

    #[test]
    fn mmd_tools_is_not_checked_without_a_blender() {
        let empty = Settings::default();
        assert!(field_checks(&empty).iter().all(|c| c.field != "mmd_tools"));
        let t = tempfile::tempdir().unwrap();
        assert!(mmd_checks(&t.path().join("missing/blender.exe"), &[("APPDATA", t.path())]).is_none());
        // Nothing about it is ever a problem the setup guide reopens for.
        let exe = blender_install(&t.path().join("b"), "5.2");
        let check = mmd_checks(&exe, &[("APPDATA", t.path())]).unwrap();
        assert_ne!(check.state, State::Problem);
        assert_ne!(check.state, State::Warning);
    }
}
