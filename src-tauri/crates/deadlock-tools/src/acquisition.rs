//! Pinned, metadata-first acquisition. Native compatibility is never inferred from downloaded files.
use deadlock_core::{
    process::{CommandSpec, ToolExecutor},
    safety::{self, digest_with_cancel, inside},
    text::casefold,
    CancellationToken, CoreError, Result, TimingRecorder,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};
const RECIPE: &str = include_str!("../../../../resources/tools/acquisition.json");
const BASE: [&str; 2] = ["2347770", "2347771"];
const TOOLS: &str = "2347779";
const PREFIXES: [&str; 7] = [
    "game/bin/",
    "game/core/",
    "game/csgo_core/",
    "game/csgo_imported/",
    "game/csgo/bin/",
    "game/csgo/cfg/",
    "game/csgo/scripts/",
];
pub const REQUIRED: [&str; 7] = [
    "game/bin/win64/resourcecompiler.exe",
    "game/bin/win64/resourcecompiler.dll",
    "game/bin/win64/modeldoc_utils.dll",
    "game/csgo/gameinfo.gi",
    "game/csgo_core/gameinfo.gi",
    "game/csgo_imported/gameinfo.gi",
    "game/core/gameinfo.gi",
];
fn fail(message: impl Into<String>) -> CoreError {
    CoreError::Integrity(message.into())
}
fn default_app() -> String {
    "730".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recipe {
    #[serde(default = "default_app")]
    pub app: String,
    pub build: String,
    pub queried_utc: String,
    pub depots: Vec<Depot>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Depot {
    pub depot: String,
    pub manifest: String,
    pub whole_depot_bytes: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub bytes: u64,
    pub sha1: String,
    pub directory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depot: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectedDepot {
    #[serde(flatten)]
    pub depot: Depot,
    pub selected_files: usize,
    pub selected_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AcquisitionPlan {
    pub schema: u32,
    pub kind: String,
    pub steam_build: String,
    pub branch_snapshot_utc: String,
    pub depots: Vec<SelectedDepot>,
    pub manifest_sha256: BTreeMap<String, String>,
    pub filelists: BTreeMap<String, Vec<String>>,
    pub transfer_uncompressed_bytes: u64,
    pub installed_bytes: u64,
    pub files: Vec<ManifestFile>,
    pub overridden_paths: BTreeMap<String, Vec<String>>,
    pub native_compile_passed: bool,
    pub absolute_minimum_proven: bool,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_estimate_note: Option<String>,
    pub estimated_download_bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IntegrityReceipt {
    pub steam_build: String,
    pub files_verified: usize,
    pub bytes_verified: u64,
    pub manifest_integrity_passed: bool,
    pub native_compile_passed: bool,
    pub absolute_minimum_proven: bool,
}
pub fn recipe() -> Result<Recipe> {
    Ok(serde_json::from_str(RECIPE)?)
}
pub fn recipe_sha256() -> String {
    format!("{:x}", Sha256::digest(RECIPE.as_bytes()))
}
pub fn safe_path(value: &str) -> Result<String> {
    if value.chars().any(|c| c < ' ') {
        return Err(CoreError::UnsafePath(value.into()));
    }
    safety::relative_asset(value)
}
fn number(text: &str) -> Result<u64> {
    text.parse()
        .map_err(|_| fail("Manifest integer is out of range"))
}
pub fn parse_manifest(
    path: &Path,
    depot: &str,
    manifest: &str,
    cancel: &CancellationToken,
) -> Result<Vec<ManifestFile>> {
    Ok(manifest_rows(path, depot, manifest, cancel)?
        .into_iter()
        .filter(|r| !r.directory)
        .collect())
}
/// Every row of a manifest listing, directories included.
fn manifest_rows(
    path: &Path,
    depot: &str,
    manifest: &str,
    cancel: &CancellationToken,
) -> Result<Vec<ManifestFile>> {
    let metadata = fs::metadata(path).map_err(|e| CoreError::io("Inspect pinned manifest", e))?;
    if metadata.len() > 256 * 1024 * 1024 {
        return Err(fail("Manifest listing exceeds supported size"));
    }
    let text = fs::read_to_string(path).map_err(|e| CoreError::io("Read pinned manifest", e))?;
    let text = text.trim_start_matches('\u{feff}');
    let capture = |pattern: &str| -> Result<String> {
        Regex::new(pattern)
            .expect("constant regex")
            .captures(text)
            .map(|c| c[1].to_owned())
            .ok_or_else(|| {
                fail(format!(
                    "Manifest identity/header mismatch: {}",
                    path.display()
                ))
            })
    };
    if capture(r"(?m)^Content Manifest for Depot (\d+)\s*$")? != depot
        || capture(r"(?m)^Manifest ID / date\s*:\s*(\d+)\s*/")? != manifest
    {
        return Err(fail(format!(
            "Manifest identity/header mismatch: {}",
            path.display()
        )));
    }
    let count = number(&capture(r"(?m)^Total number of files\s*:\s*(\d+)")?)?;
    let size = number(&capture(r"(?m)^Total bytes on disk\s*:\s*(\d+)")?)?;
    if count > 1_000_000 {
        return Err(fail("Manifest entry count exceeds supported size"));
    }
    let row = Regex::new(r"^\s*(\d+)\s+(\d+)\s+([a-fA-F0-9]{40})\s+([a-fA-F0-9]+)\s+(.+?)\s*$")
        .expect("constant regex");
    let suspicious = Regex::new(r"^\s*\d+\s+\d+\s+").expect("constant regex");
    let mut rows = vec![];
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for line in text.lines() {
        cancel.check()?;
        let Some(values) = row.captures(line) else {
            if suspicious.is_match(line) {
                return Err(fail(format!("Unparsed manifest row: {}", path.display())));
            }
            continue;
        };
        let flags =
            u64::from_str_radix(&values[4], 16).map_err(|_| fail("Invalid manifest flags"))?;
        let path = safe_path(&values[5])?;
        if flags & 512 != 0 {
            return Err(fail(format!(
                "Linked depot entries need separate review: {path}"
            )));
        }
        if !seen.insert(casefold(&path)) {
            return Err(fail(format!(
                "Duplicate/case-colliding manifest entry: {path}"
            )));
        }
        let directory = flags & 64 != 0;
        let bytes = number(&values[1])?;
        if !directory {
            total = total
                .checked_add(bytes)
                .ok_or_else(|| fail("Manifest byte sum overflow"))?;
        }
        rows.push(ManifestFile {
            path,
            bytes,
            sha1: values[3].to_ascii_lowercase(),
            directory,
            depot: None,
        });
    }
    if rows.len() as u64 != count {
        return Err(fail(format!(
            "Incomplete/unrecognized manifest listing: {}",
            path.display()
        )));
    }
    if total != size {
        return Err(fail(format!(
            "Manifest byte total mismatch: {}",
            path.display()
        )));
    }
    Ok(rows)
}
pub fn selected(depot: &str, path: &str) -> Result<bool> {
    if depot == TOOLS {
        return Ok(true);
    }
    if !BASE.contains(&depot) {
        return Err(fail("Unexpected depot"));
    }
    let path = path.to_lowercase();
    if PREFIXES.iter().any(|p| path.starts_with(p))
        || ["game/thirdpartylegalnotices.txt", "game/csgo/steam.inf"].contains(&path.as_str())
    {
        return Ok(true);
    }
    let (parent, name) = path.rsplit_once('/').unwrap_or(("", &path));
    Ok(parent == "game/csgo"
        && ((name.starts_with("gameinfo") && name.ends_with(".gi"))
            || (name.starts_with("shaders_") && name.ends_with(".vpk"))))
}
pub fn plan(
    recipe: &Recipe,
    manifests: &Path,
    cancel: &CancellationToken,
) -> Result<AcquisitionPlan> {
    let wanted: BTreeSet<_> = BASE.into_iter().chain([TOOLS]).collect();
    let actual: BTreeSet<_> = recipe.depots.iter().map(|d| d.depot.as_str()).collect();
    if actual != wanted || recipe.depots.len() != 3 {
        return Err(fail("Recipe must contain the reviewed three depots."));
    }
    let mut effective: BTreeMap<String, ManifestFile> = BTreeMap::new();
    let mut origins: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut filelists = BTreeMap::new();
    let mut manifest_sha256 = BTreeMap::new();
    let mut depots = vec![];
    let mut transfer = 0u64;
    for depot in &recipe.depots {
        cancel.check()?;
        let name = format!("manifest_{}_{}.txt", depot.depot, depot.manifest);
        let path = manifests.join(&name);
        if !path.is_file() {
            return Err(fail(format!("Missing authorized manifest for depot {}. No file download plan is permitted. Check Steam ownership/Workshop Tools access or manifest availability.",depot.depot)));
        }
        no_links(&path)?;
        let rows = parse_manifest(&path, &depot.depot, &depot.manifest, cancel)?;
        if rows
            .iter()
            .try_fold(0u64, |sum, r| sum.checked_add(r.bytes))
            .ok_or_else(|| fail("Manifest byte sum overflow"))?
            != depot.whole_depot_bytes
        {
            return Err(fail(format!(
                "Manifest does not match the pinned Steam snapshot size: {}",
                depot.depot
            )));
        }
        let chosen: Vec<_> = rows
            .into_iter()
            .filter_map(|item| match selected(&depot.depot, &item.path) {
                Ok(true) => Some(Ok(item)),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<_>>()?;
        if chosen.is_empty() {
            return Err(fail(format!(
                "Empty selection for required depot {}",
                depot.depot
            )));
        }
        let bytes = chosen
            .iter()
            .try_fold(0u64, |sum, r| sum.checked_add(r.bytes))
            .ok_or_else(|| fail("Selected byte sum overflow"))?;
        filelists.insert(
            depot.depot.clone(),
            chosen.iter().map(|r| r.path.clone()).collect(),
        );
        manifest_sha256.insert(name, digest_with_cancel(&path, cancel)?);
        depots.push(SelectedDepot {
            depot: depot.clone(),
            selected_files: chosen.len(),
            selected_bytes: bytes,
        });
        transfer = transfer
            .checked_add(bytes)
            .ok_or_else(|| fail("Transfer byte sum overflow"))?;
        for mut item in chosen {
            let key = casefold(&item.path);
            if effective.get(&key).is_some_and(|old| old.path != item.path) {
                return Err(fail(format!("Cross-depot case collision: {}", item.path)));
            }
            item.depot = Some(depot.depot.clone());
            effective.insert(key.clone(), item);
            origins.entry(key).or_default().push(depot.depot.clone());
        }
    }
    let missing: Vec<_> = REQUIRED
        .iter()
        .filter(|p| !effective.contains_key(&casefold(p)))
        .collect();
    if !missing.is_empty() {
        return Err(fail(format!(
            "Required tool/config missing from selection: {}",
            missing.iter().map(|p| **p).collect::<Vec<_>>().join(", ")
        )));
    }
    let installed = effective
        .values()
        .try_fold(0u64, |sum, r| sum.checked_add(r.bytes))
        .ok_or_else(|| fail("Installed byte sum overflow"))?;
    let overridden_paths = origins
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, v)| (effective[&k].path.clone(), v))
        .collect();
    Ok(AcquisitionPlan{schema:1,kind:"unverified_native_toolset_candidate".into(),steam_build:recipe.build.clone(),branch_snapshot_utc:recipe.queried_utc.clone(),depots,manifest_sha256,filelists,transfer_uncompressed_bytes:transfer,installed_bytes:installed,files:effective.into_values().collect(),overridden_paths,native_compile_passed:false,absolute_minimum_proven:false,scope:"Whole tool depots plus complete runtime/core/config directories. Excludes bulk CS2 game paks, maps and community addons from base depots. Runtime sufficiency must be tested on Windows.".into(),destination:None,download_estimate_note:None,estimated_download_bytes:None})
}
pub fn no_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                let linked = metadata.file_type().is_symlink();
                #[cfg(windows)]
                let linked = {
                    use std::os::windows::fs::MetadataExt;
                    linked || metadata.file_attributes() & 0x400 != 0
                };
                if linked {
                    return Err(fail(format!("Linked path refused: {}", ancestor.display())));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(CoreError::io("Inspect path links", e)),
        }
    }
    Ok(())
}
fn sha1_file(path: &Path, cancel: &CancellationToken) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| CoreError::io("Open acquired file", e))?;
    let mut hash = Sha1::new();
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        cancel.check()?;
        let count = file
            .read(&mut bytes)
            .map_err(|e| CoreError::io("Hash acquired file", e))?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn validate_files(
    plan: &AcquisitionPlan,
    toolroot: &Path,
    cancel: &CancellationToken,
) -> Result<IntegrityReceipt> {
    no_links(toolroot)?;
    let expected: HashSet<_> = plan.files.iter().map(|p| casefold(&p.path)).collect();
    let mut failures = vec![];
    fn scan(
        root: &Path,
        path: &Path,
        expected: &HashSet<String>,
        failures: &mut Vec<String>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        for entry in fs::read_dir(path).map_err(|e| CoreError::io("Inspect acquired toolset", e))? {
            cancel.check()?;
            let entry = entry.map_err(|e| CoreError::io("Read acquired tool entry", e))?;
            let path = entry.path();
            no_links(&path)?;
            let kind = entry
                .file_type()
                .map_err(|e| CoreError::io("Inspect acquired tool entry", e))?;
            if kind.is_dir() {
                scan(root, &path, expected, failures, cancel)?;
            } else if kind.is_file() {
                let extension = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let name = path
                    .strip_prefix(root)
                    .map_err(|_| CoreError::PathEscape(path.clone()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if matches!(extension.as_str(), "dll" | "exe")
                    && !expected.contains(&casefold(&name))
                {
                    failures.push(format!("{name}: unplanned executable/library"));
                }
            }
        }
        Ok(())
    }
    scan(toolroot, toolroot, &expected, &mut failures, cancel)?;
    for item in &plan.files {
        cancel.check()?;
        let path = inside(toolroot, &item.path)?;
        no_links(&path)?;
        let placeholder = item.bytes == 0 && item.sha1 == "0".repeat(40);
        if !path.is_file()
            || fs::metadata(&path)
                .map_err(|e| CoreError::io("Inspect acquired file", e))?
                .len()
                != item.bytes
            || (!placeholder && sha1_file(&path, cancel)? != item.sha1)
        {
            failures.push(format!("{}: missing/size/SHA-1 mismatch", item.path));
        }
    }
    if !failures.is_empty() {
        return Err(fail(format!(
            "Acquisition verification failed:\n{}",
            failures.into_iter().take(30).collect::<Vec<_>>().join("\n")
        )));
    }
    Ok(IntegrityReceipt {
        steam_build: plan.steam_build.clone(),
        files_verified: plan.files.len(),
        bytes_verified: plan.installed_bytes,
        manifest_integrity_passed: true,
        native_compile_passed: false,
        absolute_minimum_proven: false,
    })
}
#[derive(Clone, Debug)]
pub enum Auth {
    Qr,
    Username(String),
    Anonymous,
}
impl Auth {
    pub fn args(&self) -> Result<Vec<String>> {
        match self {
            Self::Qr => Ok(vec!["-qr".into()]),
            Self::Anonymous => Ok(vec![]),
            Self::Username(name) => {
                if name.trim().is_empty() || name.starts_with('-') {
                    return Err(fail("Username authentication requires an account name."));
                }
                Ok(vec![
                    "-username".into(),
                    name.clone(),
                    "-remember-password".into(),
                ])
            }
        }
    }
}
pub fn metadata_command(
    dd: &Path,
    recipe: &Recipe,
    destination: &Path,
    auth: &Auth,
) -> Result<CommandSpec> {
    let mut args = vec!["-app".into(), recipe.app.clone(), "-depot".into()];
    args.extend(recipe.depots.iter().map(|d| d.depot.clone()));
    args.push("-manifest".into());
    args.extend(recipe.depots.iter().map(|d| d.manifest.clone()));
    args.extend(["-os", "windows", "-osarch", "64", "-manifest-only", "-dir"].map(str::to_owned));
    let mut spec = CommandSpec::new(dd).args(args).arg(destination);
    spec = spec.args(auth.args()?).args(["-loginid", "17843291"]);
    Ok(spec)
}
/// A one-time Steam sign-in for a username: run in a console window the user
/// types into, DepotDownloader asks for the password and the Steam Guard code
/// and, with -remember-password, keeps the sign-in, so the app's later runs
/// (no console, no input) need no password. Fetches one pinned manifest into
/// `destination`.
pub fn sign_in_command(dd: &Path, destination: &Path, username: &str) -> Result<CommandSpec> {
    let recipe = recipe()?;
    let depot = recipe.depots.first().cloned().ok_or_else(|| fail("The pinned tool recipe has no depot."))?;
    let pinned = Recipe { depots: vec![depot], ..recipe };
    metadata_command(dd, &pinned, destination, &Auth::Username(username.into()))
}
pub fn download_command(
    dd: &Path,
    recipe: &Recipe,
    depot: &Depot,
    filelist: &Path,
    destination: &Path,
    auth: &Auth,
) -> Result<CommandSpec> {
    depots_download_command(dd, recipe, std::slice::from_ref(depot), filelist, destination, auth)
}
/// Downloads `depots` in one DepotDownloader run (one Steam sign-in, so one
/// QR scan) into one folder, every depot filtered by the same filelist. Where
/// depots share a file name, DepotDownloader keeps the last depot's file.
pub fn depots_download_command(
    dd: &Path,
    recipe: &Recipe,
    depots: &[Depot],
    filelist: &Path,
    destination: &Path,
    auth: &Auth,
) -> Result<CommandSpec> {
    if depots.is_empty() {
        return Err(fail("No depot to download."));
    }
    // DepotDownloader only warns about a filelist it cannot read and then downloads everything.
    if !filelist.is_file()
        || fs::metadata(filelist)
            .map_err(|e| CoreError::io("Inspect acquisition filelist", e))?
            .len()
            == 0
    {
        return Err(fail(
            "Required filelist is missing/empty; refusing an unfiltered download.",
        ));
    }
    let mut args = vec!["-app".into(), recipe.app.clone(), "-depot".into()];
    args.extend(depots.iter().map(|d| d.depot.clone()));
    args.push("-manifest".into());
    args.extend(depots.iter().map(|d| d.manifest.clone()));
    args.extend(["-os", "windows", "-osarch", "64", "-filelist"].map(str::to_owned));
    Ok(CommandSpec::new(dd)
        .args(args)
        .arg(filelist)
        .arg("-dir")
        .arg(destination)
        .arg("-validate")
        .args(auth.args()?)
        .args(["-loginid", "17843291"]))
}
/// Whether .NET's OrdinalIgnoreCase (DepotDownloader's filelist match) may
/// call `a` and `b` equal: as many UTF-16 units and the same ASCII characters
/// ignoring case. Other characters follow .NET's own case tables, so any pair
/// of them counts as a possible match.
fn loosely_equal(a: &str, b: &str) -> bool {
    a.encode_utf16().count() == b.encode_utf16().count()
        && a.encode_utf16().zip(b.encode_utf16()).all(|(x, y)| {
            x >= 0x80 || y >= 0x80 || (x as u8).eq_ignore_ascii_case(&(y as u8))
        })
}
/// The filelist for downloading every depot of `plan` in one DepotDownloader
/// run, or `None` when one run would not write exactly the planned files.
///
/// DepotDownloader 3.4 matches the filelist against every depot without regard
/// to case and, for depots sharing a folder, keeps the last depot's file (in
/// the order given, which is the plan's override order). So a file one
/// depot's selection leaves out still comes from that depot when another
/// depot selects the same name. Over the full manifests the plan approved,
/// every name the filelist can match must be a planned file, spelled as
/// planned and not a directory, and the last depot that has it must be the
/// planned one with the planned bytes.
pub fn single_run_filelist(
    plan: &AcquisitionPlan,
    manifests: &Path,
    cancel: &CancellationToken,
) -> Result<Option<Vec<String>>> {
    let planned: HashMap<String, &ManifestFile> =
        plan.files.iter().map(|f| (casefold(&f.path), f)).collect();
    let mut by_length: HashMap<usize, Vec<&str>> = HashMap::new();
    for file in &plan.files {
        by_length
            .entry(file.path.encode_utf16().count())
            .or_default()
            .push(&file.path);
    }
    let unusual = plan.files.iter().any(|f| !f.path.is_ascii());
    let mut written: HashMap<String, (&str, ManifestFile)> = HashMap::new();
    for selected in &plan.depots {
        let depot = &selected.depot;
        let name = format!("manifest_{}_{}.txt", depot.depot, depot.manifest);
        let path = manifests.join(&name);
        no_links(&path)?;
        if plan.manifest_sha256.get(&name) != Some(&digest_with_cancel(&path, cancel)?) {
            return Err(fail(format!(
                "Manifest {name} changed after planning. Click Fetch manifests and plan again."
            )));
        }
        for row in manifest_rows(&path, &depot.depot, &depot.manifest, cancel)? {
            let key = casefold(&row.path);
            // Names with other characters may match an entry casefold does not.
            let unclear = (unusual || !row.path.is_ascii())
                && by_length
                    .get(&row.path.encode_utf16().count())
                    .is_some_and(|names| {
                        names.iter().any(|n| *n != row.path && loosely_equal(n, &row.path))
                    });
            match planned.get(&key) {
                None if !unclear => {}
                Some(file) if !unclear && file.path == row.path && !row.directory => {
                    written.insert(key, (depot.depot.as_str(), row));
                }
                _ => return Ok(None),
            }
        }
    }
    let exact = written.len() == plan.files.len()
        && plan.files.iter().all(|f| {
            written.get(&casefold(&f.path)).is_some_and(|(depot, row)| {
                f.depot.as_deref() == Some(*depot) && row.bytes == f.bytes && row.sha1 == f.sha1
            })
        });
    Ok(exact.then(|| plan.files.iter().map(|f| f.path.clone()).collect()))
}
/// The DepotDownloader runs that download `plan` into `destination`, each
/// with its step name: one run for every depot when that writes exactly the
/// planned files (one Steam sign-in, so one QR scan instead of one per depot;
/// its filelist is written to `lists`), else one run per depot with the
/// depot's own filelist from `lists`.
pub fn download_commands(
    dd: &Path,
    recipe: &Recipe,
    plan: &AcquisitionPlan,
    manifests: &Path,
    lists: &Path,
    destination: &Path,
    auth: &Auth,
    cancel: &CancellationToken,
) -> Result<Vec<(String, CommandSpec)>> {
    if let Some(names) = single_run_filelist(plan, manifests, cancel)? {
        let filelist = lists.join("all-depots.txt");
        no_links(&filelist)?;
        safety::atomic_write(&filelist, format!("{}\n", names.join("\n")))?;
        let depots: Vec<Depot> = plan.depots.iter().map(|d| d.depot.clone()).collect();
        let ids: Vec<&str> = depots.iter().map(|d| d.depot.as_str()).collect();
        return Ok(vec![(
            format!("Download depots {}", ids.join(", ")),
            depots_download_command(dd, recipe, &depots, &filelist, destination, auth)?,
        )]);
    }
    plan.depots
        .iter()
        .map(|d| {
            let filelist = lists.join(format!("{}.txt", d.depot.depot));
            Ok((
                format!("Download depot {}", d.depot.depot),
                download_command(dd, recipe, &d.depot, &filelist, destination, auth)?,
            ))
        })
        .collect()
}
pub fn owned(root: &Path, recipe: &Recipe, create: bool) -> Result<PathBuf> {
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| CoreError::io("Read current directory", e))?
            .join(root)
    };
    no_links(&root)?;
    let marker = root.join("acquisition-owner.json");
    let expected = serde_json::json!({"kind":"dmb-tool-acquisition","build":recipe.build,"recipe_sha256":recipe_sha256()});
    if !root.exists() {
        if !create {
            return Err(fail(format!("Nothing has been downloaded into {} yet. Click Fetch manifests and plan, approve the Steam sign-in, then click Download. Verify existing acquisition is only for a folder that already holds a finished download (it contains acquisition-owner.json).", root.display())));
        }
        fs::create_dir_all(&root).map_err(|e| CoreError::io("Create acquisition directory", e))?;
        safety::write_json(&marker, &expected)?;
    } else if !marker.is_file() {
        if !create
            || fs::read_dir(&root)
                .map_err(|e| CoreError::io("Inspect acquisition directory", e))?
                .next()
                .is_some()
        {
            return Err(fail(
                "Destination is not an owned fresh acquisition folder. Choose a new empty folder.",
            ));
        }
        safety::write_json(&marker, &expected)?;
    }
    no_links(&marker)?;
    let actual: serde_json::Value = serde_json::from_slice(
        &fs::read(marker).map_err(|e| CoreError::io("Read acquisition ownership", e))?,
    )?;
    if actual != expected {
        return Err(fail(
            "Acquisition recipe/build differs. Choose a new destination.",
        ));
    }
    Ok(root)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreparedPlan {
    pub plan: AcquisitionPlan,
    pub plan_text: String,
    pub plan_digest: String,
}
pub fn prepared_plan(
    recipe: &Recipe,
    manifests: &Path,
    toolroot: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedPlan> {
    let mut plan = plan(recipe, manifests, cancel)?;
    plan.destination = Some(toolroot.into());
    plan.download_estimate_note=Some("Compressed network bytes unknown; transfer_uncompressed_bytes is an unpacked selection estimate.".into());
    let text = format!("{}\n", serde_json::to_string_pretty(&plan)?);
    let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
    Ok(PreparedPlan {
        plan,
        plan_text: text,
        plan_digest: digest,
    })
}
pub fn write_plan_files(root: &Path, prepared: &PreparedPlan) -> Result<PathBuf> {
    let lists = root.join("filelists");
    no_links(&lists)?;
    for (depot, names) in &prepared.plan.filelists {
        if !depot.bytes().all(|b| b.is_ascii_digit()) {
            return Err(fail("Invalid depot ID"));
        }
        safety::atomic_write(
            &lists.join(format!("{depot}.txt")),
            format!("{}\n", names.join("\n")),
        )?;
    }
    safety::atomic_write(&root.join("acquisition-plan.json"), &prepared.plan_text)?;
    Ok(lists)
}
#[derive(Clone, Copy, Debug)]
pub enum Action {
    Manifests,
    Download,
    Verify,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AcquisitionResult {
    pub plan: AcquisitionPlan,
    pub plan_digest: String,
    pub downloaded: bool,
    pub receipt: Option<IntegrityReceipt>,
}
pub fn run_acquisition(
    action: Action,
    root: &Path,
    dd: Option<&Path>,
    auth: &Auth,
    approved_plan_digest: Option<&str>,
    runner: &dyn ToolExecutor,
    timing: &TimingRecorder,
    parent: Option<&str>,
    cancel: &CancellationToken,
) -> Result<AcquisitionResult> {
    timing.measure("Acquire model tools",parent,|span|{
        let recipe=recipe()?;
        // The program, its folder or the folder it was extracted into.
        let resolved=if matches!(action,Action::Verify){None}else{Some(crate::setup_checks::depotdownloader(dd.unwrap_or(Path::new(""))).map_err(fail)?)};
        let dd=resolved.as_deref().or(dd);auth.args()?;
        let root=owned(root,&recipe,matches!(action,Action::Manifests))?;let manifests=root.join("manifests");let toolroot=root.join("toolset");let receipt_path=root.join("acquisition-verified.json");
        match fs::remove_file(&receipt_path){Ok(())=>{},Err(e)if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(CoreError::io("Remove stale acquisition receipt",e))}
        if matches!(action,Action::Manifests){no_links(&manifests)?;fs::create_dir_all(&manifests).map_err(|e|CoreError::io("Create manifest directory",e))?;timing.measure("Fetch pinned manifests",Some(span.id()),|_|runner.run(&metadata_command(dd.expect("validated"),&recipe,&manifests,auth)?.current_dir(&root)))?;}
        let prepared=timing.measure("Plan pinned tool files",Some(span.id()),|_|prepared_plan(&recipe,&manifests,&toolroot,cancel))?;
        if matches!(action,Action::Download)&&approved_plan_digest!=Some(prepared.plan_digest.as_str()){return Err(fail("Download requires confirmation of this exact preflight plan. Fetch manifests, review the selection, then download."));}
        let lists=write_plan_files(&root,&prepared)?;
        if matches!(action,Action::Manifests){return Ok(AcquisitionResult{plan:prepared.plan,plan_digest:prepared.plan_digest,downloaded:false,receipt:None});}
        if matches!(action,Action::Download){no_links(&toolroot)?;fs::create_dir_all(&toolroot).map_err(|e|CoreError::io("Create selected toolset directory",e))?;for (step,command) in download_commands(dd.expect("validated"),&recipe,&prepared.plan,&manifests,&lists,&toolroot,auth,cancel)?{cancel.check()?;timing.measure(step,Some(span.id()),|_|runner.run(&command.current_dir(&root)))?;}}
        let receipt=timing.measure("Verify selected tool files",Some(span.id()),|_|validate_files(&prepared.plan,&toolroot,cancel))?;safety::write_json(&receipt_path,&receipt)?;
        Ok(AcquisitionResult{plan:prepared.plan,plan_digest:prepared.plan_digest,downloaded:matches!(action,Action::Download),receipt:Some(receipt)})
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filelist_names_match_like_dotnet_or_count_as_unclear() {
        assert!(loosely_equal("game/bin/A.dll", "GAME/BIN/a.DLL"));
        assert!(!loosely_equal("game/bin/a.dll", "game/bin/b.dll"));
        assert!(!loosely_equal("game/bin/a.dll", "game/bin/a.dl"));
        // Other characters follow .NET's case tables: a dotless i may be an I.
        assert!(loosely_equal("game/b\u{131}n/a.dll", "game/BIN/a.dll"));
        assert!(loosely_equal("game/\u{e4}.dll", "game/a.dll"));
        // UTF-16 units, not bytes or characters.
        assert!(!loosely_equal("game/\u{1F600}.dll", "game/a.dll"));
        assert!(loosely_equal("game/\u{1F600}.dll", "game/ab.dll"));
    }
}
