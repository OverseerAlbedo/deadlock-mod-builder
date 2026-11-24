//! Readable names and list groups for a hero's animation clips, from the
//! game's own clip names only (the Motion page shows them; nothing here
//! changes a build).
//!
//! A clip name is words joined by `_`: a context (crouch, item, hideout,
//! out of combat, an ability's own name …), an action (run, aim, reload,
//! jump …) and often a direction. Direction suffixes were read from the
//! clips' own root motion (`m_rootMotion.m_totalDelta`), never guessed: on
//! ten heroes, 431 suffixed clips that move agree that `_n` moves along +x
//! (forward), `_s` along −x, `_w` along +y (left), `_e` along −y (right), the
//! diagonals in between, and `_center` does not move; `turn_left` turns about
//! +z, `turn_right` about −z. Ten game clips move another way than their
//! suffix says (two `_nw`/`_w` pairs swapped, two `_se`/`_sw` that move straight
//! back); `describe_moving` names those by their motion.
//!
//! Groups are fixed and decided by the first rule that matches, in this
//! order: hits and deaths (hits taken: `ability_knockback` is the hero thrown
//! by someone's ability), abilities named as such (`ability`, `cast`,
//! `channel…`), menu, shop and sleep screens, weapon actions (a plain `aim`
//! or `throw` with a word of the hero's own and no weapon word is the
//! ability's aim or throw), then any word outside the shared vocabulary (an
//! ability's own name such as `syphon` or `bloodbomb`: the runs and idles of
//! its stance stay with the ability), jumping and falling, movement; what is
//! left (a bind pose, an empty clip, a body-group switch) is "Emotes and
//! other".
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Movement,
    Jumping,
    Weapon,
    Abilities,
    Hits,
    Other,
}
impl Group {
    pub const ALL: [Group; 6] = [Group::Movement, Group::Jumping, Group::Weapon, Group::Abilities, Group::Hits, Group::Other];
    pub fn title(self) -> &'static str {
        match self {
            Group::Movement => "Movement",
            Group::Jumping => "Jumping and falling",
            Group::Weapon => "Weapon",
            Group::Abilities => "Abilities",
            Group::Hits => "Hits and deaths",
            Group::Other => "Emotes and other",
        }
    }
}

/// A direction suffix of a clip name, as its root motion shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Forward,
    ForwardRight,
    Right,
    BackwardRight,
    Backward,
    BackwardLeft,
    Left,
    ForwardLeft,
    InPlace,
}
impl Direction {
    pub fn words(self) -> &'static str {
        match self {
            Direction::Forward => "forward",
            Direction::ForwardRight => "forward right",
            Direction::Right => "right",
            Direction::BackwardRight => "backward right",
            Direction::Backward => "backward",
            Direction::BackwardLeft => "backward left",
            Direction::Left => "left",
            Direction::ForwardLeft => "forward left",
            Direction::InPlace => "in place",
        }
    }
    fn from_suffix(word: &str) -> Option<Self> {
        Some(match word {
            "n" => Direction::Forward,
            "ne" => Direction::ForwardRight,
            "e" => Direction::Right,
            "se" => Direction::BackwardRight,
            "s" => Direction::Backward,
            "sw" => Direction::BackwardLeft,
            "w" => Direction::Left,
            "nw" => Direction::ForwardLeft,
            "center" | "runcenter" => Direction::InPlace,
            _ => return None,
        })
    }
    /// The direction a root motion displacement (x forward, y left) shows,
    /// None below `min` inches.
    pub fn from_motion(x: f64, y: f64, min: f64) -> Option<Self> {
        if x.hypot(y) < min {
            return Some(Direction::InPlace);
        }
        let octant = ((y.atan2(x).to_degrees() + 360. + 22.5) % 360. / 45.) as usize;
        // Counter-clockwise from +x: forward, forward left, left, …
        Some([Direction::Forward, Direction::ForwardLeft, Direction::Left, Direction::BackwardLeft, Direction::Backward, Direction::BackwardRight, Direction::Right, Direction::ForwardRight][octant % 8])
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClipName {
    pub name: String,
    pub group: Group,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    /// A copy the compiler derived from another clip (`x.vnmclip+non_additive`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub derived: bool,
}

const HITS: &[&str] = &["flinch", "knockdown", "knockback", "death", "stun", "stunned", "getup", "stagger"];
const OTHER: &[&str] = &["ui", "emote", "shop", "shopping", "sleep", "bindpose", "menu", "matchmaking", "respawn", "fidget", "bodygroup"];
const ABILITY: &[&str] = &["ability", "cast", "channel", "channeling", "precast", "ult"];
const WEAPON: &[&str] = &["aim", "shoot", "fire", "reload", "melee", "parry", "holster", "unholster", "zoomed", "throw", "kick", "ammo"];
/// Weapon words that also name an ability's own action (its aim, its throw).
const ABILITY_ACTIONS: &[&str] = &["aim", "throw"];
/// Words that say the gun, the melee weapon or an item is in hand.
const WEAPON_CONTEXT: &[&str] = &["weapon", "primary", "crossbow", "gun", "guns", "item"];
const JUMPING: &[&str] = &["jump", "air", "inair", "landing", "apex", "catapult", "airdash", "fall"];
const MOVEMENT: &[&str] = &[
    "run", "walk", "idle", "stand", "crouch", "crouching", "turn", "slide", "dash", "lean", "locolean", "mantle", "climb", "rope", "zipline", "wall", "lift",
    "stop", "start", "loop", "end", "settle", "attach", "attached", "tether", "tethered", "reeling", "awaiting", "corner", "hover", "glide",
];
/// Context and filler words every hero uses; anything else is a word of the
/// hero's own (most often an ability's name).
const SHARED: &[&str] = &[
    "weapon", "primary", "item", "combat", "of", "out", "outofcombat", "ooc", "in", "to", "on", "off", "and", "hideout", "invis", "vis", "quick", "large",
    "medium", "small", "high", "low", "mid", "up", "down", "dwn", "upwards", "above", "forward", "foward", "back", "backward", "left", "right", "l", "r",
    "loco", "ground", "boosted", "forced", "still", "pose", "alt", "generic", "intro", "success", "transition", "recover", "get", "looping", "additive",
    "add", "both", "guns", "gun", "hand", "latched", "hero", "main", "select", "info", "base", "put", "crossbow", "one", "two", "three", "round", "hit",
    "impact", "self", "null", "animation", "haspath",
];

fn has(words: &[&str], list: &[&str]) -> bool {
    words.iter().any(|w| list.contains(w))
}

fn spelled(word: &str) -> &str {
    match word {
        "ooc" | "outofcombat" => "out of combat",
        "inair" => "in air",
        "dwn" => "down",
        "foward" => "forward",
        "locolean" => "lean",
        "loco" => "",
        "l" => "left",
        "r" => "right",
        "add" => "additive",
        "airdash" => "air dash",
        "ui" => "menu",
        "getup" => "get up",
        "bindpose" => "bind pose",
        "invis" => "invisible",
        "vis" => "visible",
        "null" => "(empty)",
        "haspath" => "with path",
        // A numbered take reads 1, 2 … (not 01).
        other if other.len() > 1 && other.starts_with('0') && other.chars().all(|c| c.is_ascii_digit()) => other.trim_start_matches('0'),
        other => other,
    }
}

/// `describe`, with the direction taken from the clip's root motion (x
/// forward, y left, inches over the clip) when it moves and its suffix says
/// otherwise: a few game clips are named for one direction and move in
/// another (an `_nw` that moves straight left).
pub fn describe_moving(label: &str, motion: Option<(f64, f64)>) -> ClipName {
    let mut c = describe(label);
    let moved = motion.and_then(|(x, y)| Direction::from_motion(x, y, 1.)).filter(|d| *d != Direction::InPlace);
    if let (Some(said), Some(moved)) = (c.direction, moved) {
        if said != moved {
            c.name = c.name.replacen(&format!(", {}", said.words()), &format!(", {}", moved.words()), 1);
            c.direction = Some(moved);
        }
    }
    c
}

/// Words of a clip name outside the shared vocabulary (most often an
/// ability's own name); a clip with one and no stronger word is an ability.
pub fn own_words<'a>(words: &[&'a str]) -> Vec<&'a str> {
    words
        .iter()
        .copied()
        .filter(|w| !w.chars().all(|c| c.is_ascii_digit()) && ![HITS, OTHER, ABILITY, WEAPON, JUMPING, MOVEMENT, SHARED].iter().any(|list| list.contains(w)))
        .collect()
}

/// The readable name and group of a clip, from its file stem.
pub fn describe(label: &str) -> ClipName {
    let lower = label.to_ascii_lowercase();
    let (stem, derived) = match lower.split_once(".vnmclip+") {
        Some((stem, _)) => (stem.to_string(), true),
        None => (lower.clone(), false),
    };
    let mut words: Vec<&str> = stem.split('_').filter(|w| !w.is_empty()).collect();
    let direction = words.last().and_then(|w| Direction::from_suffix(w));
    if direction.is_some() {
        if words.last() == Some(&"runcenter") {
            *words.last_mut().unwrap() = "run";
        } else {
            words.pop();
        }
    }
    // "out of combat" reads as one phrase; "melee hit" is an attack, not a hit taken.
    let hit_taken = words.contains(&"hit") && !words.contains(&"melee") && !words.contains(&"success");
    let own = !own_words(&words).is_empty();
    let group = if has(&words, HITS) || hit_taken && !has(&words, WEAPON) {
        Group::Hits
    } else if has(&words, ABILITY) {
        Group::Abilities
    } else if has(&words, OTHER) && !has(&words, JUMPING) {
        Group::Other
    } else if has(&words, WEAPON)
        && (!own || has(&words, WEAPON_CONTEXT) || words.iter().any(|w| WEAPON.contains(w) && !ABILITY_ACTIONS.contains(w)))
    {
        Group::Weapon
    } else if own {
        Group::Abilities
    } else if has(&words, JUMPING) {
        Group::Jumping
    } else if has(&words, MOVEMENT) || direction.is_some() {
        Group::Movement
    } else {
        Group::Other
    };
    let menu = words.first() == Some(&"ui");
    if menu {
        words.remove(0);
    }
    let mut text: Vec<&str> = words.iter().map(|w| spelled(w)).filter(|w| !w.is_empty()).collect();
    text.dedup();
    let mut name = if stem == "animation_null" { "empty animation".to_string() } else { text.join(" ") };
    if let Some(d) = direction {
        name = if name.is_empty() { d.words().to_string() } else { format!("{name}, {}", d.words()) };
    }
    if menu {
        name = format!("menu screen: {name}");
    }
    if derived {
        name.push_str(" (full-pose copy)");
    }
    if let Some(first) = name.get(0..1) {
        name = first.to_ascii_uppercase() + &name[1..];
    }
    ClipName { name, group, direction, derived }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_groups_come_from_the_clip_words() {
        let c = describe("weapon_run_ne");
        assert_eq!((c.name.as_str(), c.group, c.direction), ("Weapon run, forward right", Group::Movement, Some(Direction::ForwardRight)));
        assert_eq!(describe("out_of_combat_crouch_run_center").name, "Out of combat crouch run, in place");
        assert_eq!(describe("melee_hit").group, Group::Weapon);
        assert_eq!(describe("flinch_back").group, Group::Hits);
        assert_eq!(describe("jump_ground").group, Group::Jumping);
        assert_eq!(describe("ui_hero_select").group, Group::Other);
        assert_eq!(describe("ui_main_menu").name, "Menu screen: main menu");
        assert_eq!(describe("ability_sleep_dagger").group, Group::Abilities);
        assert_eq!(describe("landing_impact_sleep").group, Group::Jumping);
        assert_eq!(describe("out_of_combat_n").group, Group::Movement);
        assert_eq!(describe_moving("zoomed_crouch_run_nw", Some((0., 184.))).name, "Zoomed crouch run, left");
        assert_eq!(describe("bloodbomb_crouch_run_n").group, Group::Abilities);
        assert_eq!(describe("ability_bullet_flurry").group, Group::Abilities);
        let d = describe("aim_weapon_idle.vnmclip+non_additive");
        assert!(d.derived && d.group == Group::Weapon && d.name.ends_with("(full-pose copy)"));
    }
    #[test]
    fn an_abilitys_own_aim_and_throw_are_the_ability_and_hits_taken_stay_hits() {
        assert_eq!(describe("leeches_throw").group, Group::Abilities);
        assert_eq!(describe("aim_lurking_terrors_up").group, Group::Abilities);
        assert_eq!(describe("item_throw").group, Group::Weapon);
        assert_eq!(describe("aim_syphon_weapon_idle").group, Group::Weapon);
        assert_eq!(describe("reload_healing_needles_idle").group, Group::Weapon);
        assert_eq!(describe("ability_knockback_run").group, Group::Hits);
        assert_eq!(describe("bloodbomb_run_n").group, Group::Abilities);
        assert_eq!((describe("bindpose").name.as_str(), describe("bindpose").group), ("Bind pose", Group::Other));
        assert_eq!((describe("animation_null").name.as_str(), describe("animation_null").group), ("Empty animation", Group::Other));
        assert_eq!(describe("death_idle_01").name, "Death idle 1");
        assert_eq!(describe("knockdown_large_getup").name, "Knockdown large get up");
        assert_eq!(describe("invis_run_n").name, "Invisible run, forward");
    }
    #[test]
    fn motion_directions_follow_the_game_axes() {
        assert_eq!(Direction::from_motion(217., 0., 1.), Some(Direction::Forward));
        assert_eq!(Direction::from_motion(0., -217., 1.), Some(Direction::Right));
        assert_eq!(Direction::from_motion(-153., 153., 1.), Some(Direction::BackwardLeft));
        assert_eq!(Direction::from_motion(0.1, 0., 1.), Some(Direction::InPlace));
    }
}
