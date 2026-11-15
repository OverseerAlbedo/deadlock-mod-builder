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
use std::{
    collections::BTreeSet,
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
    fn node(&mut self, limit: u64, depth: usize, in_objects: bool) -> Step<bool> {
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
        if end <= self.pos || end > limit {
            return Err(Stop::Malformed);
        }
        let paths = PATH_NODES.contains(&name.as_slice());
        let element = in_objects && matches!(name.as_slice(), b"Texture" | b"Video");
        let mut strings = Vec::new();
        // Each property takes at least its type byte, so `end` bounds the loop.
        for _ in 0..count {
            let kind = self.bytes(1, end)?[0];
            let mut text = None;
            match kind {
                b'Z' | b'B' | b'C' => self.skip(1, end)?,
                b'Y' => self.skip(2, end)?,
                b'I' | b'F' => self.skip(4, end)?,
                b'D' | b'L' => self.skip(8, end)?,
                b'b' | b'c' | b'i' | b'l' | b'f' | b'd' => {
                    // Element count and encoding, then the (maybe deflated) payload.
                    self.skip(8, end)?;
                    let stored = self.u32(end)?;
                    self.skip(stored, end)?
                }
                b'S' | b'R' => {
                    let size = self.u32(end)?;
                    if kind == b'S' && (paths || element) {
                        text = Some(self.bytes(size, end)?);
                    } else {
                        self.skip(size, end)?
                    }
                }
                _ => return Err(Stop::Malformed),
            }
            if element {
                strings.push(text.clone());
            }
            if let (true, Some(text)) = (paths, text) {
                self.add(&String::from_utf8_lossy(&text));
            }
        }
        if element && strings.len() >= 2 {
            // The element name is the second-last property, "name\0\x01Class".
            if let Some(full) = &strings[strings.len() - 2] {
                let name = full.split(|b| *b == 0).next().unwrap_or_default();
                if name.len() <= NAME_BYTES {
                    self.add(&String::from_utf8_lossy(name));
                }
            }
        }
        if self.pos < end {
            let children = end
                .checked_sub(self.sentinel())
                .filter(|v| *v >= self.pos)
                .ok_or(Stop::Malformed)?;
            while self.pos < children {
                self.node(children, depth + 1, depth == 0 && name == b"Objects")?;
            }
            if self.bytes(self.sentinel(), end)?.iter().any(|b| *b != 0) {
                return Err(Stop::Malformed);
            }
        }
        if self.pos != end {
            return Err(Stop::Malformed);
        }
        Ok(true)
    }
    /// The base name `load_image` searches for, without case.
    fn add(&mut self, path: &str) {
        let path = path.get(..2).filter(|d| d.ends_with(':')).map_or(path, |_| &path[2..]);
        let base = path.rsplit(['/', '\\']).next().unwrap_or_default();
        if !base.is_empty() {
            self.names.insert(base.to_lowercase());
        }
    }
}

/// Lowercase base names of the images the FBX's textures refer to, or `None`
/// when `input` (of `length` bytes) is not a binary FBX this reader can walk.
pub fn image_names<R: Read + Seek>(
    input: R,
    length: u64,
    cancel: &deadlock_core::CancellationToken,
) -> crate::Result<Option<BTreeSet<String>>> {
    let mut walker = Walker {
        input,
        pos: 0,
        wide: false,
        cancel,
        names: BTreeSet::new(),
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
            while walker.node(length, 0, false)? {}
            Ok(())
        })(),
        Err(stop) => Err(stop),
    };
    match walk {
        Ok(()) => Ok(Some(walker.names)),
        Err(Stop::Malformed) => Ok(None),
        Err(Stop::Failed(error)) => Err(error),
    }
}
