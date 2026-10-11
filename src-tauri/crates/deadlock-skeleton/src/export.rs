//! Narrow model-22 exporter. Input streams are explicit bridge data, never inferred.
use crate::{
    dmx_bind::{self, BindAudit},
    invalid,
    skinning::{self, SkinningAudit},
    transforms::{self, ScaleAudit},
    IdentityAudit, Result, Skeleton,
};
use deadlock_dmx::{Dmx, DmxSummary, GeometryCounts};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedReport {
    pub meshes: Vec<PreparedMesh>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedMesh {
    pub name: String,
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    pub uvs: Vec<Vec<[f64; 2]>>,
    pub colors: Vec<Vec<[f64; 4]>>,
    #[serde(default)]
    pub tangents: Vec<[f64; 4]>,
    pub weights: Vec<Vec<(String, f64)>>,
    pub faces: Vec<PreparedFace>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedFace {
    pub vertices: Vec<usize>,
    pub corners: Vec<usize>,
    pub material: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct ExportAudit {
    pub bind_identity: IdentityAudit,
    pub target_scale_audit: ScaleAudit,
    pub target_bind_audit: BindAudit,
    pub skinning: SkinningAudit,
    pub geometry: GeometryCounts,
    pub dmx: DmxSummary,
    pub native_compiler_validated: bool,
}
struct Attr {
    key: usize,
    kind: u8,
    raw: Vec<u8>,
}
struct Elem {
    kind: usize,
    name: usize,
    uuid: [u8; 16],
    attrs: Vec<Attr>,
}
#[derive(Default)]
struct Builder {
    pool: Vec<String>,
    lookup: BTreeMap<String, usize>,
    elements: Vec<Elem>,
}
impl Builder {
    fn string(&mut self, s: &str) -> Result<usize> {
        if s.contains('\0') {
            return Err(invalid("Embedded NUL in generated DMX string"));
        }
        if let Some(&i) = self.lookup.get(s) {
            return Ok(i);
        }
        let i = self.pool.len();
        self.pool.push(s.into());
        self.lookup.insert(s.into(), i);
        Ok(i)
    }
    fn elem(&mut self, kind: &str, name: &str) -> Result<usize> {
        let index = self.elements.len();
        let uuid = uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            format!("dmb-preparation/{index}/{kind}/{name}").as_bytes(),
        )
        .to_bytes_le();
        let k = self.string(kind)?;
        let n = self.string(name)?;
        self.elements.push(Elem {
            kind: k,
            name: n,
            uuid,
            attrs: vec![],
        });
        Ok(index)
    }
    fn attr(&mut self, e: usize, key: &str, kind: u8, raw: Vec<u8>) -> Result<()> {
        let k = self.string(key)?;
        self.elements[e].attrs.push(Attr { key: k, kind, raw });
        Ok(())
    }
    fn reference(&mut self, e: usize, key: &str, i: usize) -> Result<()> {
        self.attr(e, key, 1, int(i)?.to_le_bytes().to_vec())
    }
    fn refs(&mut self, e: usize, key: &str, values: &[usize]) -> Result<()> {
        self.integers(
            e,
            key,
            values.iter().map(|i| int(*i)).collect::<Result<Vec<_>>>()?,
        )
    }
    fn integers(&mut self, e: usize, key: &str, values: Vec<i32>) -> Result<()> {
        let mut raw = int(values.len())?.to_le_bytes().to_vec();
        for v in values {
            raw.extend(v.to_le_bytes());
        }
        self.attr(e, key, 34, raw)
    }
    fn elements(&mut self, e: usize, key: &str, values: &[usize]) -> Result<()> {
        let mut raw = int(values.len())?.to_le_bytes().to_vec();
        for i in values {
            raw.extend(int(*i)?.to_le_bytes());
        }
        self.attr(e, key, 33, raw)
    }
    fn vectors<const N: usize>(
        &mut self,
        e: usize,
        key: &str,
        base: u8,
        values: &[[f64; N]],
    ) -> Result<()> {
        let mut raw = int(values.len())?.to_le_bytes().to_vec();
        for row in values {
            for v in row {
                raw.extend(f32bytes(*v)?);
            }
        }
        self.attr(e, key, base + 32, raw)
    }
    fn strings(&mut self, e: usize, key: &str, values: &[String]) -> Result<()> {
        let mut raw = int(values.len())?.to_le_bytes().to_vec();
        for v in values {
            if v.contains('\0') {
                return Err(invalid("Embedded NUL in generated DMX string"));
            }
            raw.extend(v.as_bytes());
            raw.push(0);
        }
        self.attr(e, key, 37, raw)
    }
    fn transform(&mut self, name: &str, p: [f64; 3], q: [f64; 4], s: f64) -> Result<usize> {
        let i = self.elem("DmeTransform", name)?;
        self.attr(
            i,
            "position",
            10,
            p.into_iter()
                .map(f32bytes)
                .collect::<Result<Vec<_>>>()?
                .concat(),
        )?;
        self.attr(
            i,
            "orientation",
            13,
            q.into_iter()
                .map(f32bytes)
                .collect::<Result<Vec<_>>>()?
                .concat(),
        )?;
        self.attr(i, "scale", 3, f32bytes(s)?.to_vec())?;
        Ok(i)
    }
    fn identity(&mut self, name: &str) -> Result<usize> {
        self.transform(name, [0.; 3], [0., 0., 0., 1.], 1.)
    }
    fn finish(self) -> Result<Vec<u8>> {
        let mut out = b"<!-- dmx encoding binary 9 format model 22 -->\n\0".to_vec();
        out.extend(0_i32.to_le_bytes());
        out.extend(int(self.pool.len())?.to_le_bytes());
        for s in self.pool {
            out.extend(s.as_bytes());
            out.push(0);
        }
        out.extend(int(self.elements.len())?.to_le_bytes());
        for e in &self.elements {
            out.extend(int(e.kind)?.to_le_bytes());
            out.extend(int(e.name)?.to_le_bytes());
            out.extend(e.uuid);
        }
        for e in self.elements {
            out.extend(int(e.attrs.len())?.to_le_bytes());
            for a in e.attrs {
                out.extend(int(a.key)?.to_le_bytes());
                out.push(a.kind);
                out.extend(a.raw);
            }
        }
        Ok(out)
    }
}
fn int(n: usize) -> Result<i32> {
    i32::try_from(n).map_err(|_| invalid("Generated DMX count exceeds int32"))
}
fn f32bytes(n: f64) -> Result<[u8; 4]> {
    let f = n as f32;
    if !n.is_finite() || !f.is_finite() {
        return Err(invalid(
            "Generated stream has nonfinite or unrepresentable f32 value",
        ));
    }
    Ok(f.to_le_bytes())
}
pub fn export_bytes(report: &PreparedReport, s: &Skeleton) -> Result<(Vec<u8>, ExportAudit)> {
    transforms::prepare_target(s)?;
    let mut b = Builder::default();
    let root = b.elem("DmElement", "root")?;
    let model = b.elem("DmeModel", "prepared")?;
    b.reference(root, "model", model)?;
    b.reference(root, "skeleton", model)?;
    let t = b.identity("prepared")?;
    b.reference(model, "transform", t)?;
    let joints: Vec<_> = s
        .names
        .iter()
        .map(|n| b.elem("DmeJoint", n))
        .collect::<Result<_>>()?;
    let mut trans = vec![];
    for (i, &joint) in joints.iter().enumerate() {
        let t = b.transform(&s.names[i], s.positions[i], s.rotations[i], s.scales[i])?;
        trans.push(t);
        b.reference(joint, "transform", t)?;
        let c: Vec<_> = s
            .parents
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == i as i32)
            .map(|(j, _)| joints[j])
            .collect();
        b.elements(joint, "children", &c)?;
    }
    let mut children: Vec<_> = s
        .parents
        .iter()
        .enumerate()
        .filter(|(_, p)| **p == -1)
        .map(|(i, _)| joints[i])
        .collect();
    let mut palette = joints.clone();
    let mut triangles = 0u64;
    for m in &report.meshes {
        if m.weights.len() != m.positions.len() || m.positions.is_empty() {
            return Err(invalid(
                "Generated weight rows must match nonempty positions",
            ));
        }
        let corners: Vec<_> = m
            .faces
            .iter()
            .flat_map(|f| f.vertices.iter().copied())
            .collect();
        let loop_ids: Vec<_> = m
            .faces
            .iter()
            .flat_map(|f| f.corners.iter().copied())
            .collect();
        if loop_ids.len() != corners.len()
            || loop_ids.iter().copied().collect::<BTreeSet<_>>() != (0..loop_ids.len()).collect()
        {
            return Err(invalid("Unsupported reused/missing polygon loop indices"));
        }
        for f in &m.faces {
            if f.vertices.len() < 3
                || f.vertices.len() != f.corners.len()
                || f.vertices.iter().any(|i| *i >= m.positions.len())
            {
                return Err(invalid("Invalid generated polygon"));
            }
            triangles += (f.vertices.len() - 2) as u64;
        }
        let dag = b.elem("DmeDag", &m.name)?;
        let shape = b.elem("DmeMesh", &m.name)?;
        let state = b.elem("DmeVertexData", "bind")?;
        let tr = b.identity(&m.name)?;
        children.push(dag);
        palette.push(dag);
        trans.push(tr);
        b.reference(dag, "transform", tr)?;
        b.reference(dag, "shape", shape)?;
        b.attr(dag, "visible", 4, vec![1])?;
        b.reference(shape, "bindState", state)?;
        b.reference(shape, "currentState", state)?;
        b.elements(shape, "baseStates", &[state])?;
        let mut streams = vec!["position$0".to_string(), "normal$0".to_string()];
        if !m.tangents.is_empty() {
            streams.push("tangent$0".into());
        }
        streams.extend((0..m.uvs.len()).map(|i| format!("texcoord${i}")));
        streams.extend((0..m.colors.len()).map(|i| format!("color${i}")));
        streams.extend(["blendweights$0".into(), "blendindices$0".into()]);
        b.strings(state, "vertexFormat", &streams)?;
        b.attr(state, "flipVCoordinates", 4, vec![1])?;
        fn stream<const N: usize>(
            b: &mut Builder,
            state: usize,
            name: &str,
            base: u8,
            values: &[[f64; N]],
            indices: &[usize],
        ) -> Result<()> {
            if indices.iter().any(|i| *i >= values.len()) {
                return Err(invalid("Generated attribute index outside stream"));
            }
            b.vectors(state, name, base, values)?;
            b.refs(state, &format!("{name}Indices"), indices)
        }
        stream(&mut b, state, "position$0", 10, &m.positions, &corners)?;
        stream(&mut b, state, "normal$0", 10, &m.normals, &loop_ids)?;
        if !m.tangents.is_empty() {
            stream(&mut b, state, "tangent$0", 11, &m.tangents, &loop_ids)?;
        }
        for (i, v) in m.uvs.iter().enumerate() {
            stream(&mut b, state, &format!("texcoord${i}"), 9, v, &loop_ids)?;
        }
        for (i, v) in m.colors.iter().enumerate() {
            stream(&mut b, state, &format!("color${i}"), 11, v, &loop_ids)?;
        }
        let slots = m.weights.iter().map(Vec::len).max().unwrap_or(0);
        if !(1..=64).contains(&slots) {
            return Err(invalid("Invalid influences per generated vertex"));
        }
        b.attr(state, "jointCount", 2, int(slots)?.to_le_bytes().to_vec())?;
        let lookup = s.indices();
        let mut weights = vec![];
        let mut indices = vec![];
        for row in &m.weights {
            for (n, w) in row {
                weights.push([*w]);
                indices.push(int(*lookup
                    .get(n.as_str())
                    .ok_or_else(|| invalid(format!("Unknown weighted bone: {n}")))?)?);
            }
            for _ in row.len()..slots {
                weights.push([0.]);
                indices.push(0);
            }
        }
        b.vectors(state, "blendweights$0", 3, &weights)?;
        b.integers(state, "blendindices$0", indices)?;
        let mut grouped: Vec<(String, Vec<i32>)> = vec![];
        let mut offset = 0;
        for f in &m.faces {
            let at = grouped
                .iter()
                .position(|(n, _)| n == &f.material)
                .unwrap_or_else(|| {
                    grouped.push((f.material.clone(), vec![]));
                    grouped.len() - 1
                });
            for i in offset..offset + f.vertices.len() {
                grouped[at].1.push(int(i)?);
            }
            grouped[at].1.push(-1);
            offset += f.vertices.len();
        }
        let mut faces = vec![];
        for (material, values) in grouped {
            let mat = b.elem("DmeMaterial", &material)?;
            let key = b.string("mtlName")?;
            let value = b.string(&material)?;
            b.elements[mat].attrs.push(Attr {
                key,
                kind: 5,
                raw: int(value)?.to_le_bytes().to_vec(),
            });
            let fs = b.elem("DmeFaceSet", &material)?;
            b.reference(fs, "material", mat)?;
            b.integers(fs, "faces", values)?;
            faces.push(fs);
        }
        b.elements(shape, "faceSets", &faces)?;
    }
    b.elements(model, "children", &children)?;
    b.elements(model, "jointList", &palette)?;
    let bind = b.elem("DmeTransformList", "bind")?;
    b.elements(bind, "transforms", &trans)?;
    b.elements(model, "baseStates", &[bind])?;
    let raw = b.finish()?;
    let parsed = Dmx::parse(raw.clone()).map_err(|e| invalid(e.to_string()))?;
    let skin = skinning::skinning(&parsed)?;
    if !skin.numeric_checks_passed {
        return Err(invalid(format!(
            "Prepared DMX numeric skin check failed: {:?}",
            skin.meshes
        )));
    }
    let geometry = parsed
        .geometry_counts()
        .map_err(|e| invalid(e.to_string()))?;
    let summary = parsed.summary().map_err(|e| invalid(e.to_string()))?;
    if geometry.source_triangles != triangles || summary.bones != s.names {
        return Err(invalid(
            "Prepared topology or bone palette changed during export",
        ));
    }
    let bind = dmx_bind::audit_dmx_bind(&parsed, s, true)?;
    let audit = ExportAudit {
        bind_identity: bind.bind_identity.clone(),
        target_scale_audit: bind.target_scale_audit.clone(),
        target_bind_audit: bind,
        skinning: skin,
        geometry,
        dmx: summary,
        native_compiler_validated: false,
    };
    Ok((raw, audit))
}
pub fn export_dmx(
    report: &PreparedReport,
    skeleton: &Skeleton,
    path: &Path,
) -> Result<ExportAudit> {
    let (raw, audit) = export_bytes(report, skeleton)?;
    deadlock_core::safety::atomic_write(path, raw).map_err(|e| invalid(e.to_string()))?;
    Ok(audit)
}
