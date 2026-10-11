use crate::*;
use deadlock_core::text::casefold;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
};

const MAX_IMPORT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcePackageSelection {
    pub dmx: String,
    pub expected_dmx_sha256: String,
    pub mesh_template: String,
    pub source_overlay: String,
    pub material_mapping: String,
    pub material_remap: String,
    pub skeleton_reference: String,
}
#[derive(Deserialize)]
struct ManifestEntry {
    path: String,
    bytes: u64,
    sha256: String,
}
fn inspect(destination: &Path, cancel: &CancellationToken) -> Result<SourcePackageSelection> {
    let files = walk_files(destination, cancel)?;
    let roots: Vec<_> = files
        .iter()
        .filter(|p| {
            p.file_name().is_some_and(|n| n == "manifest.json")
                && p.strip_prefix(destination)
                    .is_ok_and(|r| r.components().count() <= 2)
        })
        .collect();
    if roots.len() != 1 {
        return Err(invalid(
            "Expected one source package root with manifest.json.",
        ));
    }
    let root = roots[0]
        .parent()
        .ok_or_else(|| invalid("Manifest root missing."))?;
    let manifest: Vec<ManifestEntry> = serde_json::from_str(&read_text(roots[0])?)?;
    let mut names = BTreeSet::new();
    for item in manifest {
        cancel.check()?;
        let asset = inside(root, &item.path)?;
        if !names.insert(casefold(&item.path))
            || !asset.is_file()
            || asset.metadata()?.len() != item.bytes
            || safety::digest_with_cancel(&asset, cancel)? != item.sha256
        {
            return Err(invalid(format!(
                "Source package manifest mismatch: {}",
                item.path
            )));
        }
    }
    let mut dmxs = Vec::new();
    let mut templates = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_file() {
            match path.extension().and_then(|p| p.to_str()) {
                Some("dmx") => dmxs.push(path),
                Some("vmdl") => templates.push(path),
                _ => {}
            }
        }
    }
    if dmxs.len() != 1 || templates.len() != 1 || !root.join("source_materials").is_dir() {
        return Err(invalid(
            "Expected one DMX, one compile template and source_materials/ overlay.",
        ));
    }
    let skeleton = root.join("audit/original_skeleton_reference.json");
    Ok(SourcePackageSelection {
        dmx: path_text(&dmxs[0])?,
        expected_dmx_sha256: safety::digest_with_cancel(&dmxs[0], cancel)?,
        mesh_template: path_text(&templates[0])?,
        source_overlay: path_text(&root.join("source_materials"))?,
        material_mapping: path_text(&root.join("material_mapping.json"))?,
        material_remap: path_text(&root.join("material_remap.json"))?,
        skeleton_reference: if skeleton.is_file() {
            path_text(&skeleton)?
        } else {
            String::new()
        },
    })
}
/// Imports data only, never executes packaged scripts. Existing imports are
/// reverified on every call; extracted files are staged before publication.
pub fn import_zip(
    file: &Path,
    workspace: &Path,
    cancel: &CancellationToken,
) -> Result<SourcePackageSelection> {
    deadlock_vpk::check_unlinked_path(file, false)?;
    deadlock_vpk::check_unlinked_path(workspace, true)?;
    let hash = safety::digest_with_cancel(file, cancel)?;
    let destination = inside(workspace, &format!("source-packages/{hash}"))?;
    if destination.exists() {
        return inspect(&destination, cancel);
    }
    fs::create_dir_all(workspace)?;
    let pending = tempfile::Builder::new()
        .prefix("source-package-pending-")
        .tempdir_in(workspace)?;
    let mut archive = zip::ZipArchive::new(fs::File::open(file)?)?;
    verify_central_entry_count(
        file,
        archive.central_directory_start(),
        archive.len(),
        cancel,
    )?;
    let mut total = 0u64;
    let mut names = BTreeSet::new();
    // Preflight every path/size before writing anything.
    for i in 0..archive.len() {
        cancel.check()?;
        let item = archive.by_index(i)?;
        total = total
            .checked_add(item.size())
            .ok_or_else(|| invalid("Source package size overflow."))?;
        if total > MAX_IMPORT_BYTES {
            return Err(invalid("Source package exceeds the 4 GiB import limit."));
        }
        let name = item.name().trim_end_matches('/');
        inside(pending.path(), name)?;
        if item.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
            || (!item.is_dir() && !names.insert(casefold(name)))
        {
            return Err(invalid("Source ZIP has duplicate paths or links."));
        }
    }
    let mut buffer = vec![0; 1024 * 1024];
    for i in 0..archive.len() {
        cancel.check()?;
        let raw = archive.by_index_raw(i)?;
        let lzma = raw.compression() == zip::CompressionMethod::LZMA;
        drop(raw);
        if lzma {
            let mut item = archive.by_index_raw(i)?;
            if item.encrypted() {
                return Err(invalid("Encrypted source ZIP entries are unsupported."));
            }
            if item.is_dir() {
                continue;
            }
            let target = inside(pending.path(), item.name())?;
            fs::create_dir_all(
                target
                    .parent()
                    .ok_or_else(|| invalid("ZIP target parent missing."))?,
            )?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            decode_lzma(&mut item, &mut output, cancel)?;
            output.sync_all()?;
            continue;
        }
        let mut item = archive.by_index(i)?;
        if item.is_dir() {
            continue;
        }
        let target = inside(pending.path(), item.name())?;
        fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| invalid("ZIP target parent missing."))?,
        )?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let expected = item.size();
        let mut count = 0u64;
        loop {
            cancel.check()?;
            let n = item.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > expected {
                return Err(invalid("ZIP entry expands beyond its declared size."));
            }
            output.write_all(&buffer[..n])?;
        }
        if count != expected {
            return Err(invalid("ZIP entry size mismatch."));
        }
        output.sync_all()?;
    }
    drop(archive);
    if safety::digest_with_cancel(file, cancel)? != hash {
        return Err(invalid("Source package changed during import."));
    }
    // Validate before exposing the new directory, then rebase returned paths by
    // inspecting the published data. Malformed packages leave no cached result.
    inspect(pending.path(), cancel)?;
    cancel.check()?;
    fs::create_dir_all(
        destination
            .parent()
            .ok_or_else(|| invalid("Package destination parent missing."))?,
    )?;
    deadlock_vpk::publish_directory(pending.path(), &destination)?;
    inspect(&destination, cancel)
}

// ZIP LZMA stores a version/length wrapper and five filter-property bytes, not
// an LZMA-alone 13-byte header. zip 2.4's built-in decoder does not consume that
// wrapper correctly. Decode raw entry bytes through lzma-rs with bounded output,
// cancellation, dictionary memory limit, finalization, length and CRC checks.
fn decode_lzma(
    item: &mut zip::read::ZipFile<'_>,
    output: &mut fs::File,
    cancel: &CancellationToken,
) -> Result<()> {
    let expected = item.size();
    let expected_crc = item.crc32();
    let mut wrapper = [0; 4];
    item.read_exact(&mut wrapper)?;
    let properties = u16::from_le_bytes([wrapper[2], wrapper[3]]);
    if properties != 5 {
        return Err(invalid("Unsupported ZIP LZMA filter property length."));
    }
    let options = lzma_rs::decompress::Options {
        unpacked_size: lzma_rs::decompress::UnpackedSize::UseProvided(None),
        memlimit: Some(256 * 1024 * 1024),
        allow_incomplete: false,
    };
    let sink = BoundedOutput {
        output,
        cancel,
        expected,
        count: 0,
        crc: crc32fast::Hasher::new(),
    };
    let mut stream = lzma_rs::decompress::Stream::new_with_options(&options, sink);
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        cancel.check()?;
        let n = item.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        if let Err(error) = stream.write_all(&buffer[..n]) {
            cancel.check()?;
            return Err(error.into());
        }
    }
    let sink = match stream.finish() {
        Ok(sink) => sink,
        Err(error) => {
            cancel.check()?;
            return Err(invalid(format!("Malformed ZIP LZMA stream: {error}")));
        }
    };
    cancel.check()?;
    if sink.count != expected || sink.crc.finalize() != expected_crc {
        return Err(invalid("ZIP LZMA entry size/checksum mismatch."));
    }
    Ok(())
}
struct BoundedOutput<'a> {
    output: &'a mut fs::File,
    cancel: &'a CancellationToken,
    expected: u64,
    count: u64,
    crc: crc32fast::Hasher,
}
impl Write for BoundedOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.cancel
            .check()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Interrupted, e))?;
        if bytes.len() as u64 > self.expected.saturating_sub(self.count) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "ZIP LZMA entry expands beyond its declared size.",
            ));
        }
        self.output.write_all(bytes)?;
        self.count += bytes.len() as u64;
        self.crc.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.output.flush()
    }
}

// zip 2.x folds exact duplicate names into an IndexMap before exposing len().
// Independently count bounded central records so duplicates cannot disappear
// before the archive path/case-collision checks below see them.
fn verify_central_entry_count(
    path: &Path,
    start: u64,
    unique: usize,
    cancel: &CancellationToken,
) -> Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut input = fs::File::open(path)?;
    let length = input.metadata()?.len();
    input.seek(SeekFrom::Start(start))?;
    let mut count = 0usize;
    loop {
        cancel.check()?;
        let mut signature = [0; 4];
        input.read_exact(&mut signature)?;
        if signature != [0x50, 0x4b, 0x01, 0x02] {
            break;
        }
        count += 1;
        if count > unique {
            return Err(invalid("Source ZIP has duplicate central-directory paths."));
        }
        let mut header = [0; 42];
        input.read_exact(&mut header)?;
        let suffix = [24usize, 26, 28]
            .into_iter()
            .map(|p| u16::from_le_bytes([header[p], header[p + 1]]) as u64)
            .sum::<u64>();
        let next = input
            .stream_position()?
            .checked_add(suffix)
            .ok_or_else(|| invalid("ZIP central directory offset overflow."))?;
        if next > length {
            return Err(invalid("Truncated ZIP central directory."));
        }
        input.seek(SeekFrom::Start(next))?;
    }
    if count != unique {
        return Err(invalid("ZIP central-directory count mismatch."));
    }
    Ok(())
}
