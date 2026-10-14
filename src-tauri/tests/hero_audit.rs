//! Every hero in the installed game, checked against the app's roster: the
//! model the game loads, other models of the hero (alternates), the hero's
//! flags (playable or not), and every hero's gun-ready pose through the app's
//! own gun reference. Run with --ignored and DMB_PAK; DMB_AUDIT_OUT names a JSON
//! file for the definitions report.
use deadlock_resource::Kind;

fn plain(value: &deadlock_resource::Value) -> serde_json::Value {
    match &value.kind {
        Kind::Bool(v) => serde_json::json!(v),
        Kind::I64(v) => serde_json::json!(v),
        Kind::U64(v) => serde_json::json!(v),
        Kind::F32(v) => serde_json::json!(v),
        Kind::F64(v) => serde_json::json!(v),
        Kind::String(v) => serde_json::json!(v),
        _ => serde_json::Value::Null,
    }
}

#[test]
#[ignore = "needs the Deadlock game archive"]
fn every_game_hero_with_its_model_flags_and_clips() {
    let Ok(pak) = std::env::var("DMB_PAK") else { return };
    let vpk = deadlock_vpk::Vpk::open(&pak).unwrap();
    let raw = vpk.read("scripts/heroes.vdata_c").unwrap();
    let doc = deadlock_components::icon_catalog::definition(&raw).unwrap();
    let mut heroes = vec![];
    for (key, hero) in doc.object().unwrap() {
        let Ok(fields) = hero.object() else { continue };
        let model = hero.get("m_strModelName").and_then(|v| v.string().ok()).unwrap_or_default().to_string();
        // Flags and short settings that say whether and how the hero is offered.
        let flags: serde_json::Map<String, serde_json::Value> = fields
            .iter()
            .filter(|(k, v)| k.starts_with("m_b") || (k.starts_with("m_n") && matches!(v.kind, Kind::I64(_) | Kind::U64(_))) || k.contains("Disabled") || k.contains("Selectable"))
            .map(|(k, v)| (k.clone(), plain(v)))
            .collect();
        // The model's own folder and the clips its animation resources reference.
        let compiled = format!("{}_c", model.trim_end_matches("_c"));
        let references = vpk.read(&compiled).ok().and_then(|raw| deadlock_vpk::resource_references(&raw).ok()).unwrap_or_default();
        let folder = model.rsplit_once('/').map(|(f, _)| f.to_string()).unwrap_or_default();
        let models: Vec<&String> = vpk.entries.keys().filter(|k| k.starts_with(&format!("{folder}/")) && k.ends_with(".vmdl_c")).collect();
        heroes.push(serde_json::json!({"key": key, "model": model, "flags": flags, "references": references, "folder_models": models}));
    }
    let report = serde_json::json!({"heroes": heroes});
    for hero in report["heroes"].as_array().unwrap() {
        let flags = hero["flags"].as_object().unwrap();
        let on: Vec<String> = flags.iter().filter(|(_, v)| v.as_bool() == Some(true)).map(|(k, _)| k.clone()).collect();
        println!("{:<28} {:<55} refs {:>3} | {}", hero["key"].as_str().unwrap(), hero["model"].as_str().unwrap(), hero["references"].as_array().unwrap().len(), on.join(" "));
    }
    if let Ok(out) = std::env::var("DMB_AUDIT_OUT") {
        std::fs::write(out, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}

/// Every roster hero through the app's own gun reference (as Prepare model
/// builds it): decompiled sources in DMB_HERO_SOURCES/<roster index>/, the
/// skeleton from DMB_PAK. With DMB_POSE_OBJ_DIR it writes <index>.obj (posed
/// body and guns) and rest/<index>.obj for a visual check. Decompile each hero
/// with `Source2Viewer-CLI -i pak01_dir.vpk -f <model>.vmdl_c -o <dir>/<index>/ -d`.
#[test]
#[ignore = "needs the Deadlock game archive and every hero's decompiled sources"]
fn every_hero_has_a_holding_pose_with_its_guns_in_hand() {
    let (Ok(pak), Ok(sources)) = (std::env::var("DMB_PAK"), std::env::var("DMB_HERO_SOURCES")) else { return };
    let out = std::env::var("DMB_POSE_OBJ_DIR").ok();
    let vpk = deadlock_vpk::Vpk::open(&pak).unwrap();
    let mut problems = vec![];
    for (index, hero) in deadlock_components::heroes::roster().unwrap().into_iter().enumerate() {
        let base = std::path::Path::new(&sources).join(index.to_string());
        let raw = vpk.read(&format!("{}_c", hero.path)).unwrap();
        let Ok(data) = deadlock_resource::container::read_blocks(&raw, "DATA") else {
            problems.push(format!("{}: model DATA unreadable", hero.name));
            continue;
        };
        let skeleton = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
        let (target, _) = deadlock_skeleton::transforms::prepare_target(&skeleton).unwrap();
        let Ok(text) = std::fs::read_to_string(base.join(&hero.path)) else {
            problems.push(format!("{}: no decompiled source", hero.name));
            continue;
        };
        let simulated = skeleton.names.iter().zip(&skeleton.flags).filter(|(_, f)| **f & 0x400000 != 0).map(|(n, _)| n.clone()).collect();
        let reference = match deadlock_preparation::weapon::reference_with_simulated(&base, &text, &target, &simulated, false, &deadlock_core::CancellationToken::new()) {
            Ok(r) => r,
            Err(e) => {
                problems.push(format!("{}: gun reference failed: {e}", hero.name));
                continue;
            }
        };
        let Some(stance) = reference.stance.as_ref() else {
            problems.push(format!("{}: no holding pose", hero.name));
            continue;
        };
        let height = |p: &[f32]| {
            let z = p.chunks_exact(3).map(|v| v[2]);
            z.clone().fold(f32::MIN, f32::max) - z.fold(f32::MAX, f32::min)
        };
        let (rest, held) = (height(&reference.body_shape.positions), height(&stance.body_shape.positions));
        let sides: Vec<String> = reference.sides.iter().map(|s| s.side.clone()).collect();
        let guns = if sides.is_empty() { usize::from(!reference.shape.is_empty()) } else { sides.len() };
        let carried = if sides.is_empty() { usize::from(stance.hands.contains_key("right")) } else { sides.iter().filter(|s| stance.hands.contains_key(*s)).count() };
        println!("{index:>2} {:<14} {:<30} height {held:>5.1}/{rest:<5.1} guns {guns} carried {carried}", hero.name, stance.clip);
        if !(held > rest * 0.7 && held < rest * 1.3) {
            problems.push(format!("{}: not upright ({held} of {rest})", hero.name));
        }
        if carried < guns {
            problems.push(format!("{}: {carried} of {guns} guns move with a hand", hero.name));
        }
        let mut text = String::new();
        let mut first = 1u32;
        let mut put = |shape: &deadlock_preparation::weapon::Shape, m: Option<&deadlock_skeleton::transforms::Matrix4>, group: &str| {
            text += &format!("o {group}\n");
            for p in shape.positions.chunks_exact(3) {
                let p = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
                let q = m.map_or(p, |m| [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3]));
                text += &format!("v {} {} {}\n", q[0], q[1], q[2]);
            }
            for t in shape.triangles.chunks_exact(3) {
                text += &format!("f {} {} {}\n", t[0] + first, t[1] + first, t[2] + first);
            }
            first += (shape.positions.len() / 3) as u32;
        };
        put(&stance.body_shape, None, "body");
        if reference.sides.is_empty() {
            put(&reference.shape, stance.hands.get("right"), "gun");
        }
        for side in &reference.sides {
            put(&side.shape, stance.hands.get(&side.side), "gun");
        }
        let Some(out) = &out else { continue };
        std::fs::write(std::path::Path::new(out).join(format!("{index:02}.obj")), text).unwrap();
        // The rest pose too, to compare.
        let mut rest_text = String::new();
        let mut first = 1u32;
        for (shape, group) in [(&reference.body_shape, "body")].into_iter().chain(reference.sides.iter().map(|s| (&s.shape, "gun"))).chain([(&reference.shape, "gun")]) {
            rest_text += &format!("o {group}
");
            for p in shape.positions.chunks_exact(3) {
                rest_text += &format!("v {} {} {}
", p[0], p[1], p[2]);
            }
            for t in shape.triangles.chunks_exact(3) {
                rest_text += &format!("f {} {} {}
", t[0] + first, t[1] + first, t[2] + first);
            }
            first += (shape.positions.len() / 3) as u32;
        }
        std::fs::create_dir_all(std::path::Path::new(out).join("rest")).unwrap();
        std::fs::write(std::path::Path::new(out).join("rest").join(format!("{index:02}.obj")), rest_text).unwrap();
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
#[ignore = "debug"]
fn print_compiled_only_bones() {
    let Ok(pak) = std::env::var("DMB_PAK") else { return };
    let vpk = deadlock_vpk::Vpk::open(&pak).unwrap();
    for hero in deadlock_components::heroes::roster().unwrap() {
        let raw = vpk.read(&format!("{}_c", hero.path)).unwrap();
        let Ok(data) = deadlock_resource::container::read_blocks(&raw, "DATA") else { continue };
        let skeleton = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
        let special: Vec<&String> = skeleton.names.iter().filter(|n| n.starts_with('$')).collect();
        if !special.is_empty() {
            println!("{:<14} {} of {} bones: {:?}", hero.name, special.len(), skeleton.names.len(), special.iter().take(6).collect::<Vec<_>>());
        }
    }
}

#[test]
#[ignore = "debug"]
fn print_cloth_bone_details() {
    let (Ok(pak), Ok(model)) = (std::env::var("DMB_PAK"), std::env::var("DMB_MODEL")) else { return };
    let vpk = deadlock_vpk::Vpk::open(&pak).unwrap();
    let data = deadlock_resource::container::read_blocks(&vpk.read(&model).unwrap(), "DATA").unwrap();
    let skeleton = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
    let mut shown = 0;
    for (i, name) in skeleton.names.iter().enumerate() {
        if !name.starts_with('$') { continue; }
        let parent = usize::try_from(skeleton.parents[i]).ok().map(|p| skeleton.names[p].clone());
        if shown < 4 || parent.as_deref().is_some_and(|p| !p.starts_with('$')) && shown < 12 {
            println!("{i:>3} {name:<18} parent {parent:?} flags {:#x} pos {:?} rot {:?} scale {}", skeleton.flags[i], skeleton.positions[i].map(|v| (v * 1000.).round() / 1000.), skeleton.rotations[i].map(|v| (v * 1000.).round() / 1000.), skeleton.scales[i]);
            shown += 1;
        }
    }
    let flags: std::collections::BTreeSet<u32> = skeleton.names.iter().zip(&skeleton.flags).filter(|(n, _)| n.starts_with('$')).map(|(_, f)| *f).collect();
    println!("cloth bone flags {flags:x?}; parents of $ bones that are not $: {:?}", skeleton.names.iter().enumerate().filter(|(_, n)| n.starts_with('$')).filter_map(|(i, _)| usize::try_from(skeleton.parents[i]).ok().map(|p| skeleton.names[p].clone())).filter(|p| !p.starts_with('$')).collect::<std::collections::BTreeSet<_>>());
}

#[test]
#[ignore = "debug"]
fn print_named_bone() {
    let (Ok(pak), Ok(model), Ok(bone)) = (std::env::var("DMB_PAK"), std::env::var("DMB_MODEL"), std::env::var("DMB_BONE")) else { return };
    let vpk = deadlock_vpk::Vpk::open(&pak).unwrap();
    let bytes = if std::path::Path::new(&model).is_file() { std::fs::read(&model).unwrap() } else { vpk.read(&model).unwrap() };
    let data = deadlock_resource::container::read_blocks(&bytes, "DATA").unwrap();
    let skeleton = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
    for name in bone.split(',') {
        if let Some(like) = name.strip_prefix('*') { let hits: Vec<&String> = skeleton.names.iter().filter(|n| n.contains(like)).collect(); println!("{like}: {} bones, first {:?}", hits.len(), hits.first()); continue }
        let Some(i) = skeleton.names.iter().position(|n| n == name) else { println!("{name}: absent"); continue };
        let parent = usize::try_from(skeleton.parents[i]).ok().map(|p| skeleton.names[p].clone());
        println!("{name}: parent {parent:?} flags {:#x} pos {:?} rot {:?} scale {}", skeleton.flags[i], skeleton.positions[i], skeleton.rotations[i], skeleton.scales[i]);
    }
}
