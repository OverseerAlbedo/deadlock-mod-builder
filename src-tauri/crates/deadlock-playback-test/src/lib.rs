pub mod animation;
pub mod behaviors;
pub mod disconnect;
pub mod install;
pub mod native;
pub mod primary;
use animation::*;
use deadlock_resource::{independent, Kind};
use deadlock_vpk::Vpk;
use native::*;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::Path,
    process::Command,
};

pub fn probe(path: &Path) -> Result<serde_json::Value> {
    let vpk = Vpk::open(path)?;
    let raw = vpk.read(MODEL)?;
    let doc = data(&raw, "DATA")?;
    let skel = deadlock_skeleton::Skeleton::from_value(field(&doc.root, "m_modelSkeleton")?)?;
    let branch = selected(&skel)?;
    require(branch.iter().all(|&j| skel.flags[j] & 0x400008 == 0x400008),
        "Left-streamer physics is already modified. Restore your original working TangTang VPK before building this revision")?;
    require(
        !arr(&doc.root, "m_vecNmSkeletonRefs")?
            .iter()
            .any(|v| v.string().ok() == Some(SECONDARY)),
        "The test is already installed",
    )?;
    Ok(
        serde_json::json!({"supported":true,"model":MODEL,"model_sha256":hash(&raw),"files":vpk.entries.len()}),
    )
}
pub fn scan_addons(path: &Path) -> Result<serde_json::Value> {
    let mut rows = vec![];
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if !path.is_file()
            || path
                .extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                != Some("vpk".into())
        {
            continue;
        }
        if let Ok(archive) = Vpk::open(&path) {
            if archive.entries.contains_key(MODEL) {
                let result = probe(&path);
                rows.push(serde_json::json!({"path":path,"supported":result.is_ok(),"reason":result.err().map(|e|e.to_string())}));
            }
        }
    }
    Ok(serde_json::json!({"haze_mods":rows}))
}
fn primary_clips(
    game: &Vpk,
    model: &[u8],
    skeleton: &str,
    output: &Path,
) -> Result<(Vec<(String, Vec<u8>)>, Vec<String>, Vec<String>)> {
    let roots: Vec<_> = refs(model)?
        .into_iter()
        .map(|(_, s)| s)
        .filter(|s| s.ends_with(".vnmgraph"))
        .collect();
    require(!roots.is_empty(), "Model has no native graph references")?;
    // Scope comes from the model's explicit graph identifier, never a clip
    // filename. Unknown graph roots remain strict gameplay dependencies.
    let model_data = data(model, "DATA")?;
    let mut root_scopes = BTreeMap::<String, u8>::new();
    if let Some(defs) = model_data.root.get("m_animGraph2Refs") {
        for def in defs.array()? {
            let path = field(def, "m_hGraph")?.string()?.to_owned();
            let is_ui = field(def, "m_sIdentifier")?.string()? == "ui";
            *root_scopes.entry(path).or_default() |= if is_ui { 2 } else { 1 };
        }
    }
    let mut queue = VecDeque::new();
    for path in roots {
        let scopes = root_scopes.get(&path).copied().unwrap_or(1);
        for scope in [1u8, 2] {
            if scopes & scope != 0 {
                queue.push_back((path.clone(), scope));
            }
        }
    }
    let mut visited = BTreeSet::new();
    let mut graph_refs = BTreeMap::<String, Vec<String>>::new();
    let mut gameplay_clips = BTreeSet::new();
    let mut ui_clips = BTreeSet::new();
    let mut referrers = BTreeMap::<String, BTreeSet<String>>::new();
    while let Some((path, scope)) = queue.pop_front() {
        if !visited.insert((path.clone(), scope)) {
            continue;
        }
        // A shared subgraph must be traversed once in EACH scope. Cache bytes
        // so capture paths are written only once and gameplay takes priority.
        if !graph_refs.contains_key(&path) {
            require(graph_refs.len() < 4096, "Graph dependency limit exceeded")?;
            let asset = format!("{path}_c");
            let raw = capture_input(game, &asset, output, &format!("game/{asset}"))?;
            let names = refs(&raw)
                .map_err(|e| format!("Reading graph references in {asset}: {e}"))?
                .into_iter()
                .map(|(_, n)| n)
                .collect();
            graph_refs.insert(path.clone(), names);
        }
        for name in &graph_refs[&path] {
            if name.ends_with(".vnmgraph") {
                queue.push_back((name.clone(), scope));
            } else if name.ends_with(".vnmclip") {
                let asset = format!("{name}_c");
                referrers
                    .entry(asset.clone())
                    .or_default()
                    .insert(path.clone());
                if scope == 1 {
                    gameplay_clips.insert(asset);
                } else {
                    ui_clips.insert(asset);
                }
            }
        }
    }
    let reachable: BTreeSet<_> = gameplay_clips.union(&ui_clips).cloned().collect();
    let available: BTreeSet<_> = game
        .entries
        .keys()
        .filter(|p| p.ends_with(".vnmclip_c"))
        .cloned()
        .collect();
    let missing_gameplay: Vec<_> = gameplay_clips.difference(&available).cloned().collect();
    let missing_ui: Vec<_> = ui_clips
        .difference(&available)
        .filter(|p| !gameplay_clips.contains(*p))
        .cloned()
        .collect();
    let missing_referrers: BTreeMap<_, _> = missing_gameplay
        .iter()
        .chain(&missing_ui)
        .map(|p| (p.clone(), referrers[p].clone()))
        .collect();
    write_new(
        &output.join("dependency-report.json"),
        &serde_json::to_vec_pretty(&serde_json::json!({
            "scope_source":"model m_animGraph2Refs identifiers propagated through graph references",
            "unavailable_gameplay_clips":missing_gameplay,"unavailable_ui_clips":missing_ui,
            "missing_clip_referrers":missing_referrers,"available_clip_entries":available.len(),
            "ui_missing_action":"Leave already-absent UI-only clips absent; extend every existing matching clip",
            "install_performed":false
        }))?,
    )?;
    let missing_ui = primary::verify_coverage(&gameplay_clips, &ui_clips, &available)?;
    for path in &missing_ui {
        eprintln!("UI-only clip is absent from the game archive; left absent: {path}");
    }
    let directory = skeleton.rsplit_once('/').ok_or("Invalid skeleton path")?.0;
    let mut found = vec![];
    let mut authored_count = 0;
    for path in available {
        let raw = game.read(&path)?;
        // Include clips used outside the graph closure (UI/external playback).
        // Inspect references before parsing unrelated heroes' animation DATA.
        let uses_skeleton = refs(&raw)?
            .iter()
            .any(|(_, r)| r.eq_ignore_ascii_case(skeleton));
        if !reachable.contains(&path)
            && !path.starts_with(&format!("{directory}/"))
            && !uses_skeleton
        {
            continue;
        }
        write_new(&output.join("inputs/game").join(&path), &raw)?;
        let doc = input_data(&raw, "DATA", output, &format!("game/{path}"))?;
        if field(&doc.root, "m_skeleton")?.string()? != skeleton {
            continue;
        }
        if primary::has_authored_motion(&path, &doc.root)? {
            authored_count += 1;
        }
        found.push((path, raw));
    }
    require(authored_count > 0,"No animated Haze idle clip was found. Nothing was installed; collect the results for the next test.")?;
    Ok((found, graph_refs.into_keys().collect(), missing_ui))
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
// Capture source bytes before interpreting them. Early decode failures must be
// reproducible even when no candidate VPK or build report can be generated.
fn capture_input(archive: &Vpk, asset: &str, output: &Path, relative: &str) -> Result<Vec<u8>> {
    deadlock_vpk::relative_asset(relative)?;
    let raw = archive
        .read(asset)
        .map_err(|e| format!("Reading {asset}: {e}"))?;
    write_new(&output.join("inputs").join(relative), &raw)?;
    Ok(raw)
}
fn input_data(
    raw: &[u8],
    block: &str,
    output: &Path,
    relative: &str,
) -> Result<deadlock_resource::binary::Document> {
    data(raw, block).map_err(|error| {
        let message = format!("Reading {relative} [{block}]: {error}");
        let report = serde_json::json!({
            "stage":"decode_input", "asset":relative, "block":block,
            "sha256":hash(raw), "bytes":raw.len(), "error":error.to_string(),
            "captured_input":format!("inputs/{relative}"), "install_performed":false
        });
        let diagnostic = output.join("failure-report.json");
        let saved = serde_json::to_vec_pretty(&report)
            .map_err(|e| e.to_string())
            .and_then(|bytes| write_new(&diagnostic, &bytes).map_err(|e| e.to_string()));
        match saved {
            Ok(()) => {
                format!("{message}. The exact input was saved for 4-Collect-Results.cmd.").into()
            }
            Err(e) => format!("{message}. Could not write failure report: {e}").into(),
        }
    })
}
fn audit_native(reader: &Path, path: &Path, block: &str, raw: &[u8]) -> Result<usize> {
    // Child execution has a bounded wait on Windows through the launcher's
    // process timeout; development builds also use the same binary protocol.
    let mut child = Command::new(reader)
        .arg("-i")
        .arg(path)
        .arg("-b")
        .arg(block)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    // Drain while waiting so the native DATA dump cannot fill a pipe and hang.
    let stdout = child.stdout.take().ok_or("Missing reader stdout")?;
    let stderr = child.stderr.take().ok_or("Missing reader stderr")?;
    let out_thread = std::thread::spawn(move || {
        use std::io::Read;
        let mut out = vec![];
        stdout
            .take(128 * 1024 * 1024)
            .read_to_end(&mut out)
            .map(|_| out)
    });
    let err_thread = std::thread::spawn(move || {
        use std::io::Read;
        let mut out = vec![];
        stderr.take(1024 * 1024).read_to_end(&mut out).map(|_| out)
    });
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed().as_secs() > 45 {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Independent native reader timed out; nothing installed".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let output = out_thread
        .join()
        .map_err(|_| "Reader output thread failed")??;
    let errors = err_thread
        .join()
        .map_err(|_| "Reader error thread failed")??;
    require(
        status.success(),
        &format!(
            "Independent native reader failed: {}",
            String::from_utf8_lossy(&errors)
        ),
    )?;
    let docs = independent::parse_cli_blocks(std::str::from_utf8(&output)?)?;
    require(docs.len() == 1, "Native reader returned multiple blocks")?;
    Ok(independent::verify(&docs[0].root, &data(raw, block)?.root)?.values_checked)
}
pub fn build(
    game_path: &Path,
    mod_path: &Path,
    motion_path: &Path,
    output: &Path,
    reader: &Path,
) -> Result<serde_json::Value> {
    require(
        !output.exists(),
        "Use a fresh output directory; existing tests are never overwritten",
    )?;
    require(
        reader.is_file(),
        "Independent native reader is missing from the test package",
    )?;
    let motion_bytes = fs::read(motion_path)?;
    let motion: behaviors::Behaviors = serde_json::from_slice(&motion_bytes)?;
    motion.validate()?;
    let game = Vpk::open(game_path)?;
    let base = Vpk::open(mod_path)?;
    fs::create_dir_all(output)?;
    let mod_raw = capture_input(&base, MODEL, output, "model.vmdl_c")?;
    probe(mod_path)?;
    let model_data = input_data(&mod_raw, "DATA", output, "model.vmdl_c")?;
    let primary_refs: Vec<_> = arr(&model_data.root, "m_vecNmSkeletonRefs")?
        .iter()
        .map(|v| v.string().map(str::to_owned))
        .collect::<std::result::Result<_, _>>()?;
    require(
        primary_refs.len() == 1,
        "This test expects one original Haze animation skeleton",
    )?;
    let primary = &primary_refs[0];
    let original_relative = format!("game/{MODEL}");
    let original_model = capture_input(&game, MODEL, output, &original_relative)?;
    let original_data = input_data(&original_model, "DATA", output, &original_relative)?;
    require(
        arr(&original_data.root, "m_vecNmSkeletonRefs")?
            .iter()
            .any(|v| v.string().ok() == Some(primary)),
        "The installed model and current game use different animation skeletons",
    )?;
    let primary_path = format!("{primary}_c");
    let primary_raw = capture_input(&game, &primary_path, output, "primary.vnmskel_c")?;
    let primary_data = input_data(&primary_raw, "DATA", output, "primary.vnmskel_c")?;
    let (clips, graphs, missing_ui) = primary_clips(&game, &original_model, primary, output)?;
    let primary_bones = arr(&primary_data.root, "m_boneIDs")?.len();
    for (_, raw) in &clips {
        require(
            arr(&data(raw, "DATA")?.root, "m_trackCompressionSettings")?.len() == primary_bones,
            "Native clip track count differs from the primary skeleton",
        )?;
    }
    require(!base.entries.contains_key(&primary_path.to_ascii_lowercase()) && clips.iter().all(|(p,_)|!base.entries.contains_key(&p.to_ascii_lowercase())),"The chosen mod already changes Haze animations; this first test cannot combine those edits")?;
    eprintln!(
        "Found {} matching native clips across the installed game; {} reachable graphs.",
        clips.len(),
        graphs.len()
    );
    let (model, skel, physics) = prepare_primary_model(&mod_raw)?;
    let (primary, binding) = primary::behavior_skeleton(&primary_raw, &skel)?;
    let mut run_profiles = behaviors::RunProfiles::new();
    for graph in &graphs {
        let relative = format!("game/{graph}_c");
        let raw = fs::read(output.join("inputs").join(&relative))?;
        let doc = input_data(&raw, "DATA", output, &relative)?;
        behaviors::collect_graph_run_profiles(&doc.root, &mut run_profiles)
            .map_err(|e| format!("Reading movement blend {graph}: {e}"))?;
    }
    let mut selections = BTreeMap::new();
    let mut behavior_counts = BTreeMap::<String, usize>::new();
    for (path, raw) in &clips {
        let selection = behaviors::select(path, &data(raw, "DATA")?.root, &run_profiles)?;
        if let Some(ref behavior) = selection {
            *behavior_counts.entry(behavior.category.into()).or_default() += 1;
        }
        selections.insert(path.clone(), selection);
    }
    require(
        ["idle", "run", "turn", "stop"]
            .iter()
            .all(|k| behavior_counts.get(*k).copied().unwrap_or(0) > 0),
        "Current game lacks a required authored state family; collect results before adapting",
    )?;
    let mut receipt = serde_json::json!({"schema":2,"test":"one-streamer-native-playback-primary-r4","scope":"Restrained left-streamer authored state test; R3 binding user-confirmed, R4 behavior acceptance pending","target_side":"character left; viewer left from behind","original_primary_bones":binding.original_bones,"output_primary_bones":binding.original_bones+7,"all_primary_bones_sampled_at_low_lod_for_test":true,"in_game_validated":false,"native_windows_executed":cfg!(windows),"base_mod":mod_path,"base_model_sha256":hash(&mod_raw),"base_mod_sha256":hash(&fs::read(mod_path)?),"original_game_model_sha256":hash(&original_model),"authored_motion_sha256":hash(&motion_bytes),"physics":physics,"graphs":graphs,"clips":[],"variants":{},"body_animation_preserved":true,"install_performed":false});
    receipt["revision"] = serde_json::json!("primary-r4-authored-behaviors");
    receipt["behavior_counts"] = serde_json::json!(behavior_counts);
    receipt["run_direction_source"] = serde_json::json!("Native forward_speed/strafe_speed Blend2D slots; graph bytes unchanged; unreferenced run clips stay neutral");
    receipt["graph_run_profiles"] = serde_json::json!(run_profiles);
    receipt["turn_mask_policy"] = serde_json::json!("Only the seven added bones have UpperBodyReplace_Turn weight zero; original body weights unchanged");
    receipt["r3_binding_user_confirmed"] = serde_json::json!(true);
    receipt["unavailable_ui_clips"] = serde_json::json!(missing_ui);
    let mut outputs = BTreeMap::new();
    let mut independent_values = 0usize;
    for (animated, label) in [(true, "animated"), (false, "fixed")] {
        let stage = output.join(label).join("resources");
        fs::create_dir_all(&stage)?;
        let mut changed = BTreeMap::from([
            (MODEL.to_owned(), model.clone()),
            (primary_path.clone(), primary.clone()),
        ]);
        for (path, raw) in &clips {
            let selected = if animated {
                selections[path].as_ref()
            } else {
                None
            };
            let frames = field(&data(raw, "DATA")?.root, "m_nNumFrames")?.integer()? as usize;
            let samples = selected.map(|selection| motion.samples(selection, frames));
            let edited=primary::clip_with_deltas(raw,&binding,&skel,samples.as_deref())
                .map_err(|e| format!("Authoring native clip {path}: {e}. Input captured for 4-Collect-Results.cmd."))?;
            changed.insert(path.clone(), edited);
        }
        let mut expected = BTreeMap::new();
        for entry in base.entries.values() {
            let raw = changed
                .remove(&entry.path)
                .unwrap_or(base.read(&entry.path)?);
            let target = stage.join(&entry.path);
            write_new(&target, &raw)?;
            expected.insert(entry.path.clone(), hash(&raw));
        }
        for (path, raw) in changed {
            write_new(&stage.join(&path), &raw)?;
            expected.insert(path, hash(&raw));
        }
        let mut modified = vec![MODEL.to_owned(), primary_path.clone()];
        modified.extend(clips.iter().map(|(p, _)| p.clone()));
        for path in modified {
            let file = stage.join(&path);
            let bytes = fs::read(&file)?;
            independent_values += audit_native(reader, &file, "DATA", &bytes)?;
            if path == MODEL {
                independent_values += audit_native(reader, &file, "PHYS", &bytes)?;
            }
        }
        let destination = output.join(format!("{label}.vpk"));
        let package = deadlock_vpk::write_vpk(
            &stage,
            &expected.keys().cloned().collect::<Vec<_>>(),
            &destination,
            Some(&expected),
            2,
            &mut || Ok(()),
        )?;
        let published = Vpk::open(&destination)?;
        published.verify_files(Some(&expected), &mut || Ok(()))?;
        published.verify_v2_hashes(&mut || Ok(()))?;
        outputs.insert(label,serde_json::json!({"vpk":destination,"sha256":hash(&fs::read(&destination)?),"files":package.file_count,"physics_disabled":true,"keyframes_enabled":animated}));
        eprintln!("Verified {label} comparison package.");
    }
    let clip_receipts:Vec<_>=clips.iter().map(|(path,raw)|{let d=data(raw,"DATA").unwrap();serde_json::json!({"path":path,"sha256":hash(raw),"duration":number(field(&d.root,"m_flDuration").unwrap()).unwrap(),"frames":field(&d.root,"m_nNumFrames").unwrap().integer().unwrap(),"authored_behavior":selections[path],"additive":matches!(field(&d.root,"m_bIsAdditive").unwrap().kind,Kind::Bool(true))})}).collect();
    receipt["clips"] = serde_json::json!(clip_receipts);
    receipt["variants"] = serde_json::json!(outputs);
    receipt["independent_native_values_checked"] = serde_json::json!(independent_values);
    write_new(
        &output.join("build-report.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

pub fn inspect_model(path: &Path) -> Result<serde_json::Value> {
    let raw = fs::read(path)?;
    let skel = deadlock_skeleton::Skeleton::from_value(field(
        &data(&raw, "DATA")?.root,
        "m_modelSkeleton",
    )?)?;
    selected(&skel)?;
    Ok(serde_json::json!({"skeleton":skel,"sha256":hash(&raw)}))
}

/// Read-only comparison of a captured native input against the independent
/// reader. Kept in the experiment so actual game regressions are reproducible.
pub fn verify_input(path: &Path, reader: &Path) -> Result<serde_json::Value> {
    let raw = fs::read(path)?;
    let layout = deadlock_resource::container::layout(&raw)?;
    let mut blocks = vec![];
    for name in ["DATA", "PHYS"] {
        if layout.blocks.iter().any(|block| block.name == name) {
            let count = audit_native(reader, path, name, &raw)?;
            let doc = data(&raw, name)?;
            encode(&doc)?;
            blocks.push(
                serde_json::json!({"block":name,"values_checked":count,"roundtrip_exact":true}),
            );
        }
    }
    require(!blocks.is_empty(), "No DATA or PHYS block to verify")?;
    Ok(serde_json::json!({"file":path.file_name(),"sha256":hash(&raw),"blocks":blocks}))
}
