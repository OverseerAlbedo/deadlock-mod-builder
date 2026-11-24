//! Carried objects of a hero model: parts the game shows only while the hero
//! carries something (a turret being placed, a canister in the hand), bound to
//! bones of their own. Build & install's suggested mapping keeps the game's own
//! model for them instead of disabling them. Decided only from the game's
//! data: the model's body groups and the skin weights of its decompiled meshes.
//! The same read also marks the parts of the hero's gun (most of their weight
//! on weapon bones, whatever their name), which the suggestion lets follow
//! the gun.
use deadlock_core::{safety, CancellationToken, CoreError, Result};
use deadlock_model::{lod_base, Analysis};
use deadlock_resource::text::{Document, Node};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Share of a switched part's skin weight on bones that no default-shown,
/// non-gun part uses that makes it a carried object.
pub const CARRIED_SHARE: f64 = 0.85;
/// Share of a part's skin weight on weapon bones that makes it a gun
/// (`deadlock-preparation weapon.rs`; the hero's 3D view uses the same share).
pub const GUN_SHARE: f64 = 0.85;

fn scalar(node: &Node, key: &str) -> Option<String> {
    node.get(key).and_then(|v| v.scalar()).map(str::to_owned)
}
fn strings(node: Option<&Node>) -> Vec<String> {
    node.and_then(|n| n.array()).map(|a| a.iter().filter_map(|v| v.scalar().map(str::to_owned)).collect()).unwrap_or_default()
}

/// Per body group, the meshes its first choice shows and the meshes only
/// its other choices show (the game starts every group on its first choice).
pub fn body_groups(document: &Document) -> Vec<(String, BTreeSet<String>, BTreeMap<String, String>)> {
    let mut out = vec![];
    for group in document.objects(Some("BodyGroup")) {
        let name = scalar(group, "name").unwrap_or_default();
        let choices: Vec<&Node> = group
            .get("children")
            .and_then(|c| c.array())
            .map(|a| a.iter().filter(|c| c.get("_class").and_then(|v| v.scalar()) == Some("BodyGroupChoice")).collect())
            .unwrap_or_default();
        let shown: BTreeSet<String> = choices.first().map(|c| strings(c.get("meshes")).into_iter().collect()).unwrap_or_default();
        let mut other = BTreeMap::new();
        for choice in choices.iter().skip(1) {
            let label = scalar(choice, "name").unwrap_or_default();
            for mesh in strings(choice.get("meshes")) {
                if !shown.contains(&mesh) {
                    other.entry(mesh).or_insert_with(|| label.clone());
                }
            }
        }
        out.push((name, shown, other));
    }
    out
}

/// Whether a render mesh's `import_filter` keeps the DMX mesh `source_name`
/// (several render meshes can share one DMX), as `deadlock-preparation
/// weapon.rs` reads it: `exclude_by_default` with an `exception_list`.
pub fn filter_keeps(filter: Option<&Node>, source_name: &str) -> bool {
    let Some(filter) = filter else { return true };
    let exclude_by_default = filter.get("exclude_by_default").and_then(|v| v.scalar()) == Some("true");
    let listed = strings(filter.get("exception_list")).iter().any(|n| n == source_name);
    exclude_by_default == listed
}

/// Switched parts: base render nodes (detail levels follow their base) that
/// a later choice of a body group shows and no group's first choice shows,
/// with the body group's name.
pub fn switched(document: &Document) -> BTreeMap<String, String> {
    let groups = body_groups(document);
    let shown: BTreeSet<String> = groups.iter().flat_map(|g| g.1.iter().map(|n| lod_base(n))).collect();
    let mut out = BTreeMap::new();
    for (group, _, other) in &groups {
        for node in other.keys() {
            let base = lod_base(node);
            if !shown.contains(&base) {
                out.entry(base).or_insert_with(|| group.clone());
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// On bones of its own: the game's model stays.
    Carried,
    /// On bones the body uses: worn on the body.
    Worn,
    /// Mostly on weapon bones: a gun, left to the gun rule.
    Gun,
    /// No skin weight could be read.
    NoWeights,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Part {
    pub node: String,
    pub group: String,
    /// Bones with skin weight.
    pub bones: usize,
    /// Share of its weight on bones no default-shown, non-gun part uses.
    pub own_share: f64,
    pub verdict: Verdict,
}

fn total(weights: &BTreeMap<String, f64>) -> f64 {
    weights.values().sum()
}
fn is_gun(weights: &BTreeMap<String, f64>, weapon: &BTreeSet<String>) -> bool {
    let all = total(weights);
    all > 0. && weights.iter().filter(|(n, _)| weapon.contains(*n)).map(|(_, w)| w).sum::<f64>() / all >= GUN_SHARE
}

/// The rule. `switched` maps base nodes to their body group, `weights` holds
/// the skin weight per bone of every base node, `weapon` the weapon bones.
/// The body is every base node that is not switched and not a gun.
pub fn classify(
    switched: &BTreeMap<String, String>,
    weights: &BTreeMap<String, BTreeMap<String, f64>>,
    weapon: &BTreeSet<String>,
) -> Vec<Part> {
    let body: BTreeSet<&str> = weights
        .iter()
        .filter(|(node, w)| !switched.contains_key(*node) && !is_gun(w, weapon))
        .flat_map(|(_, w)| w.iter().filter(|(_, v)| **v > 0.).map(|(b, _)| b.as_str()))
        .collect();
    switched
        .iter()
        .map(|(node, group)| {
            let empty = BTreeMap::new();
            let w = weights.get(node).unwrap_or(&empty);
            let all = total(w);
            let own: f64 = w.iter().filter(|(b, _)| !body.contains(b.as_str())).map(|(_, v)| v).sum();
            // An empty sum is -0.0; keep the share at plain 0 then.
            let own_share = if all > 0. && own > 0. { own / all } else { 0. };
            let verdict = if all <= 0. {
                Verdict::NoWeights
            } else if is_gun(w, weapon) {
                Verdict::Gun
            } else if own_share >= CARRIED_SHARE {
                Verdict::Carried
            } else {
                Verdict::Worn
            };
            Part { node: node.clone(), group: group.clone(), bones: w.values().filter(|v| **v > 0.).count(), own_share, verdict }
        })
        .collect()
}

/// The switched parts of the hero model `text` with their verdicts, reading
/// the base nodes' meshes from the decompiled sources `base`. Empty, without
/// reading any mesh, when the model has no switched part.
pub fn parts(base: &Path, text: &str, cancel: &CancellationToken) -> Result<Vec<Part>> {
    let document = Document::parse(text).map_err(|e| CoreError::Integrity(e.to_string()))?;
    let switched = switched(&document);
    if switched.is_empty() {
        return Ok(vec![]);
    }
    let (weights, weapon) = node_weights(base, &document, cancel)?;
    Ok(classify(&switched, &weights, &weapon))
}

/// The skin weight per bone of every base render node of `document`, and the
/// weapon bones of its skeleton.
pub fn node_weights(
    base: &Path,
    document: &Document,
    cancel: &CancellationToken,
) -> Result<(BTreeMap<String, BTreeMap<String, f64>>, BTreeSet<String>)> {
    let mut files: BTreeMap<String, deadlock_skeleton::skinning::BoneWeights> = BTreeMap::new();
    let mut weights: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for node in document.objects(Some("RenderMeshFile")) {
        let (Some(name), Some(file)) = (scalar(node, "name"), scalar(node, "filename")) else { continue };
        if lod_base(&name) != name {
            continue;
        }
        cancel.check()?;
        if !files.contains_key(&file) {
            let dmx = deadlock_dmx::Dmx::load_with_cancel(&safety::inside(base, &file)?, cancel).map_err(deadlock_dmx::DmxError::into_core)?;
            let read = deadlock_skeleton::skinning::bone_weights(&dmx).map_err(|e| CoreError::Integrity(e.to_string()))?;
            files.insert(file.clone(), read);
        }
        let totals = weights.entry(name).or_default();
        for (mesh, w) in &files[&file].meshes {
            if filter_keeps(node.get("import_filter"), mesh) {
                for (bone, v) in w {
                    *totals.entry(bone.clone()).or_default() += v;
                }
            }
        }
    }
    let mut parents: BTreeMap<String, Option<String>> = BTreeMap::new();
    for read in files.values() {
        for (bone, parent) in &read.parents {
            parents.entry(bone.clone()).or_insert_with(|| parent.clone());
        }
    }
    let bones: Vec<deadlock_preparation::algorithms::Bone> = parents
        .into_iter()
        .map(|(name, parent)| deadlock_preparation::algorithms::Bone { name, parent, position: [0.; 3], details: Default::default() })
        .collect();
    Ok((weights, deadlock_preparation::weapon::weapon_bones(&bones)))
}

/// Parts of the hero's gun: base nodes, switched or shown by default, with at
/// least `GUN_SHARE` of their skin weight on weapon bones, whatever their
/// name (the same share `deadlock-preparation weapon.rs` uses to find the
/// gun meshes a fitted gun replaces).
pub fn gun_parts(weights: &BTreeMap<String, BTreeMap<String, f64>>, weapon: &BTreeSet<String>) -> BTreeSet<String> {
    weights.iter().filter(|(_, w)| is_gun(w, weapon)).map(|(node, _)| node.clone()).collect()
}

/// The carried parts and the gun parts of the hero model `text`, from one
/// read of its base nodes' meshes.
pub fn marks(base: &Path, text: &str, cancel: &CancellationToken) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let document = Document::parse(text).map_err(|e| CoreError::Integrity(e.to_string()))?;
    let (weights, weapon) = node_weights(base, &document, cancel)?;
    let switched = switched(&document);
    let carried = classify(&switched, &weights, &weapon).into_iter().filter(|p| p.verdict == Verdict::Carried).map(|p| p.node).collect();
    Ok((carried, gun_parts(&weights, &weapon)))
}

/// Sets both flags on `analysis` from base node names (detail levels follow
/// their base).
pub fn apply(analysis: &mut Analysis, carried: &BTreeSet<String>, gun: &BTreeSet<String>) {
    for mesh in &mut analysis.meshes {
        let base = lod_base(&mesh.name);
        mesh.carried = carried.contains(&base);
        mesh.gun_part = gun.contains(&base);
    }
}

/// Marks the carried parts and the gun parts of `analysis` (detail levels
/// follow their base). A read failure marks nothing; only a cancellation is
/// passed on.
pub fn mark(analysis: &mut Analysis, base: &Path, text: &str, cancel: &CancellationToken) -> Result<()> {
    let (carried, gun) = match marks(base, text, cancel) {
        Ok(found) => found,
        Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
        Err(_) => return Ok(()),
    };
    apply(analysis, &carried, &gun);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn w(rows: &[(&str, f64)]) -> BTreeMap<String, f64> {
        rows.iter().map(|(b, v)| ((*b).to_owned(), *v)).collect()
    }
    fn setup(part: BTreeMap<String, f64>) -> Vec<Part> {
        let switched = BTreeMap::from([("held".to_owned(), "held".to_owned())]);
        let weights = BTreeMap::from([
            ("body".to_owned(), w(&[("pelvis", 10.), ("hand_r", 4.), ("arm_l", 3.)])),
            ("gun".to_owned(), w(&[("weapon", 9.), ("hand_r", 0.5)])),
            ("held".to_owned(), part),
        ]);
        classify(&switched, &weights, &BTreeSet::from(["weapon".to_owned()]))
    }
    #[test]
    fn a_switched_part_on_its_own_bones_is_carried() {
        let parts = setup(w(&[("held_root", 8.), ("held_lid", 1.), ("hand_r", 0.5)]));
        assert_eq!(parts[0].verdict, Verdict::Carried);
        assert_eq!(parts[0].bones, 3);
        assert!((parts[0].own_share - 9. / 9.5).abs() < 1e-12);
    }
    #[test]
    fn a_switched_part_on_body_bones_is_worn() {
        let parts = setup(w(&[("arm_l", 6.), ("held_root", 1.)]));
        assert_eq!(parts[0].verdict, Verdict::Worn);
    }
    #[test]
    fn a_gun_is_left_to_the_gun_rule() {
        // Weapon bones count neither as the body's nor as a carried object's.
        assert_eq!(setup(w(&[("weapon", 5.)]))[0].verdict, Verdict::Gun);
        let parts = setup(w(&[("held_root", 1.)]));
        assert_eq!(parts.len(), 1, "the default gun is not a switched part");
    }
    #[test]
    fn a_part_of_the_default_choice_is_not_switched() {
        let text = r#"<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:modeldoc29:version{3cec427c-1b0e-4d48-a90a-0436f33a6041} -->
{
    rootNode = {
        _class = "RootNode"
        children = [
            { _class = "BodyGroupList" children = [
                { _class = "BodyGroup" name = "default" children = [
                    { _class = "BodyGroupChoice" name = "body" meshes = [ "body", "face" ] },
                ] },
                { _class = "BodyGroup" name = "held" children = [
                    { _class = "BodyGroupChoice" name = "held_none" meshes = [ ] },
                    { _class = "BodyGroupChoice" name = "held" meshes = [ "held", "held_lod1", "face" ] },
                ] },
                { _class = "BodyGroup" name = "gun" children = [
                    { _class = "BodyGroupChoice" name = "gun_right_hand" meshes = [ "gun" ] },
                    { _class = "BodyGroupChoice" name = "gun_none" meshes = [ ] },
                ] },
            ] },
        ]
    }
}"#;
        let document = Document::parse(text).unwrap();
        assert_eq!(switched(&document), BTreeMap::from([("held".to_owned(), "held".to_owned())]));
    }
    #[test]
    fn a_model_without_switched_parts_has_no_parts_and_an_unread_mesh_marks_nothing() {
        let text = "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:modeldoc29:version{3cec427c-1b0e-4d48-a90a-0436f33a6041} -->\n{ rootNode = { _class = \"RootNode\" children = [ { _class = \"RenderMeshFile\" name = \"body\" filename = \"missing.dmx\" } ] } }";
        let dir = std::env::temp_dir();
        assert!(parts(&dir, text, &CancellationToken::default()).unwrap().is_empty());
        // Marking reads every base node's mesh; a mesh that cannot be read marks nothing.
        let mut analysis = deadlock_model::analyze(text).unwrap();
        mark(&mut analysis, &dir, text, &CancellationToken::default()).unwrap();
        assert!(analysis.meshes.iter().all(|m| !m.carried && !m.gun_part));
    }
    #[test]
    fn parts_on_weapon_bones_are_gun_parts_whatever_their_name() {
        let weapon = BTreeSet::from(["weapon".to_owned(), "weapon_mag".to_owned()]);
        let weights = BTreeMap::from([
            ("body".to_owned(), w(&[("pelvis", 10.), ("hand_r", 4.), ("weapon", 0.2)])),
            ("gun".to_owned(), w(&[("weapon", 9.), ("hand_r", 0.5)])),
            // A switched piece of ammunition and a magazine shown by default.
            ("ammo_01".to_owned(), w(&[("weapon_mag", 3.)])),
            ("magazine".to_owned(), w(&[("weapon_mag", 8.6), ("hand_r", 1.4)])),
            // Under the share: held in the hand as much as on the gun.
            ("strap".to_owned(), w(&[("weapon", 1.), ("hand_r", 1.)])),
            ("unweighted".to_owned(), BTreeMap::new()),
        ]);
        let found = gun_parts(&weights, &weapon);
        assert_eq!(found, BTreeSet::from(["ammo_01".to_owned(), "gun".to_owned(), "magazine".to_owned()]));
        // The carried verdicts do not change with the new flag.
        let switched = BTreeMap::from([("ammo_01".to_owned(), "weapon".to_owned())]);
        assert_eq!(classify(&switched, &weights, &weapon)[0].verdict, Verdict::Gun);
    }
    #[test]
    fn detail_levels_follow_their_base_for_both_flags() {
        let text = "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:modeldoc29:version{3cec427c-1b0e-4d48-a90a-0436f33a6041} -->\n{ rootNode = { _class = \"RootNode\" children = [ \
            { _class = \"RenderMeshFile\" name = \"body\" filename = \"a.dmx\" }, \
            { _class = \"RenderMeshFile\" name = \"ammo_01\" filename = \"b.dmx\" }, \
            { _class = \"RenderMeshFile\" name = \"ammo_01_lod1\" filename = \"b.dmx\" }, \
            { _class = \"RenderMeshFile\" name = \"canister\" filename = \"c.dmx\" }, \
            { _class = \"RenderMeshFile\" name = \"canister_lod1\" filename = \"c.dmx\" } ] } }";
        let mut analysis = deadlock_model::analyze(text).unwrap();
        apply(&mut analysis, &BTreeSet::from(["canister".to_owned()]), &BTreeSet::from(["ammo_01".to_owned()]));
        let flags: Vec<(&str, bool, bool)> = analysis.meshes.iter().map(|m| (m.name.as_str(), m.carried, m.gun_part)).collect();
        assert_eq!(flags, vec![
            ("body", false, false),
            ("ammo_01", false, true),
            ("ammo_01_lod1", false, true),
            ("canister", true, false),
            ("canister_lod1", true, false),
        ]);
        // Written only when true, so other rows keep their bytes.
        let rows = serde_json::to_value(&analysis.meshes).unwrap();
        assert_eq!(rows[0], serde_json::json!({ "name": "body", "filename": "a.dmx", "lower_lod": false }));
        assert_eq!(rows[1]["gun_part"], serde_json::json!(true));
        assert!(rows[3].get("gun_part").is_none());
    }
}
