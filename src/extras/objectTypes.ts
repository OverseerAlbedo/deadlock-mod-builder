// Extras → Objects (ability objects): the saved rows and the commands' answers.
// Rust: deadlock-project `ability_objects.rs` (rows), deadlock-components
// `ability_objects.rs` (catalog), deadlock-preparation `ability_object.rs`
// (preparing a file), src-tauri `objects.rs` (the commands).

/** How the user's object is sized before `scale` and the turns. */
export type ObjectFitSize = 'match' | 'keep';
/** Quarter turns (90°, 0–3) about the game object's X (roll), Y (tip) and Z (turn) axes, in that order. */
export interface ObjectTurns { roll?: number; tip?: number; turn?: number }
/** A row's fit; `{}` is "match, scale 1, no turns". Unknown keys are refused. */
export interface ObjectFit {
  /** `match`: scaled and placed to the game object's bounds and long axis; `keep`: the file's own size (metres as inches) and origin. */
  size?: ObjectFitSize;
  /** Extra uniform scale, OBJECT_SCALE_MIN to OBJECT_SCALE_MAX (default 1). */
  scale?: number;
  turns?: ObjectTurns;
  /** Stands the object on the game object's base (its lowest point along the game's up axis meets the game's) instead of the centres meeting. Absent unless set. */
  on_base?: boolean;
}
/** One saved ability object (`customization.ability_objects`, at most MAX_ABILITY_OBJECTS, one per target ignoring case). */
export interface AbilityObjectRow {
  /** The game model (`.vmdl`, as the catalog's `target`). */
  target: string;
  /** Default true. */
  enabled?: boolean;
  /** The user's file (.blend, .fbx, .glb, .gltf); empty until chosen. */
  source?: string;
  fit?: ObjectFit;
  /** How the model follows an object with several bones (Rust `Follow`); absent unless set. */
  follow?: ObjectFollow;
}
/** The three ways the user's model follows the game object's bones. */
export type ObjectFollowWay = "one_piece" | "by_parts" | "same_skeleton";
export interface ObjectFollow {
  way: ObjectFollowWay;
  /** One piece: the bone it follows (absent: the engine's default). */
  bone?: string;
  /** By parts: part (the file's object name) → bone; a part not named takes the suggestion. */
  parts?: Record<string, string>;
}
export const MAX_FOLLOW_PARTS = 256;
export const MAX_ABILITY_OBJECTS = 32;
export const OBJECT_SCALE_MIN = 0.05;
export const OBJECT_SCALE_MAX = 20;
export const OBJECT_MAX_QUARTER_TURNS = 3;
export const OBJECT_SOURCE_SUFFIXES = ['blend', 'fbx', 'glb', 'gltf'] as const;

export interface ObjectBounds { min: [number, number, number]; max: [number, number, number] }
/** A particle renderer (`C_OP_RenderModels`) drawing the model. */
export interface ObjectDrawing {
  particle: string;
  /** The material the renderer draws the model with instead of its own. */
  override_material?: string;
}
export interface ObjectStructure { bones: number; meshes: number; physics: boolean; hitboxes: number; animated: boolean; attachments: number; materials: string[] }
/** What an object holds beyond the first proven kind (Rust `ability_objects::Feature`). */
export type ObjectFeature =
  | "one_bone" | "bones" | "simple_collision" | "other_collision" | "hitboxes" | "animated"
  | "attachments" | "several_materials" | "no_material" | "placed" | "outline_shells" | "extra_meshes" | "body_groups"
  | "body_group_shells";
/** The kind of an offered object a switch opened (Rust `ObjectKind`); the page shows its texts as they are. */
export interface ObjectKind {
  features: ObjectFeature[];
  /** One line naming the kind. */
  label: string;
  /** What stays the game's, one item each. */
  kept: string[];
  /** Has collision: the view draws the game's collision shapes. */
  collision: boolean;
  /** The fit may stand the object on the game object's base. */
  stands_on: boolean;
  /** An object with several bones: the bones carrying its mesh (absent for other kinds). */
  moving?: ObjectMoving;
  /** What the build keeps of the game's own compiled data unchanged (Rust `GAME_BLOCKS_*`); absent for other kinds. */
  game_blocks?: ObjectGameBlock[];
}
/** Rust `ability_objects::GAME_BLOCKS_ANIMATION` / `GAME_BLOCKS_COLLISION`. */
export type ObjectGameBlock = "animation" | "collision";
export interface ObjectMoving {
  bones: string[]; skeleton: number; animated: boolean;
  /** Body-group states (Rust `Moving::per_state`): the app binds each state by its own bones, so no
   * bone is picked by hand; the line shown instead (as it is). Absent for every other object. */
  per_state?: string;
}
/** A bone carrying part of the game's mesh (Rust `object_follow::Carrier`). */
export interface ObjectCarrier { name: string; points: number; centre: [number, number, number]; share: number }
/** What the fit view says about an object with several bones (Rust `objects::MovingView`). */
export interface ObjectMovingView {
  carriers: ObjectCarrier[];
  default_bone: string;
  /** The user's file's separate objects (names), in file order. */
  parts: string[];
  /** The file's armature bones (same skeleton). */
  armature: string[];
  /** The way in force (the row's, else the default) and what each part follows. */
  way: ObjectFollowWay;
  bound: Record<string, string>;
  suggested: Record<string, string>;
  /** Why the row's way cannot be built (shown as it is), when it cannot. */
  refused?: string;
  /** Whether the same-skeleton way is open. */
  same_skeleton_open: boolean;
  /** Your model is drawn unbound (its lighter preview copy did not pair with its meshes): it stays at rest. */
  still?: boolean;
}
/** One of the object's own animations (Rust `object_motion::ClipInfo`). */
export interface ObjectClip {
  file: string; label: string; duration: number; additive: boolean;
  /** Whether the object's animation graph lets it loop (absent when no graph node plays it). */
  loops?: boolean;
}
/** The motion packet's header; the data is frames × bones row-major 3×4 float32 skin matrices.
 * `lines`: what the player shows differently from the game, shown as they are. */
export interface ObjectMotionHeader { bones: string[]; clips: ObjectClip[]; clip?: string; fps: number; frames: number; note?: string; lines?: string[] }
export const OBJECT_MOTION_MIME = "application/x-dmb-object-motion";
export const OBJECT_MOTION_NOT_READ = "The game animates this object in a way the app cannot show here; it shows your model at rest.";
/** The player's line for an object whose animation the build keeps as the game's own (shown instead of the note above). */
export const OBJECT_MOTION_KEPT = "Your build keeps the game's own animation of this object unchanged. It cannot be played here, so your model is shown at rest.";
/** One listed ability object of the hero. */
export interface ObjectRow {
  /** The game model (casefolded `.vmdl`), the saved row's `target`. */
  target: string;
  /** Readable name from the file name. */
  name: string;
  /** The file stem. */
  file: string;
  /** Ability group in the Effects page's order (101–104 signature abilities, 200/201 weapon, 210 melee, 300 innate, 350 the hero). */
  group: number;
  slot: string;
  ability: string;
  ability_name: string;
  /** `particle` (drawn only by effects), `definition`, `definition_and_particle`, or `file_name` (no game data names it: found by its file name beside a listed object). */
  drawn: 'particle' | 'definition' | 'definition_and_particle' | 'file_name';
  /** For an object found by its file name: one line from Rust saying so (shown as it is); absent otherwise. */
  found_by?: string;
  /** The hero's own effects that draw it. */
  drawn_by: ObjectDrawing[];
  /** The first material a live drawing renderer draws it with instead of its own. */
  material_override?: string;
  /** Every live drawing renderer overrides the material: the user's material will not show. */
  material_always_overridden: boolean;
  /** Effects draw it in one of the game's own looks of it (Rust `game_look`): `line` is shown as it is; `always`: the user's colors never show. Absent otherwise. */
  game_look?: { always: boolean; line: string };
  structure?: ObjectStructure;
  bounds?: ObjectBounds;
  /** Other live heroes that use the same file. */
  also_used_by: string[];
  offered: boolean;
  /** Why it cannot be replaced (one sentence), when not offered. */
  reason?: string;
  /** What kind it is, for an offered object beyond the first proven kind (absent for that kind). */
  kind?: ObjectKind;
}
export interface ObjectCatalog {
  hero_path: string;
  profile: string;
  rows: ObjectRow[];
  counts: { listed: number; offered: number; shared_shapes: number };
  /** Listed targets that are scenery models (Rust `is_scenery`, decided there); absent when none. */
  scenery?: string[];
}
/** The page's line for a scenery model (`ObjectCatalog.scenery`). */
export const SCENERY_NOTE = "This model is part of the game's scenery. If the map uses it too, your object shows there as well.";
/** The stamp of a preparation (Rust `ability_object::Prepared`). */
export interface ObjectPreparation {
  version: string;
  source: string;
  source_size: number;
  source_modified: number;
  source_sha256: string;
  scripts_sha256: string;
  root: string;
  packet_sha256: string;
  counts: { meshes: number; vertices: number; triangles: number; materials: number };
  warnings: string[];
  /** Bounds of the prepared points, in the file's units. */
  bounds_min: [number, number, number];
  bounds_max: [number, number, number];
  /** Metres per file unit. */
  unit_metres: number;
  /** Material path → its colour image, when one was found. */
  color_images: Record<string, string>;
}
export interface PreparedObject {
  target: string;
  source: string;
  /** The object's own folder in the game (its materials' prefix). */
  root: string;
  prepared: ObjectPreparation;
}
/** One preview packet (`src/preview/packet.ts` form) inside the fit view's data. */
export interface ObjectFitPacket {
  /** `game`: one of the game object's mesh files; `yours`: the user's preview mesh, already through the fit. */
  role: 'game' | 'yours';
  name: string;
  offset: number;
  length: number;
}
/**
 * The JSON header (`details`) of `abilityObjectFit`'s answer. The answer is
 * `[u32 header length][header {mime_type, details}][data]`, as the Extras
 * previews are; each packet is `data.slice(offset, offset + length)`. Both
 * kinds of packet are in the game object's frame (model space, inches).
 */
export interface ObjectFitView {
  target: string;
  packets: ObjectFitPacket[];
  /** The fit's map from the prepared file's coordinates into the game object's frame, row-major: p' = M·[p, 1]. */
  transform: [[number, number, number, number], [number, number, number, number], [number, number, number, number], [number, number, number, number]];
  game_bounds: ObjectBounds;
  /** The user's object's bounds after the fit (absent before a preparation). */
  yours_bounds?: ObjectBounds;
  fit: Required<Pick<ObjectFit, 'size' | 'scale'>> & { turns: Required<ObjectTurns>; on_base?: boolean };
  /** The game DMX's axis system (upAxis, forwardParity, coordSys), copied to the user's mesh. */
  axis?: [number, number, number];
  /** The game object's collision shapes (model space, inches), for an object with collision. */
  collision?: ObjectCollisionShape[];
  /** How far the user's fitted object sticks out of that collision, and the line to show. */
  collision_fit?: ObjectCollisionFit;
  /** An object with several bones: what it can follow and what each part follows (absent otherwise). */
  moving?: ObjectMovingView;
}
export type ObjectCollisionShape =
  | { shape: "sphere"; center: [number, number, number]; radius: number }
  | { shape: "capsule"; start: [number, number, number]; end: [number, number, number]; radius: number }
  /** A hull or mesh shape as its bounds (kept from the game's own collision). */
  | { shape: "box"; min: [number, number, number]; max: [number, number, number] };
export interface ObjectCollisionFit {
  /** At most, in inches. */
  inches: number;
  /** How far the game's own object sticks out. */
  game_inches: number;
  /** A calm line when true, a warning otherwise. */
  fits: boolean;
  line: string;
}
export const OBJECT_FIT_MIME = "application/x-dmb-object-fit";

// Messages the page compares (Rust constants; a test keeps them equal).
export const OBJECTS_NO_GAME = "Choose the current Deadlock pak01_dir.vpk in Setup to see the hero's ability objects";
export const OBJECTS_NO_BLENDER = "Ability objects are prepared with Blender. Choose blender.exe in Settings";
export const OBJECT_NOT_OFFERED = "This ability object cannot be replaced (the game changed it, or it is not one of this hero's objects)";
export const OBJECT_NOT_PREPARED = "This ability object's file is not prepared, or changed since it was prepared. Prepare it again in Extras → Objects";
export const OBJECT_UNSUPPORTED_FILE = "An ability object comes from a Blender, FBX, GLB or glTF file";
export const OBJECT_NEEDS_CURRENT_SDK = "Ability objects are compiled only with Reduced CSDK 13 or later. Choose it as the Community SDK in Settings and keep the model compiler on Auto";
