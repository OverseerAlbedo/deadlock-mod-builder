//! Body-relative long garments, joined only where the rendered fabric joins.
//! Names, column numbering and proximity alone never authorize a seam.
use crate::{
    invalid, parts::{Kind, Part}, secondary::Group, surface,
    surface_geometry::{cross, dot, norm, sub, Surface}, Result,
};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_skeleton::transforms;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY: &str = "surface-fitted-long-skirt-v7";

#[derive(Debug, Serialize)]
pub struct SkirtGroup {
    pub root: String,
    pub weighted_height_inches: f64,
    pub torso_length_inches: f64,
    pub supporting_positions: usize,
    pub travel_radius_by_bone: BTreeMap<String, f64>,
    #[serde(skip)]
    pub(crate) coordinates: BTreeMap<String, f64>,
    #[serde(skip)]
    pub(crate) widths: BTreeMap<String, [f64;3]>,
}
#[derive(Debug, Serialize)]
pub struct Link {
    pub bones: [String; 2],
    pub supporting_render_triangles: usize,
}
#[derive(Debug, Serialize)]
pub struct Plan {
    pub policy: String,
    pub groups: Vec<SkirtGroup>,
    pub links: Vec<Link>,
    /// Four corners, in perimeter order. Both transverse edges have independent
    /// render evidence; both longitudinal edges are immediate skeleton links.
    pub shear_cells: Vec<[String; 4]>,
    pub stretch_tethers: bool,
    pub body_collisions_authored: bool,
    pub proxy_structure: String,
    pub stability: deadlock_cloth::generated::sheet_stability::Plan,
}
impl Plan {
    pub(crate) fn group(&self, root: &str) -> Option<&SkirtGroup> {
        self.groups.iter().find(|g| g.root == root)
    }
}
fn pair(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}

pub(crate) fn analyze(
    d: &Dmx, groups: &[Group], parts: &mut [Part],
    overrides: &BTreeMap<String, String>, cancel: &CancellationToken,
) -> Result<Option<Plan>> {
    let bind = surface::binds(d)?;
    let sk = &bind.skeleton;
    let state = transforms::bind_state(sk)?;
    let lookup = sk.indices();
    let named = |n: &str| sk.names.iter().position(|v| v.eq_ignore_ascii_case(n));
    let explicit = overrides.values().any(|v| v == "long_skirt");
    let missing = || invalid("Long-skirt support needs a waist-height garment with connected weighted panels and head/pelvis landmarks. Choose Cloth for an independent piece.");
    let (Some(head), Some(pelvis)) = (named("head"), named("pelvis")) else {
        return if explicit { Err(missing()) } else { Ok(None) };
    };
    let pos = |i: usize| std::array::from_fn(|r| state.world[i][r][3]);
    let torso_vector = sub(pos(head), pos(pelvis));
    let torso = norm(torso_vector);
    if !torso.is_finite() || torso <= 1e-4 {
        return if explicit { Err(missing()) } else { Ok(None) };
    }
    let up = torso_vector.map(|v| v / torso);
    let height = |p| dot(sub(p, pos(pelvis)), up);
    let mut body = BTreeSet::new();
    let mut at = head as i32;
    while at >= 0 {
        body.insert(at as usize);
        if at as usize == pelvis { break; }
        at = sk.parents[at as usize];
    }
    if !body.contains(&pelvis) { return if explicit { Err(missing()) } else { Ok(None) }; }
    let mut candidates = BTreeSet::new();
    let mut extents = BTreeMap::new();
    for (g, (group, part)) in groups.iter().zip(parts.iter()).enumerate() {
        cancel.check()?;
        // A deliberate part choice remains authoritative. An explicit long
        // skirt still needs the same geometric evidence as automatic mode.
        if overrides.get(&group.source_root).is_some_and(|v| v != "long_skirt")
            || !matches!(part.kind, Kind::Cloth | Kind::LongSkirt | Kind::Accessory)
            || (part.kind == Kind::Accessory && !part.review_recommended)
            || !body.contains(&lookup[group.anchor.as_str()])
            || group.weighted_bones.len() < 3 { continue; }
        let heights: Vec<_> = group.weighted_bones.iter().map(|n| height(pos(lookup[n.as_str()]))).collect();
        let top = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let bottom = heights.iter().copied().fold(f64::INFINITY, f64::min);
        // Weighted joints must start near the waist and extend well down the
        // legs. This excludes shoulder capes and upper-body ribbons even when
        // an exporter leaves very long terminal/helper bones on them.
        if top.abs() <= torso * 0.25 && bottom <= -torso * 0.5 && top - bottom >= torso * 0.65 {
            candidates.insert(g);
            extents.insert(g, (top, bottom));
        }
    }
    let mut owner = BTreeMap::<usize,usize>::new();
    for &g in &candidates {
        for name in &groups[g].weighted_bones { owner.insert(lookup[name.as_str()],g); }
    }
    let palette: Vec<_> = bind.palette.iter().map(|&e| lookup.get(d.elements[e].name.as_str()).copied()).collect();
    let meshes = surface::mesh_data(d)?;
    let mut samples = vec![vec![]; groups.len()];
    let mut group_faces = BTreeMap::<(usize, usize), usize>::new();
    let mut bone_faces = BTreeMap::<(usize, usize), usize>::new();
    for mesh in meshes.values() {
        let mut memberships = vec![BTreeSet::new(); mesh.positions.len()];
        let mut dominant = vec![None; mesh.positions.len()];
        for (i, point) in mesh.positions.iter().enumerate() {
            if i % 4096 == 0 { cancel.check()?; }
            if !mesh.drawn[i] { continue; }
            let mut totals = BTreeMap::<usize, f64>::new();
            for (&slot, &weight) in mesh.joints[i].iter().zip(&mesh.weights[i]) {
                let Some(bone) = palette.get(slot).copied().flatten() else { continue; };
                if let Some(&g) = owner.get(&bone) {
                    *totals.entry(g).or_default() += weight;
                    if weight >= 0.1 { memberships[i].insert(bone); }
                }
            }
            if let Some((&g, &weight)) = totals.iter().max_by(|a,b| a.1.total_cmp(b.1)) {
                if weight >= 0.5 { samples[g].push(height(*point)); dominant[i] = Some(g); }
            }
        }
        for face in surface::triangles(d, mesh, cancel, None)? {
            let owners: BTreeSet<_> = face.iter().filter_map(|&i| dominant[i]).collect();
            for &a in &owners { for &b in &owners { if a < b { *group_faces.entry((a,b)).or_default() += 1; } } }
            let bones: BTreeSet<_> = face.iter().flat_map(|&i| memberships[i].iter().copied()).collect();
            for &a in &bones { for &b in &bones {
                if a < b { *bone_faces.entry((a,b)).or_default() += 1; }
            } }
        }
    }
    // Require actual visible fabric, not only a misleading long rig. Trim
    // sparse outliers so a distant decoration cannot qualify a short garment.
    candidates.retain(|&g| {
        let points = &mut samples[g];
        points.sort_by(f64::total_cmp);
        if points.len() < 12 { return false; }
        let low = points[(points.len()-1)*5/100];
        let high = points[(points.len()-1)*95/100];
        high.abs() <= torso * 0.35 && low <= -torso * 0.5 && high-low >= torso*0.65
    });
    // Mutual nearest-height matching prevents diagonal braces between every
    // neighboring skin influence. Only independently evidenced mesh neighbors
    // qualify, and no proximity-based connection closes a split or slit.
    let mut closest = BTreeMap::<(usize, usize), (f64, usize)>::new();
    let mut supported = vec![];
    for (&(a,b), &count) in &bone_faces {
        let (ga,gb) = (owner[&a], owner[&b]);
        let delta = (height(pos(a))-height(pos(b))).abs();
        if ga == gb || count < 2 || !candidates.contains(&ga) || !candidates.contains(&gb)
            || groups[ga].anchor != groups[gb].anchor
            || group_faces.get(&pair(ga,gb)).copied().unwrap_or(0) < 3
            || delta > torso * 0.08 || norm(sub(pos(a),pos(b))) < torso*1e-5 { continue; }
        supported.push((a,b,count));
        for (bone, other_group, other) in [(a,gb,b),(b,ga,a)] {
            let entry = closest.entry((bone,other_group)).or_insert((delta,other));
            if (delta,other) < *entry { *entry = (delta,other); }
        }
    }
    let mut links = vec![];
    let mut linked = BTreeSet::new();
    for (a,b,count) in supported {
        let (ga,gb) = (owner[&a],owner[&b]);
        if closest[&(a,gb)].1 != b || closest[&(b,ga)].1 != a { continue; }
        links.push(Link { bones: [sk.names[a].clone(),sk.names[b].clone()], supporting_render_triangles: count });
        linked.extend([ga,gb]);
    }
    for (g,group) in groups.iter().enumerate() {
        if overrides.get(&group.source_root).is_some_and(|v| v == "long_skirt") && !linked.contains(&g) {
            return Err(missing());
        }
    }
    if links.is_empty() { return Ok(None); }
    let mut planned = vec![];
    for g in linked {
        let group = &groups[g];
        let (top,bottom) = extents[&g];
        let mut coordinates = BTreeMap::new();
        let mut travel = BTreeMap::new();
        let mut widths = BTreeMap::new();
        let group_names: BTreeSet<_> = group.bones.iter().map(String::as_str).collect();
        let mut evidence = Vec::new();
        for link in &links {
            for (a,b) in [(&link.bones[0],&link.bones[1]),(&link.bones[1],&link.bones[0])] {
                if group_names.contains(a.as_str()) {
                    let delta = sub(pos(lookup[b.as_str()]),pos(lookup[a.as_str()]));
                    evidence.push((pos(lookup[a.as_str()]),delta));
                }
            }
        }
        for name in &group.bones {
            let t = ((top-height(pos(lookup[name.as_str()])))/(top-bottom)).clamp(0.,1.);
            coordinates.insert(name.clone(),t);
            // Firm waist, progressively freer hem. Tethers limit accumulated
            // extension without raising the existing cloth attraction/drag.
            travel.insert(name.clone(), torso*(0.02+0.30*t));
            // The garment width follows evidenced adjacent fabric, independent
            // of imported bone roll. Branch/terminal joints use the nearest
            // supported row of their own group; they never create new seams.
            let p = pos(lookup[name.as_str()]);
            let (_,width) = evidence.iter().min_by(|a,b|norm(sub(p,a.0)).total_cmp(&norm(sub(p,b.0)))).unwrap();
            widths.insert(name.clone(),*width);
        }
        parts[g].kind = Kind::LongSkirt;
        parts[g].reason = "Waist-height weighted fabric extends down the legs; source mesh verifies neighboring skirt panels".into();
        parts[g].review_recommended = false;
        parts[g].roll_locked = false;
        planned.push(SkirtGroup { root: group.source_root.clone(), weighted_height_inches: top-bottom,
            torso_length_inches: torso, supporting_positions: samples[g].len(), travel_radius_by_bone: travel, coordinates, widths });
    }
    let pairs: BTreeSet<_> = links.iter().map(|l| pair(lookup[l.bones[0].as_str()],lookup[l.bones[1].as_str()])).collect();
    let mut shear_cells = Vec::new();
    for link in &links {
        let [a,b] = link.bones.each_ref().map(|n| lookup[n.as_str()]);
        let (pa,pb) = (sk.parents[a],sk.parents[b]);
        if pa < 0 || pb < 0 || !pairs.contains(&pair(pa as usize,pb as usize)) { continue; }
        // A missing row, open slit or unrelated branch cannot be bridged by
        // proximity. The four existing edges must enclose this actual cell.
        if owner.get(&(pa as usize)) != owner.get(&a) || owner.get(&(pb as usize)) != owner.get(&b) { continue; }
        if bone_faces.get(&pair(pa as usize,a)).copied().unwrap_or(0) < 2
            || bone_faces.get(&pair(pb as usize,b)).copied().unwrap_or(0) < 2 { continue; }
        shear_cells.push([sk.names[pa as usize].clone(),sk.names[pb as usize].clone(),sk.names[b].clone(),sk.names[a].clone()]);
    }
    let garment_roots: BTreeSet<_> = planned.iter().map(|g|g.root.as_str()).collect();
    let mut bones: BTreeSet<String> = groups.iter().filter(|g|garment_roots.contains(g.source_root.as_str()))
        .flat_map(|g|g.weighted_bones.iter().cloned()).collect();
    let mut hinges=BTreeSet::new();
    for name in &bones {
        let b=lookup[name.as_str()];let p=sk.parents[b];
        if p<0 {continue;} let gp=sk.parents[p as usize];
        if gp>=0 && bones.contains(&sk.names[p as usize]) && bones.contains(&sk.names[gp as usize]) {
            hinges.insert([sk.names[gp as usize].clone(),sk.names[p as usize].clone(),name.clone()]);
        }
    }
    // Cross-panel folds also need limits, but only when two independently
    // evidenced neighbors continue in opposite directions across the cloth.
    let mut neighbors=BTreeMap::<&str,Vec<&str>>::new();
    for link in &links {
        neighbors.entry(&link.bones[0]).or_default().push(&link.bones[1]);
        neighbors.entry(&link.bones[1]).or_default().push(&link.bones[0]);
    }
    for (middle,adjacent) in &neighbors {
        for (at,&a) in adjacent.iter().enumerate(){for &b in &adjacent[at+1..]{
            let u=sub(pos(lookup[a]),pos(lookup[*middle]));let v=sub(pos(lookup[b]),pos(lookup[*middle]));
            if dot(u,v) < -0.5*norm(u)*norm(v) {
                hinges.insert([a.to_string(),(*middle).to_string(),b.to_string()]);
            }
        }}
    }
    // Wide garments must obtain their orientation from the moving sheet.
    // Local four-point proxies are narrow enough to roll sharply while their
    // position centers remain near the right place. Share transverse support
    // across evidenced columns, as native surface cloth does. Do not extend
    // this to independent streamers, ornaments, gaps or unsupported branches.
    let mut frames=Vec::new();
    for (&name,adjacent) in &neighbors {
        let bone=lookup[name];
        let child=neighbors.keys().copied().filter(|n|sk.parents[lookup[n]]==bone as i32)
            .min_by(|a,b|norm(sub(pos(lookup[a]),pos(bone))).total_cmp(&norm(sub(pos(lookup[b]),pos(bone)))));
        let along=if let Some(child)=child {[name,child]} else {
            let parent=sk.parents[bone];
            if parent<0 || !neighbors.contains_key(sk.names[parent as usize].as_str()){continue;}
            [sk.names[parent as usize].as_str(),name]
        };
        let tangent=sub(pos(lookup[along[1]]),pos(lookup[along[0]]));
        let mut across=[name,adjacent[0]];
        let mut span=0.;
        for (at,&a) in adjacent.iter().enumerate(){for &b in &adjacent[at+1..]{
            let u=sub(pos(lookup[a]),pos(bone));let v=sub(pos(lookup[b]),pos(bone));
            let distance=norm(sub(pos(lookup[a]),pos(lookup[b])));
            if dot(u,v)< -0.5*norm(u)*norm(v) && distance>span {across=[a,b];span=distance;}
        }}
        let width=sub(pos(lookup[across[1]]),pos(lookup[across[0]]));
        // Ambiguous geometry keeps its native fitting. No invented world axis.
        if norm(cross(width,tangent))<0.55*norm(width)*norm(tangent){continue;}
        frames.push(deadlock_cloth::generated::sheet_frames::Frame {bone:name.to_owned(),
            along:along.map(str::to_owned),across:across.map(str::to_owned)});
    }
    let mut seams=crate::fabric_cohesion::seams(d,groups,parts,overrides,&garment_roots,cancel)?;
    // Retain every transverse connection already verified by skirt detection,
    // even when Euclidean matching prefers a different row on a curved hem.
    seams.extend(links.iter().map(|l|l.bones.clone()));
    for pair in &mut seams {pair.sort();}
    seams.sort();seams.dedup();
    bones.extend(seams.iter().flatten().cloned());
    let stability=deadlock_cloth::generated::sheet_stability::Plan {bones:bones.into_iter().collect(),hinges:hinges.into_iter().collect(),seams,frames,..Default::default()};
    Ok(Some(Plan { policy: POLICY.into(), groups: planned, links, shear_cells, stability, stretch_tethers: true, body_collisions_authored: false,
        proxy_structure: "Continuous open fabric sheet with render-supported shear cells and shared surface orientation".into() }))
}

pub(crate) fn couple(s: &mut Surface, slots: &BTreeMap<&str, usize>, plan: &Plan) -> Result<()> {
    let mut frames = BTreeMap::<usize,Vec<usize>>::new();
    for (i,weights) in s.influences.iter().enumerate() {
        if s.enabled[i] && weights.len() == 1 {
            frames.entry(*weights.keys().next().unwrap()).or_default().push(i);
        }
    }
    let get = |name: &str| -> Result<[usize;4]> {
            frames.get(&slots[name]).and_then(|v| v.as_slice().try_into().ok())
                .ok_or_else(|| invalid("Long-skirt seam requires one verified four-point fabric patch."))
    };
    let mut edges = BTreeMap::new();
    for link in &plan.links {
        let a = get(&link.bones[0])?;
        let b = get(&link.bones[1])?;
        let start = s.faces.len();
        // Join the nearest free longitudinal edges of the two open patches.
        // Two faces form a sheet. Joining every edge would recreate the rigid
        // tube lattice that suppressed the skirt's bending in the v1 proxy.
        let mut choices = Vec::new();
        for side_a in 0..2 { for side_b in 0..2 {
            let cost = norm(sub(s.points[a[side_a]],s.points[b[side_b]]))
                + norm(sub(s.points[a[side_a+2]],s.points[b[side_b+2]]));
            choices.push((cost,side_a,side_b));
        } }
        let &(_,i,j) = choices.iter().min_by(|a,b|a.0.total_cmp(&b.0)).unwrap();
        edges.insert((link.bones[0].as_str(),link.bones[1].as_str()),(i,j));
        edges.insert((link.bones[1].as_str(),link.bones[0].as_str()),(j,i));
        s.faces.extend([[a[i],b[j],a[i+2]],[a[i+2],b[j],b[j+2]]]);
        if s.faces[start..].iter().any(|f| {
            let u = sub(s.points[f[1]],s.points[f[0]]);
            let v = sub(s.points[f[2]],s.points[f[0]]);
            norm(cross(u,v)) <= norm(u)*norm(v)*1e-8
        }) { return Err(invalid("Long-skirt seam has a collapsed support triangle.")); }
    }
    // The previous narrow strips left a four-sided hole between each pair of
    // rows. Fill it with two triangles: a fabric shear constraint, not a rigid
    // cage or a change to point attraction. Reuse existing points/maps exactly.
    for cell in &plan.shear_cells {
        let [pa,pb,b,a] = cell.each_ref().map(String::as_str);
        let (pi,pj) = edges[&(pa,pb)];
        let (i,j) = edges[&(a,b)];
        let corners = [get(pa)?[pi+2],get(pb)?[pj+2],get(b)?[j],get(a)?[i]];
        let triangles = [[corners[0],corners[1],corners[3]],[corners[3],corners[1],corners[2]]];
        for f in triangles {
            let u=sub(s.points[f[1]],s.points[f[0]]);let v=sub(s.points[f[2]],s.points[f[0]]);
            if norm(cross(u,v)) <= norm(u)*norm(v)*1e-8 { return Err(invalid("Long-skirt shear cell has a collapsed support triangle.")); }
            s.faces.push(f);
        }
    }
    Ok(())
}
