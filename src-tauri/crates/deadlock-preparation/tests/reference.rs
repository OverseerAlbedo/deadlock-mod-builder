use deadlock_preparation::{algorithms, blender, source};
use serde_json::{Value, json};
use std::{fs, path::Path};
#[test]
fn python_recognition_and_review_differential() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/reference.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let a = &case["args"];
        let result = (|| -> Result<Value, String> {
            Ok(match case["operation"].as_str().unwrap() {
                "identify" => serde_json::to_value(
                    algorithms::identify(
                        &serde_json::from_value::<Vec<algorithms::Bone>>(a["bones"].clone())
                            .unwrap(),
                        &serde_json::from_value(a["overrides"].clone()).unwrap(),
                    )
                    .map_err(|e| e.to_string())?,
                )
                .unwrap(),
                "suggest" => serde_json::to_value(
                    algorithms::suggest_mapping(
                        &serde_json::from_value::<Vec<algorithms::Bone>>(a["source"].clone())
                            .unwrap(),
                        &serde_json::from_value::<Vec<algorithms::Bone>>(a["target"].clone())
                            .unwrap(),
                        &Default::default(),
                        &Default::default(),
                    )
                    .map_err(|e| e.to_string())?,
                )
                .unwrap(),
                "name" => serde_json::to_value(algorithms::name_role(a["name"].as_str().unwrap()))
                    .unwrap(),
                "review" => {
                    let analysis = &a["analysis"];
                    let info = &analysis["analysis"];
                    serde_json::to_value(algorithms::review_issues(
                        &serde_json::from_value::<Vec<algorithms::Bone>>(
                            analysis["target"].clone(),
                        )
                        .unwrap(),
                        &serde_json::from_value::<Vec<algorithms::Bone>>(info["bones"].clone())
                            .unwrap(),
                        &serde_json::from_value::<Vec<String>>(info["weighted_bones"].clone())
                            .unwrap(),
                        info["suggested_mode"].as_str().unwrap(),
                        a["mode"].as_str().unwrap(),
                        &serde_json::from_value(a["options"].clone()).unwrap(),
                    ))
                    .unwrap()
                }
                _ => panic!(),
            })
        })();
        if case.get("error").is_some() {
            assert!(result.is_err(), "{}", case["name"]);
        } else {
            assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]);
        }
    }
}
#[test]
fn source_staging_proves_dependency_bytes_and_rejects_escape() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("input 雪");
    fs::create_dir_all(source.join("textures")).unwrap();
    fs::write(source.join("mesh.bin"), b"mesh").unwrap();
    fs::write(source.join("textures/snow 雪.png"), b"pixels").unwrap();
    let file = source.join("hero.gltf");
    fs::write(&file,serde_json::to_vec(&json!({"buffers":[{"uri":"mesh.bin"}],"images":[{"uri":"textures/snow%20%E9%9B%AA.png"},{"uri":"data:image/png;base64,AAAA"}]})).unwrap()).unwrap();
    let staged =
        source::stage_source(file.to_str().unwrap(), &temp.path().join("work"), None).unwrap();
    assert_eq!(
        fs::read(staged.parent().unwrap().join("textures/snow 雪.png")).unwrap(),
        b"pixels"
    );
    let manifest: Vec<source::ManifestEntry> = serde_json::from_slice(
        &fs::read(staged.parent().unwrap().join("source-manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest.len(), 3);
    for entry in manifest {
        assert_eq!(
            deadlock_preparation::digest(&staged.parent().unwrap().join(entry.relative)).unwrap(),
            entry.sha256
        );
    }
    for uri in [
        "../outside.bin",
        "%2e%2e/outside.bin",
        "https://example.com/x",
        "//server/x",
        "bad\\x",
        "%5cserver/x",
        "%a雪",
    ] {
        fs::write(
            &file,
            serde_json::to_vec(&json!({"buffers":[{"uri":uri}]})).unwrap(),
        )
        .unwrap();
        assert!(
            source::stage_source(file.to_str().unwrap(), &temp.path().join("bad"), None).is_err(),
            "{uri}"
        );
    }
    assert!(source::stage_source(file.to_str().unwrap(), &source, None).is_err());
}
#[cfg(unix)]
#[test]
fn staging_rejects_symlink_dependency_escape() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let file = source.join("hero.gltf");
    fs::write(&file, b"{\"images\":[{\"uri\":\"image.png\"}]}").unwrap();
    let outside = temp.path().join("outside.png");
    fs::write(&outside, b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, source.join("image.png")).unwrap();
    assert!(source::stage_source(file.to_str().unwrap(), &temp.path().join("work"), None).is_err());
}
#[test]
fn supported_blender_minor_is_launch_only() {
    for version in ["4.2.0", "4.5.3", "5.0.1", "5.2.2"] {
        let r = blender::version_report(
            Path::new("blender.exe"),
            &format!("Blender {version}\nbuild fixture"),
        )
        .unwrap();
        assert!(r.launch_verified);
        assert!(!r.conversion_certified);
    }
    for version in ["4.1.9", "5.1.0", "5.3.0", "6.0.0"] {
        assert!(
            blender::version_report(Path::new("blender.exe"), &format!("Blender {version}"))
                .is_err()
        );
    }
}
#[test]
fn embedded_blender_scripts_match_current_sources() {
    let temp = tempfile::tempdir().unwrap();
    blender::install_scripts(temp.path()).unwrap();
    let reference = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/blender");
    // Check every source script is packaged, including newly added imports.
    // Iterating only installed files would miss an omitted bridge dependency.
    for entry in fs::read_dir(&reference).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_some_and(|s| s == "py") {
            assert_eq!(
                fs::read(&p).unwrap(),
                fs::read(temp.path().join(p.file_name().unwrap())).unwrap()
            );
        }
    }
}
#[test]
fn cancelled_staging_never_publishes_source() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("source.gltf");
    fs::write(&input, b"{}").unwrap();
    let cancel = deadlock_core::CancellationToken::new();
    cancel.cancel();
    let destination = temp.path().join("work");
    let error =
        source::stage_source_with_cancel(input.to_str().unwrap(), &destination, None, &cancel)
            .unwrap_err();
    assert!(matches!(
        error,
        deadlock_preparation::Error::Core(deadlock_core::CoreError::Cancelled)
    ));
    assert!(!destination.exists());
}
