//! A skirt is an open sheet, not a stack of closed triangular bone cages.
use crate::{invalid, long_skirt::SkirtGroup, parts::Part, secondary::Group,
    secondary_auto::{parameters,point,Maps}, surface_geometry::{dot,norm,sub,Surface},Result};
use deadlock_skeleton::{transforms::BindState,Skeleton};
use std::collections::BTreeMap;

fn unit(v:[f64;3])->Result<[f64;3]> {
    let n=norm(v);
    if !n.is_finite() || n<1e-6 {return Err(invalid("Long-skirt fabric has a collapsed direction."));}
    Ok(v.map(|x|x/n))
}
fn patch(s:&mut Surface,maps:&mut Vec<Maps>,p:[f64;3],width:[f64;3],delta:[f64;3],radius:f64,bone:usize,dynamic:bool,settings:Maps)->[usize;4] {
    let start=s.points.len();
    for (down,side) in [(0.,-1.),(0.,1.),(0.5,-1.),(0.5,1.)] {
        point(s,maps,std::array::from_fn(|k|p[k]+down*delta[k]+side*radius*width[k]),bone,dynamic,settings);
    }
    s.faces.extend([[start,start+1,start+2],[start+2,start+1,start+3]]);
    [start,start+1,start+2,start+3]
}
pub(crate) fn append(s:&mut Surface,group:&Group,part:&Part,slots:&BTreeMap<&str,usize>,lookup:&BTreeMap<&str,usize>,
    sk:&Skeleton,state:&BindState,choice:Option<&str>,skirt:&SkirtGroup,maps:&mut Vec<Maps>)->Result<()> {
    let pos=|name:&str|std::array::from_fn::<_,3,_>(|r|state.world[lookup[name]][r][3]);
    let mut frames=BTreeMap::<String,[usize;4]>::new();
    let mut directions=BTreeMap::new();
    let mut widths=BTreeMap::<String,[f64;3]>::new();
    for name in &group.bones {
        let i=lookup[name.as_str()];let p=pos(name);
        let children:Vec<_>=group.bones.iter().filter(|n|sk.parents[lookup[n.as_str()]]==i as i32).collect();
        // Prefer the actual downward continuation at a branch, then the
        // shortest child. This avoids letting a side ornament define the sheet.
        let child=children.iter().max_by(|a,b|skirt.coordinates[a.as_str()].total_cmp(&skirt.coordinates[b.as_str()])
            .then_with(||norm(sub(pos(b),p)).total_cmp(&norm(sub(pos(a),p)))));
        let delta=if let Some(child)=child {sub(pos(child),p)} else {
            sub(p,pos(&sk.names[sk.parents[i] as usize]))
        };
        let tangent=unit(delta)?;
        let raw=skirt.widths[name];
        let projected=std::array::from_fn(|k|raw[k]-tangent[k]*dot(raw,tangent));
        let mut width=unit(projected)?;
        if let Some(parent)=widths.get(&sk.names[sk.parents[i] as usize]) {
            if dot(*parent,width)<0. {width=width.map(|v|-v);}
        }
        widths.insert(name.clone(),width);directions.insert(name.clone(),delta);
    }
    let pitch=skirt.weighted_height_inches/(group.weighted_bones.len().max(2)-1) as f64;
    let radius=(pitch*0.12).clamp(0.05,0.45);
    let root_delta=directions[&group.root];
    let tangent=unit(root_delta)?;
    let offset=(radius*3.).max((norm(root_delta)*0.25).min(1.));
    let fixed=patch(s,maps,std::array::from_fn(|k|pos(&group.root)[k]-tangent[k]*offset),widths[&group.root],
        tangent.map(|v|v*offset),radius,slots[group.anchor.as_str()],false,[0.,0.,1.,0.,0.,0.,0.,0.]);
    for name in &group.bones {
        let mut settings=parameters(part.kind,skirt.coordinates[name],name==&group.root,skirt.weighted_height_inches,radius,choice)?;
        settings[4]=skirt.travel_radius_by_bone[name];settings[6]=1.;
        let ids=patch(s,maps,pos(name),widths[name],directions[name],radius,slots[name.as_str()],true,settings);
        let parent=if name==&group.root {fixed} else {
            frames[&sk.names[sk.parents[lookup[name.as_str()]] as usize]]
        };
        // Only the trailing/leading width edges are connected. No cap or
        // back-side braces oppose the intended fabric hinge.
        s.faces.extend([[parent[2],parent[3],ids[0]],[ids[0],parent[3],ids[1]]]);
        frames.insert(name.clone(),ids);
    }
    Ok(())
}
