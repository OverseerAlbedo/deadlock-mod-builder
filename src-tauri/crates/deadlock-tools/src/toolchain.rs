use deadlock_core::{
    safety::{digest_with_cancel, inside, resolve_path},
    CancellationToken, CoreError, Result,
};
use deadlock_project::Settings;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompileReceipt {
    pub schema: u32,
    pub created_utc: chrono::DateTime<chrono::Utc>,
    pub kind: String,
    pub native: bool,
    pub host: String,
    pub toolset: ToolIdentity,
    pub hero: String,
    pub animation_policy: deadlock_project::AnimationPolicy,
    pub ag2_nodes_checked: Vec<deadlock_model::ag2::Reference>,
    pub model_format_selection: String,
    pub compiled_sha256: String,
    pub ag2_output_checked: bool,
    pub scope: String,
    pub unhashed_dependencies: String,
}
/// Persist evidence only after the caller has completed the native DATA/RERL checks.
/// A saved receipt is diagnostic evidence, never a substitute for current validation.
pub fn write_compile_receipt(
    settings: &Settings,
    project: &deadlock_project::Project,
    destination: &Path,
    output: &Path,
    ag2_nodes: &[deadlock_model::ag2::Reference],
    runner: &dyn deadlock_core::process::ToolExecutor,
    cancel: &CancellationToken,
) -> Result<CompileReceipt> {
    let native = runner.native_execution() && cfg!(windows);
    let receipt = CompileReceipt {
        schema: 1,
        created_utc: chrono::Utc::now(),
        kind: if native { "native_original_compile" } else { "non_native_or_test_process_compile" }.into(),
        native,
        host: format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),
        toolset: identity(&select(settings,true)?,cancel)?,
        hero: project.original_model.clone(),
        animation_policy: project.animation_policy,
        ag2_nodes_checked: ag2_nodes.to_vec(),
        model_format_selection: settings.model_format.clone(),
        compiled_sha256: digest_with_cancel(output,cancel)?,
        ag2_output_checked: !ag2_nodes.is_empty(),
        scope: "This original model and local tool hashes only. Replacement skinning and live-game behavior are not certified.".into(),
        unhashed_dependencies: "Mounted resource packs, system DLLs and runtime-loaded files outside this bin remain outside the fingerprint.".into(),
    };
    deadlock_core::safety::write_json(destination, &receipt)?;
    Ok(receipt)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelToolset {
    pub root: PathBuf,
    pub compiler: PathBuf,
    pub game: PathBuf,
    pub backend: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolPaths {
    pub content: PathBuf,
    pub game: PathBuf,
    pub compiler: PathBuf,
    pub target_game: PathBuf,
}
impl ModelToolset {
    pub fn paths(&self, project_id: &str) -> Result<ToolPaths> {
        if project_id.len() != 32
            || !project_id
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(CoreError::UnsafePath("Invalid project ID".into()));
        }
        let target = self
            .game
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| CoreError::UnsafePath("Invalid model game target".into()))?;
        let addon = format!("dmb_{}_model", &project_id[..16]);
        Ok(ToolPaths {
            content: inside(&self.root, &format!("content/{target}_addons/{addon}"))?,
            game: inside(&self.root, &format!("game/{target}_addons/{addon}"))?,
            compiler: self.compiler.clone(),
            target_game: self.game.clone(),
        })
    }
}
fn fail(text: impl Into<String>) -> CoreError {
    CoreError::Integrity(text.into())
}
/// Resource jobs mounted under CSDK content/citadel_addons need the matching
/// SDK game configuration, independently of the selected model compiler.
pub fn sdk_game(settings: &Settings) -> Result<PathBuf> {
    if settings.csdk.is_empty() {
        return Err(fail("Choose the Community SDK folder in Settings."));
    }
    let root = resolve_path(Path::new(&settings.csdk))?;
    let compiler = resolve_path(Path::new(&settings.resourcecompiler))?;
    if settings.resourcecompiler.is_empty() || !compiler.is_file() {
        return Err(fail(format!("Select the material compiler in Settings → Resource compiler: {}.", root.join("game/bin_cs2/win64/resourcecompiler.exe").display())));
    }
    if !compiler.starts_with(&root) {
        return Err(fail(format!("Settings → Resource compiler points outside the Community SDK ({}). Use the SDK's own compiler, {}. The model toolset's compiler belongs only under Advanced configuration.", compiler.display(), root.join("game/bin_cs2/win64/resourcecompiler.exe").display())));
    }
    if !settings.deadlock.is_empty()
        && root.starts_with(resolve_path(Path::new(&settings.deadlock))?)
    {
        return Err(fail(
            "Community SDK must be separate from the original Deadlock installation.",
        ));
    }
    let game = root.join("game/citadel");
    let configuration = game.join("gameinfo.gi");
    if !configuration.is_file() {
        let nested = crate::setup_checks::csdk_root(&root).filter(|r| *r != root);
        return Err(fail(match nested {
            Some(inner) => format!("Settings → Community SDK points to the folder around the SDK. Choose {} instead (the SDK was extracted inside a folder of the same name).", inner.display()),
            None => format!("The Community SDK is incomplete: {} is missing. Extract the whole Reduced CSDK 12 ZIP into a new, empty folder such as C:\\CSDK12 and select that folder in Settings → Community SDK.", configuration.display()),
        }));
    }
    deadlock_vpk::check_unlinked_path(&configuration, false).map_err(|e| fail(e.to_string()))?;
    Ok(game)
}
pub const MODEL_TOOLSET_NOT_SET: &str = "The model compiler is not set up. Open Settings → Set up model compiler…, get the files, then click Use this toolset in settings and save. The Community SDK compiles materials only.";
fn suffixes(backend: &str) -> &'static [&'static str] {
    if backend == "csdk" {
        &[
            "game/bin_cs2/win64/resourcecompiler.exe",
            "game/bin/win64/resourcecompiler.exe",
        ]
    } else {
        &[
            "game/bin/win64/resourcecompiler.exe",
            "bin/win64/resourcecompiler.exe",
        ]
    }
}
pub fn select(settings: &Settings, require_files: bool) -> Result<ModelToolset> {
    let backend = match settings.model_backend.as_str() {
        // Auto never falls back to the Community SDK: its compiler stops hero
        // model compiles with a ParticleFloatType_t schema mismatch.
        "auto" => {
            if settings.cswin.is_empty() {
                return Err(fail(MODEL_TOOLSET_NOT_SET));
            }
            "cswin"
        }
        "cswin" => "cswin",
        "csdk" => "csdk",
        _ => {
            return Err(fail(
                "Choose Auto, Local AG2 toolset or CSDK candidate in Setup.",
            ));
        }
    };
    let root_text = if backend == "csdk" {
        &settings.csdk
    } else {
        &settings.cswin
    };
    if root_text.is_empty() {
        return Err(fail(MODEL_TOOLSET_NOT_SET));
    }
    let root = resolve_path(Path::new(root_text))?;
    let compiler = if !settings.cswin_resourcecompiler.is_empty() {
        resolve_path(Path::new(&settings.cswin_resourcecompiler))?
    } else {
        let options = suffixes(backend);
        root.join(
            options
                .iter()
                .find(|name| root.join(name).is_file())
                .unwrap_or(&options[0]),
        )
    };
    let game = if !settings.model_game.is_empty() {
        resolve_path(Path::new(&settings.model_game))?
    } else {
        root.join(if backend == "csdk" {
            "game/citadel"
        } else {
            "game/csgo"
        })
    };
    let name = game
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if game.parent() != Some(root.join("game").as_path())
        || name.is_empty()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || name.ends_with("_addons")
    {
        return Err(fail(
            "Model game directory must be a named target directly under the selected toolset's game folder (for example game/citadel or game/csgo).",
        ));
    }
    if !compiler.starts_with(&root) {
        return Err(fail(
            "Settings → Model compiler executable must be the compiler inside Model compiler toolset (game\\bin\\win64\\resourcecompiler.exe). Click Use this toolset in settings in Set up model compiler… to fill both.",
        ));
    }
    if !settings.deadlock.is_empty()
        && root.starts_with(resolve_path(Path::new(&settings.deadlock))?)
    {
        return Err(fail(
            "Model toolset must be separate from the original Deadlock installation.",
        ));
    }
    if require_files {
        if !root.is_dir() || !compiler.is_file() {
            return Err(fail(format!(
                "Model compiler is missing: {}. Open Settings → Set up model compiler…, download or verify the toolset, then click Use this toolset in settings and save.",
                compiler.display()
            )));
        }
        if !game.is_dir() || !game.join("gameinfo.gi").is_file() {
            return Err(fail(format!(
                "Model game configuration is missing: {}. The toolset is incomplete; run Verify existing acquisition in Set up model compiler… and download again if files are missing.",
                game.join("gameinfo.gi").display()
            )));
        }
    }
    Ok(ModelToolset {
        root,
        compiler,
        game,
        backend: backend.into(),
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub backend: String,
    pub root: PathBuf,
    pub compiler: PathBuf,
    pub game: PathBuf,
    pub configuration_found: bool,
    pub ag2_test: String,
}
pub fn candidates(settings: &Settings, roots: &[PathBuf]) -> Result<Vec<Candidate>> {
    let mut entries: Vec<(&str, PathBuf)> = roots.iter().cloned().map(|p| ("cswin", p)).collect();
    if !settings.cswin.is_empty() {
        entries.insert(0, ("cswin", PathBuf::from(&settings.cswin)));
    }
    if !settings.csdk.is_empty() {
        entries.push(("csdk", PathBuf::from(&settings.csdk)));
    }
    if !settings.optional_cs2.is_empty() {
        entries.push(("cswin", PathBuf::from(&settings.optional_cs2)));
    }
    let mut found = vec![];
    let mut seen = HashSet::new();
    for (backend, root) in entries {
        for suffix in suffixes(backend) {
            let compiler = root.join(suffix);
            if compiler.is_file() {
                let compiler = resolve_path(&compiler)?;
                if seen.insert(compiler.clone()) {
                    let game = resolve_path(&root.join(if backend == "csdk" {
                        "game/citadel"
                    } else {
                        "game/csgo"
                    }))?;
                    found.push(Candidate {
                        backend: backend.into(),
                        root: resolve_path(&root)?,
                        compiler,
                        configuration_found: game.join("gameinfo.gi").is_file(),
                        game,
                        ag2_test: "not run".into(),
                    });
                }
            }
        }
    }
    Ok(found)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolIdentity {
    pub backend: String,
    pub root: PathBuf,
    pub compiler: PathBuf,
    pub game: PathBuf,
    pub file_sha256: BTreeMap<String, String>,
    pub fingerprint: String,
    pub fingerprint_schema: String,
}
pub fn identity(toolset: &ModelToolset, cancel: &CancellationToken) -> Result<ToolIdentity> {
    let mut files = BTreeMap::new();
    if let Some(bin) = toolset.compiler.parent().filter(|p| p.is_dir()) {
        for entry in
            fs::read_dir(bin).map_err(|e| CoreError::io("Read tool binary directory", e))?
        {
            cancel.check()?;
            let path = entry
                .map_err(|e| CoreError::io("Read tool binary entry", e))?
                .path();
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if path.is_file() && matches!(extension.as_str(), "exe" | "dll") {
                let relative = path
                    .strip_prefix(&toolset.root)
                    .map_err(|_| CoreError::PathEscape(path.clone()))?;
                files.insert(
                    relative.to_string_lossy().into_owned(),
                    digest_with_cancel(&path, cancel)?,
                );
            }
        }
    }
    let game_root = toolset.root.join("game");
    if game_root.is_dir() {
        for directory in
            fs::read_dir(game_root).map_err(|e| CoreError::io("Read tool game directories", e))?
        {
            let directory = directory
                .map_err(|e| CoreError::io("Read tool game directory", e))?
                .path();
            if directory.is_dir() {
                for name in ["gameinfo.gi", "gameinfo_branchspecific.gi"] {
                    let path = directory.join(name);
                    if path.is_file() {
                        let relative = path
                            .strip_prefix(&toolset.root)
                            .map_err(|_| CoreError::PathEscape(path.clone()))?;
                        files.insert(
                            relative.to_string_lossy().into_owned(),
                            digest_with_cancel(&path, cancel)?,
                        );
                    }
                }
            }
        }
    }
    // Explicit key ordering is independent of serde_json's feature-unified
    // preserve_order setting in the final desktop executable.
    let value: BTreeMap<&str, serde_json::Value> = [
        ("backend", serde_json::to_value(&toolset.backend)?),
        ("root", serde_json::to_value(&toolset.root)?),
        ("compiler", serde_json::to_value(&toolset.compiler)?),
        ("game", serde_json::to_value(&toolset.game)?),
        ("file_sha256", serde_json::to_value(&files)?),
    ]
    .into();
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&value)?));
    Ok(ToolIdentity {
        backend: toolset.backend.clone(),
        root: toolset.root.clone(),
        compiler: toolset.compiler.clone(),
        game: toolset.game.clone(),
        file_sha256: files,
        fingerprint,
        fingerprint_schema: "rust-sorted-json-v1".into(),
    })
}
pub fn audit(settings: &Settings) -> Result<serde_json::Value> {
    audit_with_cancel(settings, &CancellationToken::new())
}
pub fn audit_with_cancel(
    settings: &Settings,
    cancel: &CancellationToken,
) -> Result<serde_json::Value> {
    let selected = select(settings, false);
    let mut report = serde_json::json!({"full_cs2_required_by_builder":false,"native_ag2_test":"not run by this audit","candidates":candidates(settings,&[])?,"optional_cs2":settings.optional_cs2,
        "limitations":["Presence of an EXE, DLL or bin_cs2 folder does not prove ModelDoc format/AG2 support.","Only a native compile and deep output checks establish compatibility for the selected source; live-game behavior is a separate test."]});
    match selected {
        Ok(tool) => {
            let required = [tool.compiler.clone(), tool.game.join("gameinfo.gi")];
            report["selected"] = serde_json::to_value(&tool)?;
            report["required_entry_files"] = serde_json::to_value(
                required
                    .iter()
                    .map(|p| (p.to_string_lossy(), p.is_file()))
                    .collect::<BTreeMap<_, _>>(),
            )?;
            report["errors"] = serde_json::json!(required
                .iter()
                .filter(|p| !p.is_file())
                .map(|p| format!("Missing {}", p.display()))
                .collect::<Vec<_>>());
            report["selected"]["required_entry_files"] = report["required_entry_files"].clone();
            report["pe_import_audit"] = crate::pe::local_import_audit(&tool.compiler, cancel)?;
            let mut binaries = vec![];
            if let Some(directory) = tool.compiler.parent().filter(|p| p.is_dir()) {
                for entry in fs::read_dir(directory)
                    .map_err(|e| CoreError::io("Read compiler directory", e))?
                {
                    cancel.check()?;
                    let path = entry
                        .map_err(|e| CoreError::io("Inspect compiler directory", e))?
                        .path();
                    if path.is_file()
                        && path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                            matches!(s.to_ascii_lowercase().as_str(), "exe" | "dll")
                        })
                    {
                        binaries.push(
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                        );
                    }
                }
            }
            binaries.sort();
            report["observed_bin_files"] = serde_json::json!(binaries);
            let mut configurations = vec![];
            let mounts=regex::Regex::new(r#"(?i)^\s*"?(?:Game|Mod|Write|AddonRoot|ContentRoot|Tools|Platform|LayeredOnMod|SatelliteDir|#base)\b"#).expect("constant regex");
            let game_root = tool.root.join("game");
            if game_root.is_dir() {
                for entry in fs::read_dir(game_root)
                    .map_err(|e| CoreError::io("Read tool game directories", e))?
                {
                    let directory = entry
                        .map_err(|e| CoreError::io("Read game directory", e))?
                        .path();
                    if directory.is_dir() {
                        for file in fs::read_dir(directory)
                            .map_err(|e| CoreError::io("Read game configurations", e))?
                        {
                            cancel.check()?;
                            let path = file
                                .map_err(|e| CoreError::io("Read game configuration", e))?
                                .path();
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            if path.is_file()
                                && name.starts_with("gameinfo")
                                && name.ends_with(".gi")
                            {
                                let raw = fs::read(&path).map_err(|e| {
                                    CoreError::io("Read game configuration text", e)
                                })?;
                                let text = String::from_utf8_lossy(&raw);
                                configurations.push(serde_json::json!({"path":path,"sha256":digest_with_cancel(&path,cancel)?,"mount_lines":text.trim_start_matches('\u{feff}').lines().filter(|line|mounts.is_match(line)).map(str::trim).collect::<Vec<_>>()}));
                            }
                        }
                    }
                }
            }
            configurations
                .sort_by_key(|value| value["path"].as_str().unwrap_or_default().to_owned());
            report["game_configurations"] = serde_json::json!(configurations);
        }
        Err(error) => {
            report["selected"] = serde_json::Value::Null;
            report["errors"] = serde_json::json!([error.to_string()]);
        }
    }
    Ok(report)
}
