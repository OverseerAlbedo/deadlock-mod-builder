//! Offline repro: inspect JSON + a real target DATA skeleton, without game tools.
use std::{fs, path::PathBuf};
use deadlock_preparation::{algorithms::Bone, automap};
use deadlock_skeleton::{Skeleton, transforms::prepare_target};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 3, "inspection.json original.vmdl_c output.json");
    let source: serde_json::Value = serde_json::from_slice(&fs::read(&args[0]).unwrap()).unwrap();
    let data = deadlock_resource::container::read_blocks(&fs::read(&args[1]).unwrap(), "DATA").unwrap();
    let skeleton = Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
    let (target, scale_audit) = prepare_target(&skeleton).unwrap();
    let bones: Vec<Bone> = serde_json::from_value(source["bones"].clone()).unwrap();
    let weighted: Vec<String> = serde_json::from_value(source["weighted_bones"].clone()).unwrap();
    let suggestions = automap::suggest(&bones, &weighted, &target.iter().map(Bone::from).collect::<Vec<_>>(), &Default::default()).unwrap();
    fs::write(PathBuf::from(&args[2]), serde_json::to_vec_pretty(&serde_json::json!({
        "source":bones,"weighted":weighted,"target":target,"target_skeleton":skeleton,
        "suggestions":suggestions,"scale_audit":scale_audit,
        "native_compiled":false,"in_game_tested":false,
    })).unwrap()).unwrap();
}
