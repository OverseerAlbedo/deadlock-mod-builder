//! Existing effect/audio targets follow authoritative hero directories and real references.
use crate::{CancellationToken, Result, audio, compiled, fail, heroes, read, read_file, sha, sound_tags};
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
    /// Sounds only: tags, readable name, group and variation family (`sound_tags`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<sound_tags::SoundInfo>,
}
/// Association of another hero's voice line that names this hero
/// (`sounds/vo/<other hero>/…_hornet_…`). Shared: replacing it changes what the
/// other hero says, so it needs the explicit shared-asset choice.
pub const OTHER_HERO_VOICE: &str = "other_hero_voice";
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
    /// A silenced sound: no source file (`source` and `source_sha256` empty),
    /// the output is the original's timeline as silence. Absent otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub silenced: bool,
    /// The sound edit the output was rendered with (`audio::render_edit`).
    /// Absent without one, so other receipts stay as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound_edit: Option<deadlock_project::customization::SoundEdit>,
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
/// Particle systems the hero's model names, directly or through its animation
/// graphs, clips and sound events (casefolded compiled paths). Particle
/// systems are not followed into their children here. Only the files' resource
/// lists (RERL) are read, not their DATA: on four heroes this found exactly what
/// decoding every DATA block found, at a quarter of the time (Violet's clips).
pub fn model_particles(archive: &Vpk, original_model: &str, cancel: &CancellationToken) -> Result<BTreeSet<String>> {
    let model = compiled(original_model)?;
    let (mut pending, mut visited, mut found) = (vec![model], BTreeSet::new(), BTreeSet::new());
    while let Some(path) = pending.pop() {
        cancel.check()?;
        let Ok(requested) = compiled(&path) else { continue };
        let key = casefold(&requested);
        let Some(entry) = archive.entries.get(&key) else { continue };
        if key.ends_with(".vpcf_c") {
            found.insert(key);
            continue;
        }
        if !visited.insert(key.clone()) || visited.len() > 20000 {
            continue;
        }
        if [".vmdl_c", ".vsndevts_c", ".vnmgraph_c", ".vnmclip_c"].iter().any(|s| key.ends_with(s)) {
            if let Ok(refs) = read(archive, &entry.path).and_then(|raw| resource_references(&raw).map_err(fail)) {
                pending.extend(refs);
            }
        }
    }
    Ok(found)
}
pub fn catalog_paths(
    archive: &Vpk,
    original_model: &str,
    kind: ResourceKind,
    cancel: &CancellationToken,
) -> Result<BTreeMap<String, Association>> {
    Ok(collect(archive, original_model, kind, cancel)?.0)
}
/// Other heroes' voice lines that name this hero: a `.vsnd_c` under
/// `sounds/vo/<folder>/` (not one of the hero's own folders) whose file name has
/// one of the hero's names as a whole word (`ghost_kill_hornet_01`).
fn names_hero(folded: &str, tokens: &BTreeSet<String>) -> bool {
    folded.starts_with("sounds/vo/")
        && folded.ends_with(".vsnd_c")
        && heroes::matches(
            folded.rsplit('/').next().unwrap_or_default().trim_end_matches(".vsnd_c"),
            tokens,
        )
}
/// The catalog and the sound event documents visited while finding it.
fn collect(
    archive: &Vpk,
    original_model: &str,
    kind: ResourceKind,
    cancel: &CancellationToken,
) -> Result<(BTreeMap<String, Association>, Vec<String>)> {
    let model = compiled(original_model)?;
    if !archive.entries.contains_key(&casefold(&model)) {
        return Err(fail("Selected hero is absent from the current game VPK."));
    }
    let tokens = heroes::tokens(original_model)?;
    let mut seeds: BTreeSet<String> = [model].into();
    let mut owned = BTreeSet::new();
    let mut voices = BTreeSet::new();
    for entry in archive.entries.values() {
        cancel.check()?;
        let path = &entry.path;
        let folded = casefold(path);
        let own = heroes::owned(path, &tokens);
        if kind == ResourceKind::Audio && !own && names_hero(&folded, &tokens) {
            voices.insert(path.clone());
        }
        if own {
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
    for path in voices {
        result.entry(path).or_insert_with(|| Association {
            association: OTHER_HERO_VOICE.into(),
            shared: true,
        });
    }
    let events = visited
        .into_iter()
        .filter(|p| casefold(p).ends_with(".vsndevts_c"))
        .collect();
    Ok((result, events))
}
/// The DATA roots of sound event documents; a document that cannot be read is
/// skipped (its sounds fall back to names for families).
pub fn event_documents(archive: &Vpk, paths: &[String]) -> Vec<Value> {
    let mut docs = vec![];
    for path in paths {
        let Ok(raw) = read(archive, path) else { continue };
        let Ok(layout) = resource_blocks(&raw) else { continue };
        for block in layout.blocks {
            if block.name != "DATA" || block.offset + block.size > raw.len() {
                continue;
            }
            if let Ok(doc) = binary::decode(&raw[block.offset..block.offset + block.size]) {
                docs.push(doc.root);
            }
        }
    }
    docs
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
    let (paths, event_paths) = collect(archive, model, kind, cancel)?;
    let sounds = if kind == ResourceKind::Audio {
        let events = sound_tags::Events::from_documents(&event_documents(archive, &event_paths));
        cancel.check()?;
        let tokens = heroes::tokens(model)?;
        let name = heroes::display_name(model).unwrap_or_else(|| {
            heroes::strip_version(&casefold(model).rsplit('/').next().unwrap_or_default().replace(".vmdl_c", "").replace(".vmdl", ""))
        });
        let rows: Vec<(String, sound_tags::Owner)> = paths
            .iter()
            .map(|(path, a)| {
                let owner = if a.association == OTHER_HERO_VOICE {
                    sound_tags::Owner::OtherHeroVoice
                } else if a.shared {
                    sound_tags::Owner::Linked
                } else {
                    sound_tags::Owner::Own
                };
                (path.clone(), owner)
            })
            .collect();
        sound_tags::classify_all(&rows, &sound_tags::Hero { tokens: &tokens, name: &name }, &events)
    } else {
        vec![]
    };
    let mut sounds = sounds.into_iter();
    for (path, association) in paths {
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
            sound: sounds.next(),
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
        // The saved format's rules for a sound edit, again before anything is written.
        item.check_sound_edit(kind.name()).map_err(fail)?;
        item.check_made(kind.name()).map_err(fail)?;
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
        if kind == ResourceKind::Audio && item.silenced() {
            let silent = audio::silent_audio(&original, cancel)?;
            let verification = verify(kind, &original, &silent.resource)?;
            safety::atomic_write(&safety::inside(game, target)?, &silent.resource)?;
            receipt.entries.push(ResourceEntry {
                target: (*target).clone(),
                source: PathBuf::new(),
                source_sha256: String::new(),
                original_sha256: sha(&original),
                output_sha256: sha(&silent.resource),
                method: "silence".into(),
                association: (*association).clone(),
                allow_shared: item.allow_shared.unwrap_or(false),
                verification,
                volume_percent: Some(item.volume_percent.unwrap_or(100)),
                audio_fit: Some(item.audio_fit.clone().unwrap_or_else(|| "exact".into())),
                audio_edit: Some(silent.report),
                silenced: true,
                sound_edit: None,
            });
            continue;
        }
        let extension = source
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mut audio_edit = None;
        // A sound edit takes its own path, streaming its file (a song can be far
        // more than 128 MiB of audio); a row without one reads and encodes as before.
        let sound_edit = if kind == ResourceKind::Audio {item.sound_edit()} else {None};
        let (raw, edited_file) = match &sound_edit {
            Some(edit) => (Vec::new(), Some(audio::prepare_edited_file(&original, source, &extension, item.volume_percent.unwrap_or(100), edit, cancel)?)),
            None => (read_file(source)?, None),
        };
        let mut source_sha = None;
        let (output, method) = if let Some((digest, edited)) = edited_file {
            source_sha = Some(digest);
            audio_edit = Some(edited.report);
            (edited.resource, "pcm_samples")
        } else if kind == ResourceKind::Audio {
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
            source_sha256: source_sha.unwrap_or_else(|| sha(&raw)),
            original_sha256: sha(&original),
            output_sha256: sha(&output),
            method: method.into(),
            association: (*association).clone(),
            allow_shared: item.allow_shared.unwrap_or(false),
            verification,
            volume_percent: (kind == ResourceKind::Audio).then_some(item.volume_percent.unwrap_or(100)),
            audio_fit: (kind == ResourceKind::Audio).then(|| item.audio_fit.clone().unwrap_or_else(|| "exact".into())),
            audio_edit,
            silenced: false,
            sound_edit,
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
        if item.silenced && (receipt.kind != ResourceKind::Audio || !audio::is_silent(&raw)?) {
            return Err(fail(format!("A silenced sound is no longer silent. Rebuild before installing: {target}")));
        }
        files.insert(target, item.output_sha256.clone());
    }
    Ok(files)
}
#[cfg(test)]
mod other_hero_voice_tests {
    use super::*;
    #[test]
    fn other_heroes_lines_that_name_the_hero() {
        let vindicta = heroes::tokens("models/heroes_staging/hornet_v3/hornet.vmdl").unwrap();
        for path in [
            "sounds/vo/ghost/ghost_kill_hornet_01.vsnd_c",
            "sounds/vo/ghost/ghost_ally_vindicta_killed_in_lane_01.vsnd_c",
            "sounds/vo/astro/ping/astro_ping_hornet_almost_respawn.vsnd_c",
            "sounds/vo/haze/haze_match_start_haze_hornet_convo01_01.vsnd_c",
        ] {
            assert!(names_hero(path, &vindicta), "{path}");
        }
        for path in [
            // The hero name must be a whole word of the file name, not of a folder.
            "sounds/vo/ghost/ghost_kill_haze_01.vsnd_c",
            "sounds/vo/hornetish/hornetish_kill_01.vsnd_c",
            "sounds/abilities/ghost/ghost_hornet_01.vsnd_c",
            "sounds/vo/ghost/ghost_kill_hornet_01.vsndevts_c",
        ] {
            assert!(!names_hero(path, &vindicta), "{path}");
        }
    }
    #[test]
    fn generic_hero_words_match_only_as_whole_words() {
        // Across the roster every match is in a hero position; these real file
        // names hold short names inside other words.
        let tokens = |model: &str| heroes::tokens(model).unwrap();
        for (model, path, expected) in [
            ("models/heroes_wip/abrams/abrams.vmdl", "sounds/vo/shopkeeper/shopkeeper_hotdog_buy_siphon_bullets_01.vsnd_c", false),
            ("models/heroes_wip/familiar/familiar_wip.vmdl", "sounds/vo/announcer/female_patron/patron_female_tutorial_buy_mod_reminder.vsnd_c", false),
            ("models/heroes_wip/vampirebat/vampirebat.vmdl", "sounds/vo/announcer/female_patron/patron_female_tutorial_stamina_reminder.vsnd_c", false),
            ("models/heroes_staging/bebop/bebop.vmdl", "sounds/vo/lash/lash_win_with_bebop_01.vsnd_c", true),
            ("models/heroes_staging/digger/digger.vmdl", "sounds/vo/doorman/doorman_power4_on_krill_01_short.vsnd_c", true),
            ("models/heroes_staging/kelvin_v2/kelvin.vmdl", "sounds/vo/gigawatt/gigawatt_storm_cloud_kelvin_survives_01.vsnd_c", true),
            ("models/heroes_staging/mirage_v2/mirage.vmdl", "sounds/vo/shopkeeper/shopkeeper_hotdog_t4_mirage_buys_carpet.vsnd_c", true),
        ] {
            assert_eq!(names_hero(path, &tokens(model)), expected, "{model} {path}");
        }
    }
}
