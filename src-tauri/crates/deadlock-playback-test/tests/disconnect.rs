use deadlock_playback_test::{animation::*, disconnect::*, native::*};
use std::collections::{BTreeMap, BTreeSet};

fn model() -> Vec<u8> {
    envelope(
        1,
        &[
            (
                "DATA".into(),
                include_bytes!("../../deadlock-cloth/tests/fixtures/compact-ribbon/data.bin")
                    .to_vec(),
            ),
            (
                "PHYS".into(),
                include_bytes!("../../deadlock-cloth/tests/fixtures/compact-ribbon/phys.bin")
                    .to_vec(),
            ),
            ("MDAT".into(), b"unchanged skinned mesh".to_vec()),
        ],
    )
    .unwrap()
}
#[test]
fn output_removal_alone_leaves_fourteen_bindings_but_r3_disconnects_them() {
    let raw = model();
    let (old_test, _, _) = prepare_model(&raw).unwrap();
    let old_fe = data(&old_test, "PHYS").unwrap();
    let old_fe = field(&old_fe.root, "m_pFeModel").unwrap();
    let targets = targets();
    assert_eq!(
        arr(old_fe, "m_CtrlName")
            .unwrap()
            .iter()
            .filter(|v| targets.contains(v.string().unwrap()))
            .count(),
        14
    );
    assert!(verify_detached(&old_test).is_err());
    let (new, skeleton, receipt) = prepare_primary_model(&raw).unwrap();
    verify_detached(&new).unwrap();
    let after = data(&new, "PHYS").unwrap();
    let fe = field(&after.root, "m_pFeModel").unwrap();
    unchanged_except(old_fe, fe, &["m_CtrlName", "m_CtrlHash"]).unwrap();
    let a = arr(old_fe, "m_CtrlName").unwrap();
    let b = arr(fe, "m_CtrlName").unwrap();
    let ah = arr(old_fe, "m_CtrlHash").unwrap();
    let bh = arr(fe, "m_CtrlHash").unwrap();
    let bone_hashes: BTreeSet<_> = skeleton.names.iter().map(|n| control_hash(n)).collect();
    let mut changes = 0;
    for j in 0..a.len() {
        if targets.contains(a[j].string().unwrap()) {
            changes += 1;
            assert!(!bone_hashes.contains(&(bh[j].integer().unwrap() as u32)));
        } else {
            assert!(a[j].canonical_eq(&b[j]) && ah[j].canonical_eq(&bh[j]));
        }
    }
    assert_eq!(changes, 14);
    assert_eq!(
        receipt["control_binding"]["selected_name_or_hash_bindings_remaining"],
        0
    );
}
#[test]
fn native_hashes_are_case_insensitive_and_unknown_hashes_stop_build() {
    assert_eq!(control_hash("$cloth_m0p0"), 3723911255);
    assert_eq!(control_hash("$CLOTH_M0P1"), 3702510188);
    let raw = model();
    let mut doc = data(&raw, "PHYS").unwrap();
    let fe = doc.root.get_mut("m_pFeModel").unwrap();
    let mut hashes = arr(fe, "m_CtrlHash").unwrap();
    hashes[0] = i(hashes[0].integer().unwrap() as i64 ^ 1);
    set(fe, "m_CtrlHash", list(hashes)).unwrap();
    let bad = rebuild(
        &raw,
        BTreeMap::from([("PHYS".into(), encode(&doc).unwrap())]),
    )
    .unwrap();
    assert!(prepare_primary_model(&bad)
        .unwrap_err()
        .to_string()
        .contains("hash algorithm"));
}
#[test]
fn install_guard_rejects_a_reconnected_control_even_with_disabled_flags() {
    let (raw, _, _) = prepare_primary_model(&model()).unwrap();
    let mut doc = data(&raw, "PHYS").unwrap();
    let fe = doc.root.get_mut("m_pFeModel").unwrap();
    let mut ids = arr(fe, "m_CtrlName").unwrap();
    let mut hashes = arr(fe, "m_CtrlHash").unwrap();
    let j = ids
        .iter()
        .position(|v| v.string().unwrap().starts_with("$dmb_playback_r3_unbound_"))
        .unwrap();
    let name = names().remove(0);
    ids[j] = s(&name);
    hashes[j] = i(control_hash(&name) as i64);
    set(fe, "m_CtrlName", list(ids)).unwrap();
    set(fe, "m_CtrlHash", list(hashes)).unwrap();
    let bad = rebuild(
        &raw,
        BTreeMap::from([("PHYS".into(), encode(&doc).unwrap())]),
    )
    .unwrap();
    assert!(verify_detached(&bad)
        .unwrap_err()
        .to_string()
        .contains("still binds"));
}
