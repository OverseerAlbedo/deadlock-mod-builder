//! Independent text transport verification against exact native numeric domains.
use crate::{
    invalid,
    text::{Document, Node, NodeKind},
    Kind, Result, Value,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NumericProof {
    pub values_checked: usize,
    pub float32_checked: usize,
    pub float64_checked: usize,
    pub numeric_tolerance_applied: bool,
}

pub fn verify(observed: &Node, expected: &Value) -> Result<NumericProof> {
    fn walk(node: &Node, expected: &Value, path: &str, proof: &mut NumericProof) -> Result<()> {
        proof.values_checked += 1;
        let observed = match &node.kind {
            NodeKind::Typed { value, .. } => value.as_ref(),
            _ => node,
        };
        let problem = || {
            invalid(format!(
                "independent reader changed value at {path}; no tolerance applied"
            ))
        };
        match &expected.kind {
            Kind::Object(rows) => {
                let actual = observed.object().ok_or_else(problem)?;
                if actual.len() != rows.len() {
                    return Err(problem());
                }
                let lookup: HashMap<_, _> = actual.iter().map(|(k, v)| (k.as_str(), v)).collect();
                for (key, value) in rows {
                    walk(
                        lookup.get(key.as_str()).ok_or_else(problem)?,
                        value,
                        &format!("{path}.{key}"),
                        proof,
                    )?;
                }
            }
            Kind::Array(values) => {
                let actual = observed.array().ok_or_else(problem)?;
                if values.len() != actual.len() {
                    return Err(problem());
                }
                for (i, (a, b)) in actual.iter().zip(values).enumerate() {
                    walk(a, b, &format!("{path}[{i}]"), proof)?;
                }
            }
            Kind::Blob(bytes) => {
                if !matches!(&observed.kind,NodeKind::Blob(actual) if actual==bytes) {
                    return Err(problem());
                }
            }
            Kind::Null => {
                if observed.scalar() != Some("null") {
                    return Err(problem());
                }
            }
            Kind::Bool(value) => {
                if observed.scalar() != Some(if *value { "true" } else { "false" }) {
                    return Err(problem());
                }
            }
            Kind::I64(value) => {
                if observed.scalar().and_then(|v| v.parse::<i128>().ok()) != Some(*value as i128) {
                    return Err(problem());
                }
            }
            Kind::U64(value) => {
                if observed.scalar().and_then(|v| v.parse::<u128>().ok()) != Some(*value as u128) {
                    return Err(problem());
                }
            }
            Kind::F32(value) => {
                let number = observed
                    .scalar()
                    .and_then(|v| v.parse::<f64>().ok())
                    .ok_or_else(problem)?;
                if !number.is_finite() || number as f32 != *value {
                    return Err(problem());
                }
                proof.float32_checked += 1;
            }
            Kind::F64(value) => {
                let number = observed
                    .scalar()
                    .and_then(|v| v.parse::<f64>().ok())
                    .ok_or_else(problem)?;
                if !number.is_finite() || number != *value {
                    return Err(problem());
                }
                proof.float64_checked += 1;
            }
            Kind::String(value) => {
                if observed.scalar() != Some(value) {
                    return Err(problem());
                }
            }
        }
        Ok(())
    }
    let mut proof = NumericProof::default();
    walk(observed, expected, "$", &mut proof)?;
    Ok(proof)
}

/// Split native CLI envelopes without injecting log/timing prefixes into KV3.
pub fn parse_cli_blocks(text: &str) -> Result<Vec<Document>> {
    let marker = "--- Data for block \"";
    let mut chunks = Vec::new();
    if text.contains(marker) {
        for chunk in text.split(marker).skip(1) {
            let header = chunk
                .find("\" ---")
                .ok_or_else(|| invalid("malformed CLI block marker"))?;
            chunks.push(&chunk[header + 5..]);
        }
    } else {
        chunks.push(text);
    }
    let mut documents = Vec::new();
    for chunk in chunks {
        let body = if let Some(end) = chunk.find("-->") {
            &chunk[end + 3..]
        } else {
            chunk
        };
        if let (Some(start), Some(end)) = (body.find('{'), body.rfind('}')) {
            if end > start {
                documents.push(Document::parse(body[start..=end].to_owned())?);
            }
        }
    }
    if documents.is_empty() {
        return Err(invalid("native reader returned no readable KV3 block"));
    }
    Ok(documents)
}
