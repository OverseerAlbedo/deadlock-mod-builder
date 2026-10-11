use deadlock_cloth::{restore, validate_motion};
use deadlock_resource::{binary, container, Kind, Value};
use deadlock_skeleton::extensions::SourceBinding;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn edit(raw: &[u8], mut mutate: impl FnMut(&str, &mut Value)) -> Vec<u8> {
    let layout = container::layout(raw).unwrap();
    let mut changes = BTreeMap::new();
    for (i, b) in layout.blocks.iter().enumerate() {
        if !["DATA", "MDAT", "AGRP"].contains(&b.name.as_str()) {
            continue;
        }
        let mut doc = binary::decode(container::payload(raw, b).unwrap()).unwrap();
        mutate(&b.name, &mut doc.root);
        changes.insert(i, binary::encode(&doc.root, doc.format_guid).unwrap());
    }
    container::rebuild_ordinals(raw, &changes).unwrap()
}
fn rename(v: &mut Value, from: &str, to: &str) {
    match &mut v.kind {
        Kind::String(s) if s == from => *s = to.into(),
        Kind::Array(rows) => {
            for v in rows {
                rename(v, from, to);
            }
        }
        Kind::Object(rows) => {
            for (_, v) in rows {
                rename(v, from, to);
            }
        }
        _ => (),
    }
}
fn inputs() -> (Vec<u8>, Vec<u8>, SourceBinding) {
    let original = fixture("extended-original.vmdl_c");
    let mut candidate = fixture("extended-candidate.vmdl_c");
    let mut source: SourceBinding =
        serde_json::from_slice(&fixture("extended-source.json")).unwrap();
    for (i, (canonical, compiled)) in [
        ("cloth-panel.L", "cloth_panel_L"),
        ("helper.001", "helper_001"),
    ]
    .iter()
    .enumerate()
    {
        let before = source.extra_names[i].clone();
        candidate = edit(&candidate, |_, v| rename(v, &before, compiled));
        for n in source
            .skeleton
            .names
            .iter_mut()
            .chain(source.extra_names.iter_mut())
            .chain(source.required_extra_names.iter_mut())
        {
            if *n == before {
                *n = canonical.to_string();
            }
        }
    }
    source.original_sha256 = digest(&original);
    (original, candidate, source)
}
fn shifted(raw: &[u8], name: &str, shift: f64) -> Vec<u8> {
    edit(raw, |block, v| {
        if block != "MDAT" {
            return;
        }
        if let Kind::Array(bones) = &mut v
            .get_mut("m_skeleton")
            .unwrap()
            .get_mut("m_bones")
            .unwrap()
            .kind
        {
            for bone in bones {
                if bone.get("m_boneName").unwrap().string().unwrap() != name {
                    continue;
                }
                if let Kind::Array(inv) = &mut bone.get_mut("m_invBindPose").unwrap().kind {
                    let current = inv[3].number().unwrap();
                    inv[3].kind = Kind::F64(current + shift);
                }
            }
        }
    })
}
#[test]
fn compiler_spellings_restore_only_with_source_authority_and_preserve_numbers() {
    let (original, candidate, source) = inputs();
    let (fixed, receipt) = restore(&original, &candidate, Some(&source)).unwrap();
    let names = receipt.bone_name_restoration.unwrap();
    assert_eq!(names.alias_count, 2);
    assert!(names.numeric_data_unchanged && names.names_only);
    assert!(names.geometry_payloads_unchanged && names.encoded_animation_samples_unchanged);
    assert_eq!(names.source_binding.unwrap().source_bones_checked.len(), 2);
    assert!(
        validate_motion(&original, &fixed, Some(&source))
            .unwrap()
            .passed
    );
    assert_eq!(restore(&original, &fixed, Some(&source)).unwrap().0, fixed);
    let (unqualified, receipt) = restore(&original, &candidate, None).unwrap();
    assert!(receipt.bone_name_restoration.is_none());
    assert!(validate_motion(&original, &unqualified, Some(&source)).is_err());
    let mut stale = source.clone();
    stale.original_sha256 = "0".repeat(64);
    assert!(restore(&original, &candidate, Some(&stale)).is_err());
    let mut changed = source.clone();
    let i = changed
        .skeleton
        .names
        .iter()
        .position(|n| n == "helper.001")
        .unwrap();
    changed.skeleton.positions[i][0] += 0.01;
    assert!(restore(&original, &candidate, Some(&changed)).is_err());
    let mut changed = source.clone();
    changed.skeleton.parents[i] = 0;
    assert!(restore(&original, &candidate, Some(&changed)).is_err());
    let mut collision = source.clone();
    let i = collision
        .skeleton
        .names
        .iter()
        .position(|n| n == "cloth-panel.L")
        .unwrap();
    collision.skeleton.names.push("cloth.panel-L".into());
    collision
        .skeleton
        .parents
        .push(collision.skeleton.parents[i]);
    collision
        .skeleton
        .positions
        .push(collision.skeleton.positions[i]);
    collision
        .skeleton
        .rotations
        .push(collision.skeleton.rotations[i]);
    collision.skeleton.scales.push(collision.skeleton.scales[i]);
    collision.skeleton.flags.push(collision.skeleton.flags[i]);
    collision.extra_names.push("cloth.panel-L".into());
    assert!(restore(&original, &candidate, Some(&collision))
        .unwrap_err()
        .to_string()
        .contains("Ambiguous compiler spelling"));
}
#[test]
fn compiled_drift_budget_requires_source_and_never_relaxes_original_bones() {
    let (original, candidate, source) = inputs();
    let small = shifted(&candidate, "helper_001", 0.002);
    let (fixed, _) = restore(&original, &small, Some(&source)).unwrap();
    let proof = validate_motion(&original, &fixed, Some(&source))
        .unwrap()
        .skeleton_extension
        .unwrap();
    let bounds = proof.compiled_bindings.inverse_bind_audit;
    assert_eq!(bounds.translation_component_limit_inches, 0.001);
    assert_eq!(bounds.source_qualified_custom_bones.unwrap().names.len(), 2);
    assert!(restore(&original, &small, None).is_err());
    assert!(restore(
        &original,
        &shifted(&candidate, "helper_001", 0.03),
        Some(&source)
    )
    .is_err());
    let original_name = &source.skeleton.names[0];
    assert!(restore(
        &original,
        &shifted(&candidate, original_name, 0.002),
        Some(&source)
    )
    .is_err());
}
