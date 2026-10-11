use deadlock_core::process::ProcessResult;
use deadlock_desktop::app::AppError;
use std::{fs, path::Path};

fn result(root: &Path, stdout: &[u8], stderr: &[u8]) -> ProcessResult {
    let out = root.join("stdout.bin");
    let err = root.join("stderr.bin");
    fs::write(&out, stdout).unwrap();
    fs::write(&err, stderr).unwrap();
    ProcessResult {
        command_id: "diagnostic-fixture".into(),
        code: Some(1),
        success: false,
        stdout_path: out,
        stderr_path: err,
        receipt_path: root.join("receipt.json"),
        elapsed_ns: 1,
        stdout_bytes: stdout.len() as u64,
        stderr_bytes: stderr.len() as u64,
        dropped_live_events: 0,
    }
}

#[test]
fn compiler_popup_contains_the_actual_stdout_error_when_stderr_is_empty() {
    let temp = tempfile::tempdir().unwrap();
    let stdout =
        include_bytes!("../crates/deadlock-tools/tests/fixtures/missing-gameinfo-stdout.bin");
    let capture = result(temp.path(), stdout, b"");
    let error =
        AppError::from(capture.failure(Path::new("SDK/game/bin_cs2/win64/resourcecompiler.exe")));
    assert_eq!(error.kind, "tool");
    assert_eq!(error.message, "Resource compiler failed");
    assert!(error.details.contains("Can't find 'citadel\\gameinfo.gi'"));
    assert!(error
        .details
        .contains("stderr:\n(no output on this stream)"));
    assert!(error
        .details
        .contains(capture.stdout_path.to_str().unwrap()));
    assert_eq!(fs::read(&capture.stdout_path).unwrap(), stdout);
}

#[test]
fn large_or_unreadable_diagnostics_do_not_hide_exit_status_or_grow_popup_unbounded() {
    let temp = tempfile::tempdir().unwrap();
    let stdout = format!("{}\nstdout failure", "模型".repeat(10_000));
    let stderr = format!("{}\nstderr failure", "x".repeat(30_000));
    let capture = result(temp.path(), stdout.as_bytes(), stderr.as_bytes());
    let error = AppError::from(capture.failure(Path::new("tool.exe")));
    assert_eq!(error.kind, "tool");
    assert!(error.details.contains("exit code 1"));
    assert!(error.details.contains("stdout failure"));
    assert!(error.details.contains("stderr failure"));
    assert!(error.details.len() < 18_000);
    assert_eq!(fs::read(&capture.stdout_path).unwrap(), stdout.as_bytes());
    fs::remove_file(&capture.stderr_path).unwrap();
    let missing = capture.failure(Path::new("tool.exe")).to_string();
    assert!(missing.contains("exit code 1"));
    assert!(missing.contains("stdout failure"));
    assert!(missing.contains("could not read saved output"));
}
