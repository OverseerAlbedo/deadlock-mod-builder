use deadlock_cloth::generated::sheet_stability::{self,Plan};
use deadlock_resource::{cloth,Kind,Value};
use serde_json::json;
use std::collections::BTreeSet;

fn value(v:serde_json::Value)->Value {Value::new(match v {
    serde_json::Value::Null=>Kind::Null,serde_json::Value::Bool(v)=>Kind::Bool(v),
    serde_json::Value::String(v)=>Kind::String(v),serde_json::Value::Array(v)=>Kind::Array(v.into_iter().map(value).collect()),
    serde_json::Value::Object(v)=>Kind::Object(v.into_iter().map(|(k,v)|(k,value(v))).collect()),
    serde_json::Value::Number(v)=>if let Some(i)=v.as_i64(){Kind::I64(i)}else{Kind::F64(v.as_f64().unwrap())},
})}
fn fixture()->(Value,Plan) {
    let mut positions=vec![];let mut rods=vec![];
    for row in 0..3 {
        for (side,down) in [(-0.2,0.),(0.2,0.),(-0.2,-0.5),(0.2,-0.5)] {
            positions.push(vec![side,0.,-(row as f64)+down,1.,0.,0.,0.,1.]);
        }
    }
    let mut edges=vec![];
    for row in 0..3 {
        let offset=row*4;
        edges.extend([[0,1],[0,2],[1,2],[1,3],[2,3]].map(|e|[e[0]+offset,e[1]+offset]));
        if row>0 {edges.extend([[offset-2,offset],[offset-1,offset+1],[offset-1,offset]]);}
    }
    for [a,b] in edges {
        let distance=(0..3).map(|i|(positions[a][i]-positions[b][i]).powi(2)).sum::<f64>().sqrt();
        rods.push(json!({"nNode":[a,b],"flMinDist":distance,"flMaxDist":distance,"flWeight0":0.5,"flRelaxationFactor":1.}));
    }
    // Each old SIMD pack is deliberately a padded single edge. Verify that
    // schedules are preserved and newly packed independent lanes are complete.
    let simd=rods.iter().map(|r|json!({"nNode":[vec![r["nNode"][0].clone();4],vec![r["nNode"][1].clone();4]],
        "f4MinDist":vec![r["flMinDist"].clone();4],"f4MaxDist":vec![r["flMaxDist"].clone();4],
        "f4Weight0":vec![0.5;4],"f4RelaxationFactor":vec![1.;4]})).collect::<Vec<_>>();
    let mut names=(0..12).map(|i|format!("point{i}")).collect::<Vec<_>>();names.extend(["a".into(),"b".into(),"c".into()]);
    positions.extend((0..3).map(|i|vec![0.,0.,-(i as f64),1.,0.,0.,0.,1.]));
    let fe=value(json!({"m_CtrlName":names,"m_InitPose":positions,"m_SkelParents":[12,12,12,12,13,13,13,13,14,14,14,14,-1,12,13],
        "m_NodeInvMasses":vec![1.;15],"m_nNodeCount":15,"m_nStaticNodes":0,"m_nFirstPositionDrivenNode":12,
        "m_ReverseOffsets":[{"nBoneCtrl":12,"nTargetNode":0},{"nBoneCtrl":13,"nTargetNode":4},{"nBoneCtrl":14,"nTargetNode":8}],
        "m_Rods":rods,"m_SimdRods":simd,"gravity":1.,"drag":0.15,"goal":0.08}));
    (fe,Plan{bones:vec!["a".into(),"b".into(),"c".into()],hinges:vec![["a".into(),"b".into(),"c".into()]],seams:vec![],frames:vec![],..Default::default()})
}

#[test]
fn stops_support_diagonal_collapse_and_large_folds_without_changing_motion_maps() {
    let (fe,plan)=fixture();let (after,proof)=sheet_stability::apply(&fe,&plan).unwrap();
    assert_eq!(proof.rigid_support_rods,3);assert_eq!(proof.bend_limit_rods,1);
    for (key,old) in fe.object().unwrap(){if key!="m_Rods" && key!="m_SimdRods"{assert!(after.get(key).unwrap().canonical_eq(old),"{key}");}}
    for key in ["m_Rods","m_SimdRods"] {
        let old=fe.get(key).unwrap().array().unwrap();let new=after.get(key).unwrap().array().unwrap();
        assert!(old.iter().zip(new).all(|(a,b)|a.canonical_eq(b)));
    }
    let rods=after.get("m_Rods").unwrap().array().unwrap();
    let diagonal=rods.iter().find(|r|r.get("nNode").unwrap().array().unwrap().iter().map(|v|v.integer().unwrap()).collect::<Vec<_>>()==[0,3]).unwrap();
    assert_eq!(diagonal.get("flMinDist"),diagonal.get("flMaxDist"));
    let hinge=rods.last().unwrap();
    let number=|key|match hinge.get(key).unwrap().kind {Kind::F32(n)=>n as f64,_=>panic!()};
    let folded_distance=|angle:f64|(2.+2.*angle.to_radians().cos()).sqrt();
    assert!(folded_distance(30.)>number("flMinDist"));
    assert!(folded_distance(150.)<number("flMinDist"));
    assert_eq!(number("flMaxDist"),2.);
    let physics=value(json!({"m_pFeModel":null}));let mut physics=physics;
    *physics.get_mut("m_pFeModel").unwrap()=after.clone();
    assert_eq!(cloth::audit_rods(&physics).status,cloth::Status::Passed,"{:?}",cloth::audit_rods(&physics));
    for pack in &after.get("m_SimdRods").unwrap().array().unwrap()[fe.get("m_SimdRods").unwrap().array().unwrap().len()..] {
        let nodes=pack.get("nNode").unwrap().array().unwrap();
        let distinct=(0..4).map(|i|[nodes[0].array().unwrap()[i].integer().unwrap(),nodes[1].array().unwrap()[i].integer().unwrap()]).collect::<BTreeSet<_>>();
        assert_eq!(distinct.iter().flatten().collect::<BTreeSet<_>>().len(),distinct.len()*2,"SIMD lanes share a writable node");
    }
    assert!(sheet_stability::apply(&after,&plan).unwrap().0.canonical_eq(&after));
}

#[test]
fn unrelated_missing_and_disconnected_parts_are_rejected_and_empty_plan_is_exact() {
    let (fe,mut plan)=fixture();assert!(sheet_stability::apply(&fe,&Plan::default()).unwrap().0.canonical_eq(&fe));
    plan.hinges[0][1]="outside".into();assert!(sheet_stability::apply(&fe,&plan).is_err());
    let (mut fe,plan)=fixture();
    if let Kind::Array(rows)=&mut fe.get_mut("m_Rods").unwrap().kind {
        rows.retain(|r|{let n=r.get("nNode").unwrap().array().unwrap();n[0].integer().unwrap()/4==n[1].integer().unwrap()/4});
    }
    assert!(sheet_stability::apply(&fe,&plan).unwrap_err().to_string().contains("disconnected seam"));
}

#[test]
fn native_matrix_fits_use_only_their_own_verified_supports() {
    let (mut fe,plan)=fixture();
    *fe.get_mut("m_ReverseOffsets").unwrap()=value(json!([]));
    if let Kind::Object(fields)=&mut fe.kind {
        fields.push(("m_FitMatrices".into(),value(json!([
            {"nNode":12,"nBeginDynamic":0,"nEnd":4},
            {"nNode":13,"nBeginDynamic":4,"nEnd":8},
            {"nNode":14,"nBeginDynamic":8,"nEnd":12}]))));
        fields.push(("m_FitWeights".into(),value(json!((0..12).map(|i|json!({"nNode":i,"flWeight":0.25})).collect::<Vec<_>>()))));
    }
    let (_,proof)=sheet_stability::apply(&fe,&plan).unwrap();assert_eq!(proof.bend_limit_rods,1);
    if let Kind::Array(rows)=&mut fe.get_mut("m_FitWeights").unwrap().kind {
        *rows[0].get_mut("nNode").unwrap()=value(json!(5));
    }
    assert!(sheet_stability::apply(&fe,&plan).unwrap_err().to_string().contains("another bone"));
}

#[test]
fn overlapping_native_limits_are_tightened_in_scalar_and_all_simd_copies() {
    let (mut fe,plan)=fixture();
    let mut rod=fe.get("m_Rods").unwrap().array().unwrap()[0].clone();
    *rod.get_mut("nNode").unwrap()=value(json!([0,8]));
    *rod.get_mut("flMinDist").unwrap()=value(json!(0.));
    *rod.get_mut("flMaxDist").unwrap()=value(json!(2.1));
    if let Kind::Array(rows)=&mut fe.get_mut("m_Rods").unwrap().kind {rows.push(rod.clone());*rod.get_mut("nNode").unwrap()=value(json!([8,0]));rows.push(rod);}
    if let Kind::Array(packs)=&mut fe.get_mut("m_SimdRods").unwrap().kind {
        packs.push(value(json!({"nNode":[[0,8,0,8],[8,0,8,0]],"f4MinDist":[0.,0.,0.,0.],
            "f4MaxDist":[2.1,2.1,2.1,2.1],"f4Weight0":vec![0.5;4],"f4RelaxationFactor":vec![1.;4]})));
    }
    let (after,proof)=sheet_stability::apply(&fe,&plan).unwrap();
    assert_eq!(proof.tightened_native_rods,vec![[0,8]]);
    assert!(!proof.original_records_preserved);
    assert_eq!(proof.bend_limit_rods,0);
    assert_eq!(proof.rigid_support_rods,3);
    let bounds=after.get("m_Rods").unwrap().array().unwrap().iter().filter(|r|r.get("nNode").unwrap().canonical_eq(&value(json!([0,8])))).collect::<Vec<_>>();
    assert_eq!(bounds.len(),1);
    for r in bounds {assert!(matches!(r.get("flMaxDist").unwrap().kind,Kind::F32(2.)));}
    let mut physics=value(json!({"m_pFeModel":null}));*physics.get_mut("m_pFeModel").unwrap()=after.clone();
    assert_eq!(cloth::audit_rods(&physics).status,cloth::Status::Passed,"{:?}",cloth::audit_rods(&physics));
    assert!(sheet_stability::apply(&after,&plan).unwrap().0.canonical_eq(&after));
    // An actual empty intersection is still rejected, with the affected nodes.
    if let Kind::Array(rows)=&mut fe.get_mut("m_Rods").unwrap().kind {
        for r in rows.iter_mut().rev().take(2) {*r.get_mut("flMaxDist").unwrap()=value(json!(0.5));}
    }
    assert!(sheet_stability::apply(&fe,&plan).unwrap_err().to_string().contains("disjoint native limits for point0 / point8"));
}

#[test]
fn evidenced_panel_seams_bound_separation_without_locking_bending() {
    let (fe,mut plan)=fixture();
    plan.hinges.clear();
    plan.seams=vec![["a".into(),"b".into()]];
    let (after,proof)=sheet_stability::apply(&fe,&plan).unwrap();
    assert!(proof.seam_limit_rods>0);
    let rod=after.get("m_Rods").unwrap().array().unwrap().iter().find(|r|
        r.get("nNode").unwrap().canonical_eq(&value(json!([0,4])))).unwrap();
    let number=|key|match rod.get(key).unwrap().kind {Kind::F32(v)=>v as f64,_=>panic!()};
    assert!(number("flMinDist")<0.1,"Bending/compression must remain possible");
    assert!((number("flMaxDist")-1.02).abs()<1e-6);
    assert!(1.15>number("flMaxDist"),"The reported separating panels must violate a constraint");
    let mut physics=value(json!({"m_pFeModel":null}));*physics.get_mut("m_pFeModel").unwrap()=after.clone();
    assert_eq!(cloth::audit_rods(&physics).status,cloth::Status::Passed);
    for key in ["gravity","drag","goal","m_InitPose","m_SkelParents","m_ReverseOffsets"] {
        assert!(fe.get(key).unwrap().canonical_eq(after.get(key).unwrap()),"{key}");
    }
    assert!(sheet_stability::apply(&after,&plan).unwrap().0.canonical_eq(&after));
    plan.seams[0][1]="unrelated".into();assert!(sheet_stability::apply(&fe,&plan).is_err());
}

/// A streamer hanging from a body control: a static row of four points on the
/// body (nodes 0-3), then three joints a, b, c of four points each, one inch
/// apart, straight down.
fn streamer() -> Value {
    let mut positions=vec![];
    for row in 0..4 {
        for (side,down) in [(-0.2,0.),(0.2,0.),(-0.2,-0.5),(0.2,-0.5)] {
            positions.push(vec![side,0.,1.-(row as f64)+down,1.,0.,0.,0.,1.]);
        }
    }
    let mut edges=vec![];
    for row in 0..4 {
        let offset=row*4;
        edges.extend([[0,1],[0,2],[1,2],[1,3],[2,3]].map(|e|[e[0]+offset,e[1]+offset]));
        if row>0 {edges.extend([[offset-2,offset],[offset-1,offset+1],[offset-1,offset]]);}
    }
    let rods=edges.iter().map(|&[a,b]|{
        let distance=(0..3).map(|i|(positions[a][i]-positions[b][i]).powi(2)).sum::<f64>().sqrt();
        json!({"nNode":[a,b],"flMinDist":distance,"flMaxDist":distance,"flWeight0":0.5,"flRelaxationFactor":1.})
    }).collect::<Vec<_>>();
    let simd=rods.iter().map(|r|json!({"nNode":[vec![r["nNode"][0].clone();4],vec![r["nNode"][1].clone();4]],
        "f4MinDist":vec![r["flMinDist"].clone();4],"f4MaxDist":vec![r["flMaxDist"].clone();4],
        "f4Weight0":vec![0.5;4],"f4RelaxationFactor":vec![1.;4]})).collect::<Vec<_>>();
    let mut names=(0..16).map(|i|format!("point{i}")).collect::<Vec<_>>();names.extend(["body".into(),"a".into(),"b".into(),"c".into()]);
    positions.push(vec![0.,0.,1.,1.,0.,0.,0.,1.]);
    positions.extend((0..3).map(|i|vec![0.,0.,-(i as f64),1.,0.,0.,0.,1.]));
    let mut parents=vec![16;4];parents.extend([17;4]);parents.extend([18;4]);parents.extend([19;4]);parents.extend([-1,16,17,18]);
    let mut masses=vec![0.;4];masses.extend(vec![1.;12]);masses.extend(vec![0.;4]);
    value(json!({"m_CtrlName":names,"m_InitPose":positions,"m_SkelParents":parents,
        "m_NodeInvMasses":masses,"m_nNodeCount":20,"m_nStaticNodes":4,"m_nFirstPositionDrivenNode":16,
        "m_ReverseOffsets":[{"nBoneCtrl":17,"nTargetNode":4},{"nBoneCtrl":18,"nTargetNode":8},{"nBoneCtrl":19,"nTargetNode":12}],
        "m_Rods":rods,"m_SimdRods":simd}))
}

#[test]
fn streamer_chains_get_bend_limits_and_tethers_to_their_anchor() {
    let fe=streamer();
    let plan=Plan{strands:vec![vec!["a".into(),"b".into(),"c".into()]],..Default::default()};
    let (after,proof)=sheet_stability::apply(&fe,&plan).unwrap();
    assert_eq!(proof.strand_bend_rods,1);
    assert_eq!(proof.strand_tethers,2);
    assert_eq!(proof.rigid_support_rods+proof.bend_limit_rods+proof.seam_limit_rods,0);
    // One bend rod a-c, and tethers to b and c from the static point closest
    // to a (node 2, half an inch above it).
    assert_eq!(proof.added_rods,vec![[4,12],[2,8],[2,12]]);
    for key in ["m_Rods","m_SimdRods"] {
        let old=fe.get(key).unwrap().array().unwrap();let new=after.get(key).unwrap().array().unwrap();
        assert!(new.len()>old.len() && old.iter().zip(new).all(|(a,b)|a.canonical_eq(b)),"{key}");
    }
    let rods=after.get("m_Rods").unwrap().array().unwrap();
    let rod=|a:i64,b:i64|rods.iter().find(|r|r.get("nNode").unwrap().canonical_eq(&value(json!([a,b])))).unwrap().clone();
    let number=|rod:&Value,key|match rod.get(key).unwrap().kind {Kind::F32(n)=>n as f64,_=>panic!()};
    // The links are straight at rest: the joint may bend 35 degrees, never fold.
    let chord=|angle:f64|(2.+2.*angle.to_radians().cos()).sqrt();
    let bend=rod(4,12);
    assert!((number(&bend,"flMinDist")-chord(sheet_stability::STRAND_BEND_DEGREES)).abs()<1e-5);
    assert!(chord(60.)<number(&bend,"flMinDist") && number(&bend,"flMinDist")<2.);
    assert_eq!(number(&bend,"flMaxDist"),2.);
    // Tethers allow any bend but no length beyond the streamer's own: half an
    // inch to a, then one inch per link.
    for (node,reach) in [(8,1.5),(12,2.5)] {
        let tether=rod(2,node);
        assert!((number(&tether,"flMaxDist")-reach).abs()<1e-6,"{node}");
        assert!(number(&tether,"flMinDist")<reach*0.02);
        // The anchor never moves: the joint takes the whole correction.
        assert_eq!(number(&tether,"flWeight0"),0.);
    }
    let mut physics=value(json!({"m_pFeModel":null}));*physics.get_mut("m_pFeModel").unwrap()=after.clone();
    assert_eq!(cloth::audit_rods(&physics).status,cloth::Status::Passed,"{:?}",cloth::audit_rods(&physics));
    // No SIMD pack uses a node in two lanes, as the compiler's own packs.
    for pack in &after.get("m_SimdRods").unwrap().array().unwrap()[fe.get("m_SimdRods").unwrap().array().unwrap().len()..] {
        let nodes=pack.get("nNode").unwrap().array().unwrap();
        let distinct=(0..4).map(|i|[nodes[0].array().unwrap()[i].integer().unwrap(),nodes[1].array().unwrap()[i].integer().unwrap()]).collect::<BTreeSet<_>>();
        assert_eq!(distinct.iter().flatten().collect::<BTreeSet<_>>().len(),distinct.len()*2,"SIMD lanes share a node");
    }
    // Recomputing on the result changes nothing (install recomputes from the snapshot).
    assert!(sheet_stability::apply(&after,&plan).unwrap().0.canonical_eq(&after));
    for bad in [vec!["a","b"],vec!["a","b","a"],vec!["a","b","outside"],vec!["point4","b","c"]] {
        let plan=Plan{strands:vec![bad.iter().map(|n|n.to_string()).collect()],..Default::default()};
        assert!(sheet_stability::apply(&fe,&plan).is_err(),"{bad:?}");
    }
    // A streamer that hangs from nothing static is refused.
    let (loose,_)=fixture();
    assert!(sheet_stability::apply(&loose,&plan).unwrap_err().to_string().contains("static anchor"));
}
