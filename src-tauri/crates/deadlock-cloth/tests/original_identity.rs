//! Synthetic compatibility/corruption gates. Actual Wraith replay is separate.
use deadlock_cloth::generated;
use deadlock_resource::{binary, container, Kind, Value};
use deadlock_skeleton::{aliases, extensions::SourceBinding};
use std::{collections::{BTreeMap, BTreeSet}, fs, path::Path};

fn inputs() -> (Vec<u8>, Vec<u8>, SourceBinding, BTreeSet<String>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/secondary");
    let source: SourceBinding = serde_json::from_slice(&fs::read(root.join("source.json")).unwrap()).unwrap();
    let selected = source.required_extra_names.iter().cloned().collect();
    (fs::read(root.join("original.vmdl_c")).unwrap(), fs::read(root.join("native.vmdl_c")).unwrap(), source, selected)
}
fn edit(raw: &[u8], block_name: &str, mut f: impl FnMut(&mut Value)) -> Vec<u8> {
    let layout = container::layout(raw).unwrap();
    let mut replacements = BTreeMap::new();
    for (i, block) in layout.blocks.iter().enumerate().filter(|(_, b)| b.name == block_name) {
        let mut doc = binary::decode(container::payload(raw, block).unwrap()).unwrap();
        f(&mut doc.root);
        replacements.insert(i, binary::encode(&doc.root, doc.format_guid).unwrap());
    }
    container::rebuild_ordinals(raw, &replacements).unwrap()
}
fn array(v: &mut Value) -> &mut Vec<Value> {
    let Kind::Array(rows) = &mut v.kind else { panic!("array required") };
    rows
}
fn field<'a>(v: &'a mut Value, name: &str) -> &'a mut Value { v.get_mut(name).unwrap() }
fn aliased(native: &[u8], collision: bool) -> Vec<u8> {
    let names = BTreeMap::from([
        ("$cloth_a".into(), "_cloth_a".into()),
        ("$cloth_b".into(), "_cloth_b".into()),
    ]);
    let mut output = native.to_vec();
    for name in ["DATA", "MDAT", "AGRP"] {
        output = edit(&output, name, |v| { aliases::rename_lookup_fields(name, v, &names).unwrap(); });
    }
    edit(&output, "PHYS", |v| {
        let fe = field(v, "m_pFeModel");
        let rows = array(field(fe, "m_CtrlName"));
        rows[1].kind = Kind::String("_cloth_a".into());
        rows[2].kind = Kind::String("_cloth_b".into());
        if collision { rows[5].kind = Kind::String("$cloth_a".into()); }
        // Independent fixed vectors for the fixture's native control names.
        let mut hashes = [3153423456_u32, 2228037438, 964223714, 1043408534, 4196077617,
            1312592778, 1135119594, 3457751635, 637231580, 1729923818, 1017557975];
        if collision { hashes[5] = 1439418812; }
        field(fe, "m_CtrlHash").kind = Kind::Array(hashes.into_iter().map(|h| Value::new(Kind::U64(h as u64))).collect());
    })
}

#[test]
fn original_aliases_and_conflicting_proxy_controls_keep_their_distinct_bindings() {
    let (original, native, source, selected) = inputs();
    for collision in [false, true] {
        let native = aliased(&native, collision);
        assert!(generated::qualify(&original, &native, &source, &selected, &selected)
            .unwrap_err().to_string().contains("missing original identities"));
        let (output, audit) = generated::apply(&original, &native, &source, &selected, &selected).unwrap();
        generated::validate(&original, &native, &output, &source, &selected, &selected).unwrap();
        assert_eq!(audit.original_identity.original_aliases.len(), 2);
        assert_eq!(audit.original_identity.control_renames.len(), if collision { 3 } else { 2 });
        assert_eq!(audit.original_identity.control_renames.iter().filter(|r| r.derived_proxy).count(), usize::from(collision));
        assert!(audit.original_identity.original_binds_and_scales_preserved);
        assert!(audit.original_identity.solver_values_unchanged_except_control_names_and_hashes);
        assert!(audit.original_identity.geometry_and_animation_samples_byte_preserved);
        assert!(audit.bind_repair.corrected_bones.is_empty());
        let before = container::read_blocks(&native, "PHYS").unwrap();
        let after = container::read_blocks(&output, "PHYS").unwrap();
        let old_fe = before[0].root.get("m_pFeModel").unwrap();
        let mut new_fe = after[0].root.get("m_pFeModel").unwrap().clone();
        for key in ["m_CtrlName", "m_CtrlHash"] {
            *field(&mut new_fe, key) = old_fe.get(key).unwrap().clone();
        }
        assert!(new_fe.canonical_eq(old_fe));
    }
}

#[test]
fn no_original_aliases_skip_identity_canonicalization() {
    let (original, native, source, selected) = inputs();
    let (output, audit) = generated::apply(&original, &native, &source, &selected, &selected).unwrap();
    assert!(audit.original_identity.original_aliases.is_empty());
    assert!(audit.original_identity.control_renames.is_empty());
    generated::validate(&original, &native, &output, &source, &selected, &selected).unwrap();
}

#[test]
fn rejects_missing_bones_ambiguous_names_bad_hashes_and_corrupt_original_binds() {
    let (original, native, source, selected) = inputs();
    let native = aliased(&native, true);
    for case in 0..6 {
        let bad = match case {
            0 => edit(&native, "DATA", |v| {
                array(field(field(v, "m_modelSkeleton"), "m_boneName"))[1].kind = Kind::String("truly_missing".into());
            }),
            1 => edit(&native, "DATA", |v| {
                array(field(field(v, "m_modelSkeleton"), "m_boneScaleParent"))[1].kind = Kind::F64(1.5);
            }),
            2 => edit(&native, "PHYS", |v| {
                array(field(field(v, "m_pFeModel"), "m_CtrlHash"))[5].kind = Kind::U64(0);
            }),
            3 => edit(&native, "DATA", |v| {
                let Kind::Object(rows) = &mut v.kind else { panic!() };
                rows.push(("unreviewed_bone_reference".into(), Value::new(Kind::String("_cloth_a".into()))));
            }),
            4 => edit(&native, "MDAT", |v| {
                let bones = array(field(field(v, "m_skeleton"), "m_bones"));
                let bone = bones.iter_mut().find(|b| b.get("m_boneName").unwrap().string().unwrap() == "root").unwrap();
                array(field(bone, "m_invBindPose"))[3].kind = Kind::F64(100.);
            }),
            _ => edit(&native, "PHYS", |v| {
                let Kind::Object(rows) = &mut field(v, "m_pFeModel").kind else { panic!() };
                rows.push(("unreviewed_control_reference".into(), Value::new(Kind::String("$cloth_a".into()))));
            }),
        };
        assert!(generated::apply(&original, &bad, &source, &selected, &selected).is_err(), "{case}");
    }
    let mut stale = source.clone();
    stale.original_sha256 = "0".repeat(64);
    assert!(generated::apply(&original, &native, &stale, &selected, &selected).is_err());
    let mut ambiguous = source.clone();
    let sk = &mut ambiguous.skeleton;
    sk.names.push("_cloth_a".into()); sk.parents.push(0); sk.positions.push([0., 0., 0.]);
    sk.rotations.push([0., 0., 0., 1.]); sk.scales.push(1.); sk.flags.push(64);
    ambiguous.extra_names.push("_cloth_a".into());
    assert!(generated::apply(&original, &native, &ambiguous, &selected, &selected)
        .unwrap_err().to_string().contains("Ambiguous"));
}

#[test]
fn install_replay_rejects_modified_names_hashes_and_solver_settings() {
    let (original, native, source, selected) = inputs();
    let native = aliased(&native, true);
    let (output, _) = generated::apply(&original, &native, &source, &selected, &selected).unwrap();
    for name in ["m_CtrlName", "m_CtrlHash", "m_NodeInvMasses"] {
        let changed = edit(&output, "PHYS", |v| {
            let row = &mut array(field(field(v, "m_pFeModel"), name))[5];
            row.kind = if name == "m_CtrlName" { Kind::String("different_proxy".into()) }
                else { Kind::F64(42.) };
        });
        assert!(generated::validate(&original, &native, &changed, &source, &selected, &selected).is_err(), "{name}");
    }
}
