//! Join an existing long garment to its other *render-connected* cloth groups.
//! An upper coat can be too short to qualify as a skirt on its own. Its seam
//! with a skirt still needs to stay together. Names/proximity never create seams.
use crate::{parts::{Kind,Part},secondary::Group,surface,Result};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_skeleton::transforms;
use std::collections::{BTreeMap,BTreeSet};

pub(crate) fn seams(d:&Dmx,groups:&[Group],parts:&[Part],overrides:&BTreeMap<String,String>,
    roots:&BTreeSet<&str>,cancel:&CancellationToken)->Result<Vec<[String;2]>> {
    let bind=surface::binds(d)?;let sk=&bind.skeleton;let lookup=sk.indices();
    let state=transforms::bind_state(sk)?;
    let pos=|i:usize| std::array::from_fn::<_,3,_>(|r|state.world[i][r][3]);
    let mut body=BTreeSet::new();
    if let Some(head)=sk.names.iter().position(|n|n.eq_ignore_ascii_case("head")) {
        let mut at=head as i32;while at>=0 {body.insert(at as usize);at=sk.parents[at as usize];}
    }
    let mut owner=BTreeMap::new();
    for (g,(group,part)) in groups.iter().zip(parts).enumerate() {
        if !matches!(part.kind,Kind::Cloth|Kind::LongSkirt)
            || !body.contains(&lookup[group.anchor.as_str()])
            || overrides.get(&group.source_root).is_some_and(|k|k!="long_skirt") {continue;}
        for n in &group.weighted_bones {owner.insert(lookup[n.as_str()],g);}
    }
    let palette:Vec<_>=bind.palette.iter().map(|&e|lookup.get(d.elements[e].name.as_str()).copied()).collect();
    let mut faces=BTreeMap::<(usize,usize),usize>::new();
    let mut group_faces=BTreeMap::<(usize,usize),usize>::new();
    for mesh in surface::mesh_data(d)?.values() {
        let mut memberships=vec![BTreeSet::new();mesh.positions.len()];
        let mut dominant=vec![None;mesh.positions.len()];
        for i in 0..mesh.positions.len() {
            if i%4096==0 {cancel.check()?;} if !mesh.drawn[i] {continue;}
            let mut totals=BTreeMap::<usize,f64>::new();
            for (&slot,&w) in mesh.joints[i].iter().zip(&mesh.weights[i]) {
                if let Some(b)=palette.get(slot).copied().flatten() {if let Some(&g)=owner.get(&b) {
                    *totals.entry(g).or_default()+=w;if w>=0.1 {memberships[i].insert(b);}
                }}
            }
            if let Some((&g,&w))=totals.iter().max_by(|a,b|a.1.total_cmp(b.1)) {if w>=0.5 {dominant[i]=Some(g);}}
        }
        for face in surface::triangles(d,mesh,cancel,None)? {
            let gs:BTreeSet<_>=face.iter().filter_map(|&i|dominant[i]).collect();
            for &a in &gs {for &b in &gs {if a<b {*group_faces.entry((a,b)).or_default()+=1;}}}
            let bs:BTreeSet<_>=face.iter().flat_map(|&i|memberships[i].iter().copied()).collect();
            for &a in &bs {for &b in &bs {if a<b && owner[&a]!=owner[&b] {*faces.entry((a,b)).or_default()+=1;}}}
        }
    }
    // One nearest evidenced match per bone/neighbouring group prevents a
    // dense web of diagonal braces from turning the garment into a rigid cage.
    let mut closest=BTreeMap::<(usize,usize),(f64,usize)>::new();let mut supported=vec![];
    for ((a,b),count) in faces {
        let (ga,gb)=(owner[&a],owner[&b]);
        let key=if ga<gb {(ga,gb)} else {(gb,ga)};
        if count<2 || group_faces.get(&key).copied().unwrap_or(0)<3 {continue;}
        let distance=(0..3).map(|k|(pos(a)[k]-pos(b)[k]).powi(2)).sum::<f64>();
        if distance<=1e-10 {continue;}
        supported.push((a,b));
        for (bone,other_group,other) in [(a,gb,b),(b,ga,a)] {
            let entry=closest.entry((bone,other_group)).or_insert((distance,other));
            if (distance,other)<*entry {*entry=(distance,other);}
        }
    }
    supported.retain(|&(a,b)|closest[&(a,owner[&b])].1==b && closest[&(b,owner[&a])].1==a);
    let mut connected:BTreeSet<_>=groups.iter().enumerate().filter(|(_,g)|roots.contains(g.source_root.as_str())).map(|(g,_)|g).collect();
    loop {
        let before=connected.len();
        for &(a,b) in &supported {if connected.contains(&owner[&a]) || connected.contains(&owner[&b]) {connected.extend([owner[&a],owner[&b]]);}}
        if before==connected.len() {break;}
    }
    Ok(supported.into_iter().filter(|(a,b)|connected.contains(&owner[a]) && connected.contains(&owner[b]))
        .map(|(a,b)|[sk.names[a].clone(),sk.names[b].clone()]).collect())
}
