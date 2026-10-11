//! Isolated experiment. No production animation/cloth validation gate is relaxed.
use deadlock_resource::{binary, container, Annotation, Kind, Value};
use std::collections::{BTreeMap, BTreeSet};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
pub fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key)
        .ok_or_else(|| format!("Missing native field: {key}").into())
}
pub fn arr(v: &Value, key: &str) -> Result<Vec<Value>> {
    Ok(field(v, key)?.array()?.to_vec())
}
pub fn set(v: &mut Value, key: &str, value: Value) -> Result<()> {
    let Kind::Object(rows) = &mut v.kind else {
        return Err("Expected native object".into());
    };
    if let Some(row) = rows.iter_mut().find(|(k, _)| k == key) {
        row.1 = value;
    } else {
        rows.push((key.into(), value));
    }
    Ok(())
}
pub fn obj(rows: Vec<(&str, Value)>) -> Value {
    Value::new(Kind::Object(
        rows.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    ))
}
pub fn list(v: Vec<Value>) -> Value {
    Value::new(Kind::Array(v))
}
pub fn s(v: &str) -> Value {
    Value::new(Kind::String(v.into()))
}
pub fn resource(v: &str) -> Value {
    Value {
        annotation: Annotation::Resource,
        kind: Kind::String(v.into()),
    }
}
pub fn i(v: i64) -> Value {
    Value::new(Kind::I64(v))
}
pub fn f(v: f64) -> Value {
    Value::new(Kind::F32(v as f32))
}
pub fn b(v: bool) -> Value {
    Value::new(Kind::Bool(v))
}
pub fn vector(v: &[f64]) -> Value {
    list(v.iter().copied().map(f).collect())
}
pub fn number(v: &Value) -> Result<f64> {
    Ok(match v.kind {
        Kind::I64(n) => n as f64,
        Kind::U64(n) => n as f64,
        _ => v.number()?,
    })
}
pub fn data(raw: &[u8], name: &str) -> Result<binary::Document> {
    let layout = container::layout(raw)?;
    let block = container::unique(&layout.blocks, name)?;
    Ok(binary::decode(container::payload(raw, block)?)?)
}
pub fn encode(doc: &binary::Document) -> Result<Vec<u8>> {
    let bytes = binary::encode(&doc.root, doc.format_guid)?;
    require(
        binary::decode(&bytes)?.root.canonical_eq(&doc.root),
        "Native KV3 round trip changed data",
    )?;
    Ok(bytes)
}
pub fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
/// Source 2 resource IDs use MurmurHash64B with seed EDABCDEF.
/// Verified against actual RERL entries, not the unrelated StringToken hash.
pub fn resource_id(name: &str) -> u64 {
    const M: u32 = 0x5bd1e995;
    fn mix(mut k: u32) -> u32 {
        k = k.wrapping_mul(M);
        k ^= k >> 24;
        k.wrapping_mul(M)
    }
    let name = name.to_ascii_lowercase();
    let bytes = name.as_bytes();
    let (mut h1, mut h2, mut at) = (0xedabcdefu32 ^ bytes.len() as u32, 0u32, 0usize);
    let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    while bytes.len() - at >= 8 {
        h1 = h1.wrapping_mul(M) ^ mix(word(at));
        at += 4;
        h2 = h2.wrapping_mul(M) ^ mix(word(at));
        at += 4;
    }
    if bytes.len() - at >= 4 {
        h1 = h1.wrapping_mul(M) ^ mix(word(at));
        at += 4;
    }
    if at < bytes.len() {
        for (j, x) in bytes[at..].iter().enumerate() {
            h2 ^= (*x as u32) << (8 * j);
        }
        h2 = h2.wrapping_mul(M);
    }
    h1 ^= h2 >> 18;
    h1 = h1.wrapping_mul(M);
    h2 ^= h1 >> 22;
    h2 = h2.wrapping_mul(M);
    h1 ^= h2 >> 17;
    h1 = h1.wrapping_mul(M);
    h2 ^= h1 >> 19;
    h2 = h2.wrapping_mul(M);
    (h1 as u64) << 32 | h2 as u64
}
pub fn refs(raw: &[u8]) -> Result<Vec<(u64, String)>> {
    let layout = container::layout(raw)?;
    let blocks: Vec<_> = layout.blocks.iter().filter(|b| b.name == "RERL").collect();
    require(blocks.len() <= 1, "Multiple RERL blocks")?;
    let Some(block) = blocks.first() else {
        return Ok(vec![]);
    };
    let p = container::payload(raw, block)?;
    require(p.len() >= 8, "Short RERL")?;
    let start = u32::from_le_bytes(p[0..4].try_into()?) as usize;
    let count = u32::from_le_bytes(p[4..8].try_into()?) as usize;
    require(
        count <= 100000 && start.checked_add(count * 16).is_some_and(|n| n <= p.len()),
        "Bad RERL table",
    )?;
    let mut out = vec![];
    for j in 0..count {
        let at = start + j * 16;
        let id = u64::from_le_bytes(p[at..at + 8].try_into()?);
        let delta = u64::from_le_bytes(p[at + 8..at + 16].try_into()?);
        let pos = (at + 8)
            .checked_add(usize::try_from(delta)?)
            .ok_or("RERL overflow")?;
        require(pos < p.len(), "RERL name outside block")?;
        let end = p[pos..]
            .iter()
            .position(|b| *b == 0)
            .ok_or("Unterminated RERL name")?;
        let name = std::str::from_utf8(&p[pos..pos + end])?.to_string();
        deadlock_vpk::relative_asset(&name)?;
        out.push((id, name));
    }
    let names = deadlock_vpk::resource_references(raw)?;
    require(
        names == out.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>(),
        "Independent reference readers disagree",
    )?;
    Ok(out)
}
pub fn rerldata(entries: &[(u64, String)]) -> Result<Vec<u8>> {
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();
    require(
        entries
            .iter()
            .all(|(id, n)| names.insert(n.to_ascii_lowercase()) && ids.insert(*id)),
        "Duplicate resource references",
    )?;
    let mut out = vec![0u8; 8 + 16 * entries.len()];
    out[..4].copy_from_slice(&8u32.to_le_bytes());
    out[4..8].copy_from_slice(&(entries.len() as u32).to_le_bytes());
    for (j, (id, name)) in entries.iter().enumerate() {
        let at = 8 + j * 16;
        let delta = out.len() - at - 8;
        out[at..at + 8].copy_from_slice(&id.to_le_bytes());
        out[at + 8..at + 16].copy_from_slice(&(delta as u64).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.push(0);
    }
    Ok(out)
}
/// New resource envelope or named block additions; opaque payloads stay exact.
pub fn rebuild(raw: &[u8], mut changed: BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    let layout = container::layout(raw)?;
    let mut parts = vec![];
    require(
        layout
            .blocks
            .iter()
            .filter(|b| changed.contains_key(&b.name))
            .map(|b| &b.name)
            .collect::<BTreeSet<_>>()
            .len()
            == layout
                .blocks
                .iter()
                .filter(|b| changed.contains_key(&b.name))
                .count(),
        "Repeated edited block",
    )?;
    for block in &layout.blocks {
        parts.push((
            block.name.clone(),
            changed
                .remove(&block.name)
                .unwrap_or(container::payload(raw, block)?.to_vec()),
        ));
    }
    for (name, bytes) in changed {
        parts.push((name, bytes));
    }
    envelope(layout.resource_version, &parts)
}
pub fn envelope(version: u16, parts: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut out = vec![0u8; 16 + 12 * parts.len()];
    out[4..6].copy_from_slice(&12u16.to_le_bytes());
    out[6..8].copy_from_slice(&version.to_le_bytes());
    out[8..12].copy_from_slice(&8u32.to_le_bytes());
    out[12..16].copy_from_slice(&(parts.len() as u32).to_le_bytes());
    for (j, (name, bytes)) in parts.iter().enumerate() {
        require(name.len() == 4, "Bad block name")?;
        out.resize((out.len() + 15) & !15, 0);
        let start = out.len();
        out.extend_from_slice(bytes);
        let at = 16 + 12 * j;
        out[at..at + 4].copy_from_slice(name.as_bytes());
        out[at + 4..at + 8].copy_from_slice(&u32::try_from(start - at - 4)?.to_le_bytes());
        out[at + 8..at + 12].copy_from_slice(&u32::try_from(bytes.len())?.to_le_bytes());
    }
    let len = u32::try_from(out.len())?;
    out[..4].copy_from_slice(&len.to_le_bytes());
    container::layout(&out)?;
    Ok(out)
}
pub fn add_ref(raw: &[u8], name: &str) -> Result<Vec<u8>> {
    let mut entries = refs(raw)?;
    require(
        !entries.iter().any(|(_, n)| n.eq_ignore_ascii_case(name)),
        "Test reference already present; use the original mod",
    )?;
    entries.push((resource_id(name), name.into()));
    rerldata(&entries)
}
pub fn unchanged_except(old: &Value, new: &Value, allowed: &[&str]) -> Result<()> {
    let filtered = |v: &Value| -> Result<Value> {
        Ok(Value::new(Kind::Object(
            v.object()?
                .iter()
                .filter(|(k, _)| !allowed.contains(&k.as_str()))
                .cloned()
                .collect(),
        )))
    };
    require(
        filtered(old)?.canonical_eq(&filtered(new)?),
        "A protected native field changed",
    )
}
