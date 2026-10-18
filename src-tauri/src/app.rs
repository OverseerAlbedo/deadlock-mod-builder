//! Application-owned state. The UI receives compact summaries and requests work;
//! filesystem operations and operation ownership remain native.
use chrono::Utc;
use deadlock_core::{CancellationToken, CoreError, TimingRecorder, TimingReport, TimingStatus};
use deadlock_dmx::{Dmx, DmxSummary, MAX_INPUT_BYTES};
use deadlock_project::{Project, Settings};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    time::Duration,
};

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    pub kind: String,
    pub message: String,
    pub details: String,
    /// What went wrong in plain words and what to do (see troubleshooting.rs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<crate::troubleshooting::Help>,
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.message, self.details)
    }
}
impl std::error::Error for AppError {}
impl AppError {
    pub(crate) fn new(kind: &str, message: &str, details: impl ToString) -> Self {
        let details = details.to_string();
        Self {
            kind: kind.into(),
            message: message.into(),
            help: crate::troubleshooting::help(kind, message, &details),
            details,
        }
    }
    pub(crate) fn io(message: &str, error: impl ToString) -> Self {
        Self::new("io", message, error)
    }
    pub(crate) fn validation(message: &str, error: impl ToString) -> Self {
        Self::new("validation", message, error)
    }
}
impl From<CoreError> for AppError {
    fn from(error: CoreError) -> Self {
        if matches!(error, CoreError::Cancelled) {
            Self::new("cancelled", "Operation cancelled", error)
        } else if let CoreError::ToolFailed { program, .. } = &error {
            let compiler = Path::new(program).file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .eq_ignore_ascii_case("resourcecompiler.exe")
            });
            Self::new(
                "tool",
                if compiler {
                    "Resource compiler failed"
                } else {
                    "External tool failed"
                },
                &error,
            )
        } else if let CoreError::Integrity(message) | CoreError::Unsupported(message) = &error {
            Self::new(
                "validation",
                message.lines().next().unwrap_or("Validation failed"),
                &error,
            )
        } else {
            Self::new("internal", "Operation could not be completed", error)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDocument {
    pub path: Option<String>,
    pub project: Project,
}
#[derive(Debug, Clone, Serialize)]
pub struct ModelSummary {
    pub path: String,
    #[serde(flatten)]
    pub summary: DmxSummary,
}
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResult {
    pub summary: ModelSummary,
    pub timings: TimingReport,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub sequence: u64,
    pub timestamp_utc: String,
    pub level: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct IncludedPhysics {
    pub source: String,
    pub hero_path: String,
    pub enabled: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct SetupStatus {
    pub files_found: std::collections::BTreeMap<String, bool>,
    pub model_status: String,
    pub model_files_found: bool,
    /// Every path field: right, wrong (with a suggested fix) or optional.
    pub fields: Vec<deadlock_tools::setup_checks::FieldCheck>,
}
pub type LogSink = Arc<dyn Fn(&LogEntry) + Send + Sync>;
pub type TimingSink = Arc<dyn Fn(&deadlock_core::TimingEvent) + Send + Sync>;
/// The Steam QR code DepotDownloader is waiting on, or none when its sign-in
/// is over.
pub type QrSink = Arc<dyn Fn(Option<&SteamQr>) + Send + Sync>;
/// A Steam sign-in QR code for the UI: one string per row, "1" dark, "0" light,
/// without the quiet zone.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SteamQr {
    pub rows: Vec<String>,
}

struct LogBook {
    entries: Vec<LogEntry>,
    writer: File,
    sequence: u64,
}
pub struct Application {
    pub state_directory: PathBuf,
    resource_directory: PathBuf,
    settings_path: PathBuf,
    log_path: PathBuf,
    log: Arc<Mutex<LogBook>>,
    sink: Arc<Mutex<Option<LogSink>>>,
    tool_directories: Mutex<Vec<PathBuf>>,
    timing_sink: Mutex<Option<TimingSink>>,
    qr_sink: Arc<Mutex<Option<QrSink>>>,
    active: Mutex<Option<CancellationToken>>,
    current_timings: Mutex<Option<TimingRecorder>>,
    last_timings: Mutex<Option<TimingReport>>,
    pub(crate) extras_catalog: Mutex<Option<crate::catalogs::PreviewCatalog>>,
}

/// A game model with what the installed game does with it.
#[derive(Debug, Clone, Serialize)]
pub struct HeroModel {
    #[serde(flatten)]
    pub model: deadlock_vpk::Model,
    /// Roster name of the hero this model belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hero: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<deadlock_components::heroes::GameTarget>,
}
fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
fn path_text(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        AppError::validation(
            "This path cannot be displayed losslessly",
            "Use a Unicode path supported by the project format.",
        )
    })
}

impl Application {
    pub fn new(state_directory: PathBuf) -> Result<Self> {
        Self::with_resources(
            state_directory,
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .to_path_buf(),
        )
    }
    pub fn with_resources(state_directory: PathBuf, resource_directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&state_directory)
            .map_err(|e| AppError::io("Could not create application settings folder", e))?;
        let log_path =
            state_directory.join(format!("session-{}.log", uuid::Uuid::new_v4().simple()));
        let writer = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&log_path)
            .map_err(|e| AppError::io("Could not create application log", e))?;
        Ok(Self {
            settings_path: state_directory.join("settings.json"),
            state_directory,
            resource_directory,
            log_path,
            log: Arc::new(Mutex::new(LogBook {
                entries: Vec::new(),
                writer,
                sequence: 0,
            })),
            sink: Arc::new(Mutex::new(None)),
            tool_directories: Mutex::new(Vec::new()),
            timing_sink: Mutex::new(None),
            qr_sink: Arc::new(Mutex::new(None)),
            active: Mutex::new(None),
            current_timings: Mutex::new(None),
            last_timings: Mutex::new(None),
            extras_catalog: Mutex::new(None),
        })
    }
    pub fn set_log_sink(&self, sink: LogSink) {
        *lock(&self.sink) = Some(sink);
    }
    pub fn set_timing_sink(&self, sink: TimingSink) {
        *lock(&self.timing_sink) = Some(sink);
    }
    pub fn set_qr_sink(&self, sink: QrSink) {
        *lock(&self.qr_sink) = Some(sink);
    }
    /// Shows DepotDownloader's Steam QR codes from `runner` until dropped.
    pub(crate) fn steam_qr_watcher(&self, runner: &deadlock_core::process::Runner) -> SteamQrWatcher {
        SteamQrWatcher::start(runner, self.qr_sink.clone())
    }
    pub fn is_busy(&self) -> bool {
        lock(&self.active).is_some()
    }
    pub fn cancel(&self) {
        if let Some(token) = lock(&self.active).as_ref() {
            token.cancel();
        }
    }
    pub fn settings(&self) -> Result<Settings> {
        Settings::load(&self.settings_path)
            .map_err(|e| AppError::validation("Settings could not be loaded", e))
    }
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let _lease = self.lease()?;
        self.persist_settings(settings)
    }
    // Call only while holding the application operation lease.
    pub(crate) fn persist_settings(&self, settings: &Settings) -> Result<()> {
        settings
            .save(&self.settings_path)
            .map_err(|e| AppError::io("Settings could not be saved", e))?;
        self.append_log("info", "Settings saved")
    }
    pub fn detect_settings(&self, settings: Settings, roots: Vec<PathBuf>) -> Result<Settings> {
        self.operation("Detect installed tools", |clock, token| {
            self.step(clock, "Find game and compiler paths", || {
                deadlock_tools::detection::detect(&settings, &roots, token).map_err(AppError::from)
            })
        })
        .map(|(settings, _)| settings)
    }
    /// Whether the included Vindicta cloth reference (a private asset) was packaged with this build.
    pub fn included_reference_available(&self) -> bool {
        self.resource_directory
            .join("physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c")
            .is_file()
    }
    pub fn setup_status(&self, settings: &Settings) -> SetupStatus {
        let model = deadlock_tools::toolchain::select(settings, true);
        SetupStatus {
            files_found: deadlock_tools::detection::status(settings),
            model_status: match &model {
                Ok(tool) => format!("Model compiler found ({}). Test it once with your hero in Set up model compiler….", tool.backend),
                Err(error) => error.to_string().trim_start_matches("Integrity check failed: ").to_owned(),
            },
            model_files_found: model.is_ok(),
            fields: deadlock_tools::setup_checks::field_checks(settings),
        }
    }
    pub fn import_source_package(&self, path: &Path, mut project: Project) -> Result<Project> {
        self.operation("Import source package", |clock, token| {
            let settings = self.settings()?;
            if settings.workspace.is_empty() {
                return Err(AppError::validation(
                    "Choose a workspace in Settings",
                    "Source packages need a workspace for verified extraction.",
                ));
            }
            let package = self.step(clock, "Extract and verify source package", || {
                deadlock_materials::import_zip(path, Path::new(&settings.workspace), token).map_err(
                    |e| {
                        if token.is_cancelled() {
                            CoreError::Cancelled.into()
                        } else {
                            AppError::validation("Source package could not be imported", e)
                        }
                    },
                )
            })?;
            token.check()?;
            project.dmx = package.dmx;
            project.expected_dmx_sha256 = package.expected_dmx_sha256;
            project.mesh_template = package.mesh_template;
            project.source_overlay = package.source_overlay;
            project.material_mapping = package.material_mapping;
            project.material_remap = package.material_remap;
            project.skeleton_reference = package.skeleton_reference;
            project.meshes.clear();
            project.materials.clear();
            project.lod_strategy = deadlock_project::LodStrategy::Choose;
            project.review_lods = false;
            project
                .validate_draft()
                .map_err(|e| AppError::validation("Imported project could not be validated", e))?;
            self.append_log(
                "info",
                "Source package verified; overlay and material identity files imported",
            )?;
            Ok(project)
        })
        .map(|(project, _)| project)
    }
    pub fn analyze_materials(&self, mut project: Project) -> Result<Project> {
        self.operation("Analyze materials", |clock, token| {
            let settings = self.settings()?;
            let (runner, _logs) = self.create_runner(clock, token)?;
            let log = |message: &str| {
                let _ = self.append_log("info", message);
            };
            let source = deadlock_pipeline::sources::SourceService {
                settings: &settings,
                runner: &runner,
                cancel: token,
                clock,
                log: &log,
            };
            let base = source.ensure_base(&project)?;
            let dmx = self.step(clock, "Read replacement material identities", || {
                Dmx::load_with_cancel(Path::new(&project.dmx), token).map_err(|e| {
                    if token.is_cancelled() {
                        CoreError::Cancelled.into()
                    } else {
                        AppError::validation("Model could not be read", e)
                    }
                })
            })?;
            let summary = dmx
                .summary_with_cancel(token)
                .map_err(|e| AppError::validation("Model materials could not be read", e))?;
            if !settings.pak.is_empty() && Path::new(&settings.pak).is_file() {
                let archive = deadlock_vpk::Vpk::open(&settings.pak)
                    .map_err(|e| AppError::validation("Game archive could not be read", e))?;
                for material in &summary.materials {
                    token.check()?;
                    if !project.source_overlay.is_empty()
                        && deadlock_core::safety::inside(
                            Path::new(&project.source_overlay),
                            material,
                        )?
                        .is_file()
                    {
                        continue;
                    }
                    if !deadlock_core::safety::inside(&base, material)?.is_file()
                        && archive
                            .entries
                            .contains_key(&deadlock_core::text::casefold(&format!("{material}_c")))
                    {
                        source.decompile(
                            Path::new(&settings.pak),
                            &format!("{material}_c"),
                            &base,
                            None,
                        )?;
                    }
                }
            }
            let root = deadlock_pipeline::workspace::root(&settings, &project)?;
            let overlay =
                (!project.source_overlay.is_empty()).then(|| Path::new(&project.source_overlay));
            let effective = self.step(clock, "Apply source overlay", || {
                deadlock_materials::effective_sources(
                    &base,
                    overlay,
                    &root.join("effective-sources"),
                    token,
                )
                .map_err(|e| AppError::validation("Source overlay could not be prepared", e))
            })?;
            let bindings = if Path::new(&project.preparation_blend).is_file() {
                self.step(clock, "Recover textures from prepared Blender materials", || {
                    let bridge = deadlock_preparation::blender::BlenderBridge {
                        executable: PathBuf::from(&settings.blender),
                        executor: Arc::new(runner.clone()),
                    };
                    match deadlock_preparation::material_recovery::recover(
                        &bridge,
                        Path::new(&project.preparation_blend),
                        &project.hero_root(),
                        &root.join("material-analysis"),
                        token,
                    ) {
                        Ok(recovered) => {
                            for warning in &recovered.warnings {
                                self.append_log("warning", warning)?;
                            }
                            Ok(recovered.materials)
                        }
                        Err(error) => {
                            token.check()?;
                            self.append_log("warning", &format!("Blender texture recovery was unavailable: {error}. Continuing with saved choices and the selected texture folder."))?;
                            Ok(Default::default())
                        }
                    }
                })?
            } else {
                Default::default()
            };
            let textures = if project.textures.is_empty() {
                root.join("no-textures-selected")
            } else {
                PathBuf::from(&project.textures)
            };
            let plans = self.step(clock, "Plan texture and material mappings", || {
                deadlock_materials::plan_with_bindings(
                    &summary.materials,
                    &effective,
                    &textures,
                    &project.materials,
                    &bindings,
                    token,
                )
                .map_err(|e| AppError::validation("Materials could not be planned", e))
            })?;
            project.materials = deadlock_materials::apply_materials(&plans, &project.customization)
                .map_err(|e| AppError::validation("Material overrides could not be applied", e))?;
            Ok(project)
        })
        .map(|(project, _)| project)
    }
    /// The Physics page's movement preview: the prepared skeleton and the
    /// hero's idle, run and jump clips (see `deadlock_pipeline::motion_preview`).
    pub fn motion_preview(&self, project: Project) -> Result<Vec<u8>> {
        self.operation("Load movement preview", |_, token| {
            let settings = self.settings()?;
            deadlock_pipeline::motion_preview::build(&settings, &project, token).map_err(AppError::from)
        })
        .map(|(packet, _)| packet)
    }
    pub fn new_project(&self) -> Result<ProjectDocument> {
        let _lease = self.lease()?;
        Ok(ProjectDocument {
            path: None,
            project: Project::default(),
        })
    }
    pub fn open_project(&self, path: &Path) -> Result<ProjectDocument> {
        self.operation("Open project", |clock, token| {
            token.check()?;
            let project = self.step(clock, "Read and resolve project assets", || {
                Project::load(path)
                    .map_err(|e| AppError::validation("Project could not be opened", e))
            })?;
            token.check()?;
            self.remember_project(path, &project);
            Ok(ProjectDocument {
                path: Some(path_text(path)?),
                project,
            })
        })
        .map(|(document, _)| document)
    }
    pub fn save_project(&self, path: &Path, mut project: Project) -> Result<ProjectDocument> {
        self.operation("Save project", |clock, token| {
            token.check()?;
            self.step(clock, "Validate and save project", || {
                project
                    .save(path)
                    .map_err(|e| AppError::validation("Project could not be saved", e))
            })?;
            self.remember_project(path, &project);
            Ok(ProjectDocument {
                path: Some(path_text(path)?),
                project,
            })
        })
        .map(|(document, _)| document)
    }
    pub fn analyze_dmx(&self, path: &Path) -> Result<AnalysisResult> {
        let (summary, timings) = self.operation("Analyze model", |clock, token| {
            token.check()?;
            let bytes = self.step(clock, "Read DMX", || {
                let file =
                    File::open(path).map_err(|e| AppError::io("Model could not be read", e))?;
                let metadata = file
                    .metadata()
                    .map_err(|e| AppError::io("Model could not be inspected", e))?;
                if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES as u64 {
                    return Err(AppError::validation(
                        "Model could not be read",
                        "DMX must be a regular file no larger than 512 MiB.",
                    ));
                }
                let mut bytes = Vec::new();
                file.take(MAX_INPUT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| AppError::io("Model could not be read", e))?;
                if bytes.len() > MAX_INPUT_BYTES {
                    return Err(AppError::validation(
                        "Model grew while reading",
                        "Input exceeds the 512 MiB limit.",
                    ));
                }
                Ok(bytes)
            })?;
            token.check()?;
            let dmx = self.step(clock, "Parse DMX", || {
                Dmx::parse_with_cancel(bytes, token).map_err(|e| {
                    if token.is_cancelled() {
                        AppError::from(CoreError::Cancelled)
                    } else {
                        AppError::validation("Model format could not be read", e)
                    }
                })
            })?;
            token.check()?;
            let summary = self.step(clock, "Inspect meshes, materials and bones", || {
                dmx.summary_with_cancel(token).map_err(|e| {
                    if token.is_cancelled() {
                        AppError::from(CoreError::Cancelled)
                    } else {
                        AppError::validation("Model structure could not be inspected", e)
                    }
                })
            })?;
            token.check()?;
            Ok(ModelSummary {
                path: path_text(path)?,
                summary,
            })
        })?;
        Ok(AnalysisResult { summary, timings })
    }
    pub fn preview_dmx(&self, path: &Path) -> Result<Vec<u8>> {
        self.operation("Load model preview", |clock, token| {
            deadlock_preview::load_packet(path, token, clock).map_err(AppError::from)
        })
        .map(|(packet, _)| packet)
    }
    pub fn preview_source(&self, path: &Path) -> Result<Vec<u8>> {
        self.operation("Load source model preview", |clock, token| {
            let settings = self.settings()?;
            if path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| {
                    value.eq_ignore_ascii_case("blend") || value.eq_ignore_ascii_case("fbx")
                })
            {
                deadlock_preparation::blender::check_executable(Path::new(&settings.blender))
                    .map_err(|error| crate::preparation::failure(error, token))?;
            }
            let (runner, _logs) = self.create_runner(clock, token)?;
            let workspace = self
                .state_directory
                .join("operations")
                .join(clock.report().run_id)
                .join("preview");
            deadlock_preview::gltf::load_source_packet(
                path,
                Path::new(&settings.blender),
                &workspace,
                &runner,
                token,
                clock,
            )
            .map_err(AppError::from)
        })
        .map(|(packet, _)| packet)
    }
    pub fn heroes(&self, all_models: bool) -> Result<Vec<HeroModel>> {
        self.operation("Read game model catalog", |clock, token| {
            let settings = self.settings()?;
            if settings.pak.is_empty() {
                return Err(AppError::validation(
                    "Choose the game archive in Settings",
                    "Select Deadlock's pak01_dir.vpk to list its actual models.",
                ));
            }
            token.check()?;
            let archive = self.step(clock, "Read VPK directory", || {
                deadlock_vpk::Vpk::open(&settings.pak)
                    .map_err(|e| AppError::validation("Game archive could not be read", e))
            })?;
            token.check()?;
            let models = self.step(clock, "List game models", || Ok(archive.models(all_models)))?;
            // Hero definitions say which model the game loads for each hero. A
            // missing or unreadable definition only removes this extra detail.
            let game = self.step(clock, "Read hero model choices of the game", || {
                Ok(deadlock_components::heroes::GameModels::read(&archive).unwrap_or_else(|error| {
                    let _ = self.append_log("warning", &format!("Hero definitions could not be read: {error}"));
                    None
                }))
            })?;
            Ok(models
                .into_iter()
                .map(|model| {
                    let target = game.as_ref().and_then(|g| g.target(&model.path).ok().flatten());
                    HeroModel { hero: deadlock_components::heroes::display_name(&model.path), game: target, model }
                })
                .collect())
        })
        .map(|(models, _)| models)
    }
    pub fn select_hero(&self, mut project: Project, path: &str) -> Result<Project> {
        let models = self.heroes(true)?;
        let selected = models
            .iter()
            .find(|record| record.model.path == path)
            .ok_or_else(|| {
                AppError::validation(
                    "Select an existing game model",
                    "The selected model is not in the configured archive.",
                )
            })?;
        project.change_hero(
            selected.hero.clone().unwrap_or_else(|| selected.model.name.clone()),
            selected.model.path.clone(),
        );
        project
            .validate_draft()
            .map_err(|e| AppError::validation("Project could not be updated", e))?;
        Ok(project)
    }
    pub fn included_physics(&self, hero_path: &str) -> Result<IncludedPhysics> {
        const HERO: &str = "models/heroes_staging/hornet_v3/hornet.vmdl";
        const SHA: &str = "27dd44ea4db29f671cff53d0dd8ad171792fe5f12f45b05aca5db65e0a6b5b0c";
        if hero_path.strip_suffix("_c").unwrap_or(hero_path) != HERO {
            return Err(AppError::validation("Select Vindicta to use the included cloth setup", "This reference is qualified for hornet_v3/hornet.vmdl. Other heroes need their own cloth setup."));
        }
        self.operation("Verify included cloth reference", |clock, token| {
            let path = self.resource_directory.join(
                "physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c",
            );
            let hash = self.step(clock, "Hash accepted Vindicta reference", || {
                deadlock_core::safety::digest_with_cancel(&path, token).map_err(AppError::from)
            })?;
            if hash != SHA {
                return Err(AppError::validation(
                    "Included cloth reference is missing or changed",
                    "Restore the complete application package. No altered reference was selected.",
                ));
            }
            Ok(IncludedPhysics {
                source: path_text(&path)?,
                hero_path: hero_path.into(),
                enabled: true,
            })
        })
        .map(|(selection, _)| selection)
    }
    pub fn get_timings(&self) -> Option<TimingReport> {
        let current = lock(&self.current_timings).clone();
        current
            .map(|clock| clock.report())
            .or_else(|| lock(&self.last_timings).clone())
    }
    fn durations_path(&self) -> PathBuf {
        self.state_directory.join("operation-durations.json")
    }
    /// Remember how long a successful operation took (the last 5 per name), for
    /// the time-left estimate of the next run.
    fn record_duration(&self, name: &str, seconds: f64) {
        let path = self.durations_path();
        let mut all: std::collections::BTreeMap<String, Vec<f64>> = fs::read(&path)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        let list = all.entry(name.to_owned()).or_default();
        list.push(seconds);
        if list.len() > 5 {
            list.drain(..list.len() - 5);
        }
        let _ = deadlock_core::safety::write_json(&path, &all);
    }
    /// Typical (median) seconds of each operation's recent successful runs.
    pub fn operation_durations(&self) -> std::collections::BTreeMap<String, f64> {
        let all: std::collections::BTreeMap<String, Vec<f64>> = fs::read(self.durations_path())
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        all.into_iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, mut v)| {
                v.sort_by(f64::total_cmp);
                (k, v[v.len() / 2])
            })
            .collect()
    }
    /// Common causes of failed builds, checked before Build starts.
    pub fn prebuild_warnings(&self) -> Result<Vec<String>> {
        Ok(deadlock_tools::setup_checks::prebuild_warnings(&self.settings()?))
    }
    /// Every VPK in the game's addons folder with the state of its hero models
    /// after the game's October 16, 2026 attachment change.
    pub fn installed_mods(&self) -> Result<Vec<deadlock_pipeline::mod_update::InstalledMod>> {
        self.operation("Check installed mods", |_, token| {
            let settings = self.settings()?;
            deadlock_pipeline::mod_update::scan(&settings, token).map_err(AppError::from)
        })
        .map(|(mods, _)| mods)
    }
    /// Rewrites the hero models of these installed VPKs to the game's current
    /// attachment convention; the previous files are kept under the workspace.
    pub fn update_installed_mods(&self, paths: Vec<PathBuf>) -> Result<Vec<deadlock_pipeline::mod_update::UpdateReport>> {
        self.operation("Update installed mods", |_, token| {
            let settings = self.settings()?;
            let backups = deadlock_project::asset_paths::resolve(Path::new(&settings.workspace))
                .map_err(|e| AppError::validation("Invalid workspace folder", e))?
                .join("mod-updates")
                .join("backups");
            let log = |message: &str| {
                let _ = self.append_log("info", message);
            };
            paths
                .iter()
                .map(|path| {
                    deadlock_pipeline::mod_update::update(&settings, path, &backups, settings.keep_backups as usize, token, &log)
                        .map_err(AppError::from)
                })
                .collect()
        })
        .map(|(reports, _)| reports)
    }
    pub fn log_entries(&self) -> Vec<LogEntry> {
        lock(&self.log).entries.clone()
    }
    pub fn log_text(&self) -> Result<String> {
        lock(&self.log)
            .writer
            .flush()
            .map_err(|e| AppError::io("Log could not be flushed", e))?;
        fs::read_to_string(&self.log_path).map_err(|e| AppError::io("Log could not be read", e))
    }
    pub fn save_log(&self, path: &Path) -> Result<()> {
        self.export_full_log(path)
    }
    pub fn append_log(&self, level: &str, message: &str) -> Result<()> {
        append_shared(&self.log, &self.sink, level, message)
    }
    pub(crate) fn native_reader_root(&self) -> PathBuf {
        let packaged = self.resource_directory.join("native/cloth-reader");
        if packaged.is_dir() {
            packaged
        } else {
            self.resource_directory
                .join("resources/native/cloth-reader")
        }
    }
    pub(crate) fn create_runner(
        &self,
        clock: &TimingRecorder,
        token: &CancellationToken,
    ) -> Result<(deadlock_core::process::Runner, ToolLogObserver)> {
        self.create_runner_with_options(clock, token, Default::default())
    }
    pub(crate) fn create_runner_with_options(
        &self,
        clock: &TimingRecorder,
        token: &CancellationToken,
        options: deadlock_core::process::RunnerOptions,
    ) -> Result<(deadlock_core::process::Runner, ToolLogObserver)> {
        let directory = self
            .state_directory
            .join("operations")
            .join(clock.report().run_id)
            .join("tools");
        let runner = deadlock_core::process::Runner::new(&directory, token.clone(), options)?;
        lock(&self.tool_directories).push(directory.clone());
        self.append_log(
            "info",
            &format!("Full tool output: {}", directory.display()),
        )?;
        let observer = ToolLogObserver::start(&runner, self.log.clone(), self.sink.clone());
        Ok((runner, observer))
    }
    fn lease(&self) -> Result<OperationLease<'_>> {
        // A short operation that is just finishing (a slot scan, a save) should
        // not make the next one fail: wait up to two seconds for the lock.
        let mut active = lock(&self.active);
        for _ in 0..40 {
            if active.is_none() {
                break;
            }
            drop(active);
            std::thread::sleep(std::time::Duration::from_millis(50));
            active = lock(&self.active);
        }
        if active.is_some() {
            return Err(AppError::new(
                "busy",
                "Another operation is running",
                "Cancel it or wait for it to finish.",
            ));
        }
        let token = CancellationToken::new();
        *active = Some(token.clone());
        Ok(OperationLease { owner: self, token })
    }
    pub(crate) fn step<T>(
        &self,
        clock: &TimingRecorder,
        name: &str,
        work: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let guard = clock.start_step(name, None)?;
        self.append_log("info", name)?;
        match work() {
            Ok(value) => {
                guard.success()?;
                Ok(value)
            }
            Err(error) => {
                let _ = guard.finish(
                    if error.kind == "cancelled" {
                        TimingStatus::Cancelled
                    } else {
                        TimingStatus::Failed
                    },
                    Some(error.to_string()),
                );
                Err(error)
            }
        }
    }
    pub(crate) fn operation<T>(
        &self,
        name: &str,
        work: impl FnOnce(&TimingRecorder, &CancellationToken) -> Result<T>,
    ) -> Result<(T, TimingReport)> {
        let lease = self.lease()?;
        let clock = TimingRecorder::new(name);
        *lock(&self.current_timings) = Some(clock.clone());
        let directory = self
            .state_directory
            .join("operations")
            .join(&clock.report().run_id);
        std::fs::create_dir_all(&directory)
            .map_err(|e| AppError::io("Could not create operation evidence folder", e))?;
        clock.enable_journal(&directory.join("timing-events.jsonl"))?;
        let _observer = TimingObserver::start(&clock, lock(&self.timing_sink).clone());
        self.append_log("info", &format!("{name} started"))?;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&clock, &lease.token)))
                .unwrap_or_else(|panic| {
                    Err(AppError::new(
                        "internal",
                        "The operation stopped unexpectedly",
                        panic
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| panic.downcast_ref::<&str>().copied())
                            .unwrap_or("Native worker panic; operation evidence was retained."),
                    ))
                });
        let status = match &result {
            Ok(_) => TimingStatus::Succeeded,
            Err(e) if e.kind == "cancelled" => TimingStatus::Cancelled,
            Err(_) => TimingStatus::Failed,
        };
        let report = clock.finish(status, result.as_ref().err().map(ToString::to_string))?;
        *lock(&self.last_timings) = Some(report.clone());
        if result.is_ok() {
            self.record_duration(name, report.total_ns as f64 / 1e9);
        }
        let evidence = clock.write_json(&directory.join("timings.json"));
        for step in &report.steps {
            let _ = self.append_log(
                "info",
                &format!(
                    "Step time: {} — {:.3}s [{:?}]",
                    step.name,
                    step.inclusive_ns as f64 / 1e9,
                    step.status
                ),
            );
        }
        if let Err(error) = &result {
            let _ = self.append_log("error", &error.to_string());
        }
        let _ = self.append_log(
            if result.is_ok() { "info" } else { "error" },
            &format!("{name}: {status:?} in {:.3}s", report.total_ns as f64 / 1e9),
        );
        match evidence {
            Ok(()) => {
                let _ = self.append_log(
                    "info",
                    &format!(
                        "Timing report: {}",
                        directory.join("timings.json").display()
                    ),
                );
            }
            Err(error) => {
                let _ = self.append_log(
                    "warning",
                    &format!("Timing evidence could not be saved: {error}"),
                );
            }
        }
        result.map(|value| (value, report))
    }
}
fn append_shared(
    log: &Mutex<LogBook>,
    sink: &Mutex<Option<LogSink>>,
    level: &str,
    message: &str,
) -> Result<()> {
    let entry = {
        let mut book = lock(log);
        let entry = LogEntry {
            sequence: book.sequence,
            timestamp_utc: Utc::now().to_rfc3339(),
            level: level.into(),
            // The user's profile folder never reaches the log (logs are shared).
            message: crate::privacy::scrub(message),
        };
        writeln!(book.writer, "{}  {}", entry.timestamp_utc, entry.message)
            .and_then(|_| book.writer.flush())
            .map_err(|e| AppError::io("Log could not be saved", e))?;
        book.sequence += 1;
        if book.entries.len() >= 20_000 {
            book.entries.drain(..5_000);
        }
        book.entries.push(entry.clone());
        entry
    };
    if let Some(sink) = lock(sink).clone() {
        sink(&entry);
    }
    Ok(())
}
struct OperationLease<'a> {
    owner: &'a Application,
    token: CancellationToken,
}
impl Drop for OperationLease<'_> {
    fn drop(&mut self) {
        *lock(&self.owner.current_timings) = None;
        *lock(&self.owner.active) = None;
    }
}
struct TimingObserver {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl TimingObserver {
    fn start(clock: &TimingRecorder, sink: Option<TimingSink>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker = sink.and_then(|sink| {
            let receiver = clock.subscribe(256);
            let done = stop.clone();
            thread::Builder::new()
                .name("dmb-stage-events".into())
                .spawn(move || {
                    while !done.load(Ordering::Acquire) {
                        if let Ok(event) = receiver.recv_timeout(Duration::from_millis(25)) {
                            sink(&event);
                        }
                    }
                    for event in receiver.try_iter() {
                        sink(&event);
                    }
                })
                .ok()
        });
        Self { stop, worker }
    }
}
impl Drop for TimingObserver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(crate) struct ToolLogObserver {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl ToolLogObserver {
    fn start(
        runner: &deadlock_core::process::Runner,
        log: Arc<Mutex<LogBook>>,
        sink: Arc<Mutex<Option<LogSink>>>,
    ) -> Self {
        let receiver = runner.subscribe(128);
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = thread::Builder::new()
            .name("dmb-tool-events".into())
            .spawn(move || {
                let mut seen = std::collections::BTreeMap::<String, u64>::new();
                let mut receive = |event: deadlock_core::process::LogEvent| {
                    let stream = match event.stream {
                        deadlock_core::process::OutputStream::Stdout => "stdout",
                        deadlock_core::process::OutputStream::Stderr => "stderr",
                    };
                    let key = format!("{} {stream}", event.command_id);
                    let count = seen.entry(key.clone()).or_default();
                    if *count < 64 * 1024 {
                        let _ = append_shared(
                            &log,
                            &sink,
                            "tool",
                            &format!("[{key}] {}", event.preview),
                        );
                        if *count + event.byte_count >= 64 * 1024 {
                            let _ = append_shared(
                                &log,
                                &sink,
                                "info",
                                &format!(
                                    "[{key}] Remaining output is available in the full tool log."
                                ),
                            );
                        }
                    }
                    *count = (*count + event.byte_count).min(64 * 1024 + 1);
                };
                while !done.load(Ordering::Acquire) {
                    if let Ok(event) = receiver.recv_timeout(Duration::from_millis(25)) {
                        receive(event);
                    }
                }
                for event in receiver.try_iter() {
                    receive(event);
                }
            })
            .ok();
        Self { stop, worker }
    }
}
impl Drop for ToolLogObserver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
/// Reads DepotDownloader's output during a QR sign-in and sends the code to
/// the UI as it appears, renews and ends. The app runs DepotDownloader without
/// a console, so the code is otherwise only in Logs, where the block
/// characters arrive as unreadable replacement characters.
pub(crate) struct SteamQrWatcher {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl SteamQrWatcher {
    /// Output read per run before giving up on finding a code in it.
    const SEARCH_BYTES: usize = 256 * 1024;
    fn start(runner: &deadlock_core::process::Runner, sink: Arc<Mutex<Option<QrSink>>>) -> Self {
        use deadlock_core::process::OutputStream;
        use deadlock_tools::steam_qr::{self, SignIn};
        let receiver = runner.subscribe(512);
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = thread::Builder::new()
            .name("dmb-steam-qr".into())
            .spawn(move || {
                let mut output = std::collections::BTreeMap::<String, String>::new();
                let mut finished = std::collections::BTreeSet::<String>::new();
                let mut shown: Option<SteamQr> = None;
                let mut show = |code: Option<SteamQr>| {
                    if code != shown {
                        if let Some(sink) = lock(&sink).clone() {
                            sink(code.as_ref());
                        }
                        shown = code;
                    }
                };
                let mut receive = |event: deadlock_core::process::LogEvent| {
                    if finished.contains(&event.command_id) {
                        return;
                    }
                    let over = match event.stream {
                        // DepotDownloader reports a failed sign-in on stderr.
                        OutputStream::Stderr => !event.preview.trim().is_empty(),
                        OutputStream::Stdout => {
                            let text = output.entry(event.command_id.clone()).or_default();
                            text.push_str(&event.preview);
                            match steam_qr::read(text) {
                                SignIn::Code(rows) => {
                                    show(Some(SteamQr {
                                        rows: rows.iter().map(|row| row.iter().map(|d| if *d { '1' } else { '0' }).collect()).collect(),
                                    }));
                                    false
                                }
                                SignIn::Over => true,
                                SignIn::Unknown => text.len() > Self::SEARCH_BYTES,
                            }
                        }
                    };
                    if over {
                        output.remove(&event.command_id);
                        finished.insert(event.command_id);
                        show(None);
                    }
                };
                while !done.load(Ordering::Acquire) {
                    if let Ok(event) = receiver.recv_timeout(Duration::from_millis(25)) {
                        receive(event);
                    }
                }
                drop(receive);
                show(None);
            })
            .ok();
        Self { stop, worker }
    }
}
impl Drop for SteamQrWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ToolLogFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct ToolLogPage {
    pub text: String,
    pub next_offset: u64,
    pub eof: bool,
}
impl Application {
    pub fn tool_logs(&self) -> Result<Vec<ToolLogFile>> {
        let mut results = Vec::new();
        for directory in lock(&self.tool_directories).iter() {
            for entry in
                fs::read_dir(directory).map_err(|e| AppError::io("Could not list tool logs", e))?
            {
                let path = entry
                    .map_err(|e| AppError::io("Could not list tool logs", e))?
                    .path();
                deadlock_vpk::check_unlinked_path(&path, false)
                    .map_err(|e| AppError::validation("Unsafe tool log path", e))?;
                let kind = match path.extension().and_then(|s| s.to_str()) {
                    Some("bin") => "output",
                    Some("json") => "receipt",
                    _ => continue,
                };
                let metadata = fs::metadata(&path)
                    .map_err(|e| AppError::io("Could not inspect tool log", e))?;
                if metadata.is_file() {
                    results.push(ToolLogFile {
                        path,
                        bytes: metadata.len(),
                        kind: kind.into(),
                    });
                }
            }
        }
        results.sort_by(|a, b| a.path.cmp(&b.path));
        results.dedup_by(|a, b| a.path == b.path);
        Ok(results)
    }
    pub fn read_tool_log(&self, path: &Path, offset: u64, limit: usize) -> Result<ToolLogPage> {
        use std::io::{Seek, SeekFrom};
        if !self.tool_logs()?.iter().any(|item| item.path == path) {
            return Err(AppError::validation(
                "Unknown tool log",
                "Choose a log from this session's tool output list.",
            ));
        }
        let mut input = File::open(path).map_err(|e| AppError::io("Could not read tool log", e))?;
        let size = input
            .metadata()
            .map_err(|e| AppError::io("Could not inspect tool log", e))?
            .len();
        if offset > size {
            return Err(AppError::validation(
                "Invalid log offset",
                "The requested offset exceeds the file length.",
            ));
        }
        input
            .seek(SeekFrom::Start(offset))
            .map_err(|e| AppError::io("Could not seek tool log", e))?;
        let mut raw = Vec::new();
        input
            .take(limit.clamp(1, 256 * 1024) as u64)
            .read_to_end(&mut raw)
            .map_err(|e| AppError::io("Could not read tool log", e))?;
        let next_offset = offset + raw.len() as u64;
        Ok(ToolLogPage {
            text: String::from_utf8_lossy(&raw).into_owned(),
            next_offset,
            eof: next_offset >= size,
        })
    }
    fn export_full_log(&self, path: &Path) -> Result<()> {
        let logs = self.tool_logs()?;
        deadlock_vpk::check_unlinked_path(path, true)
            .map_err(|e| AppError::validation("Unsafe log destination", e))?;
        let resolved = deadlock_project::asset_paths::resolve(path)
            .map_err(|e| AppError::validation("Invalid log destination", e))?;
        if resolved == self.log_path || logs.iter().any(|entry| entry.path == resolved) {
            return Err(AppError::validation(
                "Choose a different log destination",
                "Active evidence files cannot be overwritten by an export.",
            ));
        }
        let parent = resolved.parent().ok_or_else(|| {
            AppError::validation("Invalid log destination", "Choose a file path.")
        })?;
        fs::create_dir_all(parent).map_err(|e| AppError::io("Could not create log folder", e))?;
        let mut output = tempfile::Builder::new()
            .prefix(".dmb-log-")
            .tempfile_in(parent)
            .map_err(|e| AppError::io("Could not create log export", e))?;
        lock(&self.log)
            .writer
            .flush()
            .map_err(|e| AppError::io("Could not flush session log", e))?;
        std::io::copy(
            &mut File::open(&self.log_path)
                .map_err(|e| AppError::io("Could not read session log", e))?,
            &mut output,
        )
        .map_err(|e| AppError::io("Could not export session log", e))?;
        for entry in logs {
            writeln!(
                output,
                "\n--- Full tool evidence: {} ({} bytes) ---",
                entry.path.display(),
                entry.bytes
            )
            .map_err(|e| AppError::io("Could not write tool log header", e))?;
            let mut raw = vec![];
            File::open(&entry.path)
                .map_err(|e| AppError::io("Could not read tool log", e))?
                .take(entry.bytes)
                .read_to_end(&mut raw)
                .map_err(|e| AppError::io("Could not read tool log", e))?;
            // Tool output can name the user's profile folder; exports are shared.
            output
                .write_all(crate::privacy::scrub(&String::from_utf8_lossy(&raw)).as_bytes())
                .map_err(|e| AppError::io("Could not export tool log", e))?;
        }
        output
            .flush()
            .and_then(|_| output.as_file().sync_all())
            .map_err(|e| AppError::io("Could not flush log export", e))?;
        output
            .persist(&resolved)
            .map_err(|e| AppError::io("Could not publish log export", e.error))?;
        Ok(())
    }
}

#[cfg(test)]
mod log_tests {
    use super::*;
    /// A short operation still finishing (a slot scan, a save) must not make
    /// the next one fail with "Another operation is running".
    #[test]
    fn the_next_operation_waits_for_a_short_one_to_finish() {
        let folder = tempfile::tempdir().unwrap();
        let app = Arc::new(Application::new(folder.path().into()).unwrap());
        let holder = app.clone();
        let (started, wait) = std::sync::mpsc::channel();
        let running = std::thread::spawn(move || {
            holder.operation("short", |_, _| { started.send(()).unwrap(); std::thread::sleep(std::time::Duration::from_millis(300)); Ok(()) }).map(|_| ())
        });
        wait.recv().unwrap();
        assert!(app.operation("next", |_, _| Ok(())).is_ok());
        running.join().unwrap().unwrap();
    }
    #[test]
    fn panic_finalizes_evidence_and_releases_operation() {
        let folder = tempfile::tempdir().unwrap();
        let app = Application::new(folder.path().into()).unwrap();
        let result: Result<((), TimingReport)> = app.operation("panic fixture", |clock, _| {
            let _span = clock.start_step("failing stage", None)?;
            panic!("deliberate fixture panic")
        });
        assert!(result.is_err());
        assert!(!app.is_busy());
        let report = app.get_timings().unwrap();
        assert_eq!(report.status, TimingStatus::Failed);
        assert!(folder
            .path()
            .join("operations")
            .join(report.run_id)
            .join("timings.json")
            .is_file());
        assert!(app.log_text().unwrap().contains("deliberate fixture panic"));
        assert!(app.new_project().is_ok());
    }
    /// DepotDownloader's QR code reaches the UI from a real tool run: the
    /// console code page's block bytes (0xDB in code page 437) read back as
    /// replacement characters, the code is shown, then cleared when the
    /// sign-in moves on or the operation ends (issue #2).
    #[cfg(windows)]
    #[test]
    fn a_steam_qr_code_printed_by_a_tool_is_shown_and_cleared() {
        let size = 21;
        let mut rows = vec![vec![false; size]; size];
        for (top, left) in [(0, 0), (0, size - 7), (size - 7, 0)] {
            for r in 0..7 {
                for c in 0..7 {
                    rows[top + r][left + c] = r == 0 || r == 6 || c == 0 || c == 6 || ((2..=4).contains(&r) && (2..=4).contains(&c));
                }
            }
        }
        rows[10][13] = true;
        let mut bytes = b"Connecting to Steam3... Done!\r\nLogging in with QR code...\r\nUse the Steam Mobile App to sign in with this QR code:\r\n".to_vec();
        let blank = vec![b' '; (size + 8) * 2];
        for _ in 0..4 {
            bytes.extend(&blank);
            bytes.extend(b"\r\n");
        }
        for row in &rows {
            bytes.extend(b"        ");
            for dark in row {
                bytes.extend(if *dark { [0xDB, 0xDB] } else { [b' ', b' '] });
            }
            bytes.extend(b"        \r\n");
        }
        for _ in 0..4 {
            bytes.extend(&blank);
            bytes.extend(b"\r\n");
        }
        let folder = tempfile::tempdir().unwrap();
        let waiting = folder.path().join("waiting.txt");
        fs::write(&waiting, &bytes).unwrap();
        let app = Application::new(folder.path().join("state")).unwrap();
        let seen = Arc::new(Mutex::new(Vec::<Option<SteamQr>>::new()));
        let record = seen.clone();
        app.set_qr_sink(Arc::new(move |code| lock(&record).push(code.cloned())));
        let expected = SteamQr { rows: rows.iter().map(|r| r.iter().map(|d| if *d { '1' } else { '0' }).collect()).collect() };
        let cmd = std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into());
        for ends in ["operation", "output"] {
            lock(&seen).clear();
            app.operation("qr fixture", |clock, token| {
                let (runner, _observer) = app.create_runner(clock, token)?;
                let watcher = app.steam_qr_watcher(&runner);
                let mut spec = deadlock_core::process::CommandSpec::new(&cmd).args(["/d", "/c", "type"]).arg(&waiting);
                if ends == "output" {
                    // Signed in a moment later.
                    spec = spec.args(["&", "ping", "-n", "2", "127.0.0.1", ">nul", "&", "echo", "Done!"]);
                }
                runner.run(&spec)?;
                std::thread::sleep(Duration::from_millis(200));
                if ends == "output" {
                    // Signed in: cleared by the tool's own output.
                    assert_eq!(*lock(&seen), vec![Some(expected.clone()), None]);
                } else {
                    assert_eq!(*lock(&seen), vec![Some(expected.clone())]);
                }
                drop(watcher);
                Ok(())
            })
            .unwrap();
            // Every sign-in ends with the code cleared.
            assert_eq!(lock(&seen).last(), Some(&None), "{ends}");
        }
    }
    #[cfg(unix)]
    #[test]
    fn raw_tool_logs_are_paged_and_exported_without_losing_bytes() {
        let folder = tempfile::tempdir().unwrap();
        let app = Application::new(folder.path().join("state")).unwrap();
        app.operation("tool fixture", |clock, token| {
            let (runner, _observer) = app.create_runner(clock, token)?;
            app.step(clock, "run tool", || {
                runner.run(&deadlock_core::process::CommandSpec::new("/bin/sh").args([
                    "-c",
                    "printf 'hello raw stdout'; printf 'diagnostic stderr' >&2",
                ]))?;
                Ok(())
            })
        })
        .unwrap();
        let logs = app.tool_logs().unwrap();
        assert_eq!(logs.len(), 3);
        let stdout = logs
            .iter()
            .find(|p| p.path.to_string_lossy().ends_with("stdout.bin"))
            .unwrap();
        let first = app.read_tool_log(&stdout.path, 0, 5).unwrap();
        assert_eq!(first.text, "hello");
        assert!(!first.eof);
        assert_eq!(
            app.read_tool_log(&stdout.path, first.next_offset, 100)
                .unwrap()
                .text,
            " raw stdout"
        );
        assert!(app
            .read_tool_log(&folder.path().join("unknown.log"), 0, 10)
            .is_err());
        assert!(app.save_log(&stdout.path).is_err());
        let output = folder.path().join("export.log");
        app.save_log(&output).unwrap();
        let text = fs::read_to_string(output).unwrap();
        assert!(text.contains("hello raw stdout"));
        assert!(text.contains("diagnostic stderr"));
        assert!(text.contains("Step time: run tool"));
    }
}
