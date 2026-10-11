use deadlock_core::process::{CommandSpec, ProcessResult, ToolExecutor};
use deadlock_core::{CancellationToken, CoreError, TimingRecorder};
use deadlock_preview::gltf::{load_packet, load_source_packet};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gltf")
}
fn packet(path: &Path) -> Vec<u8> {
    load_packet(
        path,
        &CancellationToken::default(),
        &TimingRecorder::new("preview"),
    )
    .unwrap()
}
fn parts(bytes: &[u8]) -> (Value, &[u8]) {
    assert_eq!(&bytes[..4], b"DMBG");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 1);
    let n = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let meta = serde_json::from_slice(&bytes[16..16 + n]).unwrap();
    (meta, &bytes[16 + ((n + 3) & !3)..])
}
fn glb_json(raw: &[u8]) -> Value {
    assert_eq!(&raw[..4], b"glTF");
    assert_eq!(
        raw.len(),
        u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize
    );
    let len = u32::from_le_bytes(raw[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&raw[20..20 + len]).unwrap()
}
fn original() -> Value {
    serde_json::from_slice(&fs::read(root().join("basic.gltf")).unwrap()).unwrap()
}
fn rejects(value: &Value) -> String {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bad.gltf");
    fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    load_packet(
        &path,
        &CancellationToken::default(),
        &TimingRecorder::new("invalid"),
    )
    .unwrap_err()
    .to_string()
}
#[test]
fn python_oracle_catalogs_and_embedded_reparse() {
    let expected: Value =
        serde_json::from_slice(&fs::read(root().join("oracle.json")).unwrap()).unwrap();
    for (name, want) in expected.as_object().unwrap() {
        let source = root().join(name);
        let before = fs::read(&source).unwrap();
        let bytes = packet(&source);
        let (meta, glb) = parts(&bytes);
        assert_eq!(meta["source_sha256"], want["sha256"], "{name}");
        for key in [
            "mesh_count",
            "vertex_count",
            "triangle_count",
            "bone_count",
            "animations",
        ] {
            assert_eq!(meta[key], want[key], "{name}:{key}");
        }
        assert_eq!(before, fs::read(&source).unwrap());
        let temp = tempfile::tempdir().unwrap();
        let copy = temp.path().join("embedded.glb");
        fs::write(&copy, glb).unwrap();
        let reparsed = packet(&copy);
        let (meta2, _) = parts(&reparsed);
        assert_eq!(meta2["animations"], meta["animations"]);
        assert_eq!(meta2["vertex_count"], meta["vertex_count"]);
        let doc = glb_json(glb);
        assert_eq!(doc["buffers"].as_array().unwrap().len(), 1);
        assert!(doc["buffers"][0].get("uri").is_none());
    }
}
#[test]
fn external_images_buffers_are_hashed_embedded_and_change_identity() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["external.gltf", "mesh.bin", "cloth.png"] {
        fs::copy(root().join(name), temp.path().join(name)).unwrap();
    }
    let a = packet(&temp.path().join("external.gltf"));
    let (meta, glb) = parts(&a);
    assert_eq!(meta["dependencies"].as_object().unwrap().len(), 3);
    let doc = glb_json(glb);
    assert!(doc["images"][0].get("uri").is_none());
    assert_eq!(doc["images"][0]["mimeType"], "image/png");
    // Dependencies are keyed by the canonical path (\\?\C:\... on Windows).
    let key = temp
        .path()
        .join("cloth.png")
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let before = meta["dependencies"][&key].clone();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]))
        .save(temp.path().join("cloth.png"))
        .unwrap();
    let b = packet(&temp.path().join("external.gltf"));
    assert_ne!(parts(&b).0["dependencies"][key], before);
}
#[test]
fn path_uri_and_reference_rejections() {
    for uri in [
        "https://example.org/x",
        "../x",
        "%2e%2e/x",
        "C:/x",
        "\\server\\x",
        "//server/x",
        "x?foo",
        "x#frag",
        "%00.bin",
        "%3A.bin",
        "data:;base64,!!!!",
    ] {
        let mut d = original();
        d["buffers"][0]["uri"] = json!(uri);
        assert!(!rejects(&d).is_empty(), "{uri}");
    }
    for (pointer, value) in [
        ("/nodes/0/mesh", json!(-1)),
        ("/meshes/0/primitives/0/material", json!(-1)),
        ("/animations/0/channels/0/sampler", json!(false)),
        ("/accessors/0/count", json!(99999999)),
        ("/accessors/0/byteOffset", json!(u64::MAX)),
        ("/bufferViews/0/byteLength", json!(u64::MAX)),
        ("/accessors/0/componentType", json!(5124)),
        ("/accessors/0/normalized", json!(true)),
    ] {
        let mut d = original();
        if let Some(v) = d.pointer_mut(pointer) {
            *v = value;
        } else {
            let (path, key) = pointer.rsplit_once('/').unwrap();
            d.pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(key.into(), value);
        }
        assert!(!rejects(&d).is_empty(), "{pointer}");
    }
}
#[test]
fn hierarchy_skin_animation_and_extension_gates() {
    let edits: Vec<Box<dyn Fn(&mut Value)>> = vec![
        Box::new(|d| d["nodes"][2]["children"] = json!([1])),
        Box::new(|d| d["nodes"][0]["children"] = json!([2])),
        Box::new(|d| d["nodes"][0]["rotation"] = json!([0, 0, 0, 0])),
        Box::new(|d| d["nodes"][0]["matrix"] = json!(vec![0; 16])),
        Box::new(|d| d["skins"][0]["joints"] = json!([1, 1])),
        Box::new(|d| d["meshes"][0]["primitives"][0]["mode"] = json!(1)),
        Box::new(|d| {
            d["meshes"][0]["primitives"][0]["attributes"]
                .as_object_mut()
                .unwrap()
                .remove("POSITION")
                .map(|_| ())
                .unwrap()
        }),
        Box::new(|d| d["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_2"] = json!(4)),
        Box::new(|d| d["meshes"][0]["weights"] = json!([0, 1])),
        Box::new(|d| d["animations"][0]["samplers"][0]["interpolation"] = json!("CATMULLROM")),
        Box::new(|d| {
            let c = d["animations"][0]["channels"][0].clone();
            d["animations"][0]["channels"]
                .as_array_mut()
                .unwrap()
                .push(c);
        }),
        Box::new(|d| d["extensionsRequired"] = json!(["KHR_draco_mesh_compression"])),
    ];
    for (edit_index, edit) in edits.iter().enumerate() {
        let mut d = original();
        edit(&mut d);
        assert!(!rejects(&d).is_empty(), "{edit_index}");
    }
}
#[test]
fn optional_extensions_stripped_and_inactive_ancestors_preserved() {
    let mut d = original();
    d["extensionsUsed"] = json!(["KHR_materials_unlit", "KHR_mesh_quantization"]);
    d["meshes"][0]["primitives"][0]["extensions"] =
        json!({"KHR_draco_mesh_compression":{"bufferView":999}});
    d["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"Ancestor","translation":[5,0,0],"children":[0]}));
    d["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"Invisible","mesh":0}));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("optional.gltf");
    fs::write(&path, serde_json::to_vec(&d).unwrap()).unwrap();
    let p = packet(&path);
    let (meta, glb) = parts(&p);
    assert_eq!(meta["mesh_count"], 1);
    let bundled = glb_json(glb);
    assert!(bundled["meshes"][0]["primitives"][0]
        .get("extensions")
        .is_none());
    assert!(bundled["nodes"][4].get("mesh").is_none());
    assert!(bundled["scenes"][0]["nodes"]
        .as_array()
        .unwrap()
        .contains(&json!(3)));
}
#[test]
fn malformed_glb_chunks_are_rejected() {
    let source = fs::read(root().join("basic.glb")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bad.glb");
    for mode in 0..6 {
        let mut raw = source.clone();
        match mode {
            0 => raw.truncate(15),
            1 => raw[4..8].copy_from_slice(&1u32.to_le_bytes()),
            2 => raw[8..12].copy_from_slice(&4u32.to_le_bytes()),
            3 => raw[12..16].copy_from_slice(&u32::MAX.to_le_bytes()),
            4 => raw[16..20].copy_from_slice(&0x004e4942u32.to_le_bytes()),
            _ => {
                raw.extend_from_slice(&[0; 4]);
                let len = raw.len() as u32;
                raw[8..12].copy_from_slice(&len.to_le_bytes());
            }
        }
        fs::write(&path, raw).unwrap();
        assert!(
            load_packet(
                &path,
                &CancellationToken::default(),
                &TimingRecorder::new("invalid")
            )
            .is_err(),
            "{mode}"
        );
    }
}
#[cfg(unix)]
#[test]
fn source_and_dependency_symlinks_are_rejected_before_resolution() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    symlink(root().join("basic.gltf"), temp.path().join("linked.gltf")).unwrap();
    assert!(load_packet(
        &temp.path().join("linked.gltf"),
        &CancellationToken::default(),
        &TimingRecorder::new("link")
    )
    .is_err());
    let mut d = original();
    d["buffers"][0]["uri"] = json!("linked.bin");
    symlink(root().join("mesh.bin"), temp.path().join("linked.bin")).unwrap();
    fs::write(
        temp.path().join("model.gltf"),
        serde_json::to_vec(&d).unwrap(),
    )
    .unwrap();
    assert!(load_packet(
        &temp.path().join("model.gltf"),
        &CancellationToken::default(),
        &TimingRecorder::new("link")
    )
    .is_err());
}
#[test]
fn cancellation_is_typed() {
    let token = CancellationToken::default();
    token.cancel();
    assert!(matches!(
        load_packet(
            Path::new("missing.gltf"),
            &token,
            &TimingRecorder::new("cancel")
        ),
        Err(CoreError::Cancelled)
    ));
}
struct FixtureBlender {
    calls: AtomicUsize,
    tamper: &'static str,
}
impl ToolExecutor for FixtureBlender {
    fn run(&self, spec: &CommandSpec) -> deadlock_core::Result<ProcessResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        for flag in [
            "--factory-startup",
            "--disable-autoexec",
            "--background",
            "--python-exit-code",
        ] {
            assert!(spec.args.iter().any(|a| a == flag));
        }
        let job: Value =
            serde_json::from_slice(&fs::read(Path::new(spec.args.last().unwrap())).unwrap())
                .unwrap();
        let source = Path::new(job["source"].as_str().unwrap());
        let output = Path::new(job["output"].as_str().unwrap());
        let hash = |p: &Path| format!("{:x}", Sha256::digest(fs::read(p).unwrap()));
        let before = hash(source);
        fs::copy(root().join("eight.glb"), output).unwrap();
        let mut receipt = json!({"status":"ok","source_sha256":before,"output_sha256":hash(output),"dependencies":{source.to_string_lossy().to_string():before},"warnings":["Fixture only"]});
        match self.tamper {
            "source" => {
                fs::write(source, b"changed").unwrap();
            }
            "hash" => receipt["output_sha256"] = json!("bad"),
            "dependency" => {
                receipt["dependencies"][source.to_string_lossy().to_string()] = json!("bad")
            }
            _ => {}
        }
        fs::write(
            job["receipt"].as_str().unwrap(),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        Ok(ProcessResult {
            command_id: "fixture".into(),
            code: Some(0),
            success: true,
            stdout_path: PathBuf::new(),
            stderr_path: PathBuf::new(),
            receipt_path: PathBuf::new(),
            elapsed_ns: 1,
            stdout_bytes: 0,
            stderr_bytes: 0,
            dropped_live_events: 0,
        })
    }
}
#[test]
fn blender_snapshot_receipts_hashes_flags_and_cleanup() {
    for tamper in ["", "source", "hash", "dependency"] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("model.blend");
        let blender = temp.path().join("blender");
        fs::write(&source, b"BLENDER-fixture").unwrap();
        fs::write(&blender, b"fixture").unwrap();
        let runner = FixtureBlender {
            calls: AtomicUsize::new(0),
            tamper,
        };
        let clock = TimingRecorder::new("blender");
        let result = load_source_packet(
            &source,
            &blender,
            temp.path(),
            &runner,
            &CancellationToken::default(),
            &clock,
        );
        assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
        if tamper.is_empty() {
            let bytes = result.unwrap();
            let (meta, _) = parts(&bytes);
            // The snapshot reports the canonical source path (\\?\C:\... on Windows).
            assert_eq!(
                meta["source"],
                source.canonicalize().unwrap().to_string_lossy().as_ref()
            );
            assert!(meta["warnings"]
                .as_array()
                .unwrap()
                .contains(&json!("Fixture only")));
        } else {
            assert!(result.is_err(), "{tamper}");
        }
        assert_eq!(
            fs::read_dir(temp.path().join("preview")).unwrap().count(),
            0
        );
    }
}

fn change_f32(d: &mut Value, accessor: usize, component: usize, value: f32) {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let source = d["buffers"][0]["uri"].as_str().unwrap();
    let mut raw = STANDARD.decode(source.split_once(',').unwrap().1).unwrap();
    let view = d["accessors"][accessor]["bufferView"].as_u64().unwrap() as usize;
    let offset = d["bufferViews"][view]["byteOffset"].as_u64().unwrap() as usize + component * 4;
    raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    d["buffers"][0]["uri"] = json!(format!(
        "data:application/octet-stream;base64,{}",
        STANDARD.encode(raw)
    ));
}
#[test]
fn numeric_data_and_rest_pose_overflow_are_rejected() {
    for (accessor, component, value) in [
        (0, 0, f32::NAN),
        (0, 0, f32::INFINITY),
        (4, 0, 0.),
        (4, 0, -1.),
        (5, 3, 1.),
        (7, 1, 0.),
    ] {
        let mut d = original();
        change_f32(&mut d, accessor, component, value);
        assert!(!rejects(&d).is_empty(), "{accessor}:{component}");
    }
    let mut d = original();
    d["nodes"][1]["scale"] = json!([1e300, 1e300, 1e300]);
    assert!(rejects(&d).contains("float32"));
    let mut d = original();
    let attrs = d["meshes"][0]["primitives"][0]["attributes"]
        .as_object_mut()
        .unwrap();
    let j = attrs.remove("JOINTS_0").unwrap();
    attrs.insert("JOINTS_00".into(), j);
    assert!(rejects(&d).contains("consecutive"));
}
#[test]
fn malformed_sparse_stream_and_texture_fail_closed() {
    let sparse: Value =
        serde_json::from_slice(&fs::read(root().join("sparse.gltf")).unwrap()).unwrap();
    for (key, value) in [("count", json!(4)), ("count", json!(-1))] {
        let mut d = sparse.clone();
        d["accessors"][0]["sparse"][key] = value;
        assert!(!rejects(&d).is_empty());
    }
    let temp = tempfile::tempdir().unwrap();
    for name in ["external.gltf", "mesh.bin"] {
        fs::copy(root().join(name), temp.path().join(name)).unwrap();
    }
    fs::write(temp.path().join("cloth.png"), b"not a PNG").unwrap();
    assert!(load_packet(
        &temp.path().join("external.gltf"),
        &CancellationToken::default(),
        &TimingRecorder::new("bad image")
    )
    .is_err());
}
