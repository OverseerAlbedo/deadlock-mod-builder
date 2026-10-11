//! Bounds-checked PE imports/exports. Never loads or executes a DLL.
use deadlock_core::{text::casefold, CancellationToken, CoreError, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
fn bad(message: impl Into<String>) -> CoreError {
    CoreError::Integrity(message.into())
}
struct Bytes<'a>(&'a [u8]);
impl Bytes<'_> {
    fn at(&self, start: usize, size: usize) -> Result<&[u8]> {
        self.0
            .get(
                start
                    ..start
                        .checked_add(size)
                        .ok_or_else(|| bad("PE offset overflow"))?,
            )
            .ok_or_else(|| bad("Truncated PE header/import data"))
    }
    fn u16(&self, p: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.at(p, 2)?.try_into().expect("length checked"),
        ))
    }
    fn u32(&self, p: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.at(p, 4)?.try_into().expect("length checked"),
        ))
    }
    fn u64(&self, p: usize) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.at(p, 8)?.try_into().expect("length checked"),
        ))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Section {
    pub name: String,
    pub rva: u32,
    pub raw_size: u32,
    pub raw_offset: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Export {
    pub name: String,
    pub ordinal: u64,
    pub rva: u32,
    pub forwarder: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeInfo {
    pub machine: String,
    pub amd64: bool,
    pub imports: Vec<String>,
    pub delay_imports: Vec<String>,
    pub exports: Vec<Export>,
    pub timestamp: u32,
    pub image_base: String,
    pub sections: Vec<Section>,
}
struct Image<'a> {
    data: Bytes<'a>,
    headers: u32,
    sections: Vec<Section>,
}
impl Image<'_> {
    fn offset(&self, rva: u64) -> Result<usize> {
        if rva < (self.headers as usize).min(self.data.0.len()) as u64 {
            return Ok(rva as usize);
        }
        for section in &self.sections {
            let base = section.rva as u64;
            if base <= rva && rva < base + section.raw_size as u64 {
                let address = section.raw_offset as u64 + rva - base;
                if address < self.data.0.len() as u64 {
                    return Ok(address as usize);
                }
            }
        }
        Err(bad("Unmapped PE import RVA"))
    }
    fn string(&self, rva: u64, basename: bool) -> Result<String> {
        let start = self.offset(rva)?;
        let max = if basename { 1024 } else { 65536 };
        let bytes = &self.data.0[start..self.data.0.len().min(start + max)];
        let end = bytes
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| bad("Unterminated PE import name"))?;
        let bytes = &bytes[..end];
        if bytes.is_empty() || !bytes.is_ascii() {
            return Err(bad("Invalid PE import name"));
        }
        let text = std::str::from_utf8(bytes)
            .expect("ASCII checked")
            .to_owned();
        if basename && text.contains(['/', '\\', ':']) {
            return Err(bad("Invalid PE DLL basename"));
        }
        Ok(text)
    }
}
pub fn parse(raw: &[u8]) -> Result<PeInfo> {
    let data = Bytes(raw);
    if raw.len() < 64 || data.at(0, 2)? != b"MZ" {
        return Err(bad("Not a PE executable"));
    }
    let pe = data.u32(0x3c)? as usize;
    if data.at(pe, 4)? != b"PE\0\0" {
        return Err(bad("Invalid PE signature"));
    }
    let machine = data.u16(pe + 4)?;
    let count = data.u16(pe + 6)? as usize;
    let opt_size = data.u16(pe + 20)? as usize;
    let opt = pe + 24;
    let magic = data.u16(opt)?;
    if !matches!(magic, 0x20b | 0x10b) {
        return Err(bad("Unsupported PE optional header"));
    }
    let minimum = if magic == 0x20b { 112 } else { 96 };
    if opt_size < minimum || count > 96 {
        return Err(bad("Invalid PE header lengths"));
    }
    data.at(opt, opt_size)?;
    let base = if magic == 0x20b {
        data.u64(opt + 24)?
    } else {
        data.u32(opt + 28)? as u64
    };
    let headers = data.u32(opt + 60)?;
    let directories = (data.u32(opt + minimum - 4)? as usize).min((opt_size - minimum) / 8);
    let mut sections = vec![];
    for i in 0..count {
        let p = opt + opt_size + 40 * i;
        data.at(p, 40)?;
        let name = data.at(p, 8)?;
        let end = name.iter().position(|b| *b == 0).unwrap_or(8);
        sections.push(Section {
            name: String::from_utf8_lossy(&name[..end]).into_owned(),
            rva: data.u32(p + 12)?,
            raw_size: data.u32(p + 16)?,
            raw_offset: data.u32(p + 20)?,
        });
    }
    let image = Image {
        data,
        headers,
        sections,
    };
    let mut exports = vec![];
    if directories > 0 {
        let rva = image.data.u32(opt + minimum)? as u64;
        let size = image.data.u32(opt + minimum + 4)? as u64;
        if rva != 0 && size != 0 {
            if size < 40 {
                return Err(bad("Truncated PE export directory"));
            }
            let p = image.offset(rva)?;
            image.data.at(p, 40)?;
            let ordinal_base = image.data.u32(p + 16)? as u64;
            let functions = image.data.u32(p + 20)? as usize;
            let names = image.data.u32(p + 24)? as usize;
            let addresses = image.data.u32(p + 28)? as u64;
            let names_table = image.data.u32(p + 32)? as u64;
            let ordinals = image.data.u32(p + 36)? as u64;
            if names > 100_000 || functions > 1_000_000 || names > functions {
                return Err(bad("Invalid PE export counts"));
            }
            for i in 0..names {
                let name_rva = image.data.u32(image.offset(names_table + 4 * i as u64)?)? as u64;
                let ordinal = image.data.u16(image.offset(ordinals + 2 * i as u64)?)? as usize;
                if ordinal >= functions {
                    return Err(bad("Invalid PE export ordinal"));
                }
                let address = image
                    .data
                    .u32(image.offset(addresses + 4 * ordinal as u64)?)?;
                exports.push(Export {
                    name: image.string(name_rva, false)?,
                    ordinal: ordinal_base + ordinal as u64,
                    rva: address,
                    forwarder: if rva <= address as u64 && (address as u64) < rva + size {
                        Some(image.string(address as u64, false)?)
                    } else {
                        None
                    },
                });
            }
        }
    }
    let mut imports = vec![];
    let mut delay = vec![];
    for (index, width, target) in [(1, 20, &mut imports), (13, 32, &mut delay)] {
        if directories <= index {
            continue;
        }
        let rva = image.data.u32(opt + minimum + 8 * index)? as u64;
        let size = image.data.u32(opt + minimum + 8 * index + 4)? as usize;
        if rva == 0 || size == 0 {
            continue;
        }
        let mut terminated = false;
        for i in 0..(size / width).min(4096) {
            let p = image.offset(rva + (i * width) as u64)?;
            let values: Vec<_> = (0..width / 4)
                .map(|j| image.data.u32(p + 4 * j))
                .collect::<Result<_>>()?;
            if values.iter().all(|v| *v == 0) {
                terminated = true;
                break;
            }
            let name = if index == 1 {
                values[3] as u64
            } else {
                (values[1] as u64)
                    .checked_sub(if values[0] & 1 != 0 { 0 } else { base })
                    .ok_or_else(|| bad("Invalid delay import address"))?
            };
            target.push(image.string(name, true)?);
        }
        if !terminated {
            return Err(bad("Unterminated PE import directory"));
        }
        target.sort_by_key(|s| casefold(s));
        target.dedup();
    }
    Ok(PeInfo {
        machine: format!("0x{machine:x}"),
        amd64: machine == 0x8664,
        imports,
        delay_imports: delay,
        exports,
        timestamp: image.data.u32(pe + 8)?,
        image_base: format!("0x{base:x}"),
        sections: image.sections,
    })
}
pub fn imports(path: &Path) -> Result<PeInfo> {
    let size = fs::metadata(path)
        .map_err(|e| CoreError::io("Inspect PE binary", e))?
        .len();
    if size > 512 * 1024 * 1024 {
        return Err(bad("PE binary exceeds supported size"));
    }
    parse(&fs::read(path).map_err(|e| CoreError::io("Read PE binary", e))?)
}
const SYSTEM: [&str; 24] = [
    "kernel32.dll",
    "kernelbase.dll",
    "ntdll.dll",
    "user32.dll",
    "gdi32.dll",
    "advapi32.dll",
    "ws2_32.dll",
    "ole32.dll",
    "oleaut32.dll",
    "shell32.dll",
    "shlwapi.dll",
    "comdlg32.dll",
    "comctl32.dll",
    "secur32.dll",
    "crypt32.dll",
    "winmm.dll",
    "version.dll",
    "dbghelp.dll",
    "rpcrt4.dll",
    "psapi.dll",
    "bcrypt.dll",
    "imm32.dll",
    "setupapi.dll",
    "wintrust.dll",
];
pub fn local_import_audit(
    executable: &Path,
    cancel: &CancellationToken,
) -> Result<serde_json::Value> {
    let dir = executable
        .parent()
        .ok_or_else(|| bad("Executable has no parent"))?;
    let mut local = BTreeMap::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir).map_err(|e| CoreError::io("Read compiler directory", e))? {
            let path = entry
                .map_err(|e| CoreError::io("Read compiler dependency", e))?
                .path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    local.insert(casefold(name), path);
                }
            }
        }
    }
    let mut pending = vec![executable
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()];
    if let Some(path) = local.get("resourcecompiler.dll") {
        pending.push(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        );
    }
    let mut modules = BTreeMap::new();
    let mut missing = BTreeSet::new();
    let mut system = BTreeSet::new();
    let mut errors = BTreeMap::new();
    while let Some(name) = pending.pop() {
        cancel.check()?;
        if modules.len() + errors.len() >= 512 {
            return Err(bad("Compiler import graph exceeds512 modules"));
        }
        let key = casefold(&name);
        if modules.contains_key(&key) || errors.contains_key(&key) {
            continue;
        }
        let Some(path) = local.get(&key) else {
            missing.insert(name);
            continue;
        };
        match imports(path) {
            Ok(value) => {
                for dep in value.imports.iter().chain(&value.delay_imports) {
                    let key = casefold(dep);
                    if SYSTEM.contains(&key.as_str())
                        || key.starts_with("api-ms-")
                        || key.starts_with("ext-ms-")
                    {
                        system.insert(dep.clone());
                    } else if local.contains_key(&key) {
                        pending.push(dep.clone());
                    } else {
                        missing.insert(dep.clone());
                    }
                }
                modules.insert(key, value);
            }
            Err(error) => {
                errors.insert(key, error.to_string());
            }
        }
    }
    Ok(
        serde_json::json!({"modules":modules,"unresolved_in_selected_bin":missing,"windows_system_imports":system,"parse_errors":errors,"scope":"Static and delay imports; resourcecompiler.dll is explicitly seeded if present. Does not include runtime LoadLibrary or mounted assets. Unresolved imports may live in another configured search directory."}),
    )
}
