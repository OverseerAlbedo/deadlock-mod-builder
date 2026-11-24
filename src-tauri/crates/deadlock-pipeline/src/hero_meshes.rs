//! Which of a hero model's decompiled meshes count as the hero as the game
//! first shows it: the render meshes of LOD 0 (`lod0`), without the meshes only
//! another choice of a body group shows, each read once per file through the
//! preview reader and kept only where the render mesh's import filter keeps it.
//! The ONE choice for the Colors page's 3D view of the game hero
//! (`src-tauri/src/catalogs.rs scene::build`) and for the build of color ranges
//! that change one part (`HeroPartMeshes`), so both see the same meshes.
use crate::carried::{body_groups, filter_keeps};
use deadlock_core::{safety, CancellationToken, CoreError, Result};
use deadlock_preparation::deadlock_preview::{self, BufferSlice, Preview};
use deadlock_project::{
    color_parts::{PartMesh, NO_PARTS},
    Project,
};
use deadlock_resource::text::{Document, Node};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn scalar(node: &Node, key: &str) -> Option<String> {
    node.get(key).and_then(|v| v.scalar()).map(str::to_owned)
}

/// A lower detail level by name (`_lod1`, `LOD2` …; `deadlock_model::lod_base`
/// without lod 0), used only when a model has no `LODGroup`.
pub fn lower_lod(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let digits = lower.len() - lower.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (rest, number) = lower.split_at(lower.len() - digits);
    let rest = rest.strip_suffix(['_', '.', ' ', '-']).unwrap_or(rest);
    digits > 0 && rest.ends_with("lod") && number.trim_start_matches('0') != ""
}
/// The render meshes of LOD 0: the `LODGroup` with the lowest
/// `switch_threshold`, or, when the model has no LOD group, every render
/// mesh that is not a lower detail level (`lower_lod`).
pub fn lod0(document: &Document, render: &[String]) -> Vec<String> {
    let groups = document.objects(Some("LODGroup"));
    let lowest = groups
        .iter()
        .filter_map(|g| Some((scalar(g, "switch_threshold")?.parse::<f64>().ok()?, *g)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    match lowest {
        Some((_, group)) => group
            .get("mesh_references")
            .and_then(|m| m.array())
            .map(|a| a.iter().filter_map(|r| scalar(r, "mesh_name")).collect())
            .unwrap_or_default(),
        None => render.iter().filter(|name| !lower_lod(name)).cloned().collect(),
    }
}

/// One step of `choose`, in the model's render-mesh order.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// A render mesh only another choice of `group` shows (`choice`).
    OtherChoice { node: String, group: String, choice: String },
    /// A render mesh whose file could not be read.
    Unreadable { node: String, file: String },
    /// A render mesh of LOD 0 the game shows: the meshes of its file that
    /// its import filter keeps (`meshes`, places in the preview's mesh list)
    /// and leaves out (`left_out`, their source names), in file order.
    Node { node: String, file: String, body_group: Option<String>, meshes: Vec<usize>, left_out: Vec<String> },
}
/// The chosen meshes: the steps and the preview of every file read (`None`: unreadable).
pub struct Chosen {
    pub steps: Vec<Step>,
    pub previews: BTreeMap<String, Option<Preview>>,
}

/// The meshes of the decompiled hero model `document` (sources in `base`).
pub fn choose(base: &Path, document: &Document, cancel: &CancellationToken) -> Result<Chosen> {
    let render: Vec<(String, String, Option<&Node>)> = document
        .objects(Some("RenderMeshFile"))
        .into_iter()
        .filter_map(|n| Some((scalar(n, "name")?, scalar(n, "filename")?, n.get("import_filter"))))
        .collect();
    let names: Vec<String> = render.iter().map(|r| r.0.clone()).collect();
    let lod0: BTreeSet<String> = lod0(document, &names).into_iter().collect();
    let groups = body_groups(document);
    let mut previews: BTreeMap<String, Option<Preview>> = BTreeMap::new();
    let mut steps = vec![];
    for (node, file, filter) in &render {
        cancel.check()?;
        if !lod0.contains(node) {
            continue;
        }
        if let Some((group, _, other)) = groups.iter().find(|(_, _, other)| other.contains_key(node)) {
            steps.push(Step::OtherChoice { node: node.clone(), group: group.clone(), choice: other[node].clone() });
            continue;
        }
        let body_group = groups.iter().find(|(_, shown, _)| shown.contains(node)).map(|g| g.0.clone());
        let preview = previews.entry(file.clone()).or_insert_with(|| {
            let path = safety::inside(base, file).ok()?;
            let dmx = deadlock_dmx::Dmx::load_with_cancel(&path, cancel).ok()?;
            deadlock_preview::from_dmx(&dmx, file.clone(), cancel).ok()
        });
        cancel.check()?;
        let Some(preview) = preview.as_ref() else {
            steps.push(Step::Unreadable { node: node.clone(), file: file.clone() });
            continue;
        };
        let (mut meshes, mut left_out) = (vec![], vec![]);
        for (index, info) in preview.metadata.meshes.iter().enumerate() {
            if filter_keeps(*filter, &info.source_name) {
                meshes.push(index);
            } else {
                left_out.push(info.source_name.clone());
            }
        }
        steps.push(Step::Node { node: node.clone(), file: file.clone(), body_group, meshes, left_out });
    }
    Ok(Chosen { steps, previews })
}

fn floats(data: &[u8], slice: &BufferSlice) -> Vec<f32> {
    data.get(slice.offset..slice.offset + slice.count * 4)
        .unwrap_or_default()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
fn words(data: &[u8], slice: &BufferSlice) -> Vec<u32> {
    data.get(slice.offset..slice.offset + slice.count * 4)
        .unwrap_or_default()
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
/// The chosen meshes that use `material` (a `.vmat` path, compared ignoring
/// case, as the Colors page matches its catalog rows), with texture
/// coordinates, in the order the 3D view lists them.
pub fn part_meshes(chosen: &Chosen, material: &str) -> Vec<PartMesh> {
    let wanted = deadlock_core::text::casefold(material);
    let mut out = vec![];
    for step in &chosen.steps {
        let Step::Node { file, meshes, .. } = step else { continue };
        let Some(Some(preview)) = chosen.previews.get(file) else { continue };
        for index in meshes {
            let info = &preview.metadata.meshes[*index];
            let Some(uvs) = &info.uvs else { continue };
            if deadlock_core::text::casefold(&info.material) != wanted {
                continue;
            }
            out.push(PartMesh { positions: floats(&preview.data, &info.positions), uvs: floats(&preview.data, uvs), indices: words(&preview.data, &info.indices) });
        }
    }
    out
}
/// The meshes of `preview` that use `material`, with texture coordinates.
fn preview_part_meshes(preview: &Preview, material: &str) -> Vec<PartMesh> {
    let wanted = deadlock_core::text::casefold(material);
    preview
        .metadata
        .meshes
        .iter()
        .filter(|info| info.uvs.is_some() && deadlock_core::text::casefold(&info.material) == wanted)
        .map(|info| PartMesh {
            positions: floats(&preview.data, &info.positions),
            uvs: floats(&preview.data, info.uvs.as_ref().unwrap()),
            indices: words(&preview.data, &info.indices),
        })
        .collect()
}
/// The prepared replacement model's meshes (`project.dmx`) for color ranges
/// that change one part, read as the Colors page's 3D view reads that model
/// (`replacement_color_scene`: the preview of the DMX), once.
pub struct PreparedPartMeshes<'a> {
    pub dmx: std::path::PathBuf,
    pub cancel: &'a CancellationToken,
    preview: std::sync::OnceLock<std::result::Result<Preview, String>>,
}
impl<'a> PreparedPartMeshes<'a> {
    pub fn new(dmx: impl Into<std::path::PathBuf>, cancel: &'a CancellationToken) -> Self {
        Self { dmx: dmx.into(), cancel, preview: std::sync::OnceLock::new() }
    }
}
impl deadlock_project::color_parts::PartMeshes for PreparedPartMeshes<'_> {
    fn part_meshes(&self, material: &str) -> std::result::Result<Vec<PartMesh>, String> {
        let preview = self.preview.get_or_init(|| {
            let document = deadlock_dmx::Dmx::load_with_cancel(&self.dmx, self.cancel).map_err(|e| format!("{NO_PARTS} {e}"))?;
            deadlock_preview::from_dmx(&document, self.dmx.display().to_string(), self.cancel).map_err(|e: CoreError| e.to_string())
        });
        preview.as_ref().map(|p| preview_part_meshes(p, material)).map_err(Clone::clone)
    }
}

/// The game hero's meshes for color ranges that change one part: the hero's
/// decompiled sources (`SourceService::ensure_base`, the cache the Colors
/// page's 3D view made), read once per build.
pub struct HeroPartMeshes<'a> {
    pub sources: crate::sources::SourceService<'a>,
    chosen: std::sync::Mutex<Option<(String, std::sync::Arc<Chosen>)>>,
}
impl<'a> HeroPartMeshes<'a> {
    pub fn new(sources: crate::sources::SourceService<'a>) -> Self {
        Self { sources, chosen: std::sync::Mutex::new(None) }
    }
    fn chosen(&self, project: &Project) -> Result<std::sync::Arc<Chosen>> {
        let mut kept = self.chosen.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((model, chosen)) = kept.as_ref() {
            if *model == project.original_model {
                return Ok(chosen.clone());
            }
        }
        let base = self.sources.ensure_base(project)?;
        let path = safety::inside(&base, project.source_model())?;
        let text = std::fs::read_to_string(&path).map_err(|e| CoreError::io("The hero's decompiled model could not be read", e))?;
        let document = Document::parse(text.as_str()).map_err(|e| CoreError::Integrity(format!("The hero's decompiled model could not be read: {e}")))?;
        let chosen = std::sync::Arc::new(choose(&base, &document, self.sources.cancel)?);
        *kept = Some((project.original_model.clone(), chosen.clone()));
        Ok(chosen)
    }
}
impl deadlock_components::service::HeroMeshes for HeroPartMeshes<'_> {
    fn hero_meshes(&self, project: &Project, material: &str) -> std::result::Result<Vec<PartMesh>, String> {
        let chosen = self.chosen(project).map_err(|e| e.to_string())?;
        Ok(part_meshes(&chosen, material))
    }
}
