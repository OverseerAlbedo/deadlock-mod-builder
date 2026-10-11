//! Stored uint16 remap offsets are authoritative; congruent larger offsets are
//! reported only as wrap evidence and can never make a failing palette pass.
use crate::*;
use deadlock_resource::cloth::{self, Status};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mismatch {
    pub mesh_bone_index: usize,
    pub expected_name: String,
    pub stored_model_index: Option<i128>,
    pub actual_name: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemapMesh {
    pub name: Option<String>,
    pub mesh_index: usize,
    pub stored_start: i128,
    pub mesh_bone_count: usize,
    pub stored_slice_long_enough: bool,
    pub identity_mismatch_count: usize,
    pub mismatch_examples: Vec<Mismatch>,
    pub matching_larger_offsets_modulo_uint16: Vec<usize>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RemapEvidence {
    pub compiler_arrays_modified: bool,
    pub start_offset_storage: String,
    pub maximum_representable_start: u32,
    pub aggregate_table_length_alone_is_not_a_failure: bool,
    pub padding_length_assumed: bool,
    pub model_bone_count: usize,
    pub aggregate_remapping_entries: usize,
    pub mesh_count: usize,
    pub maximum_stored_start: Option<i128>,
    pub slice_boundaries_valid: bool,
    pub out_of_range_remap_index_count: usize,
    pub misbound_mesh_count: usize,
    pub meshes_with_uint16_wrap_evidence: usize,
    pub meshes: Vec<RemapMesh>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemapAudit {
    pub status: Status,
    pub reason: Option<String>,
    pub evidence: RemapEvidence,
}
pub fn audit_remapping(
    data: &Value,
    ctrl: &Value,
    mdats: &[Value],
    blocks: &[ResourceBlock],
) -> RemapAudit {
    let mut evidence = RemapEvidence {
        start_offset_storage: "uint16".into(),
        maximum_representable_start: 65535,
        aggregate_table_length_alone_is_not_a_failure: true,
        ..Default::default()
    };
    let check = (|| -> Result<bool> {
        let names = strings(field(field(data, "m_modelSkeleton")?, "m_boneName")?)?;
        if names.iter().collect::<BTreeSet<_>>().len() != names.len() {
            return Err(invalid("duplicate model palette identities"));
        }
        let ints = |key| -> Result<Vec<i128>> {
            field(data, key)?
                .array()?
                .iter()
                .map(|v| Ok(v.integer()?))
                .collect()
        };
        let table = ints("m_remappingTable")?;
        let starts = ints("m_remappingTableStarts")?;
        let entries = field(ctrl, "embedded_meshes")?.array()?;
        let ordinals = blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.name == "MDAT")
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        evidence.model_bone_count = names.len();
        evidence.aggregate_remapping_entries = table.len();
        evidence.mesh_count = entries.len();
        evidence.maximum_stored_start = starts.iter().copied().max();
        if starts.is_empty()
            || ordinals.len() != mdats.len()
            || entries.len() != mdats.len()
            || starts.len() != mdats.len()
        {
            return Err(invalid(
                "every mesh needs one MDAT, CTRL binding and remap start",
            ));
        }
        let mut binding = vec![];
        for entry in entries {
            let index = entry
                .get("m_nMeshIndex")
                .or_else(|| entry.get("mesh_index"))
                .ok_or_else(|| invalid("missing mesh index"))?
                .integer()?;
            let block = entry
                .get("m_nDataBlock")
                .or_else(|| entry.get("data_block"))
                .ok_or_else(|| invalid("missing mesh block"))?
                .integer()?;
            let index = usize::try_from(index).map_err(|_| invalid("invalid mesh index"))?;
            let block = usize::try_from(block).map_err(|_| invalid("invalid block index"))?;
            binding.push((index, block, entry));
        }
        let mut indices = binding.iter().map(|r| r.0).collect::<Vec<_>>();
        indices.sort_unstable();
        let mut linked = binding.iter().map(|r| r.1).collect::<Vec<_>>();
        linked.sort_unstable();
        if indices.iter().copied().ne(0..starts.len()) || linked != ordinals {
            return Err(invalid(
                "CTRL mesh indices and MDAT bindings must be unique and complete",
            ));
        }
        evidence.slice_boundaries_valid = starts[0] == 0
            && starts.windows(2).all(|w| w[0] <= w[1])
            && starts
                .iter()
                .all(|n| *n >= 0 && *n <= 65535 && *n <= table.len() as i128);
        evidence.out_of_range_remap_index_count = table
            .iter()
            .filter(|n| **n < 0 || **n >= names.len() as i128)
            .count();
        let by_block: BTreeMap<_, _> = ordinals.into_iter().zip(mdats).collect();
        let lookup: BTreeMap<_, _> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();
        binding.sort_by_key(|r| r.0);
        for (index, block, entry) in binding {
            let bones = field(field(by_block[&block], "m_skeleton")?, "m_bones")?.array()?;
            let local = bones
                .iter()
                .map(|b| Ok(field(b, "m_boneName")?.string()?.to_owned()))
                .collect::<Result<Vec<_>>>()?;
            if local.is_empty()
                || local.iter().collect::<BTreeSet<_>>().len() != local.len()
                || local.iter().any(|n| !lookup.contains_key(n.as_str()))
            {
                return Err(invalid(
                    "mesh has empty, duplicate or unknown bone identities",
                ));
            }
            let expected = local
                .iter()
                .map(|n| lookup[n.as_str()] as i128)
                .collect::<Vec<_>>();
            let start = starts[index];
            let end = starts
                .get(index + 1)
                .copied()
                .unwrap_or(table.len() as i128);
            let mut mismatch_count = 0;
            let mut examples = vec![];
            for (local, wanted) in expected.iter().enumerate() {
                let actual = usize::try_from(start)
                    .ok()
                    .and_then(|s| s.checked_add(local))
                    .and_then(|i| table.get(i))
                    .copied();
                if actual == Some(*wanted) {
                    continue;
                }
                mismatch_count += 1;
                if examples.len() < 8 {
                    examples.push(Mismatch {
                        mesh_bone_index: local,
                        expected_name: names[*wanted as usize].clone(),
                        stored_model_index: actual,
                        actual_name: actual
                            .and_then(|n| usize::try_from(n).ok())
                            .and_then(|n| names.get(n))
                            .cloned(),
                    });
                }
            }
            let mut aliases = vec![];
            if mismatch_count > 0 && (0..65536).contains(&start) {
                let mut candidate = start as usize + 65536;
                while candidate <= table.len().saturating_sub(bones.len())
                    && candidate + bones.len() <= table.len()
                {
                    if table[candidate..candidate + bones.len()] == expected {
                        aliases.push(candidate);
                    }
                    candidate += 65536;
                }
            }
            let name = entry
                .get("m_Name")
                .or_else(|| entry.get("name"))
                .map(|v| v.string().map(str::to_owned))
                .transpose()?;
            evidence.meshes.push(RemapMesh {
                name,
                mesh_index: index,
                stored_start: start,
                mesh_bone_count: bones.len(),
                stored_slice_long_enough: 0 <= start
                    && start <= end
                    && end <= table.len() as i128
                    && end - start >= bones.len() as i128,
                identity_mismatch_count: mismatch_count,
                mismatch_examples: examples,
                matching_larger_offsets_modulo_uint16: aliases,
            });
        }
        evidence.misbound_mesh_count = evidence
            .meshes
            .iter()
            .filter(|r| r.identity_mismatch_count > 0)
            .count();
        evidence.meshes_with_uint16_wrap_evidence = evidence
            .meshes
            .iter()
            .filter(|r| !r.matching_larger_offsets_modulo_uint16.is_empty())
            .count();
        Ok(evidence.slice_boundaries_valid
            && evidence.out_of_range_remap_index_count == 0
            && evidence.misbound_mesh_count == 0
            && evidence.meshes.iter().all(|r| r.stored_slice_long_enough))
    })();
    match check{Ok(true)=>RemapAudit{status:Status::Passed,reason:None,evidence},Ok(false)=>RemapAudit{status:Status::Failed,reason:Some("stored compiled remap offsets, bounds or identities invalid; larger offsets are diagnostic only".into()),evidence},Err(e)=>RemapAudit{status:Status::Failed,reason:Some(e.to_string()),evidence}}
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapacityProof {
    pub status: Status,
    pub mesh_remapping: RemapAudit,
    pub scalar_simd_rods: cloth::RodAudit,
    pub compiled_resources_modified: bool,
    pub runtime_ready: bool,
    pub build_authorized: bool,
}
pub fn audit(
    data: &Value,
    ctrl: &Value,
    mdats: &[Value],
    blocks: &[ResourceBlock],
    physics: &Value,
) -> CapacityProof {
    let mesh_remapping = audit_remapping(data, ctrl, mdats, blocks);
    let scalar_simd_rods = cloth::audit_rods(physics);
    let status = if mesh_remapping.status == Status::Failed
        || scalar_simd_rods.status == Status::Failed
    {
        Status::Failed
    } else if mesh_remapping.status == Status::Pending || scalar_simd_rods.status == Status::Pending
    {
        Status::Pending
    } else {
        Status::Passed
    };
    CapacityProof {
        status,
        mesh_remapping,
        scalar_simd_rods,
        compiled_resources_modified: false,
        runtime_ready: false,
        build_authorized: false,
    }
}
