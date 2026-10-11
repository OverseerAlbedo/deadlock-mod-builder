//! Check the real captured model's final structure gate, after motion repair.
use super::*;

#[test]
#[ignore = "requires DMB_GRAVES_INPUT from the supplied failed Windows build"]
fn captured_graves_reaches_final_mesh_material_and_ag2_validation() {
    let root = PathBuf::from(std::env::var("DMB_GRAVES_INPUT").unwrap());
    let original = fs::read(root.join("input/original.vmdl_c")).unwrap();
    let native = fs::read(root.join("compiler/model.vmdl_c")).unwrap();
    let read_json = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let source = serde_json::from_value(read_json("workspace/source-skeleton-identity.json")["extension"].clone()).unwrap();
    let plan = read_json("workspace/secondary-motion-source.json");
    let names = |key: &str| -> BTreeSet<String> {
        plan["groups"].as_array().unwrap().iter().flat_map(|g| g[key].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_owned())).collect()
    };
    let chains: Vec<deadlock_cloth::generated::roll_lock::Chain> = serde_json::from_value(plan["roll_lock_chains"].clone()).unwrap();
    let (output,audit) = deadlock_cloth::generated::apply_with_roll_lock(
        &original,&native,&source,&names("bones"),&names("weighted_bones"),&chains,
    ).unwrap();
    let data = container::read_blocks(&output,"DATA").unwrap();
    let reference = Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
    let inspection = read_json("workspace/inspection.json");
    let ag2: Vec<ag2::Reference> = serde_json::from_value(inspection["original"]["ag2"].clone()).unwrap();
    let mappings = serde_json::from_value(inspection["mappings"].clone()).unwrap();
    let materials: Vec<String> = serde_json::from_value(inspection["dmx"]["materials"].clone()).unwrap();
    let structure = compiled_structure_with_custom_bones(
        &output,&reference,&ag2,&mappings,&materials,&audit.custom_bones,true,
    ).unwrap();
    assert_eq!(structure.meshes.meshes.len(),17);
    assert!(structure.mesh_mapping_passed && structure.ag2_graph_identities_preserved);
    if let Ok(folder) = std::env::var("DMB_GRAVES_OUTPUT") {
        fs::write(PathBuf::from(folder).join("compiled-structure.json"),serde_json::to_vec_pretty(&structure).unwrap()).unwrap();
    }
    println!("Graves final structure passed: 17 reviewed meshes, replacement materials and original AG2 identities.");
}
