use deadlock_cloth::generated::{fitting,sheet_frames::Frame,sheet_stability::{self,Plan}};
use deadlock_resource::{cloth,Kind,Value};
use serde_json::json;
use std::collections::BTreeSet;

fn value(v:serde_json::Value)->Value {Value::new(match v {
    serde_json::Value::Null=>Kind::Null,serde_json::Value::Bool(v)=>Kind::Bool(v),
    serde_json::Value::String(v)=>Kind::String(v),serde_json::Value::Array(v)=>Kind::Array(v.into_iter().map(value).collect()),
    serde_json::Value::Object(v)=>Kind::Object(v.into_iter().map(|(k,v)|(k,value(v))).collect()),
    serde_json::Value::Number(v)=>if let Some(i)=v.as_i64(){Kind::I64(i)}else{Kind::F64(v.as_f64().unwrap())},
})}
fn number(v:&Value)->f64 {match v.kind {Kind::F32(n)=>n as f64,Kind::F64(n)=>n,Kind::I64(n)=>n as f64,_=>panic!()}}
fn integer(v:&Value)->usize {v.integer().unwrap() as usize}
fn fixture()->(Value,Plan) {
    let names=["a0","a1","a2","b0","b1","b2"].map(str::to_owned);
    let mut positions=Vec::new();let mut parents=Vec::new();let mut edges=Vec::new();
    for col in 0..2 {for row in 0..3 {
        let bone=col*3+row;let at=bone*4;
        for (side,down) in [(-0.2,0.),(0.2,0.),(-0.2,-0.5),(0.2,-0.5)] {
            positions.push(vec![2.*col as f64+side,0.,-(row as f64)+down,1.,0.,0.,0.,1.]);
            parents.push(24+bone as i32);
        }
        edges.extend([[0,1],[0,2],[1,2],[1,3],[2,3]].map(|e|[e[0]+at,e[1]+at]));
        if row>0 {edges.extend([[at-2,at],[at-1,at+1],[at-1,at]]);}
        if col>0 {edges.extend([[at-11,at],[at-9,at+2],[at-9,at]]);}
    }}
    let mut rods=Vec::new();
    for [a,b] in edges {
        let distance=(0..3).map(|i|(positions[a][i]-positions[b][i]).powi(2)).sum::<f64>().sqrt();
        rods.push(json!({"nNode":[a,b],"flMinDist":distance,"flMaxDist":distance,"flWeight0":0.5,"flRelaxationFactor":1.}));
    }
    let simd=rods.iter().map(|r|json!({"nNode":[vec![r["nNode"][0].clone();4],vec![r["nNode"][1].clone();4]],
        "f4MinDist":vec![r["flMinDist"].clone();4],"f4MaxDist":vec![r["flMaxDist"].clone();4],
        "f4Weight0":vec![0.5;4],"f4RelaxationFactor":vec![1.;4]})).collect::<Vec<_>>();
    for col in 0..2 {for row in 0..3 {
        positions.push(vec![2.*col as f64,0.,-(row as f64),1.,0.,0.,0.,1.]);
        parents.push(if row==0 {-1}else{24+col*3+row-1});
    }}
    let bases=(0..6).map(|i|json!({"nNode":24+i,"nDummy":[0,0,0],"nNodeX0":i*4,"nNodeX1":i*4+1,
        "nNodeY0":i*4,"nNodeY1":i*4+2,"qAdjust":[std::f64::consts::FRAC_1_SQRT_2,0.,0.,std::f64::consts::FRAC_1_SQRT_2]})).collect::<Vec<_>>();
    let reverse=(0..6).map(|i|json!({"nBoneCtrl":24+i,"nTargetNode":i*4,"vOffset":[-0.2,0.,0.]})).collect::<Vec<_>>();
    let all_names=(0..24).map(|i|format!("point{i}")).chain(names.iter().cloned()).collect::<Vec<_>>();
    let fe=value(json!({"m_CtrlName":all_names,"m_InitPose":positions,"m_SkelParents":parents,
        "m_NodeInvMasses":vec![1.;30],"m_nNodeCount":30,"m_nStaticNodes":0,"m_nFirstPositionDrivenNode":24,
        "m_NodeBases":bases,"m_SimdNodeBases":[],"m_FitMatrices":[],"m_FitWeights":[],
        "m_ReverseOffsets":reverse,"m_Rods":rods,"m_SimdRods":simd,"gravity":1.,"drag":0.15,"goal":0.08}));
    let frames=(0..6).map(|i|Frame{bone:names[i].clone(),
        along:if i%3==2 {[names[i-1].clone(),names[i].clone()]}else{[names[i].clone(),names[i+1].clone()]},
        across:[names[i%3].clone(),names[i%3+3].clone()]}).collect();
    let seams=(0..3).map(|i|[names[i].clone(),names[i+3].clone()]).collect();
    (fe,Plan{bones:names.to_vec(),hinges:vec![],seams,frames,..Default::default()})
}
fn positions(fe:&Value)->Vec<[f64;3]> {fe.get("m_InitPose").unwrap().array().unwrap().iter()
    .map(|p|std::array::from_fn(|i|number(&p.array().unwrap()[i]))).collect()}
fn sub(a:[f64;3],b:[f64;3])->[f64;3]{std::array::from_fn(|i|a[i]-b[i])}
fn dot(a:[f64;3],b:[f64;3])->f64{(0..3).map(|i|a[i]*b[i]).sum()}
fn unit(a:[f64;3])->[f64;3]{let n=dot(a,a).sqrt();a.map(|v|v/n)}
fn basis(row:&Value,p:&[[f64;3]])->[[f64;3];3] {
    let point=|k|p[integer(row.get(k).unwrap())];
    let y=unit(sub(point("nNodeY1"),point("nNodeY0")));
    let x=sub(point("nNodeX1"),point("nNodeX0"));let d=dot(x,y);let x=unit(std::array::from_fn(|i|x[i]-y[i]*d));
    let z=[x[1]*y[2]-x[2]*y[1],x[2]*y[0]-x[0]*y[2],x[0]*y[1]-x[1]*y[0]];
    std::array::from_fn(|i|[x[i],y[i],z[i]])
}
fn angle(a:[[f64;3];3],b:[[f64;3];3])->f64 {
    (((0..3).flat_map(|r|(0..3).map(move|c|a[r][c]*b[r][c])).sum::<f64>()-1.)*0.5).clamp(-1.,1.).acos().to_degrees()
}

#[test]
fn surface_fits_reject_local_patch_roll_but_follow_real_garment_motion() {
    let (fe,plan)=fixture();let (out,proof)=sheet_stability::apply(&fe,&plan).unwrap();
    assert_eq!(proof.surface_frames.len(),6);
    let selected=plan.bones.iter().cloned().collect::<BTreeSet<_>>();
    fitting::audit(&out,&plan.bones,&selected,&selected).unwrap();
    assert!(sheet_stability::apply(&out,&plan).unwrap().0.canonical_eq(&out));
    let old=&fe.get("m_NodeBases").unwrap().array().unwrap()[1];
    let new=&out.get("m_NodeBases").unwrap().array().unwrap()[1];
    let rest=positions(&fe);let mut moved=rest.clone();
    // Reproduce an isolated support rotation while all garment row centers
    // stay put. It must not rotate a broad section of the rendered skirt.
    let (s,c)=70_f64.to_radians().sin_cos();let pivot=rest[4];
    for i in [5,7] {let d=sub(rest[i],pivot);moved[i]=[pivot[0]+c*d[0]-s*d[1],pivot[1]+s*d[0]+c*d[1],rest[i][2]];}
    assert!((angle(basis(old,&rest),basis(old,&moved))-70.).abs()<1e-6);
    assert!(angle(basis(new,&rest),basis(new,&moved))<1e-5);
    // The new frame is not pinned to the body or rest pose. Real coherent
    // surface motion must rotate its fitted bone by the same amount.
    let (s,c)=35_f64.to_radians().sin_cos();
    let draped=rest.iter().map(|p|[p[0],c*p[1]-s*p[2],s*p[1]+c*p[2]]).collect::<Vec<_>>();
    assert!((angle(basis(new,&rest),basis(new,&draped))-35.).abs()<1e-6);
    let mut previous=plan.clone();previous.frames.clear();
    let (previous,_)=sheet_stability::apply(&fe,&previous).unwrap();
    for (key,old) in previous.object().unwrap(){if key!="m_NodeBases"{assert!(out.get(key).unwrap().canonical_eq(old),"{key}");}}
    let physics=value(json!({"m_pFeModel":null}));let mut physics=physics;
    *physics.get_mut("m_pFeModel").unwrap()=out;
    assert_eq!(cloth::audit_rods(&physics).status,cloth::Status::Passed);
}

#[test]
fn no_surface_frame_can_cross_a_slit_unrelated_branch_or_missing_native_connection() {
    let (fe,mut plan)=fixture();plan.seams.remove(0);
    assert!(sheet_stability::apply(&fe,&plan).unwrap_err().to_string().contains("render evidence"));
    let (fe,mut plan)=fixture();plan.frames[0].along=["a0".into(),"a2".into()];
    assert!(sheet_stability::apply(&fe,&plan).unwrap_err().to_string().contains("unrelated skeleton branch"));
    let (fe,mut plan)=fixture();plan.frames[0].along=["a1".into(),"a2".into()];
    assert!(sheet_stability::apply(&fe,&plan).is_err());
    let (fe,mut plan)=fixture();plan.frames.push(plan.frames[0].clone());
    assert!(sheet_stability::apply(&fe,&plan).unwrap_err().to_string().contains("duplicate fabric"));
    let (mut fe,plan)=fixture();
    if let Kind::Array(rods)=&mut fe.get_mut("m_Rods").unwrap().kind {
        rods.retain(|r|{let n=r.get("nNode").unwrap().array().unwrap();integer(&n[0])/12==integer(&n[1])/12});
    }
    assert!(sheet_stability::apply(&fe,&plan).unwrap_err().to_string().contains("disconnected native fabric"));
    let (fe,mut plan)=fixture();plan.frames.clear();
    let (_,proof)=sheet_stability::apply(&fe,&plan).unwrap();assert!(proof.surface_frames.is_empty());
    assert!(sheet_stability::apply(&fe,&Plan::default()).unwrap().0.canonical_eq(&fe));
}

fn rotation(q:[f64;4])->[[f64;3];3] {
    let n=q.iter().map(|v|v*v).sum::<f64>().sqrt();let [x,y,z,w]=q.map(|v|v/n);
    [[1.-2.*(y*y+z*z),2.*(x*y-z*w),2.*(x*z+y*w)],
     [2.*(x*y+z*w),1.-2.*(x*x+z*z),2.*(y*z-x*w)],
     [2.*(x*z-y*w),2.*(y*z+x*w),1.-2.*(x*x+y*y)]]
}

/// A returned, already-built VPK is sufficient to verify this transform: it
/// changes orientation fits only, not the compiler's points, rods or geometry.
/// This is a captured native replay, not an in-game movement test.
#[test]
#[ignore="requires DMB_SURFACE_MODEL and DMB_SURFACE_PLAN"]
fn captured_compiled_garment_preserves_every_other_physics_and_render_record() {
    use deadlock_resource::{binary,container};
    use sha2::{Digest,Sha256};
    use std::{collections::BTreeMap,fs};
    let native=fs::read(std::env::var("DMB_SURFACE_MODEL").unwrap()).unwrap();
    let setup:serde_json::Value=serde_json::from_slice(&fs::read(std::env::var("DMB_SURFACE_PLAN").unwrap()).unwrap()).unwrap();
    let mut plan:Plan=serde_json::from_value(setup["long_skirt"]["stability"].clone()).unwrap();
    assert!(!plan.frames.is_empty());
    // Same byte-wise compiler spelling, with unambiguous existing identities.
    let spelling=|s:&str|s.bytes().map(|b|if b.is_ascii_alphanumeric()||b==b'_'{b as char}else{'_'}).collect::<String>();
    let mut physics=container::read_blocks(&native,"PHYS").unwrap().remove(0);
    let fe=physics.root.get("m_pFeModel").unwrap();
    let names=fe.get("m_CtrlName").unwrap().array().unwrap().iter().map(|v|v.string().unwrap().to_owned()).collect::<Vec<_>>();
    let remap=|s:&String|if names.contains(s){s.clone()}else{spelling(s)};
    assert_eq!(plan.bones.iter().map(&remap).collect::<BTreeSet<_>>().len(),plan.bones.len());
    plan.bones=plan.bones.iter().map(&remap).collect();
    plan.seams=plan.seams.iter().map(|s|s.each_ref().map(&remap)).collect();
    plan.hinges=plan.hinges.iter().map(|s|s.each_ref().map(&remap)).collect();
    plan.frames=plan.frames.iter().map(|f|Frame{bone:remap(&f.bone),along:f.along.each_ref().map(&remap),across:f.across.each_ref().map(&remap)}).collect();
    let (out,proof)=sheet_stability::apply(fe,&plan).unwrap();
    assert_eq!(proof.surface_frames.len(),plan.frames.len());
    assert!(sheet_stability::apply(&out,&plan).unwrap().0.canonical_eq(&out));
    let mut preserved=Vec::new();
    for (key,old) in fe.object().unwrap(){if key!="m_NodeBases"{
        assert!(out.get(key).unwrap().canonical_eq(old),"Native field changed: {key}");preserved.push(key.clone());
    }}
    let selected=plan.frames.iter().map(|f|f.bone.as_str()).collect::<BTreeSet<_>>();
    let rest=positions(fe);let mut max_residual:f64=0.;let mut changed=0;
    for (old,new) in fe.get("m_NodeBases").unwrap().array().unwrap().iter().zip(out.get("m_NodeBases").unwrap().array().unwrap()) {
        let node=integer(old.get("nNode").unwrap());
        if !selected.contains(names[node].as_str()){assert!(old.canonical_eq(new));continue;}
        assert!(!old.canonical_eq(new));changed+=1;
        let b=basis(new,&rest);
        let q=new.get("qAdjust").unwrap().array().unwrap();let adjust=rotation(std::array::from_fn(|i|number(&q[i])));
        let pose=fe.get("m_InitPose").unwrap().array().unwrap()[node].array().unwrap();
        let expected=rotation(std::array::from_fn(|i|number(&pose[i+4])));
        for r in 0..3 {for c in 0..3 {
            let actual=(0..3).map(|k|b[r][k]*adjust[k][c]).sum::<f64>();
            max_residual=max_residual.max((actual-expected[r][c]).abs());
        }}
    }
    assert_eq!(changed,plan.frames.len());assert!(max_residual<1e-4);
    *physics.root.get_mut("m_pFeModel").unwrap()=out;
    assert_eq!(cloth::audit_rods(&physics.root).status,cloth::Status::Passed);
    let output=container::rebuild(&native,&BTreeMap::from([("PHYS".into(),binary::encode(&physics.root,physics.format_guid).unwrap())])).unwrap();
    let before=container::layout(&native).unwrap();let after=container::layout(&output).unwrap();
    assert_eq!(before.blocks.len(),after.blocks.len());let mut untouched=Vec::new();
    for (old,new) in before.blocks.iter().zip(&after.blocks) {
        assert_eq!(old.name,new.name);
        if old.name!="PHYS" {assert_eq!(container::payload(&native,old).unwrap(),container::payload(&output,new).unwrap());untouched.push(old.name.clone());}
    }
    let reread=container::read_blocks(&output,"PHYS").unwrap().remove(0);
    assert!(reread.root.canonical_eq(&physics.root));
    let hash=|b:&[u8]|format!("{:x}",Sha256::digest(b));
    let mut report=json!({"input_sha256":hash(&native),"output_sha256":hash(&output),"surface_frames":changed,
        "maximum_rest_rotation_residual":max_residual,"proof":proof,"preserved_fe_fields":preserved,
        "byte_identical_blocks":untouched,"native_replay":true,"fresh_compile":false,"in_game_tested":false});
    if let Ok(input)=std::env::var("DMB_SURFACE_VPK_INPUT") {
        use deadlock_vpk::{Vpk,write_vpk};
        use std::path::PathBuf;
        let destination=PathBuf::from(std::env::var("DMB_SURFACE_PACKAGE").unwrap());
        let staging=PathBuf::from(std::env::var("DMB_SURFACE_PACKAGE_STAGE").unwrap());
        assert!(!staging.exists() && !destination.exists());
        fs::create_dir_all(&staging).unwrap();fs::create_dir_all(destination.parent().unwrap()).unwrap();
        let package=Vpk::open(&input).unwrap();let mut expected=BTreeMap::new();let mut replaced=Vec::new();
        for entry in package.entries.values() {
            let bytes=package.read(&entry.path).unwrap();
            let bytes=if bytes==native {replaced.push(entry.path.clone());&output}else{&bytes};
            let path=staging.join(&entry.path);fs::create_dir_all(path.parent().unwrap()).unwrap();fs::write(path,bytes).unwrap();
            expected.insert(entry.path.clone(),hash(bytes));
        }
        assert_eq!(replaced.len(),1,"Package must contain exactly this verified model");
        let receipt=write_vpk(&staging,&expected.keys().cloned().collect::<Vec<_>>(),&destination,Some(&expected),package.version,&mut||Ok(())).unwrap();
        let checked=Vpk::open(&destination).unwrap();checked.verify_files(Some(&expected),&mut||Ok(())).unwrap();
        if package.version==2 {checked.verify_v2_hashes(&mut||Ok(())).unwrap();}
        report["package"]=json!({"receipt":receipt,"input_sha256":hash(&fs::read(&input).unwrap()),
            "output_sha256":hash(&fs::read(&destination).unwrap()),"replaced":replaced,"all_other_assets_byte_identical":true});
    }
    if let Ok(path)=std::env::var("DMB_SURFACE_OUTPUT") {fs::write(path,&output).unwrap();}
    if let Ok(path)=std::env::var("DMB_SURFACE_AUDIT") {fs::write(path,serde_json::to_vec_pretty(&report).unwrap()).unwrap();}
    println!("{changed} surface fits; rest rotation residual {max_residual}; other native fields and render blocks preserved");
}
