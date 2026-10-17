use deadlock_cloth_authoring::{
    parts::{self, Kind},
    secondary, surface,
};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_project::{customization::SecondaryMotion, Project};
use deadlock_skeleton::extensions::SourceBinding;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
fn binding(d: &Dmx, original: &[u8]) -> SourceBinding {
    let mut sk = surface::binds(d).unwrap().skeleton;
    sk.flags = vec![64; sk.names.len()];
    let extra = sk
        .names
        .iter()
        .filter(|n| n.starts_with("dmb_secondary_"))
        .cloned()
        .collect::<Vec<_>>();
    SourceBinding {
        source_sha256: format!("{:x}", Sha256::digest(d.bytes())),
        original_sha256: format!("{:x}", Sha256::digest(original)),
        skeleton: sk,
        extra_names: extra.clone(),
        required_extra_names: extra,
    }
}
fn project(roots: Vec<String>) -> Project {
    let mut p = Project::default();
    p.original_model = "models/heroes_staging/haze/haze.vmdl".into();
    p.customization.enabled = Some(true);
    p.customization.secondary_motion = Some(SecondaryMotion {
        enabled: Some(true),
        preset: Some("automatic".into()),
        ..Default::default()
    });
    p.preparation_options
        .values
        .insert("preserve_physics_bones".into(), true.into());
    p.preparation_options
        .values
        .insert("physics_roots".into(), serde_json::to_value(roots).unwrap());
    p
}
fn stream(d: &Dmx, key: &str) -> Vec<f32> {
    let e = d
        .elements
        .iter()
        .find(|e| e.kind == "DmeVertexData" && e.get(key).is_some())
        .unwrap();
    let a = e.attributes.iter().find(|a| a.name == key).unwrap();
    d.raw_attribute(a)[4..]
        .chunks_exact(4)
        .map(|a| f32::from_le_bytes(a.try_into().unwrap()))
        .collect()
}
#[test]
fn automatic_parts_have_distinct_motion_and_streamers_have_three_point_ribbon_fits() {
    let d = Dmx::parse(
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/secondary/chain.dmx"))
            .unwrap(),
    )
    .unwrap();
    let source = binding(&d, b"test authority");
    let mut p = project(vec!["garment_root".into()]);
    let mut settings = BTreeMap::new();
    for kind in ["hair", "tail", "ear", "cloth", "streamer", "accessory", "breast", "butt", "charm", "rigid"] {
        p.customization
            .secondary_motion
            .as_mut()
            .unwrap()
            .part_kinds = Some(BTreeMap::from([("garment_root".into(), kind.into())]));
        if kind == "rigid" {
            // A part set to stay still is not simulated at all; with nothing
            // else moving, motion has to be turned off instead.
            let error = secondary::setup(&p, &d, Some(&source), &CancellationToken::new()).unwrap_err();
            assert!(error.to_string().contains("Every moving part is set to stay still"), "{error}");
            continue;
        }
        let setup = secondary::setup(&p, &d, Some(&source), &CancellationToken::new())
            .unwrap()
            .unwrap();
        assert_eq!(setup.parts[0].kind, Kind::parse(kind).unwrap());
        assert_eq!(
            setup.roll_lock_chains.len(),
            usize::from(["streamer", "hair", "tail"].contains(&kind))
        );
        let proxy = Dmx::parse(setup.proxy).unwrap();
        assert_eq!(
            surface::binds(&proxy).unwrap().skeleton,
            surface::binds(&d).unwrap().skeleton
        );
        let enable = stream(&proxy, "cloth_enable$0");
        let goals = stream(&proxy, "cloth_goal_strength$0");
        let gravity = stream(&proxy, "cloth_gravity$0");
        assert_eq!(enable.len(), goals.len());
        assert_eq!(enable.len(), gravity.len());
        let active = enable
            .iter()
            .zip(&gravity)
            .filter(|(v, _)| **v > 0.)
            .map(|(_, g)| *g)
            .collect::<Vec<_>>();
        // A breast group's hanging parts swing like charms (checked below).
        // Gravity is written in the game's unit (1 g = 360, as Valve's own cloth).
        assert!(kind == "breast" || kind == "butt" || active.iter().all(|g| *g
            == if ["hair", "ear", "charm", "rigid"].contains(&kind) {
                0.
            } else {
                360.
            }));
        if kind == "streamer" {
            assert_eq!(
                setup.dynamic_positions,
                3 * setup.groups[0].weighted_bones.len()
            );
            assert!(stream(&proxy, "cloth_stray_radius$0")
                .iter()
                .any(|v| *v > 0.));
            let tethers=stream(&proxy,"cloth_suspenders$0");
            let stretch=stream(&proxy,"cloth_stretch$0");
            let radius=stream(&proxy,"cloth_stray_radius$0");
            for i in 0..enable.len() {if enable[i]>0. {
                assert_eq!(tethers[i],1.);assert_eq!(stretch[i],0.);
                assert!(goals[i]>=0.0599,"Streamers must not use the old 0.02 tip return");
                assert!(radius[i] as f64 <= setup.parts[0].chain_length_inches*0.60+0.451);
            }}
        }
        if kind == "breast" {
            // One breast-sized ring (a springy, lightly damped bounce with bounded
            // travel, no sag); every other bone of the group, such as a ribbon
            // hanging from the breast, still gets its own moving nodes.
            assert_eq!(setup.dynamic_positions, 3 * setup.groups[0].bones.len());
            let drag = stream(&proxy, "cloth_drag$0");
            let radius = stream(&proxy, "cloth_stray_radius$0");
            let breast = (0..enable.len())
                .filter(|i| enable[*i] > 0. && (goals[*i] - 0.45).abs() < 1e-6)
                .collect::<Vec<_>>();
            assert_eq!(breast.len(), 3);
            for &i in &breast {
                assert!(drag[i] <= 0.1 + 1e-6 && gravity[i] == 0. && radius[i] >= 0.4 && radius[i] <= 1.5);
            }
            // Pieces hanging from the breast swing like charms: no gravity (a strip
            // on the chest must not sag through the body) and a bounded swing.
            let hanging = (0..enable.len()).filter(|i| enable[*i] > 0. && !breast.contains(i)).collect::<Vec<_>>();
            assert!(!hanging.is_empty());
            assert!(hanging.iter().all(|&i| gravity[i] == 0. && radius[i] > 0. && radius[i] <= 1.6));
        }
        if kind == "butt" {
            // A small, stiffer bounce than breasts, without gravity, bounded.
            let radius = stream(&proxy, "cloth_stray_radius$0");
            let butt = (0..enable.len()).filter(|i| enable[*i] > 0. && (goals[*i] - 0.6).abs() < 1e-6).collect::<Vec<_>>();
            assert_eq!(butt.len(), 3);
            assert!(butt.iter().all(|&i| gravity[i] == 0. && radius[i] >= 0.25 && radius[i] <= 1.1));
            assert!((0..enable.len()).filter(|i| enable[*i] > 0.).all(|i| gravity[i] == 0.));
        }
        if kind == "charm" {
            // A light, quick-settling swing: the pivot stays on its anchor, the
            // rest swings with damping, no gravity and a small travel limit.
            let drag = stream(&proxy, "cloth_drag$0");
            let radius = stream(&proxy, "cloth_stray_radius$0");
            let swinging = (0..enable.len()).filter(|i| enable[*i] > 0. && goals[*i] < 1.).collect::<Vec<_>>();
            assert!(!swinging.is_empty());
            for &i in &swinging {
                assert!(goals[i] >= 0.3 && goals[i] <= 0.5 + 1e-6 && drag[i] >= 0.3 - 1e-6 && radius[i] >= 0.2 && radius[i] <= 1.6, "{i}");
            }
        }
        settings.insert(kind, goals);
    }
    assert_ne!(settings["hair"], settings["streamer"]);
    assert_ne!(settings["tail"], settings["cloth"]);
    assert_eq!(
        parts::analyze(
            &d,
            &secondary::reviewed_groups(&d, &["garment_root".into()], &source).unwrap(),
            &BTreeMap::new(),
            &CancellationToken::new()
        )
        .unwrap()[0]
            .kind,
        Kind::Cloth
    );
}
#[test]
#[ignore = "requires supplied accepted TangTang DMX, native output and original authority"]
fn accepted_tangtang_automatic_replay() {
    let d = Dmx::parse(fs::read(std::env::var("DMB_AUTO_DMX").unwrap()).unwrap()).unwrap();
    let original = fs::read(std::env::var("DMB_AUTO_ORIGINAL").unwrap()).unwrap();
    let native = fs::read(std::env::var("DMB_AUTO_NATIVE").unwrap()).unwrap();
    let source = binding(&d, &original);
    let roots = source
        .extra_names
        .iter()
        .filter(|n| {
            let i = source.skeleton.indices()[n.as_str()];
            !source
                .extra_names
                .contains(&source.skeleton.names[source.skeleton.parents[i] as usize])
        })
        .map(|n| n.strip_prefix("dmb_secondary_").unwrap().to_owned())
        .collect();
    let p = project(roots);
    let setup = secondary::setup(&p, &d, Some(&source), &CancellationToken::new())
        .unwrap()
        .unwrap();
    assert!(setup.reused_accepted_reference);
    assert_eq!(setup.groups.len(), 38);
    assert_eq!(setup.roll_lock_chains.len(), 2);
    assert_eq!(setup.proxy_sha256, secondary::TANG_REFERENCE_PROXY);
    let selected = setup
        .groups
        .iter()
        .flat_map(|g| g.bones.iter().cloned())
        .collect();
    let weighted = setup
        .groups
        .iter()
        .flat_map(|g| g.weighted_bones.iter().cloned())
        .collect();
    let (out, audit) = deadlock_cloth::generated::apply_with_roll_lock(
        &original,
        &native,
        &source,
        &selected,
        &weighted,
        &setup.roll_lock_chains,
    )
    .unwrap();
    assert_eq!(
        audit
            .roll_lock
            .chains
            .iter()
            .map(|c| c.bones.len())
            .sum::<usize>(),
        14
    );
    assert_eq!(audit.roll_lock.chains[0].reference, [91, 92]);
    assert_eq!(audit.roll_lock.chains[1].reference, [94, 95]);
    let directory = std::env::var("DMB_AUTO_OUT").unwrap();
    fs::create_dir_all(&directory).unwrap();
    fs::write(Path::new(&directory).join("automatic-output.vmdl_c"), &out).unwrap();
    fs::write(
        Path::new(&directory).join("automatic-setup.json"),
        serde_json::to_vec_pretty(&setup).unwrap(),
    )
    .unwrap();
    fs::write(
        Path::new(&directory).join("automatic-audit.json"),
        serde_json::to_vec_pretty(&audit).unwrap(),
    )
    .unwrap();
    fs::write(
        Path::new(&directory).join("source-binding.json"),
        serde_json::to_vec_pretty(&source).unwrap(),
    )
    .unwrap();
    if let Ok(reader)=std::env::var("DMB_AUTO_READER") {
        use deadlock_resource::{container,binary,independent};
        let path=Path::new(&directory).join("automatic-output.vmdl_c");
        let layout=container::layout(&out).unwrap();
        for block in ["DATA","PHYS"] {
            let result=std::process::Command::new(&reader).arg("-i").arg(&path).args(["-b",block]).output().unwrap();
            assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
            let expected=binary::decode(container::payload(&out,container::unique(&layout.blocks,block).unwrap()).unwrap()).unwrap();
            let documents=independent::parse_cli_blocks(&String::from_utf8(result.stdout).unwrap()).unwrap();assert_eq!(documents.len(),1);
            let proof=independent::verify(&documents[0].root,&expected.root).unwrap();
            println!("Independent native reader {block}: {} values verified.",proof.values_checked);
        }
    }
    // Any physics edit after the approved transform must fail installation.
    for field in ["m_NodeBases", "m_NodeInvMasses"] {
        use deadlock_resource::{binary, container, Kind};
        let layout = container::layout(&out).unwrap();
        let block = container::unique(&layout.blocks, "PHYS").unwrap();
        let mut phys = binary::decode(container::payload(&out, block).unwrap()).unwrap();
        let fe = phys.root.get_mut("m_pFeModel").unwrap();
        if let Kind::Array(rows) = &mut fe.get_mut(field).unwrap().kind {
            rows.pop();
        }
        let bad = container::rebuild(
            &out,
            &BTreeMap::from([(
                "PHYS".into(),
                binary::encode(&phys.root, phys.format_guid).unwrap(),
            )]),
        )
        .unwrap();
        assert!(deadlock_cloth::generated::validate_with_roll_lock(
            &original,
            &native,
            &bad,
            &source,
            &selected,
            &weighted,
            &setup.roll_lock_chains
        )
        .is_err());
    }
    // Installation repeats the transform from the original snapshot, rather
    // than trusting any new hash or report generated beside the edited model.
    deadlock_cloth::generated::validate_with_roll_lock(
        &original,
        &native,
        &out,
        &source,
        &selected,
        &weighted,
        &setup.roll_lock_chains,
    )
    .unwrap();
}
#[test]
fn fine_tuning_a_part_type_changes_only_its_moving_nodes() {
    let d = Dmx::parse(fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/secondary/chain.dmx")).unwrap()).unwrap();
    let source = binding(&d, b"test authority");
    let mut p = project(vec!["garment_root".into()]);
    p.customization.secondary_motion.as_mut().unwrap().part_kinds = Some(BTreeMap::from([("garment_root".into(), "streamer".into())]));
    let maps = |p: &Project| {
        let setup = secondary::setup(p, &d, Some(&source), &CancellationToken::new()).unwrap().unwrap();
        let proxy = Dmx::parse(setup.proxy).unwrap();
        ["cloth_enable$0", "cloth_goal_strength$0", "cloth_gravity$0", "cloth_stray_radius$0"].map(|k| stream(&proxy, k))
    };
    let [enable, goal, gravity, travel] = maps(&p);
    // Tuning another part type changes nothing.
    let other = deadlock_project::customization::MotionTuning { stiffness: Some(1.), ..Default::default() };
    p.customization.secondary_motion.as_mut().unwrap().tuning = Some(BTreeMap::from([("hair".into(), other)]));
    assert_eq!(maps(&p), [enable.clone(), goal.clone(), gravity.clone(), travel.clone()]);
    // Streamers: stiffer, no gravity, shorter swing; the fixed anchor stays as it was.
    let tuned = deadlock_project::customization::MotionTuning { stiffness: Some(0.5), gravity: Some(-1.), swing: Some(-1.), ..Default::default() };
    p.customization.secondary_motion.as_mut().unwrap().tuning = Some(BTreeMap::from([("streamer".into(), tuned)]));
    let [enable2, goal2, gravity2, travel2] = maps(&p);
    assert_eq!(enable, enable2);
    for i in 0..enable.len() {
        if enable[i] > 0. {
            assert!(goal2[i] > goal[i] || goal[i] >= 1., "{i}");
            assert_eq!(gravity2[i], 0.);
            assert!((travel2[i] - travel[i] * 0.25).abs() < 1e-4, "{i}");
        } else {
            assert_eq!((goal2[i], gravity2[i], travel2[i]), (goal[i], gravity[i], travel[i]));
        }
    }
}
#[test]
fn source_rig_springs_are_starting_values_only_when_chosen() {
    let d = Dmx::parse(fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/secondary/chain.dmx")).unwrap()).unwrap();
    let source = binding(&d, b"test authority");
    let mut p = project(vec!["garment_root".into()]);
    p.customization.secondary_motion.as_mut().unwrap().part_kinds = Some(BTreeMap::from([("garment_root".into(), "streamer".into())]));
    p.preparation_options.values.insert("physics_springs".into(), serde_json::json!({"garment_root": {"source": "vrm", "goal": 0.33, "drag": 0.21, "gravity": 0.5}}));
    let maps = |p: &Project| {
        let setup = secondary::setup(p, &d, Some(&source), &CancellationToken::new()).unwrap().unwrap();
        let proxy = Dmx::parse(setup.proxy).unwrap();
        (["cloth_enable$0", "cloth_goal_strength$0", "cloth_drag$0", "cloth_gravity$0"].map(|k| stream(&proxy, k)), setup.preview_nodes)
    };
    // Off (the default): the builder's own values.
    let ([enable, goal, _, _], _) = maps(&p);
    assert!(enable.iter().zip(&goal).any(|(e, g)| *e > 0. && (*g - 0.33).abs() > 1e-3));
    p.customization.secondary_motion.as_mut().unwrap().use_source_springs = Some(true);
    let ([enable, goal, drag, gravity], nodes) = maps(&p);
    for i in 0..enable.len() {
        if enable[i] > 0. && goal[i] < 0.999 {
            assert!((goal[i] - 0.33).abs() < 1e-4 && (drag[i] - 0.21).abs() < 1e-4 && (gravity[i] - 0.5 * 360.).abs() < 1e-2, "{i}");
        }
    }
    // The movement preview starts from them too.
    assert!(nodes.iter().any(|n| (n.goal - 0.33).abs() < 1e-4));
}

#[test]
fn parts_set_to_stay_still_get_no_simulated_points() {
    let d = Dmx::parse(
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/secondary/independent.dmx")).unwrap(),
    )
    .unwrap();
    let source = binding(&d, b"test authority");
    let roots: Vec<String> = ["garment_root", "garment_mid", "garment_tip"].map(String::from).to_vec();
    let mut p = project(roots);
    let all = secondary::setup(&p, &d, Some(&source), &CancellationToken::new()).unwrap().unwrap();
    assert!(all.still_roots.is_empty());
    assert_eq!(all.simulated_groups().count(), 3);
    p.customization.secondary_motion.as_mut().unwrap().part_kinds =
        Some(BTreeMap::from([("garment_mid".into(), "rigid".into())]));
    let still = secondary::setup(&p, &d, Some(&source), &CancellationToken::new()).unwrap().unwrap();
    assert_eq!(still.still_roots, vec!["garment_mid".to_string()]);
    assert_eq!(still.groups.len(), 3);
    assert_eq!(
        still.simulated_groups().map(|g| g.source_root.as_str()).collect::<Vec<_>>(),
        ["garment_root", "garment_tip"]
    );
    // The still part's ring and anchor frame are gone; the others are unchanged.
    assert_eq!(still.dynamic_positions * 3, all.dynamic_positions * 2);
    assert_eq!(still.proxy_positions * 3, all.proxy_positions * 2);
    assert!(still.preview_nodes.iter().all(|n| n.root != "garment_mid"));
    assert!(still.roll_lock_chains.iter().all(|c| !c.bones.iter().any(|b| b.contains("garment_mid"))));
}

#[test]
fn the_heros_own_vertex_set_names_are_carried_as_empty_sets() {
    let d = Dmx::parse(
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/secondary/chain.dmx")).unwrap(),
    )
    .unwrap();
    let source = binding(&d, b"test authority");
    let p = project(vec!["garment_root".into()]);
    let sets = vec!["apron_outer_l_vm".to_string(), "pony_tail_vm".to_string()];
    let setup = secondary::setup_with(&p, &d, Some(&source), &sets, &CancellationToken::new()).unwrap().unwrap();
    assert_eq!(setup.hero_vertex_sets, sets);
    let proxy = Dmx::parse(setup.proxy.clone()).unwrap();
    let points = stream(&proxy, "cloth_enable$0").len();
    for set in &sets {
        let values = stream(&proxy, &format!("cloth_vertex_set_{set}$0"));
        assert_eq!(values.len(), points);
        assert!(values.iter().all(|v| *v == 0.), "{set} must name an empty set");
    }
    // Everything else about the proxy is unchanged.
    let plain = secondary::setup(&p, &d, Some(&source), &CancellationToken::new()).unwrap().unwrap();
    assert!(plain.hero_vertex_sets.is_empty());
    assert_eq!(stream(&Dmx::parse(plain.proxy).unwrap(), "cloth_goal_strength$0"), stream(&proxy, "cloth_goal_strength$0"));
    // A name the proxy cannot carry is refused rather than written.
    assert!(secondary::setup_with(&p, &d, Some(&source), &["bad name".into()], &CancellationToken::new()).is_err());
}
