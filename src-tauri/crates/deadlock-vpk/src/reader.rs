use crate::{
    hex, key, relative_asset,
    stable::{open_read, Baseline},
    u16_at, u32_at, Check, Error, Hashes, Result, CHUNK_BYTES, MAX_READ_BYTES, MAX_TREE_BYTES,
};
use md5::Md5;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub crc: u32,
    pub archive: u16,
    pub offset: u32,
    pub length: u32,
    pub preload: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub path: String,
    pub likely_hero: bool,
}

#[derive(Debug)]
pub struct Vpk {
    pub path: PathBuf,
    pub version: u32,
    pub entries: BTreeMap<String, Entry>,
    pub data_start: u64,
    pub embedded_end: u64,
    v2_sections: Option<[u32; 4]>,
    baseline: Baseline,
}

struct Tree<'a> {
    data: &'a [u8],
    position: usize,
}
impl Tree<'_> {
    fn string(&mut self) -> Result<String> {
        let rest = &self.data[self.position..];
        let n = rest
            .iter()
            .take(32768)
            .position(|b| *b == 0)
            .ok_or_else(|| Error::invalid("Truncated VPK directory or invalid VPK path length."))?;
        let result = std::str::from_utf8(&rest[..n])
            .map_err(|_| Error::invalid("Invalid UTF-8 in VPK directory."))?
            .to_owned();
        self.position += n + 1;
        Ok(result)
    }
    fn bytes(&mut self, count: usize) -> Result<&[u8]> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| Error::invalid("VPK directory overflow."))?;
        let value = self
            .data
            .get(self.position..end)
            .ok_or_else(|| Error::invalid("Truncated VPK directory."))?;
        self.position = end;
        Ok(value)
    }
}

impl Vpk {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_owned();
        let mut file = open_read(&path)?;
        let baseline = Baseline::from_file(&path, &file)?;
        let mut header = [0; 12];
        file.read_exact(&mut header)
            .map_err(|e| Error::io(&path, e))?;
        let version = u32_at(&header, 4)?;
        if u32_at(&header, 0)? != 0x55aa1234 || !matches!(version, 1 | 2) {
            return Err(Error::invalid("Not a supported VPK v1/v2 archive."));
        }
        let tree_size = u32_at(&header, 8)? as u64;
        let mut sections = None;
        if version == 2 {
            let mut extra = [0; 16];
            file.read_exact(&mut extra)
                .map_err(|e| Error::io(&path, e))?;
            sections = Some([
                u32_at(&extra, 0)?,
                u32_at(&extra, 4)?,
                u32_at(&extra, 8)?,
                u32_at(&extra, 12)?,
            ]);
        }
        let header_size = if version == 1 { 12 } else { 28 };
        if tree_size > MAX_TREE_BYTES || tree_size + header_size > baseline.size() {
            return Err(Error::invalid("Invalid VPK directory size."));
        }
        let data_start = header_size + tree_size;
        let embedded_end = sections
            .map(|s| data_start + s[0] as u64)
            .unwrap_or(baseline.size());
        if embedded_end > baseline.size() {
            return Err(Error::invalid(
                "VPK data section extends beyond the archive.",
            ));
        }
        let mut bytes = vec![0; tree_size as usize];
        file.read_exact(&mut bytes)
            .map_err(|e| Error::io(&path, e))?;
        baseline.check_file(&path, &file)?;
        baseline.check_path(&path)?;
        let mut tree = Tree {
            data: &bytes,
            position: 0,
        };
        let mut entries = BTreeMap::new();
        loop {
            let ext = tree.string()?;
            if ext.is_empty() {
                break;
            }
            loop {
                let folder = tree.string()?;
                if folder.is_empty() {
                    break;
                }
                loop {
                    let name = tree.string()?;
                    if name.is_empty() {
                        break;
                    }
                    let record = tree.bytes(18)?;
                    let (crc, preload_size, archive, offset, length, terminator) = (
                        u32_at(record, 0)?,
                        u16_at(record, 4)?,
                        u16_at(record, 6)?,
                        u32_at(record, 8)?,
                        u32_at(record, 12)?,
                        u16_at(record, 16)?,
                    );
                    if terminator != 0xffff {
                        return Err(Error::invalid("Invalid VPK entry terminator."));
                    }
                    let path_text = format!(
                        "{}{}{}",
                        if folder == " " {
                            String::new()
                        } else {
                            format!("{folder}/")
                        },
                        name,
                        if ext == " " {
                            String::new()
                        } else {
                            format!(".{ext}")
                        }
                    );
                    let path_text = relative_asset(&path_text)?;
                    let preload = tree.bytes(preload_size as usize)?.to_vec();
                    let folded = key(&path_text);
                    if entries.contains_key(&folded) {
                        return Err(Error::invalid("Invalid/duplicate VPK entry."));
                    }
                    entries.insert(
                        folded,
                        Entry {
                            path: path_text,
                            crc,
                            archive,
                            offset,
                            length,
                            preload,
                        },
                    );
                }
            }
        }
        if tree.position != bytes.len() {
            return Err(Error::invalid("Unexpected trailing VPK directory data."));
        }
        Ok(Self {
            path,
            version,
            entries,
            data_start,
            embedded_end,
            v2_sections: sections,
            baseline,
        })
    }

    fn entry(&self, relative: &str) -> Result<&Entry> {
        self.entries
            .get(&key(&relative_asset(relative)?))
            .ok_or_else(|| Error::invalid(format!("Resource is missing from VPK: {relative}")))
    }

    /// Exhaustively stream and check one entry. A consumer error aborts immediately.
    /// Consumers must not publish collected output until this function succeeds.
    pub fn stream(
        &self,
        relative: &str,
        verify: bool,
        check: &mut Check<'_>,
        consume: &mut dyn FnMut(&[u8]) -> Result<()>,
    ) -> Result<()> {
        let entry = self.entry(relative)?;
        check()?;
        self.baseline.check_path(&self.path)?;
        let (path, offset, bound) = if entry.archive == 0x7fff {
            (
                self.path.clone(),
                self.data_start + entry.offset as u64,
                Some(self.embedded_end),
            )
        } else {
            let stem = self
                .path
                .file_stem()
                .and_then(|p| p.to_str())
                .ok_or_else(|| Error::invalid("VPK index filename is not UTF-8."))?;
            (
                self.path.with_file_name(format!(
                    "{}_{:03}.vpk",
                    stem.strip_suffix("_dir").unwrap_or(stem),
                    entry.archive
                )),
                entry.offset as u64,
                None,
            )
        };
        let mut crc = crc32fast::Hasher::new();
        crc.update(&entry.preload);
        if !entry.preload.is_empty() {
            consume(&entry.preload)?;
        }
        if entry.length != 0 {
            let mut file = open_read(&path)?;
            let file_baseline = Baseline::from_file(&path, &file)?;
            if entry.archive == 0x7fff {
                self.baseline.check_file(&path, &file)?;
            }
            let bound = bound
                .unwrap_or(file_baseline.size())
                .min(file_baseline.size());
            if offset + entry.length as u64 > bound {
                return Err(Error::invalid(format!(
                    "VPK entry extends beyond its data section: {relative}"
                )));
            }
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| Error::io(&path, e))?;
            let mut buffer = vec![0; CHUNK_BYTES];
            let mut left = entry.length as usize;
            while left != 0 {
                check()?;
                let n = left.min(buffer.len());
                file.read_exact(&mut buffer[..n])
                    .map_err(|e| Error::io(&path, e))?;
                crc.update(&buffer[..n]);
                consume(&buffer[..n])?;
                left -= n;
            }
            file_baseline.check_file(&path, &file)?;
            file_baseline.check_path(&path)?;
        }
        self.baseline.check_path(&self.path)?;
        if verify && crc.finalize() != entry.crc {
            return Err(Error::invalid(format!(
                "VPK checksum/length mismatch: {relative}"
            )));
        }
        Ok(())
    }

    pub fn read(&self, relative: &str) -> Result<Vec<u8>> {
        let entry = self.entry(relative)?;
        let size = entry.length as u64 + entry.preload.len() as u64;
        if size > MAX_READ_BYTES {
            return Err(Error::invalid(format!(
                "Resource exceeds the 512 MiB read limit: {relative}; use streaming verification."
            )));
        }
        let mut data = Vec::with_capacity(size as usize);
        self.stream(relative, true, &mut || Ok(()), &mut |bytes| {
            data.extend_from_slice(bytes);
            Ok(())
        })?;
        Ok(data)
    }

    pub fn verify_files(&self, expected: Option<&Hashes>, check: &mut Check<'_>) -> Result<Hashes> {
        let mut folded = BTreeMap::new();
        if let Some(expected) = expected {
            for (path, value) in expected {
                if folded.insert(key(&relative_asset(path)?), value).is_some() {
                    return Err(Error::invalid(
                        "Packaged VPK file list does not match staged compiled resources.",
                    ));
                }
            }
            if !folded.keys().eq(self.entries.keys()) {
                return Err(Error::invalid(
                    "Packaged VPK file list does not match staged compiled resources.",
                ));
            }
        }
        let mut hashes = Hashes::new();
        for (key, entry) in &self.entries {
            let mut sha = Sha256::new();
            self.stream(&entry.path, true, check, &mut |bytes| {
                sha.update(bytes);
                Ok(())
            })?;
            let hash = hex(sha.finalize());
            if expected.is_some() && folded.get(key).is_none_or(|v| **v != hash) {
                return Err(Error::invalid(format!(
                    "Packaged VPK content hash mismatch: {}",
                    entry.path
                )));
            }
            hashes.insert(entry.path.clone(), hash);
        }
        Ok(hashes)
    }

    /// Standard v2 integrity hashes, not publisher authentication.
    pub fn verify_v2_hashes(&self, check: &mut Check<'_>) -> Result<()> {
        let sections = self
            .v2_sections
            .ok_or_else(|| Error::invalid("Archive-wide MD5 metadata is a VPK v2 feature."))?;
        if sections[2] != 48 {
            return Err(Error::invalid(
                "VPK v2 does not contain the expected 48-byte checksum section.",
            ));
        }
        let other_start = self.embedded_end + sections[1] as u64;
        if other_start + sections[2] as u64 + sections[3] as u64 != self.baseline.size() {
            return Err(Error::invalid(
                "VPK v2 checksum/signature sections do not match file length.",
            ));
        }
        let mut file = open_read(&self.path)?;
        self.baseline.check_file(&self.path, &file)?;
        file.seek(SeekFrom::Start(other_start))
            .map_err(|e| Error::io(&self.path, e))?;
        let mut expected = [0; 48];
        file.read_exact(&mut expected)
            .map_err(|e| Error::io(&self.path, e))?;
        let mut actual = Vec::with_capacity(48);
        for (start, length) in [
            (28, self.data_start - 28),
            (self.embedded_end, sections[1] as u64),
            (0, other_start + 32),
        ] {
            actual.extend_from_slice(&md5_range(&mut file, &self.path, start, length, check)?);
        }
        self.baseline.check_file(&self.path, &file)?;
        self.baseline.check_path(&self.path)?;
        if actual != expected {
            return Err(Error::invalid("VPK v2 archive checksum mismatch."));
        }
        Ok(())
    }

    pub fn models(&self, all_models: bool) -> Vec<Model> {
        let mut results = Vec::new();
        for e in self.entries.values() {
            let Some(stem) = e.path.strip_suffix(".vmdl_c") else {
                continue;
            };
            let (folder, name) = stem.rsplit_once('/').unwrap_or(("", stem));
            let likely = folder
                .split('/')
                .any(|p| p.starts_with("hero") || p.starts_with("character"));
            if all_models || likely {
                // Display only; archive keys never use title casing.
                let mut boundary = true;
                let mut title = String::new();
                for c in name.replace('_', " ").chars() {
                    if c.is_alphabetic() {
                        title.extend(if boundary {
                            c.to_uppercase().collect::<Vec<_>>()
                        } else {
                            c.to_lowercase().collect()
                        });
                        boundary = false;
                    } else {
                        title.push(c);
                        boundary = true;
                    }
                }
                results.push(Model {
                    name: title,
                    path: e.path.clone(),
                    likely_hero: likely,
                });
            }
        }
        results.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
        results
    }
}

pub(crate) fn md5_range(
    file: &mut File,
    path: &Path,
    start: u64,
    mut left: u64,
    check: &mut Check<'_>,
) -> Result<[u8; 16]> {
    file.seek(SeekFrom::Start(start))
        .map_err(|e| Error::io(path, e))?;
    let mut hash = Md5::new();
    let mut buffer = vec![0; CHUNK_BYTES];
    while left != 0 {
        check()?;
        let n = left.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..n])
            .map_err(|e| Error::io(path, e))?;
        hash.update(&buffer[..n]);
        left -= n as u64;
    }
    Ok(hash.finalize().into())
}
