//! Ability objects with cloth kept as the game's own (`ability_objects::KeptKind::Cloth`):
//! the game's resting bones for the rebuilt model.
//!
//! Source 2 Viewer writes the cloth into the decompiled model (a `Softbody` with its
//! chains), which the compiler needs so that it writes a physics block at all (the
//! slot the game's own block is kept in). While doing so it writes the resting place
//! of the bones the cloth reaches from the cloth's own starting pose, not from the
//! model's skeleton: on the Rat King's nibbler `nibbler_tail_end` comes out at 5.032933
//! in from its parent where the skeleton holds 5.033058 (1.2e-4 in, past the read-back's
//! 1e-4), and 26 other bones move by up to 1e-5 in. Decompiled without the cloth
//! (`--skip_softbody_reconstruction`) every bone is the skeleton's. So for these objects
//! the model is decompiled twice, and every bone's `origin` and `angles` are taken from
//! the decompile without the cloth, by bone name; nothing else of it is used.
use crate::invalid;
use deadlock_core::Result;
use deadlock_resource::text::{Document, Edit};
use std::collections::BTreeMap;

/// The folder (beside the decompiled tree) of the decompile without the cloth.
pub const REST_FOLDER: &str = "without-cloth";
/// The Source 2 Viewer CLI flag that leaves the cloth out of a decompiled model (21.1).
pub const WITHOUT_CLOTH_FLAG: &str = "--skip_softbody_reconstruction";
/// The failure when the decompile without the cloth wrote no model (followed by the target).
pub const NOT_WRITTEN: &str = "Source 2 Viewer did not write the game's ability object without its cloth";
/// The failure when the two decompiles name different bones (followed by some of them).
pub const OTHER_BONES: &str = "The game's ability object decompiles with other bones without its cloth";
/// Each `Bone` of a vmdl text by name: the spans and text of its `origin` and `angles`.
fn bones(text: &str) -> Result<BTreeMap<String, [(usize, usize, String); 2]>> {
    let document = Document::parse(text).map_err(|e| invalid(e.to_string()))?;
    let mut out = BTreeMap::new();
    for bone in document.objects(Some("Bone")) {
        let Some(name) = bone.get("name").and_then(|n| n.scalar()) else { return Err(invalid("A bone of the game's ability object has no name.")) };
        let (Some(origin), Some(angles)) = (bone.get("origin"), bone.get("angles")) else {
            return Err(invalid(format!("The game's ability object's bone {name} has no resting place.")));
        };
        let span = |n: &deadlock_resource::text::Node| (n.start, n.end, text[n.start..n.end].to_owned());
        if out.insert(name.to_owned(), [span(origin), span(angles)]).is_some() {
            return Err(invalid(format!("The game's ability object names the bone {name} twice.")));
        }
    }
    Ok(out)
}
/// `text` (the decompile with the cloth, as the build writes it) with every bone's
/// `origin` and `angles` taken from `rest` (the decompile without the cloth), by name.
/// Both must name exactly the same bones, except that the decompile without the cloth may also
/// list `proxy` bones (the bones the compiler makes from the cloth's proxy mesh, as that
/// decompile spells them), which the decompile with the cloth leaves to the proxy mesh. Returns
/// the text and how many bones changed.
pub fn rest_from(text: &str, rest: &str, proxy: &std::collections::BTreeSet<String>) -> Result<(String, usize)> {
    let (ours, mut theirs) = (bones(text)?, bones(rest)?);
    theirs.retain(|name, _| ours.contains_key(name) || !proxy.contains(name));
    if ours.keys().ne(theirs.keys()) {
        let only: Vec<&String> = ours.keys().filter(|k| !theirs.contains_key(*k)).chain(theirs.keys().filter(|k| !ours.contains_key(*k))).take(8).collect();
        return Err(invalid(format!("{OTHER_BONES} ({only:?}).")));
    }
    let mut edits = vec![];
    let mut changed = 0;
    for (name, spans) in &ours {
        let mut differs = false;
        for (k, (start, end, value)) in spans.iter().enumerate() {
            let wanted = &theirs[name][k].2;
            if value != wanted {
                edits.push(Edit { start: *start, end: *end, replacement: wanted.clone() });
                differs = true;
            }
        }
        changed += usize::from(differs);
    }
    if edits.is_empty() {
        return Ok((text.to_owned(), 0));
    }
    let document = Document::parse(text).map_err(|e| invalid(e.to_string()))?;
    Ok((document.apply(&edits).map_err(|e| invalid(e.to_string()))?, changed))
}

/// The game's bones the compiler makes from a cloth proxy mesh (`$cloth_…`), as a decompile that
/// lists them as plain bones spells them: every byte other than A–Z, a–z, 0–9 and `_` written `_`
/// (`ability_objects::renamed_by_compiler`). Empty for a skeleton without such names.
pub fn proxy_bones(game_bones: &[String]) -> std::collections::BTreeSet<String> {
    game_bones.iter().filter(|b| deadlock_components::ability_objects::renamed_by_compiler(b)).map(|b| b.bytes().map(|c| if c.is_ascii_alphanumeric() || c == b'_' { c as char } else { '_' }).collect()).collect()
}
/// The key values entries a game's compiled model holds (`m_keyValueText`) for which Source 2
/// Viewer writes no node into the decompiled model, so a rebuild would lack them: the cloth's
/// ground plane (`cloth_ground_plane`, its `trace_raise`) and the older unit status settings
/// (`unit_status_settings`). Each is a plain block of keys, which a `GenericGameData` entry of
/// that class compiles back to.
pub const DROPPED_GAME_DATA: [&str; 2] = ["cloth_ground_plane", "unit_status_settings"];
/// `text` (the vmdl) with each of the game's `DROPPED_GAME_DATA` entries that its `GameDataList`
/// lacks added as a `GenericGameData` entry, its keys copied line for line from the game's compiled
/// key values (`keys`, `DATA m_modelInfo.m_keyValueText`), so that the compiler writes the entry as
/// the game's file holds it (the read-back then compares it as any other entry). Unchanged when the
/// game's key values hold none, or the vmdl already has them. Returns the text and the classes added.
pub fn with_dropped_game_data(text: &str, keys: &str) -> Result<(String, Vec<String>)> {
    let Some(entries) = deadlock_components::write_back::top_entries(keys) else { return Ok((text.to_owned(), vec![])) };
    let mut wanted = vec![];
    for class in DROPPED_GAME_DATA {
        let Some(entry) = entries.iter().find(|e| e.trim_start().split(' ').next() == Some(class)) else { continue };
        // `\tclass = \n\t{\n\t\tkey = value\n\t}\n`: the lines between the braces.
        let lines: Vec<&str> = entry.lines().collect();
        let unknown = || invalid(format!("The game's ability object holds its {class} in a form the app does not know ({entry:?})."));
        let (Some(open), Some(close)) = (lines.iter().position(|l| l.trim() == "{"), lines.iter().rposition(|l| l.trim() == "}")) else { return Err(unknown()) };
        let body: Vec<String> = lines[open + 1..close].iter().map(|l| l.strip_prefix("\t\t").unwrap_or(l).to_owned()).collect();
        if open != 1 || open + 1 >= close || body.iter().any(|l| l.trim().is_empty()) {
            return Err(unknown());
        }
        wanted.push((class, body));
    }
    if wanted.is_empty() {
        return Ok((text.to_owned(), vec![]));
    }
    let document = Document::parse(text).map_err(|e| invalid(e.to_string()))?;
    let lists = document.objects(Some("GameDataList"));
    let [list] = lists.as_slice() else { return Err(invalid("The game's ability object decompiles without exactly one game data list; its game data cannot be kept.")) };
    let Some(children) = list.get("children") else { return Err(invalid("The game's ability object's game data list has no entries.")) };
    let items = children.array().unwrap_or(&[]);
    wanted.retain(|(class, _)| !items.iter().any(|i| i.get("game_class").and_then(|c| c.scalar()) == Some(*class)));
    if wanted.is_empty() {
        return Ok((text.to_owned(), vec![]));
    }
    let tabs = indent_at(text, list.start) + "\t";
    let mut out: String = items.iter().map(|i| format!("{tabs}\t{},\n", &text[i.start..i.end])).collect();
    for (class, body) in &wanted {
        let keys_text: String = body.iter().map(|l| format!("{tabs}\t\t\t{l}\n")).collect();
        out.push_str(&format!("{tabs}\t{{\n{tabs}\t\t_class = \"GenericGameData\"\n{tabs}\t\tname = \"\"\n{tabs}\t\tgame_class = \"{class}\"\n{tabs}\t\tgame_keys = \n{tabs}\t\t{{\n{keys_text}{tabs}\t\t}}\n{tabs}\t}},\n"));
    }
    let replaced = document.apply(&[Edit { start: children.start, end: children.end, replacement: format!("\n{tabs}[\n{out}{tabs}]") }]).map_err(|e| invalid(e.to_string()))?;
    Ok((replaced, wanted.into_iter().map(|(c, _)| c.to_owned()).collect()))
}/// The tabs a vmdl line starts with, at byte `at`.
fn indent_at(text: &str, at: usize) -> String {
    let line = text[..at].rfind('\n').map_or(0, |i| i + 1);
    text[line..].chars().take_while(|c| *c == '\t').collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const GROUND_PLANE: &str = DROPPED_GAME_DATA[0];

    const LISTED: &str = "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:modeldoc29:version{3cec427c-1b0e-4d48-a90a-0436f33a6041} -->\n{\n\trootNode = \n\t{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n\t\t\t{\n\t\t\t\t_class = \"GameDataList\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{\n\t\t\t\t\t\t_class = \"GenericGameData\"\n\t\t\t\t\t\tname = \"\"\n\t\t\t\t\t\tgame_class = \"prop_data\"\n\t\t\t\t\t\tgame_keys = \n\t\t\t\t\t\t{\n\t\t\t\t\t\t\thealth = 100.0\n\t\t\t\t\t\t}\n\t\t\t\t\t},\n\t\t\t\t]\n\t\t\t},\n\t\t]\n\t}\n}\n";
    const KEYS: &str = "<!-- kv3 -->\n{\n\tprop_data = \n\t{\n\t\thealth = 100.0\n\t}\n\tcloth_ground_plane = \n\t{\n\t\ttrace_raise = 26.9\n\t}\n}\n";

    #[test]
    fn the_game_data_the_decompile_drops_goes_into_the_game_data_list() {
        let (out, added) = with_dropped_game_data(LISTED, KEYS).unwrap();
        assert_eq!(added, [GROUND_PLANE]);
        let doc = Document::parse(&out).unwrap();
        let data = doc.objects(Some("GenericGameData"));
        let classes: Vec<&str> = data.iter().filter_map(|n| n.get("game_class").and_then(|c| c.scalar())).collect();
        assert_eq!(classes, ["prop_data", GROUND_PLANE]);
        // Two of them, in the list's order.
        let both = KEYS.replace("}\n}\n", "}\n\tunit_status_settings = \n\t{\n\t\tunit_status_offset = [ 0.0, 0.0, 85.0 ]\n\t\tname = \"default\"\n\t}\n}\n");
        let (two, added) = with_dropped_game_data(LISTED, &both).unwrap();
        assert_eq!(added, [GROUND_PLANE, "unit_status_settings"]);
        assert!(two.contains("\t\t\t\t\t\t\tname = \"default\"\n"), "{two}");
        assert!(out.contains("\t\t\t\t\t\tgame_keys = \n\t\t\t\t\t\t{\n\t\t\t\t\t\t\ttrace_raise = 26.9\n\t\t\t\t\t\t}\n"), "{out}");
        // Already there, or not in the game's key values: unchanged.
        assert_eq!(with_dropped_game_data(&out, KEYS).unwrap(), (out.clone(), vec![]));
        assert_eq!(with_dropped_game_data(LISTED, "<!-- kv3 -->\n{\n\tprop_data = \n\t{\n\t\thealth = 100.0\n\t}\n}\n").unwrap(), (LISTED.to_owned(), vec![]));
        // No game data list to put it in: refused.
        assert!(with_dropped_game_data(&LISTED.replace("GameDataList", "Other"), KEYS).is_err());
    }

    const WITH: &str = "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:modeldoc29:version{3cec427c-1b0e-4d48-a90a-0436f33a6041} -->\n{\n\trootNode = \n\t{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n\t\t\t{\n\t\t\t\t_class = \"Skeleton\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{\n\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\tname = \"tail_0\"\n\t\t\t\t\t\torigin = [ 0.0, 0.0, 1.0 ]\n\t\t\t\t\t\tangles = [ 0.0, 0.0, 0.0 ]\n\t\t\t\t\t\tchildren = \n\t\t\t\t\t\t[\n\t\t\t\t\t\t\t{\n\t\t\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\t\t\tname = \"tail_end\"\n\t\t\t\t\t\t\t\torigin = [ 5.032933, -0.000001, 0.000001 ]\n\t\t\t\t\t\t\t\tangles = [ -0.000008, -0.0, -0.000001 ]\n\t\t\t\t\t\t\t},\n\t\t\t\t\t\t]\n\t\t\t\t\t},\n\t\t\t\t]\n\t\t\t},\n\t\t\t{\n\t\t\t\t_class = \"Softbody\"\n\t\t\t\tchildren = [ ]\n\t\t\t},\n\t\t]\n\t}\n}\n";

    #[test]
    fn every_bone_takes_its_resting_place_from_the_decompile_without_the_cloth() {
        let rest = WITH.replace("[ 5.032933, -0.000001, 0.000001 ]", "[ 5.033058, -0.000012, -0.000002 ]").replace("\t\t\t{\n\t\t\t\t_class = \"Softbody\"\n\t\t\t\tchildren = [ ]\n\t\t\t},\n", "");
        let (out, changed) = rest_from(WITH, &rest, &Default::default()).unwrap();
        assert_eq!(changed, 1);
        assert!(out.contains("origin = [ 5.033058, -0.000012, -0.000002 ]") && !out.contains("5.032933"));
        // Everything else is the decompile with the cloth, the cloth included.
        assert_eq!(out.replace("[ 5.033058, -0.000012, -0.000002 ]", "[ 5.032933, -0.000001, 0.000001 ]"), WITH);
        assert!(out.contains("\"Softbody\""));
        // The same resting places: the text unchanged.
        assert_eq!(rest_from(WITH, WITH, &Default::default()).unwrap(), (WITH.to_owned(), 0));
    }

    #[test]
    fn the_two_decompiles_must_name_the_same_bones() {
        let other = WITH.replace("\"tail_end\"", "\"tail_tip\"");
        assert!(rest_from(WITH, &other, &Default::default()).unwrap_err().to_string().contains("other bones without its cloth"));
        let twice = WITH.replace("\"tail_end\"", "\"tail_0\"");
        assert!(rest_from(&twice, WITH, &Default::default()).unwrap_err().to_string().contains("twice"));
    }
}
