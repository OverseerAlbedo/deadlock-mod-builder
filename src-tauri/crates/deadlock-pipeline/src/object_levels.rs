//! Building an ability object whose levels of detail each draw the replaced mesh and a part of
//! another body group kept as the game's own (`deadlock_components::object_parts::in_levels`,
//! `Switches::parts_kept_in_levels`).
//!
//! Three changes to the decompiled vmdl, each decided from the game's compiled file:
//! * The bones marked to be kept (`do_not_discard`) are exactly the bones the game's file binds
//!   the kept parts to (`object_parts::kept_bones`). The compiler binds every mesh to the bones its
//!   points use plus every marked bone: with every bone marked (as the decompile writes them) the
//!   kept parts came out bound to all 91 bones of the pigeon object, with none marked to 12, with
//!   exactly theirs marked to the game's 39.
//! * The bones stand in the order of the game's animation decode key (`AGRP m_decodeKey`, the game
//!   source's depth-first order; the decompile lists some children of a bone in another order).
//!   The compiler lists a mesh's bones in an order that follows the source's: with the decompile's
//!   order the kept parts held the game's 39 bones with five of them elsewhere in the list.
//! * Where the game's key values list the cameras in the reverse of their attachments' order
//!   (`ModelFacts::cameras_reversed`), the attachment list goes into a prefab file beside the model
//!   (`attachments_in_prefab`), as the game's source had it (its `RED2` names an attachments
//!   `.vmdl_prefab`): the compiler lists the cameras of attachments written in the vmdl in their own
//!   order (so moving the attachments would move them in the mesh blocks too), and of attachments
//!   from a prefab in the reverse order, the attachments themselves in their own.
//!
//! With only some bones marked, the compiler writes the resting turn of an unmarked bone no mesh
//! binds through its Euler angles, and so with the other sign where
//! `object_parts::round_trip_flips` says so; those turns are written back from the game's file
//! (`write_back::WrittenKind::UnboundRestSigns`), never accepted otherwise. (Writing the angles as
//! the other Euler triple of the same turn does not change it: the compiler brings them back.)
//!
//! The text is unchanged for every other object and while the switch is closed (the hook in
//! `ObjectBuild::build_carried`).
use crate::invalid;
use deadlock_components::ability_objects::{self as objects, ModelFacts};
use deadlock_components::object_parts;
use deadlock_core::Result;
use deadlock_resource::text::{Document, Edit};
use std::collections::BTreeSet;

/// The vmdl for a build of `in_levels`, with what changed for the log, and the prefab file the
/// attachments went into (path in the game tree, text) when they did; `text` unchanged otherwise.
pub fn vmdl(facts: &ModelFacts, text: String, game_raw: &[u8], target: &str) -> Result<(String, Vec<String>, Option<(String, String)>)> {
    if !objects::switches().parts_kept_in_levels {
        return Ok((text, vec![], None));
    }
    let Some(parts) = object_parts::in_levels(facts) else { return Ok((text, vec![], None)) };
    let mut notes = vec![];
    let bones = object_parts::kept_bones(facts, &parts);
    if bones.is_empty() {
        return Err(invalid("The game's kept parts state no bones they are bound to."));
    }
    let text = marked_bones(text, &bones.iter().cloned().collect())?;
    notes.push(format!("{} bones marked to be kept, exactly the bones the game's file binds the kept parts to", bones.len()));
    let (text, moved) = bones_in_order(text, &decode_order(game_raw)?)?;
    if moved {
        notes.push("the bones in the order of the game's animation decode key".into());
    }
    let (text, prefab) = if facts.cameras_reversed {
        let path = prefab_path(target);
        let (text, prefab) = attachments_in_prefab(&text, &path)?;
        notes.push(format!("the attachments in a prefab file of their own ({path}), so the cameras come out in the game's order"));
        (text, Some((path, prefab)))
    } else {
        (text, None)
    };
    Ok((text, notes, prefab))
}

/// The bone names of the game's animation decode key (`AGRP m_decodeKey.m_boneArray[].m_name`), in order.
pub fn decode_order(raw: &[u8]) -> Result<Vec<String>> {
    let agrp = deadlock_resource::container::read_blocks(raw, "AGRP").map_err(|e| invalid(e.to_string()))?;
    Ok(agrp
        .first()
        .and_then(|d| d.root.get("m_decodeKey"))
        .and_then(|k| k.get("m_boneArray"))
        .and_then(|a| a.array().ok())
        .map(|a| a.iter().filter_map(|b| b.get("m_name").and_then(|n| n.string().ok()).map(str::to_owned)).collect())
        .unwrap_or_default())
}

/// The prefab file the attachments of `target` (a `.vmdl` in the game tree) go into: beside it,
/// `<name>_attachments.vmdl_prefab`. Written for the compiler only, never packaged.
pub fn prefab_path(target: &str) -> String {
    format!("{}_attachments.vmdl_prefab", target.strip_suffix(".vmdl").unwrap_or(target))
}

/// `text` with every bone's `do_not_discard` `true` exactly for the bones in `keep` and `false`
/// for every other bone; nothing else changes. Every bone must state the key (the decompile
/// writes it on each), and every bone of `keep` must be a bone of the text.
pub fn marked_bones(text: String, keep: &BTreeSet<String>) -> Result<String> {
    let document = Document::parse(&text).map_err(|e| invalid(e.to_string()))?;
    let mut edits = vec![];
    let mut seen = BTreeSet::new();
    for bone in document.objects(Some("Bone")) {
        let name = bone.get("name").and_then(|n| n.scalar()).unwrap_or_default().to_owned();
        let Some(value) = bone.get("do_not_discard") else { return Err(invalid(format!("The bone {name} of the game's object states no do_not_discard."))) };
        let wanted = if keep.contains(&name) { "true" } else { "false" };
        if value.scalar() != Some(wanted) {
            edits.push(Edit { start: value.start, end: value.end, replacement: wanted.into() });
        }
        seen.insert(name);
    }
    if let Some(missing) = keep.iter().find(|b| !seen.contains(*b)) {
        return Err(invalid(format!("The bone {missing} the game's kept parts are bound to is not in the decompiled model.")));
    }
    if edits.is_empty() {
        return Ok(text);
    }
    document.apply(&edits).map_err(|e| invalid(e.to_string()))
}

/// `text` with the children of the skeleton and of every bone in the order `order` lists their
/// names; a child `order` does not name keeps its place after the named ones. Each bone moves
/// with its whole subtree, so only the order of siblings changes. Unchanged (`false`) when every
/// list is already in that order.
pub fn bones_in_order(text: String, order: &[String]) -> Result<(String, bool)> {
    let rank = |name: &str| order.iter().position(|o| o == name).unwrap_or(usize::MAX);
    let mut text = text;
    let mut moved = false;
    // One list at a time (the lists nest), in text order; each list is put in order once.
    for _ in 0..4096 {
        let document = Document::parse(&text).map_err(|e| invalid(e.to_string()))?;
        let mut parents = document.objects(Some("Skeleton"));
        parents.extend(document.objects(Some("Bone")));
        parents.sort_by_key(|p| p.start);
        let mut edits = vec![];
        for parent in parents {
            let Some(children) = parent.get("children").and_then(|c| c.array()) else { continue };
            let bones: Vec<_> = children.iter().filter(|c| c.get("_class").and_then(|k| k.scalar()) == Some("Bone")).collect();
            if bones.len() != children.len() || bones.len() < 2 {
                continue;
            }
            let mut sorted: Vec<_> = bones.iter().enumerate().collect();
            sorted.sort_by_key(|(i, b)| (rank(b.get("name").and_then(|n| n.scalar()).unwrap_or_default()), *i));
            if sorted.iter().enumerate().all(|(k, (i, _))| k == *i) {
                continue;
            }
            for (slot, (_, bone)) in bones.iter().zip(&sorted) {
                edits.push(Edit { start: slot.start, end: slot.end, replacement: text[bone.start..bone.end].to_owned() });
            }
            break;
        }
        if edits.is_empty() {
            return Ok((text, moved));
        }
        text = document.apply(&edits).map_err(|e| invalid(e.to_string()))?;
        moved = true;
    }
    Err(invalid("The bones of the game's object could not be put in the game's order."))
}

/// `text` with its attachment list moved into a prefab file (`prefab`, a path in the game tree)
/// that holds that list, unchanged, as its root's one child, and the vmdl naming the prefab in
/// a `PrefabList` in the list's place; and the prefab file's text.
pub fn attachments_in_prefab(text: &str, prefab: &str) -> Result<(String, String)> {
    let document = Document::parse(text).map_err(|e| invalid(e.to_string()))?;
    let lists = document.objects(Some("AttachmentList"));
    let [list] = lists.as_slice() else { return Err(invalid("The game's object holds no single attachment list.")) };
    let header = text.lines().next().filter(|l| l.starts_with("<!--")).ok_or_else(|| invalid("The decompiled model has no format line."))?;
    let indent: String = text[..list.start].rsplit('\n').next().unwrap_or_default().chars().take_while(|c| c.is_whitespace()).collect();
    // The list's own lines one level shallower would be tidier; the reader takes either.
    let body = &text[list.start..list.end];
    let prefab_text = format!("{header}\n{{\n\trootNode = \n\t{{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n\t\t\t{body},\n\t\t]\n\t}}\n}}\n");
    let reference = format!("{{\n{indent}\t_class = \"PrefabList\"\n{indent}\tchildren = \n{indent}\t[\n{indent}\t\t{{\n{indent}\t\t\t_class = \"Prefab\"\n{indent}\t\t\ttarget_file = {}\n{indent}\t\t}},\n{indent}\t]\n{indent}}}", serde_json::to_string(prefab).expect("string"));
    let vmdl = document.apply(&[Edit { start: list.start, end: list.end, replacement: reference }]).map_err(|e| invalid(e.to_string()))?;
    Ok((vmdl, prefab_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VMDL: &str = "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:modeldoc28:version{fb63b6ca-f435-4aa0-a2c7-c66ddc651dca} -->\n{\n\trootNode = \n\t{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n\t\t\t{\n\t\t\t\t_class = \"AttachmentList\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{\n\t\t\t\t\t\t_class = \"Attachment\"\n\t\t\t\t\t\tname = \"near_00\"\n\t\t\t\t\t\tparent_bone = \"a\"\n\t\t\t\t\t\tchildren = \n\t\t\t\t\t\t[\n\t\t\t\t\t\t\t{\n\t\t\t\t\t\t\t\t_class = \"Attachment Camera Preview\"\n\t\t\t\t\t\t\t\tfov = 90\n\t\t\t\t\t\t\t},\n\t\t\t\t\t\t]\n\t\t\t\t\t},\n\t\t\t\t\t{\n\t\t\t\t\t\t_class = \"Attachment\"\n\t\t\t\t\t\tname = \"mask\"\n\t\t\t\t\t\tparent_bone = \"a\"\n\t\t\t\t\t},\n\t\t\t\t]\n\t\t\t},\n\t\t\t{\n\t\t\t\t_class = \"Skeleton\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{\n\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\tname = \"a\"\n\t\t\t\t\t\tdo_not_discard = true\n\t\t\t\t\t\tchildren = \n\t\t\t\t\t\t[\n\t\t\t\t\t\t\t{\n\t\t\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\t\t\tname = \"b\"\n\t\t\t\t\t\t\t\tdo_not_discard = true\n\t\t\t\t\t\t\t\tchildren = \n\t\t\t\t\t\t\t\t[\n\t\t\t\t\t\t\t\t\t{\n\t\t\t\t\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\t\t\t\t\tname = \"b2\"\n\t\t\t\t\t\t\t\t\t\tdo_not_discard = true\n\t\t\t\t\t\t\t\t\t},\n\t\t\t\t\t\t\t\t\t{\n\t\t\t\t\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\t\t\t\t\tname = \"b1\"\n\t\t\t\t\t\t\t\t\t\tdo_not_discard = true\n\t\t\t\t\t\t\t\t\t},\n\t\t\t\t\t\t\t\t]\n\t\t\t\t\t\t\t},\n\t\t\t\t\t\t\t{\n\t\t\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\t\t\tname = \"c\"\n\t\t\t\t\t\t\t\tdo_not_discard = true\n\t\t\t\t\t\t\t},\n\t\t\t\t\t\t]\n\t\t\t\t\t},\n\t\t\t\t]\n\t\t\t},\n\t\t]\n\t}\n}\n";

    fn bone_names(text: &str) -> Vec<String> {
        Document::parse(text).unwrap().objects(Some("Bone")).iter().map(|a| (a.start, a.get("name").unwrap().scalar().unwrap().to_owned())).collect::<std::collections::BTreeMap<_, _>>().into_values().collect()
    }
    fn sorted_lines(text: &str) -> Vec<&str> {
        let mut lines: Vec<&str> = text.lines().collect();
        lines.sort_unstable();
        lines
    }

    #[test]
    fn exactly_the_kept_parts_bones_are_marked_to_be_kept() {
        let out = marked_bones(VMDL.to_owned(), &BTreeSet::from(["b".to_owned()])).unwrap();
        assert_eq!(out.matches("do_not_discard = true").count(), 1);
        assert_eq!(out.matches("do_not_discard = false").count(), 4);
        assert!(out[out.find("name = \"b\"").unwrap()..].starts_with("name = \"b\"\n\t\t\t\t\t\t\t\tdo_not_discard = true"));
        assert_eq!(out.replace("do_not_discard = false", "do_not_discard = true"), VMDL);
        let all = BTreeSet::from(["a", "b", "b1", "b2", "c"].map(str::to_owned));
        assert_eq!(marked_bones(VMDL.to_owned(), &all).unwrap(), VMDL);
        assert!(marked_bones(VMDL.to_owned(), &BTreeSet::from(["d".to_owned()])).is_err());
    }

    #[test]
    fn siblings_take_the_decode_key_s_order_with_their_subtrees() {
        assert_eq!(bone_names(VMDL), ["a", "b", "b2", "b1", "c"]);
        let order: Vec<String> = ["a", "c", "b", "b1", "b2"].map(str::to_owned).to_vec();
        let (out, moved) = bones_in_order(VMDL.to_owned(), &order).unwrap();
        assert!(moved);
        assert_eq!(bone_names(&out), ["a", "c", "b", "b1", "b2"]);
        // Only lines moved, none changed.
        assert_eq!(sorted_lines(&out), sorted_lines(VMDL));
        // In order already: unchanged.
        assert_eq!(bones_in_order(out.clone(), &order).unwrap(), (out, false));
        let same: Vec<String> = ["a", "b", "b2", "b1", "c"].map(str::to_owned).to_vec();
        assert_eq!(bones_in_order(VMDL.to_owned(), &same).unwrap(), (VMDL.to_owned(), false));
    }

    #[test]
    fn the_attachment_list_goes_whole_into_its_prefab() {
        let path = prefab_path("models/heroes_wip/sample/sample.vmdl");
        assert_eq!(path, "models/heroes_wip/sample/sample_attachments.vmdl_prefab");
        let (vmdl, prefab) = attachments_in_prefab(VMDL, &path).unwrap();
        let list = &VMDL[VMDL.find("{\n\t\t\t\t_class = \"AttachmentList\"").unwrap()..VMDL.find("\t\t\t{\n\t\t\t\t_class = \"Skeleton\"").unwrap()];
        let list = list.trim_end().trim_end_matches(',');
        assert!(prefab.contains(list), "the list unchanged in the prefab");
        assert!(prefab.starts_with(VMDL.lines().next().unwrap()));
        assert!(!vmdl.contains("AttachmentList") && !vmdl.contains("Attachment Camera Preview"));
        assert!(vmdl.contains("_class = \"PrefabList\"") && vmdl.contains(&format!("target_file = \"{path}\"")));
        // The vmdl's other nodes are unchanged.
        assert!(vmdl.contains(&VMDL[VMDL.find("\t\t\t{\n\t\t\t\t_class = \"Skeleton\"").unwrap()..]));
    }
}
