//! Accurate nested wall-clock timings. A parent's self time subtracts the union of
//! direct child intervals, so parallel children never double-subtract elapsed time.
use crate::{CoreError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver, SyncSender},
    },
    time::Instant,
};
use uuid::Uuid;

pub type StepId = String;
pub use crate::timing_import::read_report;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingStatus {
    Running,
    Succeeded,
    Cached,
    Skipped,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingEventKind {
    Started,
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimingEvent {
    pub sequence: u64,
    pub event: TimingEventKind,
    pub step_id: StepId,
    pub parent_id: Option<StepId>,
    pub name: String,
    pub timestamp_utc: DateTime<Utc>,
    pub offset_ns: u64,
    pub status: TimingStatus,
    pub error: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepTiming {
    pub id: StepId,
    pub parent_id: Option<StepId>,
    pub name: String,
    pub started_utc: DateTime<Utc>,
    pub finished_utc: Option<DateTime<Utc>>,
    pub status: TimingStatus,
    pub inclusive_ns: u64,
    pub exclusive_ns: u64,
    pub error: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimingReport {
    pub schema_version: u32,
    pub operation: String,
    pub run_id: String,
    pub started_utc: DateTime<Utc>,
    pub finished_utc: Option<DateTime<Utc>>,
    pub status: TimingStatus,
    /// Elapsed wall time for the entire operation, not the sum of step times.
    pub total_ns: u64,
    pub steps: Vec<StepTiming>,
    pub events: Vec<TimingEvent>,
    #[serde(default)]
    pub dropped_live_events: u64,
    #[serde(default)]
    pub observer_errors: Vec<String>,
}

#[derive(Debug)]
struct Span {
    timing: StepTiming,
    start_ns: u64,
    end_ns: Option<u64>,
}

#[derive(Debug)]
struct State {
    epoch: Instant,
    operation: String,
    run_id: String,
    started_utc: DateTime<Utc>,
    finished_utc: Option<DateTime<Utc>>,
    end_ns: Option<u64>,
    status: TimingStatus,
    spans: Vec<Span>,
    events: Vec<TimingEvent>,
    observers: Vec<SyncSender<TimingEvent>>,
    dropped_live_events: u64,
    journal: Option<File>,
    observer_errors: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TimingRecorder(Arc<Mutex<State>>);

/// A dropped unfinished step is recorded as failed, including during unwinding.
#[derive(Debug)]
pub struct StepGuard {
    recorder: TimingRecorder,
    id: StepId,
    finished: bool,
}

fn nanos(state: &State) -> u64 {
    state.epoch.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

impl TimingRecorder {
    pub fn new(operation: impl Into<String>) -> Self {
        Self(Arc::new(Mutex::new(State {
            epoch: Instant::now(),
            operation: operation.into(),
            run_id: Uuid::new_v4().to_string(),
            started_utc: Utc::now(),
            finished_utc: None,
            end_ns: None,
            status: TimingStatus::Running,
            spans: vec![],
            events: vec![],
            observers: vec![],
            dropped_live_events: 0,
            journal: None,
            observer_errors: vec![],
        })))
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // Retain diagnostic evidence even if another thread panicked.
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Live events are bounded and cannot block a compiler. Recover missed events from report().events.
    pub fn subscribe(&self, capacity: usize) -> Receiver<TimingEvent> {
        let (tx, rx) = mpsc::sync_channel(capacity.max(1));
        self.state().observers.push(tx);
        rx
    }

    /// Append every event separately from native stdout, with sync on each boundary.
    /// Set before the first step; partial failed/cancelled operations remain inspectable.
    pub fn enable_journal(&self, path: &Path) -> Result<()> {
        let mut state = self.state();
        if state.journal.is_some() || !state.events.is_empty() {
            return Err(CoreError::Timing(
                "Enable the timing journal once, before starting steps".into(),
            ));
        }
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|e| CoreError::io("Create timing event journal", e))?;
        state.journal = Some(file);
        Ok(())
    }

    pub fn start_step(
        &self,
        name: impl Into<String>,
        parent_id: Option<&str>,
    ) -> Result<StepGuard> {
        let mut state = self.state();
        if state.status != TimingStatus::Running {
            return Err(CoreError::Timing("operation already finished".into()));
        }
        if let Some(parent) = parent_id {
            if !state
                .spans
                .iter()
                .any(|span| span.timing.id == parent && span.end_ns.is_none())
            {
                return Err(CoreError::Timing(
                    "parent step is missing or already finished".into(),
                ));
            }
        }
        let id = Uuid::new_v4().to_string();
        let start_ns = nanos(&state);
        let timing = StepTiming {
            id: id.clone(),
            parent_id: parent_id.map(str::to_owned),
            name: name.into(),
            started_utc: Utc::now(),
            finished_utc: None,
            status: TimingStatus::Running,
            inclusive_ns: 0,
            exclusive_ns: 0,
            error: None,
            reason: None,
        };
        let sequence = state.events.len() as u64;
        emit_event(
            &mut state,
            TimingEvent {
                sequence,
                event: TimingEventKind::Started,
                step_id: id.clone(),
                parent_id: timing.parent_id.clone(),
                name: timing.name.clone(),
                timestamp_utc: timing.started_utc,
                offset_ns: start_ns,
                status: TimingStatus::Running,
                error: None,
                reason: None,
            },
        );
        state.spans.push(Span {
            timing,
            start_ns,
            end_ns: None,
        });
        Ok(StepGuard {
            recorder: self.clone(),
            id,
            finished: false,
        })
    }

    /// Record an intentionally omitted step. No work was measured, so its duration is zero.
    pub fn skip_step(
        &self,
        name: impl Into<String>,
        parent_id: Option<&str>,
        reason: impl Into<String>,
    ) -> Result<StepId> {
        let mut guard = self.start_step(name, parent_id)?;
        let mut state = self.state();
        let index = state
            .spans
            .iter()
            .position(|s| s.timing.id == guard.id)
            .expect("newly inserted span");
        let offset = state.spans[index].start_ns;
        let utc = state.spans[index].timing.started_utc;
        state.spans[index].timing.reason = Some(reason.into());
        end_span(&mut state, index, TimingStatus::Skipped, None, offset, utc);
        guard.finished = true;
        Ok(guard.id.clone())
    }

    /// Measure a fallible operation; cancellation and failure retain the step's elapsed time.
    pub fn measure<T>(
        &self,
        name: impl Into<String>,
        parent_id: Option<&str>,
        operation: impl FnOnce(&StepGuard) -> Result<T>,
    ) -> Result<T> {
        let guard = self.start_step(name, parent_id)?;
        match operation(&guard) {
            Ok(value) => {
                guard.success()?;
                Ok(value)
            }
            Err(error) => {
                let status = if matches!(error, CoreError::Cancelled) {
                    TimingStatus::Cancelled
                } else {
                    TimingStatus::Failed
                };
                guard.finish(status, Some(error.to_string()))?;
                Err(error)
            }
        }
    }

    fn finish_step(&self, id: &str, status: TimingStatus, error: Option<String>) -> Result<()> {
        if status == TimingStatus::Running {
            return Err(CoreError::Timing("final status must be terminal".into()));
        }
        let mut state = self.state();
        let index = state
            .spans
            .iter()
            .position(|span| span.timing.id == id)
            .ok_or_else(|| CoreError::Timing("unknown step".into()))?;
        if state.spans[index].end_ns.is_some() {
            return Err(CoreError::Timing("step already finished".into()));
        }
        let mut descendants: HashSet<String> = [id.to_string()].into_iter().collect();
        for span in &state.spans {
            if span
                .timing
                .parent_id
                .as_ref()
                .is_some_and(|parent| descendants.contains(parent))
            {
                descendants.insert(span.timing.id.clone());
            }
        }
        let active_children = state.spans.iter().any(|span| {
            span.timing.id != id && descendants.contains(&span.timing.id) && span.end_ns.is_none()
        });
        if matches!(
            status,
            TimingStatus::Succeeded | TimingStatus::Cached | TimingStatus::Skipped
        ) && active_children
        {
            return Err(CoreError::Timing(
                "cannot succeed while a child step is running".into(),
            ));
        }
        let offset = nanos(&state);
        let utc = Utc::now();
        for index in (0..state.spans.len()).rev() {
            if descendants.contains(&state.spans[index].timing.id)
                && state.spans[index].end_ns.is_none()
            {
                end_span(&mut state, index, status, error.clone(), offset, utc);
            }
        }
        Ok(())
    }

    /// Finish the whole operation. Failure/cancellation closes all active steps.
    /// Success rejects unfinished/failed steps rather than producing false success.
    pub fn finish(&self, status: TimingStatus, error: Option<String>) -> Result<TimingReport> {
        if status == TimingStatus::Running {
            return Err(CoreError::Timing("final status must be terminal".into()));
        }
        {
            let mut state = self.state();
            if state.status != TimingStatus::Running {
                return Err(CoreError::Timing("operation already finished".into()));
            }
            if matches!(
                status,
                TimingStatus::Succeeded | TimingStatus::Cached | TimingStatus::Skipped
            ) && state.spans.iter().any(|span| {
                !matches!(
                    span.timing.status,
                    TimingStatus::Succeeded | TimingStatus::Cached | TimingStatus::Skipped
                )
            }) {
                return Err(CoreError::Timing(
                    "success requires every started step to have succeeded".into(),
                ));
            }
            let offset = nanos(&state);
            let utc = Utc::now();
            for index in (0..state.spans.len()).rev() {
                if state.spans[index].end_ns.is_none() {
                    end_span(&mut state, index, status, error.clone(), offset, utc);
                }
            }
            state.end_ns = Some(offset);
            state.finished_utc = Some(utc);
            state.status = status;
        }
        Ok(self.report())
    }

    pub fn report(&self) -> TimingReport {
        let state = self.state();
        let end = state.end_ns.unwrap_or_else(|| nanos(&state));
        let mut by_parent: HashMap<&str, Vec<&Span>> = HashMap::new();
        for span in &state.spans {
            if let Some(parent) = &span.timing.parent_id {
                by_parent.entry(parent).or_default().push(span);
            }
        }
        let steps = state
            .spans
            .iter()
            .map(|span| {
                let mut timing = span.timing.clone();
                let span_end = span.end_ns.unwrap_or(end);
                timing.inclusive_ns = span_end.saturating_sub(span.start_ns);
                let children = by_parent
                    .remove(timing.id.as_str())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|child| {
                        (
                            child.start_ns.max(span.start_ns),
                            child.end_ns.unwrap_or(end).min(span_end),
                        )
                    })
                    .collect();
                timing.exclusive_ns = timing.inclusive_ns.saturating_sub(interval_union(children));
                timing
            })
            .collect();
        TimingReport {
            schema_version: 1,
            operation: state.operation.clone(),
            run_id: state.run_id.clone(),
            started_utc: state.started_utc,
            finished_utc: state.finished_utc,
            status: state.status,
            total_ns: end,
            steps,
            events: state.events.clone(),
            dropped_live_events: state.dropped_live_events,
            observer_errors: state.observer_errors.clone(),
        }
    }

    pub fn write_json(&self, path: &Path) -> Result<()> {
        crate::safety::write_json(path, &self.report())
    }
}

fn end_span(
    state: &mut State,
    index: usize,
    status: TimingStatus,
    error: Option<String>,
    offset: u64,
    utc: DateTime<Utc>,
) {
    let span = &mut state.spans[index];
    span.end_ns = Some(offset);
    span.timing.finished_utc = Some(utc);
    span.timing.status = status;
    span.timing.error = error.clone();
    let event = TimingEvent {
        sequence: state.events.len() as u64,
        event: TimingEventKind::Finished,
        step_id: span.timing.id.clone(),
        parent_id: span.timing.parent_id.clone(),
        name: span.timing.name.clone(),
        timestamp_utc: utc,
        offset_ns: offset,
        status,
        error,
        reason: span.timing.reason.clone(),
    };
    emit_event(state, event);
}

fn emit_event(state: &mut State, event: TimingEvent) {
    if let Some(file) = &mut state.journal {
        let result = (|| -> Result<()> {
            let mut bytes = serde_json::to_vec(&event)?;
            bytes.push(b'\n');
            file.write_all(&bytes)
                .map_err(|e| CoreError::io("Write timing journal", e))?;
            file.sync_data()
                .map_err(|e| CoreError::io("Sync timing journal", e))?;
            Ok(())
        })();
        if let Err(error) = result {
            state.observer_errors.push(error.to_string());
            state.journal = None;
        }
    }
    state
        .observers
        .retain(|observer| match observer.try_send(event.clone()) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) => {
                state.dropped_live_events += 1;
                true
            }
            Err(mpsc::TrySendError::Disconnected(_)) => false,
        });
    state.events.push(event);
}

pub(crate) fn interval_union(mut intervals: Vec<(u64, u64)>) -> u64 {
    intervals.retain(|(start, end)| end > start);
    intervals.sort_unstable();
    let mut total = 0;
    let mut current: Option<(u64, u64)> = None;
    for (start, end) in intervals {
        match current {
            Some((left, right)) if start <= right => current = Some((left, right.max(end))),
            Some((left, right)) => {
                total += right - left;
                current = Some((start, end));
            }
            None => current = Some((start, end)),
        }
    }
    total + current.map_or(0, |(start, end)| end - start)
}

impl StepGuard {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn success(self) -> Result<()> {
        self.finish(TimingStatus::Succeeded, None)
    }
    /// Cache checking/retrieval was actually measured; retain its elapsed duration.
    pub fn cached(self) -> Result<()> {
        self.finish(TimingStatus::Cached, None)
    }
    pub fn fail(self, error: impl Into<String>) -> Result<()> {
        self.finish(TimingStatus::Failed, Some(error.into()))
    }
    pub fn cancel(self) -> Result<()> {
        self.finish(TimingStatus::Cancelled, Some("Operation cancelled.".into()))
    }
    pub fn finish(mut self, status: TimingStatus, error: Option<String>) -> Result<()> {
        self.recorder.finish_step(&self.id, status, error)?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for StepGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.recorder.finish_step(
                &self.id,
                TimingStatus::Failed,
                Some("Step ended without an explicit outcome.".into()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_parallel_children_are_not_double_subtracted() {
        assert_eq!(
            interval_union(vec![(10, 50), (20, 70), (75, 90), (80, 85)]),
            75
        );
        let clock = TimingRecorder::new("build");
        let parent = clock.start_step("prepare", None).unwrap();
        let first = clock.start_step("materials", Some(parent.id())).unwrap();
        let second = clock.start_step("textures", Some(parent.id())).unwrap();
        first.success().unwrap();
        second.success().unwrap();
        parent.success().unwrap();
        let mut state = clock.state();
        for (span, (start, end)) in state.spans.iter_mut().zip([(0, 100), (10, 70), (30, 80)]) {
            span.start_ns = start;
            span.end_ns = Some(end);
        }
        state.end_ns = Some(100);
        drop(state);
        let report = clock.report();
        assert_eq!(report.steps[0].inclusive_ns, 100);
        assert_eq!(report.steps[0].exclusive_ns, 30);
        assert_eq!(report.total_ns, 100);
    }

    #[test]
    fn failed_and_cancelled_steps_keep_measured_time() {
        for status in [TimingStatus::Failed, TimingStatus::Cancelled] {
            let clock = TimingRecorder::new("build");
            let parent = clock.start_step("build", None).unwrap();
            let child = clock.start_step("compile", Some(parent.id())).unwrap();
            let report = clock.finish(status, Some("stopped".into())).unwrap();
            assert_eq!(report.status, status);
            assert!(
                report
                    .steps
                    .iter()
                    .all(|s| s.status == status && s.finished_utc.is_some())
            );
            assert!(report.total_ns > 0);
            assert_eq!(report.events.len(), 4);
            assert_eq!(report.events[2].step_id, child.id());
            assert!(
                serde_json::from_str::<TimingReport>(&serde_json::to_string(&report).unwrap())
                    .is_ok()
            );
        }
    }

    #[test]
    fn unfinished_step_cannot_be_reported_successfully() {
        let clock = TimingRecorder::new("build");
        {
            let _unfinished = clock.start_step("analyze", None).unwrap();
        }
        assert_eq!(clock.report().steps[0].status, TimingStatus::Failed);
        assert!(clock.finish(TimingStatus::Succeeded, None).is_err());
        clock.finish(TimingStatus::Failed, None).unwrap();
        assert!(clock.start_step("late", None).is_err());
    }

    #[test]
    fn invalid_parent_and_premature_success_are_rejected() {
        let clock = TimingRecorder::new("build");
        assert!(clock.start_step("orphan", Some("missing")).is_err());
        let parent = clock.start_step("build", None).unwrap();
        let _child = clock.start_step("compile", Some(parent.id())).unwrap();
        assert!(parent.success().is_err());
        assert!(
            clock
                .report()
                .steps
                .iter()
                .all(|s| s.status == TimingStatus::Failed)
        );
    }

    #[test]
    fn measure_retains_original_failure_and_cancellation() {
        let clock = TimingRecorder::new("analyze");
        assert!(matches!(
            clock.measure::<()>("reading", None, |_| Err(CoreError::Cancelled)),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(clock.report().steps[0].status, TimingStatus::Cancelled);
        assert_eq!(clock.report().events[0].sequence, 0);
        assert_eq!(clock.report().events[1].sequence, 1);
    }

    #[test]
    fn durable_journal_and_bounded_live_observer_keep_every_boundary() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        let clock = TimingRecorder::new("build");
        clock.enable_journal(&path).unwrap();
        let live = clock.subscribe(1);
        clock
            .start_step("check cache", None)
            .unwrap()
            .cached()
            .unwrap();
        clock
            .skip_step("install", None, "Installation disabled")
            .unwrap();
        let report = clock.finish(TimingStatus::Succeeded, None).unwrap();
        assert_eq!(report.steps[0].status, TimingStatus::Cached);
        assert_eq!(report.steps[1].inclusive_ns, 0);
        assert_eq!(
            report.steps[1].reason.as_deref(),
            Some("Installation disabled")
        );
        assert_eq!(report.dropped_live_events, 3);
        assert_eq!(live.try_iter().count(), 1);
        let events: Vec<TimingEvent> = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(events.len(), 4);
        assert_eq!(events[3].status, TimingStatus::Skipped);
        assert!(report.observer_errors.is_empty());
    }
}
