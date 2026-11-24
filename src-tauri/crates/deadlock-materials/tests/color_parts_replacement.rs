//! Replacement materials: a color range that gives a grey a new color and a
//! range for one part, on a staged PNG copy (synthetic image and meshes): with
//! the app's switches the grey turns violet and a part range changes only its
//! part's pixels; a switch turned off (feature `color-test-switches`, test
//! builds only) refuses its range.
use deadlock_core::CancellationToken;
use deadlock_materials::color_adjust::{adjust_material, adjust_material_with};
use deadlock_project::{
    color_parts::{PartMesh, PartMeshes},
    color_ranges::{self, ColorPart},
    customization::{AppearanceMaterial, HeroColorRange},
};
use std::{fs, path::Path};

const VMAT: &str = "Layer0\n{\n\tshader \"pbr.vfx\"\n\tTextureColor1 \"models/hero/textures/body.png\"\n}\n";
const TARGET: &str = "models/hero/materials/body.vmat";

/// Light grey, 8 x 8.
fn staged(dir: &Path) {
    let image = image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 200, 200, 255]));
    let path = dir.join("models/hero/textures/body.png");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image.save(&path).unwrap();
}
fn grey_to_violet(part: Option<ColorPart>) -> AppearanceMaterial {
    AppearanceMaterial {
        target: Some(TARGET.into()),
        ranges: Some(vec![HeroColorRange {
            saturation_range: Some([0., 0.2]),
            brightness_range: Some([0.55, 1.]),
            set_hue: Some(270.),
            set_saturation: Some(0.6),
            part,
            ..Default::default()
        }]),
        ..Default::default()
    }
}
/// A quad over the middle of the left half of the image.
struct Left;
impl PartMeshes for Left {
    fn part_meshes(&self, material: &str) -> Result<Vec<PartMesh>, String> {
        assert_eq!(material, TARGET);
        Ok(vec![PartMesh {
            positions: vec![0., 0., 0., 1., 0., 0., 1., 1., 0., 0., 1., 0.],
            uvs: vec![0.25, 0.25, 0.5, 0.25, 0.5, 0.75, 0.25, 0.75],
            indices: vec![0, 1, 2, 0, 2, 3],
        }])
    }
}

#[test]
fn grey_and_part_ranges_on_a_replacement_image() {
    let temp = tempfile::tempdir().unwrap();
    staged(temp.path());
    let cancel = CancellationToken::new();
    let row = grey_to_violet(Some(ColorPart { u: 0.3, v: 0.5 }));
    // With the app's switches (both on).
    assert_eq!(color_ranges::switches(), color_ranges::Switches::ALL);
    // Grey everywhere: the whole image turns violet, shading kept.
    let out = adjust_material(temp.path(), TARGET, VMAT, &grey_to_violet(None), &cancel).unwrap();
    let name = out.split('"').find(|s| s.ends_with(".png") && s.contains("_dmb_")).unwrap().to_owned();
    let all = image::open(temp.path().join(&name)).unwrap().to_rgba8();
    assert!(all.pixels().all(|p| p.0 == all.get_pixel(0, 0).0 && p[2] > p[1] + 30), "{:?}", all.get_pixel(0, 0));
    // One part: only its pixels, one pixel of reach around them.
    let out = adjust_material_with(temp.path(), TARGET, VMAT, &row, Some(&Left), &cancel).unwrap();
    let name = out.split('"').find(|s| s.ends_with(".png") && s.contains("_dmb_")).unwrap().to_owned();
    let image = image::open(temp.path().join(&name)).unwrap().to_rgba8();
    for (x, y, pixel) in image.enumerate_pixels() {
        let inside = (1..=4).contains(&x) && (1..=6).contains(&y);
        assert_eq!(pixel[2] > pixel[1] + 30, inside, "pixel {x},{y}: {pixel:?}");
        assert_eq!(pixel[3], 255);
    }
    // The same choices are the same copy.
    assert_eq!(adjust_material_with(temp.path(), TARGET, VMAT, &row, Some(&Left), &cancel).unwrap(), out);
    // Without the model's meshes a part range is refused, never applied everywhere.
    let error = adjust_material(temp.path(), TARGET, VMAT, &row, &cancel).unwrap_err().to_string();
    assert!(error.contains("could not be read"), "{error}");
    // A plain base color has no part to change.
    let plain = "Layer0 { shader pbr.vfx TextureColor1 \"[0.8 0.8 0.8 1]\" }";
    let error = adjust_material_with(temp.path(), TARGET, plain, &row, Some(&Left), &cancel).unwrap_err().to_string();
    assert!(error.contains(deadlock_materials::color_adjust::PART_NEEDS_IMAGE), "{error}");
    // With a switch turned off (test builds only) the build refuses the range.
    #[cfg(feature = "color-test-switches")]
    {
        color_ranges::open_for_test(Some(color_ranges::Switches::NONE));
        let error = adjust_material(temp.path(), TARGET, VMAT, &grey_to_violet(None), &cancel).unwrap_err().to_string();
        color_ranges::open_for_test(None);
        assert!(error.contains("new color of its own"), "{error}");
    }
}