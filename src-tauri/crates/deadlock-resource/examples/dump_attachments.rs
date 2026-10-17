//! Development aid: print where a compiled model keeps its attachments and
//! game-data keys, to compare a mod's compiled model with the game's current one.
//!
//!   dump_attachments MODEL.vmdl_c [NAME ...]
//!   dump_attachments OLD.vmdl_c --against NEW.vmdl_c
//!
//! The first form prints the named attachments from the first mesh data block,
//! the rest transform of `root_motion`, whether every MDAT block carries the
//! same attachment list, and whether the KV3 encoder round-trips the MDAT
//! blocks. The second form checks the October 2026 convention change: for every
//! attachment, new = (rest world rotation of its bone) applied to old.
use deadlock_resource::{binary, container, Kind, Value};

fn scalar(v: &Value) -> String {
    match &v.kind {
        Kind::String(s) => format!("{s:?}"),
        Kind::Bool(b) => b.to_string(),
        Kind::I64(n) => n.to_string(),
        Kind::U64(n) => n.to_string(),
        Kind::F32(n) => format!("{n}"),
        Kind::F64(n) => format!("{n}"),
        Kind::Array(items) => format!("[{}]", items.iter().map(scalar).collect::<Vec<_>>().join(", ")),
        Kind::Object(fields) => format!("{{{}}}", fields.iter().map(|(k, v)| format!("{k}: {}", scalar(v))).collect::<Vec<_>>().join(", ")),
        other => format!("<{other:?}>").chars().take(40).collect(),
    }
}
fn numbers(v: &Value) -> Vec<f64> {
    v.array().unwrap().iter().map(|n| n.number().unwrap()).collect()
}
type Quat = [f64; 4];
fn qmul(a: Quat, b: Quat) -> Quat {
    // (x, y, z, w) Hamilton product a*b
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}
fn rotate(q: Quat, v: [f64; 3]) -> [f64; 3] {
    let p = [v[0], v[1], v[2], 0.];
    let inv = [-q[0], -q[1], -q[2], q[3]];
    let r = qmul(qmul(q, p), inv);
    [r[0], r[1], r[2]]
}

struct Model {
    raw: Vec<u8>,
}
impl Model {
    fn open(path: &str) -> Self {
        Self { raw: std::fs::read(path).unwrap() }
    }
    /// Decoded MDAT blocks with their payloads, in file order.
    fn mdat(&self) -> Vec<(Vec<u8>, binary::Document)> {
        let layout = container::layout(&self.raw).unwrap();
        layout.blocks.iter().filter(|b| b.name == "MDAT").map(|b| {
            let payload = container::payload(&self.raw, b).unwrap().to_vec();
            let doc = binary::decode(&payload).unwrap();
            (payload, doc)
        }).collect()
    }
    fn skeleton(&self) -> Value {
        let data = container::read_blocks(&self.raw, "DATA").unwrap();
        data[0].root.get("m_modelSkeleton").unwrap().clone()
    }
    /// Rest world rotation per bone (parent chain product).
    fn rest_world_rotations(&self) -> (Vec<String>, Vec<Quat>) {
        let sk = self.skeleton();
        let names: Vec<String> = sk.get("m_boneName").unwrap().array().unwrap().iter().map(|n| n.string().unwrap().to_string()).collect();
        let parents: Vec<i64> = sk.get("m_nParent").unwrap().array().unwrap().iter().map(|n| n.integer().unwrap() as i64).collect();
        let locals: Vec<Quat> = sk.get("m_boneRotParent").unwrap().array().unwrap().iter().map(|q| { let n = numbers(q); [n[0], n[1], n[2], n[3]] }).collect();
        let mut world = vec![[0., 0., 0., 1.]; names.len()];
        for i in 0..names.len() {
            world[i] = if parents[i] < 0 { locals[i] } else { qmul(world[parents[i] as usize], locals[i]) };
        }
        (names, world)
    }
    fn attachments(&self) -> Vec<(String, String, [f64; 3], Quat)> {
        let (_, doc) = self.mdat().into_iter().next().unwrap();
        let mut out = vec![];
        for entry in doc.root.get("m_attachments").unwrap().array().unwrap() {
            let v = entry.get("value").unwrap_or(entry);
            let name = v.get("m_name").unwrap().string().unwrap().to_string();
            let bone = v.get("m_influenceNames").unwrap().array().unwrap()[0].string().unwrap().to_string();
            let o = numbers(&v.get("m_vInfluenceOffsets").unwrap().array().unwrap()[0]);
            let r = numbers(&v.get("m_vInfluenceRotations").unwrap().array().unwrap()[0]);
            out.push((name, bone, [o[0], o[1], o[2]], [r[0], r[1], r[2], r[3]]));
        }
        out
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 4 && args[1] == "--update" {
        // dump_attachments MODEL.vmdl_c --update GAME.vmdl_c OUT.vmdl_c
        let (model, game) = (std::fs::read(&args[0]).unwrap(), std::fs::read(&args[2]).unwrap());
        println!("convention: {:?}", deadlock_resource::attachments::convention(&model, &game).unwrap());
        match deadlock_resource::attachments::update(&model, &game, true).unwrap() {
            Some(update) => {
                std::fs::write(&args[3], &update.bytes).unwrap();
                println!("written {}: {} taken from the game, {} converted, {} blocks; now {:?}", args[3], update.taken_from_game, update.converted.len(), update.blocks, deadlock_resource::attachments::convention(&update.bytes, &game).unwrap());
            }
            None => println!("no change"),
        }
        return;
    }
    if args.len() == 3 && args[1] == "--factors" {
        factors(&Model::open(&args[0]), &Model::open(&args[2]));
        return;
    }
    if args.len() == 3 && args[1] == "--against" {
        let (old, new) = (Model::open(&args[0]), Model::open(&args[2]));
        let (names, world) = old.rest_world_rotations();
        let new_list = new.attachments();
        let mut ok = 0;
        let mut total = 0;
        for (name, bone, o, r) in old.attachments() {
            let Some((_, nb, no, nr)) = new_list.iter().find(|(n, ..)| *n == name) else { println!("{name}: not in the new model"); continue };
            total += 1;
            let Some(i) = names.iter().position(|n| *n == bone) else { println!("{name}: bone {bone} missing"); continue };
            let q = world[i];
            let po = rotate(q, o);
            let pr = qmul(q, r);
            let d_o = (0..3).map(|k| (po[k] - no[k]).abs()).fold(0., f64::max);
            let d_r = [1., -1.].into_iter().map(|s| (0..4).map(|k| (pr[k] * s - nr[k]).abs()).fold(0., f64::max)).fold(f64::MAX, f64::min);
            let good = d_o < 1e-3 && d_r < 1e-4 && bone == *nb;
            if good { ok += 1; }
            if !good { println!("{name:<22} bone {bone:<16} predicted offset {:?} rot {:?} | new offset {:?} rot {:?} (d_o {d_o:.5} d_r {d_r:.5})", po.map(|v| (v * 1000.).round() / 1000.), pr.map(|v| (v * 10000.).round() / 10000.), no.map(|v| (v * 1000.).round() / 1000.), nr.map(|v| (v * 10000.).round() / 10000.)); }
        }
        println!("rule new = rest_world_rotation(bone) * old holds for {ok} of {total} attachments");
        return;
    }
    let path = args.first().expect("MODEL.vmdl_c");
    let wanted: Vec<String> = args[1..].to_vec();
    let model = Model::open(path);
    let (names, world) = model.rest_world_rotations();
    for (i, name) in names.iter().enumerate() {
        if name == "root_motion" || wanted.iter().any(|w| w == name) {
            println!("bone {name}: rest world rotation {:?}", world[i].map(|v| (v * 10000.).round() / 10000.));
        }
    }
    let mut first: Option<String> = None;
    let (mut mdat, mut identical, mut round_trips, mut byte_identical) = (0, true, 0, 0);
    for (payload, doc) in model.mdat() {
        mdat += 1;
        let list = doc.root.get("m_attachments").map(scalar).unwrap_or_default();
        match &first {
            None => {
                first = Some(list.clone());
                if let Some(entries) = doc.root.get("m_attachments").and_then(|v| v.array().ok()) {
                    println!("m_attachments in the first MDAT block: {} entries", entries.len());
                    for entry in entries {
                        let value = entry.get("value").unwrap_or(entry);
                        let name = value.get("m_name").and_then(|v| v.string().ok()).unwrap_or("?");
                        if wanted.is_empty() || wanted.iter().any(|w| w == name) {
                            println!("  {}", scalar(value).chars().take(420).collect::<String>());
                        }
                    }
                }
            }
            Some(f) => identical &= *f == list,
        }
        let encoded = binary::encode(&doc.root, doc.format_guid).unwrap();
        if binary::decode(&encoded).unwrap().root.canonical_eq(&doc.root) { round_trips += 1; }
        if encoded == payload { byte_identical += 1; }
        let _ = &payload;
    }
    println!("MDAT blocks: {mdat}; same attachment list in every block: {identical}; KV3 round trips: {round_trips}/{mdat} canonical, {byte_identical}/{mdat} byte-identical");
}

/// `--factors OLD NEW`: per attachment, the left rotation factor new = q * old
/// and whether the offsets follow it, next to the bone's rest rotations.
pub fn factors(old: &Model, new: &Model) {
    let (names, world) = old.rest_world_rotations();
    let sk = old.skeleton();
    let locals: Vec<Quat> = sk.get("m_boneRotParent").unwrap().array().unwrap().iter().map(|q| { let n = numbers(q); [n[0], n[1], n[2], n[3]] }).collect();
    let parents: Vec<i64> = sk.get("m_nParent").unwrap().array().unwrap().iter().map(|n| n.integer().unwrap() as i64).collect();
    let new_list = new.attachments();
    let r4 = |q: Quat| q.map(|v| (v * 1000.).round() / 1000.);
    let r3 = |v: [f64; 3]| v.map(|x| (x * 100.).round() / 100.);
    for (name, bone, o, r) in old.attachments() {
        let Some((_, _, no, nr)) = new_list.iter().find(|(n, ..)| *n == name) else { continue };
        let Some(i) = names.iter().position(|n| *n == bone) else { println!("{name:<22} bone {bone:?} missing"); continue };
        let inv = [-r[0], -r[1], -r[2], r[3]];
        let q_left = qmul(*nr, inv);
        let q_right = qmul(inv, *nr);
        let off_left = rotate(q_left, o);
        let off_ok = (0..3).all(|k| (off_left[k] - no[k]).abs() < 1e-2);
        let parent_world = if parents[i] >= 0 { world[parents[i] as usize] } else { [0., 0., 0., 1.] };
        println!("{name:<22} {bone:<14} q_left {:?} q_right {:?} | W {:?} L {:?} Wparent {:?} | off {:?}->{:?} left-rule {}", r4(q_left), r4(q_right), r4(world[i]), r4(locals[i]), r4(parent_world), r3(o), r3(*no), if off_ok { "ok" } else { "NO" });
    }
}
