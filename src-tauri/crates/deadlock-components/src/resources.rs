//! Existing effect/audio targets follow authoritative hero directories and real references.
use crate::{CancellationToken, Result, audio, compiled, fail, heroes, read, read_file, sha};
use deadlock_core::{safety, text::casefold};
use deadlock_project::customization::ResourceReplacement;
use deadlock_resource::{Annotation, Kind, Value, binary};
use deadlock_vpk::{ResourceLayout, Vpk, resource_blocks, resource_references};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Effects,
    Audio,
}
impl ResourceKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Effects => "effects",
            Self::Audio => "audio",
        }
    }
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Effects => ".vpcf_c",
            Self::Audio => ".vsnd_c",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Association {
    pub association: String,
    pub shared: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogRow {
    pub path: String,
    pub target: String,
    pub label: String,
    #[serde(flatten)]
    pub association: Association,
    pub supported: bool,
    pub reason: String,
    pub original_sha256: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub wav_supported: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEntry {
    pub target: String,
    pub source: PathBuf,
    pub source_sha256: String,
    pub original_sha256: String,
    pub output_sha256: String,
    pub method: String,
    #[serde(flatten)]
    pub association: Association,
    pub allow_shared: bool,
    pub verification: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_percent: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_fit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_edit: Option<audio::EditReport>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReceipt {
    pub schema: u32,
    pub kind: ResourceKind,
    pub hero_path: String,
    pub entries: Vec<ResourceEntry>,
    pub native_validated: bool,
    pub in_game_validated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectVerification {
    pub status: String,
    pub resource_version: u16,
    pub schema_guid: String,
    pub renderers: usize,
    pub native_validated: bool,
    pub in_game_validated: bool,
}
fn graph_references(raw: &[u8]) -> Result<BTreeSet<String>> {
    let mut refs: BTreeSet<_> = resource_references(raw)
        .map_err(fail)?
        .into_iter()
        .collect();
    fn walk(value: &Value, refs: &mut BTreeSet<String>) -> Result<()> {
        match &value.kind {
            Kind::Object(rows) => {
                for (_, v) in rows {
                    walk(v, refs)?;
                }
            }
            Kind::Array(rows) => {
                for v in rows {
                    walk(v, refs)?;
                }
            }
            Kind::String(path) => {
                if [".vsnd", ".vpcf", ".vsnd_c", ".vpcf_c"]
                    .iter()
                    .any(|s| casefold(path).ends_with(s))
                {
                    refs.insert(safety::relative_asset(path)?);
                }
            }
            _ => {}
        }
        Ok(())
    }
    for block in resource_blocks(raw).map_err(fail)?.blocks {
        if block.name != "DATA" {
            continue;
        }
        let body = &raw[block.offset..block.offset + block.size];
        if body.starts_with(b"\x013VK") || body.starts_with(b"\x053VK") {
            walk(&binary::decode(body).map_err(fail)?.root, &mut refs)?;
        }
    }
    Ok(refs)
}
pub fn catalog_paths(
    archive: &Vpk,
    original_model: &str,
    kind: ResourceKind,
    cancel: &CancellationToken,
) -> Result<BTreeMap<String, Association>> {
    let model = compiled(original_model)?;
    if !archive.entries.contains_key(&casefold(&model)) {
        return Err(fail("Selected hero is absent from the current game VPK."));
    }
    let tokens = heroes::tokens(original_model)?;
    let mut seeds: BTreeSet<String> = [model].into();
    let mut owned = BTreeSet::new();
    for entry in archive.entries.values() {
        cancel.check()?;
        let path = &entry.path;
        let folded = casefold(path);
        if heroes::owned(path, &tokens) {
            if folded.ends_with(kind.suffix()) {
                owned.insert(path.clone());
            }
            if [".vpcf_c", ".vsndevts_c", ".vnmgraph_c"]
                .iter()
                .any(|s| folded.ends_with(s))
            {
                seeds.insert(path.clone());
            }
        } else if folded.ends_with(".vsndevts_c") {
            let stem = folded
                .rsplit('/')
                .next()
                .unwrap()
                .trim_end_matches(".vsndevts_c")
                .trim_start_matches("hero_")
                .trim_start_matches("citadel_");
            if tokens.contains(stem) {
                seeds.insert(path.clone());
            }
        }
    }
    let (mut pending, mut visited, mut linked) = (
        seeds.into_iter().collect::<Vec<_>>(),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    while let Some(path) = pending.pop() {
        cancel.check()?;
        let requested = compiled(&path)?;
        let Some(entry) = archive.entries.get(&casefold(&requested)) else {
            continue;
        };
        if !visited.insert(entry.path.clone()) {
            continue;
        }
        if visited.len() > 20000 {
            return Err(fail(
                "Character component discovery exceeds 20,000 linked resources.",
            ));
        }
        let folded = casefold(&entry.path);
        if folded.ends_with(kind.suffix()) {
            linked.insert(entry.path.clone());
        }
        if [
            ".vmdl_c",
            ".vpcf_c",
            ".vsndevts_c",
            ".vnmgraph_c",
            ".vnmclip_c",
        ]
        .iter()
        .any(|s| folded.ends_with(s))
        {
            pending.extend(graph_references(&read(archive, &entry.path)?)?);
        }
    }
    let mut result = BTreeMap::new();
    for path in owned.union(&linked) {
        let shared = !owned.contains(path);
        result.insert(
            path.clone(),
            Association {
                association: if shared {
                    "resource_reference"
                } else {
                    "hero_directory"
                }
                .into(),
                shared,
            },
        );
    }
    Ok(result)
}
struct Particle {
    layout: ResourceLayout,
    guid: [u8; 16],
    root: Value,
}
fn particle(raw: &[u8]) -> Result<Particle> {
    let layout = resource_blocks(raw).map_err(fail)?;
    if layout.declared_size != raw.len() {
        return Err(fail(
            "Particle definition has unsupported external or trailing data.",
        ));
    }
    let names: BTreeSet<_> = layout.blocks.iter().map(|b| b.name.as_str()).collect();
    if names.len() != layout.blocks.len() || !names.contains("DATA") {
        return Err(fail(
            "Particle definition requires unique resource blocks and one DATA block.",
        ));
    }
    let b = layout.blocks.iter().find(|b| b.name == "DATA").unwrap();
    let doc = binary::decode(&raw[b.offset..b.offset + b.size]).map_err(fail)?;
    let root = doc.root;
    root.object().map_err(fail)?;
    if !["m_Renderers", "m_Children", "m_Emitters"]
        .iter()
        .any(|key| root.get(key).is_some())
    {
        return Err(fail(
            "Resource DATA is not a supported particle-system definition.",
        ));
    }
    if let Some(class) = root.get("_class") {
        if class.string().map_err(fail)? != "CParticleSystemDefinition" {
            return Err(fail(
                "Resource DATA has an incompatible particle-system class.",
            ));
        }
    }
    for key in [
        "m_Renderers",
        "m_Children",
        "m_Emitters",
        "m_Initializers",
        "m_Operators",
        "m_Constraints",
        "m_ForceGenerators",
        "m_PreEmissionOperators",
    ] {
        if let Some(v) = root.get(key) {
            let rows = v.array().map_err(fail)?;
            if rows.len() > 10000 || rows.iter().any(|v| !matches!(v.kind, Kind::Object(_))) {
                return Err(fail(format!(
                    "Particle definition has an invalid function collection: {key}"
                )));
            }
        }
    }
    for key in ["m_nMaxParticles", "m_nInitialParticles"] {
        if let Some(v) = root.get(key) {
            if !(0..=1_000_000).contains(&v.integer().map_err(fail)?) {
                return Err(fail(
                    "Particle definition has an invalid particle allocation count.",
                ));
            }
        }
    }
    Ok(Particle {
        layout,
        guid: doc.format_guid,
        root,
    })
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Key(String),
    Index(usize),
}
fn contract(value: &Value) -> BTreeMap<Vec<Part>, Value> {
    fn walk(value: &Value, path: &mut Vec<Part>, out: &mut BTreeMap<Vec<Part>, Value>) {
        match &value.kind {
            Kind::Object(rows) => {
                for (key, value) in rows {
                    path.push(Part::Key(key.clone()));
                    let name = casefold(key);
                    if [
                        "_class",
                        "m_nBehaviorVersion",
                        "m_Children",
                        "m_controlPointConfigurations",
                    ]
                    .contains(&key.as_str())
                        || [
                            "controlpoint",
                            "inputcp",
                            "outputcp",
                            "snapshot",
                            "attachment",
                        ]
                        .iter()
                        .any(|s| name.contains(s))
                        || name.ends_with("cp")
                    {
                        out.insert(
                            path.iter()
                                .map(|v| match v {
                                    Part::Key(k) => Part::Key(k.clone()),
                                    Part::Index(i) => Part::Index(*i),
                                })
                                .collect(),
                            value.clone(),
                        );
                    } else {
                        walk(value, path, out);
                    }
                    path.pop();
                }
            }
            Kind::Array(rows) => {
                for (i, value) in rows.iter().enumerate() {
                    path.push(Part::Index(i));
                    walk(value, path, out);
                    path.pop();
                }
            }
            Kind::String(s) => {
                if value.annotation != Annotation::Unspecified
                    || [
                        ".vmat", ".vtex", ".vmdl", ".vpcf", ".vsnap", ".vmat_c", ".vtex_c",
                        ".vmdl_c", ".vpcf_c", ".vsnap_c",
                    ]
                    .iter()
                    .any(|suffix| casefold(s).ends_with(suffix))
                {
                    out.insert(
                        path.iter()
                            .map(|v| match v {
                                Part::Key(k) => Part::Key(k.clone()),
                                Part::Index(i) => Part::Index(*i),
                            })
                            .collect(),
                        value.clone(),
                    );
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeMap::new();
    walk(value, &mut vec![], &mut out);
    out
}
pub fn validate_effect(original: &[u8], replacement: &[u8]) -> Result<EffectVerification> {
    let old = particle(original)?;
    let new = particle(replacement)?;
    if old.layout.resource_version != new.layout.resource_version || old.guid != new.guid {
        return Err(fail(
            "Particle replacement changes the original resource or particle schema version.",
        ));
    }
    let a = contract(&old.root);
    let b = contract(&new.root);
    if a.len() != b.len()
        || a.iter()
            .any(|(k, v)| b.get(k).is_none_or(|other| !v.canonical_eq(other)))
    {
        return Err(fail(
            "Particle replacement changes the original function classes, control points, attachments or child-system contract.",
        ));
    }
    let mut a = resource_references(original).map_err(fail)?;
    let mut b = resource_references(replacement).map_err(fail)?;
    a.sort();
    b.sort();
    if a != b {
        return Err(fail(
            "Particle replacement changes original resource dependencies. Author against the original materials and child systems.",
        ));
    }
    fn opaque<'a>(raw: &'a [u8], layout: &'a ResourceLayout) -> Vec<(&'a str, &'a [u8])> {
        layout
            .blocks
            .iter()
            .filter(|b| !["DATA", "REDI", "RED2", "RERL"].contains(&b.name.as_str()))
            .map(|b| (b.name.as_str(), &raw[b.offset..b.offset + b.size]))
            .collect()
    }
    if opaque(original, &old.layout) != opaque(replacement, &new.layout) {
        return Err(fail(
            "Particle replacement changes unsupported opaque resource data.",
        ));
    }
    Ok(EffectVerification {
        status: "verified".into(),
        resource_version: new.layout.resource_version,
        schema_guid: new.guid.iter().map(|b| format!("{b:02x}")).collect(),
        renderers: new
            .root
            .get("m_Renderers")
            .map(|v| v.array().map(|a| a.len()))
            .transpose()
            .map_err(fail)?
            .unwrap_or(0),
        native_validated: false,
        in_game_validated: false,
    })
}
fn verify(kind: ResourceKind, original: &[u8], output: &[u8]) -> Result<serde_json::Value> {
    match kind {
        ResourceKind::Audio => serde_json::to_value(audio::validate_rendered_sound(original, output)?),
        ResourceKind::Effects => serde_json::to_value(validate_effect(original, output)?),
    }
    .map_err(fail)
}
pub fn discover(
    archive: &Vpk,
    model: &str,
    kind: ResourceKind,
    cancel: &CancellationToken,
) -> Result<Vec<CatalogRow>> {
    let mut result = vec![];
    for (path, association) in catalog_paths(archive, model, kind, cancel)? {
        cancel.check()?;
        let mut label = path
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(kind.suffix())
            .replace('_', " ");
        if association.shared {
            label += " (potentially shared)";
        }
        let mut row = CatalogRow {
            target: path.clone(),
            path: path.clone(),
            label,
            association,
            supported: false,
            reason: String::new(),
            original_sha256: None,
            metadata: None,
            wav_supported: false,
        };
        let checked: Result<()> = (|| {
            let raw = read(archive, &path)?;
            row.original_sha256 = Some(sha(&raw));
            if kind == ResourceKind::Audio {
                let meta = audio::metadata(&raw)?;
                row.wav_supported = ["PCM8", "PCM16"].contains(&meta.format.as_str())
                    || (meta.format == "MP3" && (meta.kind == "CTRL" || meta.resource_version >= 3 || meta.loop_start == -1));
                row.metadata = Some(serde_json::to_value(meta).map_err(fail)?);
            } else {
                row.metadata = Some(verify(kind, &raw, &raw)?);
            }
            row.supported = true;
            Ok(())
        })();
        if let Err(error) = checked {
            cancel.check()?;
            row.reason = error.to_string();
        }
        result.push(row);
    }
    Ok(result)
}
pub fn stage(
    archive: &Vpk,
    model: &str,
    replacements: &[ResourceReplacement],
    game: &Path,
    kind: ResourceKind,
    cancel: &CancellationToken,
) -> Result<ResourceReceipt> {
    let mut receipt = ResourceReceipt {
        schema: 1,
        kind,
        hero_path: model.into(),
        entries: vec![],
        native_validated: false,
        in_game_validated: false,
    };
    let selected: Vec<_> = replacements
        .iter()
        .filter(|r| r.enabled.unwrap_or(true))
        .collect();
    if selected.is_empty() {
        return Ok(receipt);
    }
    let allowed = catalog_paths(archive, model, kind, cancel)?;
    let lookup: BTreeMap<_, _> = allowed.iter().map(|(k, v)| (casefold(k), (k, v))).collect();
    let mut seen = BTreeSet::new();
    for item in selected {
        cancel.check()?;
        let requested = safety::relative_asset(item.target.as_deref().unwrap_or_default())?;
        let (target, association) = lookup.get(&casefold(&requested)).ok_or_else(|| {
            fail(format!(
                "Select a discovered {} target for the current hero: {requested}",
                kind.name()
            ))
        })?;
        if !seen.insert(*target) {
            return Err(fail(
                "Multiple replacements select the same character asset.",
            ));
        }
        if association.shared && !item.allow_shared.unwrap_or(false) {
            return Err(fail(format!(
                "This linked asset may also affect other heroes. Enable the shared-asset choice explicitly: {target}"
            )));
        }
        let source = Path::new(item.source.as_deref().unwrap_or_default());
        let original = read(archive, target)?;
        if item.original_sha256.as_ref().is_some_and(|h| !h.is_empty() && *h != sha(&original)) {
            return Err(fail("This game asset changed since selection. Reload character assets before building."));
        }
        let raw = read_file(source)?;
        let extension = source
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mut audio_edit = None;
        let (output, method) = if kind == ResourceKind::Audio {
            let edited = audio::prepare_audio(&original, &raw, &extension,
                item.volume_percent.unwrap_or(100), item.audio_fit.as_deref().unwrap_or("exact"), cancel)?;
            audio_edit = Some(edited.report);
            (edited.resource, if extension == "vsnd_c" {"compiled_resource"} else {"pcm_samples"})
        } else if format!(".{extension}") == kind.suffix() {
            (raw.clone(), "compiled_resource")
        } else {
            return Err(fail(if kind == ResourceKind::Audio {
                "Choose PCM .wav or authored .vsnd_c."
            } else {
                "Choose authored .vpcf_c. Uncompiled descriptors require a verified native authoring workflow."
            }));
        };
        let verification = verify(kind, &original, &output)?;
        safety::atomic_write(&safety::inside(game, target)?, &output)?;
        receipt.entries.push(ResourceEntry {
            target: (*target).clone(),
            source: safety::resolve_path(source)?,
            source_sha256: sha(&raw),
            original_sha256: sha(&original),
            output_sha256: sha(&output),
            method: method.into(),
            association: (*association).clone(),
            allow_shared: item.allow_shared.unwrap_or(false),
            verification,
            volume_percent: (kind == ResourceKind::Audio).then_some(item.volume_percent.unwrap_or(100)),
            audio_fit: (kind == ResourceKind::Audio).then(|| item.audio_fit.clone().unwrap_or_else(|| "exact".into())),
            audio_edit,
        });
    }
    Ok(receipt)
}
pub fn verify_compiled(
    archive: &Vpk,
    receipt: &ResourceReceipt,
    game: &Path,
    cancel: &CancellationToken,
) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    if receipt.entries.is_empty() {
        return Ok(files);
    }
    let allowed = catalog_paths(archive, &receipt.hero_path, receipt.kind, cancel)?;
    for item in &receipt.entries {
        cancel.check()?;
        let target = safety::relative_asset(&item.target)?;
        let association = allowed
            .get(&target)
            .ok_or_else(|| fail("Character asset no longer belongs to this hero."))?;
        if files.contains_key(&target) {
            return Err(fail("Character asset target is duplicated."));
        }
        if association.shared && !item.allow_shared {
            return Err(fail("Shared character asset was not explicitly selected."));
        }
        let original = read(archive, &target)?;
        let raw = read_file(&safety::inside(game, &target)?)?;
        if sha(&raw) != item.output_sha256 {
            return Err(fail(format!(
                "Character component output changed after import: {target}"
            )));
        }
        if sha(&original) != item.original_sha256 {
            return Err(fail(format!(
                "The original character asset changed. Rebuild against the current game: {target}"
            )));
        }
        verify(receipt.kind, &original, &raw)?;
        files.insert(target, item.output_sha256.clone());
    }
    Ok(files)
}
