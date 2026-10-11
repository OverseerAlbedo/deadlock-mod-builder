use deadlock_core::{
    CancellationToken, CoreError,
    process::{CommandSpec, Runner, RunnerOptions},
};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn command(mode: &str) -> CommandSpec {
    CommandSpec::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "tool_fixture", "--nocapture"])
        .env("DMB_TEST_MODE", mode)
}
fn runner(
    dir: &std::path::Path,
    cancel: CancellationToken,
    timeout: Duration,
    limit: u64,
) -> Runner {
    Runner::new(
        dir,
        cancel,
        RunnerOptions {
            timeout,
            output_limit_bytes: limit,
        },
    )
    .unwrap()
}
#[test]
#[ignore = "invoked only as a subprocess fixture"]
fn tool_fixture() {
    let mode = std::env::var("DMB_TEST_MODE").unwrap();
    match mode.as_str() {
        "invalid_utf8" => {
            io::stdout().write_all(&[0xff, 0x80, b'\n']).unwrap();
        }
        "large" => {
            let mut out = io::stdout().lock();
            out.write_all(b"BEGIN\n").unwrap();
            for i in 0..100_005 {
                writeln!(out, "{{ index={i} }}  ").unwrap();
            }
            out.write_all(b"END\n").unwrap();
            io::stderr().write_all(b"diagnostic on stderr\n").unwrap();
        }
        "failure" => {
            io::stdout().write_all(b"kept output\n").unwrap();
            std::process::exit(7)
        }
        "long" => {
            println!("ready");
            io::stdout().flush().unwrap();
            thread::sleep(Duration::from_secs(30));
        }
        "late_write" => {
            thread::sleep(Duration::from_millis(180));
            fs::write(std::env::var_os("DMB_MARKER").unwrap(), b"complete").unwrap();
        }
        "spawn_exit" | "spawn_wait" => {
            let childmode = if mode == "spawn_exit" {
                "late_write"
            } else {
                "delayed_write"
            };
            let child = Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "tool_fixture", "--nocapture"])
                .env("DMB_TEST_MODE", childmode)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            fs::write(
                std::env::var_os("DMB_READY").unwrap(),
                child.id().to_string(),
            )
            .unwrap();
            if mode == "spawn_wait" {
                thread::sleep(Duration::from_secs(30));
            }
        }
        "delayed_write" => {
            thread::sleep(Duration::from_secs(1));
            fs::write(std::env::var_os("DMB_MARKER").unwrap(), b"must not exist").unwrap();
        }
        "argument" => {
            println!("{}", std::env::var("DMB_LITERAL").unwrap());
        }
        _ => panic!("unknown fixture"),
    }
}
#[test]
fn complete_large_output_keeps_both_ends_and_significant_spaces() {
    let dir = tempfile::tempdir().unwrap();
    let run = runner(
        dir.path(),
        CancellationToken::new(),
        Duration::from_secs(10),
        16 * 1024 * 1024,
    );
    let _slow_ui = run.subscribe(1);
    let result = run.run(&command("large")).unwrap();
    let text = result.read_stdout_utf8(16 * 1024 * 1024).unwrap();
    assert!(text.contains("BEGIN\n{ index=0 }  \n"));
    assert!(text.contains("{ index=100004 }  \nEND\n"));
    assert!(
        result
            .read_stderr_utf8(2048)
            .unwrap()
            .contains("diagnostic on stderr")
    );
    assert!(result.dropped_live_events > 0);
    assert!(
        fs::metadata(&result.stdout_path)
            .unwrap()
            .permissions()
            .readonly()
    );
    assert!(result.elapsed_ns > 0);
    let _snapshot = run.quiescent().unwrap();
}
#[test]
fn nonzero_status_and_output_limit_retain_raw_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let run = runner(
        dir.path(),
        CancellationToken::new(),
        Duration::from_secs(5),
        1024,
    );
    let error = run.run(&command("failure")).unwrap_err();
    assert!(matches!(error, CoreError::ToolFailed { code: Some(7), .. }));
    assert!(error.to_string().contains("kept output"));
    assert!(
        error
            .to_string()
            .contains("stderr:\n(no output on this stream)")
    );
    assert!(matches!(
        run.run(&command("large")),
        Err(CoreError::OutputLimit { limit: 1024 })
    ));
    let raw: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|p| p.path().extension().is_some_and(|x| x == "bin"))
        .collect();
    assert_eq!(raw.len(), 4);
    assert!(raw.iter().any(|p| {
        fs::read(p.path())
            .unwrap()
            .windows(11)
            .any(|w| w == b"kept output")
    }));
    let _snapshot = run.quiescent().unwrap();
}
#[test]
fn waits_for_descendant_after_parent_and_stdio_exit() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("artifact");
    let ready = dir.path().join("ready");
    let run = runner(
        &dir.path().join("logs"),
        CancellationToken::new(),
        Duration::from_secs(5),
        1024 * 1024,
    );
    run.run(
        &command("spawn_exit")
            .env("DMB_MARKER", &marker)
            .env("DMB_READY", &ready),
    )
    .unwrap();
    let _snapshot = run.quiescent().unwrap();
    assert_eq!(fs::read(&marker).unwrap(), b"complete");
}
fn wait_for(path: &PathBuf) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < deadline, "child did not start");
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn cancellation_terminates_descendants_and_keeps_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("artifact");
    let ready = dir.path().join("ready");
    let token = CancellationToken::new();
    let run = runner(
        &dir.path().join("logs"),
        token.clone(),
        Duration::from_secs(5),
        1024 * 1024,
    );
    let spec = command("spawn_wait")
        .env("DMB_MARKER", &marker)
        .env("DMB_READY", &ready);
    let worker = thread::spawn(move || run.run(&spec));
    wait_for(&ready);
    token.cancel();
    let result = worker.join().unwrap();
    assert!(matches!(result, Err(CoreError::Cancelled)), "{result:?}");
    thread::sleep(Duration::from_millis(1100));
    assert!(!marker.exists());
    assert!(
        fs::read_dir(dir.path().join("logs"))
            .unwrap()
            .flatten()
            .any(|p| p.path().extension().is_some_and(|x| x == "json"))
    );
}
#[test]
fn timeout_terminates_process_and_releases_snapshot_barrier() {
    let dir = tempfile::tempdir().unwrap();
    let run = runner(
        dir.path(),
        CancellationToken::new(),
        Duration::from_millis(100),
        1024 * 1024,
    );
    let result = run.run(&command("long"));
    assert!(
        matches!(result, Err(CoreError::Timeout { .. })),
        "{result:?}"
    );
    let _snapshot = run.quiescent().unwrap();
}
#[test]
fn command_timeout_can_only_shorten_the_runner_limit() {
    let dir = tempfile::tempdir().unwrap();
    let run = runner(
        dir.path(),
        CancellationToken::new(),
        Duration::from_secs(3),
        1024 * 1024,
    );
    let started = std::time::Instant::now();
    let result = run.run(&command("long").timeout(Duration::from_millis(100)));
    assert!(
        matches!(result, Err(CoreError::Timeout { .. })),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    let _snapshot = run.quiescent().unwrap();
}
#[test]
fn snapshot_barrier_blocks_starts_and_failed_launch_releases_gate() {
    let dir = tempfile::tempdir().unwrap();
    let run = runner(
        dir.path(),
        CancellationToken::new(),
        Duration::from_secs(5),
        1024 * 1024,
    );
    assert!(
        run.run(&CommandSpec::new("missing-dmb-fixture-program"))
            .is_err()
    );
    let freeze = run.quiescent().unwrap();
    let next = run.clone();
    let worker = thread::spawn(move || {
        next.run(&command("argument").env("DMB_LITERAL", "a; $(echo unintended) | &"))
    });
    thread::sleep(Duration::from_millis(50));
    assert!(!worker.is_finished());
    drop(freeze);
    let result = worker.join().unwrap().unwrap();
    assert!(
        result
            .read_stdout_utf8(4096)
            .unwrap()
            .contains("a; $(echo unintended) | &")
    );
}

#[test]
fn lossy_display_preserves_raw_bytes_and_strict_parsing_remains_available() {
    let dir = tempfile::tempdir().unwrap();
    let run = runner(
        dir.path(),
        CancellationToken::new(),
        Duration::from_secs(5),
        4096,
    );
    let output = run.run(&command("invalid_utf8")).unwrap();
    assert!(
        fs::read(&output.stdout_path)
            .unwrap()
            .windows(3)
            .any(|w| w == [0xff, 0x80, b'\n'])
    );
    assert!(output.read_stdout_utf8(4096).is_err());
    assert!(output.read_stdout_lossy(4096).unwrap().contains("��"));
}
