//! The images a binary FBX refers to, read the way Blender's FBX importer
//! finds them, so staging copies those instead of every picture in the folder
//! (issue #7: a wallpaper beside the model stopped the preparation).
//!
//! Blender (io_scene_fbx `import_fbx.py` `blen_read_texture_image`) takes each
//! Texture and Video element's `RelativeFilename`, else `FileName`, else
//! `Filename`, else the element's own name, and hands it to bpy_extras
//! `image_utils.load_image` with the FBX's folder and Image Search on. That
//! tries the path, the path below the FBX's folder and the base name beside
//! the FBX, then searches the FBX's folder recursively for the base name,
//! compared without case. No other name variants are tried. Every candidate
//! therefore has one of the collected base names, and staging all files with
//! such a name keeps the copies Blender can find (and its search order).
//!
//! The layout follows `parse_fbx.py`. Anything it would not read, or that this
//! reader does not know, is `None`, and staging then falls back to every image.
//!
//! `read` also gives the file names the paths ask for (for an image saved
//! under another extension, `source.rs stand_ins`) and the materials with
//! whether a texture is connected to them (`texture_names`).
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek, SeekFrom},
};

const MAGIC: &[u8; 23] = b"Kaydara FBX Binary  \x00\x1a\x00";
const PATH_NODES: [&[u8]; 3] = [b"RelativeFilename", b"FileName", b"Filename"];
/// Blender replaces longer element names by a hash, which names no file.
const NAME_BYTES: usize = 63;
/// Real files nest a few levels; deeper data is not an FBX this reader trusts.
const MAX_DEPTH: usize = 64;

enum Stop {
    Malformed,
    Failed(crate::Error),
}
impl From<std::io::Error> for Stop {
    fn from(_: std::io::Error) -> Self {
        Stop::Malformed
    }
}
type Step<T> = std::result::Result<T, Stop>;

struct Walker<'a, R> {
    input: R,
    pos: u64,
    wide: bool,
    cancel: &'a deadlock_core::CancellationToken,
    names: BTreeSet<String>,
    /// Base names read from the path properties, lowercase, with the name as written.
    paths: BTreeMap<String, String>,
    /// Material elements: id and name.
    materials: Vec<(i64, String)>,
    /// Ids of Texture and LayeredTexture elements, with the base name Blender
    /// asks for (its `RelativeFilename`, else `FileName`, else `Filename`), when one.
    textures: BTreeMap<i64, Option<String>>,
    /// Connections: child id, parent id.
    links: Vec<(i64, i64)>,
}

/// One property value kept while reading an element.
enum Value {
    Text(Vec<u8>),
    Id(i64),
    Other,
}

/// What a binary FBX says about its images and materials.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    /// Every base name Blender may search for, lowercase (`image_names`).
    pub names: BTreeSet<String>,
    /// The base names its path properties ask for, lowercase, with the name as written.
    pub paths: BTreeMap<String, String>,
    pub materials: Vec<Material>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub name: String,
    /// Base names (lowercase) the textures connected to it ask for; empty
    /// when no texture is connected. A texture without a path counts with
    /// no name.
    pub textures: Vec<Option<String>>,
}
impl Material {
    pub fn linked(&self) -> bool {
        !self.textures.is_empty()
    }
}

impl<R: Read + Seek> Walker<'_, R> {
    /// `count` bytes that must end at or before `end`.
    fn bytes(&mut self, count: u64, end: u64) -> Step<Vec<u8>> {
        if count > end.saturating_sub(self.pos) {
            return Err(Stop::Malformed);
        }
        let mut data = vec![0; count as usize];
        self.input.read_exact(&mut data)?;
        self.pos += count;
        Ok(data)
    }
    fn skip(&mut self, count: u64, end: u64) -> Step<()> {
        if count > end.saturating_sub(self.pos) {
            return Err(Stop::Malformed);
        }
        if count <= 4096 {
            // Short skips stay inside a BufReader's buffer.
            self.bytes(count, end)?;
        } else {
            self.input.seek(SeekFrom::Current(count as i64))?;
            self.pos += count;
        }
        Ok(())
    }
    fn u32(&mut self, end: u64) -> Step<u64> {
        Ok(u32::from_le_bytes(self.bytes(4, end)?.try_into().unwrap()) as u64)
    }
    fn word(&mut self, end: u64) -> Step<u64> {
        if self.wide {
            Ok(u64::from_le_bytes(self.bytes(8, end)?.try_into().unwrap()))
        } else {
            self.u32(end)
        }
    }
    fn sentinel(&self) -> u64 {
        if self.wide { 25 } else { 13 }
    }
    /// One node record and its children; false for the null record that ends a list.
    fn node(&mut self, limit: u64, depth: usize, scope: Scope) -> Step<bool> {
        self.node_in(limit, depth, scope, &mut None)
    }
    /// `paths`: the path properties of the Texture element being read, by
    /// `PATH_NODES` order, filled by its direct children.
    fn node_in(&mut self, limit: u64, depth: usize, scope: Scope, paths: &mut Option<[Option<String>; 3]>) -> Step<bool> {
        let in_objects = scope == Scope::Objects;
        self.cancel.check().map_err(|e| Stop::Failed(e.into()))?;
        if depth > MAX_DEPTH {
            return Err(Stop::Malformed);
        }
        let end = self.word(limit)?;
        let count = self.word(limit)?;
        let _length = self.word(limit)?;
        let name_length = self.bytes(1, limit)?[0] as u64;
        let name = self.bytes(name_length, limit)?;
        if end == 0 {
            return Ok(false);
        }
        // A record without properties or children ends where its name ends
        // (Blender's own exporter writes such records).
        if end < self.pos || end > limit {
            return Err(Stop::Malformed);
        }
        let path_node = PATH_NODES.iter().position(|p| *p == name.as_slice());
        let element = in_objects && matches!(name.as_slice(), b"Texture" | b"Video");
        let texture = in_objects && matches!(name.as_slice(), b"Texture" | b"LayeredTexture");
        let material = in_objects && name.as_slice() == b"Material";
        let connection = scope == Scope::Connections && name.as_slice() == b"C";
        let keep = element || texture || material || connection;
        let mut values = Vec::new();
        let mut first_text = None;
        // Each property takes at least its type byte, so `end` bounds the loop.
        for _ in 0..count {
            let kind = self.bytes(1, end)?[0];
            let mut value = Value::Other;
            match kind {
                b'Z' | b'B' | b'C' => self.skip(1, end)?,
                b'Y' => self.skip(2, end)?,
                b'I' | b'F' => self.skip(4, end)?,
                b'L' if keep => {
                    value = Value::Id(i64::from_le_bytes(self.bytes(8, end)?.try_into().unwrap()))
                }
                b'D' | b'L' => self.skip(8, end)?,
                b'b' | b'c' | b'i' | b'l' | b'f' | b'd' => {
                    // Element count and encoding, then the (maybe deflated) payload.
                    self.skip(8, end)?;
                    let stored = self.u32(end)?;
                    self.skip(stored, end)?
                }
                b'S' | b'R' => {
                    let size = self.u32(end)?;
                    if kind == b'S' && (path_node.is_some() || keep) {
                        value = Value::Text(self.bytes(size, end)?);
                    } else {
                        self.skip(size, end)?
                    }
                }
                _ => return Err(Stop::Malformed),
            }
            if let (Some(_), Value::Text(text)) = (path_node, &value) {
                let text = String::from_utf8_lossy(text).into_owned();
                self.add(&text);
                if let Some(base) = base_name(&text) {
                    self.paths.entry(base.to_lowercase()).or_insert_with(|| base.to_owned());
                }
                first_text.get_or_insert(text);
            }
            if keep {
                values.push(value);
            }
        }
        // A direct child of a Texture element: Blender takes the first of
        // each path property, and an empty one counts as absent.
        if let (Some(index), Some(slots), Some(text)) = (path_node, paths.as_mut(), first_text) {
            if slots[index].is_none() && !text.is_empty() {
                slots[index] = Some(text);
            }
        }
        let texts: Vec<Option<&Vec<u8>>> = values.iter().map(|v| match v { Value::Text(t) => Some(t), _ => None }).collect();
        let id = match values.first() { Some(Value::Id(id)) => Some(*id), _ => None };
        // The element name is the second-last property, "name\0\x01Class".
        let element_name = (texts.len() >= 2)
            .then(|| texts[texts.len() - 2])
            .flatten()
            .map(|full| full.split(|b| *b == 0).next().unwrap_or_default().to_vec());
        if element {
            if let Some(name) = &element_name {
                if name.len() <= NAME_BYTES {
                    self.add(&String::from_utf8_lossy(name));
                }
            }
        }
        if let (true, Some(id), Some(name)) = (material, id, &element_name) {
            self.materials.push((id, String::from_utf8_lossy(name).into_owned()));
        }
        if connection {
            if let [_, Value::Id(child), Value::Id(parent), ..] = values.as_slice() {
                self.links.push((*child, *parent));
            }
        }
        let mut own_paths = (texture && name.as_slice() == b"Texture").then(|| [None, None, None]);
        if self.pos < end {
            let children = end
                .checked_sub(self.sentinel())
                .filter(|v| *v >= self.pos)
                .ok_or(Stop::Malformed)?;
            let scope = match (depth, name.as_slice()) {
                (0, b"Objects") => Scope::Objects,
                (0, b"Connections") => Scope::Connections,
                _ => Scope::Other,
            };
            while self.pos < children {
                self.node_in(children, depth + 1, scope, &mut own_paths)?;
            }
            if self.bytes(self.sentinel(), end)?.iter().any(|b| *b != 0) {
                return Err(Stop::Malformed);
            }
        }
        if self.pos != end {
            return Err(Stop::Malformed);
        }
        if let (true, Some(id)) = (texture, id) {
            let requested = own_paths
                .and_then(|slots| slots.into_iter().flatten().next())
                .and_then(|path| base_name(&path).map(str::to_lowercase));
            self.textures.insert(id, requested);
        }
        Ok(true)
    }
    /// The base name `load_image` searches for, without case.
    fn add(&mut self, path: &str) {
        if let Some(base) = base_name(path) {
            self.names.insert(base.to_lowercase());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Scope {
    Top,
    Objects,
    Connections,
    Other,
}

/// The file name at the end of `path` (after any drive letter), when one.
fn base_name(path: &str) -> Option<&str> {
    let path = path.get(..2).filter(|d| d.ends_with(':')).map_or(path, |_| &path[2..]);
    Some(path.rsplit(['/', '\\']).next().unwrap_or_default()).filter(|b| !b.is_empty())
}

/// Lowercase base names of the images the FBX's textures refer to, or `None`
/// when `input` (of `length` bytes) is not a binary FBX this reader can walk.
pub fn image_names<R: Read + Seek>(
    input: R,
    length: u64,
    cancel: &deadlock_core::CancellationToken,
) -> crate::Result<Option<BTreeSet<String>>> {
    Ok(read(input, length, cancel)?.map(|summary| summary.names))
}

/// Image names and materials of a binary FBX, or `None` when `input` (of
/// `length` bytes) is not a binary FBX this reader can walk.
pub fn read<R: Read + Seek>(
    input: R,
    length: u64,
    cancel: &deadlock_core::CancellationToken,
) -> crate::Result<Option<Summary>> {
    let mut walker = Walker {
        input,
        pos: 0,
        wide: false,
        cancel,
        names: BTreeSet::new(),
        paths: BTreeMap::new(),
        materials: Vec::new(),
        textures: BTreeMap::new(),
        links: Vec::new(),
    };
    let header = (|| -> Step<bool> {
        if walker.bytes(23, length)?.as_slice() != MAGIC {
            return Ok(false);
        }
        walker.wide = walker.u32(length)? >= 7500;
        Ok(true)
    })();
    let walk = match header {
        Ok(false) => return Ok(None),
        Ok(true) => (|| -> Step<()> {
            while walker.node(length, 0, Scope::Top)? {}
            Ok(())
        })(),
        Err(stop) => Err(stop),
    };
    match walk {
        Ok(()) => {
            // A texture connected to a material, also through a layered texture.
            let mut materials = Vec::new();
            for (id, name) in &walker.materials {
                let mut textures = Vec::new();
                let mut pending = vec![*id];
                let mut seen = BTreeSet::new();
                while let Some(parent) = pending.pop() {
                    if !seen.insert(parent) || seen.len() > 64 {
                        continue;
                    }
                    for (child, _) in walker.links.iter().filter(|(_, p)| *p == parent) {
                        if let Some(requested) = walker.textures.get(child) {
                            textures.push(requested.clone());
                            pending.push(*child);
                        }
                    }
                }
                materials.push(Material { name: name.clone(), textures });
            }
            Ok(Some(Summary { names: walker.names, paths: walker.paths, materials }))
        }
        Err(Stop::Malformed) => Ok(None),
        Err(Stop::Failed(error)) => Err(error),
    }
}
