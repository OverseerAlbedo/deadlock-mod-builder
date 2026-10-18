//! Desktop setup actions over the same pinned acquisition and private validation services.
use crate::app::{AppError, Application, Result};
use deadlock_core::process::{Runner, RunnerOptions};
use deadlock_project::{asset_paths, Project, Settings};
use deadlock_tools::{acquisition, manual_setup, toolchain, validation};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetupDefaults {
    pub options: manual_setup::Options,
    pub export_root: PathBuf,
    pub automatic_root: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquireAction {
    Manifests,
    Download,
    Verify,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum AcquireAuth {
    Qr {},
    Username { username: String },
}
impl AcquireAuth {
    fn native(&self) -> acquisition::Auth {
        match self {
            Self::Qr {} => acquisition::Auth::Qr,
            Self::Username { username } => acquisition::Auth::Username(username.clone()),
        }
    }
}
fn path_text(path: &Path) -> Result<String> {
    asset_paths::text(path)
        .map_err(|e| AppError::validation("Setup path cannot be represented without loss", e))
}
fn unicode_options(options: &manual_setup::Options) -> Result<()> {
    for path in [&options.dd, &options.toolroot, &options.manifests] {
        path_text(path)?;
    }
    Ok(())
}
fn protections(settings: &Settings) -> Vec<&Path> {
    [
        &settings.deadlock,
        &settings.csdk,
        &settings.cswin,
        &settings.optional_cs2,
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .map(Path::new)
    .collect()
}
fn failed_validation(
    mut copy: validation::OwnedToolsetCopy,
    runner: &Runner,
    error: AppError,
) -> AppError {
    let _ = copy.record_failure(&error);
    match runner.quiescent() {
        Ok(_barrier) => drop(copy),
        Err(cleanup) => {
            let _ = copy.retain_for_recovery(cleanup);
            drop(copy);
        }
    }
    error
}
impl Application {
    pub fn setup_defaults(&self) -> Result<SetupDefaults> {
        let recipe = acquisition::recipe()?;
        Ok(SetupDefaults {
            options: manual_setup::defaults(&recipe),
            export_root: PathBuf::from(r"C:\DMBTools\manual-plans"),
            automatic_root: PathBuf::from(format!(r"C:\DMBTools\{}", recipe.build)),
        })
    }
    pub fn setup_manual_preview(
        &self,
        options: manual_setup::Options,
    ) -> Result<manual_setup::Preview> {
        self.operation("Preview manual tool setup", |clock, cancel| {
            self.step(clock, "Generate literal setup commands", || {
                cancel.check()?;
                unicode_options(&options)?;
                Ok(manual_setup::preview(&options, &acquisition::recipe()?)?)
            })
        })
        .map(|(value, _)| value)
    }
    pub fn setup_manual_export(
        &self,
        options: manual_setup::Options,
        export_root: &Path,
    ) -> Result<manual_setup::Preview> {
        self.operation("Export manual tool setup", |clock, cancel| {
            let settings = self.settings()?;
            unicode_options(&options)?;
            path_text(export_root)?;
            self.step(
                clock,
                "Read manifests and export exact selected files",
                || {
                    Ok(manual_setup::export_plan(
                        &options,
                        export_root,
                        &protections(&settings),
                        &acquisition::recipe()?,
                        cancel,
                    )?)
                },
            )
        })
        .map(|(value, _)| value)
    }
    pub fn setup_verify_files(
        &self,
        options: manual_setup::Options,
    ) -> Result<acquisition::IntegrityReceipt> {
        self.operation("Verify existing model tool files", |clock, cancel| {
            unicode_options(&options)?;
            let prepared = self.step(clock, "Plan pinned files from local manifests", || {
                Ok(acquisition::prepared_plan(
                    &acquisition::recipe()?,
                    &options.manifests,
                    &options.toolroot,
                    cancel,
                )?)
            })?;
            self.step(clock, "Verify every selected file size and hash", || {
                Ok(acquisition::validate_files(
                    &prepared.plan,
                    &options.toolroot,
                    cancel,
                )?)
            })
        })
        .map(|(value, _)| value)
    }
    pub fn setup_acquire(
        &self,
        action: AcquireAction,
        root: &Path,
        dd: Option<&Path>,
        auth: AcquireAuth,
        approved_plan_digest: Option<&str>,
    ) -> Result<acquisition::AcquisitionResult> {
        self.operation(
            match action {
                AcquireAction::Manifests => "Fetch model tool manifests",
                AcquireAction::Download => "Download selected model tools",
                AcquireAction::Verify => "Verify downloaded model tools",
            },
            |clock, cancel| {
                path_text(root)?;
                if let Some(dd) = dd {
                    path_text(dd)?;
                }
                // An existing owner marker is checked against the immutable recipe by the service.
                // A new acquisition root cannot contain or be inside an installed game/toolset.
                if matches!(action, AcquireAction::Manifests)
                    && !root.join("acquisition-owner.json").exists()
                {
                    let settings = self.settings()?;
                    manual_setup::require_separate(root, &protections(&settings))?;
                }
                let options = RunnerOptions {
                    timeout: Duration::from_secs(24 * 60 * 60),
                    ..Default::default()
                };
                let (runner, _logs) = self.create_runner_with_options(clock, cancel, options)?;
                // The QR code goes on screen; DepotDownloader has no console here.
                let _qr = matches!(auth, AcquireAuth::Qr {}).then(|| self.steam_qr_watcher(&runner));
                Ok(acquisition::run_acquisition(
                    match action {
                        AcquireAction::Manifests => acquisition::Action::Manifests,
                        AcquireAction::Download => acquisition::Action::Download,
                        AcquireAction::Verify => acquisition::Action::Verify,
                    },
                    root,
                    dd,
                    &auth.native(),
                    approved_plan_digest,
                    &runner,
                    clock,
                    None,
                    cancel,
                )?)
            },
        )
        .map(|(value, _)| value)
    }
    /// Opens DepotDownloader in its own console window for a one-time Steam
    /// sign-in with `username` and waits until it closes. Tool runs have no
    /// console and no input, so DepotDownloader cannot ask them for a password;
    /// after this it uses the sign-in it saved. The app never sees the password.
    pub fn setup_steam_sign_in(&self, dd: Option<&Path>, username: &str) -> Result<()> {
        self.operation("Sign in to Steam for DepotDownloader", |clock, cancel| {
            self.step(clock, "Wait for the Steam sign-in window", || {
                let username = username.trim();
                if !steam_account_name(username) {
                    return Err(AppError::validation(
                        "Enter your Steam account name",
                        "Use the name you sign in to Steam with (letters, digits and _), not your profile name.",
                    ));
                }
                let dd = deadlock_tools::setup_checks::depotdownloader(dd.unwrap_or(Path::new("")))
                    .map_err(|e| AppError::validation("Choose DepotDownloader.exe", e))?;
                let folder = self
                    .state_directory
                    .join("operations")
                    .join(clock.report().run_id)
                    .join("steam-sign-in");
                std::fs::create_dir_all(&folder)
                    .map_err(|e| AppError::io("Could not create the sign-in folder", e))?;
                let spec = acquisition::sign_in_command(&dd, &folder, username)?;
                let code = console_window(&spec, &folder, cancel);
                let _ = std::fs::remove_dir_all(&folder);
                match code? {
                    Some(0) => {
                        // Logs leave out the account name, like command receipts.
                        self.append_log("info", "DepotDownloader saved the Steam sign-in.")?;
                        Ok(())
                    }
                    code => Err(AppError::new(
                        "tool",
                        "Steam sign-in did not finish",
                        format!(
                            "The DepotDownloader sign-in window closed before the sign-in was saved (exit code {}). Enter your Steam password and Steam Guard code in that window.",
                            code.map_or("none".into(), |c| c.to_string())
                        ),
                    )),
                }
            })
        })
        .map(|(value, _)| value)
    }
    pub fn setup_validate_existing(
        &self,
        project: Project,
        options: manual_setup::Options,
    ) -> Result<validation::ValidationReport> {
        self.operation("Validate existing tools with original hero",|clock,cancel|{
            self.step(clock,"Check native validation requirements",||{
                cancel.check()?;unicode_options(&options)?;
                if !cfg!(windows){return Err(AppError::validation("Native tool validation requires Windows","File verification remains available on this host. ModelDoc, gameplay/UI AnimGraph2 and NmSkeleton compatibility requires running the actual Windows compiler."));}
                project.validate_action().map_err(|e|AppError::validation("Select an original hero before testing tools",e))?;Ok(())
            })?;
            let settings=self.settings()?;
            let(runner,_logs)=self.create_runner(clock,cancel)?;
            let copy=validation::prepare_existing_copy(&settings,&project,&validation::ValidationOptions{toolroot:options.toolroot,manifests:options.manifests},clock,None,cancel)?;
            self.append_log("info",&format!("Independent tool validation report: {}",copy.report.report_directory.display()))?;
            let log=|message:&str|{let _=self.append_log("info",message);};
            let reader_root=self.native_reader_root();
            let pipeline=deadlock_pipeline::inspect::ModelPipeline{settings:&copy.settings,runner:&runner,cancel,clock,log:&log,reader_root:&reader_root};
            let result=pipeline.validate_original(&copy.project,true);
            let original=match result{Ok(value)=>value,Err(error)=>{return Err(failed_validation(copy,&runner,error.into()));}};
            let evidence=match original.validation_evidence{Some(evidence)=>evidence,None=>{
                let error=AppError::validation("Original compiler evidence is missing","Compatibility was not established; cleanup status and the failure are retained in the validation report.");
                return Err(failed_validation(copy,&runner,error));
            }};
            let report=copy.finish(evidence,&runner,clock,None,cancel)?;
            self.append_log("info",if report.compatible{"Original hero compatibility checks passed. Replacement and live-game testing remain separate."}else{"Original hero compatibility has not been established."})?;
            Ok(report)
        }).map(|(value,_)|value)
    }
    /// Returns a draft selection; the normal Save Settings action remains the persistence boundary.
    pub fn setup_use_toolset(&self, mut settings: Settings, toolroot: &Path) -> Result<Settings> {
        self.operation("Select model toolset", |clock, cancel| {
            self.step(clock, "Check selected compiler and model game", || {
                cancel.check()?;
                // Saved settings use the ordinary resolved spelling (C:\..., not \\?\C:\...).
                let root = asset_paths::resolve(toolroot)
                    .map_err(|e| AppError::validation("Setup path cannot be resolved", e))?;
                // A folder just above or inside the toolset (the acquisition
                // folder, the toolset's game folder) selects the toolset itself.
                let root = deadlock_tools::setup_checks::toolset_root(&root).unwrap_or(root);
                let game = root.join("game");
                settings.cswin = path_text(&root)?;
                settings.cswin_resourcecompiler = path_text(
                    &game.join("bin").join("win64").join("resourcecompiler.exe"),
                )?;
                settings.model_game = path_text(&game.join("csgo"))?;
                settings.model_backend = "cswin".into();
                settings.model_format = "auto".into();
                toolchain::select(&settings, true)?;
                cancel.check()?;
                Ok(settings)
            })
        })
        .map(|(value, _)| value)
    }
}

/// A Steam account name (what you sign in with): letters, digits and _.
fn steam_account_name(name: &str) -> bool {
    (2..=64).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
/// Runs `spec` in a new console window the user can type into (cmd's start
/// gives it the window's own input and output) and waits for its exit code.
/// `start /wait` alone leaves cmd's exit code at 0; the delayed `!errorlevel!`
/// carries the program's.
#[cfg(windows)]
fn console_window(
    spec: &deadlock_core::process::CommandSpec,
    cwd: &Path,
    cancel: &deadlock_core::CancellationToken,
) -> Result<Option<i32>> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // Every part is quoted; cmd would still expand % and ! and cannot quote ".
    let quoted = |part: &std::ffi::OsStr| -> Result<String> {
        match part.to_str() {
            Some(text) if !text.chars().any(|c| matches!(c, '"' | '%' | '!') || c.is_control()) => Ok(format!("\"{text}\"")),
            _ => Err(AppError::validation(
                "This path cannot be passed to the sign-in window",
                format!("Move DepotDownloader to a folder without \", % or ! in its path: {}", part.to_string_lossy()),
            )),
        }
    };
    let mut line = format!("/d /v:on /c start \"Steam sign-in (Deadlock Mod Builder)\" /wait {}", quoted(spec.program.as_os_str())?);
    for arg in &spec.args {
        line.push(' ');
        line.push_str(&quoted(arg)?);
    }
    line.push_str(" & exit !errorlevel!");
    let shell = std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into());
    let mut child = std::process::Command::new(shell)
        .raw_arg(line)
        .current_dir(cwd)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| AppError::io("Could not open the Steam sign-in window", e))?;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| AppError::io("Could not wait for the Steam sign-in window", e))? {
            return Ok(status.code());
        }
        if let Err(error) = cancel.check() {
            let _ = child.kill();
            return Err(error.into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
#[cfg(not(windows))]
fn console_window(
    _spec: &deadlock_core::process::CommandSpec,
    _cwd: &Path,
    _cancel: &deadlock_core::CancellationToken,
) -> Result<Option<i32>> {
    Err(AppError::validation("The Steam sign-in window needs Windows", "Sign in with DepotDownloader in a terminal: -username NAME -remember-password."))
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    use std::sync::Arc;
    use tauri::State;
    type AppState<'a> = State<'a, Arc<Application>>;
    async fn background<T: Send + 'static>(
        work: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<T> {
        tauri::async_runtime::spawn_blocking(work)
            .await
            .map_err(|e| AppError::new("internal", "Tool setup stopped unexpectedly", e))?
    }
    #[tauri::command]
    pub async fn setup_defaults(state: AppState<'_>) -> Result<SetupDefaults> {
        let app = state.inner().clone();
        background(move || app.setup_defaults()).await
    }
    #[tauri::command]
    pub async fn setup_manual_preview(
        state: AppState<'_>,
        options: manual_setup::Options,
    ) -> Result<manual_setup::Preview> {
        let app = state.inner().clone();
        background(move || app.setup_manual_preview(options)).await
    }
    #[tauri::command]
    pub async fn setup_manual_export(
        state: AppState<'_>,
        options: manual_setup::Options,
        export_root: String,
    ) -> Result<manual_setup::Preview> {
        let app = state.inner().clone();
        background(move || app.setup_manual_export(options, Path::new(&export_root))).await
    }
    #[tauri::command]
    pub async fn setup_verify_files(
        state: AppState<'_>,
        options: manual_setup::Options,
    ) -> Result<acquisition::IntegrityReceipt> {
        let app = state.inner().clone();
        background(move || app.setup_verify_files(options)).await
    }
    #[tauri::command]
    pub async fn setup_acquire(
        state: AppState<'_>,
        action: AcquireAction,
        root: String,
        dd: Option<String>,
        auth: AcquireAuth,
        approved_plan_digest: Option<String>,
    ) -> Result<acquisition::AcquisitionResult> {
        let app = state.inner().clone();
        background(move || {
            app.setup_acquire(
                action,
                Path::new(&root),
                dd.as_deref().map(Path::new),
                auth,
                approved_plan_digest.as_deref(),
            )
        })
        .await
    }
    #[tauri::command]
    pub async fn setup_steam_sign_in(
        state: AppState<'_>,
        dd: Option<String>,
        username: String,
    ) -> Result<()> {
        let app = state.inner().clone();
        background(move || app.setup_steam_sign_in(dd.as_deref().map(Path::new), &username)).await
    }
    #[tauri::command]
    pub async fn setup_validate_existing(
        state: AppState<'_>,
        project: Project,
        options: manual_setup::Options,
    ) -> Result<validation::ValidationReport> {
        let app = state.inner().clone();
        background(move || app.setup_validate_existing(project, options)).await
    }
    #[tauri::command]
    pub async fn setup_use_toolset(
        state: AppState<'_>,
        settings: Settings,
        toolroot: String,
    ) -> Result<Settings> {
        let app = state.inner().clone();
        background(move || app.setup_use_toolset(settings, Path::new(&toolroot))).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn steam_account_names_are_letters_digits_and_underscores() {
        assert!(steam_account_name("example_account"));
        for wrong in ["", "a", "-qr", "two words", "name\"&calc", "50%"] {
            assert!(!steam_account_name(wrong), "{wrong}");
        }
    }
    /// Opens a console window for a moment; run with --ignored on Windows.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn the_sign_in_window_reports_the_program_exit_code() {
        let folder = tempfile::tempdir().unwrap();
        let shell = deadlock_core::process::CommandSpec::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe").args(["-NoProfile", "-Command", "exit 5"]);
        let cancel = deadlock_core::CancellationToken::new();
        assert_eq!(console_window(&shell, folder.path(), &cancel).unwrap(), Some(5));
        let bad = deadlock_core::process::CommandSpec::new(r"C:\100%\DepotDownloader.exe");
        assert!(console_window(&bad, folder.path(), &cancel).is_err());
    }
}
