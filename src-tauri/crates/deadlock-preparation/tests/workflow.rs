//! Process-double orchestration evidence; these tests do not certify Blender/native tools.
use deadlock_core::{
    Result as CoreResult,
    process::{CommandSpec, ProcessResult, ToolExecutor},
};
use deadlock_preparation::{
    blender::BlenderBridge,
    provenance,
    service::{self, PreparationService},
    source,
};
use deadlock_project::{ModelInputMode, PreparationOptions, Project};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
struct FixtureTool {
    data: Value,
    calls: Mutex<Vec<Value>>,
    emit: bool,
}
impl ToolExecutor for FixtureTool {
    fn run(&self, spec: &CommandSpec) -> CoreResult<ProcessResult> {
        let args: Vec<_> = spec
            .args
            .iter()
            .map(|s| s.to_str().unwrap().to_string())
            .collect();
        let dir = spec
            .cwd
            .clone()
            .unwrap_or_else(|| spec.program.parent().unwrap().to_owned());
        fs::create_dir_all(&dir).unwrap();
        let stdout = dir.join("fixture-stdout.txt");
        fs::write(&stdout, "Blender 4.5.3\n").unwrap();
        let stderr = dir.join("fixture-stderr.txt");
        fs::write(&stderr, "").unwrap();
        if args != ["--version"] {
            assert_eq!(
                &args[..6],
                [
                    "--background",
                    "--factory-startup",
                    "--disable-autoexec",
                    "--python-exit-code",
                    "23",
                    "--python"
                ]
            );
            assert!(args[6].ends_with("worker.py"));
            assert_eq!(args[7], "--");
            let job: Value = serde_json::from_slice(&fs::read(&args[8]).unwrap()).unwrap();
            self.calls.lock().unwrap().push(job.clone());
            if self.emit {
                let mut response = if job["operation"] == "inspect" {
                    self.data["inspection"].clone()
                } else {
                    let mut report = self.data["report"].clone();
                    let extras: Vec<_> = report["target"].as_array().into_iter().flatten()
                        .filter(|b| b["name"].as_str().is_some_and(|n| n.starts_with("dmb_secondary_"))).cloned().collect();
                    report["target"] = job["target"].clone();
                    if job["correction"] != true && job["options"]["preserve_physics_bones"] == true {
                        report["target"].as_array_mut().unwrap().extend(extras);
                    }
                    report["status"] = json!("ok");
                    report["blend"] = json!(dir.join("prepared.blend").to_str().unwrap());
                    fs::write(dir.join("prepared.blend"), b"fixture correction scene").unwrap();
                    report
                };
                response["status"] = json!("ok");
                fs::write(
                    job["result"].as_str().unwrap(),
                    serde_json::to_vec(&response).unwrap(),
                )
                .unwrap();
            }
        }
        Ok(ProcessResult {
            command_id: "fixture".into(),
            code: Some(0),
            success: true,
            stdout_path: stdout,
            stderr_path: stderr,
            receipt_path: dir.join("fixture-process.json"),
            elapsed_ns: 1,
            stdout_bytes: 14,
            stderr_bytes: 0,
            dropped_live_events: 0,
        })
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/reference.json")).unwrap()
}
#[test]
fn proportion_choice_survives_presets_project_save_and_corrections_and_rejects_stale_outputs() {
    use deadlock_preparation::{fitting, mapping_presets};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project"); fs::create_dir(&root).unwrap();
    let source = root.join("source.gltf"); fs::write(&source, b"{}").unwrap();
    let compiled = root.join("hero.vmdl_c"); fs::write(&compiled, b"target fixture").unwrap();
    let executable = root.join("blender.exe"); fs::write(&executable, b"fixture").unwrap();
    let mut data = fixture();
    // This process double qualifies option/receipt transport only. Real geometry
    // and Blender correction round trips have separate algorithm/API tests.
    data["report"]["rest_alignment"]["policy"] = json!(fitting::PROPORTIONS_POLICY);
    let tool = Arc::new(FixtureTool {data: data.clone(), calls: Mutex::new(vec![]), emit: true});
    let svc = PreparationService::new(root, BlenderBridge {executable, executor: tool.clone()});
    let mut project = Project::default();
    project.original_model = "models/test/hero.vmdl_c".into();
    project.model_input_mode = ModelInputMode::RiggedHumanoid;
    let analysis = svc.inspect(&project, source.to_str().unwrap(), [0.; 3], &compiled,
        serde_json::from_value(data["skeleton"].clone()).unwrap()).unwrap();
    let options: PreparationOptions = serde_json::from_value(json!({
        "reviewed": true, "fit_mode": "preserve_proportions", "fit_arm_joints": false, "arm_skinning": "smooth_twist",
        "source_roles": analysis.suggestions.source.as_ref().unwrap().roles,
        "target_roles": analysis.suggestions.target.roles, "mapping": analysis.suggestions.mapping
    })).unwrap();
    let before = tool.calls.lock().unwrap().len();
    for choice in [json!(""), json!("guess"), json!(true), json!(null), json!([])] {
        let mut invalid = options.clone(); invalid.values.insert("fit_mode".into(), choice);
        assert!(service::review_issues(&project, &analysis, &invalid).unwrap().iter().any(|i| i.tab == "model fitting"));
        assert!(svc.prepare(&project, &analysis, invalid, false, &compiled).unwrap_err().to_string().contains("Model fitting"));
    }
    assert_eq!(before, tool.calls.lock().unwrap().len());
    let mut invalid_arms=options.clone();invalid_arms.values.insert("fit_arm_joints".into(),json!("yes"));
    assert!(service::review_issues(&project,&analysis,&invalid_arms).unwrap().iter().any(|i|i.tab=="model fitting"));
    let preset = mapping_presets::capture(&analysis, &options).unwrap();
    let restored = mapping_presets::restore(preset, &mapping_presets::key(&analysis).unwrap()).unwrap();
    assert_eq!(restored.values["fit_mode"], "preserve_proportions");
    assert_eq!(restored.values["fit_arm_joints"], false);
    assert_eq!(restored.values["arm_skinning"], "smooth_twist");
    assert_eq!(restored.values["reviewed"], false);
    let result = svc.prepare(&project, &analysis, options.clone(), false, &compiled).unwrap();
    assert_eq!(tool.calls.lock().unwrap().last().unwrap()["options"]["fit_mode"], "preserve_proportions");
    assert_eq!(tool.calls.lock().unwrap().last().unwrap()["options"]["fit_arm_joints"], false);
    let accepted = service::handoff(&project, &result).unwrap();
    let saved: Project = serde_json::from_slice(&serde_json::to_vec(&accepted).unwrap()).unwrap();
    assert_eq!(saved.preparation_options.values["fit_mode"], "preserve_proportions");
    service::check_handoff(&saved).unwrap();
    let mut changed_skin=saved.clone();changed_skin.preparation_options.values.insert("arm_skinning".into(),json!("source"));
    assert!(service::check_handoff(&changed_skin).unwrap_err().to_string().contains("Model fitting changed"));
    let mut invalid_skin=options.clone();invalid_skin.values.insert("arm_skinning".into(),json!("guess"));
    assert!(fitting::mode(&invalid_skin).is_err());
    let mut changed_arms=saved.clone();changed_arms.preparation_options.values.insert("fit_arm_joints".into(),json!(true));
    assert!(service::check_handoff(&changed_arms).unwrap_err().to_string().contains("Model fitting changed"));
    let corrected = svc.prepare(&saved, &analysis, options.clone(), true, &compiled).unwrap();
    let corrected = service::handoff(&saved, &corrected).unwrap();
    assert_eq!(corrected.preparation_options, options);
    service::check_handoff(&corrected).unwrap();
    let mut changed = saved.clone(); changed.preparation_options.values.remove("fit_mode");
    assert!(service::check_handoff(&changed).unwrap_err().to_string().contains("Model fitting changed"));
    let before = tool.calls.lock().unwrap().len();
    assert!(svc.prepare(&changed, &analysis, changed.preparation_options.clone(), true, &compiled).unwrap_err().to_string().contains("Model fitting changed"));
    assert_eq!(before, tool.calls.lock().unwrap().len());
    let mut receipt: Value = serde_json::from_slice(&fs::read(&saved.preparation_report).unwrap()).unwrap();
    receipt["rest_alignment"]["policy"] = json!(service::REST_POLICY);
    fs::write(&saved.preparation_report, serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert!(service::check_handoff(&saved).unwrap_err().to_string().contains("does not match"));
    assert_eq!(fs::read(source).unwrap(), b"{}");
}
#[test]
fn preserved_rig_survives_correction_presets_and_handoff_without_resetting_compatible_choices() {
    use deadlock_project::{MeshMapping, MaterialChoices};
    use deadlock_preparation::mapping_presets;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project"); fs::create_dir(&root).unwrap();
    let source = root.join("input.gltf"); fs::write(&source, b"{}").unwrap();
    let compiled = root.join("hero.vmdl_c"); fs::write(&compiled, b"original compiled fixture").unwrap();
    let executable = root.join("blender.exe"); fs::write(&executable, b"fixture").unwrap();
    let data: Value = serde_json::from_str(include_str!("fixtures/secondary-bones.json")).unwrap();
    let target: Vec<deadlock_skeleton::transforms::PreparedBone> = serde_json::from_value(data["target"].clone()).unwrap();
    let skeleton = deadlock_skeleton::Skeleton {
        names: target.iter().map(|b| b.name.clone()).collect(),
        parents: target.iter().map(|b| b.parent.as_ref().map_or(-1, |p| target.iter().position(|t| &t.name == p).unwrap() as i32)).collect(),
        positions: target.iter().map(|b| b.local_position).collect(), rotations: target.iter().map(|b| b.local_rotation).collect(),
        scales: target.iter().map(|b| b.local_scale).collect(), flags: vec![0x40;target.len()],
    };
    let tool = Arc::new(FixtureTool {data:data.clone(), calls:Mutex::new(vec![]), emit:true});
    let svc = PreparationService::new(root, BlenderBridge {executable,executor:tool.clone()});
    let mut project = Project::default(); project.original_model="models/test/hero.vmdl_c".into(); project.model_input_mode=ModelInputMode::RiggedHumanoid;
    let analysis = svc.inspect(&project, source.to_str().unwrap(), [0.;3], &compiled, skeleton).unwrap();
    let mut options: PreparationOptions = serde_json::from_value(data["options"].clone()).unwrap();
    options.values.insert("automatic_secondary_motion".into(),json!(true));
    let preset = mapping_presets::capture(&analysis, &options).unwrap();
    let restored = mapping_presets::restore(preset, &mapping_presets::key(&analysis).unwrap()).unwrap();
    assert_eq!(restored.values["physics_roots"], json!(["appendage"]));
    assert_eq!(restored.values["automatic_secondary_motion"], true);
    assert_eq!(restored.values["reviewed"], false);
    let result = svc.prepare(&project, &analysis, options.clone(), false, &compiled).unwrap();
    let mut accepted = service::handoff(&project, &result).unwrap();
    assert_eq!(accepted.customization.secondary_motion.as_ref().unwrap().enabled,Some(true));
    assert_eq!(accepted.customization.secondary_motion.as_ref().unwrap().preset.as_deref(),Some("automatic"));
    assert!(accepted.customization.active());
    accepted.meshes.insert("body".into(),MeshMapping::One("appendage geometry".into()));
    accepted.meshes.insert("body_lod1".into(),MeshMapping::One("appendage geometry".into()));
    accepted.meshes.insert("gun".into(),MeshMapping::One("@keep".into()));
    accepted.meshes.insert("unused".into(),MeshMapping::One("@disable".into()));
    accepted.review_lods=true; accepted.textures="C:\\existing\\TangTang".into();
    let material = accepted.materials.keys().next().unwrap().clone();
    let choices: MaterialChoices = serde_json::from_value(json!({"confirmed":true,"textures":{"color":"chosen.png"},"mode":"generate"})).unwrap();
    accepted.materials.insert(material.clone(), choices.clone());
    service::check_handoff(&accepted).unwrap();
    let corrected=svc.prepare(&accepted,&analysis,options.clone(),true,&compiled).unwrap();
    let last=tool.calls.lock().unwrap().last().unwrap().clone();
    assert_eq!(last["target"].as_array().unwrap().len(), target.len()+3);
    assert_eq!(last["options"]["mapping"]["dmb_secondary_tip"],"dmb_secondary_tip");
    assert!(last["options"].get("mapping_anchors").is_none());
    let ready=service::handoff(&accepted,&corrected).unwrap();
    assert_eq!(ready.meshes,accepted.meshes); assert!(ready.review_lods);
    assert_eq!(ready.materials[&material],choices); assert_eq!(ready.textures,accepted.textures);
    assert_eq!(ready.preparation_options,options); service::check_handoff(&ready).unwrap();
    // Repeated corrections continue using the original source review choices.
    svc.prepare(&ready,&analysis,options.clone(),true,&compiled).unwrap();
    let mut changed=ready.clone(); changed.preparation_options.values.insert("physics_roots".into(),json!([]));
    assert!(service::check_handoff(&changed).unwrap_err().to_string().contains("Physics bone choices changed"));
    let mut off=options; off.values.insert("preserve_physics_bones".into(),json!(false));
    assert!(svc.prepare(&ready,&analysis,off,true,&compiled).unwrap_err().to_string().contains("Convert from the original source"));
    accepted.meshes.insert("missing".into(),MeshMapping::One("removed mesh".into()));
    let filtered=service::handoff(&accepted,&corrected).unwrap();
    assert!(!filtered.meshes.contains_key("missing") && !filtered.review_lods);
    assert_eq!(fs::read(source).unwrap(),b"{}");
}
#[test]
fn shape_keys_require_an_explicit_choice_before_conversion_and_keep_it_in_the_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let source = root.join("input.gltf");
    fs::write(&source, b"{}").unwrap();
    let compiled = root.join("hero.vmdl_c");
    fs::write(&compiled, b"original compiled fixture").unwrap();
    let executable = root.join("blender.exe");
    fs::write(&executable, b"fixture").unwrap();
    let mut data = fixture();
    data["inspection"]["morphs"] = json!([{"name":"body/Smile","mesh":"body","shape":"Smile"}]);
    let tool = Arc::new(FixtureTool {data:data.clone(),calls:Mutex::new(vec![]),emit:true});
    let svc = PreparationService::new(root, BlenderBridge {executable, executor:tool.clone()});
    let mut project = Project::default();
    project.original_model = "models/test/hero.vmdl_c".into();
    project.model_input_mode = ModelInputMode::RiggedHumanoid;
    let analysis = svc.inspect(&project, source.to_str().unwrap(), [0.;3], &compiled,
        serde_json::from_value(data["skeleton"].clone()).unwrap()).unwrap();
    let mut options:PreparationOptions=serde_json::from_value(json!({"reviewed":true,"source_roles":analysis.suggestions.source.as_ref().unwrap().roles,"target_roles":analysis.suggestions.target.roles,"mapping":analysis.suggestions.mapping})).unwrap();
    let issues = service::review_issues(&project,&analysis,&options).unwrap();
    assert_eq!(issues.len(),1);
    assert_eq!(issues[0].tab,"shape keys");
    assert!(issues[0].message.contains("Keep the saved shape"));
    let before=tool.calls.lock().unwrap().len();
    for choice in [json!(""),json!("discard"),json!(true)] {
        options.values.insert("shape_key_mode".into(),choice);
        let error=svc.prepare(&project,&analysis,options.clone(),false,&compiled).unwrap_err();
        assert!(error.to_string().contains("before Blender starts"));
        assert_eq!(tool.calls.lock().unwrap().len(),before);
    }
    for choice in ["current_static","basis_static"] {
        options.values.insert("shape_key_mode".into(),json!(choice));
        assert!(service::review_issues(&project,&analysis,&options).unwrap().is_empty());
        let result=svc.prepare(&project,&analysis,options.clone(),false,&compiled).unwrap();
        assert_eq!(tool.calls.lock().unwrap().last().unwrap()["options"]["shape_key_mode"],choice);
        let prepared=service::handoff(&project,&result).unwrap();
        assert_eq!(prepared.preparation_options.values["shape_key_mode"],choice);
        service::check_handoff(&prepared).unwrap();
    }
    // Existing explicit Appearance expression baking remains a valid choice.
    let mut expression=analysis.clone();
    expression.geometry_customization.face=Some(serde_json::from_value(json!({"enabled":true,"mode":"static","weights":{"body/Smile":0.5}})).unwrap());
    options.values.remove("shape_key_mode");
    assert!(service::review_issues(&project,&expression,&options).unwrap().is_empty());
    assert_eq!(fs::read(source).unwrap(),b"{}");
}
#[test]
fn inspect_prepare_handoff_correction_relocation_and_revocation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let source = root.join("input.gltf");
    fs::write(&source, b"{}").unwrap();
    let compiled = root.join("hero.vmdl_c");
    fs::write(&compiled, b"original compiled fixture").unwrap();
    let executable = root.join("blender.exe");
    fs::write(&executable, b"fixture").unwrap();
    let data = fixture();
    let tool = Arc::new(FixtureTool {
        data: data.clone(),
        calls: Mutex::new(vec![]),
        emit: true,
    });
    let timing = deadlock_core::TimingRecorder::new("Preparation qualification");
    let svc = PreparationService {
        project_root: root.clone(),
        cancel: deadlock_core::CancellationToken::new(),
        weapon_reference: None,
        timing: Some(service::PreparationTimings {
            recorder: timing.clone(),
            parent_id: None,
        }),
        bridge: BlenderBridge {
            executable,
            executor: tool.clone(),
        },
    };
    let mut project = Project::default();
    project.original_model = "models/test/hero.vmdl_c".into();
    project.model_input_mode = ModelInputMode::RiggedHumanoid;
    let analysis = svc
        .inspect(
            &project,
            source.to_str().unwrap(),
            [0.; 3],
            &compiled,
            serde_json::from_value(data["skeleton"].clone()).unwrap(),
        )
        .unwrap();
    assert!(Path::new(&analysis.root).join("analysis.json").is_file());
    let options:PreparationOptions=serde_json::from_value(json!({"reviewed":true,"source_roles":analysis.suggestions.source.as_ref().unwrap().roles,"target_roles":analysis.suggestions.target.roles,"mapping":analysis.suggestions.mapping})).unwrap();
    assert!(
        svc.prepare(
            &project,
            &analysis,
            PreparationOptions::default(),
            false,
            &compiled
        )
        .is_err()
    );
    let result = svc
        .prepare(&project, &analysis, options.clone(), false, &compiled)
        .unwrap();
    let prepared = service::handoff(&project, &result).unwrap();
    assert!(prepared.meshes.is_empty());
    assert!(!prepared.review_lods);
    assert!(prepared.material_mapping.is_empty());
    service::check_handoff(&prepared).unwrap();
    let receipt: Value = serde_json::from_slice(&fs::read(&result.report).unwrap()).unwrap();
    assert_eq!(receipt["native_compiler_validated"], false);
    assert_eq!(
        receipt["export_validation"]["target_bones"]["required_names_and_parents_match"],
        true
    );
    assert!(Path::new(&prepared.mesh_template).is_file());
    let corrected = svc
        .prepare(&prepared, &analysis, options, true, &compiled)
        .unwrap();
    let calls = tool.calls.lock().unwrap();
    let job = calls.last().unwrap();
    assert_eq!(job["correction"], true);
    assert_eq!(job["rotation"], json!([0., 0., 0.]));
    assert!(job.get("garment_source").is_none());
    assert!(job.get("props").is_none());
    assert!(job.get("face").is_none());
    assert_eq!(job["options"]["mapping"]["pelvis"], "pelvis");
    drop(calls);
    let ready = service::handoff(&prepared, &corrected).unwrap();
    let report = timing.report();
    for name in [
        "Verify Blender launch",
        "Stage character source and dependencies",
        "Inspect source geometry in Blender",
        "Prepare geometry in Blender",
        "Export and validate prepared DMX",
        "Assemble and verify preparation handoff",
        "Re-export corrected Blender scene",
    ] {
        assert!(
            report
                .steps
                .iter()
                .any(|s| s.name == name && s.status == deadlock_core::TimingStatus::Succeeded),
            "{name}"
        );
    }
    assert!(
        report
            .steps
            .iter()
            .any(|s| s.name == "Validate humanoid review"
                && s.status == deadlock_core::TimingStatus::Failed)
    );

    let profile = root.join("project.json");
    let mut saved = ready.clone();
    saved.save(&profile).unwrap();
    let moved = temp.path().join("moved");
    fs::rename(&root, &moved).unwrap();
    let moved_project = Project::load(&moved.join("project.json")).unwrap();
    let (resumed, _, _) = service::resume(&moved_project).unwrap();
    assert!(Path::new(&resumed.source).is_file());
    assert!(resumed.root.starts_with(moved.to_str().unwrap()));
    service::check_handoff(&moved_project).unwrap();
    fs::write(&moved_project.dmx, b"changed").unwrap();
    assert!(service::check_handoff(&moved_project).is_err());
}
#[test]
fn no_fresh_receipt_cannot_reuse_prior_success() {
    let temp = tempfile::tempdir().unwrap();
    let tool = Arc::new(FixtureTool {
        data: fixture(),
        calls: Mutex::new(vec![]),
        emit: false,
    });
    let bridge = BlenderBridge {
        executable: temp.path().join("blender.exe"),
        executor: tool,
    };
    fs::write(
        temp.path().join("blender-result.json"),
        b"{\"status\":\"ok\"}",
    )
    .unwrap();
    let result: deadlock_preparation::Result<Value> =
        bridge.inspect(Path::new("input.gltf"), temp.path(), [0.; 3], None, None, None);
    assert!(result.is_err());
    assert!(!temp.path().join("blender-result.json").exists());
}
#[test]
fn garment_provenance_allows_identical_move_but_rejects_changed_dependency() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("cloth.bin"), b"cloth vertices").unwrap();
    let source = input.join("cloth.gltf");
    fs::write(&source, b"{\"buffers\":[{\"uri\":\"cloth.bin\"}]}").unwrap();
    let staged =
        source::stage_source(source.to_str().unwrap(), &temp.path().join("stage"), None).unwrap();
    let mut project:Project=serde_json::from_value(json!({"customization":{"enabled":true,"clothing":{"enabled":true,"source":source.to_str().unwrap(),"mode":"weighted"}}})).unwrap();
    let mut record = provenance::clothing_selection(&project).unwrap().unwrap();
    record.source_files = provenance::source_proof(&source, &staged).unwrap();
    let moved = temp.path().join("moved");
    fs::rename(&input, &moved).unwrap();
    project.customization.clothing.as_mut().unwrap().source =
        Some(moved.join("cloth.gltf").to_str().unwrap().into());
    provenance::check_clothing_selection(&project, Some(&record)).unwrap();
    fs::write(moved.join("cloth.bin"), b"modified").unwrap();
    assert!(provenance::check_clothing_selection(&project, Some(&record)).is_err());
}
