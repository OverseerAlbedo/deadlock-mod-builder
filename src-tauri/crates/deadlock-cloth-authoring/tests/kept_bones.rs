//! Preserve original physics with kept source bones on a real compile. Game
//! models are never fixtures: set DMB_KEPT_BONES to a folder holding the
//! game's `original.vmdl_c`, the compiler's `native.vmdl_c` (a Preserve build's
//! runtime-motion/native-model backup) and the project's `prepared.dmx`.
use deadlock_cloth_authoring::surface;
use deadlock_dmx::Dmx;
use deadlock_skeleton::extensions::SourceBinding;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[test]
fn kept_bones_take_the_compilers_animation_rest_values() {
    let Some(folder) = std::env::var_os("DMB_KEPT_BONES") else { return };
    let folder = Path::new(&folder);
    let original = fs::read(folder.join("original.vmdl_c")).unwrap();
    let native = fs::read(folder.join("native.vmdl_c")).unwrap();
    let d = Dmx::parse(fs::read(folder.join("prepared.dmx")).unwrap()).unwrap();
    let mut skeleton = surface::binds(&d).unwrap().skeleton;
    skeleton.flags = vec![64; skeleton.names.len()];
    let extras: Vec<String> = skeleton.names.iter().filter(|n| n.starts_with("dmb_secondary_")).cloned().collect();
    let source = SourceBinding {
        source_sha256: format!("{:x}", Sha256::digest(d.bytes())),
        original_sha256: format!("{:x}", Sha256::digest(&original)),
        skeleton,
        extra_names: extras.clone(),
        required_extra_names: extras,
    };
    let (restored, receipt) = deadlock_cloth::restore(&original, &native, Some(&source)).unwrap();
    let repair = receipt.kept_bind_repair.expect("kept bones reconciled");
    println!("{} kept bones corrected; largest position change {:.6} in, scale {:.6}", repair.corrected_bones.len(), repair.max_position_component_correction_inches, repair.max_scale_correction);
    assert!(!repair.corrected_bones.is_empty());
    assert!(!receipt.data_changed_only_cloth_procedural_flags);
    // The build's own post-restoration check passes on the result.
    assert!(deadlock_cloth::validate_motion(&original, &restored, Some(&source)).unwrap().passed);
}
