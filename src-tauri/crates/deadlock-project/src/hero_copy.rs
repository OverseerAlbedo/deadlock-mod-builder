//! "Make a copy for another hero": a new project for the same character on another hero.
//!
//! The copy keeps every choice that belongs to the character (the model and gun files, the
//! body roles and moving parts on the source's bones, material and color choices of the
//! replacement's own materials, face choices, physics fine-tuning, item icons, the fit and
//! output preferences). What names the old hero's files is not carried (sounds, effects and
//! their edits, ability objects, hero icons, animation clips, the game hero's own colors, equipment choices on
//! the hero's meshes, a tested cloth setup), and what is made for the hero is made again by
//! the copy's first preparation (the joint matches, the mesh mapping, the gun fit, the
//! prepared model and its reports, the VPK slot).
//!
//! The tables below decide each field. `copy_for_hero` takes the source apart without `..`,
//! so a field added to `Project` or `Customization` stops the build until it is classed here,
//! and the tests compare the tables with the serialized fields. A field in doubt is not
//! carried. The original project is never changed.
use crate::{Customization, PreparationOptions, Project};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// How a field reaches the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Carry {
    /// Carried as it is.
    Carried,
    /// Made again for the new hero (left empty or default; the first preparation fills it).
    Remade,
    /// Not carried: it names the old hero's files.
    Dropped,
    /// Set for the copy itself (identity, name, hero).
    Own,
}
use Carry::*;

/// Every field of `Project`, in declaration order.
pub const PROJECT_FIELDS: &[(&str, Carry)] = &[
    ("model_input_mode", Carried),
    ("preparation_source", Carried),
    ("preparation_blend", Remade),
    ("preparation_report", Remade),
    // Key by key, see PREPARATION_KEYS.
    ("preparation_options", Carried),
    // Field by field, see CUSTOMIZATION_FIELDS.
    ("customization", Carried),
    ("schema", Carried),
    ("id", Own),
    ("name", Own),
    ("hero_name", Own),
    ("original_model", Own),
    ("dmx", Remade),
    // Carried only when the user chose it (not the prepared model's own textures folder).
    ("textures", Carried),
    ("source_root", Remade),
    ("imported_source", Remade),
    ("original_compiled", Remade),
    ("source_overlay", Remade),
    ("mesh_template", Remade),
    ("material_mapping", Remade),
    ("material_remap", Remade),
    ("skeleton_reference", Remade),
    ("expected_dmx_sha256", Remade),
    ("meshes", Remade),
    // Moved to the new hero's material folder; the first preparation refreshes them.
    ("materials", Carried),
    ("lod_strategy", Carried),
    ("ag2_mode", Carried),
    ("animation_policy", Carried),
    ("vpk_number", Remade),
    ("auto_decompile", Carried),
    ("auto_map", Carried),
    ("generate_materials", Carried),
    ("compile_model", Carried),
    ("build_vpk", Carried),
    ("install", Carried),
    ("review_lods", Remade),
    ("allow_unskinned", Remade),
    ("auto_split", Carried),
    ("split_target", Carried),
    ("copied_report", Own),
];

/// Every field of `Customization` (`extensions` holds keys this version does not know).
pub const CUSTOMIZATION_FIELDS: &[(&str, Carry)] = &[
    ("schema", Carried),
    ("enabled", Carried),
    ("model_enabled", Carried),
    ("hero_path", Dropped),
    ("icons", Dropped),
    ("item_icons", Carried),
    // Every animation row names the old hero's clips: compiled files, clips made
    // from an animation (`made`) and clips taken from another hero (`taken`) alike.
    ("animations", Dropped),
    ("effects", Dropped),
    ("audio", Dropped),
    ("clothing", Carried),
    ("physics", Dropped),
    ("cloth_setup", Dropped),
    ("secondary_motion", Carried),
    ("appearance", Carried),
    ("equipment", Dropped),
    ("face", Carried),
    ("face_layers", Carried),
    ("hero_materials", Dropped),
    ("effect_edits", Dropped),
    // Each row replaces one of the old hero's ability models.
    ("ability_objects", Dropped),
    // Item names are the same for every hero.
    ("item_names", Carried),
    ("extensions", Dropped),
];

/// Preparation choices (`PreparationOptions` is an open map); keys not listed are not carried.
pub const PREPARATION_KEYS: &[(&str, Carry)] = &[
    ("source_roles", Carried),
    ("landmarks", Carried),
    ("scene", Carried),
    ("weapon_source", Carried),
    ("physics_roots", Carried),
    ("physics_offered", Carried),
    ("physics_kinds", Carried),
    ("physics_springs", Carried),
    ("preserve_physics_bones", Carried),
    ("automatic_secondary_motion", Carried),
    ("breast_motion", Carried),
    ("butt_motion", Carried),
    ("shape_key_mode", Carried),
    ("fit_mode", Carried),
    ("fit_arm_joints", Carried),
    ("fit_finger_curl", Carried),
    ("fit_floor", Carried),
    ("fit_leg_joints", Carried),
    ("arm_skinning", Carried),
    ("influence_limit", Carried),
    ("target_roles", Remade),
    ("mapping", Remade),
    ("mapping_anchors", Remade),
    ("follow_motion_reviewed", Remade),
    ("reviewed", Remade),
    ("complete_mapping", Remade),
    ("weapon", Remade),
];

fn class(table: &[(&str, Carry)], name: &str) -> Carry {
    table.iter().find(|(key, _)| *key == name).map(|(_, carry)| *carry).unwrap_or(Dropped)
}

/// What the copy says it did, in the words the page shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CopyReport {
    /// Carried over.
    pub carried: Vec<String>,
    /// Not carried, because they belong to the old hero.
    pub not_carried: Vec<String>,
    /// Made again for the new hero.
    pub remade: Vec<String>,
    /// The hero the original project is for (for the notice's heading).
    pub from_hero: String,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeroCopy {
    pub project: Project,
    pub report: CopyReport,
}

/// The folder a hero's prepared materials live in.
fn prepared_folder(hero_model: &str) -> Option<String> {
    let root = hero_model.rsplit_once('/').map(|(folder, _)| folder)?;
    Some(format!("{root}/materials/prepared/"))
}
/// A prepared material path of `from_model`'s folder moved to `to_model`'s folder (the
/// readable name and the 8-digit identity stay); other paths are returned unchanged.
pub fn moved_material(path: &str, from_model: &str, to_model: &str) -> String {
    match (prepared_folder(from_model), prepared_folder(to_model)) {
        (Some(from), Some(to)) => path.strip_prefix(&from).map(|name| format!("{to}{name}")).unwrap_or_else(|| path.to_owned()),
        _ => path.to_owned(),
    }
}

/// A project name with the hero in it: letters, numbers, spaces and underscores, at most 41.
pub fn copy_name(name: &str, hero: &str) -> String {
    let clean = |text: &str| -> String {
        let mapped: String = text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == ' ' { c } else { ' ' }).collect();
        mapped.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let hero = clean(hero);
    let hero = if hero.is_empty() { "copy".to_owned() } else { hero };
    let mut base = clean(name);
    if !base.starts_with(|c: char| c.is_ascii_alphabetic()) { base = format!("Character {base}").trim().to_owned(); }
    let room = 41usize.saturating_sub(hero.len() + 1).max(1);
    let base: String = base.chars().take(room).collect::<String>().trim_end().to_owned();
    let joined = format!("{base} {hero}");
    joined.chars().take(41).collect::<String>().trim_end().to_owned()
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}
fn rows<T>(value: &Option<Vec<T>>) -> usize {
    value.as_ref().map_or(0, Vec::len)
}
fn entries(options: &PreparationOptions, key: &str) -> usize {
    match options.values.get(key) {
        Some(Value::Object(map)) => map.len(),
        Some(Value::Array(list)) => list.len(),
        Some(Value::Null) | None => 0,
        Some(_) => 1,
    }
}

/// The copy of `source` for the hero `hero_name` (`hero_path` is its model). `original_source`:
/// the user's own model file the source project was prepared from, when it is known (the
/// project itself may point at a staged copy).
pub fn copy_for_hero(source: &Project, hero_name: &str, hero_path: &str, original_source: Option<String>) -> HeroCopy {
    let Project {
        model_input_mode, preparation_source, preparation_blend: _, preparation_report, preparation_options,
        customization, schema, id: _, name, hero_name: from_hero, original_model: from_model, dmx,
        textures, source_root: _, imported_source: _, original_compiled: _, source_overlay: _,
        mesh_template: _, material_mapping: _, material_remap: _, skeleton_reference: _,
        expected_dmx_sha256: _, meshes, materials, lod_strategy, ag2_mode, animation_policy,
        vpk_number: _, auto_decompile, auto_map, generate_materials, compile_model, build_vpk,
        install, review_lods: _, allow_unskinned: _, auto_split, split_target, copied_report: _,
        resolved_source: _, resolved_textures: _, rebase_log: _,
    } = source.clone();
    let Customization {
        schema: customization_schema, enabled, model_enabled, hero_path: _, icons, item_icons,
        animations, effects, audio, clothing, physics, cloth_setup, secondary_motion, appearance,
        equipment, face, face_layers, hero_materials, effect_edits, ability_objects, item_names, extensions,
    } = customization;
    let mut report = CopyReport { from_hero: if from_hero.is_empty() { "the original hero".into() } else { from_hero.clone() }, ..Default::default() };
    let carried = |report: &mut CopyReport, text: String| report.carried.push(text);
    let dropped = |report: &mut CopyReport, text: String| report.not_carried.push(text);

    // The source model: the user's own file when the project points at a staged copy.
    let model_file = original_source.filter(|path| !path.is_empty() && Path::new(path).is_file()).unwrap_or(preparation_source);
    if !model_file.is_empty() { carried(&mut report, "Your model file".into()); }
    else if !dmx.is_empty() { dropped(&mut report, "The ready-made DMX (it was made for this hero's skeleton): choose your model file again".into()); }

    // Preparation choices, key by key.
    let mut options = PreparationOptions::default();
    for (key, value) in &preparation_options.values {
        if class(PREPARATION_KEYS, key) == Carried { options.values.insert(key.clone(), value.clone()); }
    }
    if options.values.get("weapon_source").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) { carried(&mut report, "Your gun file".into()); }
    let roles = entries(&preparation_options, "source_roles");
    if roles > 0 { carried(&mut report, format!("Body roles on your model's bones ({roles})")); }
    let parts = entries(&preparation_options, "physics_roots");
    if parts > 0 { carried(&mut report, format!("Moving parts ({})", plural(parts, "group", "groups"))); }
    if ["fit_mode", "fit_floor", "fit_leg_joints", "fit_arm_joints", "fit_finger_curl", "arm_skinning", "shape_key_mode"].iter().any(|k| options.values.contains_key(*k)) {
        carried(&mut report, "Fit and shape key choices".into());
    }
    if options.values.contains_key("scene") { carried(&mut report, "Scene object choices".into()); }

    // Materials: their choices move to the new hero's material folder by their identity.
    let materials: std::collections::BTreeMap<_, _> = materials.into_iter().map(|(path, choice)| (moved_material(&path, &from_model, hero_path), choice)).collect();
    if !materials.is_empty() { carried(&mut report, format!("Material images and settings ({})", plural(materials.len(), "material", "materials"))); }
    let appearance = appearance.map(|mut value| {
        if let Some(rows) = &mut value.materials {
            for row in rows.iter_mut() {
                if let Some(target) = &mut row.target { *target = moved_material(target, &from_model, hero_path); }
            }
        }
        value
    });
    let surface_rows = appearance.as_ref().map_or(0, |a| rows(&a.materials));
    if surface_rows > 0 { carried(&mut report, format!("Color and surface changes of your model's materials ({surface_rows})")); }
    // A texture folder the user chose (not the prepared model's own).
    let prepared_textures = Path::new(&dmx).parent().map(|folder| folder.join("textures"));
    let textures = if !textures.is_empty() && !preparation_report.is_empty() && prepared_textures.as_deref() == Some(Path::new(&textures)) { String::new() } else { textures };
    if !textures.is_empty() { carried(&mut report, "Texture folder".into()); }
    if face.is_some() || face_layers.is_some() { carried(&mut report, "Face expression and face details".into()); }
    if secondary_motion.as_ref().is_some_and(|m| m.tuning.as_ref().is_some_and(|t| !t.is_empty()) || m.preset.is_some()) {
        carried(&mut report, "Physics fine-tuning and preset".into());
    }
    let items = rows(&item_icons);
    if items > 0 { carried(&mut report, plural(items, "item icon", "item icons")); }
    let renamed = rows(&item_names);
    if renamed > 0 { carried(&mut report, plural(renamed, "renamed item", "renamed items")); }
    if clothing.as_ref().is_some_and(|c| c.source.as_deref().is_some_and(|s| !s.is_empty())) { carried(&mut report, "Clothing file".into()); }
    carried(&mut report, "Output and detail-level settings".into());

    // Named for the old hero's files.
    for (count, one, many) in [
        (rows(&audio), "sound", "sounds"),
        (rows(&effects), "effect file", "effect files"),
        (rows(&effect_edits), "effect edit", "effect edits"),
        (rows(&ability_objects), "ability object", "ability objects"),
        (rows(&icons), "hero icon", "hero icons"),
        (rows(&animations), "animation clip", "animation clips"),
        (hero_materials.as_ref().map_or(0, |h| rows(&h.materials)), "hero color", "hero colors"),
    ] {
        if count > 0 { dropped(&mut report, plural(count, one, many)); }
    }
    if let Some(e) = &equipment {
        let count = rows(&e.meshes) + rows(&e.props) + rows(&e.variants);
        if count > 0 { dropped(&mut report, format!("Equipment choices on the hero's meshes and bones ({count})")); }
    }
    if physics.as_ref().is_some_and(|p| p.source.as_deref().is_some_and(|s| !s.is_empty())) { dropped(&mut report, "The tested cloth setup".into()); }
    if cloth_setup.as_ref().is_some_and(|c| rows(&c.meshes) > 0) { dropped(&mut report, "The cloth setup's garment choice".into()); }
    if preparation_options.values.get("weapon").is_some_and(|w| !w.is_null()) { dropped(&mut report, "The gun fit".into()); }
    if !extensions.is_empty() { dropped(&mut report, "Settings this version does not know".into()); }

    // Made again by the copy's first preparation.
    if entries(&preparation_options, "mapping") > 0 { report.remade.push("Joint matches".into()); }
    if !meshes.is_empty() { report.remade.push("Mesh mapping".into()); }
    if !dmx.is_empty() { report.remade.push("The prepared model".into()); }
    report.remade.push("VPK slot".into());

    let base = Project::default();
    let project = Project {
        model_input_mode,
        preparation_source: model_file,
        preparation_options: options,
        customization: Customization {
            schema: customization_schema, enabled, model_enabled, item_icons, item_names, clothing, secondary_motion,
            appearance, face, face_layers, ..Customization::default()
        },
        schema,
        name: copy_name(&name, hero_name),
        hero_name: hero_name.to_owned(),
        original_model: hero_path.to_owned(),
        textures,
        materials,
        lod_strategy, ag2_mode, animation_policy, auto_decompile, auto_map, generate_materials,
        compile_model, build_vpk, install, auto_split, split_target,
        copied_report: preparation_report,
        ..base
    };
    HeroCopy { project, report }
}
