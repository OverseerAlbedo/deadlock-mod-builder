//! Diagnostic inventory only. Cardinality never proves any cloth index mapping.
use crate::{Error, Result};
use deadlock_resource::{binary, container, Kind, Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const MERGE_REASON:&str="A new solver must preserve the original simulation and its bindings. A compiled inventory does not prove the node/control mapping, partition remapping, packed rope encoding, SIMD scheduling or collision-tree rebuild. Appending arrays or accepting changed cloth flags is not authorized.";
const COLLIDERS: [&str; 5] = [
    "m_TaperedCapsuleRigids",
    "m_TaperedCapsuleStretches",
    "m_SphereRigids",
    "m_BoxRigids",
    "m_SDFRigids",
];
fn fail(message: impl Into<String>) -> Error {
    crate::invalid(format!("Cloth inventory: {}", message.into()))
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn count(value: &Value, name: &str) -> Result<u16> {
    value
        .integer()
        .ok()
        .and_then(|v| u16::try_from(v).ok())
        .ok_or_else(|| fail(format!("invalid uint16 {name}.")))
}
fn array<'a>(value: Option<&'a Value>, name: &str) -> Result<&'a [Value]> {
    match value {
        None => Ok(&[]),
        Some(v) => v
            .array()
            .map_err(|_| fail(format!("{name} must be an array."))),
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Cardinality {
    #[serde(rename = "array")]
    Array { count: usize },
    #[serde(rename = "object")]
    Object { count: usize },
    #[serde(rename = "binary")]
    Binary { bytes: usize, sha256: String },
    #[serde(rename = "NoneType")]
    Null { value: Option<bool> },
    #[serde(rename = "bool")]
    Bool { value: bool },
    #[serde(rename = "int")]
    Integer { value: serde_json::Number },
    #[serde(rename = "float")]
    Float { value: f64 },
    #[serde(rename = "str")]
    String { value: String },
}
fn cardinality(value: &Value) -> Result<Cardinality> {
    Ok(match &value.kind {
        Kind::Array(v) => Cardinality::Array { count: v.len() },
        Kind::Object(v) => Cardinality::Object { count: v.len() },
        Kind::Blob(v) => Cardinality::Binary {
            bytes: v.len(),
            sha256: hash(v),
        },
        Kind::Null => Cardinality::Null { value: None },
        Kind::Bool(v) => Cardinality::Bool { value: *v },
        Kind::I64(v) => Cardinality::Integer { value: (*v).into() },
        Kind::U64(v) => Cardinality::Integer { value: (*v).into() },
        Kind::F32(v) if v.is_finite() => Cardinality::Float { value: *v as f64 },
        Kind::F64(v) if v.is_finite() => Cardinality::Float { value: *v },
        Kind::String(v) => Cardinality::String { value: v.clone() },
        _ => return Err(fail("Non-finite scalar.")),
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NodeReference {
    One(u16),
    Many(Vec<u16>),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColliderInventory {
    pub count: usize,
    pub authored_node_references: Vec<Option<NodeReference>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BodyPhysics {
    pub parts: usize,
    pub bone_names: Vec<String>,
    pub is_cloth_collider_table: bool,
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnverifiedBinding {
    pub status: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Inventory {
    pub inventory_version: u32,
    pub resource_sha256: String,
    pub compiled_resources_modified: bool,
    pub in_game_motion_tested: bool,
    pub authored_merge_supported: bool,
    pub authored_merge_reason: String,
    pub physics_block_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physics_payload_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_physics: Option<BodyPhysics>,
    #[serde(flatten)]
    pub details: InventoryDetails,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InventoryDetails {
    Unavailable {
        reason: String,
    },
    Absent {
        solver_sha256: Option<String>,
        nodes: u16,
        static_nodes: u16,
        dynamic_nodes: u16,
    },
    Inventoried {
        solver_sha256: String,
        solver_fingerprint_algorithm: String,
        solver_field_count: usize,
        nodes: u16,
        static_nodes: u16,
        dynamic_nodes: u16,
        rotation_locked_static_nodes: Option<u16>,
        first_position_driven_node: Option<u16>,
        ropes: u16,
        control_count: usize,
        control_names: Vec<String>,
        node_control_binding: UnverifiedBinding,
        cloth_colliders: BTreeMap<String, ColliderInventory>,
        collision_tree_cardinalities: BTreeMap<String, usize>,
        scalar_rod_count: usize,
        simd_rod_group_count: usize,
        simd_rod_lane_capacity: usize,
        simd_note: String,
        field_cardinalities: BTreeMap<String, Cardinality>,
    },
}
pub fn solver_inventory(raw: &[u8]) -> Result<Inventory> {
    let layout = container::layout(raw).map_err(|e| fail(e.to_string()))?;
    let blocks = layout
        .blocks
        .iter()
        .filter(|b| b.name == "PHYS")
        .collect::<Vec<_>>();
    let mut result=Inventory{inventory_version:1,resource_sha256:hash(raw),compiled_resources_modified:false,in_game_motion_tested:false,authored_merge_supported:false,authored_merge_reason:MERGE_REASON.into(),physics_block_count:blocks.len(),physics_payload_sha256:None,body_physics:None,details:InventoryDetails::Unavailable{reason:"A unique embedded PHYS block is not available; external or multiple physics aggregates are not interpreted by this inventory.".into()}};
    if blocks.len() != 1 {
        return Ok(result);
    }
    let payload = container::payload(raw, blocks[0]).map_err(|e| fail(e.to_string()))?;
    result.physics_payload_sha256 = Some(hash(payload));
    let physics = binary::decode(payload)
        .map_err(|e| fail(e.to_string()))?
        .root;
    physics
        .object()
        .map_err(|_| fail("PHYS must be an object."))?;
    let parts = array(physics.get("m_parts"), "m_parts")?;
    let bone_names = array(physics.get("m_boneNames"), "m_boneNames")?
        .iter()
        .map(|n| {
            n.string()
                .map(str::to_owned)
                .map_err(|_| fail("Body bone names must be strings."))
        })
        .collect::<Result<Vec<_>>>()?;
    result.body_physics = Some(BodyPhysics {
        parts: parts.len(),
        bone_names,
        is_cloth_collider_table: false,
        note: "Body collision parts are separate from finite-element cloth colliders.".into(),
    });
    let Some(solver) = physics
        .get("m_pFeModel")
        .filter(|v| !matches!(v.kind, Kind::Null))
    else {
        result.details = InventoryDetails::Absent {
            solver_sha256: None,
            nodes: 0,
            static_nodes: 0,
            dynamic_nodes: 0,
        };
        return Ok(result);
    };
    let fields = solver
        .object()
        .map_err(|_| fail("m_pFeModel must be an object or null."))?;
    let required = |name: &str| -> Result<u16> {
        count(
            solver
                .get(name)
                .ok_or_else(|| fail(format!("missing solver count '{name}'.")))?,
            name,
        )
    };
    let nodes = required("m_nNodeCount")?;
    let static_nodes = required("m_nStaticNodes")?;
    let ropes = required("m_nRopeCount")?;
    if static_nodes > nodes {
        return Err(fail("static node count exceeds total nodes."));
    }
    let optional = |name: &str| {
        solver
            .get(name)
            .filter(|v| !matches!(v.kind, Kind::Null))
            .map(|v| count(v, name))
            .transpose()
    };
    let rotation_locked_static_nodes = optional("m_nRotLockStaticNodes")?;
    let first_position_driven_node = optional("m_nFirstPositionDrivenNode")?;
    if rotation_locked_static_nodes.is_some_and(|n| n > static_nodes) {
        return Err(fail("rotation-locked static count exceeds static nodes."));
    }
    if first_position_driven_node.is_some_and(|n| n > nodes) {
        return Err(fail("position-driven boundary exceeds total nodes."));
    }
    let control_names = array(solver.get("m_CtrlName"), "m_CtrlName")?
        .iter()
        .map(|v| {
            v.string()
                .ok()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| fail("control names must be nonempty strings."))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut cloth_colliders = BTreeMap::new();
    for name in COLLIDERS {
        let values = array(solver.get(name), name)?;
        let mut references = vec![];
        for value in values {
            value
                .object()
                .map_err(|_| fail(format!("{name} contains a non-object collider.")))?;
            references.push(
                value
                    .get("nNode")
                    .map(|v| match &v.kind {
                        Kind::Array(rows) => rows
                            .iter()
                            .map(|v| count(v, &format!("{name}.nNode")))
                            .collect::<Result<Vec<_>>>()
                            .map(NodeReference::Many),
                        _ => count(v, &format!("{name}.nNode")).map(NodeReference::One),
                    })
                    .transpose()?,
            );
        }
        cloth_colliders.insert(
            name.into(),
            ColliderInventory {
                count: values.len(),
                authored_node_references: references,
            },
        );
    }
    let field_cardinalities = fields
        .iter()
        .map(|(k, v)| Ok((k.clone(), cardinality(v)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let collision_tree_cardinalities = ["m_TreeParents", "m_TreeCollisionMasks", "m_TreeChildren"]
        .into_iter()
        .map(|name| Ok((name.into(), array(solver.get(name), name)?.len())))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let scalar_rod_count = array(solver.get("m_Rods"), "m_Rods")?.len();
    let simd_rod_group_count = array(solver.get("m_SimdRods"), "m_SimdRods")?.len();
    result.details=InventoryDetails::Inventoried{solver_sha256:deadlock_resource::value::canonical_sha256(solver).map_err(|e|fail(e.to_string()))?,solver_fingerprint_algorithm:"canonical-kv3-v1-sha256".into(),solver_field_count:fields.len(),nodes,static_nodes,dynamic_nodes:nodes-static_nodes,rotation_locked_static_nodes,first_position_driven_node,ropes,control_count:control_names.len(),control_names,node_control_binding:UnverifiedBinding{status:"unverified".into(),reason:"Matching array lengths or names do not prove the runtime node-to-control mapping. No reviewed explicit mapping is available to this inventory.".into()},cloth_colliders,collision_tree_cardinalities,scalar_rod_count,simd_rod_group_count,simd_rod_lane_capacity:4*simd_rod_group_count,simd_note:"Four-lane group capacity is not an independent-constraint count; padding and scheduling require native compiler verification.".into(),field_cardinalities};
    Ok(result)
}
