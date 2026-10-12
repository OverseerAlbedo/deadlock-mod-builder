//! Disk use of project workspaces and the compiler folders the builder owns,
//! plus retention. Removal is limited to data the builder creates and can
//! recreate. A path that a project still references, a workspace whose build
//! lock is held and a compiler folder owned by another project are never removed.
use crate::vpk_error;
use deadlock_core::{CancellationToken, CoreError, Result};
use deadlock_project::{Project, Settings};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Shared extracted original hero sources, keyed by hero model and game version.
pub const SHARED_ORIGINALS: &str = "shared-originals";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// `humanoid_preparation`: one folder per Inspect, one `prepared-*` per Convert.
    Preparation,
    /// Extracted original hero sources and original compiled model.
    Originals,
    /// Replaced original sources and replaced VPKs.
    Backups,
    /// The built VPK.
    Output,
    /// Caches and staging that the next Analyze or Build recreates.
    BuildData,
    /// Per-project compiler content/game folders in the CSDK and model toolset.
    Compiler,
    /// Leftovers of interrupted operations.
    Temporary,
    /// Project snapshots, build records and analysis.
    Records,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Retention {
    /// Replaced VPKs and original-source backups kept per file (0 keeps none).
    pub keep_backups: usize,
    /// Keep the newest preparation run besides the one the project uses.
    pub keep_previous_preparation: bool,
}
impl Retention {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            keep_backups: settings.keep_backups as usize,
            keep_previous_preparation: settings.keep_previous_preparation,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usage {
    pub kind: Kind,
    pub bytes: u64,
    /// Bytes that Clear for this kind would remove now.
    pub reclaimable: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStorage {
    pub id: String,
    pub name: String,
    pub hero: String,
    /// A saved project file (outside the workspace) known to use this workspace.
    pub project_file: Option<String>,
    /// The project currently open in the app.
    pub open: bool,
    /// A build lock is present (a build is running or crashed).
    pub locked: bool,
    pub workspace: Option<String>,
    /// Newest modification time of the workspace's top-level entries (Unix seconds).
    pub modified: Option<u64>,
    pub bytes: u64,
    pub reclaimable: u64,
    pub usage: Vec<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedOriginal {
    pub key: String,
    pub model: String,
    pub bytes: u64,
    pub used_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageReport {
    pub workspace: String,
    pub bytes: u64,
    pub reclaimable: u64,
    pub projects: Vec<ProjectStorage>,
    pub shared: Vec<SharedOriginal>,
    /// Workspace entries that belong to no project (not removed by Clear).
    pub other_bytes: u64,
}

/// What the app knows about a project besides its workspace folder.
#[derive(Debug, Clone, Default)]
pub struct KnownProject {
    /// Saved project files that still exist and carry this project's ID.
    pub files: Vec<PathBuf>,
    /// The current in-app state of the open project.
    pub open: Option<Project>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClearRequest {
    /// One project, or every project when absent.
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub kinds: Vec<Kind>,
    /// Remove the whole workspace and compiler folders of `project`.
    #[serde(default)]
    pub remove_project: bool,
    /// Remove shared original hero sources that no project uses.
    #[serde(default)]
    pub unused_shared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ClearResult {
    pub removed_bytes: u64,
    pub removed: Vec<String>,
    pub skipped: Vec<String>,
}

const TEMPORARY_PREFIXES: &[&str] = &[
    "stage-",
    "install-package-",
    "install-contract-",
    "base-pending-",
    "pending_",
    ".dmb-publish-",
    "customization-build-",
];
const BUILD_DATA: &[&str] = &[
    "split-dmx",
    "material-cache",
    "effective-sources",
    "prepared-content",
    "prepared-game",
    "original-test-content",
    "original-test-game",
    "compiler-preview-content",
    "compiler-preview-game",
    "components-content",
    "components-game",
    // Earlier versions kept these at the workspace root.
    "native-model",
    "native-model.zip",
    "original-compiled.zip",
    "build-evidence",
    "install-motion-check",
];

pub fn classify(name: &str) -> Kind {
    match name {
        "humanoid_preparation" => Kind::Preparation,
        "base" | "original-compiled" | "original-source-view" | "base-fingerprint.json"
        | "original-inspection.json" => Kind::Originals,
        "backups" => Kind::Backups,
        "output" => Kind::Output,
        n if BUILD_DATA.contains(&n) => Kind::BuildData,
        n if TEMPORARY_PREFIXES.iter().any(|p| n.starts_with(p)) => Kind::Temporary,
        _ => Kind::Records,
    }
}

pub fn is_project_id(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn io(context: &str, error: std::io::Error) -> CoreError {
    CoreError::io(context.to_owned(), error)
}

/// Bytes and files below `path`, never following links.
pub fn measure(path: &Path, cancel: &CancellationToken) -> Result<u64> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(0);
    };
    if !meta.is_dir() {
        return Ok(if meta.is_file() { meta.len() } else { 0 });
    }
    let mut total = 0;
    let mut pending = vec![path.to_path_buf()];
    while let Some(folder) = pending.pop() {
        cancel.check()?;
        // Files can disappear while measuring; sizes are a snapshot.
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                pending.push(entry.path());
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    Ok(total)
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::symlink_metadata(path).and_then(|m| m.modified()).ok()
}

/// Comparable form of a filesystem path (separators, case on Windows).
fn key(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.trim_end_matches('/').to_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}
fn contains(parent: &Path, child: &str) -> bool {
    let parent = key(parent);
    child == parent || child.starts_with(&format!("{parent}/"))
}

/// Every absolute path a project mentions, in comparable form.
pub fn references(project: &Project) -> BTreeSet<String> {
    fn walk(value: &serde_json::Value, output: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::String(text) => {
                if deadlock_project::asset_paths::is_absolute(text) {
                    output.insert(key(Path::new(text)));
                }
            }
            serde_json::Value::Array(values) => values.iter().for_each(|v| walk(v, output)),
            serde_json::Value::Object(map) => map.values().for_each(|v| walk(v, output)),
            _ => {}
        }
    }
    let mut output = BTreeSet::new();
    if let Ok(value) = serde_json::to_value(project) {
        walk(&value, &mut output);
    }
    output
}

/// Projects that describe a workspace: the open project, saved files, and the
/// snapshots the builder writes into the workspace itself.
fn describing_projects(root: &Path, known: Option<&KnownProject>) -> (Vec<Project>, bool) {
    let mut projects = Vec::new();
    let mut complete = true;
    if let Some(known) = known {
        projects.extend(known.open.clone());
        for file in &known.files {
            match Project::load(file) {
                Ok(project) => projects.push(project),
                Err(_) => complete = false,
            }
        }
    }
    for name in ["project.dmb.json", "last-project.dmb.json"] {
        let file = root.join(name);
        if file.is_file() {
            match Project::load(&file) {
                Ok(project) => projects.push(project),
                Err(_) => complete = false,
            }
        }
    }
    (projects, complete)
}

fn protected_paths(projects: &[Project]) -> BTreeSet<String> {
    projects.iter().flat_map(references).collect()
}

fn referenced(path: &Path, protected: &BTreeSet<String>) -> bool {
    protected.iter().any(|p| contains(path, p))
}

/// Subfolders of `folder` sorted newest first.
fn folders_newest_first(folder: &Path, prefix: Option<&str>) -> Vec<PathBuf> {
    let mut found: Vec<(Option<SystemTime>, PathBuf)> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| prefix.is_none_or(|p| e.file_name().to_string_lossy().starts_with(p)))
        .map(|e| (modified(&e.path()), e.path()))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Preparation runs and `prepared-*` results that can be removed. `keep` names
/// the run the app is working with (for example the one an inspection just
/// created). Without it the newest unused run stays, since the app may be
/// reviewing it.
pub fn preparation_candidates(
    root: &Path,
    protected: &BTreeSet<String>,
    keep: Option<&Path>,
    retention: Retention,
) -> Vec<PathBuf> {
    let runs = folders_newest_first(&root.join("humanoid_preparation"), None);
    let mut kept_unreferenced = 0;
    let keep_unreferenced =
        usize::from(keep.is_none()) + usize::from(retention.keep_previous_preparation);
    let mut output = Vec::new();
    for run in runs {
        let pinned = keep.is_some_and(|k| key(k) == key(&run));
        if !pinned && !referenced(&run, protected) {
            if kept_unreferenced < keep_unreferenced {
                kept_unreferenced += 1;
            } else {
                output.push(run);
                continue;
            }
        }
        // Inside a kept run, only the newest result and referenced results stay.
        let results = folders_newest_first(&run, Some("prepared-"));
        for result in results.into_iter().skip(1) {
            if !referenced(&result, protected) {
                output.push(result);
            }
        }
    }
    output
}

/// Backups beyond the newest `keep` per original file.
pub fn backup_candidates(backups: &Path, keep: usize) -> Vec<PathBuf> {
    let mut groups: BTreeMap<String, Vec<(Option<SystemTime>, PathBuf)>> = BTreeMap::new();
    for entry in fs::read_dir(backups).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // base-<uuid> folders, and <file>.<uuid>.bak copies.
        let group = if name.starts_with("base-") {
            "base".to_owned()
        } else if let Some(stem) = name.strip_suffix(".bak") {
            stem.rsplit_once('.').map_or(stem, |(file, _)| file).to_owned()
        } else {
            continue;
        };
        groups
            .entry(group)
            .or_default()
            .push((modified(&entry.path()), entry.path()));
    }
    let mut output = Vec::new();
    for mut items in groups.into_values() {
        items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        output.extend(items.into_iter().skip(keep).map(|(_, p)| p));
    }
    output
}

fn top_level(root: &Path) -> Vec<(String, PathBuf)> {
    let mut entries: Vec<_> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    entries.sort();
    entries
}

/// Per-project compiler folders: `dmb_<id16>` in the CSDK and
/// `dmb_<id16>_model` in the model toolset, owned through their marker file.
pub fn compiler_folders(settings: &Settings) -> BTreeMap<String, Vec<PathBuf>> {
    let mut parents = Vec::new();
    if !settings.csdk.is_empty() {
        let csdk = Path::new(&settings.csdk);
        parents.push(csdk.join("content/citadel_addons"));
        parents.push(csdk.join("game/citadel_addons"));
    }
    if !settings.cswin.is_empty() {
        if let Some(target) = Path::new(&settings.model_game)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
        {
            let toolset = Path::new(&settings.cswin);
            parents.push(toolset.join(format!("content/{target}_addons")));
            parents.push(toolset.join(format!("game/{target}_addons")));
        }
    }
    let mut seen = BTreeSet::new();
    let mut output: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for parent in parents {
        if !seen.insert(key(&parent)) {
            continue;
        }
        for (name, path) in top_level(&parent) {
            let Some(rest) = name.strip_prefix("dmb_") else {
                continue;
            };
            let short = rest.strip_suffix("_model").unwrap_or(rest);
            if short.len() != 16 || !short.bytes().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            if let Some(owner) = owner(&path) {
                if owner.starts_with(short) {
                    output.entry(owner).or_default().push(path);
                }
            } else if folder_has_no_files(&path) {
                // An empty folder without a marker: matched by its short ID.
                output.entry(format!("dmb_{short}")).or_default().push(path);
            }
        }
    }
    output
}
fn owner(folder: &Path) -> Option<String> {
    let bytes = fs::read(folder.join(".deadlock-builder.json")).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let id = value.get("project")?.as_str()?;
    is_project_id(id).then(|| id.to_owned())
}
fn folder_has_no_files(folder: &Path) -> bool {
    let mut pending = vec![folder.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).into_iter().flatten().flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => pending.push(entry.path()),
                _ => return false,
            }
        }
    }
    true
}

fn shared_key_of(root: &Path) -> Option<String> {
    let bytes = fs::read(root.join("base-fingerprint.json")).ok()?;
    crate::sources::shared_key(&bytes)
}

fn workspace_root(settings: &Settings) -> Result<PathBuf> {
    if settings.workspace.is_empty() {
        return Err(crate::invalid("Choose a workspace folder in Settings."));
    }
    deadlock_project::asset_paths::resolve(Path::new(&settings.workspace))
        .map_err(|e| crate::invalid(e.to_string()))
}

struct Scan {
    workspace: PathBuf,
    ids: BTreeSet<String>,
    compiler: BTreeMap<String, Vec<PathBuf>>,
}
fn scan(settings: &Settings, known: &BTreeMap<String, KnownProject>) -> Result<Scan> {
    let workspace = workspace_root(settings)?;
    let mut ids: BTreeSet<String> = top_level(&workspace)
        .into_iter()
        .filter(|(name, path)| is_project_id(name) && path.is_dir())
        .map(|(name, _)| name)
        .collect();
    let mut compiler = compiler_folders(settings);
    // Unmarked empty folders join the project whose ID they start with.
    for short in compiler
        .keys()
        .filter(|k| k.starts_with("dmb_"))
        .cloned()
        .collect::<Vec<_>>()
    {
        let prefix = &short[4..];
        if let Some(id) = ids
            .iter()
            .chain(known.keys())
            .chain(compiler.keys().filter(|k| is_project_id(k)))
            .find(|id| id.starts_with(prefix))
            .cloned()
        {
            let folders = compiler.remove(&short).unwrap_or_default();
            compiler.entry(id).or_default().extend(folders);
        }
    }
    compiler.retain(|id, _| is_project_id(id));
    ids.extend(compiler.keys().cloned());
    Ok(Scan {
        workspace,
        ids,
        compiler,
    })
}

struct Plan {
    usage: BTreeMap<Kind, (u64, Vec<PathBuf>)>,
    removable: BTreeMap<Kind, Vec<PathBuf>>,
}

fn plan_project(
    settings: &Settings,
    root: &Path,
    compiler: &[PathBuf],
    known: Option<&KnownProject>,
    cancel: &CancellationToken,
) -> Result<(Plan, Vec<Project>)> {
    let retention = Retention::from_settings(settings);
    let (projects, complete) = describing_projects(root, known);
    let protected = protected_paths(&projects);
    let mut usage: BTreeMap<Kind, (u64, Vec<PathBuf>)> = BTreeMap::new();
    let mut removable: BTreeMap<Kind, Vec<PathBuf>> = BTreeMap::new();
    for (name, path) in top_level(root) {
        cancel.check()?;
        let kind = classify(&name);
        let entry = usage.entry(kind).or_default();
        entry.0 += measure(&path, cancel)?;
        entry.1.push(path.clone());
        let candidate = match kind {
            Kind::BuildData | Kind::Temporary => !referenced(&path, &protected),
            Kind::Originals => {
                // The builder extracts these again; an imported-only project cannot.
                name == "base"
                    && !referenced(&path, &protected)
                    && projects.iter().all(|p| p.auto_decompile)
            }
            _ => false,
        };
        if candidate {
            removable.entry(kind).or_default().push(path);
        }
    }
    // A saved project that cannot be read might use any run: keep them all.
    if complete {
        removable.insert(
            Kind::Preparation,
            preparation_candidates(root, &protected, None, retention),
        );
    }
    removable.insert(
        Kind::Backups,
        backup_candidates(&root.join("backups"), retention.keep_backups),
    );
    if !compiler.is_empty() {
        let entry = usage.entry(Kind::Compiler).or_default();
        for folder in compiler {
            entry.0 += measure(folder, cancel)?;
            entry.1.push(folder.clone());
        }
        removable.insert(Kind::Compiler, compiler.to_vec());
    }
    Ok((Plan { usage, removable }, projects))
}

fn latest(root: &Path) -> Option<u64> {
    top_level(root)
        .iter()
        .filter_map(|(_, p)| modified(p))
        .max()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

pub fn report(
    settings: &Settings,
    known: &BTreeMap<String, KnownProject>,
    cancel: &CancellationToken,
) -> Result<StorageReport> {
    let scan = scan(settings, known)?;
    let mut output = StorageReport {
        workspace: scan.workspace.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let mut shared_users: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in &scan.ids {
        cancel.check()?;
        let root = scan.workspace.join(id);
        let exists = root.is_dir();
        let compiler = scan.compiler.get(id).cloned().unwrap_or_default();
        let known_project = known.get(id);
        let (plan, projects) = plan_project(settings, &root, &compiler, known_project, cancel)?;
        if let Some(key) = exists.then(|| shared_key_of(&root)).flatten() {
            shared_users.entry(key).or_default().push(id.clone());
        }
        let mut usage = Vec::new();
        for (kind, (bytes, _)) in &plan.usage {
            let reclaimable = plan
                .removable
                .get(kind)
                .map(|paths| paths.iter().map(|p| measure(p, cancel)).sum::<Result<u64>>())
                .transpose()?
                .unwrap_or(0);
            usage.push(Usage {
                kind: *kind,
                bytes: *bytes,
                reclaimable: reclaimable.min(*bytes),
            });
        }
        let named = projects.first();
        let open = known_project.is_some_and(|k| k.open.is_some());
        let project = ProjectStorage {
            id: id.clone(),
            name: named.map(|p| p.name.clone()).unwrap_or_default(),
            hero: named.map(|p| p.hero_name.clone()).unwrap_or_default(),
            project_file: known_project
                .and_then(|k| k.files.first())
                .map(|p| p.to_string_lossy().into_owned()),
            open,
            locked: root.join("build.lock").exists(),
            workspace: exists.then(|| root.to_string_lossy().into_owned()),
            modified: latest(&root),
            bytes: usage.iter().map(|u| u.bytes).sum(),
            reclaimable: usage.iter().map(|u| u.reclaimable).sum(),
            usage,
        };
        output.bytes += project.bytes;
        output.reclaimable += project.reclaimable;
        output.projects.push(project);
    }
    output
        .projects
        .sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.id.cmp(&b.id)));
    let shared_root = scan.workspace.join(SHARED_ORIGINALS);
    for (key, path) in top_level(&shared_root) {
        cancel.check()?;
        let bytes = measure(&path, cancel)?;
        let model = fs::read(path.join("fingerprint.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<crate::sources::BaseFingerprint>(&b).ok())
            .map(|f| f.model)
            .unwrap_or_default();
        let used_by = shared_users.remove(&key).unwrap_or_default();
        output.bytes += bytes;
        if used_by.is_empty() {
            output.reclaimable += bytes;
        }
        output.shared.push(SharedOriginal {
            key,
            model,
            bytes,
            used_by,
        });
    }
    for (name, path) in top_level(&scan.workspace) {
        if !is_project_id(&name) && name != SHARED_ORIGINALS {
            output.other_bytes += measure(&path, cancel)?;
        }
    }
    Ok(output)
}

pub fn remove(path: &Path) -> Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io("Inspect storage entry", e)),
    };
    deadlock_vpk::check_unlinked_path(path, false).map_err(vpk_error)?;
    if meta.is_dir() {
        deadlock_vpk::remove_snapshot(path).map_err(vpk_error)
    } else {
        let mut permissions = meta.permissions();
        if permissions.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions).map_err(|e| io("Unlock storage entry", e))?;
        }
        fs::remove_file(path).map_err(|e| io("Remove storage entry", e))
    }
}

fn remove_all(paths: &[PathBuf], result: &mut ClearResult, cancel: &CancellationToken) -> Result<()> {
    for path in paths {
        cancel.check()?;
        let bytes = measure(path, cancel)?;
        match remove(path) {
            Ok(()) => {
                result.removed_bytes += bytes;
                result.removed.push(path.to_string_lossy().into_owned());
            }
            Err(error) => result
                .skipped
                .push(format!("{}: {error}", path.to_string_lossy())),
        }
    }
    Ok(())
}

/// Compiler folders are removed only when their marker still names this project.
fn removable_compiler(id: &str, folders: &[PathBuf]) -> Vec<PathBuf> {
    folders
        .iter()
        .filter(|f| owner(f).as_deref() == Some(id) || folder_has_no_files(f))
        .cloned()
        .collect()
}

pub fn clear(
    settings: &Settings,
    known: &BTreeMap<String, KnownProject>,
    request: &ClearRequest,
    cancel: &CancellationToken,
) -> Result<ClearResult> {
    let scan = scan(settings, known)?;
    let mut result = ClearResult::default();
    if request.remove_project {
        let id = request
            .project
            .as_deref()
            .ok_or_else(|| crate::invalid("Choose the project to remove."))?;
        if !is_project_id(id) || !scan.ids.contains(id) {
            return Err(crate::invalid("This project has no stored data."));
        }
        if known.get(id).is_some_and(|k| k.open.is_some()) {
            return Err(crate::invalid(
                "This project is open. Open another project before removing its stored data.",
            ));
        }
        let root = scan.workspace.join(id);
        if root.join("build.lock").exists() {
            return Err(crate::invalid(
                "A build lock is present in this workspace. Close any running build first.",
            ));
        }
        let compiler = scan.compiler.get(id).cloned().unwrap_or_default();
        remove_all(&removable_compiler(id, &compiler), &mut result, cancel)?;
        if root.is_dir() {
            remove_all(&[root], &mut result, cancel)?;
        }
    }
    if !request.kinds.is_empty() {
        let ids: Vec<&String> = match request.project.as_deref() {
            Some(id) => scan.ids.iter().filter(|i| *i == id).collect(),
            None => scan.ids.iter().collect(),
        };
        for id in ids {
            cancel.check()?;
            let root = scan.workspace.join(id);
            if root.join("build.lock").exists() {
                result
                    .skipped
                    .push(format!("{id}: a build lock is present, nothing was removed"));
                continue;
            }
            let compiler = scan.compiler.get(id).cloned().unwrap_or_default();
            let (plan, _) = plan_project(settings, &root, &compiler, known.get(id), cancel)?;
            for kind in &request.kinds {
                let paths = plan.removable.get(kind).cloned().unwrap_or_default();
                let paths = if *kind == Kind::Compiler {
                    removable_compiler(id, &paths)
                } else {
                    paths
                };
                remove_all(&paths, &mut result, cancel)?;
            }
        }
    }
    if request.unused_shared {
        let used: BTreeSet<String> = scan
            .ids
            .iter()
            .filter_map(|id| shared_key_of(&scan.workspace.join(id)))
            .collect();
        let shared = scan.workspace.join(SHARED_ORIGINALS);
        let unused: Vec<PathBuf> = top_level(&shared)
            .into_iter()
            .filter(|(key, _)| !used.contains(key))
            .map(|(_, p)| p)
            .collect();
        remove_all(&unused, &mut result, cancel)?;
    }
    Ok(result)
}

/// Retention after an inspection or conversion: remove preparation runs and
/// results that no project uses. `keep` is the run just created.
/// `current` are in-app states of the project (for example before and after
/// Accept); `files` are its saved project files. Nothing is removed when a saved
/// file cannot be read, because it might use any run.
pub fn prune_preparation(
    settings: &Settings,
    current: &[&Project],
    files: &[PathBuf],
    keep: Option<&Path>,
    log: &dyn Fn(&str),
) {
    let (Some(first), Ok(workspace)) = (current.first(), workspace_root(settings)) else {
        return;
    };
    let root = workspace.join(&first.id);
    if root.join("build.lock").exists() {
        return;
    }
    let known = KnownProject {
        files: files.to_vec(),
        open: None,
    };
    let (mut projects, complete) = describing_projects(&root, Some(&known));
    if !complete {
        return;
    }
    projects.extend(current.iter().map(|p| (*p).clone()));
    let protected = protected_paths(&projects);
    let candidates =
        preparation_candidates(&root, &protected, keep, Retention::from_settings(settings));
    let mut result = ClearResult::default();
    if remove_all(&candidates, &mut result, &CancellationToken::new()).is_ok()
        && !result.removed.is_empty()
    {
        log(&format!(
            "Removed {} unused preparation folder(s), {:.1} MB",
            result.removed.len(),
            result.removed_bytes as f64 / 1_048_576.0
        ));
    }
}

/// Retention after a backup was written.
pub fn prune_backups(backups: &Path, keep: usize, log: &dyn Fn(&str)) {
    let mut result = ClearResult::default();
    if remove_all(
        &backup_candidates(backups, keep),
        &mut result,
        &CancellationToken::new(),
    )
    .is_ok()
        && !result.removed.is_empty()
    {
        log(&format!(
            "Removed {} older backup(s), {:.1} MB",
            result.removed.len(),
            result.removed_bytes as f64 / 1_048_576.0
        ));
    }
}

/// Optional cleanup after a successful build: caches, staging and compiler
/// folders that the next build recreates.
pub fn clear_after_build(
    settings: &Settings,
    project: &Project,
    cancel: &CancellationToken,
) -> Result<ClearResult> {
    let known = BTreeMap::from([(
        project.id.clone(),
        KnownProject {
            files: vec![],
            open: Some(project.clone()),
        },
    )]);
    clear(
        settings,
        &known,
        &ClearRequest {
            project: Some(project.id.clone()),
            kinds: vec![Kind::BuildData, Kind::Temporary, Kind::Compiler],
            remove_project: false,
            unused_shared: false,
        },
        cancel,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn touch(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![7u8; bytes]).unwrap();
    }
    // Entries are created oldest first; folder times follow creation order.
    fn later() {
        std::thread::sleep(Duration::from_millis(25));
    }
    fn settings(workspace: &Path) -> Settings {
        Settings {
            workspace: workspace.to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn classifies_workspace_entries() {
        assert_eq!(classify("humanoid_preparation"), Kind::Preparation);
        assert_eq!(classify("base"), Kind::Originals);
        assert_eq!(classify("split-dmx"), Kind::BuildData);
        assert_eq!(classify("native-model"), Kind::BuildData);
        assert_eq!(classify("stage-0123"), Kind::Temporary);
        assert_eq!(classify("customization-build-ab"), Kind::Temporary);
        assert_eq!(classify("customization-build.json"), Kind::Records);
        assert_eq!(classify("runtime-motion"), Kind::Records);
        assert_eq!(classify("output"), Kind::Output);
    }

    #[test]
    fn keeps_used_newest_and_previous_preparation_runs() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let prep = root.join("humanoid_preparation");
        let used = prep.join("used");
        for run in ["old", "used", "previous", "newest"] {
            touch(&prep.join(run).join("source/file.blend"), 10);
            if run == "used" {
                for result in ["prepared-a", "prepared-b", "prepared-c"] {
                    later();
                    touch(&used.join(result).join("model.dmx"), 10);
                }
            }
            later();
        }
        let protected = BTreeSet::from([key(&used.join("prepared-a/model.dmx"))]);
        let keep_previous = Retention {
            keep_backups: 1,
            keep_previous_preparation: true,
        };
        let mut found = preparation_candidates(root, &protected, None, keep_previous);
        found.sort();
        // prepared-c is the newest result; prepared-a is referenced.
        assert_eq!(found, vec![prep.join("old"), used.join("prepared-b")]);
        let only_used = Retention {
            keep_previous_preparation: false,
            ..keep_previous
        };
        let mut found = preparation_candidates(root, &protected, None, only_used);
        found.sort();
        assert_eq!(
            found,
            vec![prep.join("old"), prep.join("previous"), used.join("prepared-b")]
        );
        // The run the app works with replaces the newest-run safety margin.
        let mut found =
            preparation_candidates(root, &protected, Some(&prep.join("old")), only_used);
        found.sort();
        assert_eq!(
            found,
            vec![prep.join("newest"), prep.join("previous"), used.join("prepared-b")]
        );
        let mut found =
            preparation_candidates(root, &protected, Some(&prep.join("old")), keep_previous);
        found.sort();
        assert_eq!(found, vec![prep.join("previous"), used.join("prepared-b")]);
    }

    #[test]
    fn keeps_newest_backups_per_file() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path();
        for name in [
            "pak18_dir.vpk.ccc.bak",
            "pak18_dir.vpk.aaa.bak",
            "pak18_dir.vpk.bbb.bak",
            "pak20_dir.vpk.ddd.bak",
        ] {
            touch(&backups.join(name), 4);
            later();
        }
        for name in ["base-2", "base-1"] {
            touch(&backups.join(name).join("x.vmdl"), 4);
            later();
        }
        let mut found = backup_candidates(backups, 1);
        found.sort();
        assert_eq!(
            found,
            vec![
                backups.join("base-2"),
                backups.join("pak18_dir.vpk.aaa.bak"),
                backups.join("pak18_dir.vpk.ccc.bak"),
            ]
        );
        assert_eq!(backup_candidates(backups, 0).len(), 6);
    }

    #[test]
    fn reports_and_clears_without_touching_referenced_data() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspaces");
        let id = "0123456789abcdef0123456789abcdef";
        let root = workspace.join(id);
        let run = root.join("humanoid_preparation/run1");
        touch(&run.join("prepared-1/model.dmx"), 100);
        touch(&root.join("split-dmx/a.dmx"), 1000);
        touch(&root.join("material-cache/k/manifest.json"), 50);
        touch(&root.join("stage-abc/x"), 5);
        touch(&root.join("output/pak18_dir.vpk"), 70);
        touch(&root.join("inspection.json"), 3);
        let mut project = Project::default();
        project.id = id.into();
        project.dmx = run.join("prepared-1/model.dmx").to_string_lossy().into_owned();
        let known = BTreeMap::from([(
            id.to_owned(),
            KnownProject {
                files: vec![],
                open: Some(project.clone()),
            },
        )]);
        let settings = settings(&workspace);
        let cancel = CancellationToken::new();
        let report = report(&settings, &known, &cancel).unwrap();
        assert_eq!(report.projects.len(), 1);
        let entry = &report.projects[0];
        assert!(entry.open);
        assert_eq!(entry.bytes, 100 + 1000 + 50 + 5 + 70 + 3);
        let build = entry.usage.iter().find(|u| u.kind == Kind::BuildData).unwrap();
        assert_eq!((build.bytes, build.reclaimable), (1050, 1050));
        let output = entry.usage.iter().find(|u| u.kind == Kind::Output).unwrap();
        assert_eq!(output.reclaimable, 0);

        let request = ClearRequest {
            project: None,
            kinds: vec![Kind::BuildData, Kind::Temporary, Kind::Preparation],
            remove_project: false,
            unused_shared: false,
        };
        let result = clear(&settings, &known, &request, &cancel).unwrap();
        assert_eq!(result.removed_bytes, 1055);
        assert!(run.join("prepared-1/model.dmx").is_file());
        assert!(root.join("output/pak18_dir.vpk").is_file());
        assert!(!root.join("split-dmx").exists());

        // The open project's workspace cannot be removed.
        let remove = ClearRequest {
            project: Some(id.into()),
            kinds: vec![],
            remove_project: true,
            unused_shared: false,
        };
        assert!(clear(&settings, &known, &remove, &cancel).is_err());
        // Removal reports the resolved long spelling; CI temp folders can be
        // 8.3 short names (C:\Users\RUNNER~1\...).
        let resolved = deadlock_project::asset_paths::resolve(&root).unwrap();
        let closed = BTreeMap::new();
        let result = clear(&settings, &closed, &remove, &cancel).unwrap();
        assert_eq!(result.removed, vec![resolved.to_string_lossy().into_owned()]);
        assert!(!root.exists());
    }

    #[test]
    fn locked_workspaces_are_left_alone() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path();
        let id = "fedcba9876543210fedcba9876543210";
        let root = workspace.join(id);
        touch(&root.join("split-dmx/a.dmx"), 10);
        touch(&root.join("build.lock"), 1);
        let settings = settings(workspace);
        let request = ClearRequest {
            project: Some(id.into()),
            kinds: vec![Kind::BuildData],
            remove_project: false,
            unused_shared: false,
        };
        let cancel = CancellationToken::new();
        let result = clear(&settings, &BTreeMap::new(), &request, &cancel).unwrap();
        assert!(result.removed.is_empty());
        assert!(root.join("split-dmx/a.dmx").is_file());
        let remove = ClearRequest {
            remove_project: true,
            kinds: vec![],
            ..request
        };
        assert!(clear(&settings, &BTreeMap::new(), &remove, &cancel).is_err());
    }

    #[test]
    fn compiler_folders_need_matching_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let csdk = temp.path().join("csdk");
        let id = "00112233445566778899aabbccddeeff";
        let other = "ffeeddccbbaa99887766554433221100";
        let mine = csdk.join("content/citadel_addons/dmb_0011223344556677");
        let theirs = csdk.join("game/citadel_addons/dmb_ffeeddccbbaa9988");
        let empty = csdk.join("game/citadel_addons/dmb_0011223344556677");
        let unowned = csdk.join("content/citadel_addons/dmb_aaaaaaaaaaaaaaaa");
        touch(&mine.join("models/x.dmx"), 10);
        fs::write(mine.join(".deadlock-builder.json"), format!("{{\"project\":\"{id}\"}}")).unwrap();
        touch(&theirs.join("models/x.vmdl_c"), 10);
        fs::write(theirs.join(".deadlock-builder.json"), format!("{{\"project\":\"{other}\"}}")).unwrap();
        fs::create_dir_all(empty.join("models")).unwrap();
        touch(&unowned.join("file"), 10);
        let workspace = temp.path().join("ws");
        fs::create_dir_all(workspace.join(id)).unwrap();
        let settings = Settings {
            workspace: workspace.to_string_lossy().into_owned(),
            csdk: csdk.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let scan = scan(&settings, &BTreeMap::new()).unwrap();
        let mut mine_found = scan.compiler[id].clone();
        mine_found.sort();
        let mut expected = vec![mine.clone(), empty.clone()];
        expected.sort();
        assert_eq!(mine_found, expected);
        assert_eq!(scan.compiler[other], vec![theirs.clone()]);
        assert!(scan.ids.contains(other));
        let request = ClearRequest {
            project: None,
            kinds: vec![Kind::Compiler],
            remove_project: false,
            unused_shared: false,
        };
        clear(&settings, &BTreeMap::new(), &request, &CancellationToken::new()).unwrap();
        assert!(!mine.exists() && !empty.exists() && !theirs.exists());
        assert!(unowned.join("file").is_file());
    }
}
