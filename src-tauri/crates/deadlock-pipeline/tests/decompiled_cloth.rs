//! Newer game files decompile the hero's own cloth into ModelDoc28 sources
//! (Haze's backpack hoses as a Softbody). Builds leave it out and secondary
//! motion must still author its proxy (run with --ignored; DMB_HAZE_SOURCE is
//! the decompiled haze.vmdl).
#[test]
#[ignore = "needs a decompiled hero source"]
fn secondary_motion_authors_on_a_source_whose_cloth_was_decompiled() {
    let Ok(path) = std::env::var("DMB_HAZE_SOURCE") else { return };
    let text = std::fs::read_to_string(path).unwrap();
    let (stripped, removed) = deadlock_model::modeldoc::without_decompiled_cloth(&text).unwrap();
    println!("removed {removed} Softbody node(s); {} -> {} bytes", text.len(), stripped.len());
    assert!(removed >= 1);
    let skeleton = deadlock_skeleton::Skeleton { names: vec![], parents: vec![], positions: vec![], rotations: vec![], scales: vec![], flags: vec![] };
    let analysis = deadlock_cloth_authoring::proxy_source::Analysis { units: deadlock_cloth_authoring::proxy_source::UNITS.into(), millimeters_per_unit: 25.4, leg_colliders: vec![], collision_disabled: true };
    let blocked = deadlock_cloth_authoring::proxy_source::author_direct_proxy(&text, "models/heroes_staging/haze/dmx/dmb_secondary_proxy.dmx", &analysis, &skeleton, "dmb_secondary", true);
    assert!(blocked.is_err(), "the unstripped source is refused");
    let (authored, _) = deadlock_cloth_authoring::proxy_source::author_direct_proxy(&stripped, "models/heroes_staging/haze/dmx/dmb_secondary_proxy.dmx", &analysis, &skeleton, "dmb_secondary", true).unwrap();
    let (authored, _) = deadlock_cloth_authoring::proxy_source::configure_secondary_proxy(&authored, "dmb_secondary_proxy").unwrap();
    assert!(authored.contains("dmb_secondary_proxy") && !authored.contains("\"Softbody\""));
}
