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
}
fn fail(message: impl Into<String>) -> CoreError {
    CoreError::Integrity(message.into())
}
pub fn powershell(argv: &[String]) -> Result<String> {
    let mut quoted = vec![];
    for value in argv {
        if value.chars().any(|c| c < ' ') {
            return Err(fail("Commands cannot contain control characters."));
        }
        quoted.push(format!("'{}'", value.replace('\'', "''")));
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
pub fn command_sets(
    recipe: &Recipe,
    options: &Options,
    plan: Option<&AcquisitionPlan>,
    export_dir: Option<&Path>,
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
            for entry in &plan.depots {
                let list = export
                    .join("filelists")
                    .join(format!("{}.txt", entry.depot.depot));
                let command = acquisition::download_command(
                    &options.dd,
                    recipe,
                    &entry.depot,
                    &list,
                    &options.toolroot,
                    &auth,
                )?;
                downloads.push(argv(&command.program, &command.args));
            }
        }
        result.insert(
            name.into(),
            Commands {
                metadata: argv(&metadata.program, &metadata.args),
                downloads,
            },
        );
    }
    Ok(result)
}
pub fn commands_text(commands: &BTreeMap<String, Commands>) -> Result<String> {
    let mut lines=vec!["POWERSHELL COMMANDS — choose ONE login mode per operation. Run commands individually, in order.".into()];
    for (mode, commands) in commands {
        lines.push(format!("\n{} LOGIN", mode.to_uppercase()));
        if mode == "username" {
            lines.push("Replace YOUR_STEAM_USERNAME. Complete authentication in DepotDownloader; never enter a password into the builder.".into());
        }
        lines.push("Step1: Metadata only; no tool payload.".into());
        lines.push(powershell(&commands.metadata)?);
        if commands.downloads.is_empty() {
            lines.push("Payload commands unavailable until ALL pinned manifests are supplied and exact filelists exported.".into());
        }
        for (index, args) in commands.downloads.iter().enumerate() {
            lines.push(format!(
                "Step2.{}: Tool payload; wait for completion.",
                index + 1
            ));
            lines.push(powershell(args)?);
        }
    }
    Ok(lines.join("\n\n") + "\n")
}
fn guide(
    recipe: &Recipe,
    options: &Options,
    commands: &str,
    plan: Option<&AcquisitionPlan>,
) -> Result<String> {
    let pins = recipe
        .depots
        .iter()
        .map(|d| format!("Depot {} / manifest {}", d.depot, d.manifest))
        .collect::<Vec<_>>()
        .join("\n");
    let sizes=plan.map(|p|format!("Selected installed bytes: {}. Unpacked transfer bytes (including overridden files): {}. Final files: {}.",p.installed_bytes,p.transfer_uncompressed_bytes,p.files.len())).unwrap_or_else(||"Selected installed/transfer sizes are unknown until every authorized pinned listing is supplied.".into());
    Ok(format!("MANUAL MODEL TOOLSET SETUP\n\nAutomatic acquisition is optional. This guide displays commands; no downloader was launched.\n\nRequirements: your Deadlock installation and original archive; Source2Viewer CLI; Reduced CSDK12 for material compilation; official SteamRE DepotDownloader; your Steam entitlement to the pinned content/Workshop Tools.\nOfficial releases: https://github.com/SteamRE/DepotDownloader/releases\nNo Valve binaries are bundled by the builder.\n\nUse a dedicated toolset destination: {}\nManifest folder: {}\nDepotDownloader: {}\nKeep this separate from Deadlock, CSDK, Counter-Strike and other toolsets. Do not overlay older versions or mix individual DLLs.\n\nSteam app {} / build {} / snapshot {}\n{pins}\nNo command automatically selects a newer manifest.\n\n{}\n\n{sizes}\nCompressed network download size is unknown. Whole-depot compressed sizes are not selected-download estimates. Allow downloader cache and an additional complete validation copy plus model outputs.\n\n1. Obtain official DepotDownloader and extract it locally.\n2. Open PowerShell and choose ONE login mode below. QR mode uses -qr only. Username mode uses -username and -remember-password only. DepotDownloader owns its prompts and token cache; never place a password in commands, projects, settings or logs.\n3. Run the metadata command and finish Steam authentication.\n4. Read all pinned listings in the builder and export the exact plan/filelists. Review installed sizes and expected-files.tsv.\n5. Run each payload command to completion in the listed order. Never remove -filelist to work around an error. Resolve a failed depot operation before continuing.\n6. Verify every selected file and reject unplanned executables/libraries. Size and SHA-1 must match. The only exception is an expected zero-byte file with a forty-zero SHA-1: it must exist and be empty.\n7. Configure the model toolset root, game/bin/win64/resourcecompiler.exe and game/csgo. Model backend cswin; ModelDoc format auto. Keep material compiler/CSDK settings separate.\n8. Validate the original selected hero with the real Windows compiler. Integrity/static evidence alone does not establish ModelDoc41 or AG2 compatibility. Deep gameplay/UI AnimGraph2 and NmSkeleton checks, replacement builds and live-game tests remain separate. A test of one hero certifies only that original hero.\n\nValidation must use an independent complete copy of selected tool files, never hardlinks or a mixed DLL family. It must preserve the acquired installation and saved settings, retain diagnostics, and reverify source hashes. No downloader should run during validation. Non-Windows hosts cannot certify native compiler compatibility.\n\n{commands}\n\nTroubleshooting: missing tool/config means verify all depot operations. Class-registration/schema errors require a compatible complete binary family; changing DMX or relabeling ModelDoc headers cannot repair startup ABI mismatches. Unavailable pinned metadata requires checking your Steam entitlement; do not substitute arbitrary versions. Validation-copy disk shortage requires a larger workspace.\n\nTrust: this plan checks consistency against your locally supplied authorized manifest listings, not an independent Valve signature. Recipe pins, selection, overrides and strict hashes use the same Rust acquisition backend for automatic and manual paths.\n",options.toolroot.display(),options.manifests.display(),options.dd.display(),recipe.app,recipe.build,recipe.queried_utc,policy_text(recipe)?))
}
pub fn preview(options: &Options, recipe: &Recipe) -> Result<Preview> {
    let argv = command_sets(recipe, options, None, None)?;
    let commands = commands_text(&argv)?;
    Ok(Preview {
        guide: guide(recipe, options, &commands, None)?,
        commands,
        argv,
        plan: serde_json::json!({"recipe":recipe,"policy":policy_text(recipe)?,"payload_plan_available":false}),
        plan_digest: None,
        expected_files: String::new(),
        export_dir: None,
        compatible: false,
        downloaded: false,
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
    let argv = command_sets(recipe, options, Some(&prepared.plan), Some(&destination))?;
    let commands = commands_text(&argv)?;
    let guide = guide(recipe, options, &commands, Some(&prepared.plan))?;
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
    })
}
