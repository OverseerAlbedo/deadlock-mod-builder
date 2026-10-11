use deadlock_vpk::{
    relative_asset, resource_blocks, resource_references, write_vpk, Error, Hashes, Vpk,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn expected() -> Value {
    serde_json::from_slice(&fs::read(fixtures().join("expected.json")).unwrap()).unwrap()
}
fn hashes() -> Hashes {
    serde_json::from_value(expected()["hashes"].clone()).unwrap()
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn compiled(blocks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let table_end = 16 + blocks.len() * 12;
    let size = table_end + blocks.iter().map(|b| b.1.len()).sum::<usize>();
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&12u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
    let mut offset = table_end;
    for (i, (name, bytes)) in blocks.iter().enumerate() {
        out.extend_from_slice(*name);
        out.extend_from_slice(&((offset - (16 + i * 12 + 4)) as u32).to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        offset += bytes.len();
    }
    for (_, bytes) in blocks {
        out.extend_from_slice(bytes);
    }
    out
}
fn set32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn python_writer_byte_parity_v1_v2_and_input_order() {
    let temp = tempfile::tempdir().unwrap();
    let hashes = hashes();
    for version in [1, 2] {
        let mut files: Vec<_> = hashes.keys().cloned().collect();
        files.reverse();
        let output = temp.path().join(format!("pak{version:02}_test.vpk"));
        let receipt = write_vpk(
            &fixtures().join("source"),
            &files,
            &output,
            Some(&hashes),
            version,
            &mut || Ok(()),
        )
        .unwrap();
        let python = fs::read(fixtures().join(format!("python-v{version}_dir.vpk"))).unwrap();
        assert_eq!(fs::read(&output).unwrap(), python);
        let mut value = serde_json::to_value(receipt).unwrap();
        // Backend is intentionally identified as Rust; bytes/receipt otherwise match.
        value["backend"] = "builtin".into();
        assert_eq!(value, expected()["receipts"][format!("v{version}")]);
        let reader = Vpk::open(&output).unwrap();
        assert_eq!(
            reader.verify_files(Some(&hashes), &mut || Ok(())).unwrap(),
            hashes
        );
        if version == 2 {
            reader.verify_v2_hashes(&mut || Ok(())).unwrap();
        } else {
            assert!(reader.verify_v2_hashes(&mut || Ok(())).is_err());
        }
    }
}

#[test]
fn python_preload_split_embedded_reader_parity() {
    for version in [1, 2] {
        for split in [0, 1] {
            let archive =
                Vpk::open(fixtures().join(format!("preload-v{version}-{split}_dir.vpk"))).unwrap();
            assert_eq!(
                archive
                    .verify_files(Some(&hashes()), &mut || Ok(()))
                    .unwrap(),
                hashes()
            );
            for (name, hash) in hashes() {
                assert_eq!(sha(&archive.read(&name).unwrap()), hash);
                assert_eq!(
                    archive.read(&name).unwrap(),
                    fs::read(fixtures().join("source").join(name)).unwrap()
                );
            }
        }
    }
}
#[test]
fn python_envelope_640_blocks_and_late_references() {
    let data = fs::read(fixtures().join("resource.bin")).unwrap();
    assert_eq!(
        serde_json::to_value(resource_blocks(&data).unwrap()).unwrap(),
        expected()["resource"]
    );
    assert_eq!(
        serde_json::to_value(resource_references(&data).unwrap()).unwrap(),
        expected()["references"]
    );
    assert_eq!(resource_blocks(&data).unwrap().blocks.len(), 640);
    assert!(resource_references(b"not a resource").unwrap().is_empty());
    assert!(resource_references(b"not a resource not a resource")
        .unwrap()
        .is_empty());
}
#[test]
fn python_model_listing_parity() {
    let archive = Vpk::open(fixtures().join("python-v2_dir.vpk")).unwrap();
    assert_eq!(
        serde_json::to_value(archive.models(false)).unwrap(),
        expected()["models"]
    );
}
#[test]
fn shared_casefold_matches_all_python_unicode_15_mappings() {
    let data: Value =
        serde_json::from_slice(&fs::read(fixtures().join("casefold.json")).unwrap()).unwrap();
    assert_eq!(data["unicode_version"], "15.0.0");
    for pair in data["mapping"].as_array().unwrap() {
        assert_eq!(
            deadlock_core::text::casefold(pair[0].as_str().unwrap()),
            pair[1].as_str().unwrap()
        );
    }
    assert_eq!(
        deadlock_core::text::casefold("ŉ Straße Σς İ"),
        "ʼn strasse σσ i\u{307}"
    );
}
#[test]
fn malformed_envelopes_are_bounded_and_rejected() {
    let original = compiled(&[(b"DATA", b"payload")]);
    for (at, value) in [
        (0, 15),
        (0, 99999),
        (4, 11),
        (8, 0),
        (8, u32::MAX),
        (12, 0),
        (12, 640),
        (12, u32::MAX),
        (20, 0),
        (20, u32::MAX),
        (24, u32::MAX),
    ] {
        let mut data = original.clone();
        set32(&mut data, at, value);
        assert!(resource_blocks(&data).is_err(), "field {at}, value {value}");
    }
    let mut overlap = compiled(&[(b"DATA", b"01234567"), (b"PHYS", b"abcdefgh")]);
    set32(&mut overlap, 32, 8); // second payload starts at first payload + 4
    assert!(resource_blocks(&overlap)
        .unwrap_err()
        .to_string()
        .contains("Overlapping"));
    assert!(resource_blocks(&compiled(&[(b"\xffBAD", b"x")])).is_err());
    let empty = compiled(&[(b"MARK", b""), (b"DATA", b"x"), (b"MARK", b"")]);
    assert_eq!(resource_blocks(&empty).unwrap().blocks.len(), 3);
    let mut outside = original.clone();
    outside.extend_from_slice(&[0; 100]);
    set32(&mut outside, 24, 8);
    assert!(resource_blocks(&outside).is_err());
}
#[test]
fn all_truncated_envelope_prefixes_fail_without_panicking() {
    let data = fs::read(fixtures().join("resource.bin")).unwrap();
    for end in 0..data.len() {
        assert!(resource_blocks(&data[..end]).is_err(), "prefix {end}");
    }
}
#[test]
fn malformed_rerl_counts_offsets_strings_and_utf8_rejected() {
    for bytes in [
        b"1234567".to_vec(),
        [8u32.to_le_bytes(), 1u32.to_le_bytes()].concat(),
        [8u32.to_le_bytes(), 1_000_001u32.to_le_bytes()].concat(),
    ] {
        assert!(resource_references(&compiled(&[(b"RERL", &bytes)])).is_err());
    }
    for relative in [i64::MIN, i64::MAX, -100, 999] {
        let payload = [
            8u32.to_le_bytes().as_slice(),
            1u32.to_le_bytes().as_slice(),
            1u64.to_le_bytes().as_slice(),
            relative.to_le_bytes().as_slice(),
            b"name\0",
        ]
        .concat();
        assert!(resource_references(&compiled(&[(b"RERL", &payload)])).is_err());
    }
    for name in [b"../escape\0".as_slice(), b"not terminated", b"bad\xff\0"] {
        let payload = [
            8u32.to_le_bytes().as_slice(),
            1u32.to_le_bytes().as_slice(),
            1u64.to_le_bytes().as_slice(),
            8i64.to_le_bytes().as_slice(),
            name,
        ]
        .concat();
        assert!(resource_references(&compiled(&[(b"RERL", &payload)])).is_err());
    }
}
#[test]
fn unsafe_asset_paths_and_casefold_collisions() {
    for path in [
        "",
        "../secret",
        "/absolute",
        "C:/absolute",
        "a//b",
        "a/./b",
        "a/../b",
        "bad\0",
        "bad\n",
        "bad*",
        "bad?",
        "a:b",
        "a\"b",
    ] {
        assert!(relative_asset(path).is_err(), "{path:?}");
    }
    assert_eq!(
        relative_asset("models\\hero.vmdl").unwrap(),
        "models/hero.vmdl"
    );
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("Straße.txt"), "payload").unwrap();
    fs::write(source.join("STRASSE.txt"), "payload").unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    let error = write_vpk(
        &source,
        &["Straße.txt".into(), "STRASSE.txt".into()],
        &output,
        None,
        2,
        &mut || Ok(()),
    )
    .unwrap_err();
    assert!(error.to_string().contains("case-colliding"));
    assert!(!output.exists());
}
#[test]
fn output_preservation_allowlists_size_and_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    let source = fixtures().join("source");
    let hashes = hashes();
    let files: Vec<_> = hashes.keys().cloned().collect();
    fs::write(&output, "preserve").unwrap();
    assert!(write_vpk(&source, &files, &output, None, 2, &mut || Ok(())).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"preserve");
    fs::remove_file(&output).unwrap();
    for target in [
        temp.path().join("pak01_dir.vpk"),
        source.join("pak18_dir.vpk"),
    ] {
        assert!(write_vpk(&source, &files, &target, None, 2, &mut || Ok(())).is_err());
    }
    assert!(write_vpk(&source, &[], &output, None, 2, &mut || Ok(())).is_err());
    assert!(write_vpk(&source, &files, &output, None, 3, &mut || Ok(())).is_err());
    assert!(write_vpk(
        &source,
        &files,
        &output,
        Some(&Hashes::new()),
        2,
        &mut || Ok(())
    )
    .is_err());
    let mut wrong = hashes.clone();
    wrong.insert("empty.txt".into(), "0".repeat(64));
    assert!(write_vpk(&source, &files, &output, Some(&wrong), 2, &mut || Ok(())).is_err());
    assert!(!output.exists());
    let result = write_vpk(&source, &files, &output, None, 2, &mut || {
        if output.exists() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!output.exists());
}
#[test]
fn large_data_is_streamed_in_bounded_chunks_and_read_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let bytes = vec![42; 3 * 1024 * 1024 + 51];
    fs::write(source.join("big.bin"), &bytes).unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    write_vpk(&source, &["big.bin".into()], &output, None, 2, &mut || {
        Ok(())
    })
    .unwrap();
    let archive = Vpk::open(output).unwrap();
    let mut count = 0;
    let mut chunks = 0;
    archive
        .stream("big.bin", true, &mut || Ok(()), &mut |chunk| {
            assert!(chunk.len() <= 1024 * 1024);
            count += chunk.len();
            chunks += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(count, bytes.len());
    assert_eq!(chunks, 4);
    assert!(matches!(
        archive.verify_files(None, &mut || Err(Error::Cancelled)),
        Err(Error::Cancelled)
    ));
}
#[test]
fn parser_corruption_crc_footer_bounds_and_missing_chunk() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    let original = fs::read(fixtures().join("python-v2_dir.vpk")).unwrap();
    fs::write(&output, &original).unwrap();
    let start = Vpk::open(&output).unwrap().data_start as usize;
    let mut data = original.clone();
    data[start + 20] ^= 1;
    fs::write(&output, data).unwrap();
    let archive = Vpk::open(&output).unwrap();
    assert!(archive.verify_files(None, &mut || Ok(())).is_err());
    assert!(archive.verify_v2_hashes(&mut || Ok(())).is_err());
    drop(archive);
    let mut data = original.clone();
    *data.last_mut().unwrap() ^= 1;
    fs::write(&output, data).unwrap();
    let archive = Vpk::open(&output).unwrap();
    archive
        .verify_files(Some(&hashes()), &mut || Ok(()))
        .unwrap();
    assert!(archive.verify_v2_hashes(&mut || Ok(())).is_err());
    drop(archive);
    for (offset, value) in [(0, 0), (4, 3), (8, u32::MAX), (12, u32::MAX)] {
        let mut data = original.clone();
        set32(&mut data, offset, value);
        fs::write(&output, data).unwrap();
        assert!(Vpk::open(&output).is_err());
    }
    fs::copy(fixtures().join("preload-v1-1_dir.vpk"), &output).unwrap();
    assert!(Vpk::open(&output)
        .unwrap()
        .verify_files(None, &mut || Ok(()))
        .is_err());
}
#[test]
fn sparse_oversize_input_is_rejected_before_archive_creation() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::File::create(source.join("huge.bin"))
        .unwrap()
        .set_len(1u64 << 31)
        .unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    assert!(
        write_vpk(&source, &["huge.bin".into()], &output, None, 2, &mut || Ok(
            ()
        ))
        .unwrap_err()
        .to_string()
        .contains("2 GiB")
    );
    assert!(!output.exists());
}

#[test]
fn python_snapshot_receipt_and_selection_parity() {
    use deadlock_vpk::{create_snapshot, remove_snapshot, SnapshotOptions};
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("snapshot");
    let opts = SnapshotOptions {
        settle_time: std::time::Duration::ZERO,
        retry_time: std::time::Duration::ZERO,
        ..Default::default()
    };
    let receipt = create_snapshot(
        &fixtures().join("source"),
        &destination,
        &opts,
        &mut || Ok(()),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(receipt).unwrap(),
        expected()["snapshot_receipt"]
    );
    remove_snapshot(&destination).unwrap();
}
