//! Hash-bound, non-mutating source staging and local glTF dependency closure.
use crate::{Result, invalid, path_text, write_json};
use deadlock_project::asset_paths;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
};
pub const SOURCE_SUFFIXES: [&str; 6] = ["glb", "gltf", "fbx", "blend", "pmx", "pmd"];
/// MikuMikuDance models, imported with the MMD Tools add-on in the user's
/// Blender (`mmd_source.py`) and staged as a packed Blender file.
pub const MMD_SUFFIXES: [&str; 2] = ["pmx", "pmd"];
/// Manifest transform of a staged MMD model (`ManifestEntry::transform`).
pub const MMD_TRANSFORM: &str = "mmd-tools-packed-source-v1";
/// The source formats as players read them, for messages.
pub const SOURCE_FORMATS: &str = "Blender, GLB, glTF, FBX or MMD (.pmx, .pmd)";
pub fn suffix(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_lowercase()
}
pub fn normalized_input(value: &str) -> Result<PathBuf> {
    let mut value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value = &value[1..value.len() - 1];
    }
    if value.trim().is_empty() {
        return Err(invalid(format!(
            "No source model selected. Choose a replacement {SOURCE_FORMATS} file in Prepare."
        )));
    }
    if value.contains(['\0', '\n', '\r']) {
        return Err(invalid("Invalid source path: null or line break."));
    }
    let path =
        if value == "~" || value.starts_with("~/") || cfg!(windows) && value.starts_with("~\\") {
            let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .ok_or_else(|| invalid("Cannot resolve the user home directory."))?;
            PathBuf::from(home).join(value.get(2..).unwrap_or_default())
        } else {
            PathBuf::from(value)
        };
    Ok(asset_paths::resolve(&path)?)
}
pub fn resolve_source(value: &str) -> Result<PathBuf> {
    let path = normalized_input(value)?;
    let metadata = fs::metadata(&path).map_err(|e| {
        invalid(format!(
            "Source model cannot be accessed: {e}\nReceived: {value:?}\nResolved: {path:?}"
        ))
    })?;
    if !metadata.is_file() {
        return Err(invalid(
            "Source model must be a regular file, not a directory.",
        ));
    }
    if !SOURCE_SUFFIXES.contains(&suffix(&path).as_str()) {
        return Err(invalid(format!(
            "Unsupported source extension. Select the replacement {SOURCE_FORMATS} file: {}",
            path.display()
        )));
    }
    Ok(path)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackedImage {
    pub origin: String,
    pub sha256: String,
    #[serde(flatten)]
    pub details: std::collections::BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestEntry {
    /// The staged copy, relative to the staging folder.
    pub relative: String,
    pub sha256: String,
    /// The path relative to the model's folder that the model names this
    /// file by, when its copy is staged directly beside the model: under a
    /// shorter path (`shorter_copies`), or for an image the model names under
    /// another extension, which this file stands in for (`stands_in`: the
    /// extensions differ). Absent otherwise (the same as `relative`). The
    /// Blender worker reads it (`staged_images.py`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Where the file is in the model's folder, when that is not `origin`:
    /// always for a stand-in, and for a staged copy staged again (Prepare
    /// again imports the saved workspace copy), whose shorter copies stand
    /// for the paths the model names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packed_images: Option<Vec<PackedImage>>,
    /// The materials this image may be the base color of by its file name
    /// (`texture_names`): materials the model gives no image it can find.
    /// The Blender worker reads it (`staged_images.py`, `material_textures.py`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materials: Option<Vec<String>>,
}
/// Images staging copies and searches for.
pub const IMAGE_SUFFIXES: [&str; 7] = ["png", "jpg", "jpeg", "tga", "tif", "tiff", "bmp"];
/// Files with the same name and another image extension staged for one image
/// the model names, at most; more stage none of them.
const STAND_IN_LIMIT: usize = 8;
/// Images staged for materials by file name, at most, and the largest one.
const NAME_MATCH_LIMIT: usize = 32;
const NAME_MATCH_BYTES: u64 = 256 * 1024 * 1024;
/// Whether `entry` stands for an image the model names under another
/// extension (`origin` and the copy have different extensions).
pub fn stands_in(entry: &ManifestEntry) -> bool {
    entry.origin.as_deref().is_some_and(|origin| other_extension(origin, &entry.relative))
}
fn other_extension(a: &str, b: &str) -> bool {
    suffix(Path::new(a)) != suffix(Path::new(b))
}
pub trait BlendStager {
    fn stage_blend(&self, source: &Path, destination: &Path) -> Result<Vec<PackedImage>>;
    /// Import an MMD model (`MMD_SUFFIXES`) with MMD Tools and save it at
    /// `destination` as a Blender file with its textures packed.
    fn stage_mmd(&self, source: &Path, destination: &Path) -> Result<Vec<PackedImage>>;
}
fn decode_uri(uri: &str) -> Result<String> {
    let bytes = uri.as_bytes();
    let mut output = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                let value = (a * 16 + b) as u8;
                output.push(value);
                i += 3;
                continue;
            }
        }
        output.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(output)
        .map_err(|_| invalid("glTF external dependency URI is not valid UTF-8."))
}
fn dependency(source: &Path, uri: &str) -> Result<PathBuf> {
    let path = dependency_path(source, uri)?;
    if !path.is_file() {
        return Err(missing_dependency(uri));
    }
    Ok(path)
}
fn missing_dependency(uri: &str) -> crate::Error {
    invalid(format!("Missing or escaping glTF dependency: {uri}"))
}
/// The path a local glTF dependency names, inside the model's folder; it may not exist.
fn dependency_path(source: &Path, uri: &str) -> Result<PathBuf> {
    let value = decode_uri(uri)?;
    let relative = Path::new(&value);
    let scheme = uri.split_once(':').is_some_and(|(before, _)| {
        !before.is_empty()
            && before
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ['+', '-', '.'].contains(&c))
    });
    if scheme
        || uri.starts_with("//")
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
        || uri.contains('\\')
        || value.contains('\\')
        || value.contains('\0')
    {
        return Err(invalid(format!(
            "glTF external dependencies must be local paths inside the source folder: {uri}"
        )));
    }
    let parent = source
        .parent()
        .ok_or_else(|| invalid("Source has no parent directory."))?;
    let path = asset_paths::resolve(&parent.join(relative))?;
    if !path.starts_with(parent) {
        return Err(missing_dependency(uri));
    }
    Ok(path)
}
/// Files in `path`'s folder with its stem and another image extension (a
/// model naming `body.tga` beside `body.jpg`), without case, sorted.
fn same_stem_in_folder(path: &Path) -> Vec<PathBuf> {
    let (Some(folder), Some(stem)) = (path.parent(), path.file_stem()) else { return vec![] };
    let stem = stem.to_string_lossy().to_lowercase();
    let wanted = suffix(path);
    let Ok(entries) = list_directory(folder) else { return vec![] };
    let mut found: Vec<PathBuf> = entries
        .into_iter()
        .flatten()
        .filter(|(file, kind)| {
            *kind == Kind::File
                && IMAGE_SUFFIXES.contains(&suffix(file).as_str())
                && suffix(file) != wanted
                && file.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == stem)
        })
        .map(|(file, _)| file)
        .collect();
    found.sort();
    found
}
/// The path the model names an image by when `file` stands in for it: the
/// file's folder relative to the model's and its stem, with the extension
/// asked for (`requested`, a file name).
fn requested_origin(relative: &Path, requested: &str) -> Result<String> {
    let extension = Path::new(requested).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let named = relative.with_extension(extension);
    Ok(path_text(&named)?.replace(std::path::MAIN_SEPARATOR, "/"))
}
fn gltf_data(source: &Path) -> Result<serde_json::Value> {
    if suffix(source) == "gltf" {
        return Ok(serde_json::from_reader(File::open(source)?)?);
    }
    let mut input = File::open(source)?;
    let mut header = [0; 20];
    input
        .read_exact(&mut header)
        .map_err(|_| invalid("Truncated GLB header."))?;
    let word = |i| u32::from_le_bytes(header[i..i + 4].try_into().unwrap());
    let length = word(12);
    if word(0) != 0x46546c67
        || word(4) != 2
        || word(16) != 0x4e4f534a
        || word(8) as u64 != input.metadata()?.len()
        || length > 64 * 1024 * 1024
    {
        return Err(invalid("Unsupported or invalid GLB 2 container."));
    }
    let mut bytes = vec![0; length as usize];
    input.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
/// Folder entries an FBX's texture search looks at, at most: the model's
/// folder can be a Downloads folder or a drive.
const SEARCH_ENTRIES: usize = 200_000;
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Directory,
    File,
    Other,
}
type Listing = std::io::Result<Vec<std::io::Result<(PathBuf, Kind)>>>;
fn list_directory(directory: &Path) -> Listing {
    Ok(fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            let ty = entry.file_type()?;
            // Links are never followed.
            let kind = if ty.is_symlink() {
                Kind::Other
            } else if ty.is_dir() {
                Kind::Directory
            } else if ty.is_file() {
                Kind::File
            } else {
                Kind::Other
            };
            Ok((entry.path(), kind))
        })
        .collect())
}
/// Images below an FBX's folder that `wanted` accepts by file name;
/// `referenced` marks them as files Blender will open (see
/// `stage_source_with_cancel`). The extension list keeps staging to the
/// formats it always copied.
struct TextureSearch<'a> {
    root: &'a Path,
    /// The staging folder, which may lie below `root`: its earlier copies must
    /// not be found and staged again one level deeper each time.
    destination: &'a Path,
    files: &'a mut BTreeMap<PathBuf, bool>,
    referenced: bool,
    wanted: &'a dyn Fn(&str) -> bool,
    list: &'a dyn Fn(&Path) -> Listing,
    examined: usize,
    limit: usize,
    cancel: &'a deadlock_core::CancellationToken,
}
impl TextureSearch<'_> {
    fn walk(&mut self, directory: &Path) -> Result<()> {
        let entries = match (self.list)(directory) {
            Ok(entries) => entries,
            // Blender's image search passes over folders it cannot list; only
            // the FBX's own folder must be readable.
            Err(_) if directory != self.root => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            self.cancel.check()?;
            self.examined += 1;
            if self.examined > self.limit {
                return Err(invalid(format!(
                    "The folder of the model, {}, holds too many files to search for its textures (more than {}). Put the model and its textures in a folder of their own.",
                    self.root.display(),
                    self.limit
                )));
            }
            // An entry removed or refused mid-walk is passed over like a folder.
            let Ok((path, kind)) = entry else { continue };
            match kind {
                Kind::Directory if !self.destination.starts_with(&path) => self.walk(&path)?,
                Kind::File
                    if IMAGE_SUFFIXES.contains(&suffix(&path).as_str())
                        && path
                            .file_name()
                            .is_some_and(|name| (self.wanted)(&name.to_string_lossy())) =>
                {
                    *self.files.entry(path).or_default() |= self.referenced;
                }
                _ => {}
            }
            if self.files.len() > 5000 {
                return Err(invalid(
                    "FBX texture folder too large. Export this character into a dedicated folder.",
                ));
            }
        }
        Ok(())
    }
}
fn collect_textures(
    root: &Path,
    destination: &Path,
    files: &mut BTreeMap<PathBuf, bool>,
    referenced: bool,
    wanted: &dyn Fn(&str) -> bool,
    cancel: &deadlock_core::CancellationToken,
) -> Result<()> {
    TextureSearch {
        root,
        destination,
        files,
        referenced,
        wanted,
        list: &list_directory,
        examined: 0,
        limit: SEARCH_ENTRIES,
        cancel,
    }
    .walk(root)
}
/// Images below `root` that `wanted` accepts, for the optional searches
/// (`stand_ins`, name matches): a folder too large to search gives none
/// instead of stopping the staging, which never needed these images.
fn optional_textures(
    root: &Path,
    destination: &Path,
    wanted: &dyn Fn(&str) -> bool,
    cancel: &deadlock_core::CancellationToken,
) -> Result<Vec<PathBuf>> {
    let mut files = BTreeMap::new();
    match collect_textures(root, destination, &mut files, false, wanted, cancel) {
        Ok(()) => Ok(files.into_keys().collect()),
        Err(crate::Error::Validation(_)) => Ok(vec![]),
        Err(error) => Err(error),
    }
}
/// UTF-16 length of `path` when Blender (which, unlike this app, is not
/// long-path aware) cannot open it: 260 or more, checked with Blender 5.2.
pub fn too_long_for_blender(path: &Path) -> Option<usize> {
    match deadlock_core::safety::require_windows_native_path(path) {
        Err(deadlock_core::CoreError::NativePathTooLong { units, .. }) => Some(units),
        _ => None,
    }
}
/// One file to copy into the staging folder.
struct Staged {
    file: PathBuf,
    /// Its copy, relative to the staging folder.
    relative: PathBuf,
    /// The path the model names it by, when the copy is elsewhere.
    origin: Option<String>,
    /// Where it is in the model's folder, when that is not where the model
    /// names it (a staged copy staged again, `staged_names`).
    found: Option<String>,
    /// Its copy at `relative` would be too long for Blender.
    shorter: bool,
    /// The path the model names an image by that this file stands in for
    /// under another extension (`stands_in`); its copy goes beside the model.
    stand_in: Option<String>,
    /// Materials it may be the base color of by name (`ManifestEntry::materials`).
    materials: Vec<String>,
}
/// The images of a staged copy that is staged again (Prepare again imports
/// the saved workspace copy): its manifest says which file stands for which
/// path the model names (`ManifestEntry::origin`), keyed by the file.
fn staged_names(model: &Path) -> BTreeMap<PathBuf, String> {
    let (Some(parent), Some(filename)) = (model.parent(), model.file_name()) else {
        return BTreeMap::new();
    };
    let Ok(text) = fs::read(parent.join("source-manifest.json")) else {
        return BTreeMap::new();
    };
    let Ok(entries) = serde_json::from_slice::<Vec<ManifestEntry>>(&text) else {
        return BTreeMap::new();
    };
    // Only a manifest that lists this model as its staged model.
    if !entries.iter().any(|e| e.origin.is_none() && Path::new(&e.relative) == Path::new(filename)) {
        return BTreeMap::new();
    }
    let plain = |path: &Path| path.components().all(|c| matches!(c, Component::Normal(_)));
    entries
        .into_iter()
        .filter_map(|entry| {
            let origin = entry.origin?;
            let relative = Path::new(&entry.relative);
            let file = parent.join(relative);
            (relative.components().count() == 1
                && plain(relative)
                && plain(Path::new(&origin))
                && !origin.contains('\\')
                && file.is_file())
            .then_some((file, origin))
        })
        .collect()
}
/// Relative paths compared by their parts ("./tex/a.png" is "tex/a.png"),
/// ignoring case as Windows does: a glTF names its images by URI, a manifest
/// by the file's real path.
fn same_relative(a: &str, b: &str) -> bool {
    let parts = |path: &str| -> Vec<String> {
        path.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").map(str::to_lowercase).collect()
    };
    parts(a) == parts(b)
}
/// Lowercase file name, for comparing names the way Windows and Blender's
/// image search do.
fn folded_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}
/// A short name for the copy of an image Blender could not open at its
/// staged path, keeping its extension: the first 16 hex digits of the
/// SHA-256 of its path relative to the model's folder.
fn short_name(origin: &str, file: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(origin.as_bytes());
    let mut name: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    if let Some(extension) = file.extension() {
        name.push('.');
        name.push_str(&extension.to_string_lossy());
    }
    PathBuf::from(name)
}
/// Images whose copy would be too long for Blender go directly into the
/// staging folder, beside the staged model: under their own name when no
/// other staged file has that name, where Blender's FBX importer finds them
/// by name; otherwise, or when the name alone is too long, under
/// `short_name`, which `staged_images.py` points Blender to. The manifest
/// records where each came from (`ManifestEntry::origin`). A stand-in for an
/// image the model names under another extension is placed the same way, so
/// `staged_images.py` finds it by the name the model asks for. Every other
/// file keeps its place, so a model without such images stages as before.
fn shorter_copies(destination: &Path, staged: &mut [Staged]) -> Result<()> {
    let placed = |s: &Staged| s.shorter || s.stand_in.is_some();
    if !staged.iter().any(placed) {
        return Ok(());
    }
    // By the names Blender's search looks for: the file's own, or the one a
    // staged copy staged again stands for (`staged_names`).
    let mut names = BTreeMap::<String, usize>::new();
    for item in staged.iter() {
        *names.entry(folded_name(&item.relative)).or_default() += 1;
    }
    let mut taken: std::collections::BTreeSet<String> = staged
        .iter()
        .filter(|s| !placed(s))
        .map(|s| s.relative.to_string_lossy().to_lowercase())
        .collect();
    for item in staged.iter_mut().filter(|s| placed(s)) {
        let origin = path_text(&item.relative)?.replace(std::path::MAIN_SEPARATOR, "/");
        let name = folded_name(&item.relative);
        let own = PathBuf::from(item.relative.file_name().unwrap_or_default());
        let relative = if names.get(&name) == Some(&1)
            && !taken.contains(&name)
            && too_long_for_blender(&destination.join(&own)).is_none()
        {
            own
        } else {
            short_name(&origin, &item.relative)
        };
        if let Some(units) = too_long_for_blender(&destination.join(&relative)) {
            return Err(invalid(format!(
                "{} is too long for Blender once copied into the workspace, even under a short name: its path there would be {units} characters, and Blender can only open paths shorter than 260. Choose a shorter Workspace folder in Settings, for example C:\\DMB.",
                item.file.display()
            )));
        }
        if !taken.insert(relative.to_string_lossy().to_lowercase()) {
            return Err(invalid(format!(
                "Two textures of the model would get the same short name in the workspace ({}): {}. Give one of them a different name.",
                relative.display(),
                item.file.display()
            )));
        }
        item.relative = relative;
        item.origin = Some(item.stand_in.clone().unwrap_or(origin));
    }
    Ok(())
}
pub fn stage_source(
    value: &str,
    destination: &Path,
    blend_stager: Option<&dyn BlendStager>,
) -> Result<PathBuf> {
    stage_source_with_cancel(
        value,
        destination,
        blend_stager,
        &deadlock_core::CancellationToken::new(),
    )
}
pub fn stage_source_with_cancel(
    value: &str,
    destination: &Path,
    blend_stager: Option<&dyn BlendStager>,
    cancel: &deadlock_core::CancellationToken,
) -> Result<PathBuf> {
    cancel.check()?;
    let digest = |path: &Path| {
        deadlock_core::safety::digest_with_cancel(path, cancel).map_err(crate::Error::from)
    };
    let source = resolve_source(value)?;
    let destination = asset_paths::resolve(destination)?;
    let filename = source
        .file_name()
        .ok_or_else(|| invalid("Source filename missing."))?;
    let local = destination.join(filename);
    if local == source {
        return Err(invalid(
            "The source and staged copy must be different files.",
        ));
    }
    fs::create_dir_all(&destination)?;
    let mmd = MMD_SUFFIXES.contains(&suffix(&source).as_str());
    if suffix(&source) == "blend" || mmd {
        let bridge = blend_stager.ok_or_else(|| {
            invalid(if mmd {
                "MMD models need the configured Blender with MMD Tools to import them. Use Import and Analyze in Prepare."
            } else {
                "Blender files need the configured Blender importer to pack their textures. Use Import and Analyze in Prepare."
            })
        })?;
        // An MMD model is imported once and staged as the packed Blender file
        // a user would save after importing it with MMD Tools.
        let local = if mmd {
            destination.join(Path::new(filename).with_extension("blend"))
        } else {
            local
        };
        if mmd && cfg!(windows) {
            for (path, what) in [(&source, "The model file"), (&local, "Its Blender copy")] {
                if let Some(units) = too_long_for_blender(path) {
                    return Err(invalid(format!(
                        "{what} {} is too long for Blender: its path is {units} characters, and Blender can only open paths shorter than 260. Move the model's folder to a shorter path, or choose a shorter Workspace folder in Settings, for example C:\\DMB.",
                        path.display()
                    )));
                }
            }
        }
        let before = digest(&source)?;
        let packed_images = if mmd {
            bridge.stage_mmd(&source, &local)?
        } else {
            bridge.stage_blend(&source, &local)?
        };
        if digest(&source)? != before {
            return Err(invalid("Source changed during import."));
        }
        if !local.is_file() {
            return Err(invalid(
                "Blender did not create the packed source copy. See the Blender log.",
            ));
        }
        write_json(
            &destination.join("source-manifest.json"),
            &vec![ManifestEntry {
                relative: path_text(Path::new(filename))?,
                sha256: digest(&local)?,
                origin: None,
                found: None,
                original_sha256: Some(before),
                transform: Some(if mmd { MMD_TRANSFORM } else { "blender-packed-source-v1" }.into()),
                packed_images: Some(packed_images),
                materials: None,
            }],
        )?;
        return Ok(local);
    }
    // Each file to stage, and whether Blender is known to open it.
    let mut files = BTreeMap::from([(source.clone(), true)]);
    // glTF buffers: Blender's importer reads them at their exact path.
    let mut buffers = std::collections::BTreeSet::new();
    // A staged copy staged again: its shorter copies and the paths the model
    // names them by. Empty for a model in a folder of the user's own.
    let carried = staged_names(&source);
    let mut named = BTreeMap::<PathBuf, String>::new();
    let parent = source.parent().unwrap();
    let relative_text = |file: &Path| -> Result<String> {
        let actual = file.strip_prefix(parent).map_err(|_| invalid("Escaping source dependency."))?;
        Ok(path_text(actual)?.replace(std::path::MAIN_SEPARATOR, "/"))
    };
    // Images that stand in for images the model names under another
    // extension: the file, the path the model names, and where the file is
    // when it is a copy staged again.
    let mut stand_ins: Vec<(PathBuf, String)> = Vec::new();
    // Materials the model gives no image it can find, every material name,
    // images below the model's folder that may match them by name, and the
    // file names of the images the model names.
    let mut imageless: Vec<String> = Vec::new();
    let mut material_names: Vec<String> = Vec::new();
    let mut nearby: Vec<PathBuf> = Vec::new();
    let mut named_images: Vec<String> = Vec::new();
    if ["gltf", "glb"].contains(&suffix(&source).as_str()) {
        let data = gltf_data(&source)?;
        for kind in ["buffers", "images"] {
            if let Some(items) = data.get(kind) {
                let items = items
                    .as_array()
                    .ok_or_else(|| invalid("glTF buffers and images must be arrays."))?;
                for item in items {
                    let Some(uri) = item.get("uri") else { continue };
                    let uri = uri
                        .as_str()
                        .ok_or_else(|| invalid("glTF dependency URI must be text."))?;
                    if uri.is_empty() || uri.starts_with("data:") {
                        continue;
                    }
                    let wanted = decode_uri(uri)?;
                    if kind == "images" {
                        named_images.push(wanted.rsplit('/').next().unwrap_or_default().to_owned());
                    }
                    let copy = (kind == "images")
                        .then(|| carried.iter().find(|(_, origin)| same_relative(origin, &wanted)))
                        .flatten();
                    let path = match copy {
                        // A stand-in staged again stays one.
                        Some((file, origin)) if other_extension(origin, &file.to_string_lossy()) => {
                            stand_ins.push((file.clone(), origin.clone()));
                            continue;
                        }
                        Some((file, origin)) => {
                            named.insert(file.clone(), origin.clone());
                            file.clone()
                        }
                        None if kind == "images" => {
                            let path = dependency_path(&source, uri)?;
                            if !path.is_file() {
                                // The image under another extension in its folder.
                                let found = same_stem_in_folder(&path);
                                match found.as_slice() {
                                    [] => return Err(missing_dependency(uri)),
                                    [file] => {
                                        let requested = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                                        let origin = requested_origin(Path::new(&relative_text(file)?), &requested)?;
                                        stand_ins.push((file.clone(), origin));
                                        continue;
                                    }
                                    several => {
                                        let names: Vec<_> = several.iter().map(|f| f.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
                                        return Err(invalid(format!(
                                            "The glTF names the image {uri}, which is not in its folder, and several files there have its name with another extension ({}). Keep only the one to use, or export the model again so that it names it.",
                                            names.join(", ")
                                        )));
                                    }
                                }
                            }
                            path
                        }
                        None => dependency(&source, uri)?,
                    };
                    if kind == "buffers" {
                        buffers.insert(path.clone());
                    }
                    files.insert(path, true);
                }
            }
        }
        // Materials without any texture (`baseColorTexture`, `normalTexture` ...).
        fn textured(value: &serde_json::Value) -> bool {
            match value {
                serde_json::Value::Object(map) => map.iter().any(|(k, v)| (k.ends_with("Texture") && v.is_object()) || textured(v)),
                serde_json::Value::Array(items) => items.iter().any(textured),
                _ => false,
            }
        }
        for material in data.get("materials").and_then(|m| m.as_array()).into_iter().flatten() {
            let Some(name) = material.get("name").and_then(|n| n.as_str()).filter(|n| !n.is_empty()) else { continue };
            material_names.push(name.to_owned());
            if !textured(material) {
                imageless.push(name.to_owned());
            }
        }
        if !imageless.is_empty() {
            let wanted = |name: &str| {
                let stem = Path::new(name).file_stem().unwrap_or_default().to_string_lossy();
                imageless.iter().any(|m| crate::texture_names::Matcher::may_fit(m, &stem))
            };
            nearby = optional_textures(parent, &destination, &wanted, cancel)?;
        }
    }
    if suffix(&source) == "fbx" {
        // Only the images the FBX names (issue #7: a folder of wallpapers next
        // to the model); every image when it cannot be read as a binary FBX.
        let input = File::open(&source)?;
        let length = input.metadata()?.len();
        // `source` and `destination` are both `asset_paths::resolve`d (real
        // case, long names, no `\\?\`), so `starts_with` compares like with like.
        let summary = crate::fbx_images::read(BufReader::new(input), length, cancel)?;
        let names = summary.as_ref().map(|s| s.names.clone());
        match &names {
            Some(names) => collect_textures(
                parent,
                &destination,
                &mut files,
                true,
                &|name| names.contains(&name.to_lowercase()),
                cancel,
            )?,
            None => {
                collect_textures(parent, &destination, &mut files, false, &|_| true, cancel)?
            }
        }
        // Names the model asks for that a staged file answers.
        let mut present: std::collections::BTreeSet<String> = files.keys().map(|f| folded_name(f)).collect();
        for (file, origin) in &carried {
            let name = folded_name(Path::new(origin));
            if names.as_ref().is_none_or(|names| names.contains(&name)) {
                present.insert(name);
                // A stand-in staged again stays one.
                if names.is_some() && other_extension(origin, &file.to_string_lossy()) {
                    stand_ins.push((file.clone(), origin.clone()));
                    continue;
                }
                *files.entry(file.clone()).or_default() |= names.is_some();
                named.insert(file.clone(), origin.clone());
            }
        }
        if let Some(summary) = &summary {
            named_images.extend(summary.paths.values().cloned());
            // Path names with no file of that name: by stem, the names as written.
            let mut missing = BTreeMap::<String, Vec<String>>::new();
            for (lower, written) in &summary.paths {
                if !present.contains(lower) && Path::new(lower).extension().is_some() {
                    let stem = Path::new(lower).file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    missing.entry(stem).or_default().push(written.clone());
                }
            }
            for material in &summary.materials {
                material_names.push(material.name.clone());
            }
            // Materials that may get no image: no texture, or none of its
            // textures' names found yet (a stand-in may still answer one).
            let unanswered: Vec<&crate::fbx_images::Material> = summary
                .materials
                .iter()
                .filter(|m| !m.textures.iter().flatten().any(|name| present.contains(name)))
                .collect();
            if !missing.is_empty() || !unanswered.is_empty() {
                let wanted = |name: &str| {
                    let stem = Path::new(name).file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
                    missing.contains_key(&stem)
                        || unanswered.iter().any(|m| crate::texture_names::Matcher::may_fit(&m.name, &stem))
                };
                nearby = optional_textures(parent, &destination, &wanted, cancel)?;
            }
            // The one file (or the few, left for the user to choose from) with
            // the same stem and another extension.
            for (stem, requested) in &missing {
                let found: Vec<&PathBuf> = nearby
                    .iter()
                    .filter(|f| f.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == *stem))
                    .collect();
                if found.is_empty() || found.len() > STAND_IN_LIMIT {
                    continue;
                }
                for name in requested {
                    for file in &found {
                        stand_ins.push(((*file).clone(), requested_origin(Path::new(&relative_text(file)?), name)?));
                    }
                    if found.len() == 1 {
                        present.insert(name.to_lowercase());
                    }
                }
            }
            imageless = unanswered
                .iter()
                .filter(|m| !m.textures.iter().flatten().any(|name| present.contains(name)))
                .map(|m| m.name.clone())
                .collect();
        }
    }
    // Images for materials the model gives none, matched by file name.
    let mut matches = BTreeMap::<PathBuf, Vec<String>>::new();
    if !imageless.is_empty() {
        let mut pool: std::collections::BTreeSet<PathBuf> = nearby.iter().cloned().collect();
        pool.extend(files.keys().filter(|f| **f != source && !buffers.contains(*f) && IMAGE_SUFFIXES.contains(&suffix(f).as_str())).cloned());
        pool.extend(stand_ins.iter().map(|(f, _)| f.clone()));
        let stem = |f: &Path| f.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let stems: Vec<String> = pool.iter().map(|f| stem(f)).chain(named_images.iter().map(|n| stem(Path::new(n)))).collect();
        let model = stem(&source);
        let matcher = crate::texture_names::Matcher::new(&model, stems.iter().map(String::as_str), material_names.iter().map(String::as_str));
        let mut added = 0;
        for material in &imageless {
            let fits: Vec<&PathBuf> = pool.iter().filter(|f| matcher.fits(material, &stem(f))).collect();
            if fits.len() > STAND_IN_LIMIT {
                continue;
            }
            for file in fits {
                let staged = files.contains_key(file) || stand_ins.iter().any(|(f, _)| f == file);
                if !staged {
                    if added >= NAME_MATCH_LIMIT || fs::metadata(file).map_or(true, |m| m.len() > NAME_MATCH_BYTES) {
                        continue;
                    }
                    added += 1;
                    // Not needed by Blender: left out when its copy would be too long.
                    files.insert(file.clone(), false);
                }
                matches.entry(file.clone()).or_default().push(material.clone());
            }
        }
    }
    let mut staged = Vec::new();
    for (file, referenced) in files {
        cancel.check()?;
        let actual = file
            .strip_prefix(source.parent().unwrap())
            .map_err(|_| invalid("Escaping source dependency."))?
            .to_path_buf();
        // A copy staged again goes where the model names it.
        let (relative, found) = match named.get(&file) {
            Some(origin) => (
                PathBuf::from(origin.replace('/', std::path::MAIN_SEPARATOR_STR)),
                Some(path_text(&actual)?.replace(std::path::MAIN_SEPARATOR, "/")),
            ),
            None => (actual, None),
        };
        let dest = destination.join(&relative);
        // Blender cannot open the copy from 260 characters on: an image it may
        // not need is left out, an image it needs gets a shorter path
        // (`shorter_copies`), and the model itself or a glTF buffer, which
        // Blender reads from exactly that path, stops here.
        let mut shorter = false;
        if let (true, Some(units)) = (cfg!(windows), too_long_for_blender(&dest)) {
            if !referenced {
                continue;
            }
            if file == source {
                return Err(invalid(format!(
                    "The model file {} is too long for Blender once copied into the workspace: its path there would be {units} characters, and Blender can only open paths shorter than 260. Choose a shorter Workspace folder in Settings, for example C:\\DMB, or give the model file a shorter name.",
                    file.display()
                )));
            }
            if buffers.contains(&file) {
                return Err(invalid(format!(
                    "{} is too long for Blender once copied into the workspace: its path there would be {units} characters, and Blender can only open paths shorter than 260. Blender reads this glTF data file from exactly that path, so choose a shorter Workspace folder in Settings, for example C:\\DMB, or export the model again with shorter folder and file names.",
                    file.display()
                )));
            }
            shorter = true;
        }
        let materials = matches.remove(&file).unwrap_or_default();
        staged.push(Staged { file, relative, origin: None, found, shorter, stand_in: None, materials });
    }
    // Stand-ins go beside the model (`shorter_copies`); `found` always says
    // where the file is, since the model names another file.
    for (file, origin) in stand_ins {
        let found = relative_text(&file)?;
        let materials = matches.remove(&file).unwrap_or_default();
        let relative = PathBuf::from(found.replace('/', std::path::MAIN_SEPARATOR_STR));
        staged.push(Staged { file, relative, origin: None, found: Some(found), shorter: false, stand_in: Some(origin), materials });
    }
    shorter_copies(&destination, &mut staged)?;
    let mut manifest = Vec::new();
    for Staged { file, relative, origin, found, materials, .. } in staged {
        cancel.check()?;
        let dest = destination.join(&relative);
        let relative = relative.as_path();
        if asset_paths::resolve(&dest)? == file {
            return Err(invalid(
                "Source and staged dependency must be different files.",
            ));
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let before = digest(&file)?;
        deadlock_core::safety::atomic_copy(&file, &dest, cancel)?;
        if digest(&dest)? != before || digest(&file)? != before {
            return Err(invalid(format!(
                "Source changed during import: {}",
                file.display()
            )));
        }
        manifest.push(ManifestEntry {
            relative: path_text(relative)?.replace(std::path::MAIN_SEPARATOR, "/"),
            sha256: before,
            origin,
            found,
            original_sha256: None,
            transform: None,
            packed_images: None,
            materials: (!materials.is_empty()).then_some(materials),
        });
    }
    write_json(&destination.join("source-manifest.json"), &manifest)?;
    Ok(local)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};
    /// A folder tree in memory: `m/` holds a texture, a folder Windows refuses,
    /// an entry that fails to read and a folder with three more images.
    fn listing(directory: &Path) -> Listing {
        let p = |s: &str| PathBuf::from(s);
        match directory.to_str().unwrap() {
            "m" => Ok(vec![
                Ok((p("m/body.png"), Kind::File)),
                Ok((p("m/locked"), Kind::Directory)),
                Err(Error::new(ErrorKind::NotFound, "removed mid-walk")),
                Ok((p("m/textures"), Kind::Directory)),
                Ok((p("m/link.png"), Kind::Other)),
            ]),
            "m/textures" => Ok(vec![
                Ok((p("m/textures/body.png"), Kind::File)),
                Ok((p("m/textures/face.png"), Kind::File)),
                Ok((p("m/textures/notes.txt"), Kind::File)),
            ]),
            _ => Err(Error::new(ErrorKind::PermissionDenied, "Access is denied. (os error 5)")),
        }
    }
    fn search(root: &str, destination: &str, limit: usize, all: bool) -> Result<Vec<String>> {
        let mut files = BTreeMap::new();
        let wanted = |name: &str| all || name == "body.png";
        TextureSearch {
            root: Path::new(root),
            destination: Path::new(destination),
            files: &mut files,
            referenced: true,
            wanted: &wanted,
            list: &listing,
            examined: 0,
            limit,
            cancel: &deadlock_core::CancellationToken::new(),
        }
        .walk(Path::new(root))?;
        Ok(files.keys().map(|p| p.to_str().unwrap().to_owned()).collect())
    }
    #[test]
    fn texture_search_passes_over_unreadable_folders_and_entries() {
        assert_eq!(search("m", "w", 100, false).unwrap(), ["m/body.png", "m/textures/body.png"]);
        assert_eq!(search("m", "w", 100, true).unwrap(), ["m/body.png", "m/textures/body.png", "m/textures/face.png"]);
        // The model's own folder must be readable.
        assert!(search("m/locked", "w", 100, true).unwrap_err().to_string().contains("os error 5"));
    }
    #[test]
    fn texture_search_skips_the_staging_folder() {
        assert_eq!(search("m", "m/textures/work/source", 100, true).unwrap(), ["m/body.png"]);
    }
    #[test]
    fn texture_search_stops_past_its_entry_limit() {
        // Eight entries in all; the failed entry counts too.
        assert!(search("m", "w", 8, true).is_ok());
        let error = search("m", "w", 7, true).unwrap_err().to_string();
        assert!(error.contains("The folder of the model, m, holds too many files to search for its textures (more than 7)"), "{error}");
        // The kind `optional_textures` passes over: the optional searches
        // (stand-ins, name matches) never stop a staging that worked before.
        assert!(matches!(search("m", "w", 7, true), Err(crate::Error::Validation(_))));
    }
    #[test]
    fn a_stand_in_names_the_extension_asked_for_in_its_own_folder() {
        assert_eq!(requested_origin(Path::new("tex/Body.jpg"), "body.TGA").unwrap(), "tex/Body.TGA");
        assert_eq!(requested_origin(Path::new("Face.v2.png"), "Face.v2.psd").unwrap(), "Face.v2.psd");
        let entry = |origin: Option<&str>, relative: &str| ManifestEntry {
            relative: relative.into(),
            sha256: String::new(),
            origin: origin.map(Into::into),
            found: None,
            original_sha256: None,
            transform: None,
            packed_images: None,
            materials: None,
        };
        assert!(stands_in(&entry(Some("Body.tga"), "Body.jpg")));
        // A shorter copy keeps its extension; an ordinary entry has no origin.
        assert!(!stands_in(&entry(Some("tex/Body.png"), "0123456789abcdef.png")));
        assert!(!stands_in(&entry(None, "Body.jpg")));
    }
}
