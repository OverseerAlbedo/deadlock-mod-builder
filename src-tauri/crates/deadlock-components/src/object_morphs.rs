//! Objects whose own shape the game changes while it plays (`Switches::without_morphs`).
//!
//! Some ability objects hold a morph block (`MRPH`): flex controllers, the rules
//! that mix stored shapes, and per-vertex shape data in a morph texture
//! (`…_vmorf.vtex`, named by the mesh's `CTRL embedded_meshes[].morph_texture`
//! and, in the newer layout, by `MRPH m_pTextureAtlas`). The model's own
//! animation turns the controllers over time: a jar or a gravestone breaking
//! apart, a bat flapping its wings, ice growing in.
//!
//! The shape data is per vertex of the game's own mesh, so a replacement can
//! never carry it: the user's model is rebuilt WITHOUT any morph data and simply
//! keeps its own shape while the game plays the shape change. Everything else
//! stays under the existing rules (the game's own animation blocks kept byte for
//! byte, bones followed as for any object with bones). The rule is a fact read
//! from the game's file (`left_out`, `shape_animation`), never a name.
//!
//! The read-back (`allowance`) lets through exactly what the absent morph data
//! changes and nothing else: the game's file has an `MRPH` block and the rebuilt
//! one has none; the game's reference table names the morph textures and the
//! rebuilt one does not. The other rows a morph block touches are render-mesh rows
//! the read-back already leaves out for every object (`mesh_row`: `CTRL`,
//! `MDAT m_morphSet`), and the animation blocks that name the morph channels are
//! the game's own, compared by their bytes.
use crate::ability_objects::{ModelFacts, flatten};
use deadlock_core::text::casefold;
use deadlock_resource::container;
use std::collections::BTreeSet;

/// The kept-list line of an object whose shape the game changes (the page shows it as it is).
pub const KEPT_SHAPE: &str = "While its animation plays, the game changes the shape of its own object; your object keeps its own shape";

/// The block a morph-bearing model holds.
pub const MORPH_BLOCK: &str = "MRPH";
/// The channel class of a morph controller in an animation decode key.
pub const MORPH_CHANNEL: &str = "MorphChannel";

/// Whether the model's animation decode keys (`AGRP m_decodeKey`) name no bone and
/// only morph channels (every `m_dataChannelArray[].m_szChannelClass` is
/// `MorphChannel`, and there is at least one): its animation can change nothing
/// but the shape the morph block stores.
pub fn morph_channels_only(raw: &[u8]) -> bool {
    let groups = container::read_blocks(raw, "AGRP").unwrap_or_default();
    let mut channels = 0;
    for group in &groups {
        let Some(key) = group.root.get("m_decodeKey") else { return false };
        if key.get("m_boneArray").and_then(|b| b.array().ok()).is_some_and(|b| !b.is_empty()) {
            return false;
        }
        for channel in key.get("m_dataChannelArray").and_then(|c| c.array().ok()).unwrap_or(&[]) {
            if channel.get("m_szChannelClass").and_then(|c| c.string().ok()) != Some(MORPH_CHANNEL) {
                return false;
            }
            channels += 1;
        }
    }
    !groups.is_empty() && channels > 0
}

/// The ONE rule for leaving a model's morph block out of the rebuild: the model
/// holds one, the game animates it only through the model's own animation blocks
/// (one each of `ANIM`, `ASEQ`, `AGRP`, no animation graph), and nothing that
/// players bump into or hit could depend on the shape (no collision, no hit
/// areas). Any other morph-bearing model stays closed (`REASON_MORPHS`).
pub fn left_out(facts: &ModelFacts) -> bool {
    facts.morphs && facts.legacy_animation() && facts.embedded_animation() && !facts.has_physics() && facts.hitboxes == 0
}

/// Whether nothing in the model but its render mesh and its animation reads the morph
/// block: the model holds one (`MRPH`); no cloth in its physics blocks names a morph layer
/// (`PHYS m_pFeModel.m_MorphLayers` and `m_MorphSetData` empty or absent); and every hit
/// area is a shape on a bone of the model skeleton (`MDAT m_hitboxsets[].m_HitBoxes[].m_sBoneName`).
/// Collision and hit areas then move with bones only, which the rebuild keeps.
pub fn morphs_apart(raw: &[u8], bones: &[String]) -> bool {
    if !container::layout(raw).is_ok_and(|l| l.blocks.iter().any(|b| b.name == MORPH_BLOCK)) {
        return false;
    }
    let empty = |v: Option<&deadlock_resource::Value>| v.is_none_or(|v| v.array().is_ok_and(<[_]>::is_empty) || matches!(v.kind, deadlock_resource::Kind::Null));
    let Ok(physics) = container::read_blocks(raw, "PHYS") else { return false };
    for block in &physics {
        if let Some(fe) = block.root.get("m_pFeModel").filter(|f| !matches!(f.kind, deadlock_resource::Kind::Null)) {
            if !empty(fe.get("m_MorphLayers")) || !empty(fe.get("m_MorphSetData")) {
                return false;
            }
        }
    }
    let Ok(mdats) = container::read_blocks(raw, "MDAT") else { return false };
    for mdat in &mdats {
        for set in mdat.root.get("m_hitboxsets").and_then(|v| v.array().ok()).unwrap_or(&[]) {
            for hit in set.get("value").unwrap_or(set).get("m_HitBoxes").and_then(|v| v.array().ok()).unwrap_or(&[]) {
                match hit.get("m_sBoneName").and_then(|n| n.string().ok()) {
                    Some(bone) if bones.iter().any(|b| b == bone) => {}
                    _ => return false,
                }
            }
        }
    }
    true
}

/// The rule for leaving a morph block out of an object played through an animation graph, or
/// one with collision or hit areas (`Switches::without_morphs_beside_bones`): the model holds
/// one; it has bones; its own animation blocks are one each of `ANIM`, `ASEQ`, `AGRP`, kept
/// as the game's (they may name the morph channels, as the shape-only animations `left_out`
/// opens do); its physics, if any, is one block inside the model, kept as the game's; and
/// nothing but the render mesh and the animation reads the morph data (`ModelFacts::morphs_apart`).
/// Never an object `left_out` already takes.
pub fn left_out_beside_bones(facts: &ModelFacts) -> bool {
    facts.morphs && facts.morphs_apart && !facts.bones.is_empty() && facts.embedded_animation() && (!facts.has_physics() || facts.embedded_physics()) && !left_out(facts)
}

/// Either rule: the morph block of this model is left out of a rebuild (the read-back's
/// allowance, the kept list and the install check go through here; the offered check asks
/// each rule's own switch).
pub fn left_out_any(facts: &ModelFacts) -> bool {
    left_out(facts) || left_out_beside_bones(facts)
}

/// An object whose animation changes only its shape: no bone, no animated material
/// value, a morph block `left_out` lets go, and decode keys of morph channels only.
/// Its animation is opened by `Switches::without_morphs` alone (a model with bones
/// is opened by its bones' own kinds).
pub fn shape_animation(facts: &ModelFacts) -> bool {
    facts.boneless() && facts.animated_material_values.is_empty() && facts.morph_channels_only && left_out(facts)
}

/// The morph textures a compiled model names (`CTRL embedded_meshes[].morph_texture`,
/// `MRPH m_pTextureAtlas`), casefolded without `_c`, sorted.
pub fn morph_textures(raw: &[u8]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut add = |value: &str| {
        if !value.is_empty() {
            out.insert(casefold(value).trim_end_matches("_c").to_owned());
        }
    };
    for ctrl in container::read_blocks(raw, "CTRL").unwrap_or_default() {
        for mesh in ctrl.root.get("embedded_meshes").and_then(|m| m.array().ok()).unwrap_or(&[]) {
            if let Some(texture) = mesh.get("morph_texture").and_then(|t| t.string().ok()) {
                add(texture);
            }
        }
    }
    for morph in container::read_blocks(raw, MORPH_BLOCK).unwrap_or_default() {
        if let Some(texture) = morph.root.get("m_pTextureAtlas").and_then(|t| t.string().ok()) {
            add(texture);
        }
    }
    out
}

/// What the read-back lets through because the morph data is absent from the rebuilt
/// model: the game's references that are its morph textures. `Ok` with an empty set for
/// a model without a morph block (nothing allowed). For a model `left_out_any` holds, every
/// condition is checked positively, and any other difference is an error naming it:
/// - the rebuilt file holds no `MRPH` block and names no morph texture;
/// - the game's file holds at least one `MRPH` block;
/// - the references the game's model has and the rebuilt one lacks are exactly the
///   morph textures the game's own `CTRL` / `MRPH` name, each a `.vtex`
///   (`ModelFacts::other_refs` of the two files).
pub fn allowance(original: &[u8], compiled: &[u8], theirs: &ModelFacts, ours: &ModelFacts) -> std::result::Result<BTreeSet<String>, String> {
    if !theirs.morphs || !left_out_any(theirs) {
        return Ok(BTreeSet::new());
    }
    let count = |raw: &[u8]| container::layout(raw).map(|l| l.blocks.iter().filter(|b| b.name == MORPH_BLOCK).count()).unwrap_or(0);
    if count(original) == 0 {
        return Err("the game's model holds no morph block".into());
    }
    if count(compiled) > 0 || ours.morphs {
        return Err(format!("the rebuilt model holds a morph block ({} blocks)", count(compiled)));
    }
    let textures = morph_textures(original);
    if textures.is_empty() || textures.iter().any(|t| !t.ends_with(".vtex")) {
        return Err(format!("the game's morph textures are not textures: {textures:?}"));
    }
    let named = morph_textures(compiled);
    if !named.is_empty() {
        return Err(format!("the rebuilt model names morph textures {named:?}"));
    }
    let theirs_refs: BTreeSet<&String> = theirs.other_refs.iter().collect();
    let ours_refs: BTreeSet<&String> = ours.other_refs.iter().collect();
    let lacking: BTreeSet<String> = theirs_refs.difference(&ours_refs).map(|r| (*r).clone()).collect();
    if lacking != textures {
        return Err(format!("the rebuilt model lacks the game's references {lacking:?}, not exactly its morph textures {textures:?}"));
    }
    Ok(textures)
}

/// The game's facts as `kept_differences` compares them with a rebuilt model's: the
/// morph textures `allowance` lets through taken out of the references, and those
/// textures (empty for every model without a morph block: the facts unchanged). The
/// read-back and the install check both go through here.
pub fn compared(original: &[u8], compiled: &[u8], theirs: ModelFacts, ours: &ModelFacts) -> std::result::Result<(ModelFacts, BTreeSet<String>), String> {
    let left = allowance(original, compiled, &theirs, ours)?;
    if left.is_empty() {
        return Ok((theirs, left));
    }
    let other_refs = theirs.other_refs.iter().filter(|r| !left.contains(*r)).cloned().collect();
    Ok((ModelFacts { other_refs, ..theirs }, left))
}

/// The rows of the game's `MRPH` block (counted for the log; never compared: the rebuilt
/// model has none).
pub fn morph_rows(raw: &[u8]) -> usize {
    container::read_blocks(raw, MORPH_BLOCK).unwrap_or_default().iter().map(|b| flatten(&b.root).len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability_objects::{REASON_ANIMATED, REASON_MORPHS, SWITCHES, Switches, offered_with};

    /// One of the shape-only objects as its facts read: no bone, one mesh, one material,
    /// a legacy animation of morph channels only, a morph block.
    fn jar() -> ModelFacts {
        ModelFacts {
            blocks: ["MRPH", "MVTX", "MIDX", "MDAT", "ANIM", "ASEQ", "AGRP", "CTRL", "RERL", "RED2", "DATA"].map(String::from).to_vec(),
            mesh_skeleton_bones: Some(0),
            embedded_meshes: 1,
            animation_blocks: ["ANIM", "ASEQ", "AGRP"].map(String::from).to_vec(),
            materials: vec!["materials/sample/glass.vmat".into()],
            other_refs: vec!["models/sample/jar/jar_vmorf.vtex".into()],
            morphs: true,
            morph_channels_only: true,
            ..ModelFacts::default()
        }
    }
    /// The winged icon: bones, a legacy animation of bones and morph channels, four materials.
    fn icon() -> ModelFacts {
        let bones: Vec<String> = ["root", "eye", "wing_l", "wing_r"].map(String::from).to_vec();
        ModelFacts {
            bones: bones.clone(),
            mesh_skeleton_bones: Some(4),
            mesh_bones: bones,
            materials: vec!["materials/sample/a.vmat".into(), "materials/sample/b.vmat".into()],
            morph_channels_only: false,
            ..jar()
        }
    }
    fn on() -> Switches {
        Switches { without_morphs: true, ..SWITCHES }
    }
    /// The app's switches with this kind closed.
    fn off() -> Switches {
        Switches { without_morphs: false, ..SWITCHES }
    }

    #[test]
    fn the_switch_alone_opens_a_shape_only_animation_and_bones_with_a_morph_block() {
        // The app's switches open this kind.
        assert_eq!(SWITCHES, on());
        // Off: both stay closed with the shape reason.
        assert_eq!(offered_with(&jar(), false, &off()), Err(REASON_MORPHS));
        assert_eq!(offered_with(&icon(), false, &off()), Err(REASON_MORPHS));
        // On: both offered, the jar as a shape-only animation, the icon on its bones' kinds.
        assert!(shape_animation(&jar()));
        assert!(!shape_animation(&icon()));
        assert_eq!(offered_with(&jar(), false, &on()), Ok(()));
        assert_eq!(offered_with(&jar(), true, &on()), Ok(()));
        assert_eq!(offered_with(&icon(), false, &on()), Ok(()));
        // The kinds under it still count: without kept animation the jar stays closed.
        assert!(offered_with(&jar(), false, &Switches { kept_animation: false, ..on() }).is_err());
        assert_eq!(offered_with(&icon(), false, &Switches { several_bones: false, ..on() }), Err(crate::ability_objects::REASON_BONES));
    }

    #[test]
    fn the_kept_list_says_the_shape_change_is_the_game_s_alone() {
        use crate::ability_objects::{KEPT_GAME_ANIMATIONS, KEPT_BONES, object_kind};
        let kind = object_kind(&jar(), false).unwrap();
        assert_eq!(kind.kept, [KEPT_GAME_ANIMATIONS, KEPT_SHAPE]);
        let kind = object_kind(&icon(), false).unwrap();
        assert_eq!(kind.kept.first().map(String::as_str), Some(KEPT_GAME_ANIMATIONS));
        assert_eq!(kind.kept.get(1).map(String::as_str), Some(KEPT_SHAPE));
        assert!(kind.kept.iter().any(|k| k == KEPT_BONES));
        // Without a morph block the line never shows.
        let none = ModelFacts { morphs: false, ..icon() };
        assert!(!object_kind(&none, false).unwrap().kept.iter().any(|k| k == KEPT_SHAPE));
    }

    #[test]
    fn a_morph_block_collision_or_hit_areas_could_depend_on_stays_closed() {
        for facts in [
            ModelFacts { physics_block: true, physics_shapes: [("capsule".to_owned(), 1)].into(), blocks: [jar().blocks, vec!["PHYS".into()]].concat(), ..jar() },
            ModelFacts { hitboxes: 1, ..jar() },
            ModelFacts { physics_block: true, physics_shapes: [("capsule".to_owned(), 1)].into(), ..icon() },
        ] {
            assert!(!left_out(&facts));
            assert!(offered_with(&facts, false, &on()).is_err(), "{facts:?}");
        }
        // An animation graph, two animation blocks of a kind: not left out.
        assert!(!left_out(&ModelFacts { anim_graph: Some("animgraphs/sample.vnmgraph".into()), ..jar() }));
        assert!(!left_out(&ModelFacts { blocks: [jar().blocks, vec!["ANIM".into()]].concat(), ..jar() }));
    }

    #[test]
    fn only_an_animation_of_morph_channels_alone_counts_as_shape_only() {
        // Channels of another class (bones), a material value, a bone: not shape only, and
        // without bones the animation stays closed as animated.
        for facts in [
            ModelFacts { morph_channels_only: false, ..jar() },
            ModelFacts { animated_material_values: vec!["F_JITTER".into()], ..jar() },
        ] {
            assert!(!shape_animation(&facts));
            assert_eq!(offered_with(&facts, false, &on()), Err(REASON_MORPHS));
        }
        // No morph block: the switch opens nothing (the animation is the other kinds' business).
        let none = ModelFacts { morphs: false, other_refs: vec![], blocks: jar().blocks.into_iter().filter(|b| b != "MRPH").collect(), ..jar() };
        assert!(!left_out(&none) && !shape_animation(&none));
        assert_eq!(offered_with(&none, false, &on()), Err(REASON_ANIMATED));
        assert_eq!(offered_with(&none, false, &on()), offered_with(&none, false, &off()));
    }

    use deadlock_resource::{Kind, Value, binary};

    const GUID: [u8; 16] = [0x7c, 0x16, 0x12, 0x74, 0xe9, 0x06, 0x98, 0x46, 0xaf, 0xf2, 0xe6, 0x3e, 0xb5, 0x90, 0x37, 0xe7];
    fn s(t: &str) -> Value {
        Value::new(Kind::String(t.into()))
    }
    fn obj(rows: Vec<(&str, Value)>) -> Value {
        Value::new(Kind::Object(rows.into_iter().map(|(k, x)| (k.to_owned(), x)).collect()))
    }
    fn arr(items: Vec<Value>) -> Value {
        Value::new(Kind::Array(items))
    }
    fn kv3(root: &Value) -> Vec<u8> {
        binary::encode(root, GUID).unwrap()
    }
    /// A compiled resource holding `blocks` in order (16-byte aligned payloads).
    fn container_of(blocks: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0u8; 16 + 12 * blocks.len()];
        out[4..6].copy_from_slice(&12u16.to_le_bytes());
        out[8..12].copy_from_slice(&8u32.to_le_bytes());
        out[12..16].copy_from_slice(&(blocks.len() as u32).to_le_bytes());
        for (n, (name, bytes)) in blocks.iter().enumerate() {
            out.resize((out.len() + 15) & !15, 0);
            let start = out.len();
            out.extend_from_slice(bytes);
            let entry = 16 + 12 * n;
            out[entry..entry + 4].copy_from_slice(name.as_bytes());
            out[entry + 4..entry + 8].copy_from_slice(&((start - entry - 4) as u32).to_le_bytes());
            out[entry + 8..entry + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        }
        let size = out.len() as u32;
        out[..4].copy_from_slice(&size.to_le_bytes());
        out
    }
    fn ctrl(texture: Option<&str>) -> Vec<u8> {
        let mesh = match texture {
            Some(t) => obj(vec![("m_Name", s("jar")), ("morph_texture", s(t))]),
            None => obj(vec![("m_Name", s("jar"))]),
        };
        kv3(&obj(vec![("embedded_meshes", arr(vec![mesh]))]))
    }
    fn agrp(classes: &[&str], bones: &[&str]) -> Vec<u8> {
        let channels = classes.iter().map(|c| obj(vec![("m_szChannelClass", s(c))])).collect();
        let bones = bones.iter().map(|b| obj(vec![("m_name", s(b))])).collect();
        kv3(&obj(vec![("m_decodeKey", obj(vec![("m_boneArray", arr(bones)), ("m_dataChannelArray", arr(channels))]))]))
    }
    const TEXTURE: &str = "models/sample/jar/jar_vmorf.vtex";
    fn game_file() -> Vec<u8> {
        container_of(&[("MRPH", kv3(&obj(vec![("m_pTextureAtlas", s(TEXTURE))]))), ("AGRP", agrp(&["MorphChannel"], &[])), ("CTRL", ctrl(Some(TEXTURE)))])
    }
    fn rebuilt() -> (Vec<u8>, ModelFacts) {
        let facts = ModelFacts { morphs: false, morph_channels_only: false, other_refs: vec![], blocks: jar().blocks.into_iter().filter(|b| b != "MRPH").collect(), ..jar() };
        (container_of(&[("AGRP", agrp(&["MorphChannel"], &[])), ("CTRL", ctrl(None))]), facts)
    }

    /// A graph-played object with bones, a morph block, collision on a bone and hit areas on
    /// bones (the cat form's kind).
    fn creature() -> ModelFacts {
        let bones: Vec<String> = ["root", "pelvis", "tail_0", "tail_1"].map(String::from).to_vec();
        ModelFacts {
            blocks: ["MRPH", "MVTX", "MIDX", "MDAT", "ANIM", "ASEQ", "AGRP", "PHYS", "CTRL", "RERL", "RED2", "DATA"].map(String::from).to_vec(),
            bones: bones.clone(),
            mesh_skeleton_bones: Some(3),
            mesh_bones: bones[1..].to_vec(),
            anim_graph: Some("animgraphs/sample/creature.vnmgraph".into()),
            physics_block: true,
            physics_on_bones: true,
            physics_shapes: [("hull of 8 points".to_owned(), 1)].into(),
            hitboxes: 2,
            morphs_apart: true,
            ..jar()
        }
    }
    fn beside() -> Switches {
        Switches { without_morphs_beside_bones: true, ..Switches::EVERY }
    }

    #[test]
    fn a_morph_block_beside_bones_is_left_out_only_behind_its_own_switch() {
        let f = creature();
        assert!(!left_out(&f) && left_out_beside_bones(&f) && left_out_any(&f));
        assert_eq!(offered_with(&f, true, &Switches { without_morphs_beside_bones: false, ..Switches::EVERY }), Err(REASON_MORPHS));
        assert_eq!(offered_with(&f, true, &Switches { without_morphs_beside_bones: false, ..SWITCHES }), Err(REASON_MORPHS));
        assert_eq!(offered_with(&f, true, &SWITCHES), Ok(()), "the app's switches");
        assert_eq!(offered_with(&f, true, &beside()), Ok(()));
        // The morph data read by more than the mesh and the animation, no bones, a physics file,
        // or two animation blocks of a kind: not left out.
        for other in [
            ModelFacts { morphs_apart: false, ..creature() },
            ModelFacts { bones: vec![], mesh_skeleton_bones: Some(0), mesh_bones: vec![], ..creature() },
            ModelFacts { physics_refs: 1, ..creature() },
            ModelFacts { blocks: [creature().blocks, vec!["AGRP".into()]].concat(), ..creature() },
        ] {
            assert!(!left_out_beside_bones(&other), "{other:?}");
            assert!(offered_with(&other, true, &beside()).is_err(), "{other:?}");
        }
        // An object the first rule takes is never this rule's.
        assert!(left_out(&jar()) && !left_out_beside_bones(&jar()));
        // The kept list says the shape is the game's alone.
        let kind = crate::ability_objects::object_kind(&f, true).unwrap();
        assert!(kind.kept.iter().any(|k| k == KEPT_SHAPE));
        // The allowance takes this rule too: exactly the morph textures.
        let (ours_raw, _) = rebuilt();
        let ours = ModelFacts { morphs: false, morphs_apart: false, other_refs: vec![], blocks: creature().blocks.into_iter().filter(|b| b != "MRPH").collect(), ..creature() };
        assert_eq!(allowance(&game_file(), &ours_raw, &f, &ours), Ok(BTreeSet::from([TEXTURE.to_owned()])));
    }

    fn phys(morph_layers: usize) -> Vec<u8> {
        let layers = (0..morph_layers).map(|_| obj(vec![("m_Name", s("layer"))])).collect();
        kv3(&obj(vec![("m_boneNames", arr(vec![s("root")])), ("m_pFeModel", obj(vec![("m_CtrlName", arr(vec![s("tail_0")])), ("m_MorphLayers", arr(layers)), ("m_MorphSetData", arr(vec![]))]))]))
    }
    fn mdat(bone: &str) -> Vec<u8> {
        let hit = obj(vec![("m_name", s("hit")), ("m_sBoneName", s(bone))]);
        kv3(&obj(vec![("m_hitboxsets", arr(vec![obj(vec![("key", s("default")), ("value", obj(vec![("m_HitBoxes", arr(vec![hit]))]))])]))]))
    }

    #[test]
    fn the_morph_data_is_apart_when_no_cloth_layer_and_no_hit_area_off_a_bone_reads_it() {
        let bones: Vec<String> = ["root", "pelvis"].map(String::from).to_vec();
        let morph = ("MRPH", kv3(&obj(vec![("m_pTextureAtlas", s(TEXTURE))])));
        assert!(morphs_apart(&container_of(&[morph.clone(), ("PHYS", phys(0)), ("MDAT", mdat("pelvis"))]), &bones));
        assert!(morphs_apart(&container_of(&[morph.clone(), ("MDAT", mdat("pelvis"))]), &bones), "no physics");
        assert!(!morphs_apart(&container_of(&[morph.clone(), ("PHYS", phys(1)), ("MDAT", mdat("pelvis"))]), &bones), "a cloth morph layer");
        assert!(!morphs_apart(&container_of(&[morph.clone(), ("PHYS", phys(0)), ("MDAT", mdat("wing"))]), &bones), "a hit area off the skeleton");
        assert!(!morphs_apart(&container_of(&[("PHYS", phys(0)), ("MDAT", mdat("pelvis"))]), &bones), "no morph block");
    }

    #[test]
    fn the_channel_classes_of_the_decode_key_tell_a_shape_only_animation() {
        assert!(morph_channels_only(&game_file()));
        assert!(!morph_channels_only(&container_of(&[("AGRP", agrp(&["BoneChannel", "MorphChannel"], &[]))])));
        assert!(!morph_channels_only(&container_of(&[("AGRP", agrp(&["MorphChannel"], &["root"]))])));
        assert!(!morph_channels_only(&container_of(&[("AGRP", agrp(&[], &[]))])), "no channel at all");
        assert!(!morph_channels_only(&container_of(&[("CTRL", ctrl(None))])), "no animation group");
        assert_eq!(morph_textures(&game_file()), BTreeSet::from([TEXTURE.to_owned()]));
    }

    #[test]
    fn the_allowance_is_exactly_the_morph_textures_and_nothing_else() {
        let (ours_raw, ours) = rebuilt();
        // Allowed: the game's morph textures, and the compared facts lose only them.
        assert_eq!(allowance(&game_file(), &ours_raw, &jar(), &ours), Ok(BTreeSet::from([TEXTURE.to_owned()])));
        let (theirs, _) = compared(&game_file(), &ours_raw, jar(), &ours).unwrap();
        assert!(theirs.other_refs.is_empty());
        assert!(crate::ability_objects::kept_differences(&theirs, &ours).is_empty());
        assert_eq!(crate::ability_objects::kept_differences(&jar(), &ours), ["non-material references"], "without the allowance");
        // Another missing reference beside the texture: refused, naming both.
        let more = ModelFacts { other_refs: vec![TEXTURE.into(), "models/sample/other.vmdl".into()], ..jar() };
        assert!(allowance(&game_file(), &ours_raw, &more, &ours).unwrap_err().contains("other.vmdl"));
        // The rebuilt model still names the texture, or holds a morph block: refused.
        let named = container_of(&[("CTRL", ctrl(Some(TEXTURE)))]);
        assert!(allowance(&game_file(), &named, &jar(), &ours).is_err());
        let with_block = container_of(&[("MRPH", kv3(&obj(vec![]))), ("CTRL", ctrl(None))]);
        assert!(allowance(&game_file(), &with_block, &jar(), &ours).is_err());
        // The game's facts say morphs but its file holds no morph block: refused.
        assert!(allowance(&ours_raw, &ours_raw, &jar(), &ours).is_err());
        // A model without a morph block, or one the rule does not let go: nothing allowed, facts unchanged.
        assert_eq!(allowance(&ours_raw, &ours_raw, &ours, &ours), Ok(BTreeSet::new()));
        let hit = ModelFacts { hitboxes: 1, ..jar() };
        assert_eq!(allowance(&game_file(), &ours_raw, &hit, &ours), Ok(BTreeSet::new()));
        assert_eq!(compared(&game_file(), &ours_raw, hit.clone(), &ours).unwrap().0, hit);
    }
}
