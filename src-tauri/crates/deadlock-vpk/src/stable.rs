//! Safe standard-library file access plus same-file's platform file identity.
use crate::{Check, Error, Result, CHUNK_BYTES};
use same_file::Handle;
use sha2::{Digest, Sha256};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    changed: (i64, i64),
}
impl Stamp {
    fn of(path: &Path, m: &Metadata) -> Result<Self> {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            len: m.len(),
            modified: m.modified().map_err(|e| Error::io(path, e))?,
            created: m.created().ok(),
            #[cfg(unix)]
            device: m.dev(),
            #[cfg(unix)]
            inode: m.ino(),
            #[cfg(unix)]
            changed: (m.ctime(), m.ctime_nsec()),
        })
    }
}

#[derive(Debug)]
pub(crate) struct Baseline {
    identity: Handle,
    stamp: Stamp,
}
impl Baseline {
    pub(crate) fn size(&self) -> u64 {
        self.stamp.len
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.identity == other.identity && self.stamp == other.stamp
    }
    pub(crate) fn capture(path: &Path) -> Result<Self> {
        let file = open_read(path)?;
        let result = Self::from_file(path, &file)?;
        result.check_file(path, &file)?;
        result.check_path(path)?;
        Ok(result)
    }
    pub(crate) fn from_file(path: &Path, file: &File) -> Result<Self> {
        let metadata = file.metadata().map_err(|e| Error::io(path, e))?;
        if !metadata.is_file() {
            return Err(Error::invalid(format!(
                "Package input must be an ordinary file: {}",
                path.display()
            )));
        }
        let identity = Handle::from_file(file.try_clone().map_err(|e| Error::io(path, e))?)
            .map_err(|e| Error::io(path, e))?;
        Ok(Self {
            identity,
            stamp: Stamp::of(path, &metadata)?,
        })
    }
    pub(crate) fn check_file(&self, path: &Path, file: &File) -> Result<()> {
        let current = Self::from_file(path, file)?;
        if self.identity != current.identity || self.stamp != current.stamp {
            return Err(Error::Unstable(format!(
                "{} (file identity, size or timestamps)",
                path.display()
            )));
        }
        Ok(())
    }
    pub(crate) fn check_path(&self, path: &Path) -> Result<()> {
        let current = open_read(path)?;
        self.check_file(path, &current)
    }
}

/// Inspect every unresolved component. Never canonicalize before rejecting links.
/// Windows unknown reparse tags fail closed (including cloud placeholders).
pub fn check_unlinked_path(path: &Path, missing_ok: bool) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| Error::io(path, e))?
            .join(path)
    };
    let components: Vec<_> = absolute.components().collect();
    // A bare prefix such as `\\?\C:` names no directory: Windows treats verbatim
    // prefixes as absolute, so require the root separator explicitly.
    let bare_prefix = matches!(components.first(), Some(Component::Prefix(_)))
        && !matches!(components.get(1), Some(Component::RootDir));
    if !absolute.is_absolute() || bare_prefix {
        return Err(Error::invalid(
            "Choose a complete absolute filesystem path, including the drive root.",
        ));
    }
    let mut current = PathBuf::new();
    for (i, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        // A Windows prefix is not a directory. In particular, querying
        // `\\?\C:` fails with ERROR_INVALID_FUNCTION. Inspect it only after
        // RootDir has completed `\\?\C:\`; every actual ancestor still gets
        // the same link/reparse checks below.
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err(Error::invalid(format!(
                            "Linked or unqualified reparse package path: {}",
                            current.display()
                        )));
                    }
                }
                if metadata.file_type().is_symlink() {
                    return Err(Error::invalid(format!(
                        "Linked package input is not supported: {}",
                        current.display()
                    )));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && missing_ok => {
                if components[i + 1..]
                    .iter()
                    .any(|c| *c == Component::ParentDir)
                {
                    return Err(Error::invalid(format!(
                        "Package path has a missing prefix before parent traversal: {}",
                        path.display()
                    )));
                }
                return Ok(());
            }
            Err(e) => return Err(Error::io(&current, e)),
        }
    }
    Ok(())
}

pub(crate) fn resolve_new(path: &Path) -> Result<PathBuf> {
    check_unlinked_path(path, true)?;
    let mut existing = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| Error::io(path, e))?
            .join(path)
    };
    let mut suffix = Vec::new();
    while !existing.try_exists().map_err(|e| Error::io(&existing, e))? {
        suffix.push(
            existing
                .file_name()
                .ok_or_else(|| Error::invalid("Invalid output path."))?
                .to_owned(),
        );
        if !existing.pop() {
            return Err(Error::invalid("Output path has no existing ancestor."));
        }
    }
    let mut resolved = existing
        .canonicalize()
        .map_err(|e| Error::io(&existing, e))?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

pub(crate) fn open_read(path: &Path) -> Result<File> {
    check_unlinked_path(path, false)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Denies simultaneous writes/deletes, including already-open writers.
        options.share_mode(1);
    }
    let file = options.open(path).map_err(|e| Error::io(path, e))?;
    if !file.metadata().map_err(|e| Error::io(path, e))?.is_file() {
        return Err(Error::invalid(format!(
            "Package input must be an ordinary file: {}",
            path.display()
        )));
    }
    Ok(file)
}

pub(crate) fn hash_copy(
    path: &Path,
    baseline: &Baseline,
    mut output: Option<&mut File>,
    check: &mut Check<'_>,
) -> Result<String> {
    baseline.check_path(path)?;
    let mut source = open_read(path)?;
    baseline.check_file(path, &source)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; CHUNK_BYTES];
    let mut count = 0u64;
    loop {
        check()?;
        let available = (baseline.size() - count)
            .saturating_add(1)
            .min(CHUNK_BYTES as u64) as usize;
        let n = source
            .read(&mut buffer[..available])
            .map_err(|e| Error::io(path, e))?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > baseline.size() {
            return Err(Error::Unstable(format!(
                "{} grew during read",
                path.display()
            )));
        }
        hash.update(&buffer[..n]);
        if let Some(dest) = output.as_mut() {
            dest.write_all(&buffer[..n])
                .map_err(|e| Error::io(path, e))?;
        }
    }
    if count != baseline.size() {
        return Err(Error::Unstable(format!(
            "{} size changed during read",
            path.display()
        )));
    }
    baseline.check_file(path, &source)?;
    baseline.check_path(path)?;
    if let Some(dest) = output.as_mut() {
        dest.sync_all().map_err(|e| Error::io(path, e))?;
    }
    Ok(crate::hex(hash.finalize()))
}
