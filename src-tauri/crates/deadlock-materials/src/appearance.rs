use crate::*;
use deadlock_core::text::casefold;
use deadlock_project::customization::{AppearanceMaterial, Customization};
use std::collections::{BTreeMap, BTreeSet};

pub fn validate_appearance(state: &Customization) -> Result<()> {
    let Some(appearance) = &state.appearance else {
        return Ok(());
    };
    let rows = appearance.materials.as_deref().unwrap_or(&[]);
    if rows.len() > 256 {
        return Err(invalid(
            "Appearance supports at most 256 material overrides.",
        ));
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        let target = row.target.as_deref().unwrap_or("");
        if !target.is_empty() {
            safety::relative_asset(target)?;
            if !target.to_ascii_lowercase().ends_with(".vmat") {
                return Err(invalid("Appearance selects a source .vmat material."));
            }
            if row.enabled.unwrap_or(true) && !seen.insert(casefold(target)) {
                return Err(invalid(format!("Duplicate appearance override: {target}")));
            }
        }
        validate_row(row)?;
    }
    Ok(())
}
fn validate_row(row: &AppearanceMaterial) -> Result<()> {
    for (name, value, high) in [
        ("roughness", row.roughness, 1.0),
        ("metalness", row.metalness, 1.0),
        ("opacity", row.opacity, 1.0),
        ("emission", row.emission, 64.0),
    ] {
        if value.is_some_and(|v| !v.is_finite() || !(0.0..=high).contains(&v)) {
            return Err(invalid(format!(
                "{name} must be a finite number from 0 to {high}."
            )));
        }
    }
    if row
        .color
        .is_some_and(|c| c.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
    {
        return Err(invalid(
            "Appearance color needs three finite RGB values from 0 to 1.",
        ));
    }
    if let Some(adjustment) = crate::color_adjust::Adjustment::from_row(row) {
        adjustment.validate()?;
    }
    Ok(())
}
fn selections(state: &Customization) -> Result<&[AppearanceMaterial]> {
    validate_appearance(state)?;
    if !state.enabled.unwrap_or(false) {
        return Ok(&[]);
    }
    let Some(appearance) = &state.appearance else {
        return Ok(&[]);
    };
    if !appearance.enabled.unwrap_or(false) {
        return Ok(&[]);
    }
    Ok(appearance.materials.as_deref().unwrap_or(&[]))
}
pub fn apply_materials(plans: &Plans, state: &Customization) -> Result<Plans> {
    for row in selections(state)?
        .iter()
        .filter(|r| r.enabled.unwrap_or(true))
    {
        let target = row.target.as_deref().unwrap_or("");
        if !plans.contains_key(target) {
            return Err(invalid(format!("Choose a material from the analyzed character before applying appearance: {target:?}")));
        }
    }
    Ok(plans.clone())
}
#[derive(Debug)]
pub(crate) struct Token {
    pub(crate) value: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
    structural: bool,
}
fn tokens(text: &str) -> Result<Vec<Token>> {
    let mut out = Vec::new();
    let mut p = 0;
    let bytes = text.as_bytes();
    while p < bytes.len() {
        let c = text[p..]
            .chars()
            .next()
            .ok_or_else(|| invalid("Invalid VMAT position."))?;
        if c.is_whitespace() {
            p += c.len_utf8();
            continue;
        }
        if text[p..].starts_with("//") {
            p = text[p..].find('\n').map(|n| p + n).unwrap_or(bytes.len());
            continue;
        }
        if text[p..].starts_with("/*") {
            let n = text[p + 2..]
                .find("*/")
                .ok_or_else(|| invalid("Unterminated VMAT comment."))?;
            p += n + 4;
            continue;
        }
        let start = p;
        if bytes[p] == b'"' {
            p += 1;
            let mut closed = false;
            while p < bytes.len() {
                match bytes[p] {
                    b'\\' => {
                        p += 1;
                        if p >= bytes.len() {
                            break;
                        }
                        p += 1;
                    }
                    b'"' => {
                        p += 1;
                        closed = true;
                        break;
                    }
                    _ => p += 1,
                }
            }
            if !closed {
                return Err(invalid("Unterminated VMAT string."));
            }
            let value = serde_json::from_str(&text[start..p])
                .map_err(|_| invalid("Unsupported escaping in VMAT source."))?;
            out.push(Token {
                value,
                start,
                end: p,
                structural: false,
            });
        } else if matches!(bytes[p], b'{' | b'}') {
            p += 1;
            out.push(Token {
                value: text[start..p].into(),
                start,
                end: p,
                structural: true,
            });
        } else {
            while p < bytes.len() {
                let c = text[p..]
                    .chars()
                    .next()
                    .ok_or_else(|| invalid("Invalid VMAT source."))?;
                if c.is_whitespace() || matches!(c, '{' | '}' | '"') {
                    break;
                }
                p += c.len_utf8();
            }
            out.push(Token {
                value: text[start..p].into(),
                start,
                end: p,
                structural: false,
            });
        }
    }
    Ok(out)
}
pub(crate) fn layer(text: &str) -> Result<(BTreeMap<String, Token>, usize)> {
    let tokens = tokens(text)?;
    if tokens.len() < 3
        || tokens[0].value != "Layer0"
        || (!tokens[1].structural || tokens[1].value != "{")
        || tokens
            .last()
            .is_none_or(|v| !v.structural || v.value != "}")
    {
        return Err(invalid("Appearance edits require a flat Layer0 VMAT source; retain this material or author its shader manually."));
    }
    let end = tokens
        .last()
        .ok_or_else(|| invalid("Missing VMAT end."))?
        .start;
    let mut params = BTreeMap::new();
    let mut i = 2;
    while i < tokens.len() - 1 {
        if i + 1 >= tokens.len() - 1 || tokens[i].structural {
            return Err(invalid("Malformed VMAT parameter list."));
        }
        let value = &tokens[i + 1];
        if value.structural {
            return Err(invalid(
                "Layered/nested VMAT parameters need manual material authoring.",
            ));
        }
        if params
            .insert(
                tokens[i].value.clone(),
                Token {
                    value: value.value.clone(),
                    start: value.start,
                    end: value.end,
                    structural: value.structural,
                },
            )
            .is_some()
        {
            return Err(invalid(format!(
                "Ambiguous repeated VMAT parameter: {}",
                tokens[i].value
            )));
        }
        i += 2;
    }
    Ok((params, end))
}
pub(crate) fn number(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    let scientific = format!("{value:.8e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("scientific formatter emits exponent");
    let exponent: i32 = exponent.parse().expect("valid formatter exponent");
    if !(-4..9).contains(&exponent) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        format!("{mantissa}e{}{:+03}", "", exponent)
    } else {
        let precision = (8 - exponent).max(0) as usize;
        let fixed = format!("{value:.precision$}");
        if fixed.contains('.') {
            fixed.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            fixed
        }
    }
}
fn vector(values: &[f64; 3]) -> String {
    format!(
        "[{} {} {} 1]",
        number(values[0]),
        number(values[1]),
        number(values[2])
    )
}
pub fn patch_text(text: &str, row: &AppearanceMaterial) -> Result<String> {
    if row.color.is_none()
        && row.roughness.is_none()
        && row.metalness.is_none()
        && row.opacity.is_none()
        && row.emission.is_none()
    {
        return Ok(text.into());
    }
    validate_row(row)?;
    let (params, end) = layer(text)?;
    if params
        .get("shader")
        .is_none_or(|v| casefold(&v.value) != "pbr.vfx")
    {
        return Err(invalid("Appearance overrides currently support pbr.vfx materials. This shader is retained unchanged; edit its own verified parameters in the source material."));
    }
    let mut edits = Vec::new();
    if let Some(color) = row.color {
        edits.push(("g_vColorTint1", vector(&color)));
    }
    if let Some(value) = row.roughness {
        edits.push(("TextureRoughness1", vector(&[value; 3])));
    }
    if let Some(value) = row.metalness {
        edits.push(("TextureMetalness1", vector(&[value; 3])));
    }
    if let Some(value) = row.opacity {
        edits.push(("TextureTranslucency1", vector(&[value; 3])));
        edits.push(("F_TRANSLUCENT", "1".into()));
    }
    if let Some(value) = row.emission {
        edits.push((
            "F_SELF_ILLUM",
            if value == 0.0 { "0".into() } else { "1".into() },
        ));
        edits.push(("g_flSelfIllumScale1", number(value)));
        if !params.contains_key("TextureSelfIllumMask1") {
            edits.push(("TextureSelfIllumMask1", vector(&[1.0; 3])));
        }
    }
    let mut replacements = Vec::new();
    let mut added = String::new();
    for (key, value) in edits {
        if let Some(param) = params.get(key) {
            replacements.push((param.start, param.end, quote(&value)));
        } else {
            added.push_str(&format!("\n\t{}\t{}", quote(key), quote(&value)));
        }
    }
    if !added.is_empty() {
        added.push('\n');
        replacements.push((end, end, added));
    }
    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = text.to_owned();
    for (start, end, value) in replacements {
        result.replace_range(start..end, &value);
    }
    layer(&result)?;
    Ok(result)
}
pub fn patch_materials(
    content: &Path,
    plans: &Plans,
    state: &Customization,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    apply_materials(plans, state)?;
    let mut staged = Vec::new();
    for row in selections(state)?
        .iter()
        .filter(|r| r.enabled.unwrap_or(true))
    {
        cancel.check()?;
        let target = row.target.as_deref().unwrap_or("");
        let path = inside(content, target)?;
        if !path.is_file() {
            return Err(invalid(format!(
                "Material source missing for appearance: {target}"
            )));
        }
        let source = read_text(&path)?;
        let patched = crate::color_adjust::adjust_material(content, target, &patch_text(&source, row)?, row, cancel)?;
        if patched != source {
            staged.push((target.to_owned(), path, patched));
        }
    }
    let mut changed = Vec::new();
    for (target, path, text) in staged {
        cancel.check()?;
        safety::atomic_write(&path, text)?;
        changed.push(target);
    }
    Ok(changed)
}
