//! Blender versions the bridge supports, and the choice among installed copies.
use std::path::{Path, PathBuf};

/// Validated Blender minors; newer minors need bridge validation first.
pub const SUPPORTED_DESCRIPTION: &str = "Blender 4.2–4.5, 5.0, 5.2 LTS or 5.3";
pub fn supported(version: [u32; 3]) -> bool {
    matches!((version[0], version[1]), (4, 2..=5) | (5, 0 | 2 | 3))
}

/// The version in an install folder name such as `Blender 5.2` or `Blender 4.5.3`.
pub fn folder_version(name: &str) -> Option<[u32; 3]> {
    let text = name.strip_prefix("Blender")?.trim();
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = match parts.next() {
        Some(part) => part.parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some([major, minor, patch])
}

/// Picks among installed `blender.exe` paths by their folder's version number:
/// the highest supported version first; without one, the highest version; folders
/// without a version in their name last, by name (the former behavior).
pub fn choose_install(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().max_by(|a, b| rank(a).cmp(&rank(b)))
}

fn rank(executable: &Path) -> (bool, Option<[u32; 3]>, PathBuf) {
    let version = executable
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .and_then(folder_version);
    (version.is_some_and(supported), version, executable.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(folder: &str) -> PathBuf {
        PathBuf::from("C:/Program Files/Blender Foundation")
            .join(folder)
            .join("blender.exe")
    }

    fn chosen(folders: &[&str]) -> PathBuf {
        choose_install(folders.iter().map(|f| exe(f))).unwrap()
    }

    #[test]
    fn folder_names_give_versions() {
        assert_eq!(folder_version("Blender 5.2"), Some([5, 2, 0]));
        assert_eq!(folder_version("Blender 4.5.3"), Some([4, 5, 3]));
        assert_eq!(folder_version("Blender 5.10"), Some([5, 10, 0]));
        assert_eq!(folder_version("Blender"), None);
        assert_eq!(folder_version("Blender 5.2 LTS"), None);
        assert_eq!(folder_version("Other 5.2"), None);
    }

    #[test]
    fn supported_minors() {
        for v in [[4, 2, 0], [4, 5, 3], [5, 0, 1], [5, 2, 2], [5, 3, 0]] {
            assert!(supported(v), "{v:?}");
        }
        for v in [[4, 1, 9], [5, 1, 0], [5, 4, 0], [6, 0, 0]] {
            assert!(!supported(v), "{v:?}");
        }
    }

    #[test]
    fn newest_supported_install_is_chosen_by_number() {
        assert_eq!(chosen(&["Blender 5.2", "Blender 5.3"]), exe("Blender 5.3"));
        assert_eq!(chosen(&["Blender 5.3", "Blender 5.2"]), exe("Blender 5.3"));
        // An unsupported newer minor is not preferred over a supported one.
        assert_eq!(chosen(&["Blender 5.3", "Blender 5.4"]), exe("Blender 5.3"));
        assert_eq!(chosen(&["Blender 5.10", "Blender 5.2"]), exe("Blender 5.2"));
    }

    #[test]
    fn version_number_orders_unsupported_installs() {
        // Name order would put 5.2 above 5.10.
        assert_eq!(chosen(&["Blender 5.10", "Blender 5.2", "Blender 4.1"]), exe("Blender 5.2"));
        assert_eq!(chosen(&["Blender 5.10", "Blender 5.9"]), exe("Blender 5.10"));
        assert_eq!(chosen(&["Blender 5.4"]), exe("Blender 5.4"));
        // Folders without a version stay below versioned ones, then by name.
        assert_eq!(chosen(&["Blender custom", "Blender 4.1"]), exe("Blender 4.1"));
        assert_eq!(chosen(&["b", "a"]), exe("b"));
        assert!(choose_install(Vec::new()).is_none());
    }
}
