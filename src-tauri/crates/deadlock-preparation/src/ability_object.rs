//! Preparing the user's file for an ability object: its own operation, before
//! and apart from the build (which needs no Blender).
//!
//! The file is staged by the source staging every model uses (`source.rs`:
//! manifests, long paths, shorter image copies and stand-ins), then Blender
//! runs `ability_object.py` as its own script (never `worker.py`): the file's
//! visible meshes, evaluated and triangulated, become one packet in the file's
//! own coordinates and units (`unit_metres`) without a skeleton, with each
//! material's colour image and basic values as `material_textures.py` finds
//! them, and a lighter preview when the object is large. The fit and the game
//! object's axis system are applied later, in Rust, when the build writes the
//! DMX, so changing the fit never needs Blender again.
//!
//! Everything lives in one folder per object under the project's workspace:
//! `source/` (the staged copy), `textures/` (the material images), `packet.json`
//! and `prepared.json` (the stamp). A preparation is reused while the source
//! file's stamp (path, size, modification time, digest) and the scripts'
//! digest match; the build checks the source's digest and the packet's.
use crate::{blender::BlenderBridge, invalid, path_text, source, write_json, Result};
use deadlock_core::{safety, CancellationToken};
use deadlock_project::MaterialChoices;
use deadlock_skeleton::export::PreparedMesh;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// The packet's version; equals `VERSION` in `ability_object.py`.
pub const PACKET_VERSION: &str = "ability-object-packet-v2";
/// Packets of the first version still build (each mesh object was already one
/// named mesh; they hold no armature or weights).
pub const OLD_PACKET_VERSIONS: [&str; 1] = ["ability-object-packet-v1"];
/// A packet or stamp version this build reads.
pub fn readable_version(version: &str) -> bool {
    version == PACKET_VERSION || OLD_PACKET_VERSIONS.contains(&version)
}
/// A bone of the file's armature (packet version 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmatureBone {
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
}
pub const SCRIPT: &str = "ability_object.py";
/// Messages the page and troubleshooting compare.
pub const NOT_PREPARED: &str = "This ability object's file is not prepared, or changed since it was prepared. Prepare it again in Extras → Objects";
pub const UNSUPPORTED_FILE: &str = "An ability object comes from a Blender, FBX, GLB or glTF file";

/// The scripts a packet depends on (the entry and what it imports).
fn scripts_digest() -> String {
    let mut digest = Sha256::new();
    for bytes in [
        include_bytes!("../resources/blender/ability_object.py").as_slice(),
        include_bytes!("../resources/blender/material_textures.py"),
        include_bytes!("../resources/blender/mesh_export.py"),
        include_bytes!("../resources/blender/staged_images.py"),
        include_bytes!("../resources/blender/readable_names.py"),
    ] {
        digest.update(Sha256::digest(bytes));
    }
    format!("{:x}", digest.finalize())
}
/// The prepared object as Blender wrote it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub version: String,
    /// Metres per file unit (the file's scene unit scale).
    pub unit_metres: f64,
    pub meshes: Vec<PreparedMesh>,
    /// Material path (under the object's folder) → choices (`material_textures.py`).
    pub materials: BTreeMap<String, MaterialChoices>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// A lighter copy for the fit view (only when the object is large).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preview: Vec<PreparedMesh>,
    /// The bones of the armatures deforming the file's meshes (rest pose used), when
    /// there are any; each mesh then carries its points' weights on them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub armature: Vec<ArmatureBone>,
}
impl Packet {
    pub fn points(&self) -> impl Iterator<Item = [f64; 3]> + '_ {
        self.meshes.iter().flat_map(|m| m.positions.iter().copied())
    }
    /// The meshes the fit view draws.
    pub fn preview_meshes(&self) -> &[PreparedMesh] {
        if self.preview.is_empty() { &self.meshes } else { &self.preview }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub meshes: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub materials: usize,
}
/// The stamp of a preparation (`prepared.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prepared {
    pub version: String,
    pub source: String,
    pub source_size: u64,
    pub source_modified: u128,
    pub source_sha256: String,
    pub scripts_sha256: String,
    /// The object's folder in the game, the materials' prefix.
    pub root: String,
    pub packet_sha256: String,
    pub counts: Counts,
    pub warnings: Vec<String>,
    /// Bounds of the packet's points (file units).
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub unit_metres: f64,
    /// Material path → its colour image, when one was found.
    pub color_images: BTreeMap<String, String>,
}
fn stamp(path: &Path) -> Result<(u64, u128)> {
    let meta = fs::metadata(path)?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    Ok((meta.len(), modified))
}
/// Whether `source` is a file type an ability object may come from.
pub fn check_source(source: &Path) -> Result<()> {
    let suffix = source::suffix(source);
    if !deadlock_project::ability_objects::SOURCE_SUFFIXES.contains(&suffix.as_str()) {
        return Err(invalid(format!("{UNSUPPORTED_FILE}: {}", source.display())));
    }
    Ok(())
}
/// The kept preparation in `folder` for `source` and `root`, when it is still current
/// (same file stamp and digest, same scripts, packet present with its digest).
pub fn current(folder: &Path, source: &Path, root: &str, cancel: &CancellationToken) -> Option<Prepared> {
    let saved: Prepared = serde_json::from_slice(&fs::read(folder.join("prepared.json")).ok()?).ok()?;
    let source = &source::resolve_source(&path_text(source).ok()?).ok()?;
    let (size, modified) = stamp(source).ok()?;
    let packet = folder.join("packet.json");
    (saved.version == PACKET_VERSION
        && saved.source == path_text(source).ok()?
        && saved.root == root
        && saved.source_size == size
        && saved.source_modified == modified
        && saved.scripts_sha256 == scripts_digest()
        && safety::digest_with_cancel(&packet, cancel).ok()? == saved.packet_sha256)
        .then_some(saved)
}
/// Prepares `source` for the object whose own files live under `root` (a game
/// folder) into `folder`, or returns the kept preparation when it is current.
pub fn prepare(bridge: &BlenderBridge, source: &Path, folder: &Path, root: &str, cancel: &CancellationToken) -> Result<Prepared> {
    cancel.check()?;
    check_source(source)?;
    let source = source::resolve_source(&path_text(source)?)?;
    if let Some(kept) = current(folder, &source, root, cancel) {
        return Ok(kept);
    }
    let before = safety::digest_with_cancel(&source, cancel)?;
    let (size, modified) = stamp(&source)?;
    fs::create_dir_all(folder)?;
    for old in ["source", "textures", "blender"] {
        let path = folder.join(old);
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
    }
    for old in ["packet.json", "prepared.json"] {
        let path = folder.join(old);
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }
    let staged = source::stage_source_with_cancel(&path_text(&source)?, &folder.join("source"), Some(bridge), cancel)?;
    cancel.check()?;
    #[derive(Serialize)]
    struct Job<'a> {
        source: String,
        root: &'a str,
        packet: String,
    }
    #[derive(Deserialize)]
    struct Reply {
        meshes: usize,
        vertices: usize,
        triangles: usize,
        materials: usize,
        #[serde(default)]
        warnings: Vec<String>,
    }
    let packet_path = folder.join("packet.json");
    let reply: Reply = bridge.execute_script(
        SCRIPT,
        &Job { source: path_text(&staged)?, root, packet: path_text(&packet_path)? },
        &folder.join("blender"),
    )?;
    // The script writes textures beside its job; keep them in the object's folder.
    let images = folder.join("blender").join("textures");
    if images.is_dir() {
        fs::rename(&images, folder.join("textures"))?;
    }
    if safety::digest_with_cancel(&source, cancel)? != before {
        return Err(invalid("The object's file changed while it was being prepared. Prepare it again."));
    }
    let mut packet = read_packet(&packet_path)?;
    // Image paths pointed into the job folder; they now live in `textures/`.
    let moved = |path: &mut String| {
        if let Ok(rest) = Path::new(path.as_str()).strip_prefix(folder.join("blender").join("textures")) {
            *path = folder.join("textures").join(rest).to_string_lossy().into_owned();
        }
    };
    for choices in packet.materials.values_mut() {
        for path in choices.textures.iter_mut().flat_map(|m| m.values_mut()) {
            moved(path);
        }
        for paths in choices.candidates.iter_mut().flat_map(|m| m.values_mut()) {
            paths.iter_mut().for_each(moved);
        }
    }
    let bounds = crate::ability_object::bounds(&packet).ok_or_else(|| invalid("The prepared object has no points."))?;
    write_json(&packet_path, &packet)?;
    let prepared = Prepared {
        version: PACKET_VERSION.into(),
        source: path_text(&source)?,
        source_size: size,
        source_modified: modified,
        source_sha256: before,
        scripts_sha256: scripts_digest(),
        root: root.into(),
        packet_sha256: safety::digest_with_cancel(&packet_path, cancel)?,
        counts: Counts { meshes: reply.meshes, vertices: reply.vertices, triangles: reply.triangles, materials: reply.materials },
        warnings: reply.warnings,
        bounds_min: bounds.0,
        bounds_max: bounds.1,
        unit_metres: packet.unit_metres,
        color_images: packet.materials.iter().filter_map(|(m, c)| c.textures.as_ref()?.get("color").map(|p| (m.clone(), p.clone()))).collect(),
    };
    write_json(&folder.join("prepared.json"), &prepared)?;
    Ok(prepared)
}
fn read_packet(path: &Path) -> Result<Packet> {
    let packet: Packet = serde_json::from_slice(&fs::read(path)?)?;
    if !readable_version(&packet.version) {
        return Err(invalid(NOT_PREPARED));
    }
    if packet.meshes.is_empty() || !packet.unit_metres.is_finite() || packet.unit_metres <= 0. {
        return Err(invalid("The prepared object has no meshes or no valid unit."));
    }
    Ok(packet)
}
/// Bounds (min, max) of a packet's points.
pub fn bounds(packet: &Packet) -> Option<([f64; 3], [f64; 3])> {
    let mut out: Option<([f64; 3], [f64; 3])> = None;
    for p in packet.points() {
        let b = out.get_or_insert((p, p));
        for i in 0..3 {
            b.0[i] = b.0[i].min(p[i]);
            b.1[i] = b.1[i].max(p[i]);
        }
    }
    out
}
/// The prepared packet for a build or the fit view: refused (`NOT_PREPARED`)
/// unless the stamp is there, the source file still has the digest it was
/// prepared from and the packet the digest the stamp recorded. No Blender.
pub fn load(folder: &Path, source: &Path, root: &str, cancel: &CancellationToken) -> Result<(Prepared, Packet)> {
    let not_prepared = || invalid(format!("{NOT_PREPARED}: {}", source.display()));
    // The path as preparing saved it (resolved: any spelling of the same file).
    let source = &path_text(source).ok().and_then(|s| source::resolve_source(&s).ok()).ok_or_else(not_prepared)?;
    let saved: Prepared = fs::read(folder.join("prepared.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).ok_or_else(not_prepared)?;
    let packet_path = folder.join("packet.json");
    if !readable_version(&saved.version)
        || saved.root != root
        || saved.source != path_text(source)?
        || !source.is_file()
        || safety::digest_with_cancel(source, cancel)? != saved.source_sha256
        || !packet_path.is_file()
        || safety::digest_with_cancel(&packet_path, cancel)? != saved.packet_sha256
    {
        return Err(not_prepared());
    }
    let packet = read_packet(&packet_path)?;
    Ok((saved, packet))
}
/// The folder of one object's preparation under a project's workspace root.
pub fn folder(project_root: &Path, target: &str) -> PathBuf {
    let key = format!("{:x}", Sha256::digest(deadlock_core::text::casefold(target).as_bytes()));
    project_root.join("ability-objects").join(&key[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_packet_version_and_the_script_agree() {
        let script = include_str!("../resources/blender/ability_object.py");
        assert!(script.contains(&format!("VERSION = '{PACKET_VERSION}'")));
        assert!(!script.contains("import worker") && !script.contains("from worker"), "its own entry, never worker.py");
    }

    #[test]
    fn only_object_file_types_are_accepted_and_folders_are_per_target() {
        assert!(check_source(Path::new("C:/files/shape.pmx")).is_err());
        assert!(check_source(Path::new("C:/files/shape.FBX")).is_ok());
        let root = Path::new("C:/w/p");
        assert_eq!(folder(root, "models/abilities/A.vmdl"), folder(root, "MODELS/abilities/a.vmdl"));
        assert_ne!(folder(root, "models/abilities/a.vmdl"), folder(root, "models/abilities/b.vmdl"));
    }

    #[test]
    fn a_missing_or_changed_preparation_is_refused_without_blender() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("shape.fbx");
        fs::write(&source, b"first").unwrap();
        let place = temp.path().join("object");
        let cancel = CancellationToken::new();
        let error = load(&place, &source, "models/dmb_ability_objects/x", &cancel).unwrap_err().to_string();
        assert!(error.starts_with(NOT_PREPARED), "{error}");
        // A stamp and packet as a preparation leaves them.
        fs::create_dir_all(&place).unwrap();
        let packet = Packet {
            version: PACKET_VERSION.into(),
            unit_metres: 1.,
            meshes: vec![serde_json::from_value(serde_json::json!({"name": "m", "positions": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "normals": [[0, 0, 1], [0, 0, 1], [0, 0, 1]], "faces": [{"vertices": [0, 1, 2], "corners": [0, 1, 2], "material": "models/dmb_ability_objects/x/materials/prepared/m.vmat"}]})).unwrap()],
            materials: BTreeMap::new(),
            warnings: vec![],
            preview: vec![],
            armature: vec![],
        };
        write_json(&place.join("packet.json"), &packet).unwrap();
        let (size, modified) = stamp(&source).unwrap();
        let prepared = Prepared {
            version: PACKET_VERSION.into(),
            source: path_text(&source::resolve_source(&path_text(&source).unwrap()).unwrap()).unwrap(),
            source_size: size,
            source_modified: modified,
            source_sha256: safety::digest_with_cancel(&source, &cancel).unwrap(),
            scripts_sha256: scripts_digest(),
            root: "models/dmb_ability_objects/x".into(),
            packet_sha256: safety::digest_with_cancel(&place.join("packet.json"), &cancel).unwrap(),
            counts: Counts::default(),
            warnings: vec![],
            bounds_min: [0.; 3],
            bounds_max: [1., 1., 0.],
            unit_metres: 1.,
            color_images: BTreeMap::new(),
        };
        write_json(&place.join("prepared.json"), &prepared).unwrap();
        let (_, loaded) = load(&place, &source, "models/dmb_ability_objects/x", &cancel).unwrap();
        // A packet of the first version, kept from before: still built from, prepared again only on request.
        {
            let old = place.join("old");
            fs::create_dir_all(&old).unwrap();
            write_json(&old.join("packet.json"), &Packet { version: OLD_PACKET_VERSIONS[0].into(), ..packet.clone() }).unwrap();
            write_json(&old.join("prepared.json"), &Prepared { version: OLD_PACKET_VERSIONS[0].into(), packet_sha256: safety::digest_with_cancel(&old.join("packet.json"), &cancel).unwrap(), ..prepared.clone() }).unwrap();
            assert!(load(&old, &source, "models/dmb_ability_objects/x", &cancel).is_ok());
            assert!(current(&old, &source, "models/dmb_ability_objects/x", &cancel).is_none());
        }
        // Another spelling of the same file (forward slashes) is the same preparation.
        let spelled = PathBuf::from(source.to_string_lossy().replace('\\', "/"));
        assert!(load(&place, &spelled, "models/dmb_ability_objects/x", &cancel).is_ok());
        assert_eq!(bounds(&loaded), Some(([0.; 3], [1., 1., 0.])));
        assert!(current(&place, &source, "models/dmb_ability_objects/x", &cancel).is_some());
        assert!(load(&place, &source, "models/dmb_ability_objects/y", &cancel).is_err(), "another object's folder");
        // Prepared by an earlier version of the scripts: still built from (the packet and the
        // file are what they were), only prepared again when the user prepares it.
        write_json(&place.join("prepared.json"), &Prepared { scripts_sha256: "an earlier script".into(), ..prepared.clone() }).unwrap();
        assert!(load(&place, &source, "models/dmb_ability_objects/x", &cancel).is_ok());
        assert!(current(&place, &source, "models/dmb_ability_objects/x", &cancel).is_none());
        write_json(&place.join("prepared.json"), &prepared).unwrap();
        fs::write(&source, b"second").unwrap();
        assert!(load(&place, &source, "models/dmb_ability_objects/x", &cancel).is_err(), "the file changed");
        assert!(current(&place, &source, "models/dmb_ability_objects/x", &cancel).is_none());
    }
}
