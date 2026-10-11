//! Export aliases require identity-bound original evidence; source nodes are never renamed.
use crate::{invalid, skeleton_identity, Result, Skeleton};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SourceAuditReport {
    #[serde(default)]
    pub compiled_sha256: Option<String>,
    #[serde(default)]
    pub runtime_motion: RuntimeMotionSourceAudit,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RuntimeMotionSourceAudit {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub resource_sha256: Option<String>,
    #[serde(default)]
    pub cloth_nodes: usize,
    #[serde(default)]
    pub controls: Vec<String>,
    #[serde(default, deserialize_with = "cli_skeleton")]
    pub skeleton: Option<Skeleton>,
}

fn cli_skeleton<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Skeleton>, D::Error> {
    use serde::de::Error;
    let mut value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(skeleton) = value.as_mut() else {
        return Ok(None);
    };
    for key in ["m_nParent", "m_nFlag"] {
        if let Some(rows) = skeleton
            .get_mut(key)
            .and_then(serde_json::Value::as_array_mut)
        {
            for value in rows {
                if let Some(token) = value.as_str() {
                    let text = token.strip_prefix('+').unwrap_or(token);
                    let n = if let Some(hex) =
                        text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
                    {
                        i64::from_str_radix(hex, 16)
                    } else if let Some(hex) = text
                        .strip_prefix("-0x")
                        .or_else(|| text.strip_prefix("-0X"))
                    {
                        i64::from_str_radix(hex, 16).map(|v| -v)
                    } else {
                        text.parse::<i64>()
                    }
                    .map_err(D::Error::custom)?;
                    *value = serde_json::Value::from(n);
                }
            }
        }
    }
    fn numeric(value: &mut serde_json::Value) -> std::result::Result<(), String> {
        match value {
            serde_json::Value::Array(rows) => {
                for v in rows {
                    numeric(v)?;
                }
            }
            serde_json::Value::String(token) => {
                let n = token.parse::<f64>().map_err(|e| e.to_string())?;
                *value = serde_json::Value::Number(
                    serde_json::Number::from_f64(n).ok_or("Nonfinite source bind token")?,
                );
            }
            _ => (),
        }
        Ok(())
    }
    for key in ["m_bonePosParent", "m_boneRotParent", "m_boneScaleParent"] {
        if let Some(value) = skeleton.get_mut(key) {
            numeric(value).map_err(D::Error::custom)?;
        }
    }
    serde_json::from_value(value.unwrap())
        .map(Some)
        .map_err(D::Error::custom)
}
pub fn canonical_aliases(original: &SourceAuditReport) -> Result<BTreeMap<String, String>> {
    let audit = &original.runtime_motion;
    if audit.status != "audited" {
        return Ok(BTreeMap::new());
    }
    if audit.skeleton.is_none() && audit.cloth_nodes == 0 {
        return Ok(BTreeMap::new());
    }
    let sha = original
        .compiled_sha256
        .as_deref()
        .ok_or_else(|| invalid("Source skeleton export aliases require original SHA-256"))?;
    if sha.len() != 64
        || !sha
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || audit.resource_sha256.as_deref() != Some(sha)
    {
        return Err(invalid(
            "Source skeleton export aliases require an audit of the same original compiled model",
        ));
    }
    let skeleton = audit
        .skeleton
        .as_ref()
        .ok_or_else(|| invalid("Source skeleton export alias audit is incomplete"))?;
    skeleton.validate(true)?;
    skeleton_identity(skeleton, skeleton)?;
    let names: BTreeSet<_> = skeleton.names.iter().map(String::as_str).collect();
    Ok(skeleton
        .names
        .iter()
        .zip(&skeleton.flags)
        .filter_map(|(name, &flag)| {
            name.strip_prefix('$')
                .filter(|_| flag & 0x400008 == 0x400008)
                .map(|suffix| format!("_{suffix}"))
                .filter(|alias| !names.contains(alias.as_str()))
                .map(|alias| (alias, name.clone()))
        })
        .collect())
}

pub(crate) fn required_cli_skeleton<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Skeleton, D::Error> {
    use serde::de::Error;
    cli_skeleton(d)?.ok_or_else(|| D::Error::custom("Required skeleton missing"))
}
