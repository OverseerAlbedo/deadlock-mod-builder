//! Replay source authoring on an imported capture, not native compilation.
use deadlock_cloth_authoring::{secondary, surface};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_project::Project;
use deadlock_skeleton::extensions::SourceBinding;
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires DMB_SKIRT_INPUT and DMB_SKIRT_OUTPUT"]
fn captured_source_authoring() {
    let root = PathBuf::from(std::env::var("DMB_SKIRT_INPUT").unwrap());
    let out = PathBuf::from(std::env::var("DMB_SKIRT_OUTPUT").unwrap());
    let project: Project = serde_json::from_slice(&fs::read(root.join("project.json")).unwrap()).unwrap();
    let dmx = Dmx::parse(fs::read(root.join("input/prepared.dmx")).unwrap()).unwrap();
    let identity: serde_json::Value = serde_json::from_slice(&fs::read(root.join("workspace/source-skeleton-identity.json")).unwrap()).unwrap();
    let source: SourceBinding = serde_json::from_value(identity["extension"].clone()).unwrap();
    let setup = secondary::setup(&project, &dmx, Some(&source), &CancellationToken::new()).unwrap().unwrap();
    let proxy = Dmx::parse(setup.proxy.clone()).unwrap();
    assert_eq!(surface::binds(&proxy).unwrap().skeleton, surface::binds(&dmx).unwrap().skeleton);
    if let Ok(expected) = std::env::var("DMB_SKIRT_EXPECTED_GROUPS") {
        assert_eq!(setup.long_skirt.as_ref().map_or(0,|p|p.groups.len()),expected.parse::<usize>().unwrap());
    }
    if let Ok(expected) = std::env::var("DMB_STREAMER_ROOTS") {
        let vertex=proxy.elements.iter().find(|e|e.get("cloth_enable$0").is_some()).unwrap();
        let floats=|name:&str| {
            let a=vertex.attributes.iter().find(|a|a.name==name).unwrap();
            proxy.raw_attribute(a)[4..].chunks_exact(4).map(|v|f32::from_le_bytes(v.try_into().unwrap())).collect::<Vec<_>>()
        };
        let attr=|name:&str|proxy.raw_attribute(vertex.attributes.iter().find(|a|a.name==name).unwrap());
        let slots=i32::from_le_bytes(attr("jointCount").try_into().unwrap()) as usize;
        let joints=attr("blendindices$0")[4..].chunks_exact(4).map(|v|i32::from_le_bytes(v.try_into().unwrap()) as usize).collect::<Vec<_>>();
        let palette=surface::binds(&proxy).unwrap().palette;
        let goals=floats("cloth_goal_strength$0");let tethers=floats("cloth_suspenders$0");let stretch=floats("cloth_stretch$0");
        for root in expected.split(';') {
            let part=setup.parts.iter().find(|p|p.root==root).unwrap();
            assert_eq!(part.kind,deadlock_cloth_authoring::parts::Kind::Streamer);
            let mut checked=0;
            for (i,js) in joints.chunks_exact(slots).enumerate() {
                if part.weighted_chain.contains(&proxy.elements[palette[js[0]]].name) {
                    assert_eq!(tethers[i],1.);assert_eq!(stretch[i],0.);assert!(goals[i]>=0.0599);checked+=1;
                }
            }
            assert_eq!(checked,3*part.weighted_chain.len());
            println!("{root}: {checked} streamer supports have inextensible tethers and faster return");
        }
    }
    if let Ok(path) = std::env::var("DMB_SKIRT_BASELINE_SETUP") {
        let old: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let new = serde_json::to_value(&setup).unwrap();
        for key in ["groups","hair_motion","roll_lock_chains"] {
            assert_eq!(new[key],old[key],"{key}");
        }
        for (part,prior) in new["parts"].as_array().unwrap().iter().zip(old["parts"].as_array().unwrap()) {
            if part["kind"] != "long_skirt" { assert_eq!(part,prior); }
        }
        if setup.long_skirt.is_none() {
            for key in ["proxy_sha256","proxy_positions","dynamic_positions"] {assert_eq!(new[key],old[key]);}
        }
    }
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("proxy.dmx"), &setup.proxy).unwrap();
    fs::write(out.join("setup.json"), serde_json::to_vec_pretty(&setup).unwrap()).unwrap();
    assert_eq!(dmx.bytes(), fs::read(root.join("input/prepared.dmx")).unwrap());
    println!("{} groups, {} positions, proxy {}", setup.groups.len(), setup.proxy_positions, setup.proxy_sha256);
}
