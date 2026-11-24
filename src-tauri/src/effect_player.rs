//! The effect player of Extras → Effects: one hidden helper process
//! (`Dmb.EffectPlayer.exe serve`, `tools/effect-player/README.md`) that plays
//! the selected effect and streams JPEG frames. This module owns that process
//! and speaks its protocol; the page drives it through a few commands and
//! gets frames as raw bytes (`EffectPlayer::frame_after`) and status events.
//!
//! Rules:
//! * At most one helper, started when the page plays an effect, stopped when
//!   the page leaves Edit colors, the project closes or changes, and when the
//!   app exits (closing stdin; killed when it has not gone within a moment).
//!   It never takes the operation lease: playback works while a build runs.
//! * ONE source of bytes: "yours" is the overlay VPK holding exactly the files
//!   a build of the project's rows (this one with the page's current edit,
//!   `effect_build::rows_with`) writes for the played tree's parts, whichever
//!   row writes each (`effect_build::HeroTrees::shown`, the build's own
//!   `plan`), plus the user's own replacement files where they win, staged by
//!   the build's `resources::stage`; so the picture is the mod's file for
//!   every part. An edit that writes nothing plays without an overlay.
//! * Edits coalesce: the newest call wins, older ones that have not started
//!   their overlay return `superseded`.
//! * Failures are states, never error toasts: no helper (`unavailable`,
//!   `missing`), no OpenGL 4.6 (`unavailable`, `opengl`, remembered for the
//!   session), the helper dying (one automatic restart per effect, then
//!   `failed`), a refused command (`error` on the answer; the previous picture
//!   stays).
//! * Memory: the helper grows with use (overlays, effects). Its private
//!   memory is asked for whenever another effect is chosen; past
//!   `MEMORY_LIMIT_MB` it is started again before that effect loads.
use crate::app::{AppError, Application, Result};
use deadlock_components::effect_build::{HeroTrees, Outcome};
use deadlock_project::effect_edit::{EffectEditRow, TreeEdit};
use deadlock_project::Project;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

/// The helper's file name and where the installer puts it (beside the cloth reader).
pub const HELPER_EXE: &str = "Dmb.EffectPlayer.exe";
pub const HELPER_FOLDER: &str = "native/effect-player/win-x64";
/// An explicit helper for development and tests (debug builds only, see `locate`).
pub const HELPER_ENV: &str = "DMB_EFFECT_PLAYER";
/// Private memory (MB) past which the helper is started again before the next effect.
pub const MEMORY_LIMIT_MB: f64 = 1536.;
/// Bytes before the helper's frame payload in a frame the page receives.
pub const FRAME_PREFIX: usize = 16;
pub const FRAME_MAGIC: &[u8; 4] = b"DMBF";
/// The only backgrounds offered (glow vanishes on light ones).
pub const BACKGROUNDS: [&str; 2] = ["0b0c10", "2a2e3a"];
/// View brightness the page offers, in stops.
pub const BRIGHTNESS_RANGE: (f64, f64) = (0., 4.);
/// Frame rates the page offers.
pub const FRAME_RATES: [u32; 4] = [30, 60, 120, 144];
/// Messages of the player (matched by the page).
pub const HELPER_MISSING: &str = "The effect player is not installed with this copy of the app.";
pub const HELPER_STOPPED: &str = "The effect player stopped unexpectedly.";
pub const HELPER_STOPPED_AGAIN: &str = "The effect player stopped again.";

const READY_TIMEOUT: Duration = Duration::from_secs(20);
const LOAD_TIMEOUT: Duration = Duration::from_secs(30);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(8);
const EXIT_GRACE: Duration = Duration::from_millis(1500);

// ───────────────────────────────────────────────────────────── page contract

/// Why the player cannot run on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unavailable {
    /// No helper file (neither installed nor, in a debug build, named by `DMB_EFFECT_PLAYER`).
    Missing,
    /// The graphics driver has no OpenGL 4.6 (exit code 3).
    Opengl,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PlayerState {
    /// No helper running (not started yet, or stopped).
    Idle,
    /// Running; `driver` names the graphics card and driver.
    Ready { driver: String },
    Unavailable { reason: Unavailable, message: String },
    /// Stopped after an error (after one automatic restart): one line and Try again.
    Failed { message: String },
}
/// What the page shows of the playback (kept for the session, not in the project).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct View {
    pub compare: bool,
    #[serde(rename = "loop")]
    pub looping: bool,
    pub speed: f64,
    pub paused: bool,
    pub background: String,
    pub brightness: f64,
    pub width: u32,
    pub height: u32,
    /// Frames per second (one of FRAME_RATES; a helper that refuses one keeps its rate).
    pub fps: u32,
}
impl Default for View {
    fn default() -> Self {
        Self { compare: false, looping: true, speed: 1., paused: false, background: BACKGROUNDS[0].into(), brightness: 0., width: 640, height: 480, fps: 30 }
    }
}
impl View {
    fn clamped(mut self) -> Self {
        if !BACKGROUNDS.contains(&self.background.as_str()) {
            self.background = BACKGROUNDS[0].into();
        }
        self.brightness = self.brightness.clamp(BRIGHTNESS_RANGE.0, BRIGHTNESS_RANGE.1);
        if !(self.speed > 0. && self.speed <= 8.) {
            self.speed = 1.;
        }
        self.width = self.width.clamp(64, 2048);
        self.height = self.height.clamp(64, 2048);
        self
    }
}
/// The answer to `play` and `edit`.
#[derive(Debug, Clone, Serialize)]
pub struct Answer {
    pub state: PlayerState,
    /// The first frame whose generation is at least this one shows the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<u32>,
    /// The helper's `loaded` report (effect, notes, files from the overlay ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded: Option<Value>,
    /// The overlay's files (game paths) — empty when the edit writes nothing.
    pub overlay: Vec<String>,
    /// A newer edit came in before this one was made.
    pub superseded: bool,
    /// The helper refused the command (one plain sentence); the previous picture stays.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Milliseconds spent: making the overlay and the helper's reply.
    pub overlay_ms: f64,
    pub reply_ms: f64,
    /// A sentence about the view (a frame rate this helper cannot play).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}
impl Answer {
    fn state(state: PlayerState) -> Self {
        Self { state, generation: None, loaded: None, overlay: vec![], superseded: false, error: None, overlay_ms: 0., reply_ms: 0., notice: None }
    }
}
/// A view command from the page.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Control {
    Compare { on: bool },
    Pause,
    Resume,
    Restart,
    Loop { on: bool },
    Speed { value: f64 },
    Size { width: u32, height: u32 },
    Background { color: String },
    Brightness { stops: f64 },
    Fit,
    ResetView,
    Orbit { dx: f64, dy: f64 },
    Zoom { steps: f64 },
    Fps { value: u32 },
}
/// Status events the page gets (`effect-player` in the desktop app).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Starting,
    Ready { driver: String, start_ms: f64 },
    Loaded { target: String, generation: u32, loaded: Value },
    Finished { generation: u32 },
    /// Started again after it stopped unexpectedly (the effect plays again).
    Restarted { message: String },
    Unavailable { reason: Unavailable, message: String },
    Failed { message: String },
    Stopped,
}
pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

// ───────────────────────────────────────────────────────────── processes

/// How the helper is started.
#[derive(Debug, Clone)]
pub struct Launch {
    pub exe: PathBuf,
    pub pak: String,
    /// Arguments after `serve --pak <pak>`.
    pub args: Vec<String>,
}
/// A running helper as the bridge sees it (a real process, or a fake in tests).
pub struct Process {
    pub stdin: Box<dyn Write + Send>,
    pub stdout: Box<dyn Read + Send>,
    pub stderr: Box<dyn Read + Send>,
    pub control: Box<dyn ProcessControl>,
}
pub trait ProcessControl: Send {
    fn pid(&self) -> u32;
    /// The exit code once it has exited (`None` while it runs).
    fn try_wait(&mut self) -> Option<i32>;
    fn kill(&mut self);
}
pub type Spawner = Arc<dyn Fn(&Launch) -> std::io::Result<Process> + Send + Sync>;

struct Child(std::process::Child);
impl ProcessControl for Child {
    fn pid(&self) -> u32 {
        self.0.id()
    }
    fn try_wait(&mut self) -> Option<i32> {
        self.0.try_wait().ok().flatten().map(|s| s.code().unwrap_or(-1))
    }
    fn kill(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
/// The real helper: hidden (no console window), every pipe taken.
pub fn real_spawner() -> Spawner {
    Arc::new(|launch: &Launch| {
        let mut command = std::process::Command::new(&launch.exe);
        command.arg("serve").arg("--pak").arg(&launch.pak).args(&launch.args);
        command.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        Ok(Process { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: Box::new(stderr), control: Box::new(Child(child)) })
    })
}
/// The helper to use: in a debug build `DMB_EFFECT_PLAYER` when it names a
/// file, else the installed one under `resources`, else the development layout
/// (`<resources>/resources/native/...`). `None`: not available. A release
/// build never reads the variable.
pub fn locate(resources: &Path) -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        if let Some(path) = std::env::var_os(HELPER_ENV).map(PathBuf::from).filter(|p| p.is_file()) {
            return Some(path);
        }
    }
    [resources.join(HELPER_FOLDER).join(HELPER_EXE), resources.join("resources").join(HELPER_FOLDER).join(HELPER_EXE)].into_iter().find(|p| p.is_file())
}

// ───────────────────────────────────────────────────────────── protocol

/// One message of the helper's stdout.
#[derive(Debug, PartialEq)]
pub enum Message {
    Status(Value),
    Frame(Vec<u8>),
    Other,
}
/// Reads one message; `Ok(None)` at the end of the stream.
pub fn read_message(reader: &mut impl Read) -> std::io::Result<Option<Message>> {
    let mut header = [0u8; 12];
    let mut got = 0;
    while got < 12 {
        let n = reader.read(&mut header[got..])?;
        if n == 0 {
            return if got == 0 { Ok(None) } else { Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "truncated header")) };
        }
        got += n;
    }
    if &header[..4] != b"DMBE" {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the stream is broken"));
    }
    let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if length > 64 * 1024 * 1024 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "message too large"));
    }
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(match header[4] {
        1 => Message::Status(serde_json::from_slice(&payload).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?),
        2 if payload.len() >= 32 => Message::Frame(payload),
        _ => Message::Other,
    }))
}
/// Writes one message in the helper's layout (fake helpers in tests).
pub fn write_message(writer: &mut impl Write, kind: u8, payload: &[u8]) -> std::io::Result<()> {
    let mut header = [0u8; 12];
    header[..4].copy_from_slice(b"DMBE");
    header[4] = kind;
    header[8..12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    writer.write_all(&header)?;
    writer.write_all(payload)?;
    writer.flush()
}
/// The fields of a frame header the bridge reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameInfo {
    pub sequence: u32,
    pub generation: u32,
    pub time: f32,
    pub width: u16,
    pub height: u16,
    pub coverage: f32,
    pub flags: u8,
}
pub fn frame_info(payload: &[u8]) -> Option<FrameInfo> {
    if payload.len() < 32 {
        return None;
    }
    let u32_at = |i: usize| u32::from_le_bytes(payload[i..i + 4].try_into().unwrap());
    let u16_at = |i: usize| u16::from_le_bytes(payload[i..i + 2].try_into().unwrap());
    Some(FrameInfo {
        sequence: u32_at(0),
        generation: u32_at(4),
        time: f32::from_bits(u32_at(8)),
        width: u16_at(16),
        height: u16_at(18),
        coverage: f32::from_bits(u32_at(20)),
        flags: payload[24],
    })
}

/// The newest frame: the reader overwrites it, the page takes it. Never a queue.
pub struct FrameSlot {
    newest: Mutex<Option<(u32, Arc<Vec<u8>>)>>,
    changed: Condvar,
    counter: AtomicU32,
}
impl FrameSlot {
    fn new() -> Self {
        Self { newest: Mutex::new(None), changed: Condvar::new(), counter: AtomicU32::new(0) }
    }
    fn put(&self, payload: &[u8]) {
        let n = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        let mut bytes = Vec::with_capacity(FRAME_PREFIX + payload.len());
        bytes.extend_from_slice(FRAME_MAGIC);
        bytes.extend_from_slice(&n.to_le_bytes());
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.).unwrap_or(0.);
        bytes.extend_from_slice(&now.to_le_bytes());
        bytes.extend_from_slice(payload);
        *lock(&self.newest) = Some((n, Arc::new(bytes)));
        self.changed.notify_all();
    }
    /// The newest frame with a counter above `after`, waiting up to `timeout`.
    pub fn after(&self, after: u32, timeout: Duration) -> Option<Arc<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        let mut newest = lock(&self.newest);
        loop {
            if let Some((n, bytes)) = newest.as_ref() {
                if *n > after {
                    return Some(bytes.clone());
                }
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            newest = self.changed.wait_timeout(newest, left).unwrap_or_else(|e| e.into_inner()).0;
        }
    }
    fn wake(&self) {
        self.changed.notify_all();
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ───────────────────────────────────────────────────────────── the helper

struct Helper {
    instance: u64,
    stdin: Option<Box<dyn Write + Send>>,
    control: Box<dyn ProcessControl>,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>>,
    next_id: u64,
    pak: String,
    driver: String,
    /// The view the helper has now.
    view: View,
    /// Frame rates this helper refused (an older build stops lower).
    refused_fps: BTreeSet<u32>,
    threads: Vec<std::thread::JoinHandle<()>>,
    /// Set by the reader: an `error` status with `code: opengl` came.
    opengl: Arc<Mutex<Option<String>>>,
}

struct Current {
    target: String,
    /// The overlay file the helper last read (kept until the next one is read).
    overlay: Option<PathBuf>,
    /// The bytes it holds (to skip an unchanged edit).
    files: BTreeMap<String, Vec<u8>>,
    generation: u32,
}

struct Inner {
    helper: Option<Helper>,
    state: PlayerState,
    /// No OpenGL 4.6 or no helper file: not tried again this session.
    remembered: Option<(Unavailable, String)>,
    view: View,
    current: Option<Current>,
    /// Automatic restarts since the current effect was chosen.
    restarts: u32,
    private_mb: Option<f64>,
    /// The last effect played (target, edit, user files), for a restart.
    last: Option<(String, BTreeMap<String, Vec<u8>>)>,
}

/// Kept trees of the hero the page plays (one hero at a time).
struct Trees {
    key: String,
    trees: Arc<HeroTrees>,
}

struct Shared {
    spawner: Spawner,
    inner: Mutex<Inner>,
    /// Serialises play, edit and recovery (each builds an overlay and waits for a reply).
    work: Mutex<()>,
    edits: AtomicU64,
    instances: AtomicU64,
    overlays: AtomicU64,
    stopping: AtomicBool,
    frames: FrameSlot,
    sink: Mutex<Option<EventSink>>,
    trees: Mutex<Option<Trees>>,
    /// The user's own files last staged for "yours" (`own_files`): their key and bytes.
    own: Mutex<Option<(String, BTreeMap<String, Vec<u8>>)>>,
    folder: Mutex<Option<PathBuf>>,
    /// The helper file last used (for an automatic restart).
    exe: Mutex<Option<PathBuf>>,
    /// Lines the helper wrote to stderr (the last few, for the log).
    stderr: Mutex<Vec<String>>,
    /// Each command's round trip, for the log and the measurements.
    timings: Mutex<Vec<(String, f64)>>,
}

/// The bridge. Owned by `Application`; dropping it stops the helper.
pub struct EffectPlayer {
    shared: Arc<Shared>,
}

impl Default for EffectPlayer {
    fn default() -> Self {
        Self::with_spawner(real_spawner())
    }
}
impl Drop for EffectPlayer {
    fn drop(&mut self) {
        self.shared.stop();
    }
}

/// What a play or edit needs from the application: the game archive the
/// page's effects were read from, its identity, the hero and the user's own
/// replacement files.
pub struct Source {
    pub archive: Arc<deadlock_vpk::Vpk>,
    pub archive_key: String,
    pub model: String,
    /// The hero's Effects catalog read with the assets (else it is read again).
    pub catalog: Option<Arc<Vec<deadlock_components::resources::CatalogRow>>>,
    pub user_files: BTreeSet<String>,
    /// The project's saved effect edit rows and whether its extras are on:
    /// "yours" is what a build of every row writes for the played tree
    /// (`effect_build::rows_with`, `HeroTrees::shown`).
    pub rows: Vec<EffectEditRow>,
    pub active: bool,
    /// The user's own effect replacement files, staged as a build stages
    /// them (`resources::stage`) for the parts they win.
    pub replacements: Vec<deadlock_project::customization::ResourceReplacement>,
    pub launch: Option<Launch>,
    /// Where overlays are written (the app's cache).
    pub folder: PathBuf,
}

impl EffectPlayer {
    pub fn with_spawner(spawner: Spawner) -> Self {
        Self {
            shared: Arc::new(Shared {
                spawner,
                inner: Mutex::new(Inner { helper: None, state: PlayerState::Idle, remembered: None, view: View::default(), current: None, restarts: 0, private_mb: None, last: None }),
                work: Mutex::new(()),
                edits: AtomicU64::new(0),
                instances: AtomicU64::new(0),
                overlays: AtomicU64::new(0),
                stopping: AtomicBool::new(false),
                frames: FrameSlot::new(),
                sink: Mutex::new(None),
                trees: Mutex::new(None),
                own: Mutex::new(None),
                folder: Mutex::new(None),
                exe: Mutex::new(None),
                stderr: Mutex::new(vec![]),
                timings: Mutex::new(vec![]),
            }),
        }
    }
    pub fn set_sink(&self, sink: EventSink) {
        *lock(&self.shared.sink) = Some(sink);
    }
    pub fn state(&self) -> PlayerState {
        lock(&self.shared.inner).state.clone()
    }
    pub fn view(&self) -> View {
        lock(&self.shared.inner).view.clone()
    }
    /// The helper's process id while one runs (tests check it is gone after a stop).
    pub fn pid(&self) -> Option<u32> {
        lock(&self.shared.inner).helper.as_ref().map(|h| h.control.pid())
    }
    /// The newest frame after `after` (the counter in its first bytes), waiting up to `timeout`.
    pub fn frame_after(&self, after: u32, timeout: Duration) -> Option<Arc<Vec<u8>>> {
        self.shared.frames.after(after, timeout)
    }
    /// The overlay file the helper last read (`None`: the effect plays without one).
    pub fn overlay_path(&self) -> Option<PathBuf> {
        lock(&self.shared.inner).current.as_ref().and_then(|c| c.overlay.clone())
    }
    /// The last stderr lines of the helper.
    pub fn stderr_lines(&self) -> Vec<String> {
        lock(&self.shared.stderr).clone()
    }
    pub fn timings(&self) -> Vec<(String, f64)> {
        lock(&self.shared.timings).clone()
    }
    /// Plays `target` with `edit` (the page's current edit, saved or not).
    pub fn play(&self, source: &Source, target: &str, edit: &TreeEdit, view: Option<View>) -> Result<Answer> {
        let n = self.shared.edits.fetch_add(1, Ordering::SeqCst) + 1;
        let _work = lock(&self.shared.work);
        if let Some(view) = view {
            lock(&self.shared.inner).view = view.clamped();
        }
        self.shared.run(source, target, edit, n, true)
    }
    /// Updates "yours" for the loaded effect (coalesced: the newest wins).
    pub fn edit(&self, source: &Source, target: &str, edit: &TreeEdit) -> Result<Answer> {
        let n = self.shared.edits.fetch_add(1, Ordering::SeqCst) + 1;
        let _work = lock(&self.shared.work);
        if self.shared.edits.load(Ordering::SeqCst) != n {
            let mut answer = Answer::state(self.state());
            answer.superseded = true;
            return Ok(answer);
        }
        self.shared.run(source, target, edit, n, false)
    }
    /// The files "yours" plays for `target` with `edit` (game path → bytes), without a helper:
    /// what the overlay holds.
    pub fn yours(&self, source: &Source, target: &str, edit: &TreeEdit) -> Result<BTreeMap<String, Vec<u8>>> {
        Ok(self.shared.yours(source, target, edit)?.0)
    }
    /// A view command; the helper's reply (generation, achieved, loaded ...).
    pub fn control(&self, control: Control) -> Result<Answer> {
        self.shared.control(control)
    }
    /// Asks the helper for its memory and frame counts.
    pub fn stats(&self) -> Option<Value> {
        let reply = self.shared.command(json!({"cmd": "stats"}), COMMAND_TIMEOUT).ok()?;
        let stats = reply.get("stats").cloned();
        if let Some(mb) = stats.as_ref().and_then(|s| s["privateMB"].as_f64()) {
            lock(&self.shared.inner).private_mb = Some(mb);
        }
        stats
    }
    /// Stops the helper (closing its stdin) and removes the overlays.
    pub fn stop(&self) {
        let _work = lock(&self.shared.work);
        self.shared.stop();
    }
}

impl Shared {
    fn emit(&self, event: Event) {
        let sink = lock(&self.sink).clone();
        if let Some(sink) = sink {
            sink(event);
        }
    }
    fn timing(&self, what: &str, ms: f64) {
        let mut t = lock(&self.timings);
        if t.len() > 200 {
            t.remove(0);
        }
        t.push((what.into(), ms));
    }

    /// "Yours" for `target` with `edit`: what a build of every saved row (this one with `edit`)
    /// writes for the tree's parts, whichever row writes them, and the user's own files where they
    /// win; with a sentence when one of the user's files could not be staged (the game's file
    /// plays there).
    fn yours(&self, source: &Source, target: &str, edit: &TreeEdit) -> Result<(BTreeMap<String, Vec<u8>>, Option<String>)> {
        let trees = self.trees(source)?;
        if !trees.has(target) {
            return Err(AppError::validation(deadlock_components::effect_trees::TREE_UNKNOWN, target));
        }
        let rows = deadlock_components::effect_build::rows_with(&source.rows, source.active, target, edit);
        let cancel = deadlock_core::CancellationToken::new();
        let (shown, mut files) = trees.shown(&source.archive, &rows, target, &source.user_files, &cancel)?;
        let own: Vec<String> = shown.parts.iter().filter(|p| p.outcome == Outcome::UserFile).map(|p| p.path.clone()).collect();
        let mut notice = None;
        if !own.is_empty() {
            match self.own_files(source, &own) {
                Ok(bytes) => files.extend(bytes),
                Err(e) => notice = Some(format!("Your own file for {} could not be read ({e}); the game's file plays there.", own.iter().map(|p| p.rsplit('/').next().unwrap_or(p)).collect::<Vec<_>>().join(", "))),
            }
        }
        Ok((files, notice))
    }

    /// Builds the overlay of `edit`, starts the helper when needed and loads
    /// or replaces the overlay. Runs under the `work` lock.
    fn run(self: &Arc<Self>, source: &Source, target: &str, edit: &TreeEdit, n: u64, play: bool) -> Result<Answer> {
        // The overlay: exactly the build's files for the tree's parts.
        let started = Instant::now();
        let (files, own_notice) = self.yours(source, target, edit)?;
        let overlay_ms = started.elapsed().as_secs_f64() * 1000.;
        if self.edits.load(Ordering::SeqCst) != n && !play {
            let mut answer = Answer::state(lock(&self.inner).state.clone());
            answer.superseded = true;
            return Ok(answer);
        }
        *lock(&self.folder) = Some(source.folder.clone());
        if let Some(launch) = &source.launch {
            *lock(&self.exe) = Some(launch.exe.clone());
        }

        // A helper that runs, unless this computer cannot run one.
        let same = {
            let inner = lock(&self.inner);
            inner.current.as_ref().filter(|c| c.target.eq_ignore_ascii_case(target)).map(|c| c.files == files)
        };
        if play {
            let changed_effect = same.is_none();
            if changed_effect {
                lock(&self.inner).restarts = 0;
                self.restart_if_large();
            }
            if matches!(lock(&self.inner).state, PlayerState::Failed { .. }) {
                // Try again.
                lock(&self.inner).state = PlayerState::Idle;
                lock(&self.inner).restarts = 0;
            }
        }
        if let Some(state) = self.ensure(source.launch.as_ref())? {
            let mut answer = Answer::state(state);
            answer.overlay = files.keys().cloned().collect();
            answer.overlay_ms = overlay_ms;
            return Ok(answer);
        }
        if same == Some(true) && !play {
            // Nothing a build would write changed (a pick saved as the same shift, a drag back).
            let inner = lock(&self.inner);
            let mut answer = Answer::state(inner.state.clone());
            answer.generation = inner.current.as_ref().map(|c| c.generation);
            answer.overlay = files.keys().cloned().collect();
            answer.overlay_ms = overlay_ms;
            answer.notice = own_notice;
            return Ok(answer);
        }
        let path = if files.is_empty() { None } else { Some(self.write_overlay(&source.folder, &files)?) };
        // The same effect: only "yours" is rebuilt (the camera stays); another one is loaded.
        let replace = same.is_some();
        let notice = self.sync_view();
        let mut answer = self.send_effect(target, &files, path.as_deref(), replace)?;
        answer.overlay_ms = overlay_ms;
        answer.notice = match (own_notice, notice) {
            (Some(own), Some(rate)) => Some(format!("{own} {rate}")),
            (own, rate) => own.or(rate),
        };
        lock(&self.inner).last = Some((target.into(), files));
        Ok(answer)
    }

    /// Sends `load` (or `overlay` when only "yours" changes) and records the result.
    fn send_effect(self: &Arc<Self>, target: &str, files: &BTreeMap<String, Vec<u8>>, path: Option<&Path>, replace: bool) -> Result<Answer> {
        let overlay = path.map(|p| p.display().to_string());
        let view = lock(&self.inner).view.clone();
        let command = if replace {
            json!({"cmd": "overlay", "overlay": overlay})
        } else {
            json!({"cmd": "load", "effect": target, "overlay": overlay, "compare": view.compare, "loop": view.looping, "speed": view.speed, "paused": view.paused})
        };
        let started = Instant::now();
        let reply = match self.command(command.clone(), LOAD_TIMEOUT) {
            Ok(reply) => reply,
            Err(Failure::Refused(error, _)) => {
                if let Some(p) = path {
                    let _ = std::fs::remove_file(p);
                }
                let mut answer = Answer::state(lock(&self.inner).state.clone());
                answer.error = Some(error);
                answer.reply_ms = started.elapsed().as_secs_f64() * 1000.;
                return Ok(answer);
            }
            Err(Failure::Gone(instance)) => {
                // The helper stopped while loading: one restart, then this effect once more.
                let recovered = self.recover(instance);
                if !recovered {
                    return Ok(Answer::state(lock(&self.inner).state.clone()));
                }
                let command = json!({"cmd": "load", "effect": target, "overlay": overlay, "compare": view.compare, "loop": view.looping, "speed": view.speed, "paused": view.paused});
                match self.command(command, LOAD_TIMEOUT) {
                    Ok(reply) => reply,
                    Err(Failure::Refused(error, _)) => {
                        let mut answer = Answer::state(lock(&self.inner).state.clone());
                        answer.error = Some(error);
                        return Ok(answer);
                    }
                    Err(Failure::Gone(instance)) => {
                        self.recover(instance);
                        return Ok(Answer::state(lock(&self.inner).state.clone()));
                    }
                }
            }
        };
        let reply_ms = started.elapsed().as_secs_f64() * 1000.;
        self.timing(if replace { "overlay" } else { "load" }, reply_ms);
        let generation = reply["generation"].as_u64().unwrap_or(0) as u32;
        let loaded = reply.get("loaded").cloned();
        let old = {
            let mut inner = lock(&self.inner);
            let old = inner.current.take().and_then(|c| c.overlay);
            inner.current = Some(Current { target: target.into(), overlay: path.map(Path::to_path_buf), files: files.clone(), generation });
            old
        };
        // The helper read the new overlay into memory: the old file can go.
        if let Some(old) = old.filter(|o| Some(o.as_path()) != path) {
            let _ = std::fs::remove_file(old);
        }
        if let Some(loaded) = &loaded {
            self.emit(Event::Loaded { target: target.into(), generation, loaded: loaded.clone() });
        }
        let inner = lock(&self.inner);
        Ok(Answer {
            state: inner.state.clone(),
            generation: Some(generation),
            loaded,
            overlay: files.keys().cloned().collect(),
            superseded: false,
            error: None,
            overlay_ms: 0.,
            reply_ms,
            notice: None,
        })
    }

    /// Between effects: asks the helper for its private memory and stops it
    /// past `MEMORY_LIMIT_MB` (the next load starts a fresh one). Returns
    /// whether it was stopped.
    fn restart_if_large(&self) -> bool {
        if lock(&self.inner).helper.is_none() {
            return false;
        }
        let mb = self.command(json!({"cmd": "stats"}), COMMAND_TIMEOUT).ok().and_then(|r| r["stats"]["privateMB"].as_f64());
        lock(&self.inner).private_mb = mb;
        if mb.is_some_and(|mb| mb > MEMORY_LIMIT_MB) {
            self.shutdown_helper();
            return true;
        }
        false
    }

    fn trees(&self, source: &Source) -> Result<Arc<HeroTrees>> {
        let key = format!("{}|{}", source.archive_key, deadlock_core::text::casefold(&source.model));
        if let Some(kept) = lock(&self.trees).as_ref().filter(|t| t.key == key) {
            return Ok(kept.trees.clone());
        }
        let cancel = deadlock_core::CancellationToken::new();
        let trees = Arc::new(match &source.catalog {
            Some(rows) => HeroTrees::from_catalog(&source.archive, rows, &cancel)?,
            None => HeroTrees::read(&source.archive, &source.model, &cancel)?,
        });
        *lock(&self.trees) = Some(Trees { key, trees: trees.clone() });
        Ok(trees)
    }

    /// The user's own replacement files for `parts` (game paths), staged exactly as a build
    /// stages them (`resources::stage`, into a scratch folder removed at once), kept while the
    /// rows and their files stay the same.
    fn own_files(&self, source: &Source, parts: &[String]) -> Result<BTreeMap<String, Vec<u8>>> {
        let fold = deadlock_core::text::casefold;
        let wanted: BTreeSet<String> = parts.iter().map(|p| fold(p)).collect();
        let rows: Vec<_> = source.replacements.iter().filter(|r| r.target.as_deref().is_some_and(|t| wanted.contains(&fold(t)))).cloned().collect();
        let stamps: Vec<String> = rows
            .iter()
            .map(|r| {
                let meta = r.source.as_deref().and_then(|s| std::fs::metadata(s).ok());
                format!("{:?}/{:?}", meta.as_ref().map(|m| m.len()), meta.and_then(|m| m.modified().ok()))
            })
            .collect();
        let key = format!("{}|{}|{}|{}", source.archive_key, fold(&source.model), serde_json::to_string(&rows).unwrap_or_default(), stamps.join(";"));
        if let Some((_, files)) = lock(&self.own).as_ref().filter(|(k, _)| *k == key) {
            return Ok(files.clone());
        }
        let n = self.overlays.fetch_add(1, Ordering::SeqCst) + 1;
        let scratch = source.folder.join(format!("own-{n}"));
        let _ = std::fs::remove_dir_all(&scratch);
        let cancel = deadlock_core::CancellationToken::new();
        let staged = deadlock_components::resources::stage(&source.archive, &source.model, &rows, &scratch, deadlock_components::resources::ResourceKind::Effects, &cancel)
            .map_err(|e| AppError::validation("Your own effect file could not be staged", e))
            .and_then(|receipt| {
                receipt
                    .entries
                    .iter()
                    .map(|e| {
                        let path = deadlock_core::safety::inside(&scratch, &e.target)?;
                        let bytes = std::fs::read(&path).map_err(|err| AppError::io("Could not prepare the effect preview", err))?;
                        Ok((e.target.clone(), bytes))
                    })
                    .collect::<Result<BTreeMap<String, Vec<u8>>>>()
            });
        let _ = std::fs::remove_dir_all(&scratch);
        let files = staged?;
        *lock(&self.own) = Some((key, files.clone()));
        Ok(files)
    }

    /// A new overlay VPK (a new name each time: the VPK writer only makes new files).
    fn write_overlay(&self, folder: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<PathBuf> {
        let n = self.overlays.fetch_add(1, Ordering::SeqCst) + 1;
        std::fs::create_dir_all(folder).map_err(|e| AppError::io("Could not prepare the effect preview", e))?;
        let staging = folder.join(format!("overlay-{n}"));
        let _ = std::fs::remove_dir_all(&staging);
        for (path, bytes) in files {
            deadlock_core::safety::atomic_write(&deadlock_core::safety::inside(&staging, path)?, bytes)?;
        }
        let vpk = folder.join(format!("overlay-{n}.vpk"));
        let _ = std::fs::remove_file(&vpk);
        let names: Vec<String> = files.keys().cloned().collect();
        let written = deadlock_vpk::write_vpk(&staging, &names, &vpk, None, 2, &mut || Ok(())).map_err(|e| AppError::validation("Could not prepare the effect preview", e));
        let _ = std::fs::remove_dir_all(&staging);
        written?;
        Ok(vpk)
    }

    /// Starts the helper when none runs. `Some(state)`: it cannot run (the page shows the strips only).
    fn ensure(self: &Arc<Self>, launch: Option<&Launch>) -> Result<Option<PlayerState>> {
        {
            let inner = lock(&self.inner);
            if let Some((reason, message)) = &inner.remembered {
                return Ok(Some(PlayerState::Unavailable { reason: *reason, message: message.clone() }));
            }
            if let PlayerState::Failed { .. } = inner.state {
                return Ok(Some(inner.state.clone()));
            }
            if let (Some(helper), Some(launch)) = (inner.helper.as_ref(), launch) {
                if helper.pak == launch.pak {
                    return Ok(None);
                }
            }
        }
        // Another game archive, or none running.
        self.shutdown_helper();
        let Some(launch) = launch else {
            let message = HELPER_MISSING.to_owned();
            let mut inner = lock(&self.inner);
            inner.remembered = Some((Unavailable::Missing, message.clone()));
            inner.state = PlayerState::Unavailable { reason: Unavailable::Missing, message: message.clone() };
            drop(inner);
            self.emit(Event::Unavailable { reason: Unavailable::Missing, message });
            return Ok(Some(lock(&self.inner).state.clone()));
        };
        self.spawn(launch).map(|r| r.err())
    }

    /// Starts one helper and waits for `ready`. `Err(state)`: it cannot run.
    fn spawn(self: &Arc<Self>, launch: &Launch) -> Result<std::result::Result<(), PlayerState>> {
        self.stopping.store(false, Ordering::SeqCst);
        self.emit(Event::Starting);
        let view = lock(&self.inner).view.clone();
        let mut launch = launch.clone();
        launch.args.extend([
            "--size".into(),
            format!("{}x{}", view.width, view.height),
            "--background".into(),
            view.background.clone(),
            "--exposure".into(),
            format!("{}", view.brightness),
        ]);
        let started = Instant::now();
        let process = match (self.spawner)(&launch) {
            Ok(p) => p,
            Err(e) => {
                // A helper file that cannot be started counts as not available.
                let message = format!("{HELPER_MISSING} ({e})");
                let mut inner = lock(&self.inner);
                inner.remembered = Some((Unavailable::Missing, message.clone()));
                inner.state = PlayerState::Unavailable { reason: Unavailable::Missing, message: message.clone() };
                drop(inner);
                self.emit(Event::Unavailable { reason: Unavailable::Missing, message });
                return Ok(Err(lock(&self.inner).state.clone()));
            }
        };
        let instance = self.instances.fetch_add(1, Ordering::SeqCst) + 1;
        let pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let opengl = Arc::new(Mutex::new(None));
        let (ready_tx, ready_rx) = mpsc::channel::<Value>();
        let Process { stdin, mut stdout, stderr, control } = process;
        let weak: Weak<Shared> = Arc::downgrade(self);
        let reader = {
            let pending = pending.clone();
            let opengl = opengl.clone();
            let weak = weak.clone();
            std::thread::Builder::new()
                .name("effect-player-out".into())
                .spawn(move || {
                    let mut ready = Some(ready_tx);
                    loop {
                        let message = read_message(&mut stdout);
                        let Some(shared) = weak.upgrade() else { break };
                        match message {
                            Ok(Some(Message::Frame(payload))) => shared.frames.put(&payload),
                            Ok(Some(Message::Status(status))) => match status["type"].as_str() {
                                Some("ready") => {
                                    if let Some(tx) = ready.take() {
                                        let _ = tx.send(status);
                                    }
                                }
                                Some("reply") => {
                                    let id = status["id"].as_u64().unwrap_or(0);
                                    if let Some(tx) = lock(&pending).remove(&id) {
                                        let _ = tx.send(status);
                                    }
                                }
                                Some("finished") => shared.emit(Event::Finished { generation: status["generation"].as_u64().unwrap_or(0) as u32 }),
                                Some("error") => {
                                    if status["code"] == "opengl" {
                                        *lock(&opengl) = Some(status["message"].as_str().unwrap_or("OpenGL 4.6 is not available.").to_owned());
                                    }
                                    let mut lines = lock(&shared.stderr);
                                    lines.push(format!("error status: {status}"));
                                }
                                _ => {}
                            },
                            Ok(Some(Message::Other)) => {}
                            Ok(None) | Err(_) => break,
                        }
                    }
                    // The stream ended: the helper exited (or broke). Waiting commands fail now.
                    lock(&pending).clear();
                    ready.take();
                    if let Some(shared) = weak.upgrade() {
                        shared.frames.wake();
                        if !shared.stopping.load(Ordering::SeqCst) {
                            let shared = shared.clone();
                            let _ = std::thread::Builder::new().name("effect-player-recover".into()).spawn(move || {
                                let _work = lock(&shared.work);
                                shared.recover(instance);
                            });
                        }
                    }
                })
                .expect("thread")
        };
        let drain = {
            let weak = weak.clone();
            std::thread::Builder::new()
                .name("effect-player-err".into())
                .spawn(move || {
                    let mut lines = std::io::BufReader::new(stderr);
                    let mut line = String::new();
                    while lines.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
                        if let Some(shared) = weak.upgrade() {
                            let mut kept = lock(&shared.stderr);
                            if kept.len() >= 40 {
                                kept.remove(0);
                            }
                            kept.push(line.trim_end().to_owned());
                        }
                        line.clear();
                    }
                })
                .expect("thread")
        };
        let mut helper = Helper { instance, stdin: Some(stdin), control, pending, next_id: 0, pak: launch.pak.clone(), driver: String::new(), view: View { fps: 30, ..view.clone() }, refused_fps: BTreeSet::new(), threads: vec![reader, drain], opengl };
        let ready = ready_rx.recv_timeout(READY_TIMEOUT);
        match ready {
            Ok(status) => {
                let gl = &status["gl"];
                helper.driver = [gl["renderer"].as_str(), gl["version"].as_str()].into_iter().flatten().collect::<Vec<_>>().join(", ");
                let driver = helper.driver.clone();
                let start_ms = started.elapsed().as_secs_f64() * 1000.;
                self.timing("start", start_ms);
                let mut inner = lock(&self.inner);
                inner.helper = Some(helper);
                inner.state = PlayerState::Ready { driver: driver.clone() };
                inner.current = None;
                inner.private_mb = None;
                drop(inner);
                self.emit(Event::Ready { driver, start_ms });
                Ok(Ok(()))
            }
            Err(_) => {
                // Exited (or never answered) before ready: OpenGL missing, or it could not start.
                let deadline = Instant::now() + EXIT_GRACE;
                let mut code = helper.control.try_wait();
                while code.is_none() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(20));
                    code = helper.control.try_wait();
                }
                let opengl = lock(&helper.opengl).clone();
                self.stopping.store(true, Ordering::SeqCst);
                finish(&mut helper);
                self.stopping.store(false, Ordering::SeqCst);
                let state = if code == Some(3) || opengl.is_some() {
                    let message = opengl.unwrap_or_else(|| "This computer's graphics driver does not offer OpenGL 4.6.".into());
                    lock(&self.inner).remembered = Some((Unavailable::Opengl, message.clone()));
                    self.emit(Event::Unavailable { reason: Unavailable::Opengl, message: message.clone() });
                    PlayerState::Unavailable { reason: Unavailable::Opengl, message }
                } else {
                    let message = format!("The effect player could not start{}.", code.map(|c| format!(" (exit code {c})")).unwrap_or_default());
                    self.emit(Event::Failed { message: message.clone() });
                    PlayerState::Failed { message }
                };
                lock(&self.inner).state = state.clone();
                Ok(Err(state))
            }
        }
    }

    /// The helper `instance` stopped unexpectedly: one restart per effect
    /// (playing the last effect again), else `failed`. Under the `work` lock.
    /// Returns whether a helper runs again.
    fn recover(self: &Arc<Self>, instance: u64) -> bool {
        let (launch_pak, restarts) = {
            let inner = lock(&self.inner);
            match inner.helper.as_ref() {
                Some(h) if h.instance == instance => (h.pak.clone(), inner.restarts),
                // Already handled (or stopped on purpose).
                _ => return inner.helper.is_some(),
            }
        };
        let helper = lock(&self.inner).helper.take();
        let mut code = None;
        let mut opengl = None;
        if let Some(mut helper) = helper {
            let deadline = Instant::now() + EXIT_GRACE;
            code = helper.control.try_wait();
            while code.is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
                code = helper.control.try_wait();
            }
            opengl = lock(&helper.opengl).clone();
            self.stopping.store(true, Ordering::SeqCst);
            finish(&mut helper);
            self.stopping.store(false, Ordering::SeqCst);
        }
        if code == Some(3) || opengl.is_some() {
            let message = opengl.unwrap_or_else(|| "This computer's graphics driver does not offer OpenGL 4.6.".into());
            let mut inner = lock(&self.inner);
            inner.remembered = Some((Unavailable::Opengl, message.clone()));
            inner.state = PlayerState::Unavailable { reason: Unavailable::Opengl, message: message.clone() };
            drop(inner);
            self.emit(Event::Unavailable { reason: Unavailable::Opengl, message });
            return false;
        }
        if restarts >= 1 {
            let message = HELPER_STOPPED_AGAIN.to_owned();
            let mut inner = lock(&self.inner);
            inner.state = PlayerState::Failed { message: message.clone() };
            inner.current = None;
            drop(inner);
            self.emit(Event::Failed { message });
            return false;
        }
        lock(&self.inner).restarts += 1;
        let exe = self.last_exe();
        let Some(exe) = exe else { return false };
        let launch = Launch { exe, pak: launch_pak, args: vec![] };
        match self.spawn(&launch) {
            Ok(Ok(())) => {}
            _ => return false,
        }
        self.emit(Event::Restarted { message: format!("{HELPER_STOPPED} It was started again{}.", code.map(|c| format!(" (exit code {c})")).unwrap_or_default()) });
        // Play the last effect again.
        let last = lock(&self.inner).last.clone();
        if let Some((target, files)) = last {
            let folder = lock(&self.folder).clone();
            let path = match (files.is_empty(), folder) {
                (false, Some(folder)) => self.write_overlay(&folder, &files).ok(),
                _ => None,
            };
            let _ = self.send_effect(&target, &files, path.as_deref(), false);
        }
        lock(&self.inner).helper.is_some()
    }

    fn last_exe(&self) -> Option<PathBuf> {
        lock(&self.exe).clone()
    }
    /// Size, background and brightness of the page's view on the running helper.
    fn sync_view(&self) -> Option<String> {
        let (want, have) = {
            let inner = lock(&self.inner);
            match inner.helper.as_ref() {
                Some(h) => (inner.view.clone(), h.view.clone()),
                None => return None,
            }
        };
        let mut commands = vec![];
        if (want.width, want.height) != (have.width, have.height) {
            commands.push(json!({"cmd": "size", "width": want.width, "height": want.height}));
        }
        if want.background != have.background {
            commands.push(json!({"cmd": "background", "color": want.background}));
        }
        if want.brightness != have.brightness {
            commands.push(json!({"cmd": "brightness", "stops": want.brightness}));
        }
        for command in commands {
            let _ = self.command(command, COMMAND_TIMEOUT);
        }
        let notice = if want.fps != have.fps { self.set_fps(want.fps).1 } else { None };
        if let Some(h) = lock(&self.inner).helper.as_mut() {
            h.view = View { fps: h.view.fps, ..want };
        }
        notice
    }

    /// Asks for `wanted` frames per second. A helper that refuses it plays at
    /// the highest offered rate below that it accepts; the page's saved choice
    /// stays as it is (`View.fps` keeps `wanted`). Returns the rate it plays
    /// at and, when that is not `wanted`, one sentence saying so.
    fn set_fps(&self, wanted: u32) -> (u32, Option<String>) {
        let mut rates = vec![wanted];
        rates.extend(FRAME_RATES.iter().rev().copied().filter(|r| *r < wanted));
        for rate in rates {
            let skip = lock(&self.inner).helper.as_ref().is_none_or(|h| h.refused_fps.contains(&rate));
            if skip && rate != 30 {
                continue;
            }
            match self.command(json!({"cmd": "fps", "value": rate}), COMMAND_TIMEOUT) {
                Ok(_) => {
                    if let Some(h) = lock(&self.inner).helper.as_mut() {
                        h.view.fps = rate;
                    }
                    let note = (rate != wanted).then(|| format!("This effect player cannot play at {wanted} frames per second here, so it plays at {rate}."));
                    return (rate, note);
                }
                Err(Failure::Refused(..)) => {
                    if let Some(h) = lock(&self.inner).helper.as_mut() {
                        h.refused_fps.insert(rate);
                    }
                }
                Err(Failure::Gone(_)) => break,
            }
        }
        let have = lock(&self.inner).helper.as_ref().map_or(30, |h| h.view.fps);
        (have, (have != wanted).then(|| format!("This effect player cannot play at {wanted} frames per second here, so it plays at {have}.")))
    }

    /// One command and its reply.
    fn command(&self, mut command: Value, timeout: Duration) -> std::result::Result<Value, Failure> {
        let (rx, instance) = {
            let mut inner = lock(&self.inner);
            let Some(helper) = inner.helper.as_mut() else { return Err(Failure::Gone(0)) };
            helper.next_id += 1;
            let id = helper.next_id;
            command["id"] = json!(id);
            let (tx, rx) = mpsc::channel();
            lock(&helper.pending).insert(id, tx);
            let mut line = serde_json::to_vec(&command).unwrap_or_default();
            line.push(b'\n');
            let written = helper.stdin.as_mut().map(|s| s.write_all(&line).and_then(|_| s.flush()));
            if !matches!(written, Some(Ok(()))) {
                lock(&helper.pending).remove(&id);
                return Err(Failure::Gone(helper.instance));
            }
            (rx, helper.instance)
        };
        match rx.recv_timeout(timeout) {
            Ok(reply) if reply["ok"].as_bool() == Some(true) => Ok(reply),
            Ok(reply) => Err(Failure::Refused(reply["error"].as_str().unwrap_or("The effect player refused the command.").to_owned(), reply)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // A helper that does not answer is stopped (and handled as stopped).
                if let Some(helper) = lock(&self.inner).helper.as_mut().filter(|h| h.instance == instance) {
                    helper.control.kill();
                }
                Err(Failure::Gone(instance))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(Failure::Gone(instance)),
        }
    }

    fn control(self: &Arc<Self>, control: Control) -> Result<Answer> {
        if let Control::Fps { value } = control {
            let wanted = value.clamp(1, 240);
            lock(&self.inner).view.fps = wanted;
            let mut answer = Answer::state(lock(&self.inner).state.clone());
            if lock(&self.inner).helper.is_some() {
                let started = Instant::now();
                answer.notice = self.set_fps(wanted).1;
                answer.reply_ms = started.elapsed().as_secs_f64() * 1000.;
            }
            return Ok(answer);
        }
        let command = {
            let mut inner = lock(&self.inner);
            let view = &mut inner.view;
            let command = match &control {
                Control::Compare { on } => {
                    view.compare = *on;
                    json!({"cmd": "compare", "on": on})
                }
                Control::Pause => {
                    view.paused = true;
                    json!({"cmd": "pause"})
                }
                Control::Resume => {
                    view.paused = false;
                    json!({"cmd": "resume"})
                }
                Control::Restart => json!({"cmd": "restart"}),
                Control::Loop { on } => {
                    view.looping = *on;
                    json!({"cmd": "loop", "on": on})
                }
                Control::Speed { value } => {
                    view.speed = *value;
                    *view = view.clone().clamped();
                    json!({"cmd": "speed", "value": view.speed})
                }
                Control::Size { width, height } => {
                    view.width = *width;
                    view.height = *height;
                    *view = view.clone().clamped();
                    json!({"cmd": "size", "width": view.width, "height": view.height})
                }
                Control::Background { color } => {
                    view.background = color.to_ascii_lowercase();
                    *view = view.clone().clamped();
                    json!({"cmd": "background", "color": view.background})
                }
                Control::Brightness { stops } => {
                    view.brightness = *stops;
                    *view = view.clone().clamped();
                    json!({"cmd": "brightness", "stops": view.brightness})
                }
                Control::Fit => json!({"cmd": "fit"}),
                Control::ResetView => json!({"cmd": "resetView"}),
                Control::Orbit { dx, dy } => json!({"cmd": "orbit", "dx": dx.clamp(-4000., 4000.), "dy": dy.clamp(-4000., 4000.)}),
                Control::Zoom { steps } => json!({"cmd": "zoom", "steps": steps.clamp(-40., 40.)}),
                Control::Fps { .. } => unreachable!("handled above"),
            };
            if inner.helper.is_none() {
                // Kept for the next start.
                return Ok(Answer::state(inner.state.clone()));
            }
            command
        };
        let started = Instant::now();
        let name = command["cmd"].as_str().unwrap_or_default().to_owned();
        let timeout = if name == "compare" { LOAD_TIMEOUT } else { COMMAND_TIMEOUT };
        let result = self.command(command, timeout);
        let reply_ms = started.elapsed().as_secs_f64() * 1000.;
        self.timing(&name, reply_ms);
        let mut answer = Answer::state(lock(&self.inner).state.clone());
        answer.reply_ms = reply_ms;
        match result {
            Ok(reply) => {
                answer.generation = reply["generation"].as_u64().map(|g| g as u32);
                answer.loaded = reply.get("loaded").cloned();
                {
                    let mut inner = lock(&self.inner);
                    let view = inner.view.clone();
                    if let Some(h) = inner.helper.as_mut() {
                        h.view = View { fps: h.view.fps, ..view };
                    }
                }
                if let (Some(loaded), Some(target)) = (&answer.loaded, lock(&self.inner).current.as_ref().map(|c| c.target.clone())) {
                    self.emit(Event::Loaded { target, generation: answer.generation.unwrap_or(0), loaded: loaded.clone() });
                }
            }
            Err(Failure::Refused(error, _)) => {
                answer.error = Some(error);
            }
            Err(Failure::Gone(instance)) => {
                let _work = lock(&self.work);
                self.recover(instance);
                answer.state = lock(&self.inner).state.clone();
            }
        }
        Ok(answer)
    }

    fn shutdown_helper(&self) {
        let helper = lock(&self.inner).helper.take();
        if let Some(mut helper) = helper {
            self.stopping.store(true, Ordering::SeqCst);
            finish(&mut helper);
            self.stopping.store(false, Ordering::SeqCst);
        }
        let mut inner = lock(&self.inner);
        inner.current = None;
        if matches!(inner.state, PlayerState::Ready { .. }) {
            inner.state = PlayerState::Idle;
        }
    }

    fn stop(&self) {
        let had = lock(&self.inner).helper.is_some();
        self.shutdown_helper();
        {
            let mut inner = lock(&self.inner);
            inner.last = None;
            if matches!(inner.state, PlayerState::Failed { .. }) {
                inner.state = PlayerState::Idle;
            }
        }
        if let Some(folder) = lock(&self.folder).take() {
            let _ = std::fs::remove_dir_all(folder);
        }
        if had {
            self.emit(Event::Stopped);
        }
    }
}

enum Failure {
    /// `ok: false`: the helper's sentence.
    Refused(String, #[allow(dead_code)] Value),
    /// The helper is gone (or never ran): its instance.
    Gone(u64),
}

/// Closes stdin, waits a moment for the helper to exit, kills it otherwise, and joins its threads.
fn finish(helper: &mut Helper) {
    helper.stdin.take();
    let deadline = Instant::now() + EXIT_GRACE;
    while helper.control.try_wait().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    if helper.control.try_wait().is_none() {
        helper.control.kill();
    }
    lock(&helper.pending).clear();
    for thread in helper.threads.drain(..) {
        let _ = thread.join();
    }
}

// ───────────────────────────────────────────────────────────── the application

impl Application {
    /// What a play needs: the archive the page's effects come from (loaded by
    /// `discover_extras`, refused when the game changed), the hero, the user's
    /// own replacement files and the helper to start.
    fn effect_player_source(&self, project: &Project) -> Result<Source> {
        let (archive, key, rows) = self.effects_archive(project)?;
        let user_files = deadlock_components::service::effect_files(project)?;
        let replacements = deadlock_components::service::effect_replacements(project)?;
        let exe = locate(&self.resource_directory());
        let settings = self.settings()?;
        Ok(Source {
            archive,
            archive_key: key,
            model: project.original_model.clone(),
            catalog: Some(rows),
            user_files,
            rows: project.customization.effect_edits.clone().unwrap_or_default(),
            active: project.customization.active(),
            replacements,
            launch: exe.map(|exe| Launch { exe, pak: settings.pak.clone(), args: vec![] }),
            folder: self.state_directory.join("effect-player").join(format!("overlays-{}", std::process::id())),
        })
    }
    /// Extras → Effects: plays `target` with the page's current `edit` (saved or not).
    pub fn effect_player_play(&self, project: Project, target: String, edit: TreeEdit, view: Option<View>) -> Result<Answer> {
        let source = self.effect_player_source(&project)?;
        let answer = self.effect_player.play(&source, &target, &edit, view)?;
        self.log_answer("play", &target, &answer);
        Ok(answer)
    }
    /// Updates "yours" while the user edits (coalesced; the newest call wins).
    pub fn effect_player_edit(&self, project: Project, target: String, edit: TreeEdit) -> Result<Answer> {
        let source = self.effect_player_source(&project)?;
        self.effect_player.edit(&source, &target, &edit)
    }
    pub fn effect_player_control(&self, control: Control) -> Result<Answer> {
        self.effect_player.control(control)
    }
    pub fn effect_player_stop(&self) {
        self.effect_player.stop()
    }
    pub fn effect_player_state(&self) -> PlayerState {
        self.effect_player.state()
    }
    /// The player itself (frames, events, the helper's process id and stats).
    pub fn effect_player(&self) -> &EffectPlayer {
        &self.effect_player
    }
    fn log_answer(&self, what: &str, target: &str, answer: &Answer) {
        let name = target.rsplit('/').next().unwrap_or(target);
        let line = match (&answer.state, &answer.error) {
            (_, Some(error)) => format!("Effect player: {what} {name} refused: {error}"),
            (PlayerState::Ready { .. }, None) => format!("Effect player: {what} {name} ({} overlay files, {:.0} ms to make, {:.0} ms to load)", answer.overlay.len(), answer.overlay_ms, answer.reply_ms),
            (PlayerState::Unavailable { message, .. }, None) => format!("Effect player not available: {message}"),
            (PlayerState::Failed { message }, None) => format!("Effect player stopped: {message}"),
            (PlayerState::Idle, None) => return,
        };
        let _ = self.append_log("info", &line);
    }
}

// ───────────────────────────────────────────────────────────── desktop commands

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    use tauri::Emitter;
    type State<'a> = tauri::State<'a, Arc<Application>>;
    async fn background<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
        tauri::async_runtime::spawn_blocking(f).await.map_err(|e| AppError::new("internal", "The effect player stopped unexpectedly", e))?
    }
    /// Sends the player's events to the window as `effect-player`.
    pub fn connect(app: &Application, handle: tauri::AppHandle) {
        app.effect_player.set_sink(Arc::new(move |event| {
            let _ = handle.emit("effect-player", event);
        }));
    }
    #[tauri::command]
    pub async fn effect_player_play(state: State<'_>, project: Project, target: String, edit: TreeEdit, view: Option<View>) -> Result<Answer> {
        let app = state.inner().clone();
        background(move || app.effect_player_play(project, target, edit, view)).await
    }
    #[tauri::command]
    pub async fn effect_player_edit(state: State<'_>, project: Project, target: String, edit: TreeEdit) -> Result<Answer> {
        let app = state.inner().clone();
        background(move || app.effect_player_edit(project, target, edit)).await
    }
    #[tauri::command]
    pub async fn effect_player_control(state: State<'_>, control: Control) -> Result<Answer> {
        let app = state.inner().clone();
        background(move || app.effect_player_control(control)).await
    }
    #[tauri::command]
    pub async fn effect_player_stop(state: State<'_>) -> Result<()> {
        let app = state.inner().clone();
        background(move || {
            app.effect_player_stop();
            Ok(())
        })
        .await
    }
    #[tauri::command]
    pub fn effect_player_state(state: State<'_>) -> PlayerState {
        state.effect_player_state()
    }
    /// The newest frame after `after` as raw bytes (waits up to `wait_ms`; empty when none came).
    #[tauri::command]
    pub async fn effect_player_frame(state: State<'_>, after: u32, wait_ms: Option<u64>) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        let wait = Duration::from_millis(wait_ms.unwrap_or(500).min(2000));
        background(move || Ok(tauri::ipc::Response::new(app.effect_player.frame_after(after, wait).map(|b| (*b).clone()).unwrap_or_default()))).await
    }
}

#[cfg(test)]
#[path = "effect_player_tests.rs"]
mod tests;
