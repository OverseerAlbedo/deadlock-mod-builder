//! Attachments of a compiled model and the game's October 16, 2026 change.
//!
//! A compiled model keeps its attachment list (`m_attachments`) in every mesh
//! data block (`MDAT`): name, influence bone, offset and rotation relative to
//! that bone. With the October 16, 2026 update the game stores attachments
//! parented to the skeleton's root bone (`root_motion`) as if that bone had no
//! rotation: the new offset and rotation are the bone's rest rotation applied
//! to the old ones, while attachments on every other bone are unchanged
//! (checked on Haze, Vindicta, Lady Geist, Paige and Ivy). The engine reads
//! compiled data the new way, so a model compiled before the update has its
//! camera pivots (`standing_pivot`, `crouching_pivot`) rotated by 90 degrees.
//!
//! [`update`] repairs a compiled model against the game's current model of the
//! same hero: standard attachments take the game's values, custom attachments
//! on the root bone are converted by the rule, everything else stays.
use crate::{binary, container, invalid, Kind, Result, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Quat = [f64; 4];
pub type Vec3 = [f64; 3];

/// Offsets closer than this (inches) and quaternion components closer than
/// `ROTATION_TOLERANCE` count as the same attachment.
pub const OFFSET_TOLERANCE: f64 = 1e-3;
pub const ROTATION_TOLERANCE: f64 = 1e-4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    /// The first influence bone (empty for world attachments).
    pub bone: String,
    pub offset: Vec3,
    /// (x, y, z, w)
    pub rotation: Quat,
}

/// How a model stores its root-bone attachments compared with the game's
/// current model of the same hero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Convention {
    /// Same as the game: nothing to do.
    Current,
    /// The convention before October 16, 2026: the root bone's rest rotation
    /// is still baked into every root-bone attachment.
    Previous,
    /// Neither (a custom rig, or a mixture): left alone.
    Unknown,
    /// The model has no attachments shared with the game's model.
    NoAttachments,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Update {
    pub bytes: Vec<u8>,
    /// Attachments replaced by the game's values.
    pub taken_from_game: usize,
    /// Custom root-bone attachments converted by the rule.
    pub converted: Vec<String>,
    /// Mesh data blocks rewritten.
    pub blocks: usize,
}

fn qmul(a: Quat, b: Quat) -> Quat {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}
fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let r = qmul(qmul(q, [v[0], v[1], v[2], 0.]), [-q[0], -q[1], -q[2], q[3]]);
    [r[0], r[1], r[2]]
}
fn same_offset(a: Vec3, b: Vec3) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= OFFSET_TOLERANCE)
}
/// q and -q are the same rotation.
fn same_rotation(a: Quat, b: Quat) -> bool {
    [1., -1.].into_iter().any(|s| (0..4).all(|k| (a[k] * s - b[k]).abs() <= ROTATION_TOLERANCE))
}

fn numbers(v: &Value) -> Result<Vec<f64>> {
    v.array()?.iter().map(|n| n.number()).collect()
}
/// The attachment object of a list entry (`{key, value}` or the object itself).
fn entry_value(entry: &Value) -> &Value {
    entry.get("value").unwrap_or(entry)
}
fn entry_value_mut(entry: &mut Value) -> &mut Value {
    if entry.get("value").is_some() {
        entry.get_mut("value").unwrap()
    } else {
        entry
    }
}
fn parse(value: &Value) -> Result<Attachment> {
    let name = value.get("m_name").ok_or_else(|| invalid("attachment without m_name"))?.string()?.to_string();
    let bone = value
        .get("m_influenceNames")
        .and_then(|v| v.array().ok())
        .and_then(|a| a.first())
        .and_then(|v| v.string().ok())
        .unwrap_or_default()
        .to_string();
    let offsets = value.get("m_vInfluenceOffsets").ok_or_else(|| invalid("attachment without offsets"))?.array()?;
    let rotations = value.get("m_vInfluenceRotations").ok_or_else(|| invalid("attachment without rotations"))?.array()?;
    let o = numbers(offsets.first().ok_or_else(|| invalid("attachment without an influence"))?)?;
    let r = numbers(rotations.first().ok_or_else(|| invalid("attachment without an influence"))?)?;
    if o.len() != 3 || r.len() != 4 {
        return Err(invalid(format!("attachment {name}: malformed influence transform")));
    }
    Ok(Attachment { name, bone, offset: [o[0], o[1], o[2]], rotation: [r[0], r[1], r[2], r[3]] })
}

/// Ordinals of the MDAT blocks and their decoded documents.
fn mesh_blocks(raw: &[u8]) -> Result<Vec<(usize, binary::Document)>> {
    let layout = container::layout(raw)?;
    let mut out = vec![];
    for (ordinal, block) in layout.blocks.iter().enumerate() {
        if block.name == "MDAT" {
            let doc = binary::decode(container::payload(raw, block)?)?;
            out.push((ordinal, doc));
        }
    }
    Ok(out)
}

/// The attachments of the first mesh data block (every block carries the same list).
pub fn list(raw: &[u8]) -> Result<Vec<Attachment>> {
    let Some((_, doc)) = mesh_blocks(raw)?.into_iter().next() else { return Ok(vec![]) };
    let Some(entries) = doc.root.get("m_attachments").and_then(|v| v.array().ok()) else { return Ok(vec![]) };
    entries.iter().map(|e| parse(entry_value(e))).collect()
}

/// The skeleton's root bones (no parent) with their rest rotations; the first
/// is `root_motion` when the skeleton has it.
pub fn root_bones(raw: &[u8]) -> Result<Vec<(String, Quat)>> {
    let data = container::read_blocks(raw, "DATA")?;
    let Some(skeleton) = data.first().and_then(|d| d.root.get("m_modelSkeleton")) else { return Ok(vec![]) };
    let names = skeleton.get("m_boneName").ok_or_else(|| invalid("skeleton without bone names"))?.array()?;
    let parents = skeleton.get("m_nParent").ok_or_else(|| invalid("skeleton without parents"))?.array()?;
    let rotations = skeleton.get("m_boneRotParent").ok_or_else(|| invalid("skeleton without rotations"))?.array()?;
    let mut roots = vec![];
    for i in 0..names.len().min(parents.len()).min(rotations.len()) {
        if parents[i].integer()? < 0 {
            let q = numbers(&rotations[i])?;
            if q.len() == 4 {
                roots.push((names[i].string()?.to_string(), [q[0], q[1], q[2], q[3]]));
            }
        }
    }
    roots.sort_by_key(|(name, _)| name != "root_motion");
    Ok(roots)
}

/// How `model` stores its root-bone attachments, judged against `game`
/// (the game's current compiled model of the same hero).
pub fn convention(model: &[u8], game: &[u8]) -> Result<Convention> {
    let roots = root_bones(model)?;
    let mine = list(model)?;
    let theirs = list(game)?;
    // Per root-bone attachment shared with the game: current, previous or neither.
    let classify = |a: &Attachment| -> Option<Option<bool>> {
        let (_, rest) = roots.iter().find(|(name, _)| *name == a.bone)?;
        let b = theirs.iter().find(|b| b.name == a.name && b.bone == a.bone)?;
        if same_offset(a.offset, b.offset) && same_rotation(a.rotation, b.rotation) {
            Some(Some(true))
        } else if same_offset(rotate(*rest, a.offset), b.offset) && same_rotation(qmul(*rest, a.rotation), b.rotation) {
            Some(Some(false))
        } else {
            Some(None)
        }
    };
    // The camera pivots decide when both are there: other root attachments
    // (aim, zipline) also move between game patches for unrelated reasons.
    let pivots: Vec<Option<bool>> = ["standing_pivot", "crouching_pivot"]
        .iter()
        .filter_map(|name| mine.iter().find(|a| a.name == *name).and_then(&classify))
        .collect();
    if pivots.len() == 2 {
        return Ok(match (pivots[0], pivots[1]) {
            (Some(true), Some(true)) => Convention::Current,
            (Some(false), Some(false)) => Convention::Previous,
            _ => Convention::Unknown,
        });
    }
    let (mut current, mut previous, mut other) = (0, 0, 0);
    for verdict in mine.iter().filter_map(&classify) {
        match verdict {
            Some(true) => current += 1,
            Some(false) => previous += 1,
            None => other += 1,
        }
    }
    Ok(match (current, previous, other) {
        (0, 0, 0) => Convention::NoAttachments,
        (_, 0, _) if current > 0 => Convention::Current,
        (0, _, _) if previous > 0 => Convention::Previous,
        _ => Convention::Unknown,
    })
}

/// Rewrites a transform array in place, keeping its number kind.
fn write_numbers(target: &mut Value, values: &[f64]) -> Result<()> {
    let Kind::Array(items) = &mut target.kind else { return Err(invalid("transform is not an array")) };
    if items.len() != values.len() {
        return Err(invalid("transform length changed"));
    }
    for (item, value) in items.iter_mut().zip(values) {
        item.kind = match item.kind {
            Kind::F64(_) => Kind::F64(*value),
            _ => Kind::F32(*value as f32),
        };
    }
    Ok(())
}
fn convert_entry(value: &mut Value, rest: Quat) -> Result<()> {
    let a = parse(value)?;
    let offset = rotate(rest, a.offset);
    let rotation = qmul(rest, a.rotation);
    let offsets = value.get_mut("m_vInfluenceOffsets").ok_or_else(|| invalid("attachment without offsets"))?;
    let Kind::Array(items) = &mut offsets.kind else { return Err(invalid("offsets are not an array")) };
    write_numbers(&mut items[0], &offset)?;
    let rotations = value.get_mut("m_vInfluenceRotations").ok_or_else(|| invalid("attachment without rotations"))?;
    let Kind::Array(items) = &mut rotations.kind else { return Err(invalid("rotations are not an array")) };
    write_numbers(&mut items[0], &rotation)
}

/// Brings `model`'s attachments to the game's convention using `game`, the
/// game's current compiled model of the same hero. With `only_previous` the
/// model is left alone unless it clearly uses the previous convention (for
/// installed mods); without it every attachment the game also has takes the
/// game's value (for the app's own builds). `None` when nothing changes.
pub fn update(model: &[u8], game: &[u8], only_previous: bool) -> Result<Option<Update>> {
    let convention = convention(model, game)?;
    if only_previous && convention != Convention::Previous {
        return Ok(None);
    }
    let roots = root_bones(model)?;
    let theirs: BTreeMap<String, &Value> = {
        let mut map = BTreeMap::new();
        if let Some((_, doc)) = mesh_blocks(game)?.into_iter().next() {
            if let Some(entries) = doc.root.get("m_attachments").and_then(|v| v.array().ok()) {
                // Leaked on purpose: the game's values are copied into every block below.
                let entries: &'static [Value] = Box::leak(entries.to_vec().into_boxed_slice());
                for entry in entries {
                    let value = entry_value(entry);
                    if let Some(name) = value.get("m_name").and_then(|v| v.string().ok()) {
                        map.insert(name.to_string(), value);
                    }
                }
            }
        }
        map
    };
    let mut replacements = BTreeMap::new();
    let (mut taken, mut converted, mut changed) = (0usize, Vec::new(), false);
    let blocks = mesh_blocks(model)?;
    let count = blocks.len();
    for (ordinal, mut doc) in blocks {
        let (mut taken_here, mut converted_here) = (0usize, Vec::new());
        if let Some(Kind::Array(entries)) = doc.root.get_mut("m_attachments").map(|v| &mut v.kind) {
            for entry in entries.iter_mut() {
                let value = entry_value_mut(entry);
                let before = value.clone();
                let a = parse(value)?;
                if let Some(game_value) = theirs.get(&a.name) {
                    let game_attachment = parse(game_value)?;
                    if game_attachment.bone == a.bone {
                        *value = (*game_value).clone();
                        taken_here += 1;
                    }
                } else if convention == Convention::Previous {
                    if let Some((_, rest)) = roots.iter().find(|(name, _)| *name == a.bone) {
                        convert_entry(value, *rest)?;
                        converted_here.push(a.name.clone());
                    }
                }
                changed |= !value.canonical_eq(&before);
            }
        }
        taken = taken_here;
        converted = converted_here;
        replacements.insert(ordinal, binary::encode(&doc.root, doc.format_guid)?);
    }
    if !changed {
        return Ok(None);
    }
    Ok(Some(Update { bytes: container::rebuild_ordinals(model, &replacements)?, taken_from_game: taken, converted, blocks: count }))
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &[u8] = include_bytes!("../../deadlock-cloth/tests/fixtures/tested-original.vmdl_c");
    const ROOT: Quat = [0.5, 0.5, 0.5, 0.5];

    fn f32s(values: &[f64]) -> Value {
        Value::new(Kind::Array(values.iter().map(|v| Value::new(Kind::F32(*v as f32))).collect()))
    }
    fn attachment(name: &str, bone: &str, offset: Vec3, rotation: Quat) -> Value {
        let value = Value::new(Kind::Object(vec![
            ("m_name".into(), Value::new(Kind::String(name.into()))),
            ("m_influenceNames".into(), Value::new(Kind::Array([bone, "", ""].iter().map(|b| Value::new(Kind::String(b.to_string()))).collect()))),
            ("m_vInfluenceRotations".into(), Value::new(Kind::Array(vec![f32s(&rotation), f32s(&[0., 0., 0., 1.]), f32s(&[0., 0., 0., 1.])]))),
            ("m_vInfluenceOffsets".into(), Value::new(Kind::Array(vec![f32s(&offset), f32s(&[0.; 3]), f32s(&[0.; 3])]))),
            ("m_influenceWeights".into(), f32s(&[1., 0., 0.])),
            ("m_nInfluences".into(), Value::new(Kind::I64(1))),
            ("m_bIgnoreRotation".into(), Value::new(Kind::Bool(false))),
        ]));
        Value::new(Kind::Object(vec![("key".into(), Value::new(Kind::String(name.into()))), ("value".into(), value)]))
    }
    /// The fixture with a two-bone skeleton and the given attachments in every MDAT block.
    fn model(attachments: Vec<Value>) -> Vec<u8> {
        let layout = container::layout(FIXTURE).unwrap();
        let mut replacements = BTreeMap::new();
        for (ordinal, block) in layout.blocks.iter().enumerate() {
            let mut doc = binary::decode(container::payload(FIXTURE, block).unwrap()).unwrap();
            match block.name.as_str() {
                "DATA" => {
                    let skeleton = Value::new(Kind::Object(vec![
                        ("m_boneName".into(), Value::new(Kind::Array(["root_motion", "spine_0"].iter().map(|n| Value::new(Kind::String(n.to_string()))).collect()))),
                        ("m_nParent".into(), Value::new(Kind::Array(vec![Value::new(Kind::I64(-1)), Value::new(Kind::I64(0))]))),
                        ("m_bonePosParent".into(), Value::new(Kind::Array(vec![f32s(&[0.; 3]), f32s(&[0., 40., 0.])]))),
                        ("m_boneRotParent".into(), Value::new(Kind::Array(vec![f32s(&ROOT), f32s(&[0., 0., 0.2, 0.98])]))),
                    ]));
                    match doc.root.get_mut("m_modelSkeleton") {
                        Some(existing) => *existing = skeleton,
                        None => {
                            let Kind::Object(fields) = &mut doc.root.kind else { panic!() };
                            fields.push(("m_modelSkeleton".into(), skeleton));
                        }
                    }
                }
                "MDAT" => {
                    let Kind::Object(fields) = &mut doc.root.kind else { panic!() };
                    fields.retain(|(k, _)| k != "m_attachments");
                    fields.push(("m_attachments".into(), Value::new(Kind::Array(attachments.clone()))));
                }
                _ => continue,
            }
            replacements.insert(ordinal, binary::encode(&doc.root, doc.format_guid).unwrap());
        }
        container::rebuild_ordinals(FIXTURE, &replacements).unwrap()
    }
    const OLD_ROT: Quat = [-0.5, -0.5, -0.5, 0.5];
    fn previous(custom: bool) -> Vec<u8> {
        let mut list = vec![
            attachment("standing_pivot", "root_motion", [0., 86.25, 0.], OLD_ROT),
            attachment("crouching_pivot", "root_motion", [0., 57.5, 0.], OLD_ROT),
            attachment("mask", "spine_0", [3.5, 8., 0.], [0., 0., 0., 1.]),
        ];
        if custom {
            list.push(attachment("my_camera", "root_motion", [10., 120., -5.], OLD_ROT));
            list.push(attachment("my_fx", "spine_0", [1., 2., 3.], [0., 0., 0., 1.]));
        }
        model(list)
    }
    fn current() -> Vec<u8> {
        model(vec![
            attachment("standing_pivot", "root_motion", [0., 0., 86.25], [0., 0., 0., 1.]),
            attachment("crouching_pivot", "root_motion", [0., 0., 57.5], [0., 0., 0., 1.]),
            attachment("mask", "spine_0", [3.5, 8., 0.], [0., 0., 0., 1.]),
        ])
    }

    #[test]
    fn lists_attachments_and_root_bones() {
        let names: Vec<String> = list(&current()).unwrap().into_iter().map(|a| a.name).collect();
        assert_eq!(names, ["standing_pivot", "crouching_pivot", "mask"]);
        assert_eq!(root_bones(&current()).unwrap(), vec![("root_motion".to_string(), ROOT)]);
        assert!(list(FIXTURE).unwrap().is_empty());
    }
    #[test]
    fn conventions_are_told_apart() {
        let game = current();
        assert_eq!(convention(&previous(false), &game).unwrap(), Convention::Previous);
        assert_eq!(convention(&previous(true), &game).unwrap(), Convention::Previous);
        assert_eq!(convention(&current(), &game).unwrap(), Convention::Current);
        let odd = model(vec![attachment("standing_pivot", "root_motion", [5., 5., 5.], [0., 0., 0., 1.])]);
        assert_eq!(convention(&odd, &game).unwrap(), Convention::Unknown);
        assert_eq!(convention(FIXTURE, &game).unwrap(), Convention::NoAttachments);
        // Third-party mods: the pivots are old while `aim` or `zipline` differ for
        // other reasons (moved by a game patch since the mod was built).
        let game_with_aim = model(vec![
            attachment("standing_pivot", "root_motion", [0., 0., 86.25], [0., 0., 0., 1.]),
            attachment("crouching_pivot", "root_motion", [0., 0., 57.5], [0., 0., 0., 1.]),
            attachment("aim", "root_motion", [0., 0., 63.87], [0., 0., 0., 1.]),
        ]);
        let patched_elsewhere = model(vec![
            attachment("standing_pivot", "root_motion", [0., 86.25, 0.], OLD_ROT),
            attachment("crouching_pivot", "root_motion", [0., 57.5, 0.], OLD_ROT),
            attachment("aim", "root_motion", [-1.24, 93.6, -6.63], [0., 0.707, 0., 0.707]),
        ]);
        assert_eq!(convention(&patched_elsewhere, &game_with_aim).unwrap(), Convention::Previous);
        let after = update(&patched_elsewhere, &game_with_aim, true).unwrap().unwrap();
        assert_eq!(after.taken_from_game, 3);
        assert_eq!(convention(&after.bytes, &game_with_aim).unwrap(), Convention::Current);
        // One pivot current, one previous: nobody knows what happened; left alone.
        let mixed = model(vec![
            attachment("standing_pivot", "root_motion", [0., 0., 86.25], [0., 0., 0., 1.]),
            attachment("crouching_pivot", "root_motion", [0., 57.5, 0.], OLD_ROT),
        ]);
        assert_eq!(convention(&mixed, &game).unwrap(), Convention::Unknown);
    }
    #[test]
    fn previous_models_take_the_game_values_and_custom_root_attachments_follow_the_rule() {
        let game = current();
        let update = update(&previous(true), &game, true).unwrap().expect("changed");
        assert_eq!((update.taken_from_game, update.blocks), (3, 2));
        assert_eq!(update.converted, ["my_camera"]);
        let after = list(&update.bytes).unwrap();
        let expected = list(&game).unwrap();
        for a in &expected {
            let b = after.iter().find(|b| b.name == a.name).unwrap();
            assert!(same_offset(a.offset, b.offset) && same_rotation(a.rotation, b.rotation), "{}", a.name);
        }
        let camera = after.iter().find(|a| a.name == "my_camera").unwrap();
        assert!(same_offset(camera.offset, [-5., 10., 120.]), "{:?}", camera.offset);
        assert!(same_rotation(camera.rotation, [0., 0., 0., 1.]), "{:?}", camera.rotation);
        let fx = after.iter().find(|a| a.name == "my_fx").unwrap();
        assert_eq!((fx.offset, fx.rotation), ([1., 2., 3.], [0., 0., 0., 1.]));
        // Every block carries the same list, the result is current, and a second update is a no-op.
        for (_, doc) in mesh_blocks(&update.bytes).unwrap() {
            assert_eq!(doc.root.get("m_attachments").unwrap().array().unwrap().len(), 5);
        }
        assert_eq!(convention(&update.bytes, &game).unwrap(), Convention::Current);
        assert!(update_again(&update.bytes, &game).is_none());
    }
    fn update_again(model: &[u8], game: &[u8]) -> Option<Update> {
        update(model, game, true).unwrap()
    }
    #[test]
    fn current_and_unknown_models_are_left_alone_unless_forced() {
        let game = current();
        assert!(update(&current(), &game, true).unwrap().is_none());
        let odd = model(vec![attachment("standing_pivot", "root_motion", [5., 5., 5.], [0., 0., 0., 1.])]);
        assert!(update(&odd, &game, true).unwrap().is_none());
        // The build's safety net takes the game's values even then.
        let forced = update(&odd, &game, false).unwrap().expect("changed");
        assert_eq!(forced.taken_from_game, 1);
        assert_eq!(list(&forced.bytes).unwrap()[0].offset, list(&game).unwrap()[0].offset);
        assert!(update(&current(), &game, false).unwrap().is_none());
    }
}
