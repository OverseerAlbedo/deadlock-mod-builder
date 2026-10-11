//! Validate a captured Blender preparation with the production native exporter.
//! Usage: validate_prepared INSPECTION SKELETON TARGET OPTIONS REPORT OUTPUT.dmx AUDIT.json
//! Skeleton authority is the supplied data; this does not run the game compiler.
use deadlock_preparation::{algorithms::Bone, fitting, secondary_bones, service::{Inspection, PreparedResponse}};
use deadlock_project::PreparationOptions;
use deadlock_skeleton::{export::{export_dmx, PreparedReport}, transforms::PreparedBone, Skeleton};
use std::{collections::BTreeMap, fs, path::Path};
fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    serde_json::from_slice(&fs::read(path).expect("read input")).expect("decode input")
}
fn main() {
    let a: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(a.len(), 7, "INSPECTION SKELETON TARGET OPTIONS REPORT OUTPUT.dmx AUDIT.json");
    assert!(!Path::new(&a[5]).exists() && !Path::new(&a[6]).exists(), "Refusing to overwrite existing output");
    let inspection: Inspection = read(&a[0]);
    let skeleton: Skeleton = read(&a[1]);
    let target: Vec<PreparedBone> = read(&a[2]);
    let options: PreparationOptions = read(&a[3]);
    let report: PreparedResponse = read(&a[4]);
    fitting::check_policy(&options, &report.rest_alignment.policy).expect("fitting policy");
    let plan = secondary_bones::selection(&inspection.bones, &inspection.weighted_bones,
        &target.iter().map(Bone::from).collect::<Vec<_>>(), &options).expect("secondary plan");
    let extended = secondary_bones::export_skeleton(&skeleton, &target, &report.target, &inspection.bones, plan.as_ref()).expect("unchanged original and exact extra binds");
    let receipt = export_dmx(&PreparedReport {meshes: report.meshes, extra: BTreeMap::new()}, &extended, Path::new(&a[5])).expect("native export and bind checks");
    let audit = serde_json::json!({"passed":true, "fit_mode":fitting::mode(&options).unwrap(),
        "original_bones":skeleton.names.len(), "exported_bones":extended.names.len(),
        "original_binds_unchanged":true, "export_validation":receipt,
        "source2_compiler_executed":false,"in_game_validated":false});
    fs::write(&a[6], serde_json::to_vec_pretty(&audit).unwrap()).unwrap();
    println!("Native DMX export validated: {} original bones, {} total", skeleton.names.len(), extended.names.len());
}
