//! Process-double checks of native cache/orchestration, complemented by real bpy tests.
use deadlock_core::{
    process::{CommandSpec, ProcessResult, ToolExecutor},
    CancellationToken,
};
use deadlock_preparation::{blender::BlenderBridge, material_recovery::recover};
use serde_json::{json, Value};
use std::{
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct Tool {
    calls: AtomicUsize,
}
impl ToolExecutor for Tool {
    fn run(&self, spec: &CommandSpec) -> deadlock_core::Result<ProcessResult> {
        let version = spec.args.iter().any(|v| v == "--version");
        let dir = spec
            .cwd
            .clone()
            .unwrap_or_else(|| spec.program.parent().unwrap().into());
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("stdout.txt");
        let err = dir.join("stderr.txt");
        fs::write(&out, "Blender 5.2.2 LTS\n").unwrap();
        fs::write(&err, "").unwrap();
        if !version {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(spec.args.iter().any(|v| v == "--disable-autoexec"));
            let job: Value =
                serde_json::from_slice(&fs::read(spec.args.last().unwrap()).unwrap()).unwrap();
            assert_eq!(job["operation"], "recover_materials");
            let image = dir.join("textures/body.png");
            fs::create_dir_all(image.parent().unwrap()).unwrap();
            fs::write(&image, b"image").unwrap();
            let report = json!({"status":"ok","materials":{"models/test/materials/prepared/body_1234abcd.vmat":{"textures":{"color":image},"confirmed":false}},"warnings":[]});
            fs::write(
                job["result"].as_str().unwrap(),
                serde_json::to_vec(&report).unwrap(),
            )
            .unwrap();
        }
        Ok(ProcessResult {
            command_id: "fixture".into(),
            code: Some(0),
            success: true,
            stdout_path: out,
            stderr_path: err,
            receipt_path: dir.join("receipt.json"),
            elapsed_ns: 1,
            stdout_bytes: 18,
            stderr_bytes: 0,
            dropped_live_events: 0,
        })
    }
}
#[test]
fn existing_preparation_is_recovered_without_reconversion_and_cache_needs_no_blender() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("prepared.blend");
    fs::write(&source, b"saved geometry and materials").unwrap();
    let exe = temp.path().join("blender.exe");
    fs::write(&exe, b"fixture").unwrap();
    let tool = Arc::new(Tool {
        calls: AtomicUsize::new(0),
    });
    let bridge = BlenderBridge {
        executable: exe.clone(),
        executor: tool.clone(),
    };
    let cache = temp.path().join("cache");
    let token = CancellationToken::new();
    let first = recover(&bridge, &source, "models/test", &cache, &token).unwrap();
    assert_eq!(tool.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fs::read(&source).unwrap(), b"saved geometry and materials");
    fs::remove_file(&exe).unwrap();
    let cached = recover(&bridge, &source, "models/test", &cache, &token).unwrap();
    assert_eq!(first.materials, cached.materials);
    assert_eq!(tool.calls.load(Ordering::SeqCst), 1);
    fs::write(&exe, b"fixture").unwrap();
    let image = &first
        .materials
        .values()
        .next()
        .unwrap()
        .textures
        .as_ref()
        .unwrap()["color"];
    fs::remove_file(image).unwrap();
    recover(&bridge, &source, "models/test", &cache, &token).unwrap();
    assert_eq!(tool.calls.load(Ordering::SeqCst), 2);
    fs::write(&source, b"changed material connections").unwrap();
    recover(&bridge, &source, "models/test", &cache, &token).unwrap();
    assert_eq!(tool.calls.load(Ordering::SeqCst), 3);
    assert_eq!(fs::read(&source).unwrap(), b"changed material connections");
    token.cancel();
    assert!(recover(&bridge, &source, "models/test", &cache, &token).is_err());
    assert_eq!(tool.calls.load(Ordering::SeqCst), 3);
}
