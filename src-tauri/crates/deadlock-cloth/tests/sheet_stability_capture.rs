//! Compatibility replay against a returned native build; not a fresh compile.
use deadlock_cloth::generated;
use deadlock_resource::{binary,container,Kind,Value};
use deadlock_skeleton::extensions::SourceBinding;
use std::{collections::{BTreeMap,BTreeSet},fs,path::PathBuf};

#[test]
#[ignore="requires DMB_FABRIC_CAPTURE and DMB_FABRIC_PLAN"]
fn native_fabric_limits_preserve_other_physics_and_are_rechecked_on_install() {
    let root=PathBuf::from(std::env::var("DMB_FABRIC_CAPTURE").unwrap());
    let original=fs::read(root.join("input/original.vmdl_c")).unwrap();
    let native=fs::read(root.join("compiler/model.vmdl_c")).unwrap();
    let json=|p:PathBuf|serde_json::from_slice::<serde_json::Value>(&fs::read(p).unwrap()).unwrap();
    let identity=json(root.join("workspace/source-skeleton-identity.json"));
    let source:SourceBinding=serde_json::from_value(identity["extension"].clone()).unwrap();
    let old_plan=json(root.join("workspace/secondary-motion-source.json"));
    let setup=json(PathBuf::from(std::env::var("DMB_FABRIC_PLAN").unwrap()));
    let mut plan:generated::sheet_stability::Plan=serde_json::from_value(setup["long_skirt"]["stability"].clone()).unwrap();
    let names=|key:&str|->BTreeSet<String>{old_plan["groups"].as_array().unwrap().iter().flat_map(|g|g[key].as_array().unwrap().iter().map(|n|n.as_str().unwrap().to_owned())).collect()};
    let chains:Vec<generated::roll_lock::Chain>=serde_json::from_value(old_plan["roll_lock_chains"].clone()).unwrap();
    let (selected,weighted)=(names("bones"),names("weighted_bones"));
    let (baseline,_)=generated::apply_with_roll_lock(&original,&native,&source,&selected,&weighted,&chains).unwrap();
    // This returned build predates the lateral sheet connections. It must
    // reject the new complete plan rather than invent native seam evidence.
    assert!(generated::apply_with_fabric(&original,&native,&source,&selected,&weighted,&chains,&plan)
        .unwrap_err().to_string().contains("disconnected seam"));
    let indices=source.skeleton.indices();
    plan.hinges.retain(|h|source.skeleton.parents[indices[h[1].as_str()]]==indices[h[0].as_str()] as i32
        && source.skeleton.parents[indices[h[2].as_str()]]==indices[h[1].as_str()] as i32);
    let (output,audit)=generated::apply_with_fabric(&original,&native,&source,&selected,&weighted,&chains,&plan).unwrap();
    generated::validate_with_fabric(&original,&native,&output,&source,&selected,&weighted,&chains,&plan).unwrap();
    assert!(audit.sheet_stability.bend_limit_rods>0);
    assert!(audit.original_rigid_body_physics_preserved && audit.compiled_bindings.passed);
    let old=container::read_blocks(&baseline,"PHYS").unwrap();
    let mut new=container::read_blocks(&output,"PHYS").unwrap().remove(0);
    let a=old[0].root.get("m_pFeModel").unwrap();let b=new.root.get("m_pFeModel").unwrap();
    for (key,value) in a.object().unwrap(){
        if !["m_Rods","m_SimdRods"].contains(&key.as_str()){assert!(value.canonical_eq(b.get(key).unwrap()),"{key}");}
        else {assert!(value.array().unwrap().iter().zip(b.get(key).unwrap().array().unwrap()).all(|(x,y)|x.canonical_eq(y)));}
    }
    for key in ["DATA","MDAT","AGRP","CTRL"] {
        let a=container::read_blocks(&baseline,key).unwrap();let b=container::read_blocks(&output,key).unwrap();
        assert_eq!(a.len(),b.len());assert!(a.iter().zip(&b).all(|(a,b)|a.root.canonical_eq(&b.root)),"{key}");
    }
    if let Some(Value{kind:Kind::Array(rods),..})=new.root.get_mut("m_pFeModel").unwrap().get_mut("m_Rods") {
        *rods.last_mut().unwrap().get_mut("flMinDist").unwrap()=Value::new(Kind::F32(0.));
    }
    let tampered=container::rebuild(&output,&BTreeMap::from([("PHYS".into(),binary::encode(&new.root,new.format_guid).unwrap())])).unwrap();
    assert!(generated::validate_with_fabric(&original,&native,&tampered,&source,&selected,&weighted,&chains,&plan).is_err());
    // Reproduce a compiler-emitted flexible support edge on the returned
    // native model. Its interval overlaps the planned local support distance.
    // Build and Install must agree on tightening both scalar and SIMD copies.
    let mut flexible=container::read_blocks(&native,"PHYS").unwrap().remove(0);
    let fe=flexible.root.get_mut("m_pFeModel").unwrap();
    let names=fe.get("m_CtrlName").unwrap().array().unwrap();
    let parents=fe.get("m_SkelParents").unwrap().array().unwrap();
    let rods=fe.get("m_Rods").unwrap().array().unwrap();
    let chosen=rods.iter().find(|rod|{
        let p=rod.get("nNode").unwrap().array().unwrap();
        let (a,b)=(p[0].integer().unwrap() as usize,p[1].integer().unwrap() as usize);
        let parent=parents[a].integer().unwrap();
        parent>=0 && parent==parents[b].integer().unwrap()
            && plan.bones.iter().any(|n|names[parent as usize].string().ok()==Some(n.as_str()))
    }).unwrap();
    let endpoints=chosen.get("nNode").unwrap().array().unwrap();
    let pair=[endpoints[0].integer().unwrap(),endpoints[1].integer().unwrap()];
    let old=match chosen.get("flMaxDist").unwrap().kind {Kind::F32(v)=>v,Kind::F64(v)=>v as f32,_=>panic!("native float expected")};
    if let Kind::Array(rods)=&mut fe.get_mut("m_Rods").unwrap().kind {for rod in rods {
        let p=rod.get("nNode").unwrap().array().unwrap();let k=[p[0].integer().unwrap(),p[1].integer().unwrap()];
        if k==pair || k==[pair[1],pair[0]] {
            *rod.get_mut("flMinDist").unwrap()=Value::new(Kind::F32(0.));
            *rod.get_mut("flMaxDist").unwrap()=Value::new(Kind::F32(old*1.1));
        }
    }}
    if let Kind::Array(packs)=&mut fe.get_mut("m_SimdRods").unwrap().kind {for pack in packs {
        let nodes=pack.get("nNode").unwrap().array().unwrap();
        let lanes=(0..4).filter(|&i|{
            let k=[nodes[0].array().unwrap()[i].integer().unwrap(),nodes[1].array().unwrap()[i].integer().unwrap()];
            k==pair || k==[pair[1],pair[0]]
        }).collect::<Vec<_>>();
        for (key,v) in [("f4MinDist",0.),("f4MaxDist",old*1.1)] {
            if let Kind::Array(values)=&mut pack.get_mut(key).unwrap().kind {for &i in &lanes {values[i]=Value::new(Kind::F32(v));}}
        }
    }}
    let flexible=container::rebuild(&native,&BTreeMap::from([("PHYS".into(),binary::encode(&flexible.root,flexible.format_guid).unwrap())])).unwrap();
    let (tightened,proof)=generated::apply_with_fabric(&original,&flexible,&source,&selected,&weighted,&chains,&plan).unwrap();
    assert!(!proof.sheet_stability.tightened_native_rods.is_empty());
    generated::validate_with_fabric(&original,&flexible,&tightened,&source,&selected,&weighted,&chains,&plan).unwrap();
    if let Ok(path)=std::env::var("DMB_FABRIC_AUDIT") {fs::write(path,serde_json::to_vec_pretty(&audit).unwrap()).unwrap();}
    println!("{} local diagonals; {} fold limits; native fields, meshes, binds and other physics preserved",audit.sheet_stability.rigid_support_rods,audit.sheet_stability.bend_limit_rods);
}
