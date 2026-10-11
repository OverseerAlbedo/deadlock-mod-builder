use deadlock_cloth::generated::attachment_limits::{self,Plan,Restraint};
use deadlock_resource::{Kind,Value};
use serde_json::json;
fn value(v:serde_json::Value)->Value {Value::new(match v {
    serde_json::Value::Null=>Kind::Null,serde_json::Value::Bool(v)=>Kind::Bool(v),serde_json::Value::String(v)=>Kind::String(v),
    serde_json::Value::Array(v)=>Kind::Array(v.into_iter().map(value).collect()),
    serde_json::Value::Object(v)=>Kind::Object(v.into_iter().map(|(k,v)|(k,value(v))).collect()),
    serde_json::Value::Number(v)=>if let Some(n)=v.as_i64(){Kind::I64(n)}else{Kind::F64(v.as_f64().unwrap())},
})}
fn fixture()->(Value,Plan) {
    let fe=value(json!({"m_nNodeCount":5,"m_nStaticNodes":1,"m_nFirstPositionDrivenNode":4,
        "m_CtrlName":["body","a","b","c","shell"],"m_SkelParents":[-1,4,4,4,0],"m_NodeInvMasses":[0.,1.,1.,1.,1.],
        "m_InitPose":[[0,0,0,1,0,0,0,1],[0,0,0,1,0,0,0,1],[1,0,0,1,0,0,0,1],[0,1,0,1,0,0,0,1],[0,0,0,1,0,0,0,1]],
        "m_CtrlOffsets":[{"nCtrlParent":4,"nCtrlChild":1},{"nCtrlParent":4,"nCtrlChild":2},{"nCtrlParent":4,"nCtrlChild":3}],
        "m_NodeBases":[{"nNode":4,"nNodeX0":1,"nNodeX1":2,"nNodeY0":1,"nNodeY1":3,"qAdjust":[0,0,0,1]}],
        "m_NodeIntegrator":vec![json!({"flPointDamping":4.5,"flAnimationForceAttraction":0.2,"flAnimationVertexAttraction":0.2,"flGravity":1.});5],
        "m_AnimStrayRadii":[],"m_SimdAnimStrayRadii":[],"m_Rods":[1,2,3]}));
    (fe,Plan{bones:vec![Restraint{bone:"shell".into(),anchor:"body".into(),radius_inches:0.04,region:"upper_shell".into()}]})
}
#[test]
fn supports_stay_bounded_without_touching_body_fit_or_other_physics() {
    let (fe,plan)=fixture();let (out,proof)=attachment_limits::apply(&fe,&plan).unwrap();
    assert_eq!(proof.bones[0].points,[1,2,3]);assert!(proof.bones[0].maximum_normal_rotation_degrees<15.);
    assert!(attachment_limits::apply(&out,&plan).unwrap().0.canonical_eq(&out));
    for (key,v) in fe.object().unwrap() {if !["m_NodeIntegrator","m_AnimStrayRadii","m_SimdAnimStrayRadii"].contains(&key.as_str()) {assert!(out.get(key).unwrap().canonical_eq(v));}}
    for i in [0,4] {assert!(fe.get("m_NodeIntegrator").unwrap().array().unwrap()[i].canonical_eq(&out.get("m_NodeIntegrator").unwrap().array().unwrap()[i]));}
    attachment_limits::verify_coverage(&out).unwrap();
    assert!(attachment_limits::apply(&fe,&Plan::default()).unwrap().0.canonical_eq(&fe));
}
#[test]
fn oversized_radius_is_shape_bounded_and_invalid_selections_or_schedules_fail() {
    let (fe,mut plan)=fixture();plan.bones[0].radius_inches=100.;
    let (_,proof)=attachment_limits::apply(&fe,&plan).unwrap();
    assert!(proof.bones[0].radius_inches<0.11);assert!(proof.bones[0].maximum_normal_rotation_degrees<=26.75);
    let mut duplicate=plan.clone();duplicate.bones.push(plan.bones[0].clone());assert!(attachment_limits::apply(&fe,&duplicate).is_err());
    plan.bones[0].anchor="a".into();assert!(attachment_limits::apply(&fe,&plan).is_err());
    let (fe,plan)=fixture();let (mut out,_)=attachment_limits::apply(&fe,&plan).unwrap();
    *out.get_mut("m_AnimStrayRadii").unwrap()=value(json!([]));assert!(attachment_limits::apply(&out,&plan).is_err());
}
