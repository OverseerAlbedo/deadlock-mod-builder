use crate::app::{
    AnalysisResult, AppError, Application, IncludedPhysics, LogEntry, ProjectDocument, Result,
    SetupStatus,
};
use deadlock_core::TimingReport;
use deadlock_project::{Project, Settings};
use std::{path::PathBuf, sync::Arc};
use tauri::{image::Image, Emitter, Manager, State};

type AppState<'a> = State<'a, Arc<Application>>;

async fn background<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| AppError::new("internal", "The operation stopped unexpectedly", error))?
}

#[tauri::command]
async fn get_settings(state: AppState<'_>) -> Result<Settings> {
    let app = state.inner().clone();
    background(move || app.settings()).await
}
#[tauri::command]
async fn save_settings(state: AppState<'_>, settings: Settings) -> Result<()> {
    let app = state.inner().clone();
    background(move || app.save_settings(&settings)).await
}
/// The window icon of a theme: the plain D, or one of the gradient Ds (the
/// taskbar and the title bar follow it while the app runs).
fn theme_icon(theme: &str) -> Option<Image<'static>> {
    let bytes: &'static [u8] = match theme {
        "violet" => include_bytes!("../icons/icon-violet.png"),
        "mint" => include_bytes!("../icons/icon-mint.png"),
        _ => include_bytes!("../icons/icon.png"),
    };
    Image::from_bytes(bytes).ok()
}
fn apply_theme_icon(window: &tauri::WebviewWindow, theme: &str) {
    if let Some(icon) = theme_icon(theme) {
        let _ = window.set_icon(icon);
    }
}
/// The window icon a theme uses: a built-in theme's own D, a custom theme's chosen one.
fn icon_for(settings: &Settings) -> String {
    settings.custom_themes.iter().find(|p| p.id == settings.theme).map(|p| p.icon.clone()).unwrap_or_else(|| settings.theme.clone())
}
/// Saves the theme at once (not with the tool paths) and switches the window icon.
#[tauri::command]
async fn set_theme(window: tauri::WebviewWindow, state: AppState<'_>, theme: String, icon: Option<String>) -> Result<Settings> {
    let app = state.inner().clone();
    let settings = background(move || {
        let mut settings = app.settings()?;
        settings.theme = theme;
        app.save_settings(&settings)?;
        Ok(settings)
    })
    .await?;
    apply_theme_icon(&window, icon.as_deref().unwrap_or(&icon_for(&settings)));
    Ok(settings)
}
/// Saves the custom themes at once (Settings → Appearance → Customize).
#[tauri::command]
async fn save_theme_presets(state: AppState<'_>, presets: Vec<deadlock_project::ThemePreset>) -> Result<Settings> {
    let app = state.inner().clone();
    background(move || {
        let mut settings = app.settings()?;
        settings.custom_themes = presets;
        app.save_settings(&settings)?;
        Ok(settings)
    })
    .await
}
#[tauri::command]
async fn detect_settings(
    state: AppState<'_>,
    settings: Settings,
    extra_roots: Vec<PathBuf>,
) -> Result<Settings> {
    let app = state.inner().clone();
    background(move || app.detect_settings(settings, extra_roots)).await
}
#[tauri::command]
fn get_setup_status(state: AppState<'_>, settings: Settings) -> SetupStatus {
    state.setup_status(&settings)
}
/// What this copy of the app was built with; the UI hides choices that are not compiled in.
#[derive(serde::Serialize)]
pub struct Capabilities {
    /// The TangTang presets (private data; see deadlock-cloth-authoring/build.rs).
    pub tangtang_presets: bool,
}
#[tauri::command]
fn get_capabilities() -> Capabilities {
    Capabilities { tangtang_presets: deadlock_pipeline::TANGTANG_PRESETS }
}
/// Whether the hero has extreme proportions (Shiv, Wraith), for a notice at selection.
#[tauri::command]
async fn hero_proportions(state: AppState<'_>, path: String) -> Result<Option<crate::hero_fit::HeroProportions>> {
    let app = state.inner().clone();
    background(move || app.hero_proportions(&path)).await
}
/// Autosave of the open project (no operation lock; a plain file write).
#[tauri::command]
async fn autosave_project(state: AppState<'_>, document: crate::app::ProjectDocument) -> Result<crate::autosave::Autosave> {
    let app = state.inner().clone();
    background(move || app.autosave_project(&document)).await
}
#[tauri::command]
fn clear_autosaves(state: AppState<'_>, id: String) -> Result<()> {
    state.clear_autosaves(&id)
}
/// Whether the last session crashed, and the autosaves that can be restored.
#[tauri::command]
fn session_recovery(state: AppState<'_>) -> crate::autosave::Recovery {
    state.recovery()
}
#[tauri::command]
fn dismiss_recovery(state: AppState<'_>) {
    state.dismiss_recovery()
}
#[tauri::command]
fn restore_autosave(state: AppState<'_>, id: String, file: String) -> Result<crate::app::ProjectDocument> {
    state.restore_autosave(&id, &file)
}
/// Saved physics presets (Fine-tune movement values per part type).
#[tauri::command]
fn physics_presets(state: AppState<'_>) -> Result<Vec<crate::physics_presets::PhysicsPreset>> {
    state.physics_presets()
}
#[tauri::command]
fn save_physics_preset(state: AppState<'_>, name: String, tuning: std::collections::BTreeMap<String, deadlock_project::customization::MotionTuning>) -> Result<Vec<crate::physics_presets::PhysicsPreset>> {
    state.save_physics_preset(&name, tuning)
}
#[tauri::command]
fn delete_physics_preset(state: AppState<'_>, name: String) -> Result<Vec<crate::physics_presets::PhysicsPreset>> {
    state.delete_physics_preset(&name)
}
/// The movement preview packet of the Physics page (binary).
#[tauri::command]
async fn motion_preview(state: AppState<'_>, project: Project) -> Result<tauri::ipc::Response> {
    let app = state.inner().clone();
    background(move || app.motion_preview(project)).await.map(tauri::ipc::Response::new)
}
/// Test my setup: paths, one material compile and the model compiler probe.
#[tauri::command]
async fn test_setup(state: AppState<'_>, settings: Settings) -> Result<crate::setup_test::SetupTest> {
    let app = state.inner().clone();
    background(move || app.test_setup(settings)).await
}
/// Warnings shown before Build starts (game running, low space, OneDrive).
#[tauri::command]
async fn prebuild_warnings(state: AppState<'_>) -> Result<Vec<String>> {
    let app = state.inner().clone();
    background(move || app.prebuild_warnings()).await
}
/// Typical seconds per operation, for the time-left estimate.
#[tauri::command]
fn operation_durations(state: AppState<'_>) -> std::collections::BTreeMap<String, f64> {
    state.operation_durations()
}
/// Live per-field path checks for the Settings page (files only, no tools run).
#[tauri::command]
async fn check_setting_fields(settings: Settings) -> Result<Vec<deadlock_tools::setup_checks::FieldCheck>> {
    background(move || Ok(deadlock_tools::setup_checks::field_checks(&settings))).await
}
/// Apply every suggested path fix (Fix all on the Settings page).
#[tauri::command]
async fn fix_setting_fields(settings: Settings) -> Result<Settings> {
    background(move || Ok(deadlock_tools::setup_checks::apply_fixes(&settings))).await
}
#[tauri::command]
async fn import_source_package(
    state: AppState<'_>,
    path: PathBuf,
    project: Project,
) -> Result<Project> {
    let app = state.inner().clone();
    background(move || app.import_source_package(&path, project)).await
}
#[tauri::command]
async fn analyze_materials(state: AppState<'_>, project: Project) -> Result<Project> {
    let app = state.inner().clone();
    background(move || app.analyze_materials(project)).await
}
#[tauri::command]
fn new_project(state: AppState<'_>) -> Result<ProjectDocument> {
    state.new_project()
}
#[tauri::command]
async fn open_project(state: AppState<'_>, path: PathBuf) -> Result<ProjectDocument> {
    let app = state.inner().clone();
    background(move || app.open_project(&path)).await
}
#[tauri::command]
async fn save_project(
    state: AppState<'_>,
    path: PathBuf,
    project: Project,
) -> Result<ProjectDocument> {
    let app = state.inner().clone();
    background(move || app.save_project(&path, project)).await
}
#[tauri::command]
async fn analyze_dmx(state: AppState<'_>, path: PathBuf) -> Result<AnalysisResult> {
    let app = state.inner().clone();
    background(move || app.analyze_dmx(&path)).await
}
#[tauri::command]
async fn preview_dmx(state: AppState<'_>, path: PathBuf) -> Result<tauri::ipc::Response> {
    let app = state.inner().clone();
    background(move || app.preview_dmx(&path).map(tauri::ipc::Response::new)).await
}
#[tauri::command]
async fn preview_source(state: AppState<'_>, path: PathBuf) -> Result<tauri::ipc::Response> {
    let app = state.inner().clone();
    background(move || app.preview_source(&path).map(tauri::ipc::Response::new)).await
}
#[tauri::command]
async fn get_heroes(state: AppState<'_>, all_models: bool) -> Result<Vec<crate::app::HeroModel>> {
    let app = state.inner().clone();
    background(move || app.heroes(all_models)).await
}
#[tauri::command]
async fn select_hero(state: AppState<'_>, project: Project, path: String) -> Result<Project> {
    let app = state.inner().clone();
    background(move || app.select_hero(project, &path)).await
}
#[tauri::command]
async fn included_physics(state: AppState<'_>, hero_path: String) -> Result<IncludedPhysics> {
    let app = state.inner().clone();
    background(move || app.included_physics(&hero_path)).await
}
#[tauri::command]
async fn validate_asset_path(path: PathBuf, kind: crate::intake::AssetKind) -> Result<String> {
    background(move || crate::intake::validate_asset_path(&path, kind)).await
}
#[tauri::command]
fn cancel_operation(state: AppState<'_>) {
    state.cancel();
}
#[tauri::command]
fn get_timing_report(state: AppState<'_>) -> Option<TimingReport> {
    state.get_timings()
}
#[tauri::command]
async fn read_timing_report(path: PathBuf) -> Result<TimingReport> {
    background(move || deadlock_core::timing::read_report(&path).map_err(AppError::from)).await
}
#[tauri::command]
fn get_log_entries(state: AppState<'_>) -> Vec<LogEntry> {
    state.log_entries()
}
#[tauri::command]
async fn get_log_text(state: AppState<'_>) -> Result<String> {
    let app = state.inner().clone();
    background(move || app.log_text()).await
}
#[tauri::command]
async fn save_log(state: AppState<'_>, path: PathBuf) -> Result<()> {
    let app = state.inner().clone();
    background(move || app.save_log(&path)).await
}
#[tauri::command]
async fn list_tool_logs(state: AppState<'_>) -> Result<Vec<crate::app::ToolLogFile>> {
    let app = state.inner().clone();
    background(move || app.tool_logs()).await
}
#[tauri::command]
async fn read_tool_log(
    state: AppState<'_>,
    path: PathBuf,
    offset: u64,
    limit: usize,
) -> Result<crate::app::ToolLogPage> {
    let app = state.inner().clone();
    background(move || app.read_tool_log(&path, offset, limit)).await
}
#[tauri::command]
async fn inspect_project(
    state: AppState<'_>,
    project: Project,
) -> Result<crate::workflows::ProjectInspection> {
    let app = state.inner().clone();
    background(move || app.inspect_project(project)).await
}
#[tauri::command]
async fn prepare_project(
    state: AppState<'_>,
    project: Project,
    offline: bool,
) -> Result<crate::workflows::ProjectPreparation> {
    let app = state.inner().clone();
    background(move || app.prepare_project(project, offline)).await
}
#[tauri::command]
async fn build_project(
    state: AppState<'_>,
    project: Project,
    overwrite_output: bool,
    overwrite_install: bool,
    project_path: Option<PathBuf>,
) -> Result<crate::workflows::ProjectBuild> {
    let app = state.inner().clone();
    background(move || {
        app.build_project(project, overwrite_output, overwrite_install, project_path)
    })
    .await
}
#[tauri::command]
async fn install_project(
    state: AppState<'_>,
    project: Project,
    path: PathBuf,
    overwrite: bool,
) -> Result<PathBuf> {
    let app = state.inner().clone();
    background(move || app.install_project(project, &path, overwrite)).await
}
#[tauri::command]
async fn validate_original(
    state: AppState<'_>,
    project: Project,
    compile: bool,
) -> Result<deadlock_pipeline::model_compile::OriginalValidation> {
    let app = state.inner().clone();
    background(move || app.validate_original(project, compile)).await
}

pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|host| {
            let config = host.path().app_config_dir()?;
            let resources = if cfg!(debug_assertions) {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .to_path_buf()
            } else {
                host.path().resource_dir()?
            };
            let application = Arc::new(Application::with_resources(config.clone(), resources)?);
            application.begin_session();
            let handle = host.handle().clone();
            application.set_log_sink(Arc::new(move |entry| {
                let _ = handle.emit("build-log", entry);
            }));
            let handle = host.handle().clone();
            application.set_timing_sink(Arc::new(move |event| {
                let _ = handle.emit("timing-event", event);
            }));
            // Copy legacy settings once into the new application's own location.
            // The original settings file remains the working reference's property.
            if !config.join("settings.json").exists() {
                if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                    let legacy = PathBuf::from(local).join("DeadlockModBuilder/settings.json");
                    if legacy.is_file() {
                        match Settings::load(&legacy) {
                            Ok(settings) => application.save_settings(&settings)?,
                            Err(error) => {
                                application.append_log(
                                    "warning",
                                    &format!("Existing settings could not be imported: {error}"),
                                )?;
                            }
                        }
                    }
                }
            }
            // The saved theme's icon from the first frame on.
            if let (Ok(settings), Some(window)) = (application.settings(), host.get_webview_window("main")) {
                apply_theme_icon(&window, &icon_for(&settings));
            }
            host.manage(application);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.state::<Arc<Application>>();
                if app.is_busy() {
                    api.prevent_close();
                    app.cancel();
                    let _ = app.append_log(
                        "info",
                        "Cancellation requested. Close the window after the operation has stopped.",
                    );
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            set_theme,
            save_theme_presets,
            detect_settings,
            get_setup_status,
            get_capabilities,
            check_setting_fields,
            hero_proportions,
            prebuild_warnings,
            operation_durations,
            fix_setting_fields,
            test_setup,
            motion_preview,
            autosave_project,
            clear_autosaves,
            session_recovery,
            dismiss_recovery,
            restore_autosave,
            physics_presets,
            save_physics_preset,
            delete_physics_preset,
            import_source_package,
            analyze_materials,
            new_project,
            open_project,
            save_project,
            analyze_dmx,
            preview_dmx,
            preview_source,
            get_heroes,
            select_hero,
            included_physics,
            validate_asset_path,
            cancel_operation,
            get_timing_report,
            read_timing_report,
            get_log_entries,
            get_log_text,
            save_log,
            list_tool_logs,
            read_tool_log,
            inspect_project,
            prepare_project,
            build_project,
            install_project,
            validate_original,
            crate::preparation::commands::preparation_inspect,
            crate::preparation::commands::preparation_suggest,
            crate::preparation::commands::preparation_save_mapping_preset,
            crate::preparation::commands::preparation_review,
            crate::preparation::commands::preparation_weapon_preview,
            crate::preparation::commands::preparation_convert,
            crate::preparation::commands::preparation_accept,
            crate::preparation::commands::preparation_resume,
            crate::preparation::commands::preparation_open_blend,
            crate::preparation::commands::outfit_save,
            crate::preparation::commands::outfit_load,
            crate::setup::commands::setup_defaults,
            crate::setup::commands::setup_manual_preview,
            crate::setup::commands::setup_manual_export,
            crate::setup::commands::setup_verify_files,
            crate::setup::commands::setup_acquire,
            crate::setup::commands::setup_validate_existing,
            crate::setup::commands::setup_use_toolset,
            crate::catalogs::commands::discover_components,
            crate::catalogs::commands::discover_extras,
            crate::catalogs::commands::discover_variants,
            crate::catalogs::commands::discover_weapon_models,
            crate::catalogs::commands::preview_image,
            crate::catalogs::commands::preview_extra,
            crate::diagnostics::commands::tool_candidates,
            crate::diagnostics::commands::tool_diagnostics,
            crate::diagnostics::commands::validate_blender,
            crate::diagnostics::commands::open_output_folder,
            crate::cloth_authoring::commands::skirt_analyze,
            crate::cloth_authoring::commands::skirt_author,
            crate::cloth_authoring::commands::cloth_setup,
            crate::vpk_slots::commands::get_vpk_slots,
            crate::vpk_slots::commands::choose_free_vpk_slot,
            crate::storage::commands::storage_report,
            crate::storage::commands::storage_clear,
            crate::storage::commands::storage_find_projects
        ])
        .build(tauri::generate_context!())
        .map(|app| {
            app.run(|handle, event| {
                // A normal exit; a crash leaves the session marker for recovery.
                if let tauri::RunEvent::Exit = event {
                    if let Some(application) = handle.try_state::<Arc<Application>>() {
                        application.end_session();
                    }
                }
            })
        });
    if let Err(error) = result {
        eprintln!("Deadlock Mod Builder could not start: {error}");
    }
}
