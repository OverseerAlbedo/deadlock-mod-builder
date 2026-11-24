// Ability object catalogs shaped like Rust `ability_objects::ObjectCatalog` (rows as the
// catalog lists them for two heroes), small generated meshes in the preview-packet form
// (`src/preview/packet.ts`) and fit answers, for tests and the layout harness.
import { OBJECT_FIT_MIME, type ObjectBounds, type ObjectCatalog, type ObjectCollisionShape, type ObjectFit, type ObjectFitView, type ObjectKind, type ObjectRow } from "./objectTypes";

type V3 = [number, number, number];
const structure = (bones: number, meshes: number, physics: boolean, hitboxes: number, animated: boolean, attachments: number, materials: number) =>
  ({ bones, meshes, physics, hitboxes, animated, attachments, materials: Array.from({ length: materials }, (_, i) => `models/sample/materials/part_${i + 1}.vmat`) });
const bounds = (x: number, y: number, z: number, at: V3 = [0, 0, 0]): ObjectBounds => ({ min: [at[0] - x / 2, at[1] - y / 2, at[2] - z / 2], max: [at[0] + x / 2, at[1] + y / 2, at[2] + z / 2] });
function row(r: Partial<ObjectRow> & Pick<ObjectRow, "target" | "name" | "group" | "slot" | "ability" | "ability_name">): ObjectRow {
  return {
    file: r.target.split("/").pop()!.replace(/\.vmdl$/, ""),
    drawn: "particle",
    drawn_by: [],
    material_always_overridden: false,
    also_used_by: [],
    offered: true,
    ...r,
  };
}
export const DAGGER = "models/abilities/haze_dagger_model.vmdl";
export const TRIANGLE = "models/particle/triangle_fx_model.vmdl";
export const SKELETON_REASON = "It has a skeleton; objects with bones cannot be replaced yet.";
export const COLLISION_REASON = "It has collision (a physics shape); objects with collision cannot be replaced yet.";
export const MORPHS_REASON = "The game changes this object's own shape while it plays, which a replacement cannot carry yet.";
export const DAGGER_BOUNDS: ObjectBounds = { min: [-1.76, -0.65, -2.1], max: [18.66, 0.65, 2.1] };

export function hazeObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_staging/haze/haze.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 2, offered: 2, shared_shapes: 5 },
    rows: [
      row({
        target: DAGGER, name: "Dagger", group: 101, slot: "ESlot_Signature_1", ability: "ability_sleep_dagger", ability_name: "Sleep Dagger",
        drawn_by: [{ particle: "particles/abilities/haze/haze_sleep_dagger_model.vpcf_c", override_material: "materials/dev/gray_metal.vmat" }],
        material_override: "materials/dev/gray_metal.vmat", material_always_overridden: true, also_used_by: ["Mina", "Vyper"],
        structure: structure(0, 1, false, 0, false, 0, 1), bounds: DAGGER_BOUNDS,
      }),
      row({
        target: TRIANGLE, name: "Triangle", group: 102, slot: "ESlot_Signature_2", ability: "ability_smoke_bomb", ability_name: "Smoke Bomb",
        drawn_by: [{ particle: "particles/upgrades/tech_purge_model_ring.vpcf_c", override_material: "materials/models/particle/tech_purge_triangle_model.vmat" }],
        material_override: "materials/models/particle/tech_purge_triangle_model.vmat", material_always_overridden: true,
        structure: structure(1, 1, false, 0, false, 0, 1), bounds: bounds(14.3, 3.1, 12.4),
        // Offered: the one-bone kind on objects an effect draws is switched on (the engine's switches).
        kind: { features: ["one_bone"], label: "An object an effect draws, with one bone", kept: ["Its bone: your object moves as the game's does"], collision: false, stands_on: false },
      }),
    ],
  };
}
export function kelvinObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_staging/kelvin_v2/kelvin.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 5, offered: 4, shared_shapes: 4 },
    rows: [
      row({ target: "models/abilities/kelvin_fx_bomb_model.vmdl", name: "Bomb", group: 101, slot: "ESlot_Signature_1", ability: "ability_ice_grenade", ability_name: "Frost Grenade",
        drawn_by: [{ particle: "particles/abilities/kelvin/kelvin_ice_grenade_preview_model.vpcf_c" }, { particle: "particles/abilities/kelvin/kelvin_ice_grenade_projectile_model.vpcf_c" }],
        structure: structure(0, 1, false, 0, false, 0, 1), bounds: bounds(10.4, 10.4, 14.3) }),
      // Offered: the kind whose shape the game changes (a morph block left out) is switched on.
      row({ target: ICE_PATH, name: "Ice path body", group: 102, slot: "ESlot_Signature_2", ability: "ability_icepath", ability_name: "Ice Path",
        drawn: "definition_and_particle", drawn_by: [{ particle: "particles/abilities/kelvin/kelvin_ice_path_buff_model.vpcf_c" }],
        structure: structure(0, 5, false, 0, true, 0, 1), bounds: bounds(306.1, 239, 27.3), kind: ICE_PATH_KIND }),
      row({ target: "models/abilities/ice_path_icicle.vmdl", name: "Ice path icicle", group: 102, slot: "ESlot_Signature_2", ability: "ability_icepath", ability_name: "Ice Path",
        structure: structure(0, 2, false, 0, false, 0, 1), bounds: bounds(13.6, 19.9, 51.2),
        kind: { features: ["body_groups"], label: "An object an effect draws, with body groups", kept: ["Its body groups: whichever part an effect picks shows your object"], collision: false, stands_on: false } }),
      row({ target: "models/abilities/ice_dome.vmdl", name: "Ice dome", group: 104, slot: "ESlot_Signature_4", ability: "ability_ice_dome", ability_name: "Frozen Shelter",
        // The game never draws it (a modifier's blocker; what is seen is an effect): not offered.
        drawn: "definition", structure: structure(0, 1, true, 0, false, 0, 3), bounds: ICE_DOME_BOUNDS, offered: false, reason: NOT_DRAWN_REASON }),
      row({ target: "models/abilities/kelvin_ice_projectile_model.vmdl", name: "Ice projectile", group: 200, slot: "ESlot_Weapon_Primary", ability: "citadel_weapon_kelvin_set", ability_name: "",
        drawn_by: [{ particle: "particles/weapon_fx/kelvin/kelvin_bullet_tracer_model.vpcf_c" }], structure: structure(0, 1, false, 0, false, 0, 1), bounds: bounds(33.7, 6.1, 6.1) }),
    ],
  };
}

export const ICE_PATH = "models/props_gameplay/ice_path_body.vmdl";
/** Rust `object_morphs::KEPT_SHAPE`: the game changes its own object's shape while the animation plays; yours keeps its shape. */
export const KEPT_SHAPE = "While its animation plays, the game changes the shape of its own object; your object keeps its own shape";
/** The ice path body's kind as the engine gives it: no bones, an animation that changes only the shape (Rust `object_kind`). */
export const ICE_PATH_KIND: ObjectKind = {
  features: ["animated", "placed"],
  label: "An object the game places, with an animation",
  kept: ["Its animations, kept unchanged from the game's own object, and when the game plays them", KEPT_SHAPE, "When it appears and how long it stays"],
  collision: false,
  stands_on: true,
  game_blocks: ["animation"],
};
export const ICE_DOME = "models/abilities/ice_dome.vmdl";
export const ICE_DOME_BOUNDS: ObjectBounds = { min: [-127.8, -127.8, -125], max: [127.8, 127.8, 127.8] };
/** Rust `REASON_NOT_DRAWN`: a model the game uses only as an invisible blocker. */
export const NOT_DRAWN_REASON = "The game does not draw this model: it only blocks players, shots or sight, and what is seen there is an effect. A replacement would not show.";
/** What the engine says the build keeps of the game's own collision (Rust `KEPT_GAME_COLLISION`). */
export const KEPT_GAME_COLLISION = "Its collision, kept unchanged from the game's own object: what players bump into, stand on and shoot";
export const KEPT_GAME_ANIMATIONS = "Its animations, kept unchanged from the game's own object, and when the game plays them";
export const CUBE = "models/abilities/viscous_cube.vmdl";
/** Viscous's cube's kind: hull collision, kept as the game's own (Rust `object_kind`). */
export const CUBE_KIND: ObjectKind = {
  features: ["other_collision", "attachments", "placed"],
  label: "An object the game places, with collision and attachment points",
  kept: [KEPT_GAME_COLLISION, "When it appears and how long it stays", "The points the game attaches effects to"],
  collision: true,
  stands_on: true,
  game_blocks: ["collision"],
};
export const CUBE_BOUNDS: ObjectBounds = { min: [-44.9, -46.3, -0.6], max: [45.7, 45.1, 83.5] };
/** The cube's kept collision as the engine draws it: one box per hull or mesh shape (rounded). */
export const CUBE_BOX: ObjectCollisionShape = { shape: "box", min: [-40.5, -40.5, 0], max: [40.5, 40.5, 81] };
/** Viscous's catalog with the app's switches (the cube offered with its kept collision). */
export function viscousObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_staging/viscous/viscous.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 3 },
    rows: [
      row({ target: CUBE, name: "Cube", group: 102, slot: "ESlot_Signature_2", ability: "viscous_restorative_goo", ability_name: "The Cube",
        drawn: "definition_and_particle", structure: structure(0, 1, true, 0, false, 2, 1), bounds: CUBE_BOUNDS, kind: CUBE_KIND }),
    ],
  };
}
/** The cube's fit answer: the game's cube as a box, yours if given, the kept collision and the line. */
export const cubeAnswer = (yours: ArrayBuffer | null) =>
  fitAnswer(CUBE, [boxPacket([{ min: [-40.5, -40.5, 0], max: [40.5, 40.5, 81] }], "cube")], yours, {}, CUBE_BOUNDS, { collision: [CUBE_BOX], ...(yours ? { collision_fit: collisionFit(3.2, 2.5) } : {}) });
export const KEG = "models/abilities/astro_explosive.vmdl";
/** The engine's kind of an object the game places, with collision, one bone, attachment points and an outline (Rust `object_kind`). */
export const KEG_KIND: ObjectKind = {
  features: ["one_bone", "simple_collision", "attachments", "placed", "outline_shells"],
  label: "An object the game places, with collision, one bone, attachment points and an outline",
  kept: ["Its collision: what players bump into, stand on and shoot", "When it appears and how long it stays", "The points the game attaches effects to", "Its bone: your object moves as the game's does"],
  collision: true,
  stands_on: true,
};
export const KEG_BOUNDS: ObjectBounds = { min: [-15.9, -33.5, -18.6], max: [15.9, 15.9, 15.9] };
/** The keg's capsule as the engine places it (model space, rounded). */
export const KEG_CAPSULE: ObjectCollisionShape = { shape: "capsule", start: [0, 0.91, 0], end: [0, -1.38, 0], radius: 16 };
export const BONES_REASON = "It has several bones; objects with more than one bone cannot be replaced yet.";
export const FOLDERS_REASON = "Its animations are arranged in folders, which a rebuild cannot keep in the game's order; such objects cannot be replaced yet.";
export const BONE_ORDER_REASON = "Its animation data lists its bones in an order a rebuild cannot keep; such objects cannot be replaced yet.";
export const LEGACY_ANIMATION_REASON = "Its animation is not played through an animation graph; such objects cannot be replaced yet.";
export const BREAK_PIECES_REASON = "It breaks into pieces when destroyed; such objects cannot be replaced yet.";
export const OTHER_COLLISION_REASON = "Its collision is made of hull or mesh shapes; only spheres and capsules can be kept so far.";
export const ANIMATION_BY_NAME_REASON = "Its animation data lists its bones in another order than the object's skeleton; such objects cannot be replaced yet.";
/** Holliday's catalog as the engine lists it with the app's switches (the keg offered with its kind). */
export function hollidayObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_staging/astro/astro.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 2, offered: 2, shared_shapes: 5 },
    rows: [
      row({ target: KEG, name: "Explosive", group: 101, slot: "ESlot_Signature_1", ability: "ability_explosive_barrel", ability_name: "Powder Keg",
        drawn: "definition", structure: structure(1, 6, true, 0, false, 2, 2), bounds: KEG_BOUNDS, kind: KEG_KIND }),
      // Offered: the kind animated on one bone is switched on (the engine's switches).
      row({ target: LAUNCHPAD, name: "Cowboy launchpad", group: 102, slot: "ESlot_Signature_2", ability: "ability_bounce_pad", ability_name: "Bounce Pad",
        drawn: "definition", structure: structure(1, 1, true, 0, true, 0, 1), bounds: bounds(55, 55, 33), kind: LAUNCHPAD_KIND }),
    ],
  };
}
export const LAUNCHPAD = "models/particle/cowboy_launchpad.vmdl";
/** The bounce pad's kind: one bone, kept mesh collision and kept animation (Rust `object_kind`). */
export const LAUNCHPAD_KIND: ObjectKind = {
  features: ["one_bone", "other_collision", "animated", "placed"],
  label: "An object the game places, with collision, one bone and an animation",
  kept: [KEPT_GAME_ANIMATIONS, KEPT_GAME_COLLISION, "When it appears and how long it stays", "Its bone: your object moves as the game's does"],
  collision: true,
  stands_on: true,
  game_blocks: ["animation", "collision"],
};
export const WALL = "models/abilities/engineer_wall.vmdl";
export const WALL_PHYSICS = "models/abilities/engineer_wall_physics.vmdl";
/** Rust `KEPT_MATERIAL_ANIMATION_JITTER`: the wall's animation only makes its own material shake while it appears. */
export const KEPT_MATERIAL_ANIMATION_JITTER = "Its animation, kept unchanged from the game's own object. While it appears, the game makes the surface of its own object shake. Your object does not shake: it keeps its shape";
/** The travelling wall's kind: no bones, an animation driving only a material value (Rust `object_kind`). */
export const WALL_KIND: ObjectKind = {
  features: ["other_collision", "animated", "several_materials"],
  label: "An object an effect draws, with collision, several materials and an animation",
  kept: [KEPT_MATERIAL_ANIMATION_JITTER, KEPT_GAME_COLLISION],
  collision: true,
  stands_on: true,
  game_blocks: ["animation", "collision"],
};
/** Rust `GAME_LOOK_ALWAYS` / `GAME_LOOK_SOMETIMES`: effects draw the object in one of the game's own looks of it. */
export const GAME_LOOK_ALWAYS = "The game's effects draw this object in one of the game's own looks of it, with the game's materials for that look. Your object's shape shows in game; its colors and images do not.";
export const GAME_LOOK_SOMETIMES = "Some of the game's effects draw this object in one of the game's own looks of it, with the game's materials for that look. There your object's shape shows, but not its colors and images.";
/** Rust `FOUND_BY_FILE_NAME` for the standing wall beside the wall. */
export const WALL_FOUND_BY = "Found by its file name, next to “Wall”. No game data names it: the game places it itself, and the data does not say when it shows.";
/** McGinnis's Spectral Wall; with the app's switches (`withStanding`, the default) the standing wall is
 *  listed beside it by its file name (switch `listed_by_file_name`); `false` leaves it out. */
export function mcginnisWallObjects(withStanding = true): ObjectCatalog {
  const rows = [
    row({ target: WALL, name: "Wall", group: 103, slot: "ESlot_Signature_3", ability: "citadel_ability_fissure_wall", ability_name: "Spectral Wall",
      drawn_by: [{ particle: "particles/abilities/engineer/engineer_wall_deploy_model.vpcf_c" }, { particle: "particles/abilities/engineer/engineer_wall_deploy_model_enemy.vpcf_c" }],
      structure: structure(0, 1, true, 0, true, 0, 4), bounds: bounds(59, 174, 286, [0, 0, 143]), kind: WALL_KIND, game_look: { always: true, line: GAME_LOOK_ALWAYS } }),
  ];
  if (withStanding) {
    rows.push(row({ target: WALL_PHYSICS, name: "Wall, as placed", group: 103, slot: "ESlot_Signature_3", ability: "citadel_ability_fissure_wall", ability_name: "Spectral Wall",
      drawn: "file_name", found_by: WALL_FOUND_BY, structure: structure(0, 1, true, 0, false, 1, 2), bounds: bounds(71, 209, 343, [0, 0, 171]),
      kind: { features: ["other_collision", "attachments", "several_materials", "placed"], label: "An object the game places, with collision, attachment points and several materials",
        kept: [KEPT_GAME_COLLISION, "When it appears and how long it stays", "The points the game attaches effects to"], collision: true, stands_on: true, game_blocks: ["collision"] } }));
  }
  return { hero_path: "models/heroes_wip/mcginnis/mcginnis.vmdl", profile: "ability-object-profile-v1", counts: { listed: rows.length, offered: rows.length, shared_shapes: 5 }, rows };
}
/** Rust `KEPT_GAME_CLOTH`: the game's own cloth kept unchanged; only the parts that follow its bones swing. */
export const KEPT_GAME_CLOTH = "Its cloth, kept unchanged from the game's own object: only the parts of your object that follow the bones the cloth moves swing like cloth; the rest of it does not";
export const HELPER = "models/abilities/familiar/familiar_helper_npc.vmdl";
export const HELPER_ATTACHED = "models/abilities/familiar/familiar_helper_attached.vmdl";
/** Rem's helper's kind: several bones, kept animation and the game's own cloth (Rust `object_kind`, its moving bones shortened). */
export const HELPER_KIND: ObjectKind = {
  features: ["bones", "animated", "attachments", "several_materials", "placed"],
  label: "Animated object · 23 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, KEPT_GAME_CLOTH, "When it appears and how long it stays", "The points the game attaches effects to", "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: false,
  stands_on: true,
  moving: { bones: ["cog", "inner_cog_scale", "ear_0_L", "ear_end_L", "tassel_0", "tassel_end"], skeleton: 38, animated: true },
  game_blocks: ["animation"],
};
/** Rem's catalog with the app's switches: the helper with cloth beside the attached helper, which has none. */
export function remObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_wip/familiar/familiar_wip.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 2, offered: 2, shared_shapes: 5 },
    rows: [
      row({ target: HELPER_ATTACHED, name: "Helper attached", group: 102, slot: "ESlot_Signature_2", ability: "ability_familiar_attach", ability_name: "Tag Along",
        drawn: "definition", structure: structure(2, 1, false, 0, true, 12, 1), bounds: bounds(21.4, 26.9, 30.5, [0.6, 0.2, 5.2]),
        kind: { features: ["bones", "animated", "attachments", "placed"], label: "Animated object · 1 moving parts",
          kept: [KEPT_GAME_ANIMATIONS, "When it appears and how long it stays", "The points the game attaches effects to", "Its bones: each part of yours moves and turns with the bone it follows"],
          collision: false, stands_on: true, moving: { bones: ["root"], skeleton: 2, animated: true }, game_blocks: ["animation"] } }),
      row({ target: HELPER, name: "Helper npc", group: 103, slot: "ESlot_Signature_3", ability: "ability_familiar_helpinghands", ability_name: "Lil Helpers",
        drawn_by: [{ particle: "particles/abilities/familiar/familiar_helpinghand_debuff_minions.vpcf_c" }],
        structure: structure(38, 1, true, 0, true, 2, 2), bounds: bounds(12.5, 12.9, 17, [-0.1, 0, 8.5]), kind: HELPER_KIND }),
    ],
  };
}
export const ARROW = "models/heroes_staging/archer/archer_arrow.vmdl";
/** The owl's kind (Rust `object_kind`, its moving bones shortened). */
export const OWL_KIND: ObjectKind = {
  features: ["bones", "animated", "attachments", "several_materials", "placed"],
  label: "Animated object · 47 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, "When it appears and how long it stays", "The points the game attaches effects to", "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: false,
  stands_on: true,
  moving: { bones: ["owl_body", "owl_head_0", "owl_hand_L", "owl_hand_R"], skeleton: 49, animated: true },
  game_blocks: ["animation"],
};
/** Grey Talon's catalog with the app's switches: the arrow (one bone, drawn only by effects) is offered with its kind. */
export function greyTalonObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_staging/archer/archer.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 2, offered: 2, shared_shapes: 7 },
    rows: [
      row({ target: ARROW, name: "Arrow", group: 101, slot: "ESlot_Signature_1", ability: "ability_charged_shot", ability_name: "Charged Shot",
        drawn_by: [{ particle: "particles/abilities/archer/archer_charged_shot_proj_arrow_model.vpcf_c", override_material: "materials/particle/model/white_self_illum_var.vmat" }, { particle: "particles/weapon_fx/archer/archer_tracer_model.vpcf_c" }],
        material_override: "materials/particle/model/white_self_illum_var.vmat", structure: structure(1, 1, false, 0, false, 0, 1), bounds: { min: [-35.1, -1.1, -1.3], max: [0, 1.6, 1.7] },
        kind: { features: ["one_bone"], label: "An object an effect draws, with one bone", kept: ["Its bone: your object moves as the game's does"], collision: false, stands_on: false } }),
      // Offered since the game's own animation blocks are kept on graph-played objects (its
      // folders once closed it); moving bones shortened here.
      row({ target: "models/heroes_staging/grey_talon/grey_talon_owl.vmdl", name: "Owl", group: 104, slot: "ESlot_Signature_4", ability: "ability_guided_owl", ability_name: "Guided Owl",
        drawn: "definition", structure: structure(49, 1, false, 0, true, 6, 2), bounds: bounds(45, 151, 51), kind: OWL_KIND }),
    ],
  };
}
const kind = (label: string, features: ObjectKind["features"], kept: string[], collision: boolean, standsOn: boolean): ObjectKind => ({ features, label, kept, collision, stands_on: standsOn });
const KEPT = { collision: "Its collision: what players bump into, stand on and shoot", placed: "When it appears and how long it stays", points: "The points the game attaches effects to", hit: "Its hit areas", bone: "Its bone: your object moves as the game's does", groups: "Its body groups: whichever part an effect picks shows your object" };
export const BODY_GROUPS_REASON = "It has body groups (parts an effect switches between); such objects cannot be replaced yet.";
/** Rust `REASON_KEY_VALUES_REORDERED`. */
export const KEY_VALUES_REASON = "Its game data is stored in an order a rebuild cannot keep; such objects cannot be replaced yet.";
/** Rust `KEPT_BREAK_PIECES`: the break-piece line of "Stays the game's". */
export const KEPT_BREAK_PIECES = "The pieces it breaks into when destroyed: the game's own pieces, not pieces of your object";
/** The written-back fields that only describe how the file stores something: Rust no longer lists them, and the page never shows them. */
export const FILE_DETAIL_LINES = [
  "Its bones' resting turns, stored as the game's own file stores them",
  "Its bones' resting sizes, as the game's own",
  "Its game data, in the game's own order",
  "Its body groups' names, in the game's own older form",
];
/** The bishop's (and the queen's) kind as the engine names it: break pieces kept as the game's own. */
export const BISHOP_KIND: ObjectKind = {
  features: ["simple_collision", "hitboxes", "attachments", "placed"],
  label: "An object the game places, with collision, attachment points and hit areas",
  kept: [KEPT.collision, KEPT.placed, KEPT.points, KEPT.hit, KEPT_BREAK_PIECES],
  collision: true,
  stands_on: true,
};
/** Solomon's catalog with the app's switches (rounded from the engine's answer): all nine offered (the crown and the tile with body groups, the hand and the rabbit with their kept animation, the bishop and the queen with the game's own break pieces). */
export function solomonObjects(): ObjectCatalog {
  const piece = (target: string, name: string, group: number, ability: string, abilityName: string, b: ObjectBounds, extra: Partial<ObjectRow>) =>
    row({ target, name, group, slot: `ESlot_Signature_${group - 100}`, ability, ability_name: abilityName, bounds: b, ...extra });
  return {
    hero_path: "models/heroes_wip/chessmaster/chessmaster.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 9, offered: 9, shared_shapes: 7 },
    rows: [
      piece("models/particle/blend/chessmaster_chesspiece_bishop.vmdl", "Chesspiece bishop", 101, "ability_chessmaster_throwbishop", "Setup Bishop", { min: [-23.4, -23.4, 0.3], max: [23.4, 23.4, 125.2] },
        { drawn_by: [{ particle: "particles/abilities/chessmaster/chessmaster_bishop_cast_projectile_model.vpcf_c", override_material: "materials/particle/abilities/chessmaster/chessmaster_bishop_cast_projectile_model.vmat" }],
          material_override: "materials/particle/abilities/chessmaster/chessmaster_bishop_cast_projectile_model.vmat", material_always_overridden: true, structure: structure(0, 1, true, 1, false, 5, 1),
          kind: BISHOP_KIND }),
      piece("models/abilities/chessmaster_rook_crown_fx_model.vmdl", "Rook crown", 101, "ability_chessmaster_throwbishop", "Setup Bishop", { min: [-20.8, -19.4, -5.9], max: [20.8, 19.4, 24] },
        { structure: structure(1, 2, false, 0, false, 0, 1),
          kind: kind("An object an effect draws, with one bone and body groups", ["one_bone", "body_groups"], [KEPT.bone, KEPT.groups], false, false) }),
      piece("models/particle/blend/chessmaster_chesspiece_knight.vmdl", "Chesspiece knight", 102, "ability_chessmaster_knightbuff", "Kingside Knight", { min: [-23.4, -19.4, 0.9], max: [27.5, 19.4, 102.2] },
        { material_override: "materials/particle/abilities/chessmaster/chessmaster_bishop_cast_projectile_model.vmat", material_always_overridden: true, structure: structure(0, 1, true, 1, false, 4, 1),
          kind: kind("An object an effect draws, with collision, attachment points and hit areas", ["simple_collision", "hitboxes", "attachments"], [KEPT.collision, KEPT.points, KEPT.hit], true, true) }),
      piece("models/heroes_wip/chessmaster/chessmaster_knightshield.vmdl", "Knightshield", 102, "ability_chessmaster_knightbuff", "Kingside Knight", { min: [-5.7, -17.4, 0.8], max: [5.7, 17.4, 62.1] },
        { structure: structure(0, 2, false, 0, false, 0, 2), kind: kind("An object an effect draws, with several materials", ["several_materials"], [], false, false) }),
      piece("models/particle/blend/chessmaster_chesspiece_rook.vmdl", "Chesspiece rook", 103, "ability_chessmaster_forkpiece", "Rook Battery", { min: [-50.5, -23.4, 0.3], max: [23.4, 23.4, 113.6] },
        { structure: structure(0, 2, true, 1, false, 2, 2),
          kind: kind("An object an effect draws, with collision, attachment points, hit areas and several materials", ["simple_collision", "hitboxes", "attachments", "several_materials"], [KEPT.collision, KEPT.points, KEPT.hit], true, true) }),
      piece("models/abilities/rabbit/rabbit.vmdl", "Rabbit", 103, "ability_chessmaster_forkpiece", "Rook Battery", bounds(53, 40.6, 76),
        // Offered since the game's own order of its game data is written back (moving bones shortened).
        { drawn: "definition", structure: structure(49, 1, false, 3, true, 19, 1),
          kind: { ...kind("Animated object · 49 moving parts", ["bones", "hitboxes", "animated", "attachments", "placed"], [KEPT_GAME_ANIMATIONS, KEPT.placed, KEPT.points, KEPT.hit, "Its bones: each part of yours moves and turns with the bone it follows"], false, true),
            moving: { bones: ["root_motion", "Pelvis", "Head"], skeleton: 49, animated: true }, game_blocks: ["animation"] } }),
      piece("models/abilities/chessmaster_tile_model.vmdl", "Tile", 103, "ability_chessmaster_forkpiece", "Rook Battery", bounds(100, 100, 5),
        { material_override: "materials/particle/abilities/chessmaster/chessmaster_square_tile_model.vmat", structure: structure(0, 2, false, 0, false, 0, 2),
          kind: kind("An object an effect draws, with several materials and body groups", ["several_materials", "body_groups"], [KEPT.groups], false, false) }),
      piece("models/heroes_wip/chessmaster/chessmaster_hand.vmdl", "Hand", 104, "ability_chessmaster_queen", "Develop Queen", bounds(13, 5, 3),
        // Offered since the game's own animation is kept when its data binds the bones by name (moving bones shortened).
        { structure: structure(36, 2, false, 2, true, 0, 2),
          kind: { ...kind("Animated object · 23 moving parts", ["bones", "hitboxes", "animated", "several_materials"], [KEPT_GAME_ANIMATIONS, KEPT.hit, "Its bones: each part of yours moves and turns with the bone it follows"], false, false),
            moving: { bones: ["hand_R", "finger_index_0_R", "finger_thumb_0_R"], skeleton: 36, animated: true }, game_blocks: ["animation"] } }),
      piece("models/particle/blend/chessmaster_queen.vmdl", "Queen", 104, "ability_chessmaster_queen", "Develop Queen", { min: [-39.1, -39.1, 1.9], max: [39.1, 39.1, 181.5] },
        { material_override: "materials/particle/deployment_preview.vmat", material_always_overridden: true, structure: structure(0, 1, true, 1, false, 3, 1),
          kind: BISHOP_KIND }),
    ],
  };
}
/** The rook's capsule as the engine places it. */
export const ROOK_CAPSULE: ObjectCollisionShape = { shape: "capsule", start: [0, 0, 10], end: [0, 0, 75], radius: 15 };
/** The keg as two boxes (body and a cap), a stand-in drawn for tests, not game data. */
export const kegPacket = () => boxPacket([{ min: [-15.9, -33.5, -15.9], max: [15.9, 12, 15.9] }, { min: [-6, 12, -6], max: [6, 15.9, 6] }], "keg");
/** "Yours" beside the keg: a block taller than the game's (it sticks out of the collision). */
export const blockPacket = () => boxPacket([{ min: [-14, -33.5, -20.8], max: [14, 15.9, 18] }], "yours");

/** A preview packet (`decodePreview` form) of boxes, each given by its corners. */
export function boxPacket(boxes: { min: V3; max: V3 }[], name = "mesh"): ArrayBuffer {
  const positions: number[] = [], normals: number[] = [], indices: number[] = [];
  const faces: [V3, number[][]][] = [
    [[1, 0, 0], [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]]], [[-1, 0, 0], [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]]],
    [[0, 1, 0], [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]]], [[0, -1, 0], [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]],
    [[0, 0, 1], [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]], [[0, 0, -1], [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]]],
  ];
  for (const box of boxes)
    for (const [normal, corners] of faces) {
      const base = positions.length / 3;
      for (const c of corners) { positions.push(...c.map((t, i) => (t ? box.max[i] : box.min[i]))); normals.push(...normal); }
      indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
    }
  const count = positions.length / 3;
  const all = [...positions, ...normals];
  const min = [0, 1, 2].map((i) => Math.min(...boxes.map((b) => b.min[i]))) as V3, max = [0, 1, 2].map((i) => Math.max(...boxes.map((b) => b.max[i]))) as V3;
  const metadata = {
    schema_version: 1, source: name, source_sha256: "0".repeat(64), up_axis: "Z", warnings: [], bounds: { min, max }, bones: [],
    meshes: [{ name, source_name: name, material: "plain", vertex_count: count, index_count: indices.length,
      positions: { offset: 0, count: count * 3 }, normals: { offset: count * 12, count: count * 3 }, uvs: null, colors: null,
      indices: { offset: all.length * 4, count: indices.length } }],
  };
  const json = new TextEncoder().encode(JSON.stringify(metadata));
  const dataOffset = 16 + Math.ceil(json.length / 4) * 4;
  const buffer = new ArrayBuffer(dataOffset + all.length * 4 + indices.length * 4);
  const view = new DataView(buffer);
  view.setUint32(0, 0x50424d44, true); view.setUint32(4, 1, true); view.setUint32(8, json.length, true);
  new Uint8Array(buffer, 16, json.length).set(json);
  new Float32Array(buffer, dataOffset, all.length).set(all);
  new Uint32Array(buffer, dataOffset + all.length * 4, indices.length).set(indices);
  return buffer;
}
/** The game's dagger as two boxes (blade and grip), a stand-in drawn for tests, not game data. */
export const daggerPacket = () => boxPacket([{ min: [2, -0.3, -1.2], max: [18.6, 0.3, 1.2] }, { min: [-1.7, -0.65, -2.1], max: [2, 0.65, 2.1] }], "dagger");
/** "Yours": a wand (a bar with a star-ish head), already fitted to the dagger's length. */
export const wandPacket = (scale = 1) => {
  const s = (v: V3): V3 => [8.45 + (v[0] - 8.45) * scale, v[1] * scale, v[2] * scale];
  return boxPacket([{ min: s([-1.75, -0.5, -0.5]), max: s([14, 0.5, 0.5]) }, { min: s([0.5, -0.9, -2.2]), max: s([1.6, 0.9, 2.2]) }, { min: s([14, -0.7, -2.6]), max: s([18.65, 0.7, 2.6]) }], "yours");
};

/** A fit answer: `[u32 header length][header {mime_type, details}][data]`. */
export function fitAnswer(target: string, game: ArrayBuffer[], yours: ArrayBuffer | null, fit: ObjectFit = {}, gameBounds: ObjectBounds = DAGGER_BOUNDS, extra: Partial<ObjectFitView> = {}): ArrayBuffer {
  const packets: ObjectFitView["packets"] = [];
  let offset = 0;
  for (const [i, bytes] of game.entries()) { packets.push({ role: "game", name: `mesh_${i}.dmx`, offset, length: bytes.byteLength }); offset += bytes.byteLength; }
  if (yours) packets.push({ role: "yours", name: "yours", offset, length: yours.byteLength });
  const details: ObjectFitView = {
    target, packets, transform: [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]], game_bounds: gameBounds,
    ...(yours ? { yours_bounds: gameBounds } : {}),
    fit: { size: fit.size ?? "match", scale: fit.scale ?? 1, turns: { roll: fit.turns?.roll ?? 0, tip: fit.turns?.tip ?? 0, turn: fit.turns?.turn ?? 0 }, ...(fit.on_base ? { on_base: true } : {}) },
    ...extra,
  };
  const head = new TextEncoder().encode(JSON.stringify({ mime_type: OBJECT_FIT_MIME, details }));
  const total = 4 + head.length + offset + (yours?.byteLength ?? 0);
  const out = new Uint8Array(total);
  new DataView(out.buffer).setUint32(0, head.length, true);
  out.set(head, 4);
  let at = 4 + head.length;
  for (const bytes of [...game, ...(yours ? [yours] : [])]) { out.set(new Uint8Array(bytes), at); at += bytes.byteLength; }
  return out.buffer;
}
/** The engine's lines about the collision (Rust `collision_fit`), for fixtures. */
export const collisionFit = (inches: number, game: number) => {
  const shown = (v: number) => String(Math.round(v * 10) / 10);
  const fits = inches <= game + 2;
  return {
    inches, game_inches: game, fits,
    line: fits
      ? `Your object fits the game's collision: it sticks out by up to ${shown(inches)} in, the game's own object by ${shown(game)} in.`
      : `Your object sticks out of the game's collision by up to ${shown(inches)} in (the game's own object by ${shown(game)} in): players would be blocked by, or walk through, something that does not match what they see.`,
  };
};
/** The keg's fit answer: the game's keg, yours if given, the capsule and the line. */
export const kegAnswer = (yours: ArrayBuffer | null, fit: ObjectFit = {}, inches = 24.8) =>
  fitAnswer(KEG, [kegPacket()], yours, fit, KEG_BOUNDS, { collision: [KEG_CAPSULE], ...(yours ? { collision_fit: collisionFit(inches, 17.6) } : {}) });

export const CANISTER = "models/heroes_wip/mcginnis/healing_canister/healing_canister.vmdl";
/** The canister's kind as the engine names it with the several-bones switch opened (Rust `object_kind`). */
export const CANISTER_KIND: ObjectKind = {
  features: ["bones", "simple_collision", "hitboxes", "animated", "attachments", "several_materials", "placed"],
  label: "Animated object · 3 moving parts",
  kept: ["Its animations, and when the game plays them", "Its collision: what players bump into, stand on and shoot", "When it appears and how long it stays", "The points the game attaches effects to", "Its hit areas", "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: true,
  stands_on: true,
  moving: { bones: ["main", "full_top", "full_bottom"], skeleton: 7, animated: true },
};
/** McGinnis's catalog with the several-bones switch opened for a test: the canister offered with its kind. */
export function mcginnisObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_wip/mcginnis/mcginnis.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 5 },
    rows: [
      row({ target: CANISTER, name: "Healing canister", group: 102, slot: "ESlot_Signature_2", ability: "ability_medicinal_specter", ability_name: "Medicinal Specter",
        drawn: "definition", structure: structure(7, 1, true, 2, true, 8, 3), bounds: { min: [-12, -12, 0], max: [12, 12, 52] }, kind: CANISTER_KIND }),
    ],
  };
}
export const TURRET = "models/heroes_wip/mcginnis/turret/turret.vmdl";
/** Rust `kept_part_line` for the turret's range markers, and `KEPT_BODY_GROUP_PARTS`. */
export const KEPT_RANGE_MARKERS = "Its range markers, kept as the game's own";
export const KEPT_BODY_GROUP_PARTS = "Its body groups: your object shows where the game's object normally shows";
/** The turret's kind as the engine names it with the app's switches: a part of another body group kept as the game's own (moving bones shortened). */
export const TURRET_KIND: ObjectKind = {
  features: ["bones", "other_collision", "hitboxes", "animated", "attachments", "several_materials", "placed", "body_groups"],
  label: "Animated object · 23 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, KEPT_GAME_COLLISION, "When it appears and how long it stays", "The points the game attaches effects to", "Its hit areas", KEPT_BODY_GROUP_PARTS, KEPT_RANGE_MARKERS, "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: true,
  stands_on: true,
  moving: { bones: ["root_motion", "turret_body", "gun_assembly_02"], skeleton: 26, animated: true },
  game_blocks: ["animation", "collision"],
};
/** McGinnis's catalog with the app's switches: the canister and the turret. */
export function mcginnisTurretObjects(): ObjectCatalog {
  const canister = mcginnisObjects();
  return {
    ...canister,
    counts: { listed: 2, offered: 2, shared_shapes: 5 },
    rows: [
      row({ target: TURRET, name: "Turret", group: 101, slot: "ESlot_Signature_1", ability: "citadel_ability_shieldedsentry", ability_name: "Mini Turret",
        drawn: "definition_and_particle", structure: structure(26, 2, true, 9, true, 8, 6), bounds: { min: [-53.3, -304.1, -3.3], max: [304.1, 304.1, 67.7] }, kind: TURRET_KIND }),
      ...canister.rows,
    ],
  };
}
export const BRIEFCASE = "models/abilities/pocket_briefcase_model.vmdl";
/** Rust `object_glow_shells::kept_line` for the briefcase's shell (its body group's choice "glow"). */
export const KEPT_GLOW_SHELL = "Its glow: a shell made from your object, in the game's own material, shown when the game shows it";
/** The briefcase's kind as the engine names it with the app's switches: a glow shell only another choice of its body group shows. */
export const BRIEFCASE_KIND: ObjectKind = {
  features: ["bones", "animated", "body_groups", "body_group_shells"],
  label: "Animated object · 2 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, "Its body groups: whichever part an effect picks shows your object", KEPT_GLOW_SHELL, "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: false,
  stands_on: false,
  moving: { bones: ["case_01", "case_02"], skeleton: 2, animated: true },
  game_blocks: ["animation"],
};
/** Pocket's catalog with the app's switches: the briefcase, drawn by an effect, with its glow shell. */
export function pocketObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_wip/pocket/pocket.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 6 },
    rows: [
      row({ target: BRIEFCASE, name: "Briefcase", group: 103, slot: "ESlot_Signature_3", ability: "synth_pulse", ability_name: "Enchanter's Satchel",
        drawn_by: [{ particle: "particles/abilities/pocket/pocket_satchel_model.vpcf_c" }], structure: structure(2, 7, false, 0, true, 0, 2),
        bounds: { min: [-6.34, -15.38, -10.43], max: [5.16, 14.17, 12.32] }, kind: BRIEFCASE_KIND }),
    ],
  };
}
export const MISSILE = "models/abilities/deadpack_missile_model.vmdl";
/** Rust `object_states::PER_STATE_LINE`: under "How your model moves" for an object whose body-group choices are states. */
export const PER_STATE_LINE = "This object shows one of several states, and each state follows the game's own bones for that state: the app picks what your model follows in each one.";
/** The missile's kind as the engine names it with the states switch (Rust `object_kind`): four states bound to different bones. */
export const MISSILE_KIND: ObjectKind = {
  features: ["bones", "animated", "body_groups"],
  label: "Animated object · 6 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, "Its body groups: whichever part an effect picks shows your object", "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: false,
  stands_on: false,
  moving: { bones: ["base", "mine_base", "sensors_a", "sensors_b", "sensors_c", "sensors_d"], skeleton: 6, animated: true, per_state: PER_STATE_LINE },
  game_blocks: ["animation"],
};
/** Deadman Danny's catalog with the states switch: the missile, drawn by effects. */
export function deadmanDannyObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_staging/deadman/deadman.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 5 },
    rows: [
      row({ target: MISSILE, name: "Missile", group: 101, slot: "ESlot_Signature_1", ability: "ability_deadpack_chargeblast", ability_name: "Strix Salvo",
        drawn: "particle", drawn_by: [{ particle: "particles/abilities/airheart/airheart_mine_model.vpcf_c" }, { particle: "particles/abilities/airheart/airheart_missile_model.vpcf_c" }],
        structure: structure(6, 4, false, 0, true, 0, 1), bounds: { min: [-0.15, -6.04, -6.52], max: [46.45, 6.12, 6.26] }, kind: MISSILE_KIND }),
    ],
  };
}
/** The fit view's moving part for the missile with a two-part file: its default state's carriers and binding. */
export const MISSILE_MOVING = {
  carriers: [{ name: "base", points: 1200, centre: [23, 0, 0] as [number, number, number], share: 1 }],
  default_bone: "base",
  parts: ["lower", "upper"],
  armature: [],
  way: "by_parts" as const,
  bound: { lower: "base", upper: "base" },
  suggested: { lower: "base", upper: "base" },
  same_skeleton_open: true,
};
export const missileAnswer = (yours: ArrayBuffer | null) => fitAnswer(MISSILE, [blockPacket()], yours, {}, { min: [-0.15, -6.04, -6.52], max: [46.45, 6.12, 6.26] }, yours ? { moving: MISSILE_MOVING } : {});
/** The fit view's moving part for the canister with a three-part file. */
export const CANISTER_MOVING = {
  carriers: [{ name: "main", points: 900, centre: [0, 0, 22] as [number, number, number], share: 0.9 }, { name: "full_top", points: 50, centre: [0, 0, 27] as [number, number, number], share: 0.05 }, { name: "full_bottom", points: 50, centre: [-10, 0, 25] as [number, number, number], share: 0.05 }],
  default_bone: "main",
  parts: ["column", "collar", "bar"],
  armature: [],
  way: "by_parts" as const,
  // What the engine's nearest-point vote gives on the real canister with this file.
  bound: { bar: "main", collar: "main", column: "main" },
  suggested: { bar: "main", collar: "main", column: "main" },
  // The app's switches offer the same-skeleton way.
  same_skeleton_open: true,
};
export const canisterAnswer = (yours: ArrayBuffer | null) => fitAnswer(CANISTER, [blockPacket()], yours, {}, { min: [-12, -12, 0], max: [12, 12, 52] }, yours ? { moving: CANISTER_MOVING } : {});
export const HAND = "models/heroes_wip/necro/hand/necro_hand.vmdl";
/** The hand's kind as the engine names it with the app's switches: its bone constraints written
 * back, which the page does not list (file detail; moving bones shortened). */
export const HAND_KIND: ObjectKind = {
  features: ["bones", "animated"],
  label: "Animated object · 23 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: false,
  stands_on: false,
  moving: { bones: ["prop_arm_lower_R", "prop_hand_R", "prop_finger_index_0_R"], skeleton: 34, animated: true },
  game_blocks: ["animation"],
};
/** Graves's catalog with the app's switches: the hand, drawn by effects in their own material. */
export function gravesHandObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_wip/necro/necro.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 5 },
    rows: [
      row({ target: HAND, name: "Hand", group: 102, slot: "ESlot_Signature_2", ability: "ability_necro_zombiewall", ability_name: "Grasping Hands",
        drawn_by: [{ particle: "particles/abilities/necro/necro_root.vpcf_c", override_material: "models/abilities/materials/necro_hands.vmat" }, { particle: "particles/abilities/necro/necro_wall_hand_persistent.vpcf_c", override_material: "models/abilities/materials/necro_hands.vmat" }],
        material_override: "models/abilities/materials/necro_hands.vmat", material_always_overridden: true,
        structure: structure(34, 1, false, 0, true, 0, 1), bounds: { min: [-4.37, -2.64, -7.51], max: [3.09, 3.46, 19.46] }, kind: HAND_KIND }),
    ],
  };
}
export const DOOR = "models/heroes_wip/doorman/doorman_door.vmdl";
/** Rust `object_parts::kept_line` for the door's frame and portal surface (parts shown together with the door). */
export const KEPT_DOOR_FRAME = "Its doorman frame, kept as the game's own";
export const KEPT_DOOR_PORTAL = "Its doorman doubledoor portal, kept as the game's own";
/** The door's kind as the engine names it with the app's switches: its leaves replaced, the frame and the portal surface kept. */
export const DOOR_KIND: ObjectKind = {
  features: ["bones", "other_collision", "animated", "several_materials", "placed", "body_groups"],
  label: "Animated object · 3 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, KEPT_GAME_COLLISION, "When it appears and how long it stays", KEPT_BODY_GROUP_PARTS, KEPT_DOOR_PORTAL, KEPT_DOOR_FRAME, "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: true,
  stands_on: true,
  moving: { bones: ["root", "doorhinge_r", "doorhinge_l"], skeleton: 3, animated: true },
  game_blocks: ["animation", "collision"],
};
/** Doorman's catalog with the app's switches: the door, placed by the game (no game-look note: its looks keep the user's colors). */
export function doormanObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_wip/doorman_v2/doorman.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 4 },
    rows: [
      row({ target: DOOR, name: "Door", group: 102, slot: "ESlot_Signature_2", ability: "ability_doorman_doorway", ability_name: "Doorway",
        drawn: "definition_and_particle", drawn_by: [{ particle: "particles/abilities/doorman/doorman_doorway_barrier_buff_model.vpcf_c", override_material: "materials/particle/model/white_self_illum_var_trans_additive.vmat" }, { particle: "particles/abilities/doorman/doorman_doorway_deploy_preview_model.vpcf_c" }],
        material_override: "materials/particle/model/white_self_illum_var_trans_additive.vmat",
        structure: structure(3, 3, true, 0, true, 0, 7), bounds: { min: [-10.05, -70.29, -0.88], max: [6.85, 70.29, 185.67] }, kind: DOOR_KIND }),
    ],
  };
}
export const GROUND_IMPACT = "models/particle/ground_impact_fx_model.vmdl";
/** Rust `object_parts::KEPT_PIECES`: the further pieces of a state the game stores without names. */
export const KEPT_PIECES = "Its other pieces in the same look, kept as the game's own";
/** The ground impact's kind as the engine names it with the app's switches: each state shows the
 * user's object, the second state's other pieces kept (moving bones shortened). */
export const GROUND_IMPACT_KIND: ObjectKind = {
  features: ["bones", "animated", "body_groups"],
  label: "Animated object · 103 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, "Its body groups: whichever part an effect picks shows your object", KEPT_PIECES, "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: false,
  stands_on: false,
  moving: { bones: ["piece0Shape_packed1", "piece1Shape_packed1", "piece2Shape_packed1"], skeleton: 103, animated: true },
  game_blocks: ["animation"],
};
/** Abrams's catalog with the app's switches: the ground impact, drawn by an effect, also used by another hero. */
export function abramsObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_wip/abrams/abrams.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 5 },
    rows: [
      row({ target: GROUND_IMPACT, name: "Ground impact", group: 104, slot: "ESlot_Signature_4", ability: "citadel_ability_bull_leap", ability_name: "Seismic Impact",
        drawn_by: [{ particle: "particles/abilities/abrams/abrams_leap_impact_ground_model.vpcf_c" }], also_used_by: ["Mo & Krill"],
        structure: structure(103, 3, false, 0, true, 0, 1), bounds: { min: [-32.21, -32.58, -0.35], max: [31.72, 32.57, 0.36] }, kind: GROUND_IMPACT_KIND }),
    ],
  };
}
/** A motion answer: two bones, two frames, identity matrices. */
export function motionAnswer(clip = "x/healing_canister_deploy.vnmclip_c"): ArrayBuffer {
  const header = { bones: ["main", "full_bottom"], clips: [{ file: "x/healing_canister_deploy.vnmclip_c", label: "Deploy", duration: 1, additive: false, loops: false }, { file: "x/healing_canister_drain_liquid.vnmclip_c", label: "Drain liquid", duration: 1, additive: true, loops: false }], clip, fps: 30, frames: 2 };
  const json = new TextEncoder().encode(JSON.stringify({ mime_type: "application/x-dmb-object-motion", details: header }));
  const data = new Float32Array(2 * 2 * 12);
  for (let i = 0; i < 4; i++) { data[i * 12] = 1; data[i * 12 + 5] = 1; data[i * 12 + 10] = 1; }
  const out = new Uint8Array(4 + json.length + data.byteLength);
  new DataView(out.buffer).setUint32(0, json.length, true);
  out.set(json, 4);
  out.set(new Uint8Array(data.buffer), 4 + json.length);
  return out.buffer;
}
export const CAT_FORM = "models/abilities/nano_catform_model.vmdl";
/** The cat form's kind as the engine names it: its morph block left out beside its bones, and the
 * game's collision kept on a skeleton a rebuild lists in another order (Rust `object_kind`, moving bones shortened). */
export const CAT_FORM_KIND: ObjectKind = {
  features: ["bones", "other_collision", "hitboxes", "animated", "attachments", "placed"],
  label: "Animated object · 45 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, KEPT_SHAPE, KEPT_GAME_COLLISION, KEPT_GAME_CLOTH, "When it appears and how long it stays", "The points the game attaches effects to", "Its hit areas", "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: true,
  stands_on: true,
  moving: { bones: ["root_motion", "cat_pelvis", "cat_head", "cat_tail_3", "cat_tail_end"], skeleton: 56, animated: true },
  game_blocks: ["animation", "collision"],
};
/** Calico's catalog: the cat form. */
export function calicoObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_wip/nano/nano.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 5 },
    rows: [
      row({ target: CAT_FORM, name: "Catform", group: 103, slot: "ESlot_Signature_3", ability: "ability_nano_catform", ability_name: "Ava",
        drawn: "definition", structure: structure(56, 1, true, 2, true, 19, 1), bounds: { min: [-61, -7.7, -0.1], max: [33.8, 7.7, 45.3] }, kind: CAT_FORM_KIND }),
    ],
  };
}
export const PIGEON = "models/heroes_wip/baba/baba_pigeon_transform.vmdl";
/** Rust `kept_part_line` for the pigeon's shawl, named once although each level of detail draws one. */
export const KEPT_SHAWL = "Its shawl, kept as the game's own";
/** The pigeon's kind as the engine names it: a part of another body group kept as the game's own in
 * every level of detail, and the game's cloth (Rust `object_kind`, moving bones shortened). */
export const PIGEON_KIND: ObjectKind = {
  features: ["bones", "hitboxes", "animated", "attachments", "placed", "extra_meshes", "body_groups"],
  label: "Animated object · 73 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, KEPT_GAME_CLOTH, "When it appears and how long it stays", "The points the game attaches effects to", "Its hit areas", KEPT_BODY_GROUP_PARTS, KEPT_SHAWL, "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: false,
  stands_on: true,
  moving: { bones: ["pigeon_torso_1", "tailB_0", "pigeon_eye_0_L", "pigeon_eye_end_L"], skeleton: 91, animated: true },
  game_blocks: ["animation"],
};
/** Baba's catalog: the pigeon. */
export function babaObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_wip/baba/baba.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 5 },
    rows: [
      row({ target: PIGEON, name: "Pigeon transform", group: 104, slot: "ESlot_Signature_4", ability: "ability_baba_ultimate_2", ability_name: "Feed The Birds",
        drawn: "definition", structure: structure(91, 6, true, 4, true, 17, 1), bounds: { min: [-14.7, -16.8, -0.1], max: [4.9, 16.8, 16.9] }, kind: PIGEON_KIND }),
    ],
  };
}
export const ASSISTANT = "models/abilities/magician_cloneturret_model.vmdl";
/** Rust `KEPT_COLLISION` for collision a rebuild makes again (spheres and capsules). */
export const KEPT_COLLISION = "Its collision: what players bump into, stand on and shoot";
/** The assistant's kind as the engine names it: cloth made from a proxy mesh, its bones given the
 * game's names back after compiling (Rust `object_kind`, moving bones shortened). */
export const ASSISTANT_KIND: ObjectKind = {
  features: ["bones", "simple_collision", "hitboxes", "animated", "attachments", "placed"],
  label: "Animated object · 692 moving parts",
  kept: [KEPT_GAME_ANIMATIONS, KEPT_COLLISION, KEPT_GAME_CLOTH, "When it appears and how long it stays", "The points the game attaches effects to", "Its hit areas", "Its bones: each part of yours moves and turns with the bone it follows"],
  collision: true,
  stands_on: true,
  moving: { bones: ["root_motion", "pelvis", "earring_R_1"], skeleton: 771, animated: true },
  game_blocks: ["animation"],
};
/** Sinclair's catalog: the assistant. */
export function sinclairObjects(): ObjectCatalog {
  return {
    hero_path: "models/heroes_staging/magician/magician.vmdl",
    profile: "ability-object-profile-v1",
    counts: { listed: 1, offered: 1, shared_shapes: 5 },
    rows: [
      row({ target: ASSISTANT, name: "Cloneturret", group: 102, slot: "ESlot_Signature_2", ability: "ability_magician_cloneturret", ability_name: "Spectral Assistant",
        drawn: "definition_and_particle", drawn_by: [{ particle: "particles/abilities/magician/magician_clone_preview_model.vpcf_c" }, { particle: "particles/abilities/magician/magician_cloneturret_deploy_preview.vpcf_c" }],
        structure: structure(771, 3, true, 22, true, 27, 1), bounds: { min: [-11.9, -22.2, -2.6], max: [9.3, 22, 92.9] }, kind: ASSISTANT_KIND }),
    ],
  };
}
