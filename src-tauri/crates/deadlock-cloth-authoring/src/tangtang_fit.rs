//! Refit the reviewed TangTang proxy after a source-proportions conversion.
//! Only rigid placement of complete secondary groups is supported. The render
//! DMX is immutable, and all proxy skinning, topology and motion maps survive.
use crate::{
    invalid,
    secondary::Group,
    surface::{self, Editable},
    Result,
};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_skeleton::transforms::{self, Matrix4};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const TOPOLOGY: &str = "28c37fc17b8f0197b77b3a8a5660335b426f7103f6aeb7275d67e4cc653cf576";
const POSITION_TOLERANCE: f64 = 1e-4; // inches; permits float32 export rounding only
const BASIS_TOLERANCE: f64 = 1e-5;

#[derive(Debug, Serialize)]
pub struct FitReceipt {
    pub policy: &'static str,
    pub topology_sha256: String,
    pub groups_rebound: usize,
    pub max_group_position_residual_inches: f64,
    pub max_group_basis_residual: f64,
    pub render_source_modified: bool,
    pub motion_maps_modified: bool,
}

fn put(h: &mut Sha256, raw: &[u8]) {
    h.update((raw.len() as u64).to_le_bytes());
    h.update(raw);
}

// Retain names, types, palette, topology, UVs, weights, material identities,
// stream lengths, hierarchy, scales, and all non-fit metadata. Export UUIDs
// and fitted coordinates/directions are not model identity.
pub(crate) fn topology(d: &Dmx, cancel: &CancellationToken) -> Result<String> {
    let mut h = Sha256::new();
    put(&mut h, b"tangtang-fit-topology-v1");
    for s in &d.strings {
        put(&mut h, s.as_bytes());
    }
    for e in &d.elements {
        cancel.check()?;
        put(&mut h, e.kind.as_bytes());
        put(&mut h, e.name.as_bytes());
        for a in &e.attributes {
            let raw = d.raw_attribute(a);
            put(&mut h, a.name.as_bytes());
            put(&mut h, &[a.type_code]);
            put(&mut h, &(raw.len() as u64).to_le_bytes());
            if e.kind == "DmeTransform" && matches!(a.name.as_str(), "position" | "orientation") {
                continue;
            }
            if e.kind == "DmeVertexData"
                && matches!(a.name.as_str(), "position$0" | "normal$0" | "tangent$0")
            {
                put(
                    &mut h,
                    raw.get(..4)
                        .ok_or_else(|| invalid("Invalid TangTang fit stream"))?,
                );
            } else {
                put(&mut h, raw);
            }
        }
    }
    Ok(format!("{:x}", h.finalize()))
}

#[derive(Deserialize)]
struct Range {
    #[serde(flatten)]
    group: Group,
    fixed_points: Vec<usize>,
    dynamic_point_range: [usize; 2],
}
#[derive(Deserialize)]
struct Saved {
    groups: Vec<Range>,
}

fn vector(m: &Matrix4, v: &[f64], point: bool) -> [f64; 3] {
    std::array::from_fn(|r| {
        (0..3).map(|c| m[r][c] * v[c]).sum::<f64>() + if point { m[r][3] } else { 0. }
    })
}
fn vectors(values: &[[f64; 3]]) -> Result<Vec<u8>> {
    let mut raw = (values.len() as i32).to_le_bytes().to_vec();
    for v in values.iter().flatten() {
        raw.extend((transforms::float32(*v)? as f32).to_le_bytes());
    }
    Ok(raw)
}

pub(crate) fn rebind(
    source: &Dmx,
    proxy: &Dmx,
    cancel: &CancellationToken,
) -> Result<(Vec<u8>, FitReceipt)> {
    let signature = topology(source, cancel)?;
    if signature != TOPOLOGY {
        return Err(invalid("This TangTang conversion has different topology, skin weights or model data. Use Automatic for a modified model."));
    }
    let a = surface::binds(proxy)?.skeleton;
    let b = surface::binds(source)?.skeleton;
    if a.names != b.names || a.parents != b.parents || a.scales != b.scales {
        return Err(invalid("TangTang preset fitting requires the same bone names, parents and scales. Use Automatic for a changed rig."));
    }
    let lookup = a.indices();
    let old = transforms::bind_state(&a)?;
    let new = transforms::bind_state(&b)?;
    for (i, name) in a.names.iter().enumerate() {
        if !name.starts_with("dmb_secondary_")
            && (a.positions[i] != b.positions[i] || a.rotations[i] != b.rotations[i])
        {
            return Err(invalid(
                "TangTang preset fitting requires unchanged original Haze body binds.",
            ));
        }
    }
    let groups = crate::tangtang_assets::GROUPS.ok_or_else(|| invalid(crate::tangtang_assets::NOT_INCLUDED))?;
    let saved: Saved = serde_json::from_str(groups)?;
    let mut matrices = vec![];
    let mut owners = vec![None; 900];
    let mut max_position: f64 = 0.;
    let mut max_basis: f64 = 0.;
    for (g, row) in saved.groups.iter().enumerate() {
        cancel.check()?;
        let root = lookup[row.group.root.as_str()];
        let m = transforms::multiply(&new.world[root], &old.inverse[root]);
        for name in &row.group.bones {
            let i = lookup[name.as_str()];
            let fitted = transforms::multiply(&m, &old.world[i]);
            for r in 0..3 {
                max_position = max_position.max((fitted[r][3] - new.world[i][r][3]).abs());
                for c in 0..3 {
                    max_basis = max_basis.max((fitted[r][c] - new.world[i][r][c]).abs());
                }
            }
        }
        if max_position > POSITION_TOLERANCE || max_basis > BASIS_TOLERANCE {
            return Err(invalid(format!("TangTang group {} changed shape or bone orientation. Use Automatic for a changed rig.", row.group.source_root)));
        }
        let [start, end] = row.dynamic_point_range;
        for i in row.fixed_points.iter().copied().chain(start..end) {
            let slot = owners
                .get_mut(i)
                .ok_or_else(|| invalid("Invalid bundled TangTang fit range"))?;
            if slot.replace(g).is_some() {
                return Err(invalid("Overlapping TangTang fit ranges"));
            }
        }
        matrices.push(m);
    }
    if owners.iter().any(Option::is_none) {
        return Err(invalid("Incomplete TangTang fit ranges"));
    }
    let owners = owners.into_iter().map(Option::unwrap).collect::<Vec<_>>();
    let mut edit = Editable::new(proxy);
    let states = proxy
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "DmeVertexData" && e.get("cloth_enable$0").is_some())
        .collect::<Vec<_>>();
    if states.len() != 1 {
        return Err(invalid("Invalid bundled TangTang proxy state"));
    }
    let (state_index, state) = states[0];
    let positions = surface::floats(proxy, state, "position$0", 42, 3)?;
    if positions.len() != owners.len() * 3 {
        return Err(invalid("Invalid TangTang proxy position count"));
    }
    let fitted = positions
        .chunks_exact(3)
        .enumerate()
        .map(|(i, v)| vector(&matrices[owners[i]], v, true))
        .collect::<Vec<_>>();
    edit.set(state_index, "position$0", 42, vectors(&fitted)?)?;
    // Fixed supports move with their group as well, while remaining weighted
    // to the same original body anchor. Leaving these behind would stretch it.
    let corners = surface::ints(proxy, state, "position$0Indices", 34)?;
    let normal_indices = surface::ints(proxy, state, "normal$0Indices", 34)?;
    let normals = surface::floats(proxy, state, "normal$0", 42, 3)?;
    if corners.len() != normal_indices.len() {
        return Err(invalid("Invalid bundled TangTang corner normals"));
    }
    let mut normal_owners = vec![None; normals.len() / 3];
    for (&corner, &normal) in corners.iter().zip(&normal_indices) {
        let owner = *usize::try_from(corner)
            .ok()
            .and_then(|i| owners.get(i))
            .ok_or_else(|| invalid("Invalid TangTang proxy corner"))?;
        let slot = usize::try_from(normal)
            .ok()
            .and_then(|i| normal_owners.get_mut(i))
            .ok_or_else(|| invalid("Invalid TangTang normal index"))?;
        if slot.is_some_and(|previous| previous != owner) {
            return Err(invalid("TangTang proxy normal crosses independent groups"));
        }
        *slot = Some(owner);
    }
    let mut fitted_normals = vec![];
    for (normal, owner) in normals.chunks_exact(3).zip(normal_owners) {
        let owner = owner.ok_or_else(|| invalid("Unreferenced TangTang proxy normal"))?;
        // The accepted matrices are rigid; no normalization or resynthesis of
        // normals, topology, skinning or motion maps is necessary.
        fitted_normals.push(vector(&matrices[owner], normal, false));
    }
    edit.set(state_index, "normal$0", 42, vectors(&fitted_normals)?)?;
    let joints = source
        .elements
        .iter()
        .filter(|e| e.kind == "DmeJoint")
        .map(|e| (e.name.as_str(), e))
        .collect::<BTreeMap<_, _>>();
    for e in proxy.elements.iter().filter(|e| e.kind == "DmeJoint") {
        let target = surface::reference(proxy, e, "transform", "DmeTransform")?;
        let src = surface::reference(source, joints[e.name.as_str()], "transform", "DmeTransform")?;
        for name in ["position", "orientation"] {
            let attr = source.elements[src]
                .attributes
                .iter()
                .find(|a| a.name == name)
                .ok_or_else(|| invalid("Missing TangTang source bind"))?;
            edit.set(
                target,
                name,
                attr.type_code,
                source.raw_attribute(attr).to_vec(),
            )?;
        }
    }
    cancel.check()?;
    let result = edit.bytes()?;
    // This also verifies current/bind-state identity and finite matrices. The
    // caller then compares the entire output skeleton to the new source.
    surface::binds(&Dmx::parse(result.clone())?)?;
    Ok((
        result,
        FitReceipt {
            policy: "tangtang-rigid-group-refit-v1",
            topology_sha256: signature,
            groups_rebound: saved.groups.len(),
            max_group_position_residual_inches: max_position,
            max_group_basis_residual: max_basis,
            render_source_modified: false,
            motion_maps_modified: false,
        },
    ))
}
