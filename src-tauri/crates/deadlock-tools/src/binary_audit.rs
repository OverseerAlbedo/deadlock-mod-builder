//! Read-only static ModelDoc evidence, separate from actual native registration.
use crate::pe;
use deadlock_core::{text::casefold, CancellationToken, CoreError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
pub const CLASSES: [(&str, &str); 5] = [
    ("NmSkeletonList", "CModelDocNmSkeletonList"),
    ("NmSkeletonReference", "CModelDocNmSkeletonReference"),
    ("AnimGraph2List", "CModelDocAnimGraph2List"),
    ("DefaultAnimGraph2", "CModelDocDefaultAnimGraph2Reference"),
    ("AnimGraph2", "CModelDocAnimGraph2Reference"),
];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Matches {
    pub count: usize,
    pub file_offsets: Vec<usize>,
}
fn word(value: u8) -> bool {
    value.is_ascii_alphanumeric() || value == b'_'
}
fn matches(data: &[u8], needle: &[u8], literal: bool, utf16: bool) -> Matches {
    let mut result = Matches {
        count: 0,
        file_offsets: vec![],
    };
    let mut start = 0;
    while let Some(index) = memchr::memmem::find(&data[start..], needle) {
        let position = start + index;
        start = position + 1;
        let prefixed = if utf16 {
            position >= 2 && word(data[position - 2]) && data[position - 1] == 0
        } else {
            position > 0 && word(data[position - 1])
        };
        if literal && prefixed {
            continue;
        }
        result.count += 1;
        if result.file_offsets.len() < 8 {
            result.file_offsets.push(position);
        }
        if start >= data.len() {
            break;
        }
    }
    result
}
pub fn scan_file(path: &Path, cancel: &CancellationToken) -> Result<serde_json::Value> {
    crate::acquisition::no_links(path)?;
    let metadata = fs::metadata(path).map_err(|e| CoreError::io("Inspect binary", e))?;
    if metadata.len() == 0 || metadata.len() > 512 * 1024 * 1024 {
        return Err(CoreError::Integrity("Empty or oversized binary".into()));
    }
    cancel.check()?;
    let data = fs::read(path).map_err(|e| CoreError::io("Read binary for static audit", e))?;
    let info = pe::parse(&data)?;
    let export_count = info.exports.len();
    let exports: Vec<_> = info
        .exports
        .iter()
        .enumerate()
        .filter(|(i, value)| {
            *i < 64
                || [
                    "modeldoc",
                    "animgraph",
                    "nmskeleton",
                    "createinterface",
                    "installschemabindings",
                    "extractmodulemetadata",
                    "getresourcemanifest",
                ]
                .iter()
                .any(|token| value.name.to_lowercase().contains(token))
        })
        .map(|(_, value)| value)
        .collect();
    let mut pe = serde_json::to_value(&info)?;
    pe["exports"] = serde_json::to_value(&exports)?;
    pe["export_count"] = serde_json::json!(export_count);
    pe["exports_omitted"] = serde_json::json!(export_count - exports.len());
    let mut classes = BTreeMap::new();
    for (name, native) in CLASSES {
        cancel.check()?;
        let mut ascii = name.as_bytes().to_vec();
        ascii.push(0);
        let utf16: Vec<u8> = name
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        let rtti = format!(".?AV{native}@@\0");
        classes.insert(name,serde_json::json!({"substring":matches(&data,name.as_bytes(),false,false),"ascii_literal":matches(&data,&ascii,true,false),"utf16le_literal":matches(&data,&utf16,true,true),"modeldoc_rtti":matches(&data,rtti.as_bytes(),false,false)}));
    }
    let runtime: BTreeMap<_, _> = ["CAnimGraph2Instance", "CAnimGraph2Parameter", "CNmSkeleton"]
        .into_iter()
        .map(|name| (name, matches(&data, name.as_bytes(), false, false)))
        .collect();
    let controls: BTreeMap<_, _> = ["CModelDocBone", "RenderMeshFile", "InitModelDocLib"]
        .into_iter()
        .map(|name| (name, matches(&data, name.as_bytes(), false, false)))
        .collect();
    let versions: std::collections::BTreeSet<_> = regex::bytes::Regex::new(r"modeldoc\d{1,3}\x00")
        .expect("constant regex")
        .find_iter(&data)
        .map(|m| String::from_utf8_lossy(&m.as_bytes()[..m.len() - 1]).into_owned())
        .collect();
    Ok(
        serde_json::json!({"path":path,"bytes":data.len(),"sha256":format!("{:x}",Sha256::digest(&data)),"pe":pe,"classes":classes,"runtime_type_substrings":runtime,"modeldoc_format_strings":versions,"controls":controls,"scope":"Static file bytes only. Names/RTTI/exports are evidence of code/data presence, not proof that a native registry contains these classes."}),
    )
}
pub fn scan_tree(
    root: &Path,
    cancel: &CancellationToken,
    mut progress: impl FnMut(&str),
) -> Result<serde_json::Value> {
    crate::acquisition::no_links(root)?;
    if !root.is_dir() {
        return Err(CoreError::Integrity(
            "Choose an existing non-linked local toolset folder.".into(),
        ));
    }
    let root = deadlock_core::safety::resolve_path(root)?;
    let mut files = vec![];
    let mut skipped = vec![];
    let mut errors = BTreeMap::new();
    fn walk(
        root: &Path,
        dir: &Path,
        files: &mut Vec<serde_json::Value>,
        skipped: &mut Vec<String>,
        errors: &mut BTreeMap<String, String>,
        cancel: &CancellationToken,
        progress: &mut impl FnMut(&str),
    ) -> Result<()> {
        let mut entries = fs::read_dir(dir)
            .map_err(|e| CoreError::io("Read static audit directory", e))?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| CoreError::io("Read static audit entry", e))?;
        entries.sort_by_key(|p| casefold(&p.file_name().to_string_lossy()));
        for entry in entries {
            cancel.check()?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| CoreError::PathEscape(path.clone()))?
                .to_string_lossy()
                .replace('\\', "/");
            if crate::acquisition::no_links(&path).is_err() {
                skipped.push(relative);
                continue;
            }
            let kind = entry
                .file_type()
                .map_err(|e| CoreError::io("Inspect static audit entry", e))?;
            if kind.is_dir() {
                walk(root, &path, files, skipped, errors, cancel, progress)?;
            } else if kind.is_file()
                && path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "dll" | "exe"))
            {
                progress(&format!("Scanning {relative}"));
                match scan_file(&path, cancel) {
                    Ok(mut value) => {
                        value["path"] = serde_json::json!(relative);
                        files.push(value);
                    }
                    Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                    Err(error) => {
                        errors.insert(relative, error.to_string());
                    }
                }
            }
        }
        Ok(())
    }
    walk(
        &root,
        &root,
        &mut files,
        &mut skipped,
        &mut errors,
        cancel,
        &mut progress,
    )?;
    let mut classes = BTreeMap::new();
    for (name, _) in CLASSES {
        classes.insert(
            name,
            files
                .iter()
                .filter(|f| {
                    ["ascii_literal", "utf16le_literal", "modeldoc_rtti"]
                        .iter()
                        .any(|key| f["classes"][name][key]["count"].as_u64().unwrap_or(0) > 0)
                })
                .map(|f| f["path"].clone())
                .collect::<Vec<_>>(),
        );
    }
    Ok(
        serde_json::json!({"schema":1,"kind":"static_modeldoc_binary_audit","root":root,"native_tools_executed":false,"files_scanned":files.len(),"bytes_scanned":files.iter().map(|f|f["bytes"].as_u64().unwrap_or(0)).sum::<u64>(),"class_evidence_by_module":classes,"scan_complete_without_errors":errors.is_empty()&&skipped.is_empty(),"files":files,"errors":errors,"skipped_links":skipped,"interpretation":"No hits means no searched literals/RTTI in the scanned PE bytes. This is not proof of absence of encoded or dynamically generated names. A hit does not prove registration, load order, ABI compatibility or successful compilation. Procmon text search is not used."}),
    )
}
