use crate::*;
use deadlock_core::text::casefold;
use deadlock_project::customization::NumericChoice;
use std::collections::{BTreeMap, BTreeSet};

pub const PRESETS: &[(&str, f64, f64)] = &[
    ("generic", 0.55, 0.0),
    ("cloth", 0.85, 0.0),
    ("painted metal", 0.4, 0.0),
    ("bare metal", 0.28, 1.0),
    ("skin", 0.5, 0.0),
    ("hair", 0.55, 0.0),
    ("emissive", 0.5, 0.0),
    ("transparent", 0.3, 0.0),
];
pub const CHANNELS: &[(&str, &[&str])] = &[
    (
        "color",
        &[
            "basecolor",
            "base_color",
            "albedo",
            "diffuse",
            "color",
            "colour",
        ],
    ),
    ("normal", &["normal", "norm", "nrm"]),
    ("roughness", &["roughness", "rough"]),
    ("metalness", &["metalness", "metallic", "metal"]),
    ("ao", &["ambientocclusion", "occlusion", "ao"]),
    ("emission", &["emissive", "emission", "selfillum"]),
    ("opacity", &["opacity", "alpha"]),
];
fn parameter(channel: &str) -> Result<&'static str> {
    match channel {
        "color" => Ok("TextureColor1"),
        "normal" => Ok("TextureNormal1"),
        "roughness" => Ok("TextureRoughness1"),
        "metalness" => Ok("TextureMetalness1"),
        "ao" => Ok("TextureAmbientOcclusion1"),
        "emission" => Ok("TextureSelfIllumMask1"),
        "opacity" => Ok("TextureTranslucency1"),
        _ => Err(invalid(format!("Unknown texture channel: {channel}"))),
    }
}
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
fn texture_extension(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "png" | "tga" | "jpg" | "jpeg" | "tif" | "tiff" | "exr"
    )
}
fn alias_match(suffix: &str, alias: &str) -> bool {
    suffix.match_indices(alias).any(|(at, _)| {
        let end = at + alias.len();
        (at == 0 || matches!(suffix.as_bytes()[at - 1], b'_' | b'.' | b'-'))
            && (end == suffix.len() || matches!(suffix.as_bytes()[end], b'_' | b'.' | b'-'))
    })
}
pub fn texture_candidates(
    material: &str,
    folder: &Path,
    cancel: &CancellationToken,
) -> Result<BTreeMap<String, Vec<String>>> {
    candidates_from_files(material, &texture_files(folder, cancel)?, cancel)
}

struct TextureFile {
    path: String,
    name: String,
    stem: String,
}
fn texture_files(folder: &Path, cancel: &CancellationToken) -> Result<Vec<TextureFile>> {
    if !folder.is_dir() {
        return Ok(Vec::new());
    }
    walk_files(folder, cancel)?
        .into_iter()
        .filter(|file| texture_extension(file))
        .map(|file| {
            Ok(TextureFile {
                path: path_text(&deadlock_project::asset_paths::resolve(&file)?)?,
                name: casefold(
                    file.file_name()
                        .and_then(|v| v.to_str())
                        .ok_or_else(|| invalid("Texture filename is not Unicode."))?,
                ),
                stem: casefold(
                    file.file_stem()
                        .and_then(|v| v.to_str())
                        .ok_or_else(|| invalid("Texture filename is not Unicode."))?,
                ),
            })
        })
        .collect()
}
// Conversion adds an eight-digit identity hash. Remove it only in the bridge's
// prepared-material namespace, never from arbitrary user material names.
fn prepared_identity<'a>(material: &str, base: &'a str) -> Option<&'a str> {
    if !material.contains("/materials/prepared/") {
        return None;
    }
    let (name, hash) = base.rsplit_once('_')?;
    (hash.len() == 8 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some(name)
}
fn words(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut numeric = false;
    for ch in value.chars() {
        if !ch.is_alphanumeric() || (!current.is_empty() && ch.is_ascii_digit() != numeric) {
            if !current.is_empty() {
                let value = if numeric {
                    current.trim_start_matches('0')
                } else {
                    &current
                };
                parts.push(if value.is_empty() {
                    "0".into()
                } else {
                    value.into()
                });
                current.clear();
            }
        }
        if ch.is_alphanumeric() {
            numeric = ch.is_ascii_digit();
            current.push(ch);
        }
    }
    if !current.is_empty() {
        let value = if numeric {
            current.trim_start_matches('0')
        } else {
            &current
        };
        parts.push(if value.is_empty() {
            "0".into()
        } else {
            value.into()
        });
    }
    parts
}
fn image_identity(stem: &str) -> Option<(Vec<String>, &'static str)> {
    let mut tokens = words(stem);
    let suffix = tokens.pop()?;
    let channel = match suffix.as_str() {
        "d" | "diff" | "diffuse" | "albedo" | "basecolor" | "color" | "colour" => "color",
        "n" | "normal" | "norm" | "nrm" => "normal",
        "rough" | "roughness" => "roughness",
        "metal" | "metallic" | "metalness" => "metalness",
        "ao" | "occlusion" | "ambientocclusion" => "ao",
        "emission" | "emissive" | "selfillum" => "emission",
        "alpha" | "opacity" => "opacity",
        // Packed maps and unknown suffixes are not guessed as base color.
        _ => return None,
    };
    if channel == "color" && tokens.last().is_some_and(|v| v == "base") {
        tokens.pop();
    }
    Some((tokens, channel))
}
fn same_identity(material: &[String], image: &[String]) -> bool {
    // Numeric-only and empty converted names carry too little information.
    if !material
        .iter()
        .any(|v| v.chars().filter(|c| c.is_alphabetic()).count() >= 3)
    {
        return false;
    }
    if image.ends_with(material) {
        return true;
    }
    // Body -> body_01 is useful only if the resulting candidate is unique.
    let numbered = image
        .last()
        .is_some_and(|v| v.bytes().all(|b| b.is_ascii_digit()));
    let material_numbered = material
        .last()
        .is_some_and(|v| v.bytes().all(|b| b.is_ascii_digit()));
    numbered && !material_numbered && image[..image.len() - 1].ends_with(material)
}
/// Prepared names join the mesh and the source material ("Body__CH01_Body"),
/// so the source material's own image name is a whole-word ending of it.
fn named_within(material: &[String], image: &[String]) -> bool {
    image
        .iter()
        .any(|v| v.chars().filter(|c| c.is_alphabetic()).count() >= 3)
        && material.len() > image.len()
        && material.ends_with(image)
}
fn candidates_from_files(
    material: &str,
    files: &[TextureFile],
    cancel: &CancellationToken,
) -> Result<BTreeMap<String, Vec<String>>> {
    let material = safety::relative_asset(material)?;
    let base = casefold(
        Path::new(&material)
            .file_stem()
            .and_then(|p| p.to_str())
            .ok_or_else(|| invalid("Material filename missing."))?,
    );
    let mut matches: BTreeMap<_, _> = CHANNELS
        .iter()
        .map(|(ch, _)| (ch.to_string(), Vec::new()))
        .collect();
    let identity = prepared_identity(&material, &base).map(words);
    let mut fallback: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in files {
        cancel.check()?;
        let name = &file.name;
        if !name.starts_with(&(base.clone() + "_")) && !name.starts_with(&(base.clone() + ".")) {
            if let Some(identity) = &identity {
                match image_identity(&file.stem) {
                    Some((tokens, channel)) => {
                        if same_identity(identity, &tokens) {
                            fallback
                                .entry(channel.into())
                                .or_default()
                                .push(file.path.clone());
                        }
                    }
                    None => {
                        // An image without any suffix that is named like the
                        // material is its base color: Body.png for "Body", or
                        // CH01_Body.png for a prepared "Body__CH01_Body".
                        let tokens = words(&file.stem);
                        if same_identity(identity, &tokens) || named_within(identity, &tokens) {
                            fallback
                                .entry("color".into())
                                .or_default()
                                .push(file.path.clone());
                        }
                    }
                }
            }
            continue;
        }
        let suffix = &name[base.len()..];
        let mut channels: Vec<_> = CHANNELS
            .iter()
            .filter(|(_, aliases)| aliases.iter().any(|a| alias_match(suffix, a)))
            .map(|(ch, _)| *ch)
            .collect();
        if channels.is_empty() {
            channels.push("color");
        }
        if channels.len() == 1 {
            matches
                .get_mut(channels[0])
                .expect("declared channel")
                .push(file.path.clone());
        }
    }
    for (channel, paths) in fallback {
        if matches[&channel].is_empty() {
            matches.insert(channel, paths);
        }
    }
    Ok(matches)
}
pub fn plan(
    material_paths: &[String],
    source_root: &Path,
    textures: &Path,
    configured: &Plans,
    cancel: &CancellationToken,
) -> Result<Plans> {
    plan_with_bindings(
        material_paths,
        source_root,
        textures,
        configured,
        &Plans::new(),
        cancel,
    )
}
/// Image links recovered from the same prepared Blender material take precedence
/// over filename suggestions. Explicit user choices, including no image, win.
pub fn plan_with_bindings(
    material_paths: &[String],
    source_root: &Path,
    textures: &Path,
    configured: &Plans,
    bindings: &Plans,
    cancel: &CancellationToken,
) -> Result<Plans> {
    let mut result = Plans::new();
    let files = texture_files(textures, cancel)?;
    for material in material_paths {
        cancel.check()?;
        let material = safety::relative_asset(material)?;
        if !material.ends_with(".vmat") {
            return Err(invalid(format!(
                "DMX material must reference a .vmat path: {material}"
            )));
        }
        let current = configured.get(&material).cloned().unwrap_or_default();
        let mut candidates = candidates_from_files(&material, &files, cancel)?;
        if let Some(binding) = bindings.get(&material) {
            for (channel, _) in CHANNELS {
                let mut paths = BTreeSet::new();
                if let Some(path) = binding
                    .textures
                    .as_ref()
                    .and_then(|m| m.get(*channel))
                    .filter(|p| !p.is_empty())
                {
                    paths.insert(path.clone());
                }
                if let Some(values) = binding.candidates.as_ref().and_then(|m| m.get(*channel)) {
                    paths.extend(values.iter().filter(|p| !p.is_empty()).cloned());
                }
                if !paths.is_empty() {
                    candidates.insert(channel.to_string(), paths.into_iter().collect());
                }
            }
        }
        let exists = inside(source_root, &material)?.is_file();
        let mut chosen = current.textures.unwrap_or_default();
        let mut ambiguous = Vec::new();
        let mut added = false;
        for (channel, _) in CHANNELS {
            let paths = &candidates[*channel];
            if !chosen.contains_key(*channel) {
                if paths.len() == 1 {
                    chosen.insert(channel.to_string(), paths[0].clone());
                    added = true;
                } else if paths.len() > 1 {
                    ambiguous.push(channel.to_string());
                }
            }
        }
        result.insert(
            material.clone(),
            MaterialChoices {
                preset: Some(current.preset.unwrap_or_else(|| "generic".into())),
                textures: Some(chosen),
                color: Some(current.color.unwrap_or([0.5; 3])),
                confirmed: Some(
                    if added
                        && (material.contains("/materials/prepared/")
                            || bindings.contains_key(&material))
                    {
                        false
                    } else {
                        current.confirmed.unwrap_or(exists)
                    },
                ),
                mode: Some(current.mode.unwrap_or_else(|| {
                    if exists {
                        "keep".into()
                    } else {
                        "generate".into()
                    }
                })),
                existing: Some(exists),
                ambiguous: Some(ambiguous),
                candidates: Some(candidates),
                roughness: Some(current.roughness.unwrap_or(NumericChoice::Inherit(()))),
                metalness: Some(current.metalness.unwrap_or(NumericChoice::Inherit(()))),
                extensions: current.extensions,
            },
        );
    }
    Ok(result)
}
fn rgba(value: &[f64; 3]) -> String {
    format!("[{:.6} {:.6} {:.6} 1.000000]", value[0], value[1], value[2])
}
pub fn material_text(config: &MaterialChoices, paths: &BTreeMap<String, String>) -> Result<String> {
    let preset = config.preset.as_deref().unwrap_or("generic");
    let (_, roughness, metalness) = PRESETS
        .iter()
        .find(|p| p.0 == preset)
        .ok_or_else(|| invalid(format!("Unknown material preset: {preset}")))?;
    let color = config.color.unwrap_or([0.5; 3]);
    if color
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(invalid(
            "Material color must have three values between 0 and 1.",
        ));
    }
    let mut params = vec![
        ("shader", "pbr.vfx".into()),
        ("F_USE_NPR_LIGHTING", "1".into()),
        ("F_USE_STATUS_EFFECTS_PROXY", "1".into()),
        ("TextureColor1", rgba(&[1.0; 3])),
        ("g_vColorTint1", rgba(&color)),
    ];
    for (channel, choice, fallback) in [
        ("roughness", config.roughness.as_ref(), *roughness),
        ("metalness", config.metalness.as_ref(), *metalness),
    ] {
        let value = choice.and_then(|v| v.value()).unwrap_or(fallback);
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(invalid(format!("{channel} must be 0–1.")));
        }
        params.push((parameter(channel)?, rgba(&[value; 3])));
    }
    fn set(params: &mut Vec<(&'static str, String)>, key: &'static str, value: String) {
        if let Some(row) = params.iter_mut().find(|v| v.0 == key) {
            row.1 = value;
        } else {
            params.push((key, value));
        }
    }
    // Python preserves configured texture insertion order; serialized profile
    // schema maps are ordered lexically in Rust. Parameter values are identical.
    for (channel, path) in paths {
        set(
            &mut params,
            parameter(channel)?,
            safety::relative_asset(path)?,
        );
    }
    if preset == "emissive" || paths.contains_key("emission") {
        set(&mut params, "F_SELF_ILLUM", "1".into());
        set(&mut params, "g_flSelfIllumScale1", "1".into());
        set(
            &mut params,
            "TextureSelfIllumMask1",
            paths
                .get("emission")
                .cloned()
                .unwrap_or_else(|| rgba(&[1.0; 3])),
        );
    }
    if preset == "transparent" {
        set(&mut params, "F_TRANSLUCENT", "1".into());
    }
    let mut out = "\"Layer0\"\n{\n".to_owned();
    for (key, value) in params {
        out.push_str(&format!("\t{}\t{}\n", quote(key), quote(&value)));
    }
    out.push_str("}\n");
    Ok(out)
}
pub fn generate(
    plans: &Plans,
    content: &Path,
    hero_root: &str,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    safety::relative_asset(hero_root)?;
    let mut staged = Vec::new();
    let mut copies = Vec::new();
    // Validate all material decisions before changing generated files.
    for (material, config) in plans {
        cancel.check()?;
        if config.mode.as_deref() == Some("keep") {
            if !inside(content, material)?.is_file() {
                return Err(invalid(format!("Missing material: {material}")));
            }
            continue;
        }
        if let Some(channels) = config.ambiguous.as_ref().filter(|v| !v.is_empty()) {
            return Err(invalid(format!(
                "Choose a texture for {material}: {}",
                channels.join(", ")
            )));
        }
        if config.confirmed != Some(true) {
            return Err(invalid(format!(
                "Review the material preset/color for {material}."
            )));
        }
        let mut paths = BTreeMap::new();
        for (channel, value) in config.textures.as_ref().into_iter().flatten() {
            if value.is_empty() {
                continue;
            }
            parameter(channel)?;
            let source = PathBuf::from(value);
            deadlock_vpk::check_unlinked_path(&source, false)?;
            if !source.is_file() {
                return Err(invalid(format!("Texture not found: {}", source.display())));
            }
            let name = source
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or_else(|| invalid("Texture filename missing."))?;
            let slug: String = name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || "_.-".contains(c) {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            use sha2::{Digest, Sha256};
            let tag = format!(
                "{:x}",
                Sha256::digest(
                    path_text(&deadlock_project::asset_paths::resolve(&source)?)?.as_bytes()
                )
            );
            let rel = format!("{hero_root}/textures/builder/{}_{slug}", &tag[..10]);
            copies.push((source, inside(content, &rel)?));
            paths.insert(channel.clone(), rel);
        }
        staged.push((
            material.clone(),
            inside(content, material)?,
            material_text(config, &paths)?,
        ));
    }
    for (source, destination) in copies {
        copy_file(&source, &destination, cancel)?;
    }
    let mut created = Vec::new();
    for (material, path, text) in staged {
        cancel.check()?;
        safety::atomic_write(&path, text)?;
        created.push(material);
    }
    Ok(created)
}
