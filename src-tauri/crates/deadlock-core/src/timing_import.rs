use crate::{
    CoreError, Result,
    timing::{StepTiming, TimingEvent, TimingEventKind, TimingReport, TimingStatus},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::Path,
};

const MAX_REPORT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_STEPS: usize = 100_000;
fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Timing(format!("Invalid timing report: {}", message.into()))
}

/// Load current Rust reports or measured a34 Python reports. No durations are
/// inferred from compiler text, file timestamps, line counts, or total sums.
pub fn read_report(path: &Path) -> Result<TimingReport> {
    let file = File::open(path).map_err(|e| CoreError::io("Open timing report", e))?;
    if file
        .metadata()
        .map_err(|e| CoreError::io("Inspect timing report", e))?
        .len()
        > MAX_REPORT_BYTES
    {
        return Err(invalid("report exceeds 32 MiB"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_REPORT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| CoreError::io("Read timing report", e))?;
    if bytes.len() as u64 > MAX_REPORT_BYTES {
        return Err(invalid("report exceeds 32 MiB"));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let report = if value.get("records").is_some() {
        convert_python(serde_json::from_value(value)?)?
    } else {
        serde_json::from_value(value)?
    };
    validate(&report)?;
    Ok(report)
}
#[derive(Deserialize)]
struct PythonReport {
    schema_version: u32,
    operation_id: String,
    total_seconds: f64,
    status: String,
    clock: String,
    duration_unit: String,
    durations_are_inclusive: bool,
    records: Vec<PythonRecord>,
    #[serde(default)]
    observer_errors: Vec<String>,
}
#[derive(Deserialize)]
struct PythonRecord {
    id: String,
    parent_id: Option<String>,
    name: String,
    status: String,
    started_utc: DateTime<Utc>,
    #[serde(default)]
    finished_utc: Option<DateTime<Utc>>,
    start_offset_seconds: f64,
    elapsed_seconds: f64,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    details: serde_json::Value,
}
fn nanoseconds(seconds: f64) -> Result<u64> {
    if !seconds.is_finite() || seconds < 0.0 || seconds > (u64::MAX as f64) / 1_000_000_000.0 {
        return Err(invalid("duration is negative, non-finite, or too large"));
    }
    Ok((seconds * 1_000_000_000.0).round() as u64)
}
fn status(value: &str) -> Result<TimingStatus> {
    match value {
        "running" => Ok(TimingStatus::Running),
        "completed" | "succeeded" => Ok(TimingStatus::Succeeded),
        "cached" => Ok(TimingStatus::Cached),
        "skipped" => Ok(TimingStatus::Skipped),
        "failed" => Ok(TimingStatus::Failed),
        "cancelled" => Ok(TimingStatus::Cancelled),
        _ => Err(invalid(format!("unknown status {value:?}"))),
    }
}
fn convert_python(source: PythonReport) -> Result<TimingReport> {
    if source.schema_version != 1
        || source.clock != "monotonic"
        || source.duration_unit != "seconds"
        || !source.durations_are_inclusive
    {
        return Err(invalid("unsupported Python schema or duration semantics"));
    }
    if source.records.len() > MAX_STEPS {
        return Err(invalid("too many steps"));
    }
    let roots: Vec<_> = source
        .records
        .iter()
        .filter(|r| r.parent_id.is_none())
        .collect();
    if roots.len() != 1 {
        return Err(invalid(
            "Python report must contain exactly one operation root",
        ));
    }
    let root = roots[0];
    let total = nanoseconds(source.total_seconds)?;
    if total.abs_diff(nanoseconds(root.elapsed_seconds)?) > 2 {
        return Err(invalid("root elapsed time disagrees with operation total"));
    }
    if source.status != root.status {
        return Err(invalid("root status disagrees with operation status"));
    }
    let mut steps = Vec::new();
    let mut events = Vec::new();
    let mut intervals = HashMap::new();
    for row in &source.records {
        let start = nanoseconds(row.start_offset_seconds)?;
        let duration = nanoseconds(row.elapsed_seconds)?;
        let end = start
            .checked_add(duration)
            .ok_or_else(|| invalid("step end overflows"))?;
        if intervals.insert(row.id.clone(), (start, end)).is_some() {
            return Err(invalid("duplicate step ID"));
        }
        let outcome = status(&row.status)?;
        let reason = row
            .details
            .get("reason")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        steps.push(StepTiming {
            id: row.id.clone(),
            parent_id: row.parent_id.clone(),
            name: row.name.clone(),
            started_utc: row.started_utc,
            finished_utc: row.finished_utc,
            status: outcome,
            inclusive_ns: duration,
            exclusive_ns: duration,
            error: row.error.clone(),
            reason: reason.clone(),
        });
        events.push(TimingEvent {
            sequence: 0,
            event: TimingEventKind::Started,
            step_id: row.id.clone(),
            parent_id: row.parent_id.clone(),
            name: row.name.clone(),
            timestamp_utc: row.started_utc,
            offset_ns: start,
            status: TimingStatus::Running,
            error: None,
            reason: None,
        });
        if let Some(finished) = row.finished_utc {
            events.push(TimingEvent {
                sequence: 0,
                event: TimingEventKind::Finished,
                step_id: row.id.clone(),
                parent_id: row.parent_id.clone(),
                name: row.name.clone(),
                timestamp_utc: finished,
                offset_ns: end,
                status: outcome,
                error: row.error.clone(),
                reason,
            });
        }
    }
    let mut child_intervals: HashMap<&str, Vec<(u64, u64)>> = HashMap::new();
    for row in &source.records {
        if let Some(parent) = &row.parent_id {
            child_intervals
                .entry(parent)
                .or_default()
                .push(intervals[&row.id]);
        }
    }
    // Direct-child union subtracts overlapping work once, in O(n log n) overall.
    for step in &mut steps {
        let (start, end) = intervals[&step.id];
        let children = child_intervals
            .remove(step.id.as_str())
            .unwrap_or_default()
            .into_iter()
            .map(|(s, e)| (s.max(start), e.min(end)))
            .collect();
        step.exclusive_ns = step
            .inclusive_ns
            .saturating_sub(crate::timing::interval_union(children));
    }
    events.sort_by_key(|e| e.offset_ns);
    for (index, event) in events.iter_mut().enumerate() {
        event.sequence = index as u64;
    }
    Ok(TimingReport {
        schema_version: 1,
        operation: root.name.clone(),
        run_id: source.operation_id,
        started_utc: root.started_utc,
        finished_utc: root.finished_utc,
        status: status(&source.status)?,
        total_ns: total,
        steps,
        events,
        dropped_live_events: 0,
        observer_errors: source.observer_errors,
    })
}
fn validate(report: &TimingReport) -> Result<()> {
    if report.schema_version != 1 {
        return Err(invalid("unsupported schema version"));
    }
    if report.run_id.is_empty() || report.operation.is_empty() {
        return Err(invalid("missing operation identity"));
    }
    if report.steps.len() > MAX_STEPS || report.events.len() > MAX_STEPS * 2 {
        return Err(invalid("too many steps/events"));
    }
    if (report.status == TimingStatus::Running) != report.finished_utc.is_none() {
        return Err(invalid("operation status disagrees with finish timestamp"));
    }
    let mut by_id = HashMap::new();
    for step in &report.steps {
        if step.id.is_empty()
            || step.name.is_empty()
            || by_id.insert(step.id.as_str(), step).is_some()
        {
            return Err(invalid("missing or duplicate step identity"));
        }
        if step.exclusive_ns > step.inclusive_ns {
            return Err(invalid("self time exceeds elapsed time"));
        }
        if step.inclusive_ns > report.total_ns.saturating_add(2) {
            return Err(invalid("step exceeds operation duration"));
        }
        if (step.status == TimingStatus::Running) != step.finished_utc.is_none() {
            return Err(invalid("step status disagrees with finish timestamp"));
        }
    }
    let mut done = HashSet::new();
    for step in &report.steps {
        let mut chain = HashSet::new();
        let mut current = Some(step.id.as_str());
        while let Some(id) = current {
            if done.contains(id) {
                break;
            }
            if !chain.insert(id) {
                return Err(invalid("parent cycle"));
            }
            current = by_id
                .get(id)
                .ok_or_else(|| invalid("missing parent step"))?
                .parent_id
                .as_deref();
        }
        done.extend(chain);
    }
    let mut started = HashSet::new();
    let mut finished = HashSet::new();
    let mut previous = 0;
    for (index, event) in report.events.iter().enumerate() {
        if event.sequence != index as u64
            || event.offset_ns < previous
            || event.offset_ns > report.total_ns.saturating_add(2)
        {
            return Err(invalid("invalid event order or offset"));
        }
        previous = event.offset_ns;
        let step = by_id
            .get(event.step_id.as_str())
            .ok_or_else(|| invalid("event references unknown step"))?;
        if event.parent_id != step.parent_id || event.name != step.name {
            return Err(invalid("event identity disagrees with step"));
        }
        match event.event {
            TimingEventKind::Started => {
                if event.status != TimingStatus::Running || !started.insert(event.step_id.as_str())
                {
                    return Err(invalid("invalid or duplicate start event"));
                }
            }
            TimingEventKind::Finished => {
                if !started.contains(event.step_id.as_str())
                    || !finished.insert(event.step_id.as_str())
                    || event.status != step.status
                {
                    return Err(invalid("invalid or duplicate finish event"));
                }
            }
        }
    }
    if started.len() != report.steps.len()
        || finished.len()
            != report
                .steps
                .iter()
                .filter(|s| s.status != TimingStatus::Running)
                .count()
    {
        return Err(invalid("missing step boundary events"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> serde_json::Value {
        serde_json::json!({"schema_version":1,"operation_id":"run","clock":"monotonic","duration_unit":"seconds","durations_are_inclusive":true,"total_seconds":10.0,"status":"completed","records":[
            {"id":"root","parent_id":null,"name":"Build","status":"completed","started_utc":"2026-10-06T01:00:00Z","finished_utc":"2026-10-06T01:00:10Z","start_offset_seconds":0.0,"elapsed_seconds":10.0},
            {"id":"a","parent_id":"root","name":"Materials","status":"completed","started_utc":"2026-10-06T01:00:01Z","finished_utc":"2026-10-06T01:00:07Z","start_offset_seconds":1.0,"elapsed_seconds":6.0},
            {"id":"b","parent_id":"root","name":"Textures","status":"cached","started_utc":"2026-10-06T01:00:03Z","finished_utc":"2026-10-06T01:00:08Z","start_offset_seconds":3.0,"elapsed_seconds":5.0}
        ]})
    }
    fn read(value: serde_json::Value) -> Result<TimingReport> {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), serde_json::to_vec(&value).unwrap()).unwrap();
        read_report(file.path())
    }
    #[test]
    fn python_import_preserves_total_and_parallel_self_time() {
        let report = read(sample()).unwrap();
        assert_eq!(report.total_ns, 10_000_000_000);
        assert_eq!(report.steps[0].exclusive_ns, 3_000_000_000);
        assert_eq!(report.steps[2].status, TimingStatus::Cached);
    }
    #[test]
    fn malformed_tree_and_duration_are_rejected() {
        for edit in 0..5 {
            let mut value = sample();
            match edit {
                0 => value["records"][1]["elapsed_seconds"] = serde_json::json!(-1),
                1 => value["records"][1]["parent_id"] = serde_json::json!("missing"),
                2 => value["records"][1]["parent_id"] = serde_json::json!("a"),
                3 => value["total_seconds"] = serde_json::json!(99),
                _ => value["records"][1]["id"] = serde_json::json!("root"),
            };
            assert!(read(value).is_err(), "case {edit}");
        }
    }
    #[test]
    fn native_report_round_trips() {
        let clock = crate::TimingRecorder::new("Analyze");
        clock.start_step("read", None).unwrap().success().unwrap();
        let report = clock.finish(TimingStatus::Succeeded, None).unwrap();
        assert_eq!(
            read(serde_json::to_value(&report).unwrap()).unwrap().run_id,
            report.run_id
        );
    }
    #[test]
    fn file_limit_checked_before_json_parse() {
        let file = tempfile::NamedTempFile::new().unwrap();
        file.as_file().set_len(MAX_REPORT_BYTES + 1).unwrap();
        assert!(matches!(
            read_report(file.path()),
            Err(CoreError::Timing(_))
        ));
    }
}
