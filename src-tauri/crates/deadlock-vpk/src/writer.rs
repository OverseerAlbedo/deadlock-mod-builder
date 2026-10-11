use crate::{
    hex, key,
    reader::md5_range,
    relative_asset,
    stable::{check_unlinked_path, open_read, resolve_new, Baseline},
    Check, Error, Hashes, Result, CHUNK_BYTES, MAX_ARCHIVE_BYTES, MAX_TREE_BYTES,
};
use md5::Md5;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageReceipt {
    pub backend: String,
    pub format: String,
    pub version: u32,
    pub single_file: bool,
    pub file_count: usize,
    pub payload_bytes: u64,
    pub archive_bytes: u64,
    pub files: Hashes,
}
struct Source {
    path: String,
    file: PathBuf,
    baseline: Baseline,
    crc_position: u64,
}
struct Pending {
    path: PathBuf,
    remove: bool,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if self.remove {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Creates a NEW single-file archive from an explicit allowlist. Does not publish.
/// Memory is bounded to the directory, source metadata and 1 MiB payload buffers.
/// If a source changes, a hash mismatches or cancellation occurs, the new output
/// is removed. Existing outputs and the original game archive are never changed.
pub fn write_vpk(
    root: &Path,
    files: &[String],
    destination: &Path,
    expected: Option<&Hashes>,
    version: u32,
    check: &mut Check<'_>,
) -> Result<PackageReceipt> {
    if !matches!(version, 1 | 2) {
        return Err(Error::invalid(
            "Built-in writer supports VPK versions 1 and 2.",
        ));
    }
    check_unlinked_path(root, true)?;
    check_unlinked_path(destination, true)?;
    let root = root.canonicalize().map_err(|e| Error::io(root, e))?;
    let destination = resolve_new(destination)?;
    if !root.is_dir() {
        return Err(Error::invalid("Package source folder does not exist."));
    }
    if destination
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|s| key(s) == "pak01_dir.vpk")
    {
        return Err(Error::invalid(
            "pak01_dir.vpk is reserved for the original game; choose pak02–pak99.",
        ));
    }
    if destination.starts_with(&root) {
        return Err(Error::invalid(
            "Package output must be outside its source folder.",
        ));
    }
    if destination
        .try_exists()
        .map_err(|e| Error::io(&destination, e))?
    {
        return Err(Error::invalid("VPK writer only creates new archives; existing output needs explicit backup/replacement."));
    }
    let mut seen = BTreeSet::new();
    let mut groups = BTreeMap::<String, BTreeMap<String, BTreeMap<String, Source>>>::new();
    for value in files {
        check()?;
        let relative = relative_asset(value)?;
        if !seen.insert(key(&relative)) {
            return Err(Error::invalid(format!(
                "Duplicate/case-colliding package path: {relative}"
            )));
        }
        let file = root.join(&relative);
        let baseline = Baseline::capture(&file)?;
        let (folder, filename) = relative.rsplit_once('/').unwrap_or((" ", &relative));
        let (name, ext) = match filename.rsplit_once('.') {
            Some((name, ext)) if !name.is_empty() && !ext.is_empty() => (name, ext),
            _ => (filename, " "),
        };
        for part in [ext, folder, name] {
            if part.is_empty() || part.len() >= 32768 {
                return Err(Error::invalid(format!(
                    "Package path component is too long: {relative}"
                )));
            }
        }
        if name.trim().is_empty()
            || filename.ends_with('.')
            || (ext == " " && name != filename)
            || (folder == " " && relative.contains('/'))
        {
            return Err(Error::invalid(format!(
                "Unsupported package filename or sentinel directory: {relative}"
            )));
        }
        groups
            .entry(ext.to_owned())
            .or_default()
            .entry(folder.to_owned())
            .or_default()
            .insert(
                name.to_owned(),
                Source {
                    path: relative,
                    file,
                    baseline,
                    crc_position: 0,
                },
            );
    }
    if seen.is_empty() {
        return Err(Error::invalid("No files selected for packaging."));
    }
    let mut tree = Vec::new();
    let mut sources = Vec::new();
    let mut data_size = 0u64;
    for (ext, folders) in groups {
        cstring(&mut tree, &ext);
        for (folder, names) in folders {
            cstring(&mut tree, &folder);
            for (name, mut source) in names {
                let size = source.baseline.size();
                if size > MAX_ARCHIVE_BYTES || data_size + size > MAX_ARCHIVE_BYTES {
                    return Err(Error::invalid("Single-file VPK exceeds the supported 2 GiB limit. Reduce the payload; split archive writing is not implemented."));
                }
                cstring(&mut tree, &name);
                source.crc_position = tree.len() as u64;
                tree.extend_from_slice(&0u32.to_le_bytes());
                tree.extend_from_slice(&0u16.to_le_bytes());
                tree.extend_from_slice(&0x7fffu16.to_le_bytes());
                tree.extend_from_slice(&(data_size as u32).to_le_bytes());
                tree.extend_from_slice(&(size as u32).to_le_bytes());
                tree.extend_from_slice(&0xffffu16.to_le_bytes());
                data_size += size;
                sources.push(source);
                if tree.len() as u64 > MAX_TREE_BYTES {
                    return Err(Error::invalid(
                        "VPK directory exceeds the supported 128 MiB limit.",
                    ));
                }
            }
            tree.push(0);
        }
        tree.push(0);
    }
    tree.push(0);
    let (header_size, footer_size) = if version == 2 {
        (28u64, 48u64)
    } else {
        (12, 0)
    };
    let total = header_size + tree.len() as u64 + data_size + footer_size;
    if total > MAX_ARCHIVE_BYTES || tree.len() as u64 > MAX_TREE_BYTES {
        return Err(Error::invalid(
            "Single-file VPK exceeds the supported size limit, including its directory.",
        ));
    }
    if let Some(expected) = expected {
        let selected: BTreeSet<_> = sources.iter().map(|s| &s.path).collect();
        if !selected.into_iter().eq(expected.keys()) {
            return Err(Error::invalid(
                "Package hash allowlist does not match its file list.",
            ));
        }
    }
    let parent = destination
        .parent()
        .ok_or_else(|| Error::invalid("Package output has no parent."))?;
    fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    check_unlinked_path(&destination, true)?;
    let mut pending = Pending {
        path: destination.clone(),
        remove: false,
    };
    // Declare the guard before the file: unwinding/errors close the Windows
    // handle before cleanup attempts to unlink the incomplete archive.
    let mut out = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|e| Error::io(&destination, e))?;
    pending.remove = true;
    let write = |out: &mut std::fs::File, bytes: &[u8]| {
        out.write_all(bytes).map_err(|e| Error::io(&destination, e))
    };
    write(&mut out, &0x55aa1234u32.to_le_bytes())?;
    write(&mut out, &version.to_le_bytes())?;
    write(&mut out, &(tree.len() as u32).to_le_bytes())?;
    if version == 2 {
        for value in [data_size as u32, 0, 48, 0] {
            write(&mut out, &value.to_le_bytes())?;
        }
    }
    write(&mut out, &tree)?;
    let mut hashes = Hashes::new();
    let mut buffer = vec![0; CHUNK_BYTES];
    for source in &sources {
        check()?;
        source.baseline.check_path(&source.file)?;
        let mut input = open_read(&source.file)?;
        source.baseline.check_file(&source.file, &input)?;
        let mut crc = crc32fast::Hasher::new();
        let mut sha = Sha256::new();
        let mut count = 0u64;
        loop {
            check()?;
            let n = input
                .read(&mut buffer)
                .map_err(|e| Error::io(&source.file, e))?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > source.baseline.size() {
                return Err(Error::Unstable(format!(
                    "{} grew during build",
                    source.path
                )));
            }
            write(&mut out, &buffer[..n])?;
            sha.update(&buffer[..n]);
            crc.update(&buffer[..n]);
        }
        if count != source.baseline.size() {
            return Err(Error::Unstable(format!(
                "{} size changed during read",
                source.path
            )));
        }
        source.baseline.check_file(&source.file, &input)?;
        source.baseline.check_path(&source.file)?;
        let hash = hex(sha.finalize());
        if expected.is_some_and(|e| e.get(&source.path) != Some(&hash)) {
            return Err(Error::invalid(format!(
                "Staged content hash changed before packaging: {}",
                source.path
            )));
        }
        hashes.insert(source.path.clone(), hash);
        let crc = crc.finalize();
        let end = out
            .stream_position()
            .map_err(|e| Error::io(&destination, e))?;
        out.seek(SeekFrom::Start(header_size + source.crc_position))
            .map_err(|e| Error::io(&destination, e))?;
        write(&mut out, &crc.to_le_bytes())?;
        tree[source.crc_position as usize..source.crc_position as usize + 4]
            .copy_from_slice(&crc.to_le_bytes());
        out.seek(SeekFrom::Start(end))
            .map_err(|e| Error::io(&destination, e))?;
    }
    if version == 2 {
        let tree_hash = Md5::digest(&tree);
        let archive_hash = Md5::digest([]);
        write(&mut out, &tree_hash)?;
        write(&mut out, &archive_hash)?;
        out.flush().map_err(|e| Error::io(&destination, e))?;
        let whole = md5_range(&mut out, &destination, 0, total - 16, check)?;
        write(&mut out, &whole)?;
    }
    check()?;
    for source in &sources {
        source.baseline.check_path(&source.file)?;
    }
    out.sync_all().map_err(|e| Error::io(&destination, e))?;
    // Close the file before disarming cleanup or returning a publishable receipt.
    drop(out);
    pending.remove = false;
    Ok(PackageReceipt {
        backend: "builtin-rust".into(),
        format: "VPK".into(),
        version,
        single_file: true,
        file_count: sources.len(),
        payload_bytes: data_size,
        archive_bytes: total,
        files: hashes,
    })
}
fn cstring(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(value.as_bytes());
    out.push(0);
}
