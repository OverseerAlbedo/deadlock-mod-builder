//! Offline, literal command generation. These functions never launch DepotDownloader.
use crate::acquisition::{self, AcquisitionPlan, Auth, Recipe};
use deadlock_core::{safety, CancellationToken, CoreError, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Options {
    pub dd: PathBuf,
    pub toolroot: PathBuf,
    pub manifests: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Commands {
    pub metadata: Vec<String>,
    pub downloads: Vec<Vec<String>>,
    /// The same commands as PowerShell lines to paste.
    #[serde(default)]
    pub metadata_command: String,
    #[serde(default)]
    pub download_commands: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preview {
    pub guide: String,
    pub commands: String,
    pub argv: BTreeMap<String, Commands>,
    pub plan: serde_json::Value,
    pub plan_digest: Option<String>,
    pub expected_files: String,
    pub export_dir: Option<PathBuf>,
    pub compatible: bool,
    pub downloaded: bool,
    /// The pinned depots in full, and the selected files once a plan exists.
    #[serde(default)]
    pub whole_bytes: u64,
    #[serde(default)]
    pub selected_bytes: Option<u64>,
}
fn fail(message: impl Into<String>) -> CoreError {
    CoreError::Integrity(message.into())
}
/// A PowerShell line running `argv`. The program and any value PowerShell
/// would read specially are single-quoted; flags, numbers and plain paths stay
/// bare so the command is readable.
pub fn powershell(argv: &[String]) -> Result<String> {
    let plain = |v: &str| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:\\/".contains(c));
    let mut quoted = vec![];
    for (index, value) in argv.iter().enumerate() {
        if value.chars().any(|c| c < ' ') {
            return Err(fail("Commands cannot contain control characters."));
        }
        quoted.push(if index > 0 && plain(value) { value.clone() } else { format!("'{}'", value.replace('\'', "''")) });
    }
    Ok(format!("& {}", quoted.join(" ")))
}
fn argv(program: &Path, args: &[OsString]) -> Vec<String> {
    std::iter::once(program.to_string_lossy().into_owned())
        .chain(args.iter().map(|s| s.to_string_lossy().into_owned()))
        .collect()
}
pub fn defaults(recipe: &Recipe) -> Options {
    Options {
        dd: PathBuf::from(r"C:\Tools\DepotDownloader\DepotDownloader.exe"),
        toolroot: PathBuf::from(format!(r"C:\DMBTools\{}\toolset", recipe.build)),
        manifests: PathBuf::from(format!(r"C:\DMBTools\{}\manifests", recipe.build)),
    }
}
pub fn policy_text(recipe: &Recipe) -> Result<String> {
    let mut lines=vec!["Selection is case-insensitive for filtered base depots. Directory entries are not payload files.".into()];
    for depot in &recipe.depots {
        match depot.depot.as_str() {
            "2347779" => lines.push(format!(
                "Depot {}: include every regular file in the pinned Tools manifest (whole depot).",
                depot.depot
            )),
            "2347770" | "2347771" => {
                lines.push(format!("Depot {}: include complete game/bin/, game/core/, game/csgo_core/, game/csgo_imported/, game/csgo/bin/, game/csgo/cfg/, game/csgo/scripts/ directories; game/thirdpartylegalnotices.txt, game/csgo/steam.inf; and direct game/csgo/ gameinfo*.gi and shaders_*.vpk families. Exclude other files, including bulk game paks/maps/community addons.",depot.depot));
            }
            _ => return Err(fail("Unreviewed depot selection")),
        }
    }
    lines.extend(["Each filelist contains literal selected paths, one per line; no regular expressions.","Run depots in the listed order. Later depots replace the same path from earlier depots.","Expected-files.tsv records the final effective size, SHA-1 and owning depot; the plan records all overrides."].map(str::to_owned));
    Ok(lines.join("\n"))
}
/// The commands for each sign-in. With an exported plan, the download is the
/// same as the app's own: one run for all depots when that run writes exactly
/// the planned files (`acquisition::download_commands`), else one per depot.
pub fn command_sets(
    recipe: &Recipe,
    options: &Options,
    plan: Option<&AcquisitionPlan>,
    export_dir: Option<&Path>,
    cancel: &CancellationToken,
) -> Result<BTreeMap<String, Commands>> {
    let mut result = BTreeMap::new();
    for (name, auth) in [
        ("qr", Auth::Qr),
        ("username", Auth::Username("YOUR_STEAM_USERNAME".into())),
    ] {
        let metadata =
            acquisition::metadata_command(&options.dd, recipe, &options.manifests, &auth)?;
        let mut downloads = vec![];
        if let Some(plan) = plan {
            let export = export_dir
                .ok_or_else(|| fail("Export exact filelists before showing payload commands."))?;
            for (_, command) in acquisition::download_commands(
                &options.dd,
                recipe,
                plan,
                &options.manifests,
                &export.join("filelists"),
                &options.toolroot,
                &auth,
                cancel,
            )? {
                downloads.push(argv(&command.program, &command.args));
            }
        }
        let metadata = argv(&metadata.program, &metadata.args);
        result.insert(
            name.into(),
            Commands {
                metadata_command: powershell(&metadata)?,
                download_commands: downloads.iter().map(|d| powershell(d)).collect::<Result<_>>()?,
                metadata,
                downloads,
            },
        );
    }
    Ok(result)
}
/// How each sign-in works, in the words the guide and commands.txt use.
fn sign_in(mode: &str) -> (&'static str, &'static str) {
    if mode == "username" {
        ("Sign in with your Steam username", "Replace YOUR_STEAM_USERNAME with the account name you sign in to Steam with. DepotDownloader asks for your password and Steam Guard code in the PowerShell window; type them there, never into a command. -remember-password keeps the sign-in, so later commands do not ask again.")
    } else {
        ("Sign in with a QR code", "DepotDownloader draws a QR code in the PowerShell window. Scan it with the Steam app on your phone (Steam Guard → Scan a QR code). Every command shows a new code.")
    }
}
/// The depots a download command is for.
fn depots_of(argv: &[String]) -> String {
    let depots: Vec<&str> = argv.iter().skip_while(|a| *a != "-depot").skip(1).take_while(|a| !a.starts_with('-')).map(String::as_str).collect();
    depots.join(", ")
}
/// The heading of one download command.
fn download_label(index: usize, count: usize, argv: &[String]) -> String {
    if count == 1 {
        format!("One command for depots {}", depots_of(argv))
    } else {
        format!("Part {} of {count} (depot {})", index + 1, depots_of(argv))
    }
}
/// Why the download commands only exist after Export exact file plan.
fn downloads_later(whole_bytes: u64) -> String {
    format!("The download commands are made in step 3, so they are not here yet. Each one needs a file list that picks only the files the model compiler needs (a few GB instead of all {} of the three depots), and the app makes those lists from the files you get in step 2.", size(whole_bytes))
}
fn whole_bytes(recipe: &Recipe) -> u64 {
    recipe.depots.iter().map(|d| d.whole_depot_bytes).sum()
}
/// commands.txt: every command, grouped by sign-in.
pub fn commands_text(commands: &BTreeMap<String, Commands>, recipe: &Recipe) -> Result<String> {
    let mut lines = vec![
        "COMMANDS FOR THE MODEL TOOLSET DOWNLOAD".into(),
        "Paste one command at a time into PowerShell and press Enter. Wait until it finishes before the next one. Use the commands of one sign-in throughout.".into(),
    ];
    for (mode, commands) in commands {
        let (title, how) = sign_in(mode);
        lines.push(format!("{}\n{how}", title.to_uppercase()));
        lines.push(format!("Get the file lists:\n{}", powershell(&commands.metadata)?));
        if commands.downloads.is_empty() {
            lines.push(format!("Download the tools:\n{}", downloads_later(whole_bytes(recipe))));
        }
        let count = commands.downloads.len();
        for (index, args) in commands.downloads.iter().enumerate() {
            lines.push(format!("Download the tools. {}:\n{}", download_label(index, count, args), powershell(args)?));
        }
    }
    Ok(lines.join("\n\n") + "\n")
}
/// A size in the units File Explorer shows.
fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.0} MB", b as f64 / (1u64 << 20) as f64),
        b => format!("{} KB", b.div_ceil(1024)),
    }
}
/// What each pinned depot contributes, in plain words.
fn depot_text(depot: &acquisition::Depot) -> Result<String> {
    let what = match depot.depot.as_str() {
        "2347771" => "the same folders from Counter-Strike 2's second base depot.",
        "2347770" => r"Counter-Strike 2 base files, only the folders the compiler needs (game\bin, game\core, game\csgo_core, game\csgo_imported, game\csgo\bin, cfg and scripts, the gameinfo*.gi files and the shaders_*.vpk files). No maps, game archives or addons.",
        "2347779" => "Counter-Strike 2 Workshop Tools, complete.",
        _ => return Err(fail("Unreviewed depot selection")),
    };
    Ok(format!("- Depot {} ({} in full): {what}", depot.depot, size(depot.whole_depot_bytes)))
}
/// The guide as plain text (instructions.txt). The app shows the same steps
/// as a page (ManualGuide.tsx); keep the two in step.
fn guide(
    recipe: &Recipe,
    options: &Options,
    commands: &BTreeMap<String, Commands>,
    plan: Option<&AcquisitionPlan>,
) -> Result<String> {
    let indent = |text: &str| text.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n");
    let mut fetch = vec![];
    let mut download = vec![];
    for (mode, commands) in commands {
        let (title, how) = sign_in(mode);
        fetch.push(format!("{title}. {how}\n{}", indent(&commands.metadata_command)));
        if !commands.download_commands.is_empty() {
            let count = commands.download_commands.len();
            let parts = commands.downloads.iter().zip(&commands.download_commands).enumerate()
                .map(|(i, (args, text))| format!("{}:\n{}", download_label(i, count, args), indent(text)))
                .collect::<Vec<_>>()
                .join("\n");
            download.push(format!("{title}:\n{parts}"));
        }
    }
    let download = match plan {
        Some(p) if !download.is_empty() => format!(
            "{} ({} in total, {} files). If a command stops with an error, run it again; it continues where it stopped. Never remove -filelist: without it DepotDownloader downloads the whole depot.\n\n{}",
            if commands.values().all(|c| c.downloads.len() == 1) { "Run the command for your sign-in and let it finish" } else { "Run these one after another and let each one finish" },
            size(p.installed_bytes),
            p.files.len(),
            download.join("\n\n")
        ),
        _ => downloads_later(whole_bytes(recipe)),
    };
    let depots = recipe.depots.iter().map(depot_text).collect::<Result<Vec<_>>>()?.join("\n");
    let snapshot = recipe.queried_utc.split('T').next().unwrap_or(&recipe.queried_utc);
    Ok(format!(
        r#"DOWNLOAD THE MODEL TOOLSET YOURSELF

Use this if you would rather run DepotDownloader in PowerShell yourself, where you see its sign-in prompts. The app can also do all of it for you: Fetch manifests and plan, then Download, at the top of the Model tool setup page. Nothing has run yet.

YOU NEED
- DepotDownloader, extracted to a folder: https://github.com/SteamRE/DepotDownloader/releases
- A Steam account with Counter-Strike 2 in its library. It is free and does not have to be installed.
- New, empty folders, outside Deadlock, the Community SDK and any Counter-Strike installation:
    DepotDownloader:  {dd}
    File lists:       {manifests}
    Toolset:          {toolroot}

STEP 1. OPEN POWERSHELL
Start menu → type PowerShell → Enter.

STEP 2. GET THE FILE LISTS
Paste the command for your sign-in and press Enter. It downloads only lists of file names (under 1 MB), not the tools.

{fetch}

STEP 3. MAKE YOUR DOWNLOAD PLAN
In the app, under Or use files you already have, set:
    Manifest listings folder:  the file lists folder above
    Existing toolset folder:   the toolset folder above (it can still be empty)
    Manual plan export folder: any empty folder
Then click Export exact file plan.

STEP 4. DOWNLOAD THE TOOLS
{download}

STEP 5. CHECK AND USE THE TOOLSET
In the app, click Verify file integrity (every file's size and SHA-1 must match the plan), then Use this toolset in settings. Optional: Test original hero compatibility compiles your hero's original model once with the new toolset.

WHAT GETS DOWNLOADED
Counter-Strike 2 (Steam app {app}), build {build} from {snapshot}. The versions are pinned; no command picks a newer one.
{depots}
Files in later depots replace the same files from earlier ones. expected-files.tsv (in the export) lists every file with its size, SHA-1 and depot.

IF SOMETHING GOES WRONG
- "Failed to authenticate with Steam" or "InitializeSteam failed": the QR code was not scanned in time, or Steam refused the sign-in. Run the command again and scan the newest code. The account needs Counter-Strike 2 in its library.
- "Access token was rejected": the saved sign-in expired. Run the step 2 command with your username again.
- "Unable to locate manifest" or no access to a depot: check that this Steam account has Counter-Strike 2. Do not change the manifest numbers.
- Verify file integrity finds missing files: a download command did not finish. Run it again, then verify again.
- A schema mismatch or class registration error when compiling: the toolset mixes files from different versions. Download again into a new, empty toolset folder.

ABOUT THE CHECKS
The plan is checked against the file lists you fetched with your own Steam account, not against a Valve signature. The app's own download uses the same pinned versions, file selection and checks.
"#,
        fetch = fetch.join("\n\n"),
        dd = options.dd.display(),
        manifests = options.manifests.display(),
        toolroot = options.toolroot.display(),
        app = recipe.app,
        build = recipe.build,
    ))
}
pub fn preview(options: &Options, recipe: &Recipe) -> Result<Preview> {
    let argv = command_sets(recipe, options, None, None, &CancellationToken::new())?;
    let commands = commands_text(&argv, recipe)?;
    Ok(Preview {
        guide: guide(recipe, options, &argv, None)?,
        commands,
        argv,
        plan: serde_json::json!({"recipe":recipe,"policy":policy_text(recipe)?,"payload_plan_available":false}),
        plan_digest: None,
        expected_files: String::new(),
        export_dir: None,
        compatible: false,
        downloaded: false,
        whole_bytes: whole_bytes(recipe),
        selected_bytes: None,
    })
}
pub fn require_separate(output: &Path, inputs: &[&Path]) -> Result<()> {
    acquisition::no_links(output)?;
    let out = safety::resolve_path(output)?;
    for input in inputs {
        if input.as_os_str().is_empty() {
            continue;
        }
        let source = safety::resolve_path(input)?;
        if out.starts_with(&source) || source.starts_with(&out) {
            return Err(fail("Choose a separate export/work directory outside the toolset, manifests and existing tool installations."));
        }
    }
    Ok(())
}
pub fn export_plan(
    options: &Options,
    export_root: &Path,
    protected: &[&Path],
    recipe: &Recipe,
    cancel: &CancellationToken,
) -> Result<Preview> {
    acquisition::no_links(&options.manifests)?;
    let mut inputs = vec![options.toolroot.as_path(), options.manifests.as_path()];
    inputs.extend_from_slice(protected);
    require_separate(export_root, &inputs)?;
    let prepared =
        acquisition::prepared_plan(recipe, &options.manifests, &options.toolroot, cancel)?;
    fs::create_dir_all(export_root)
        .map_err(|e| CoreError::io("Create manual export directory", e))?;
    let destination = export_root.join(format!(
        "manual-{}-{}",
        &prepared.plan_digest[..12],
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir(&destination).map_err(|e| CoreError::io("Create fresh manual export", e))?;
    acquisition::write_plan_files(&destination, &prepared)?;
    let mut expected = String::from("path\tbytes\tsha1\tdepot\n");
    for file in &prepared.plan.files {
        expected.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            file.path,
            file.bytes,
            file.sha1,
            file.depot.as_deref().unwrap_or_default()
        ));
    }
    let argv = command_sets(recipe, options, Some(&prepared.plan), Some(&destination), cancel)?;
    let commands = commands_text(&argv, recipe)?;
    let guide = guide(recipe, options, &argv, Some(&prepared.plan))?;
    let selected_bytes = Some(prepared.plan.installed_bytes);
    safety::atomic_write(&destination.join("expected-files.tsv"), &expected)?;
    safety::atomic_write(&destination.join("commands.txt"), &commands)?;
    safety::atomic_write(&destination.join("instructions.txt"), &guide)?;
    Ok(Preview {
        guide,
        commands,
        argv,
        plan: serde_json::to_value(prepared.plan)?,
        plan_digest: Some(prepared.plan_digest),
        expected_files: expected,
        export_dir: Some(destination),
        compatible: false,
        downloaded: false,
        whole_bytes: whole_bytes(recipe),
        selected_bytes,
    })
}
