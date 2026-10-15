//! Shell-free tool execution with separate byte-exact logs and owned process trees.
use crate::{CancellationToken, CoreError, Result, safety};
use chrono::{DateTime, Utc};
use process_wrap::std::{ChildWrapper, CommandWrap};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub check: bool,
    pub env: BTreeMap<OsString, OsString>,
    /// Optional shorter limit; this never increases the runner's operation limit.
    pub timeout: Option<Duration>,
}
impl CommandSpec {
    pub fn new(program: impl AsRef<Path>) -> Self {
        Self {
            program: program.as_ref().to_path_buf(),
            args: vec![],
            cwd: None,
            check: true,
            env: BTreeMap::new(),
            timeout: None,
        }
    }
    pub fn arg(mut self, value: impl AsRef<OsStr>) -> Self {
        self.args.push(value.as_ref().to_os_string());
        self
    }
    pub fn args<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(values.into_iter().map(|s| s.as_ref().to_os_string()));
        self
    }
    pub fn current_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cwd = Some(path.as_ref().to_path_buf());
        self
    }
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.env
            .insert(key.as_ref().to_os_string(), value.as_ref().to_os_string());
        self
    }
}
#[derive(Clone, Debug)]
pub struct RunnerOptions {
    pub timeout: Duration,
    pub output_limit_bytes: u64,
}
impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(1800),
            output_limit_bytes: 512 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEvent {
    pub command_id: String,
    pub stream: OutputStream,
    pub timestamp_utc: DateTime<Utc>,
    pub offset_bytes: u64,
    pub byte_count: u64,
    /// A bounded display preview only. Consumers parsing tool documents must read the raw file.
    pub preview: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessResult {
    pub command_id: String,
    pub code: Option<i32>,
    pub success: bool,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
    pub receipt_path: PathBuf,
    pub elapsed_ns: u64,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub dropped_live_events: u64,
}
impl ProcessResult {
    /// Keep a bounded excerpt of both streams in the propagated error. Many
    /// native tools report failures entirely on stdout; an empty stderr is valid.
    pub fn failure(&self, program: &Path) -> CoreError {
        CoreError::ToolFailed {
            program: program.display().to_string(),
            code: self.code,
            stdout: self.stdout_path.clone(),
            stderr: self.stderr_path.clone(),
            diagnostics: format!(
                "stdout:\n{}\nstderr:\n{}",
                log_tail(&self.stdout_path),
                log_tail(&self.stderr_path)
            ),
        }
    }
    pub fn read_stdout_utf8(&self, max_bytes: u64) -> Result<String> {
        read_utf8(&self.stdout_path, max_bytes)
    }
    pub fn read_stderr_utf8(&self, max_bytes: u64) -> Result<String> {
        read_utf8(&self.stderr_path, max_bytes)
    }
    pub fn read_stdout_lossy(&self, max_bytes: u64) -> Result<String> {
        Ok(String::from_utf8_lossy(&read_bytes(&self.stdout_path, max_bytes)?).into_owned())
    }
    pub fn read_stderr_lossy(&self, max_bytes: u64) -> Result<String> {
        Ok(String::from_utf8_lossy(&read_bytes(&self.stderr_path, max_bytes)?).into_owned())
    }
}
fn log_tail(path: &Path) -> String {
    const LIMIT: u64 = 8 * 1024;
    let read = || -> std::io::Result<String> {
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        let offset = length.saturating_sub(LIMIT);
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new();
        file.take(LIMIT).read_to_end(&mut bytes)?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(if text.trim().is_empty() {
            "(no output on this stream)".into()
        } else if offset > 0 {
            format!("[last {LIMIT} bytes; full output saved]\n{}", text.trim())
        } else {
            text.trim().to_owned()
        })
    };
    read().unwrap_or_else(|error| format!("(could not read saved output: {error})"))
}
fn read_bytes(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|e| CoreError::io("Open raw tool log", e))?;
    if file
        .metadata()
        .map_err(|e| CoreError::io("Inspect raw tool log", e))?
        .len()
        > max_bytes
    {
        return Err(CoreError::OutputLimit { limit: max_bytes });
    }
    let mut data = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(|e| CoreError::io("Read raw tool log", e))?;
    if data.len() as u64 > max_bytes {
        return Err(CoreError::OutputLimit { limit: max_bytes });
    }
    Ok(data)
}
fn read_utf8(path: &Path, max_bytes: u64) -> Result<String> {
    String::from_utf8(read_bytes(path, max_bytes)?).map_err(|e| {
        CoreError::Integrity(format!(
            "Tool output is not UTF-8: {e}. Raw bytes are retained at {}",
            path.display()
        ))
    })
}
#[derive(Default)]
struct GateState {
    active: usize,
    frozen: bool,
    cleanup_error: Option<String>,
}
#[derive(Default)]
struct Gate {
    state: Mutex<GateState>,
    changed: Condvar,
}
#[derive(Clone)]
pub struct Runner {
    directory: PathBuf,
    cancel: CancellationToken,
    options: RunnerOptions,
    gate: Arc<Gate>,
    observers: Arc<Mutex<Vec<SyncSender<LogEvent>>>>,
}
struct ActiveGuard(Arc<Gate>);
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        self.0.changed.notify_all();
    }
}
pub struct QuiescentGuard(Arc<Gate>);

/// Injection boundary for deterministic process fixtures. Test executors cannot
/// certify native compatibility unless they deliberately override provenance.
pub trait ToolExecutor: Send + Sync {
    fn run(&self, spec: &CommandSpec) -> Result<ProcessResult>;
    fn quiescent(&self) -> Result<Box<dyn Send>> {
        Err(CoreError::Unsupported(
            "This executor does not provide a process quiescence barrier".into(),
        ))
    }
    /// Freeze starts only after every owned tree and capture thread has ended.
    /// Cleanup proof must remain available after user cancellation. Executors
    /// without that proof fail closed and retain their workspace for recovery.
    fn completed_tree_guard(&self) -> Result<Box<dyn Send>> {
        self.quiescent()
    }
    fn native_execution(&self) -> bool {
        false
    }
}

impl ToolExecutor for Runner {
    fn run(&self, spec: &CommandSpec) -> Result<ProcessResult> {
        Runner::run(self, spec)
    }
    fn quiescent(&self) -> Result<Box<dyn Send>> {
        Ok(Box::new(Runner::quiescent(self)?))
    }
    fn completed_tree_guard(&self) -> Result<Box<dyn Send>> {
        Ok(Box::new(Runner::completed_tree_guard(self)?))
    }
    fn native_execution(&self) -> bool {
        cfg!(windows)
    }
}
impl Drop for QuiescentGuard {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.frozen = false;
        self.0.changed.notify_all();
    }
}
impl Runner {
    pub fn new(
        directory: impl AsRef<Path>,
        cancel: CancellationToken,
        options: RunnerOptions,
    ) -> Result<Self> {
        if options.timeout.is_zero() || options.output_limit_bytes == 0 {
            return Err(CoreError::Unsupported(
                "Tool timeout and output limit must be positive".into(),
            ));
        }
        fs::create_dir_all(directory.as_ref())
            .map_err(|e| CoreError::io("Create tool-log directory", e))?;
        Ok(Self {
            directory: directory.as_ref().to_path_buf(),
            cancel,
            options,
            gate: Arc::default(),
            observers: Arc::default(),
        })
    }
    /// Slow UI consumers cannot block tools. Missed previews are counted in the receipt;
    /// every byte remains in the raw log, allowing consumers to catch up by file offset.
    pub fn subscribe(&self, capacity: usize) -> Receiver<LogEvent> {
        let (tx, rx) = mpsc::sync_channel(capacity.max(1));
        self.observers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tx);
        rx
    }
    fn enter(&self) -> Result<ActiveGuard> {
        let deadline = Instant::now() + self.options.timeout;
        let mut state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            self.cancel.check()?;
            if let Some(error) = &state.cleanup_error {
                return Err(CoreError::Integrity(error.clone()));
            }
            if !state.frozen {
                state.active += 1;
                return Ok(ActiveGuard(self.gate.clone()));
            }
            if Instant::now() >= deadline {
                return Err(CoreError::Timeout {
                    program: "waiting for package snapshot".into(),
                    seconds: self.options.timeout.as_secs_f64(),
                });
            }
            state = self
                .gate
                .changed
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
    /// Wait for all owned tool trees and prevent new starts until this guard drops.
    pub fn quiescent(&self) -> Result<QuiescentGuard> {
        let deadline = Instant::now() + self.options.timeout;
        let mut state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
        while state.frozen {
            self.cancel.check()?;
            if Instant::now() >= deadline {
                return Err(CoreError::Timeout {
                    program: "waiting for package snapshot".into(),
                    seconds: self.options.timeout.as_secs_f64(),
                });
            }
            state = self
                .gate
                .changed
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        state.frozen = true;
        let frozen = QuiescentGuard(self.gate.clone());
        while state.active > 0 {
            if self.cancel.is_cancelled() || Instant::now() >= deadline {
                drop(state);
                drop(frozen);
                self.cancel.check()?;
                return Err(CoreError::Timeout {
                    program: "waiting for owned tools to exit".into(),
                    seconds: self.options.timeout.as_secs_f64(),
                });
            }
            state = self
                .gate
                .changed
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        let error = state.cleanup_error.clone();
        drop(state);
        if let Some(error) = error {
            drop(frozen);
            return Err(CoreError::Integrity(error));
        }
        if let Err(error) = self.cancel.check() {
            drop(frozen);
            return Err(error);
        }
        Ok(frozen)
    }
    /// Non-cancellable cleanup proof. The caller may release workspace ownership
    /// only while this guard prevents another tool start. It never waits on an
    /// unproven live tree, so failed cleanup cannot hang lease finalization.
    pub fn completed_tree_guard(&self) -> Result<QuiescentGuard> {
        let mut state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(error) = &state.cleanup_error {
            return Err(CoreError::Integrity(error.clone()));
        }
        if state.active != 0 {
            return Err(CoreError::Integrity(
                "Owned tool commands are still active; workspace cleanup is unproven".into(),
            ));
        }
        if state.frozen {
            return Err(CoreError::Integrity(
                "Another owned process barrier is active; workspace ownership cannot be released"
                    .into(),
            ));
        }
        state.frozen = true;
        Ok(QuiescentGuard(self.gate.clone()))
    }
    pub fn run(&self, spec: &CommandSpec) -> Result<ProcessResult> {
        if spec.timeout.is_some_and(|limit| limit.is_zero()) {
            return Err(CoreError::Integrity(
                "Command timeout must be positive".into(),
            ));
        }
        let _active = self.enter()?;
        let command_id = Uuid::new_v4().to_string();
        let stdout_path = self.directory.join(format!("{command_id}-stdout.bin"));
        let stderr_path = self.directory.join(format!("{command_id}-stderr.bin"));
        let receipt_path = self.directory.join(format!("{command_id}.json"));
        // The receipt keeps the arguments, except a Steam account name (DepotDownloader's -username).
        let raw: Vec<String> = spec.args.iter().map(|s| s.to_string_lossy().into_owned()).collect();
        let args: Vec<String> = raw.iter().enumerate()
            .map(|(i, a)| if i > 0 && raw[i - 1].eq_ignore_ascii_case("-username") { "<account>".to_owned() } else { a.clone() })
            .collect();
        let mut receipt = serde_json::json!({"schema_version":1,"command_id":command_id,"program":spec.program,
            "args":args,"cwd":spec.cwd,
            "started_utc":Utc::now(),"status":"running","stdout_path":stdout_path,"stderr_path":stderr_path,
            "timeout_seconds":spec.timeout.unwrap_or(self.options.timeout).min(self.options.timeout).as_secs_f64()});
        safety::write_json(&receipt_path, &receipt)?;
        let start = Instant::now();
        let outcome = self.run_inner(
            spec,
            &command_id,
            &stdout_path,
            &stderr_path,
            &receipt_path,
            start,
        );
        receipt["finished_utc"] = serde_json::to_value(Utc::now())?;
        receipt["elapsed_ns"] = serde_json::json!(elapsed(start));
        match &outcome {
            Ok(result) => {
                receipt["status"] = serde_json::json!(if result.success {
                    "succeeded"
                } else {
                    "failed"
                });
                receipt["result"] = serde_json::to_value(result)?;
            }
            Err(error) => {
                receipt["status"] = serde_json::json!(if matches!(error, CoreError::Cancelled) {
                    "cancelled"
                } else {
                    "failed"
                });
                receipt["error"] = serde_json::json!(error.to_string());
            }
        }
        safety::write_json(&receipt_path, &receipt)?;
        for path in [&stdout_path, &stderr_path, &receipt_path] {
            if path.exists() {
                let mut permissions = fs::metadata(path)
                    .map_err(|e| CoreError::io("Inspect completed tool log", e))?
                    .permissions();
                permissions.set_readonly(true);
                fs::set_permissions(path, permissions)
                    .map_err(|e| CoreError::io("Seal completed tool log", e))?;
            }
        }
        outcome
    }
    fn run_inner(
        &self,
        spec: &CommandSpec,
        id: &str,
        stdout_path: &Path,
        stderr_path: &Path,
        receipt_path: &Path,
        start: Instant,
    ) -> Result<ProcessResult> {
        let out = create_log(stdout_path)?;
        let err = create_log(stderr_path)?;
        self.cancel.check()?;
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        let mut command = CommandWrap::from(command);
        let tracker = platform::configure(&mut command)?;
        let child = command
            .spawn()
            .map_err(|e| CoreError::io(format!("Launch {}", spec.program.display()), e))?;
        let mut owned = OwnedChild {
            child,
            tracker,
            armed: true,
            gate: self.gate.clone(),
        };
        let total = Arc::new(AtomicU64::new(0));
        let dropped = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::channel();
        let mut readers = Vec::new();
        for (stream, pipe, file) in [
            (
                OutputStream::Stdout,
                owned
                    .child
                    .stdout()
                    .take()
                    .map(|p| Box::new(p) as Box<dyn Read + Send>),
                out,
            ),
            (
                OutputStream::Stderr,
                owned
                    .child
                    .stderr()
                    .take()
                    .map(|p| Box::new(p) as Box<dyn Read + Send>),
                err,
            ),
        ] {
            let pipe = pipe
                .ok_or_else(|| CoreError::Integrity("Spawned tool has no capture pipe".into()))?;
            let context = ReaderContext {
                id: id.to_string(),
                stream,
                limit: self.options.output_limit_bytes,
                total: total.clone(),
                observers: self.observers.clone(),
                dropped: dropped.clone(),
            };
            let tx = tx.clone();
            match thread::Builder::new()
                .name("dmb-tool-log".into())
                .spawn(move || {
                    let result = capture(pipe, file, context);
                    let _ = tx.send(result);
                }) {
                Ok(handle) => readers.push(handle),
                Err(error) => {
                    let _ = owned.cleanup();
                    return Err(CoreError::io("Start output capture", error));
                }
            }
        }
        drop(tx);
        let timeout = spec
            .timeout
            .unwrap_or(self.options.timeout)
            .min(self.options.timeout);
        let result = (|| {
            let mut ended = 0;
            loop {
                self.cancel.check()?;
                if start.elapsed() >= timeout {
                    return Err(CoreError::Timeout {
                        program: spec.program.display().to_string(),
                        seconds: timeout.as_secs_f64(),
                    });
                }
                match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(result) => {
                        result?;
                        ended += 1;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        thread::sleep(Duration::from_millis(20));
                    }
                }
                if let Some(status) = owned
                    .child
                    .try_wait()
                    .map_err(|e| CoreError::io("Poll owned tool", e))?
                {
                    if ended == 2 && owned.tracker.empty(owned.child.id())? {
                        return Ok(status);
                    }
                }
            }
        })();
        if result.is_err() {
            if let Err(error) = owned.cleanup() {
                self.gate
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .cleanup_error = Some(error.to_string());
                return Err(error);
            }
        } else {
            owned.armed = false;
        }
        let join_deadline = Instant::now() + Duration::from_secs(2);
        while readers.iter().any(|reader| !reader.is_finished()) && Instant::now() < join_deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if readers.iter().any(|reader| !reader.is_finished()) {
            let error =
                "Owned tool output did not close after process-tree cleanup; packaging is blocked";
            self.gate
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .cleanup_error = Some(error.into());
            return Err(CoreError::Integrity(error.into()));
        }
        for reader in readers {
            reader
                .join()
                .map_err(|_| CoreError::Integrity("Tool output capture thread panicked".into()))?;
        }
        let status = result?;
        let output = ProcessResult {
            command_id: id.into(),
            code: status.code(),
            success: status.success(),
            stdout_path: stdout_path.into(),
            stderr_path: stderr_path.into(),
            receipt_path: receipt_path.into(),
            elapsed_ns: elapsed(start),
            stdout_bytes: fs::metadata(stdout_path)
                .map_err(|e| CoreError::io("Inspect stdout capture", e))?
                .len(),
            stderr_bytes: fs::metadata(stderr_path)
                .map_err(|e| CoreError::io("Inspect stderr capture", e))?
                .len(),
            dropped_live_events: dropped.load(Ordering::Relaxed),
        };
        if spec.check && !status.success() {
            return Err(output.failure(&spec.program));
        }
        Ok(output)
    }
}
fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
fn create_log(path: &Path) -> Result<File> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| CoreError::io(format!("Create raw log {}", path.display()), e))
}
struct ReaderContext {
    id: String,
    stream: OutputStream,
    limit: u64,
    total: Arc<AtomicU64>,
    observers: Arc<Mutex<Vec<SyncSender<LogEvent>>>>,
    dropped: Arc<AtomicU64>,
}
fn capture(mut pipe: Box<dyn Read + Send>, mut file: File, context: ReaderContext) -> Result<()> {
    let mut buffer = [0u8; 64 * 1024];
    let mut offset = 0;
    let result = (|| loop {
        let count = pipe
            .read(&mut buffer)
            .map_err(|e| CoreError::io("Read tool output", e))?;
        if count == 0 {
            return Ok(());
        }
        let previous = context.total.fetch_add(count as u64, Ordering::AcqRel);
        let allowed = count.min(context.limit.saturating_sub(previous) as usize);
        if allowed > 0 {
            file.write_all(&buffer[..allowed])
                .map_err(|e| CoreError::io("Write raw tool log", e))?;
            file.flush()
                .map_err(|e| CoreError::io("Flush raw tool log", e))?;
            let event = LogEvent {
                command_id: context.id.clone(),
                stream: context.stream.clone(),
                timestamp_utc: Utc::now(),
                offset_bytes: offset,
                byte_count: allowed as u64,
                preview: String::from_utf8_lossy(&buffer[..allowed.min(8192)]).into_owned(),
            };
            context
                .observers
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|observer| match observer.try_send(event.clone()) {
                    Ok(()) => true,
                    Err(mpsc::TrySendError::Full(_)) => {
                        context.dropped.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                    Err(mpsc::TrySendError::Disconnected(_)) => false,
                });
            offset += allowed as u64;
        }
        if allowed < count {
            return Err(CoreError::OutputLimit {
                limit: context.limit,
            });
        }
    })();
    file.sync_all()
        .map_err(|e| CoreError::io("Sync raw tool log", e))?;
    result
}
struct OwnedChild {
    child: Box<dyn ChildWrapper>,
    tracker: platform::Tracker,
    armed: bool,
    gate: Arc<Gate>,
}
impl OwnedChild {
    fn cleanup(&mut self) -> Result<()> {
        // A group can be empty already. Preserve actual cleanup failures otherwise.
        if !self.tracker.empty(self.child.id())? {
            self.child
                .start_kill()
                .map_err(|e| CoreError::io("Terminate owned tool tree", e))?;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let root_done = self
                .child
                .try_wait()
                .map_err(|e| CoreError::io("Reap owned tool", e))?
                .is_some();
            if root_done && self.tracker.empty(self.child.id())? {
                self.armed = false;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(CoreError::Integrity(
                    "Owned tool tree did not terminate; packaging is blocked".into(),
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.armed {
            if let Err(error) = self.cleanup() {
                self.gate
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .cleanup_error = Some(error.to_string());
            }
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use nix::{errno::Errno, sys::signal::killpg, unistd::Pid};
    pub struct Tracker;
    pub fn configure(command: &mut CommandWrap) -> Result<Tracker> {
        command.wrap(process_wrap::std::ProcessGroup::leader());
        Ok(Tracker)
    }
    impl Tracker {
        pub fn empty(&self, pid: u32) -> Result<bool> {
            #[cfg(target_os = "linux")]
            {
                match killpg(Pid::from_raw(pid as i32), None) {
                    Err(Errno::ESRCH) => return Ok(true),
                    Err(error) => {
                        return Err(CoreError::io("Inspect owned process group", error.into()));
                    }
                    Ok(()) => {}
                }
                let own = fs::read_to_string("/proc/self/status")
                    .map_err(|e| CoreError::io("Read process namespace", e))?;
                let levels: Vec<u32> = own
                    .lines()
                    .find_map(|line| line.strip_prefix("NSpid:"))
                    .ok_or_else(|| {
                        CoreError::Unsupported(
                            "Linux process namespace identifiers are unavailable".into(),
                        )
                    })?
                    .split_whitespace()
                    .filter_map(|id| id.parse().ok())
                    .collect();
                let namespace_index = levels
                    .iter()
                    .position(|id| *id == std::process::id())
                    .ok_or_else(|| {
                        CoreError::Integrity("Cannot identify current Linux PID namespace".into())
                    })?;
                // Group membership remains observable when the group leader exits. Ignore
                // reparented zombies: they cannot write any output or generated artifact.
                for entry in fs::read_dir("/proc")
                    .map_err(|e| CoreError::io("Inspect owned process group", e))?
                {
                    let entry = entry.map_err(|e| CoreError::io("Read process entry", e))?;
                    if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
                        continue;
                    }
                    match fs::read_to_string(entry.path().join("status")) {
                        Ok(status) => {
                            let group = status
                                .lines()
                                .find_map(|line| line.strip_prefix("NSpgid:"))
                                .and_then(|values| values.split_whitespace().nth(namespace_index))
                                .and_then(|id| id.parse::<u32>().ok());
                            let state = status
                                .lines()
                                .find_map(|line| line.strip_prefix("State:"))
                                .and_then(|value| value.split_whitespace().next());
                            if group == Some(pid) && !matches!(state, Some("Z" | "X")) {
                                return Ok(false);
                            }
                        }
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                            ) => {}
                        Err(e) => return Err(CoreError::io("Read process-group state", e)),
                    }
                }
                Ok(true)
            }
            #[cfg(not(target_os = "linux"))]
            {
                match killpg(Pid::from_raw(pid as i32), None) {
                    Ok(()) => Ok(false),
                    Err(Errno::ESRCH) => Ok(true),
                    Err(error) => Err(CoreError::io("Inspect owned process group", error.into())),
                }
            }
        }
    }
}
#[cfg(windows)]
mod platform {
    use super::*;
    use process_wrap::std::{CommandWrapper, CreationFlags, JobObject};
    use std::os::windows::io::AsRawHandle;
    use win32job::{ExtendedLimitInfo, Job};
    #[derive(Clone, Debug)]
    pub struct Tracker(Arc<Job>);
    impl CommandWrapper for Tracker {
        fn wrap_child(
            &mut self,
            mut child: Box<dyn ChildWrapper>,
            _: &CommandWrap,
        ) -> std::io::Result<Box<dyn ChildWrapper>> {
            let result = child
                .process_handle()
                .ok_or_else(|| std::io::Error::other("Missing process handle"))
                .and_then(|handle| {
                    self.0
                        .assign_process(handle.as_raw_handle() as isize)
                        .map_err(std::io::Error::other)
                });
            if let Err(error) = result {
                let _ = child.start_kill();
                let _ = child.wait();
                return Err(error);
            }
            Ok(child)
        }
    }
    pub fn configure(command: &mut CommandWrap) -> Result<Tracker> {
        let mut limits = ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let tracker = Tracker(Arc::new(Job::create_with_limit_info(&limits).map_err(
            |e| CoreError::Integrity(format!("Create owned process job: {e}")),
        )?));
        // All pre_spawn hooks execute before CreateProcess. JobObject suspends the
        // root, then Tracker assigns its observable outer job before JobObject
        // assigns the nested job and resumes. No child can escape the assignment race.
        command
            .wrap(tracker.clone())
            .wrap(CreationFlags(
                windows::Win32::System::Threading::CREATE_NO_WINDOW,
            ))
            .wrap(JobObject);
        Ok(tracker)
    }
    impl Tracker {
        pub fn empty(&self, _: u32) -> Result<bool> {
            self.0
                .query_process_id_list()
                .map(|ids| ids.is_empty())
                .map_err(|e| CoreError::Integrity(format!("Inspect owned process job: {e}")))
        }
    }
}

#[cfg(test)]
mod cleanup_guard_tests {
    use super::*;
    #[test]
    fn completed_tree_guard_releases_clean_cancelled_operations_and_freezes_starts() {
        let temp = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let runner = Runner::new(temp.path(), cancel.clone(), RunnerOptions::default()).unwrap();
        cancel.cancel();
        assert!(matches!(runner.quiescent(), Err(CoreError::Cancelled)));
        let guard = runner.completed_tree_guard().unwrap();
        assert!(runner.gate.state.lock().unwrap().frozen);
        assert!(runner.completed_tree_guard().is_err());
        drop(guard);
        assert!(!runner.gate.state.lock().unwrap().frozen);
        assert!(runner.completed_tree_guard().is_ok());
    }
    #[test]
    fn completed_tree_guard_refuses_live_or_poisoned_tree_even_after_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let runner = Runner::new(temp.path(), cancel.clone(), RunnerOptions::default()).unwrap();
        runner.gate.state.lock().unwrap().active = 1;
        cancel.cancel();
        assert!(
            runner
                .completed_tree_guard()
                .unwrap_err_text()
                .contains("still active")
        );
        {
            let mut state = runner.gate.state.lock().unwrap();
            state.active = 0;
            state.cleanup_error = Some("owned capture pipe remains open".into());
        }
        assert!(
            runner
                .completed_tree_guard()
                .unwrap_err_text()
                .contains("pipe remains open")
        );
        assert!(!runner.gate.state.lock().unwrap().frozen);
    }
    trait ErrorText {
        fn unwrap_err_text(self) -> String;
    }
    impl<T> ErrorText for Result<T> {
        fn unwrap_err_text(self) -> String {
            match self {
                Ok(_) => panic!("expected cleanup rejection"),
                Err(e) => e.to_string(),
            }
        }
    }
}
