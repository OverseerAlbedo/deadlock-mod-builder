use crate::{Result, fail, heroes};
use deadlock_core::{safety::relative_asset, text::casefold};
use deadlock_project::Project;
use deadlock_vpk::Vpk;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Variant {
    pub path: String,
    pub target: String,
    pub label: String,
    pub hero: String,
    pub supported: bool,
    pub requires_model_project: bool,
    /// "form" for alternate hero forms, "weapon" for a separate weapon model.
    #[serde(default = "form", skip_serializing_if = "is_form")]
    pub kind: String,
}
fn form() -> String {
    "form".into()
}
fn is_form(kind: &str) -> bool {
    kind == "form"
}
const WEAPON_WORDS: [&str; 12] = ["weapon", "gun", "pistol", "rifle", "shotgun", "revolver", "smg", "blaster", "launcher", "cannon", "wpn", "firearm"];
pub fn weapon_like(path: &str) -> bool {
    let name = casefold(path);
    let file = name.rsplit('/').next().unwrap_or(&name);
    WEAPON_WORDS.iter().any(|w| file.contains(w)) || name.contains("/weapons/")
}
/// Separate weapon models that probably belong to this hero, best match first.
pub fn discover_weapons(archive: &Vpk, original_model: &str) -> Result<Vec<Variant>> {
    let current = casefold(&model(original_model)?);
    let folder = current.rsplit_once('/').map(|(f, _)| f.to_string()).unwrap_or_default();
    let codename = current.rsplit('/').next().unwrap_or_default().trim_end_matches(".vmdl").to_string();
    let hero = heroes::roster()?
        .into_iter()
        .find(|h| casefold(&h.path) == current || h.extras.iter().any(|e| casefold(&e.path) == current))
        .map(|h| h.name)
        .unwrap_or_default();
    let siblings: std::collections::BTreeSet<String> = heroes::roster()?
        .into_iter()
        .flat_map(|h| std::iter::once(h.path).chain(h.extras.into_iter().map(|e| e.path)))
        .map(|p| casefold(&p))
        .collect();
    let mut ranked = vec![];
    for (key, entry) in &archive.entries {
        if !key.ends_with(".vmdl_c") {
            continue;
        }
        let path = &entry.path[..entry.path.len() - 2];
        let folded = casefold(path);
        if folded == current || siblings.contains(&folded) || !weapon_like(&folded) {
            continue;
        }
        let rank = if !folder.is_empty() && folded.starts_with(&(folder.clone() + "/")) {
            0
        } else if folded.contains("/weapons/") && !codename.is_empty() && folded.contains(&codename) {
            1
        } else if !codename.is_empty() && folded.contains(&codename) {
            2
        } else {
            continue;
        };
        ranked.push((rank, path.to_string()));
    }
    ranked.sort();
    Ok(ranked
        .into_iter()
        .take(40)
        .map(|(_, path)| Variant {
            label: format!("Weapon model: {}", path.rsplit('/').next().unwrap_or(&path)),
            target: path.clone(),
            path,
            hero: hero.clone(),
            supported: true,
            requires_model_project: true,
            kind: "weapon".into(),
        })
        .collect())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariantEligibility {
    pub hero_path: String,
    pub variant_path: String,
    pub project_id: String,
    pub requires_full_model_validation: bool,
}
fn model(path: &str) -> Result<String> {
    let path = relative_asset(path)?;
    let value = if casefold(&path).ends_with(".vmdl_c") {
        path[..path.len() - 2].to_owned()
    } else {
        path
    };
    if !casefold(&value).ends_with(".vmdl") {
        return Err(fail(
            "Alternate forms require an original VMDL model target.",
        ));
    }
    Ok(value)
}
pub fn discover_variants(archive: &Vpk, original_model: &str) -> Result<Vec<Variant>> {
    let current = casefold(&model(original_model)?);
    let mut result = vec![];
    for hero in heroes::roster()? {
        let mut members = vec![heroes::Member {
            name: hero.name.clone(),
            path: hero.path,
        }];
        members.extend(hero.extras);
        if !members.iter().any(|m| casefold(&m.path) == current) {
            continue;
        }
        for member in members {
            if casefold(&member.path) == current {
                continue;
            }
            if let Some(entry) = archive.entries.get(&casefold(&(member.path + "_c"))) {
                let path = entry.path[..entry.path.len() - 2].to_owned();
                result.push(Variant {
                    target: path.clone(),
                    path,
                    label: member.name,
                    hero: hero.name.clone(),
                    supported: true,
                    requires_model_project: true,
                    kind: "form".into(),
                });
            }
        }
        break;
    }
    Ok(result)
}
/// Intent eligibility only. The caller must run the child through the full model pipeline.
pub fn validate_variant_project(
    parent: &Project,
    child: &Project,
    archive: &Vpk,
) -> Result<VariantEligibility> {
    parent.validate(true).map_err(fail)?;
    child.validate(true).map_err(fail)?;
    let allowed = discover_variants(archive, &parent.original_model)?;
    let target = model(&child.original_model).unwrap_or_default();
    let sibling = allowed.iter().any(|v| casefold(&v.target) == casefold(&target));
    // A separate weapon model may also be bundled when it is prepared as a
    // weapon project and exists in this game.
    let weapon = child.model_input_mode == deadlock_project::ModelInputMode::Weapon
        && weapon_like(&target)
        && archive.entries.contains_key(&casefold(&(target.clone() + "_c")));
    if !sibling && !weapon {
        return Err(fail(
            "Alternate-form project must target a registered sibling of the selected hero, or a weapon model prepared as a weapon project, present in this game.",
        ));
    }
    if child.id == parent.id {
        return Err(fail(
            "Alternate-form project must have its own workspace identity.",
        ));
    }
    let value = &child.customization;
    if !value.model_enabled() {
        return Err(fail(
            "Alternate-form project must include a replacement model.",
        ));
    }
    if value.active()
        && value.equipment.as_ref().is_some_and(|equipment| {
            equipment.enabled.unwrap_or(false)
                && equipment
                    .variants
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|v| v.enabled.unwrap_or(true))
        })
    {
        return Err(fail(
            "Alternate-form projects cannot recursively include other forms.",
        ));
    }
    if child.dmx.is_empty() && child.preparation_source.is_empty() {
        return Err(fail(
            "Prepare the alternate form or choose its replacement DMX before bundling it.",
        ));
    }
    Ok(VariantEligibility {
        hero_path: parent.original_model.clone(),
        variant_path: child.original_model.clone(),
        project_id: child.id.clone(),
        requires_full_model_validation: true,
    })
}
