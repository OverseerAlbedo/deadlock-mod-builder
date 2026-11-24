//! Prepared materials are named "<mesh>__<source material>_<hash>". Game rips often
//! name the base color after the source material with no channel suffix.
use deadlock_core::CancellationToken;
use deadlock_materials::texture_candidates;
use std::{fs, path::Path};

fn names(found: &[String]) -> Vec<String> {
    found
        .iter()
        .map(|p| Path::new(p).file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn unsuffixed_source_material_images_are_the_base_color() {
    let temp = tempfile::tempdir().unwrap();
    for name in [
        "AB12345_01_Body.png",
        "AB12345_01_Body_M.png",
        "AB12345_01_Body_S.png",
        "AB12345_01_Body_SM.png",
        "AB12345_01_Hair.png",
        "AB12345_01_Hair_M.png",
        "AB12345_01_Face.png",
        "AB12345_01_MC_M.png",
        "Arti12345_01.png",
        "01.png",
    ] {
        fs::write(temp.path().join(name), b"image").unwrap();
    }
    let cancel = CancellationToken::new();
    for (material, color) in [
        ("models/heroes_wip/geist/materials/prepared/body__ab12345_01_body_0ae57f18.vmat", "AB12345_01_Body.png"),
        ("models/heroes_wip/geist/materials/prepared/body__ab12345_01_hair_1e8ef7d0.vmat", "AB12345_01_Hair.png"),
        ("models/heroes_wip/geist/materials/prepared/face__ab12345_01_face_9a2261c9.vmat", "AB12345_01_Face.png"),
        ("models/heroes_wip/geist/materials/prepared/rose__arti12345_01_e3f27612.vmat", "Arti12345_01.png"),
    ] {
        let found = texture_candidates(material, temp.path(), &cancel).unwrap();
        assert_eq!(names(&found["color"]), vec![color.to_string()], "{material}");
        // Toon masks and shadow maps (_M, _S, _SM) are not guessed as PBR channels.
        for channel in ["normal", "roughness", "metalness", "ao", "emission", "opacity"] {
            assert!(found[channel].is_empty(), "{material} {channel}");
        }
    }
    // A material named exactly like an unsuffixed image still matches it.
    let found = texture_candidates(
        "models/heroes_wip/geist/materials/prepared/ab12345_01_body_0ae57f18.vmat",
        temp.path(),
        &cancel,
    )
    .unwrap();
    assert_eq!(names(&found["color"]), vec!["AB12345_01_Body.png".to_string()]);
}
