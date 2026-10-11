//! Profile paths are filesystem paths. Game paths are validated separately.
use crate::{Error, Result};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

pub const PATH_FIELDS: &[&str] = &[
    "preparation_source",
    "preparation_blend",
    "preparation_report",
    "dmx",
    "textures",
    "source_root",
    "imported_source",
    "original_compiled",
    "source_overlay",
    "mesh_template",
    "material_mapping",
    "material_remap",
    "skeleton_reference",
];
pub const PACKAGE_FIELDS: &[&str] = &[
    "source_overlay",
    "mesh_template",
    "material_mapping",
    "material_remap",
    "skeleton_reference",
];
pub fn relative_asset(value: &str) -> Result<String> {
    let normalized = value.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized
            .split('/')
            .any(|x| x.is_empty() || x == "." || x == "..")
    {
        return Err(Error::Validation(format!(
            "Unsafe game-relative asset path: {value:?}"
        )));
    }
    if normalized.chars().any(|c| ":\0\r\n*?\"<>|".contains(c)) {
        return Err(Error::Validation(format!("Invalid asset path: {value:?}")));
    }
    Ok(normalized)
}
pub fn windows_drive(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        || value.starts_with("\\\\")
        || value.starts_with("//")
}
pub fn is_absolute(value: &str) -> bool {
    Path::new(value).is_absolute()
        || value.starts_with("\\\\")
        || (windows_drive(value)
            && value
                .as_bytes()
                .get(2)
                .is_some_and(|v| *v == b'/' || *v == b'\\'))
}
pub fn text(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(|| Error::Validation("This project format requires a Unicode filesystem path; the path cannot be encoded without loss.".into()))
}
/// Resolve existing ancestors/symlinks, preserving missing leaves like Path.resolve(strict=False).
/// This also canonicalizes short Windows names on Windows through std::fs::canonicalize.
pub fn resolve(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    // Resolve components in order: resolving `link/..` must use the link's actual target.
    let mut result = PathBuf::new();
    for component in absolute.components() {
        match component {
            // Canonicalizing a bare `C:` uses that drive's current directory,
            // while `\\?\C:` is invalid. Wait for the root separator before
            // resolving the volume, including extended drive and UNC paths.
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => {
                result.push(other.as_os_str());
                match std::fs::canonicalize(&result) {
                    Ok(actual) => result = actual,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                        ) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    // canonicalize returns extended Windows paths; persist the same ordinary
    // drive/UNC spelling Path.resolve uses for normal input.
    #[cfg(windows)]
    {
        if let Some(s) = result.to_str() {
            if let Some(tail) = s.strip_prefix(r"\\?\UNC\") {
                return Ok(PathBuf::from(format!(r"\\{tail}")));
            }
            if let Some(tail) = s.strip_prefix(r"\\?\") {
                return Ok(PathBuf::from(tail));
            }
        }
    }
    Ok(result)
}
fn lexical_relative(value: &str, root: &str, windows: bool) -> Option<String> {
    let value = if windows {
        value.replace('\\', "/")
    } else {
        value.to_owned()
    };
    let root = if windows {
        root.replace('\\', "/")
    } else {
        root.to_owned()
    };
    let parts = |s: &str| {
        s.split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let v = parts(&value);
    let r = parts(&root);
    if v.len() < r.len() || (value.starts_with('/') != root.starts_with('/')) {
        return None;
    }
    if v.iter().zip(&r).any(|(a, b)| {
        if windows {
            a.to_lowercase() != b.to_lowercase()
        } else {
            a != b
        }
    }) {
        return None;
    }
    Some(if v.len() == r.len() {
        ".".into()
    } else {
        v[r.len()..].join("/")
    })
}
pub fn relative_to(value: &str, root: &str) -> Result<Option<String>> {
    if value.is_empty() || root.is_empty() {
        return Ok(None);
    }
    let foreign_windows = !cfg!(windows) && (windows_drive(value) || windows_drive(root));
    if is_absolute(value) && is_absolute(root) && !foreign_windows {
        let value = resolve(Path::new(value))?;
        let root = resolve(Path::new(root))?;
        if cfg!(windows) {
            return Ok(lexical_relative(&text(&value)?, &text(&root)?, true));
        }
        return value
            .strip_prefix(root)
            .ok()
            .map(|p| text(p).map(|s| if s.is_empty() { ".".into() } else { s }))
            .transpose();
    }
    Ok(lexical_relative(value, root, windows_drive(root)))
}
pub fn encode(value: &str, profile_dir: &Path) -> Result<String> {
    if value.is_empty() || !is_absolute(value) || (!cfg!(windows) && windows_drive(value)) {
        return Ok(value.to_owned());
    }
    let value = resolve(Path::new(value))?;
    let root = resolve(profile_dir)?;
    let v: Vec<_> = value.components().collect();
    let r: Vec<_> = root.components().collect();
    let equivalent = |a: &Component<'_>, b: &Component<'_>| {
        if cfg!(windows) {
            a.as_os_str()
                .to_str()
                .zip(b.as_os_str().to_str())
                .is_some_and(|(a, b)| a.to_lowercase() == b.to_lowercase())
        } else {
            a == b
        }
    };
    if v.first()
        .zip(r.first())
        .is_none_or(|(a, b)| !equivalent(a, b))
    {
        return text(&value);
    } // different Windows drive
    let common = v
        .iter()
        .zip(&r)
        .take_while(|(a, b)| equivalent(a, b))
        .count();
    let mut result = PathBuf::new();
    for _ in common..r.len() {
        result.push("..");
    }
    for part in &v[common..] {
        result.push(part.as_os_str());
    }
    if result.as_os_str().is_empty() {
        return Ok(".".into());
    }
    let result = text(&result)?;
    Ok(if cfg!(windows) {
        result.replace('\\', "/")
    } else {
        result
    })
}
pub fn decode(value: &str, profile_dir: &Path) -> Result<String> {
    if value.is_empty() || is_absolute(value) {
        return Ok(value.to_owned());
    }
    text(&resolve(&profile_dir.join(value))?)
}
pub fn under(root: &str, suffix: &str) -> Result<String> {
    if root.is_empty()
        || suffix.is_empty()
        || is_absolute(suffix)
        || suffix.replace('\\', "/").split('/').any(|p| p == "..")
    {
        return Err(Error::Validation(
            "Invalid package-relative asset path.".into(),
        ));
    }
    text(
        &Path::new(root)
            .join(suffix)
            .components()
            .collect::<PathBuf>(),
    )
}
/// Legacy recovery requires a matching directory anchor and one unique existing
/// suffix. A basename match alone never authorizes substituting an input.
pub fn recover(value: &str, roots: &[&str]) -> Result<Option<String>> {
    let normalized = value.replace('\\', "/");
    let parts: Vec<_> = normalized.split('/').collect();
    let mut candidates = BTreeSet::new();
    for root in roots.iter().filter(|r| !r.is_empty()) {
        let root_path = Path::new(root);
        let anchor = deadlock_core::text::casefold(
            root_path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
        );
        for (i, part) in parts.iter().enumerate() {
            if deadlock_core::text::casefold(part) == anchor {
                let mut candidate = root_path.to_path_buf();
                for part in &parts[i + 1..] {
                    candidate.push(part);
                }
                if candidate.exists() {
                    candidates.insert(text(&candidate)?);
                }
            }
        }
    }
    Ok(if candidates.len() == 1 {
        candidates.into_iter().next()
    } else {
        None
    })
}
