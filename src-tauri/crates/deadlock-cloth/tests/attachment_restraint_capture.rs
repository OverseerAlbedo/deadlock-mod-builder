//! Explicit, hash-bound movement trial, never called by the application.
//! The user accepted the broad skirt surface fits and requested a separate
//! hood / upper-back test before any universal attachment policy is enabled.
use deadlock_resource::{binary, cloth, container, Kind, Value};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, BTreeSet}, fs, path::PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bone {
    bone: String,
    region: String,
    radius_inches: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    policy: String,
    input_model_sha256: String,
    input_package_sha256: String,
    attachment: String,
    point_damping: f64,
    animation_force_attraction: f64,
    animation_vertex_attraction: f64,
    bones: Vec<Bone>,
    automatic_rollout: bool,
}
fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn arr(values: impl IntoIterator<Item=Value>) -> Value { Value::new(Kind::Array(values.into_iter().collect())) }
fn f32v(n: f64) -> Value { assert!(n.is_finite()); Value::new(Kind::F32(n as f32)) }
fn int(n: usize) -> Value { Value::new(Kind::I64(n as i64)) }
fn index(v: &Value) -> usize { usize::try_from(v.integer().unwrap()).unwrap() }
fn rows<'a>(v: &'a Value, k: &str) -> &'a [Value] { v.get(k).unwrap().array().unwrap() }
fn number(v: &Value) -> f64 { match v.kind {
    Kind::F32(n) => n as f64, Kind::F64(n) => n, Kind::I64(n) => n as f64, _ => panic!("not a number"),
}}
fn edit(v: &mut Value, k: &str, n: f64) { *v.get_mut(k).unwrap() = f32v(n); }
fn nodes(row: &Value) -> [usize;2] {
    let a=rows(row,"nNode"); assert_eq!(a.len(),2); [index(&a[0]),index(&a[1])]
}
fn simd_nodes(pack: &Value, lane: usize) -> [usize;2] {
    let a=rows(pack,"nNode"); assert_eq!(a.len(),2);
    [index(&a[0].array().unwrap()[lane]),index(&a[1].array().unwrap()[lane])]
}
fn stray_coverage(fe: &Value) {
    let scalar=rows(fe,"m_AnimStrayRadii").iter().map(|r| {
        (nodes(r),[number(r.get("flMaxDist").unwrap()),number(r.get("flRelaxationFactor").unwrap())])
    }).collect::<BTreeMap<_,_>>();
    assert_eq!(scalar.len(),rows(fe,"m_AnimStrayRadii").len(),"duplicate scalar limit");
    let mut seen=BTreeSet::new();
    for pack in rows(fe,"m_SimdAnimStrayRadii") {
        for key in ["flMaxDist","flRelaxationFactor"] { assert_eq!(rows(pack,key).len(),4); }
        for lane in 0..4 {
            let pair=simd_nodes(pack,lane); let expected=scalar.get(&pair).expect("SIMD limit has no scalar record");
            for (key,want) in ["flMaxDist","flRelaxationFactor"].into_iter().zip(expected) {
                assert_eq!(number(&rows(pack,key)[lane]),*want,"scalar / SIMD travel mismatch");
            }
            seen.insert(pair);
        }
    }
    assert_eq!(seen,scalar.keys().copied().collect());
}

fn apply(fe: &Value, plan: &Plan) -> (Value, BTreeMap<usize,f64>, serde_json::Value) {
    assert!(!plan.automatic_rollout);
    assert_eq!(plan.policy,"graves-hood-upper-trial-r1");
    assert!(!plan.bones.is_empty());
    stray_coverage(fe);
    let names=rows(fe,"m_CtrlName").iter().map(|n|n.string().unwrap()).collect::<Vec<_>>();
    let anchor=names.iter().position(|n|*n==plan.attachment).unwrap();
    let static_=index(fe.get("m_nStaticNodes").unwrap());
    let driven=index(fe.get("m_nFirstPositionDrivenNode").unwrap());
    assert!(anchor<static_);
    let parents=rows(fe,"m_SkelParents");
    let mut selected=BTreeSet::new(); let mut targets=BTreeMap::new(); let mut proof=Vec::new();
    for bone in &plan.bones {
        assert!(bone.radius_inches>0. && bone.radius_inches<=0.1);
        let ctrl=names.iter().position(|n|*n==bone.bone).unwrap();
        assert!(ctrl>=driven && selected.insert(ctrl));
        let mut ancestor=ctrl; let mut visited=BTreeSet::new();
        while ancestor!=anchor {
            assert!(visited.insert(ancestor),"cyclic attachment");
            ancestor=index(&parents[ancestor]);
        }
        let supports=rows(fe,"m_CtrlOffsets").iter().filter(|r|index(r.get("nCtrlParent").unwrap())==ctrl)
            .map(|r|index(r.get("nCtrlChild").unwrap())).collect::<Vec<_>>();
        assert_eq!(supports.len(),3,"This trial only supports the captured three-point patches");
        for &point in &supports {
            assert!(point>=static_ && point<driven);
            assert_eq!(index(&parents[point]),ctrl);
            assert!(number(&rows(fe,"m_NodeInvMasses")[point])>0.);
            assert!(targets.insert(point,bone.radius_inches).is_none(),"shared patch ownership");
        }
        proof.push(json!({"bone":bone.bone,"region":bone.region,"control":ctrl,
            "points":supports,"radius_inches":bone.radius_inches}));
    }
    let mut out=fe.clone();
    let mut integrators=rows(fe,"m_NodeIntegrator").to_vec();
    for &point in targets.keys() {
        edit(&mut integrators[point],"flPointDamping",plan.point_damping);
        edit(&mut integrators[point],"flAnimationForceAttraction",plan.animation_force_attraction);
        edit(&mut integrators[point],"flAnimationVertexAttraction",plan.animation_vertex_attraction);
    }
    *out.get_mut("m_NodeIntegrator").unwrap()=arr(integrators);
    let mut scalar=rows(fe,"m_AnimStrayRadii").to_vec();
    let mut existing=BTreeSet::new();
    for row in &mut scalar {
        let pair=nodes(row);
        if let Some(&radius)=targets.get(&pair[0]) {
            assert_eq!(pair[0],pair[1],"Unexpected non-animation travel reference");
            assert!(radius<=number(row.get("flMaxDist").unwrap())+1e-8,"Never loosen an existing limit");
            edit(row,"flMaxDist",radius); edit(row,"flRelaxationFactor",1.); existing.insert(pair[0]);
        }
    }
    let mut packs=rows(fe,"m_SimdAnimStrayRadii").to_vec();
    for pack in &mut packs {
        let pairs=(0..4).map(|lane|simd_nodes(pack,lane)).collect::<Vec<_>>();
        for key in ["flMaxDist","flRelaxationFactor"] {
            let mut values=rows(pack,key).to_vec();
            for lane in 0..4 { if let Some(&radius)=targets.get(&pairs[lane][0]) {
                assert_eq!(pairs[lane][0],pairs[lane][1]);
                assert!(existing.contains(&pairs[lane][0]));
                values[lane]=f32v(if key=="flMaxDist" {radius} else {1.});
            }}
            *pack.get_mut(key).unwrap()=arr(values);
        }
    }
    let mut additions=Vec::new();
    for (&point,&radius) in &targets { if !existing.contains(&point) {
        additions.push(Value::new(Kind::Object(vec![
            ("nNode".into(),arr([int(point),int(point)])),
            ("flMaxDist".into(),f32v(radius)),("flRelaxationFactor".into(),f32v(1.)),
        ])));
    }}
    // Self-referenced animation bounds on distinct particles have no shared
    // dynamic writes. Preserve native packs; pad only the final new pack.
    for chunk in additions.chunks(4) {
        let lanes=(0..4).map(|i|&chunk[i.min(chunk.len()-1)]).collect::<Vec<_>>();
        let mut fields=vec![("nNode".into(),arr((0..2).map(|axis|arr(lanes.iter().map(|r|rows(r,"nNode")[axis].clone())))))];
        for key in ["flMaxDist","flRelaxationFactor"] {
            fields.push((key.into(),arr(lanes.iter().map(|r|r.get(key).unwrap().clone()))));
        }
        packs.push(Value::new(Kind::Object(fields)));
    }
    scalar.extend(additions);
    *out.get_mut("m_AnimStrayRadii").unwrap()=arr(scalar);
    *out.get_mut("m_SimdAnimStrayRadii").unwrap()=arr(packs);
    stray_coverage(&out);
    (out,targets,json!(proof))
}

#[test]
#[ignore="requires DMB_ATTACHMENT_MODEL, PLAN, VPK and a fresh OUTPUT directory"]
fn captured_hood_and_upper_panel_trial_preserves_accepted_skirt() {
    let native=fs::read(std::env::var("DMB_ATTACHMENT_MODEL").unwrap()).unwrap();
    let plan:Plan=serde_json::from_slice(&fs::read(std::env::var("DMB_ATTACHMENT_PLAN").unwrap()).unwrap()).unwrap();
    assert_eq!(hash(&native),plan.input_model_sha256);
    let mut physics=container::read_blocks(&native,"PHYS").unwrap().remove(0);
    let before=physics.root.get("m_pFeModel").unwrap();
    let original_limit_count=rows(before,"m_AnimStrayRadii").len();
    let automatic=std::env::var("DMB_ATTACHMENT_AUTOMATIC_SETUP").ok().map(|path| {
        let setup:serde_json::Value=serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        serde_json::from_value::<deadlock_cloth::generated::attachment_limits::Plan>(setup["attachments"]["plan"].clone()).unwrap()
    });
    if let Some(automatic)=&automatic {
        assert_eq!(automatic.bones.iter().map(|b|&b.bone).collect::<BTreeSet<_>>(),plan.bones.iter().map(|b|&b.bone).collect());
    }
    let transform=|fe:&Value| {
        if let Some(automatic)=&automatic {
            let (out,proof)=deadlock_cloth::generated::attachment_limits::apply(fe,automatic).unwrap();
            let names=rows(fe,"m_CtrlName");
            let targets=proof.bones.iter().flat_map(|b|b.points.iter().map(move |&p|(p,b.radius_inches))).collect();
            let proof=proof.bones.iter().map(|b|json!({"bone":b.bone,"points":b.points,"radius_inches":b.radius_inches,
                "region":automatic.bones.iter().find(|r|r.bone==b.bone).unwrap().region,
                "control":names.iter().position(|n|n.string().unwrap()==b.bone).unwrap()})).collect::<Vec<_>>();
            (out,targets,json!(proof))
        } else {apply(fe,&plan)}
    };
    let (after,targets,proof)=transform(before);
    assert!(transform(&after).0.canonical_eq(&after),"repeat application must be idempotent");
    let changed_fields=["m_NodeIntegrator","m_AnimStrayRadii","m_SimdAnimStrayRadii"];
    let mut preserved_fields=Vec::new();
    for (key,old) in before.object().unwrap() { if !changed_fields.contains(&key.as_str()) {
        assert!(after.get(key).unwrap().canonical_eq(old),"Unrelated field changed: {key}");
        preserved_fields.push(key.clone());
    }}
    let mut edited_integrators=0;
    for (point,(old,new)) in rows(before,"m_NodeIntegrator").iter().zip(rows(&after,"m_NodeIntegrator")).enumerate() {
        if !targets.contains_key(&point) { assert!(old.canonical_eq(new)); }
        else {
            for (key,v) in old.object().unwrap() {
                if !["flPointDamping","flAnimationForceAttraction","flAnimationVertexAttraction"].contains(&key.as_str()) {
                    assert!(new.get(key).unwrap().canonical_eq(v));
                }
            }
            edited_integrators+=usize::from(!old.canonical_eq(new));
        }
    }
    for (old,new) in rows(before,"m_AnimStrayRadii").iter().zip(rows(&after,"m_AnimStrayRadii")) {
        assert_eq!(nodes(old),nodes(new));
        if !targets.contains_key(&nodes(old)[0]) { assert!(old.canonical_eq(new)); }
    }
    *physics.root.get_mut("m_pFeModel").unwrap()=after;
    cloth::self_contained(&physics.root).unwrap(); cloth::structure(&physics.root).unwrap();
    assert_eq!(cloth::audit_rods(&physics.root).status,cloth::Status::Passed);
    let output=container::rebuild(&native,&BTreeMap::from([("PHYS".into(),binary::encode(&physics.root,physics.format_guid).unwrap())])).unwrap();
    let before=container::layout(&native).unwrap(); let after=container::layout(&output).unwrap();
    assert_eq!(before.blocks.len(),after.blocks.len()); let mut preserved_blocks=Vec::new();
    for (a,b) in before.blocks.iter().zip(&after.blocks) {
        assert_eq!(a.name,b.name);
        if a.name!="PHYS" { assert_eq!(container::payload(&native,a).unwrap(),container::payload(&output,b).unwrap());preserved_blocks.push(a.name.clone()); }
    }
    let reread=container::read_blocks(&output,"PHYS").unwrap().remove(0);
    assert!(reread.root.canonical_eq(&physics.root));
    let destination=PathBuf::from(std::env::var("DMB_ATTACHMENT_OUTPUT").unwrap());
    assert!(!destination.exists(),"Use a new output directory; never overwrite an input");
    let input=PathBuf::from(std::env::var("DMB_ATTACHMENT_VPK").unwrap());
    assert_eq!(hash(&fs::read(&input).unwrap()),plan.input_package_sha256);
    let package=deadlock_vpk::Vpk::open(&input).unwrap();
    let stage=destination.join("assets");fs::create_dir_all(&stage).unwrap();
    let mut expected=BTreeMap::new();let mut replaced=Vec::new();
    for entry in package.entries.values() {
        let bytes=package.read(&entry.path).unwrap();
        let bytes=if bytes==native {replaced.push(entry.path.clone());&output}else{&bytes};
        let path=stage.join(&entry.path);fs::create_dir_all(path.parent().unwrap()).unwrap();fs::write(path,bytes).unwrap();
        expected.insert(entry.path.clone(),hash(bytes));
    }
    assert_eq!(replaced.len(),1);
    let vpk=destination.join("pak27_dir.vpk");
    let receipt=deadlock_vpk::write_vpk(&stage,&expected.keys().cloned().collect::<Vec<_>>(),&vpk,Some(&expected),package.version,&mut||Ok(())).unwrap();
    let checked=deadlock_vpk::Vpk::open(&vpk).unwrap();checked.verify_files(Some(&expected),&mut||Ok(())).unwrap();
    if package.version==2 { checked.verify_v2_hashes(&mut||Ok(())).unwrap(); }
    let report=json!({"policy":plan.policy,"input_model_sha256":hash(&native),"output_model_sha256":hash(&output),
        "input_package_sha256":plan.input_package_sha256,"output_package_sha256":hash(&fs::read(&vpk).unwrap()),
        "bones":proof,"selected_points":targets.len(),"edited_integrators":edited_integrators,
        "preserved_fe_fields":preserved_fields,"byte_identical_blocks":preserved_blocks,
        "original_limit_count":original_limit_count,
        "limit_count":rows(physics.root.get("m_pFeModel").unwrap(),"m_AnimStrayRadii").len(),
        "receipt":receipt,"replaced":replaced,"other_assets_byte_identical":expected.len()-1,
        "scalar_simd_coverage":true,"native_rod_audit":"passed","rest_binds_unchanged":true,
        "all_orientation_fits_unchanged":true,"automatic_rollout":automatic.is_some(),"fresh_compile":false,"in_game_tested":false});
    fs::write(destination.join("model.vmdl_c"),&output).unwrap();
    fs::write(destination.join("audit.json"),serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("{} restrained patches, {} support points; accepted skirt fits / constraints and other {} assets preserved",plan.bones.len(),targets.len(),expected.len()-1);
}
