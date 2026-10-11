//! Source preflight only; the native welded GPU count remains compiler-owned.
use crate::DmxSummary;
use serde::Serialize;
use std::collections::BTreeSet;

pub const ANIMATED_VERTEX_LIMIT: usize = 524_288;
pub const SAFETY_TARGET: usize = 450_000;

#[derive(Debug, Serialize)]
pub struct MeshLimit {
    pub mesh: String,
    pub source_vertices: usize,
    pub indexed_vertices: usize,
    pub limit: usize,
    pub compiled_vertices: Option<usize>,
    pub over_source_limit: bool,
}
#[derive(Debug, Serialize)]
pub struct Preflight {
    pub meshes: Vec<MeshLimit>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub count_scope: String,
    pub automatic_subdivision: bool,
}

fn grouped(n: usize) -> String {
    let s = n.to_string();
    s.chars()
        .enumerate()
        .fold(String::new(), |mut out, (i, c)| {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
            out
        })
}
pub fn preflight(summary: &DmxSummary, selected: &BTreeSet<String>) -> Preflight {
    let mut result=Preflight{meshes:vec![],errors:vec![],warnings:vec![],count_scope:"DMX source positions; indexed entries are a conservative seam warning, not Valve's compiled count".into(),automatic_subdivision:false};
    for mesh in &summary.meshes {
        if !selected.contains(&mesh.name) || !mesh.has_skin_weights {
            continue;
        }
        let count = mesh.vertices;
        let indexed = mesh.indexed_vertices;
        result.meshes.push(MeshLimit {
            mesh: mesh.name.clone(),
            source_vertices: count,
            indexed_vertices: indexed,
            limit: ANIMATED_VERTEX_LIMIT,
            compiled_vertices: None,
            over_source_limit: count > ANIMATED_VERTEX_LIMIT,
        });
        let label = format!(
            "Skinned DMX mesh {:?}: {} source vertices",
            mesh.name,
            grouped(count)
        );
        let advice=" Choose Automatically split safely, or export separate skinned pieces and map all pieces to the original render node. Aim below about 450,000 vertices per piece, with room for UV/normal/material seams. Skinning must remain enabled; DiscardSkeleton is not a workaround.";
        if count > ANIMATED_VERTEX_LIMIT {
            let message =
                format!("{label} exceeds the 524,288 animated vertex-cache limit.{advice}");
            result.errors.push(message.clone());
            result.warnings.push(message);
        } else if count >= SAFETY_TARGET || indexed > ANIMATED_VERTEX_LIMIT {
            result.warnings.push(format!("{label} ({} indexed entries). Valve may expand vertices at attribute seams; the compiled count is not yet known.{advice}",grouped(indexed)));
        }
    }
    result
}
