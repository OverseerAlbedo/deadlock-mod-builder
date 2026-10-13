//! The native compiler writes the textures it makes next to their sources and
//! names them after them; a material's combined normal/roughness texture is
//! named after the material itself. Windows tools fail on paths of 260
//! characters or more, so the build checks the longest of these names before
//! compiling, when the cause can still be explained.
use crate::{inside, invalid, read_text, Result};
use std::path::Path;

/// The most characters a path may have for the native tools.
pub const NATIVE_PATH_LIMIT: usize = 259;
/// Readable characters a prepared material name keeps (material_textures.py
/// NAME_CHARS); longer names from older preparations shrink when prepared again.
const PREPARED_NAME_CHARS: usize = 24;
/// The longest name the compiler gives a texture it combines for a material:
/// `<material>_vmat_g_tnormalroughness_<hash>.vtex` in place of `<material>.vmat`.
const MATERIAL_TEXTURE: &str = "_vmat_g_tnormalroughness_00000000.vtex";

/// The compiler's texture for an image source: `<stem>_<ext>_<hash>.vtex`.
fn compiled(relative: &str) -> Option<String> {
    let (stem, extension) = relative.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    (!["vtex", "vmat"].contains(&extension.as_str())).then(|| format!("{stem}_{extension}_00000000.vtex"))
}

/// The path as the native tools see it (without the `\\?\` prefix).
fn native_length(path: &Path) -> usize {
    let text = path.as_os_str().to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).encode_utf16().count()
}

/// Check that the compiler can write every texture of these materials under
/// `content` (the addon's content folder).
pub fn check_native_paths(content: &Path, materials: &[String]) -> Result<()> {
    for material in materials {
        let file = inside(content, material)?;
        let mut names = vec![];
        if let Some(stem) = material.strip_suffix(".vmat") {
            names.push(format!("{stem}{MATERIAL_TEXTURE}"));
        }
        if file.is_file() {
            names.extend(crate::texture_references(&read_text(&file)?).iter().filter_map(|r| compiled(&r.replace('\\', "/"))));
        }
        for name in names {
            let path = content.join(&name);
            let length = native_length(&path);
            if length > NATIVE_PATH_LIMIT {
                return Err(invalid(too_long(material, &path, length)));
            }
        }
    }
    Ok(())
}

fn too_long(material: &str, path: &Path, length: usize) -> String {
    let over = length - NATIVE_PATH_LIMIT;
    let shown = path.to_string_lossy();
    let shown = shown.strip_prefix(r"\\?\").unwrap_or(&shown);
    // The readable part of a prepared material's name (before its hash).
    let readable = material
        .rsplit_once("/materials/prepared/")
        .and_then(|(_, file)| file.strip_suffix(".vmat")?.rsplit_once('_'))
        .map_or(0, |(name, _)| name.chars().count());
    let fix = if readable >= PREPARED_NAME_CHARS + over {
        "Prepare the model again (step 2, Prepare again): this version gives materials shorter names, and your material choices are kept."
    } else {
        "Move the Reduced CSDK12 folder to a shorter path (for example C:\\CSDK12) and choose it again in Setup, or give the material a shorter name in Blender and prepare the model again."
    };
    format!(
        "The compiler would write a texture of {material} at a path {length} characters long, but Windows tools stop at {NATIVE_PATH_LIMIT} ({over} too many): {shown}. {fix}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_compiled_texture_paths_are_found_before_compiling() {
        let temp = tempfile::tempdir().unwrap();
        // A content folder 135 characters long: the long frost material name
        // overflows there, a short one fits.
        let content = temp.path().join("c".repeat(134usize.saturating_sub(native_length(temp.path())).max(1)));
        let long = "models/heroes_staging/haze/materials/prepared/optional_frost___translucent_pale_ice_2362886d.vmat";
        let short = "models/heroes_staging/haze/materials/prepared/optional_frost___transl_2362886d.vmat";
        let error = check_native_paths(&content, &[long.into()]).unwrap_err().to_string();
        assert!(error.contains("Prepare the model again"), "{error}");
        assert!(error.contains("_vmat_g_tnormalroughness_"), "{error}");
        // The shorter name fits; images are checked under their compiled names too.
        let file = inside(&content, short).unwrap();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "\"Layer0\"\n{\n\t\"TextureColor\"\t\"models/heroes_staging/haze/textures/builder/ab12cd34ef_optional_frost___transl_color.png\"\n}\n").unwrap();
        check_native_paths(&content, &[short.into()]).unwrap();
        std::fs::write(&file, format!("\"Layer0\"\n{{\n\t\"TextureColor\"\t\"models/heroes_staging/haze/textures/builder/{}.png\"\n}}\n", "x".repeat(80))).unwrap();
        let error = check_native_paths(&content, &[short.into()]).unwrap_err().to_string();
        assert!(error.contains("_png_00000000.vtex") && error.contains("Reduced CSDK12"), "{error}");
    }
}
