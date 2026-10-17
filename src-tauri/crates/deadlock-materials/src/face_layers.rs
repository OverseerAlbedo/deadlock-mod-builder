//! Face layers: materials a source keeps fully transparent until an expression
//! fades them in (MMD blush, tears ...). Preparation records them as the
//! material's `face_layer` (blush, tears or other); a game build has no
//! expressions, so each kind is hidden (fully transparent) unless the user
//! shows it (Appearance > Face details, `Customization.face_layers`). A shown
//! layer is drawn with soft edges from its own image's alpha. SiViolet's blush
//! (赤面, 照れ) and tears (涙) were drawn solid and turned her face red.
use crate::*;
use deadlock_project::customization::Customization;
use std::io::Cursor;

/// The kinds of face layer, in the order the app lists them.
pub const FACE_LAYERS: [&str; 3] = ["blush", "tears", "other"];

/// The face layer kind of a prepared material, if it is one.
pub fn face_layer(config: &MaterialChoices) -> Option<&str> {
    config.extensions.get("face_layer").and_then(|v| v.as_str())
}

/// Writes each face layer material as hidden or shown. Runs after the material
/// files and appearance changes are written; returns the changed materials.
pub fn patch_face_layers(
    content: &Path,
    plans: &Plans,
    state: &Customization,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    let shown = |kind: &str| state.enabled == Some(true) && state.face_layers.as_ref().is_some_and(|f| f.shown(kind));
    let mut staged = Vec::new();
    let mut masks = Vec::new();
    for (material, config) in plans {
        cancel.check()?;
        let Some(kind) = face_layer(config) else { continue };
        if config.mode.as_deref() == Some("keep") {
            continue;
        }
        let path = inside(content, material)?;
        if !path.is_file() {
            return Err(invalid(format!("Material source missing for a face layer: {material}")));
        }
        let text = read_text(&path)?;
        let (params, _) = crate::appearance::layer(&text)?;
        let edits: Vec<(&str, String)> = if shown(kind) {
            // Soft edges from the image's own alpha; an image without
            // transparency is drawn as it is.
            let color = params.get("TextureColor1").map(|t| t.value.clone()).filter(|v| !v.trim_start().starts_with('['));
            match color {
                Some(color) => match alpha_mask(content, &color, cancel)? {
                    Some((relative, png)) => {
                        masks.push((inside(content, &relative)?, png));
                        vec![("F_TRANSLUCENT", "1".into()), ("TextureTranslucency1", relative)]
                    }
                    None => vec![],
                },
                None => vec![],
            }
        } else {
            vec![("F_TRANSLUCENT", "1".into()), ("TextureTranslucency1", crate::appearance::vector(&[0.0; 3]))]
        };
        if edits.is_empty() {
            continue;
        }
        let patched = crate::appearance::apply_edits(&text, edits)?;
        if patched != text {
            staged.push((material.clone(), path, patched));
        }
    }
    for (path, png) in masks {
        cancel.check()?;
        safety::atomic_write(&path, png)?;
    }
    let mut changed = Vec::new();
    for (material, path, text) in staged {
        cancel.check()?;
        safety::atomic_write(&path, text)?;
        changed.push(material);
    }
    Ok(changed)
}

/// A grayscale transparency mask (white = opaque) from the alpha of the staged
/// color image at `color` (content-relative), written beside it as
/// `<name>_a.png`; none when the image has no transparency.
fn alpha_mask(content: &Path, color: &str, cancel: &CancellationToken) -> Result<Option<(String, Vec<u8>)>> {
    cancel.check()?;
    let source = inside(content, color)?;
    let image = image::load_from_memory(&fs::read(&source)?)
        .map_err(|e| invalid(format!("Face layer image cannot be read ({color}): {e}")))?;
    if !image.color().has_alpha() {
        return Ok(None);
    }
    let rgba = image.to_rgba8();
    let mask = image::GrayImage::from_fn(rgba.width(), rgba.height(), |x, y| image::Luma([rgba.get_pixel(x, y)[3]]));
    if mask.pixels().all(|p| p[0] == 255) {
        return Ok(None);
    }
    let mut png = Vec::new();
    mask.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| invalid(format!("Face layer mask cannot be written: {e}")))?;
    let stem = color.rsplit_once('.').map_or(color, |(stem, _)| stem);
    Ok(Some((format!("{stem}_a.png"), png)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_project::customization::FaceLayers;

    fn material(dir: &Path, name: &str, color: &str) -> String {
        let relative = format!("models/test/materials/{name}.vmat");
        let path = dir.join(&relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("\"Layer0\"\n{{\n\t\"shader\"\t\"pbr.vfx\"\n\t\"TextureColor1\"\t\"{color}\"\n}}\n")).unwrap();
        relative
    }
    fn plans(entries: &[(&str, Option<&str>)]) -> Plans {
        entries
            .iter()
            .map(|(name, kind)| {
                let mut config = MaterialChoices::default();
                if let Some(kind) = kind {
                    config.extensions.insert("face_layer".into(), serde_json::json!(kind));
                }
                (name.to_string(), config)
            })
            .collect()
    }

    #[test]
    fn face_layers_are_hidden_unless_shown_and_shown_ones_use_their_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let texture = "models/test/textures/builder/blush.png";
        fs::create_dir_all(dir.path().join("models/test/textures/builder")).unwrap();
        // A blush image: transparent except a soft spot.
        let image = image::RgbaImage::from_fn(4, 4, |x, y| image::Rgba([255, 0, 0, if (x, y) == (1, 1) { 200 } else { 0 }]));
        image.save(dir.path().join(texture)).unwrap();
        let blush = material(dir.path(), "blush", texture);
        let tears = material(dir.path(), "tears", "[1.000000 1.000000 1.000000 0.000000]");
        let skin = material(dir.path(), "skin", texture);
        let all = plans(&[(&blush, Some("blush")), (&tears, Some("tears")), (&skin, None)]);
        let cancel = CancellationToken::new();
        let read = |m: &str| fs::read_to_string(dir.path().join(m)).unwrap();

        // Default: every face layer is fully transparent; other materials stay.
        let skin_before = read(&skin);
        let changed = patch_face_layers(dir.path(), &all, &Customization::default(), &cancel).unwrap();
        assert_eq!(changed, vec![blush.clone(), tears.clone()]);
        for m in [&blush, &tears] {
            let text = read(m);
            assert!(text.contains("\"F_TRANSLUCENT\"\t\"1\"") && text.contains("\"TextureTranslucency1\"\t\"[0 0 0 1]\""), "{text}");
        }
        assert_eq!(read(&skin), skin_before);

        // Shown blush: a mask from the image's alpha drives its transparency.
        material(dir.path(), "blush", texture);
        material(dir.path(), "tears", "[1.000000 1.000000 1.000000 0.000000]");
        let mut state = Customization { enabled: Some(true), ..Default::default() };
        state.face_layers = Some(FaceLayers { blush: Some(true), ..Default::default() });
        patch_face_layers(dir.path(), &all, &state, &cancel).unwrap();
        let text = read(&blush);
        assert!(text.contains("\"TextureTranslucency1\"\t\"models/test/textures/builder/blush_a.png\""), "{text}");
        let mask = image::open(dir.path().join("models/test/textures/builder/blush_a.png")).unwrap().to_luma8();
        assert_eq!((mask.get_pixel(1, 1)[0], mask.get_pixel(0, 0)[0]), (200, 0));
        // Tears stay hidden; a disabled customization hides everything again.
        assert!(read(&tears).contains("\"TextureTranslucency1\"\t\"[0 0 0 1]\""));
        material(dir.path(), "blush", texture);
        state.enabled = Some(false);
        patch_face_layers(dir.path(), &all, &state, &cancel).unwrap();
        assert!(read(&blush).contains("\"TextureTranslucency1\"\t\"[0 0 0 1]\""));
    }
}
