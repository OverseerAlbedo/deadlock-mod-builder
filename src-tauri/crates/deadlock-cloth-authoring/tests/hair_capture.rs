//! Capture source authoring from a real import. Native compilation is separate.
use deadlock_cloth_authoring::{secondary, surface};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_project::Project;
use deadlock_skeleton::extensions::SourceBinding;
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires DMB_HAIR_INPUT and DMB_HAIR_OUTPUT"]
fn captured_hair_source_authoring() {
    let root = PathBuf::from(std::env::var("DMB_HAIR_INPUT").unwrap());
    let output = PathBuf::from(std::env::var("DMB_HAIR_OUTPUT").unwrap());
    let p: Project = serde_json::from_slice(&fs::read(root.join("project.json")).unwrap()).unwrap();
    let raw = fs::read(root.join("input/prepared.dmx")).unwrap();
    let d = Dmx::parse(raw.clone()).unwrap();
    let identity: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("workspace/source-skeleton-identity.json")).unwrap(),
    )
    .unwrap();
    let source: SourceBinding = serde_json::from_value(identity["extension"].clone()).unwrap();
    let setup = secondary::setup(&p, &d, Some(&source), &CancellationToken::new())
        .unwrap()
        .unwrap();
    let proxy = Dmx::parse(setup.proxy.clone()).unwrap();
    assert_eq!(
        surface::binds(&proxy).unwrap().skeleton,
        surface::binds(&d).unwrap().skeleton
    );
    assert_eq!(raw, d.bytes());
    assert_eq!(setup.groups.len(), 23);
    assert_eq!(setup.roll_lock_chains.len(), 14);
    assert!(setup.requires_animation_limits());
    // Old compiler output has no travel limits. The new build/install gate
    // must reject it instead of claiming the new protection was compiled.
    let old_native = fs::read(fs::read_dir(root.join("native")).unwrap().next().unwrap().unwrap().path()).unwrap();
    assert!(deadlock_cloth::generated::require_animation_limit_records(&old_native).is_err());
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("proxy.dmx"), &setup.proxy).unwrap();
    fs::write(
        output.join("setup.json"),
        serde_json::to_vec_pretty(&setup).unwrap(),
    )
    .unwrap();
}
