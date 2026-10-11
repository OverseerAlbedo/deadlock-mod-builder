use crate::{CancellationToken, CoreError, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use tempfile::NamedTempFile;

pub fn relative_asset(value: &str) -> Result<String> {
    let normalized = value.replace('\\', "/");
    if normalized.is_empty()
        || normalized
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        || normalized.chars().any(|c| ":\0\r\n*?\"<>|".contains(c))
    {
        return Err(CoreError::UnsafePath(value.into()));
    }
    // Windows aliases are not legitimate asset names and can bypass file checks.
    if normalized.split('/').any(|part| {
        let base = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        part.ends_with([' ', '.'])
            || matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && matches!(base.as_bytes()[3], b'1'..=b'9'))
    }) {
        return Err(CoreError::UnsafePath(value.into()));
    }
    Ok(normalized)
}

/// Resolve existing ancestors and all symlinks; retain missing trailing components.
/// This check assumes the app owns the workspace. It is not a race-proof sandbox
/// against another process changing symlinks between this call and a file open.
fn resolved(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| CoreError::io("Read working directory", e))?
            .join(path)
    };
    let mut clean = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other.as_os_str()),
        }
    }
    let mut ancestor = clean.as_path();
    let mut suffix = vec![];
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                let mut result = ancestor
                    .canonicalize()
                    .map_err(|e| CoreError::io(format!("Resolve {}", ancestor.display()), e))?;
                for part in suffix.into_iter().rev() {
                    result.push(part);
                }
                return Ok(result);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| CoreError::UnsafePath(path.display().to_string()))?
                        .to_os_string(),
                );
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| CoreError::UnsafePath(path.display().to_string()))?;
            }
            Err(e) => return Err(CoreError::io(format!("Inspect {}", ancestor.display()), e)),
        }
    }
}

pub fn resolve_path(path: &Path) -> Result<PathBuf> {
    resolved(path)
}

pub fn inside(root: &Path, relative: &str) -> Result<PathBuf> {
    let root = resolved(root)?;
    let path = resolved(&root.join(relative_asset(relative)?))?;
    if !path.starts_with(root) {
        return Err(CoreError::PathEscape(path));
    }
    Ok(path)
}

pub fn require_windows_native_path(path: &Path) -> Result<()> {
    let units = path.as_os_str().to_string_lossy().encode_utf16().count();
    if units >= 260 {
        Err(CoreError::NativePathTooLong {
            path: path.to_path_buf(),
            units,
        })
    } else {
        Ok(())
    }
}

pub fn require_native_path(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        require_windows_native_path(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}

pub fn digest(path: &Path) -> Result<String> {
    digest_with_cancel(path, &CancellationToken::new())
}

pub fn digest_with_cancel(path: &Path, cancel: &CancellationToken) -> Result<String> {
    let mut source =
        File::open(path).map_err(|e| CoreError::io(format!("Open {}", path.display()), e))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        cancel.check()?;
        let count = source
            .read(&mut buffer)
            .map_err(|e| CoreError::io("Read file for SHA-256", e))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn temporary_for(path: &Path) -> Result<NamedTempFile> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|e| CoreError::io(format!("Create {}", parent.display()), e))?;
    tempfile::Builder::new()
        .prefix(".builder-")
        .tempfile_in(parent)
        .map_err(|e| CoreError::io("Create staging file", e))
}

fn publish(mut temporary: NamedTempFile, path: &Path) -> Result<()> {
    temporary
        .flush()
        .map_err(|e| CoreError::io("Flush staging file", e))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| CoreError::io("Sync staging file", e))?;
    temporary
        .persist(path)
        .map_err(|e| CoreError::io(format!("Publish {}", path.display()), e.error))?;
    Ok(())
}

pub fn atomic_write(path: &Path, data: impl AsRef<[u8]>) -> Result<()> {
    let mut temporary = temporary_for(path)?;
    temporary
        .write_all(data.as_ref())
        .map_err(|e| CoreError::io("Write staging file", e))?;
    publish(temporary, path)
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut data = serde_json::to_vec_pretty(value)?;
    data.push(b'\n');
    atomic_write(path, data)
}

pub fn atomic_copy(source: &Path, target: &Path, cancel: &CancellationToken) -> Result<()> {
    cancel.check()?;
    let mut input =
        File::open(source).map_err(|e| CoreError::io(format!("Open {}", source.display()), e))?;
    let mut temporary = temporary_for(target)?;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        cancel.check()?;
        let count = input
            .read(&mut buffer)
            .map_err(|e| CoreError::io("Read source archive", e))?;
        if count == 0 {
            break;
        }
        temporary
            .write_all(&buffer[..count])
            .map_err(|e| CoreError::io("Copy staged archive", e))?;
    }
    temporary
        .flush()
        .map_err(|e| CoreError::io("Flush staged archive", e))?;
    if digest_with_cancel(temporary.path(), cancel)? != digest_with_cancel(source, cancel)? {
        return Err(CoreError::Integrity(
            "Staged copy does not match the source archive".into(),
        ));
    }
    cancel.check()?;
    publish(temporary, target)
}

pub fn copy_tree(source: &Path, target: &Path, cancel: &CancellationToken) -> Result<()> {
    let source = resolved(source)?;
    let target = resolved(target)?;
    if target.starts_with(&source) {
        return Err(CoreError::UnsafePath(
            "Output workspace must be outside imported source folder".into(),
        ));
    }
    fn walk(
        source: &Path,
        directory: &Path,
        target: &Path,
        cancel: &CancellationToken,
    ) -> Result<()> {
        for entry in fs::read_dir(directory)
            .map_err(|e| CoreError::io(format!("Read {}", directory.display()), e))?
        {
            cancel.check()?;
            let entry = entry.map_err(|e| CoreError::io("Read source entry", e))?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|e| CoreError::io("Inspect source entry", e))?;
            if kind.is_symlink() || !resolved(&path)?.starts_with(source) {
                return Err(CoreError::PathEscape(path));
            }
            if kind.is_dir() {
                walk(source, &path, target, cancel)?;
            } else if kind.is_file() {
                let relative = path
                    .strip_prefix(source)
                    .map_err(|_| CoreError::PathEscape(path.clone()))?;
                let text = relative
                    .to_str()
                    .ok_or_else(|| CoreError::UnsafePath("Non-Unicode asset filename".into()))?;
                let destination = inside(target, text)?;
                atomic_copy(&path, &destination, cancel)?;
            } else {
                return Err(CoreError::Unsupported(format!(
                    "Special file in imported project: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }
    walk(&source, &source, &target, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_paths_reject_escape_and_windows_aliases() {
        assert_eq!(
            relative_asset("models\\heroes\\hornet.vmdl").unwrap(),
            "models/heroes/hornet.vmdl"
        );
        for path in [
            "",
            "/x",
            "a//b",
            "../x",
            "a/../b",
            "./x",
            "C:\\x",
            "a\n",
            "a*",
            "a/CON.txt",
            "NUL",
            "x.",
            "x ",
            "LPT9",
        ] {
            assert!(relative_asset(path).is_err(), "{path}");
        }
    }
    #[test]
    fn windows_native_budget_counts_utf16() {
        assert!(require_windows_native_path(Path::new(&"a".repeat(259))).is_ok());
        assert!(require_windows_native_path(Path::new(&"a".repeat(260))).is_err());
        assert!(require_windows_native_path(Path::new(&"😀".repeat(130))).is_err());
    }
    #[test]
    fn atomic_copy_preserves_target_when_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, b"new bytes").unwrap();
        fs::write(&target, b"old bytes").unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(matches!(
            atomic_copy(&source, &target, &cancel),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(fs::read(&target).unwrap(), b"old bytes");
        atomic_copy(&source, &target, &CancellationToken::new()).unwrap();
        assert_eq!(digest(&source).unwrap(), digest(&target).unwrap());
    }
    #[cfg(unix)]
    #[test]
    fn existing_symlink_and_dangling_symlink_cannot_escape() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), dir.path().join("escape")).unwrap();
        assert!(matches!(
            inside(dir.path(), "escape/new.vmdl"),
            Err(CoreError::PathEscape(_))
        ));
        symlink(dir.path().join("missing"), dir.path().join("dangling")).unwrap();
        assert!(inside(dir.path(), "dangling/file").is_err());
        assert!(inside(dir.path(), "new/valid.vmdl").is_ok());
    }
    #[test]
    fn copying_source_into_itself_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            copy_tree(
                dir.path(),
                &dir.path().join("nested"),
                &CancellationToken::new()
            )
            .is_err()
        );
    }
}
