use deadlock_vpk::{Vpk, MAX_READ_BYTES};
use std::fs;

fn entry_record(
    crc: u32,
    preload: u16,
    archive: u16,
    offset: u32,
    length: u32,
    terminator: u16,
) -> Vec<u8> {
    [
        crc.to_le_bytes().as_slice(),
        preload.to_le_bytes().as_slice(),
        archive.to_le_bytes().as_slice(),
        offset.to_le_bytes().as_slice(),
        length.to_le_bytes().as_slice(),
        terminator.to_le_bytes().as_slice(),
    ]
    .concat()
}
fn index(version: u32, tree: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = [
        0x55aa1234u32.to_le_bytes(),
        version.to_le_bytes(),
        (tree.len() as u32).to_le_bytes(),
    ]
    .concat();
    if version == 2 {
        out.extend_from_slice(
            &[data.len() as u32, 0, 0, 0]
                .iter()
                .flat_map(|n| n.to_le_bytes())
                .collect::<Vec<_>>(),
        );
    }
    out.extend_from_slice(tree);
    out.extend_from_slice(data);
    out
}
fn tree(names: &[&[u8]], record: &[u8], folder: &[u8]) -> Vec<u8> {
    let mut out = b"txt\0".to_vec();
    out.extend_from_slice(folder);
    out.push(0);
    for name in names {
        out.extend_from_slice(name);
        out.push(0);
        out.extend_from_slice(record);
    }
    out.extend_from_slice(&[0, 0, 0]);
    out
}
#[test]
fn truncated_headers_directory_records_preload_and_strings_fail() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    let record = entry_record(0, 0, 0x7fff, 0, 0, 0xffff);
    let valid = index(1, &tree(&[b"name"], &record, b" "), b"");
    for count in 0..valid.len() {
        fs::write(&output, &valid[..count]).unwrap();
        assert!(Vpk::open(&output).is_err(), "prefix {count}");
    }
    let mut cases = vec![
        b"not-terminated".to_vec(),
        tree(&[b"name"], &record[..17], b" "),
        tree(&[b"\xff"], &record, b" "),
    ];
    cases.push(tree(
        &[b"name"],
        &entry_record(0, 100, 0x7fff, 0, 0, 0xffff),
        b" ",
    ));
    let huge_name = vec![b'x'; 32768];
    cases.push(tree(&[&huge_name], &record, b" "));
    for value in cases {
        fs::write(&output, index(1, &value, b"")).unwrap();
        assert!(Vpk::open(&output).is_err());
    }
}
#[test]
fn duplicate_casefold_paths_traversal_and_terminator_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    let record = entry_record(0, 0, 0x7fff, 0, 0, 0xffff);
    for value in [
        tree(&[b"name", b"NAME"], &record, b" "),
        tree(&["Straße".as_bytes(), b"STRASSE"], &record, b" "),
        tree(&[b"name"], &record, b".."),
        tree(&[b"../escape"], &record, b" "),
        tree(&[b"name"], &entry_record(0, 0, 0x7fff, 0, 0, 0), b" "),
    ] {
        fs::write(&output, index(1, &value, b"")).unwrap();
        assert!(Vpk::open(&output).is_err());
    }
    let mut trailing = tree(&[b"name"], &record, b" ");
    trailing.push(0);
    fs::write(&output, index(1, &trailing, b"")).unwrap();
    assert!(Vpk::open(&output).is_err());
}
#[test]
fn entry_bounds_and_in_memory_read_limit_fail_before_allocation() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    for (offset, length) in [(0, 2), (u32::MAX, 1), (1, u32::MAX)] {
        let value = tree(
            &[b"name"],
            &entry_record(0, 0, 0x7fff, offset, length, 0xffff),
            b" ",
        );
        fs::write(&output, index(2, &value, b"x")).unwrap();
        let archive = Vpk::open(&output).unwrap();
        assert!(archive.read("name.txt").is_err());
        assert!(archive.verify_files(None, &mut || Ok(())).is_err());
    }
    let value = tree(
        &[b"name"],
        &entry_record(0, 0, 0x7fff, 0, MAX_READ_BYTES as u32 + 1, 0xffff),
        b" ",
    );
    fs::write(&output, index(1, &value, b"")).unwrap();
    assert!(Vpk::open(&output)
        .unwrap()
        .read("name.txt")
        .unwrap_err()
        .to_string()
        .contains("512 MiB"));
}
#[test]
fn truncated_or_invalid_v2_footer_sections_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    let record = entry_record(0, 0, 0x7fff, 0, 0, 0xffff);
    for (archive_size, other_size, signature_size) in
        [(0, 0, 0), (0, 48, 0), (u32::MAX, 48, 0), (0, 48, u32::MAX)]
    {
        let mut data = index(2, &tree(&[b"name"], &record, b" "), b"");
        data[16..20].copy_from_slice(&u32::to_le_bytes(archive_size));
        data[20..24].copy_from_slice(&u32::to_le_bytes(other_size));
        data[24..28].copy_from_slice(&u32::to_le_bytes(signature_size));
        fs::write(&output, data).unwrap();
        assert!(Vpk::open(&output)
            .unwrap()
            .verify_v2_hashes(&mut || Ok(()))
            .is_err());
    }
}
