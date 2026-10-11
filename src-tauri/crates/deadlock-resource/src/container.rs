use crate::{binary, invalid, Result};
use deadlock_vpk::{resource_blocks, ResourceBlock, ResourceLayout};
use std::collections::BTreeMap;

pub fn layout(raw: &[u8]) -> Result<ResourceLayout> {
    let layout = resource_blocks(raw)?;
    if layout.declared_size != raw.len() {
        return Err(invalid(
            "motion edits do not support external/trailing resource payloads",
        ));
    }
    Ok(layout)
}
pub fn unique<'a>(blocks: &'a [ResourceBlock], name: &str) -> Result<&'a ResourceBlock> {
    let mut found = blocks.iter().filter(|b| b.name == name);
    let block = found
        .next()
        .ok_or_else(|| invalid(format!("exactly one embedded {name} block required")))?;
    if found.next().is_some() {
        return Err(invalid(format!("multiple embedded {name} blocks")));
    }
    Ok(block)
}
pub fn payload<'a>(raw: &'a [u8], block: &ResourceBlock) -> Result<&'a [u8]> {
    raw.get(
        block.offset
            ..block
                .offset
                .checked_add(block.size)
                .ok_or_else(|| invalid("block range overflow"))?,
    )
    .ok_or_else(|| invalid("block outside container"))
}
pub fn read_blocks(raw: &[u8], name: &str) -> Result<Vec<binary::Document>> {
    layout(raw)?
        .blocks
        .iter()
        .filter(|b| b.name == name)
        .map(|b| binary::decode(payload(raw, b)?))
        .collect()
}
/// Rebuild offsets and alignment only. Every untouched payload remains byte exact.
pub fn rebuild(raw: &[u8], replacements: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    let layout = layout(raw)?;
    for name in replacements.keys() {
        if !layout.blocks.iter().any(|b| &b.name == name) {
            return Err(invalid(format!("replacement block absent: {name}")));
        }
    }
    let replacements = layout
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| replacements.get(&b.name).map(|v| (i, v.as_slice())))
        .collect();
    rebuild_selected(raw, &layout, &replacements)
}
/// Replace explicitly selected block ordinals, including repeated MDAT/AGRP.
pub fn rebuild_ordinals(raw: &[u8], replacements: &BTreeMap<usize, Vec<u8>>) -> Result<Vec<u8>> {
    let layout = layout(raw)?;
    if replacements.keys().any(|i| *i >= layout.blocks.len()) {
        return Err(invalid("replacement ordinal outside block table"));
    }
    let replacements = replacements
        .iter()
        .map(|(i, v)| (*i, v.as_slice()))
        .collect();
    rebuild_selected(raw, &layout, &replacements)
}
fn rebuild_selected(
    raw: &[u8],
    layout: &ResourceLayout,
    replacements: &BTreeMap<usize, &[u8]>,
) -> Result<Vec<u8>> {
    let mut output = raw[..16].to_vec();
    output.resize(16 + 12 * layout.blocks.len(), 0);
    output[8..12].copy_from_slice(&8u32.to_le_bytes());
    output[12..16].copy_from_slice(&(layout.blocks.len() as u32).to_le_bytes());
    for (i, block) in layout.blocks.iter().enumerate() {
        let bytes = replacements
            .get(&i)
            .copied()
            .unwrap_or(payload(raw, block)?);
        output.resize((output.len() + 15) & !15, 0);
        let start = output.len();
        output.extend_from_slice(bytes);
        let entry = 16 + 12 * i;
        let offset =
            u32::try_from(start - entry - 4).map_err(|_| invalid("resource offset overflow"))?;
        let length = u32::try_from(bytes.len()).map_err(|_| invalid("resource size overflow"))?;
        output[entry..entry + 4].copy_from_slice(block.name.as_bytes());
        output[entry + 4..entry + 8].copy_from_slice(&offset.to_le_bytes());
        output[entry + 8..entry + 12].copy_from_slice(&length.to_le_bytes());
    }
    let size =
        u32::try_from(output.len()).map_err(|_| invalid("resource exceeds supported size"))?;
    output[..4].copy_from_slice(&size.to_le_bytes());
    let checked = self::layout(&output)?;
    for (i, (old, new)) in layout.blocks.iter().zip(&checked.blocks).enumerate() {
        if !replacements.contains_key(&i) && payload(raw, old)? != payload(&output, new)? {
            return Err(invalid("untouched resource payload changed"));
        }
    }
    Ok(output)
}
