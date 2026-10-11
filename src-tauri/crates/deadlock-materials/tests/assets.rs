use deadlock_core::{safety, CancellationToken};
use deadlock_dmx::{DmxSummary, MaterialIdentity};
use deadlock_materials::*;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn summary() -> DmxSummary {
    DmxSummary {
        encoding: "binary 9".into(),
        format: "model".into(),
        format_version: 22,
        meshes: vec![],
        materials: vec!["a.vmat".into()],
        material_identities: vec![MaterialIdentity {
            name: "A".into(),
            uuid: "id".into(),
            path: "a.vmat".into(),
        }],
        bones: vec![],
        elements: 0,
    }
}
fn create_zip(path: &Path, entries: &[(&str, &[u8])], link: bool) {
    let mut out = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, data) in entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        if link {
            out.add_symlink(*name, "../../escape", options).unwrap();
        } else {
            out.start_file(*name, options).unwrap();
            out.write_all(data).unwrap();
        }
    }
    out.finish().unwrap();
}
#[test]
fn stored_deflate_bzip2_lzma_import_and_reimport_verify_manifest() {
    for kind in ["stored", "deflate", "bzip2", "lzma"] {
        let temp = tempfile::tempdir().unwrap();
        let zip = fixtures().join(format!("source-{kind}.zip"));
        let selection = import_zip(&zip, temp.path(), &CancellationToken::new()).unwrap();
        assert_eq!(fs::read(&selection.dmx).unwrap(), b"fixture opaque DMX");
        assert_eq!(
            selection.expected_dmx_sha256,
            safety::digest(Path::new(&selection.dmx)).unwrap()
        );
        assert_eq!(fs::read(&selection.material_mapping).unwrap(), b"{}");
        assert_eq!(
            selection,
            import_zip(&zip, temp.path(), &CancellationToken::new()).unwrap()
        );
        assert!(selection.skeleton_reference.is_empty());
    }
}
#[test]
fn tampered_cached_source_package_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let zip = fixtures().join("source-deflate.zip");
    let selected = import_zip(&zip, temp.path(), &CancellationToken::new()).unwrap();
    fs::write(selected.dmx, "changed").unwrap();
    assert!(import_zip(&zip, temp.path(), &CancellationToken::new())
        .unwrap_err()
        .to_string()
        .contains("manifest mismatch"));
}
#[test]
fn zip_traversal_case_collision_links_and_missing_manifest_leave_no_cache() {
    for (entries, link) in [
        (vec![("../escape", b"bad".as_slice())], false),
        (
            vec![("a.dmx", b"one".as_slice()), ("A.dmx", b"two".as_slice())],
            false,
        ),
        (vec![("link", b"x".as_slice())], true),
        (vec![("a.dmx", b"only".as_slice())], false),
        (vec![("CON.txt", b"device".as_slice())], false),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("source.zip");
        create_zip(&zip, &entries, link);
        let work = temp.path().join("work");
        assert!(import_zip(&zip, &work, &CancellationToken::new()).is_err());
        assert!(!temp.path().join("escape").exists());
        if work.exists() {
            assert_eq!(fs::read_dir(work).unwrap().count(), 0);
        }
    }
}
#[test]
fn cancelled_source_import_publishes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(import_zip(&fixtures().join("source-deflate.zip"), temp.path(), &cancel).is_err());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}
#[test]
fn overlay_refreshes_changed_sources_revalidates_cache_and_preserves_base() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("base");
    let overlay = temp.path().join("overlay");
    let destination = temp.path().join("work/effective");
    fs::create_dir(&base).unwrap();
    fs::create_dir(&overlay).unwrap();
    fs::write(base.join("a.vmat"), "original").unwrap();
    fs::write(overlay.join("a.vmat"), "v1").unwrap();
    effective_sources(
        &base,
        Some(&overlay),
        &destination,
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(fs::read(destination.join("a.vmat")).unwrap(), b"v1");
    fs::write(overlay.join("a.vmat"), "v2").unwrap();
    effective_sources(
        &base,
        Some(&overlay),
        &destination,
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(fs::read(destination.join("a.vmat")).unwrap(), b"v2");
    assert_eq!(fs::read(base.join("a.vmat")).unwrap(), b"original");
    fs::write(destination.join("a.vmat"), "tampered effective tree").unwrap();
    effective_sources(
        &base,
        Some(&overlay),
        &destination,
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(fs::read(destination.join("a.vmat")).unwrap(), b"v2");
    assert_eq!(
        effective_sources(&base, None, &destination, &CancellationToken::new()).unwrap(),
        base
    );
}
#[test]
fn invalid_overlay_leaves_prior_effective_tree_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("base");
    let overlay = temp.path().join("overlay");
    let destination = temp.path().join("effective");
    fs::create_dir(&base).unwrap();
    fs::create_dir(&overlay).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), "prior").unwrap();
    fs::write(overlay.join("script.exe"), "not a material").unwrap();
    assert!(effective_sources(
        &base,
        Some(&overlay),
        &destination,
        &CancellationToken::new()
    )
    .is_err());
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"prior");
}
#[test]
fn identity_mapping_remap_and_direct_dependencies_checked() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(
        root.join("a.vmat"),
        "Layer0 { TextureColor1 \"missing.png\" }",
    )
    .unwrap();
    let summary = summary();
    assert!(validate_materials(&summary, root, None, None)
        .unwrap_err()
        .to_string()
        .contains("missing.png referenced by a.vmat"));
    fs::write(root.join("missing.png"), "texture").unwrap();
    let mapping = root.join("mapping.json");
    let remap = root.join("remap.json");
    fs::write(&mapping, r#"{"A":{"source2_material_path":"old.vmat"}}"#).unwrap();
    assert!(validate_materials(&summary, root, Some(&mapping), None).is_err());
    fs::write(&remap, r#"{"A":{"vmat":"a.vmat"}}"#).unwrap();
    let result = validate_materials(&summary, root, Some(&mapping), Some(&remap)).unwrap();
    assert_eq!(result.direct_source_dependencies, ["missing.png"]);
    assert!(result.mapping_preserved && result.remap_preserved);
    fs::write(&remap, r#"{"A":{"vmat":"other.vmat"}}"#).unwrap();
    assert!(validate_materials(&summary, root, Some(&mapping), Some(&remap)).is_err());
}
#[cfg(unix)]
#[test]
fn texture_overlay_and_cached_package_symlinks_rejected() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let textures = root.join("textures");
    fs::create_dir(&textures).unwrap();
    fs::write(root.join("outside.png"), "outside").unwrap();
    symlink(root.join("outside.png"), textures.join("body_color.png")).unwrap();
    assert!(texture_candidates(
        "models/test/body.vmat",
        &textures,
        &CancellationToken::new()
    )
    .is_err());
    let work = root.join("work");
    let zip = fixtures().join("source-deflate.zip");
    let imported = import_zip(&zip, &work, &CancellationToken::new()).unwrap();
    fs::remove_file(&imported.dmx).unwrap();
    symlink(root.join("outside.png"), &imported.dmx).unwrap();
    assert!(import_zip(&zip, &work, &CancellationToken::new()).is_err());
}

#[test]
fn duplicate_exact_central_entries_cannot_be_hidden_by_zip_index() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("duplicate.zip");
    // Build two distinct names, then change both local/central names of b.txt to
    // a.txt without changing their lengths. A name-keyed index would hide one.
    create_zip(
        &archive,
        &[("a.txt", b"first"), ("b.txt", b"second")],
        false,
    );
    let bytes = fs::read(&archive).unwrap();
    let mut changed = bytes.clone();
    for at in bytes
        .windows(5)
        .enumerate()
        .filter_map(|(i, s)| (s == b"b.txt").then_some(i))
    {
        changed[at..at + 5].copy_from_slice(b"a.txt");
    }
    fs::write(&archive, changed).unwrap();
    let work = temp.path().join("work");
    assert!(import_zip(&archive, &work, &CancellationToken::new())
        .unwrap_err()
        .to_string()
        .contains("duplicate"));
    assert_eq!(fs::read_dir(work).unwrap().count(), 0);
}
#[test]
fn declared_zip_expansion_over_four_gib_rejected_before_extracting() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("oversize.zip");
    create_zip(
        &archive,
        &[("a.txt", b"first"), ("b.txt", b"second")],
        false,
    );
    let mut bytes = fs::read(&archive).unwrap();
    let positions: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(i, s)| (s == [0x50, 0x4b, 0x01, 0x02]).then_some(i))
        .collect();
    for at in positions {
        bytes[at + 24..at + 28].copy_from_slice(&(3u32 * 1024 * 1024 * 1024).to_le_bytes());
    }
    fs::write(&archive, bytes).unwrap();
    let work = temp.path().join("work");
    assert!(import_zip(&archive, &work, &CancellationToken::new())
        .unwrap_err()
        .to_string()
        .contains("4 GiB"));
    assert_eq!(fs::read_dir(work).unwrap().count(), 0);
}
#[test]
fn corrupt_zip_payload_crc_and_lzma_wrapper_fail_without_cache() {
    for kind in ["stored", "lzma"] {
        let temp = tempfile::tempdir().unwrap();
        let source = fixtures().join(format!("source-{kind}.zip"));
        let mut archive = zip::ZipArchive::new(fs::File::open(&source).unwrap()).unwrap();
        let offset = archive.by_index_raw(0).unwrap().data_start() as usize;
        let mut bytes = fs::read(source).unwrap();
        if kind == "lzma" {
            bytes[offset + 2] = 4;
        } else {
            bytes[offset] ^= 1;
        }
        let changed = temp.path().join("corrupt.zip");
        fs::write(&changed, bytes).unwrap();
        let work = temp.path().join("work");
        assert!(import_zip(&changed, &work, &CancellationToken::new()).is_err());
        assert_eq!(fs::read_dir(work).unwrap().count(), 0);
    }
}
