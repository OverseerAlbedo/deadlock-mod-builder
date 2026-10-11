//! Complete typed cloth preservation and explicitly selected native solver import.
//! All transforms operate on immutable byte snapshots. An independent native
//! reader and publication transaction remain mandatory caller responsibilities.
#![forbid(unsafe_code)]
pub mod capacity;
mod restore;
mod runtime;
mod tested;
pub mod generated;
pub use capacity::CapacityProof;
use deadlock_resource::{binary, container, Value};
use deadlock_skeleton::{bindings, Skeleton};
use deadlock_vpk::{ResourceBlock, ResourceLayout};
pub use restore::{restore, RestorationReceipt};
pub use runtime::{
    audit_resource, audit_skeleton_extension, validate_motion, ExtensionProof, MotionValidation,
    RuntimeAudit,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use tested::{
    apply_tested, inspect_tested, qualify_tested, validate_tested, Qualification, TargetProof,
    TestedAudit, TestedSelection, TransferReceipt,
};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Cloth validation: {0}")]
    Invalid(String),
    #[error(transparent)]
    Resource(#[from] deadlock_resource::Error),
    #[error(transparent)]
    Skeleton(#[from] deadlock_skeleton::Error),
    #[error(transparent)]
    Vpk(#[from] deadlock_vpk::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
pub(crate) fn hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
pub(crate) fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    value
        .get(name)
        .ok_or_else(|| invalid(format!("missing {name}")))
}
pub(crate) fn field_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Value> {
    value
        .get_mut(name)
        .ok_or_else(|| invalid(format!("missing {name}")))
}
pub(crate) fn strings(value: &Value) -> Result<Vec<String>> {
    value
        .array()?
        .iter()
        .map(|v| Ok(v.string()?.to_owned()))
        .collect()
}
pub(crate) fn skeleton(data: &Value) -> Result<Skeleton> {
    Ok(Skeleton::from_value(field(data, "m_modelSkeleton")?)?)
}
pub(crate) fn reject_external_physics(raw: &[u8]) -> Result<()> {
    for reference in deadlock_vpk::resource_references(raw)? {
        let reference = reference.to_ascii_lowercase();
        if reference
            .strip_suffix("_c")
            .unwrap_or(&reference)
            .ends_with(".vphys")
        {
            return Err(invalid("external VPHYS simulation requires a separate dependency audit; publication blocked"));
        }
    }
    Ok(())
}
pub(crate) fn decode_unique(
    raw: &[u8],
    blocks: &[ResourceBlock],
    name: &str,
) -> Result<binary::Document> {
    Ok(binary::decode(container::payload(
        raw,
        container::unique(blocks, name)?,
    )?)?)
}
pub(crate) struct Decoded {
    pub layout: ResourceLayout,
    pub data: binary::Document,
    pub physics: binary::Document,
    pub ctrl: binary::Document,
    pub meshes: Vec<Value>,
    pub animations: Vec<Value>,
}
impl Decoded {
    pub fn read(raw: &[u8]) -> Result<Self> {
        let layout = container::layout(raw)?;
        reject_external_physics(raw)?;
        let data = decode_unique(raw, &layout.blocks, "DATA")?;
        let physics = decode_unique(raw, &layout.blocks, "PHYS")?;
        let ctrl = decode_unique(raw, &layout.blocks, "CTRL")?;
        let ordinal = layout
            .blocks
            .iter()
            .position(|b| b.name == "PHYS")
            .ok_or_else(|| invalid("missing PHYS"))?;
        if field(field(&ctrl.root, "embedded_physics")?, "phys_data_block")?.integer()?
            != ordinal as i128
        {
            return Err(invalid("embedded PHYS block does not match CTRL binding"));
        }
        let read = |name: &str| -> Result<Vec<Value>> {
            layout
                .blocks
                .iter()
                .filter(|b| b.name == name)
                .map(|b| Ok(binary::decode(container::payload(raw, b)?)?.root))
                .collect()
        };
        let meshes = read("MDAT")?;
        let animations = read("AGRP")?;
        Ok(Self {
            layout,
            data,
            physics,
            ctrl,
            meshes,
            animations,
        })
    }
    pub fn bindings(&self) -> Result<bindings::BindingAudit> {
        self.bindings_with_custom_bones(&std::collections::BTreeSet::new())
    }
    pub fn bindings_with_custom_bones(
        &self,
        custom: &std::collections::BTreeSet<String>,
    ) -> Result<bindings::BindingAudit> {
        Ok(bindings::audit_bindings_with_custom_bones(
            &self.data.root,
            &self.ctrl.root,
            &self.meshes,
            &self.animations,
            &self.layout.blocks,
            custom,
        )?)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnchangedBlock {
    pub ordinal: usize,
    pub name: String,
    pub sha256: String,
}
pub(crate) fn unchanged_blocks(
    before: &[u8],
    after: &[u8],
    require_except: &[&str],
) -> Result<Vec<UnchangedBlock>> {
    let a = container::layout(before)?;
    let b = container::layout(after)?;
    if a.blocks.len() != b.blocks.len() {
        return Err(invalid("resource block count changed"));
    }
    let mut unchanged = Vec::new();
    for (ordinal, (old, new)) in a.blocks.iter().zip(&b.blocks).enumerate() {
        if old.name != new.name {
            return Err(invalid("resource block ordering changed"));
        }
        let old_bytes = container::payload(before, old)?;
        let new_bytes = container::payload(after, new)?;
        if old_bytes == new_bytes {
            unchanged.push(UnchangedBlock {
                ordinal,
                name: old.name.clone(),
                sha256: hash(new_bytes),
            });
        } else if !require_except.contains(&old.name.as_str()) {
            return Err(invalid(format!(
                "unexpected compiled block change: {}",
                old.name
            )));
        }
    }
    if deadlock_vpk::resource_references(before)? != deadlock_vpk::resource_references(after)? {
        return Err(invalid("compiled resource references changed"));
    }
    Ok(unchanged)
}
