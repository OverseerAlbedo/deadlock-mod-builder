use crate::app::{
    AnalysisResult, AppError, Application, IncludedPhysics, LogEntry, ProjectDocument, Result,
    SetupStatus,
};
use deadlock_core::TimingReport;
use deadlock_project::{Project, Settings};
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};

type AppState<'a> = State<'a, Arc<Application>>;

async fn background<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| AppError {
            kind: "internal".into(),
            message: "The operation stopped unexpectedly".into(),
            details: error.to_string(),
        })?
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
            detect_settings,
            get_setup_status,
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
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("Deadlock Mod Builder could not start: {error}");
    }
}
