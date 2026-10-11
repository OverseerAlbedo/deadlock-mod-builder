use crate::{relative_asset, u16_at, u32_at, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBlock {
    pub name: String,
    pub offset: usize,
    pub size: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLayout {
    pub resource_version: u16,
    pub declared_size: usize,
    pub blocks: Vec<ResourceBlock>,
}

/// Validate the outer container only. DATA/PHYS compression remains opaque.
pub fn resource_blocks(data: &[u8]) -> Result<ResourceLayout> {
    if data.len() < 16 {
        return Err(Error::invalid("Truncated Source 2 resource header."));
    }
    let size = u32_at(data, 0)? as usize;
    let count = u32_at(data, 12)? as usize;
    if u16_at(data, 4)? != 12 || count == 0 || size < 16 || size > data.len() {
        return Err(Error::invalid(
            "Unsupported or malformed Source 2 resource header.",
        ));
    }
    let table = 8u64 + u32_at(data, 8)? as u64;
    if table < 16 || table > size as u64 || count as u64 > (size as u64 - table) / 12 {
        return Err(Error::invalid("Invalid resource block table."));
    }
    let table = table as usize;
    let table_end = table + count * 12;
    let mut blocks = Vec::with_capacity(count);
    for i in 0..count {
        let p = table + i * 12;
        let start = p as u64 + 4 + u32_at(data, p + 4)? as u64;
        let length = u32_at(data, p + 8)? as u64;
        if start < table_end as u64 || start + length > size as u64 {
            return Err(Error::invalid(
                "Resource block points outside its declared container.",
            ));
        }
        let name_bytes = &data[p..p + 4];
        if !name_bytes.is_ascii() {
            return Err(Error::invalid("Invalid resource block name."));
        }
        let name = String::from_utf8(name_bytes.to_vec())
            .map_err(|_| Error::invalid("Invalid resource block name."))?;
        blocks.push(ResourceBlock {
            name,
            offset: start as usize,
            size: length as usize,
        });
    }
    let mut ranges: Vec<_> = blocks
        .iter()
        .filter(|b| b.size != 0)
        .map(|b| (b.offset, b.offset + b.size))
        .collect();
    ranges.sort_unstable();
    if ranges.windows(2).any(|r| r[0].1 > r[1].0) {
        return Err(Error::invalid("Overlapping compiled resource blocks."));
    }
    Ok(ResourceLayout {
        resource_version: u16_at(data, 6)?,
        declared_size: size,
        blocks,
    })
}

/// Decode validated RERL tables only; preserve order and repeated references.
/// A non-resource input has no references, matching dependency discovery in Python.
pub fn resource_references(data: &[u8]) -> Result<Vec<String>> {
    if data.len() < 16 || u16_at(data, 4)? != 12 {
        return Ok(Vec::new());
    }
    let mut refs = Vec::new();
    for block in resource_blocks(data)?.blocks {
        if block.name != "RERL" {
            continue;
        }
        if block.size < 8 {
            return Err(Error::invalid("Invalid RERL block."));
        }
        let end = block.offset + block.size;
        let offset = u32_at(data, block.offset)? as u64;
        let count = u32_at(data, block.offset + 4)? as u64;
        if count > 1_000_000 {
            return Err(Error::invalid("Invalid external-reference count."));
        }
        // Bound all table iteration before allocating strings.
        if offset > block.size as u64 || count > (block.size as u64 - offset) / 16 {
            return Err(Error::invalid("Truncated external reference."));
        }
        for i in 0..count {
            let q = block.offset + offset as usize + i as usize * 16;
            let rel = i64::from_le_bytes(
                data[q + 8..q + 16]
                    .try_into()
                    .map_err(|_| Error::invalid("Truncated external reference."))?,
            );
            let s = (q as i128 + 8) + rel as i128;
            if s < block.offset as i128 || s >= end as i128 {
                return Err(Error::invalid("Invalid external resource path."));
            }
            let s = s as usize;
            let length = data[s..end]
                .iter()
                .position(|b| *b == 0)
                .ok_or_else(|| Error::invalid("Invalid external resource path."))?;
            let text = std::str::from_utf8(&data[s..s + length])
                .map_err(|_| Error::invalid("Invalid UTF-8 external resource path."))?;
            refs.push(relative_asset(text)?);
        }
    }
    Ok(refs)
}
