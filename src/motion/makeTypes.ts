// Motion → Make it from my animation: the saved row's `made` object and the
// commands' answers. Rust: deadlock-project `made_clip.rs` (the saved format),
// deadlock-preparation `clip_making.rs` (actions, packets, the report and the
// switches), deadlock-components `made_clips.rs` (the build), src-tauri
// `made_clips.rs` (the commands). The constants below are tested equal to
// Rust's in `troubleshooting.rs`.

/** How the pose is carried over. */
export type ClipMode = 'as_it_looks' | 'from_rest';
/** Which bones follow the animation. */
export type ClipBones = 'all' | 'keyed';

/**
 * A clip made from an animation: `made` on an animation row of
 * `customization.animations`, whose `source` is the animation file (one of
 * ANIMATION_FILE_SUFFIXES: a Blender, FBX or glTF file).
 * Absent unless set; refused on every other kind of row and with unknown keys.
 */
export type MadeClip = {
  /** The action's name in the file: an FBX take or a glTF animation (1 to MAX_ACTION_CHARS characters). */
  action: string;
  /** Default 'as_it_looks'. */
  mode?: ClipMode;
  /** Absent: the whole action stretched to the clip's length. A number (0 to MAX_PART_START): the part from this second at its own speed. */
  part_start_seconds?: number;
  /** Absent: no blend. More than 0 and at most MAX_LOOP_BLEND: the last seconds ease into the first frame. */
  loop_blend_seconds?: number;
  /** Default true (written only when false): the straight-line travel across the floor is removed. */
  in_place?: boolean;
  /** Default 'all'. */
  bones?: ClipBones;
};
export const MAX_ACTION_CHARS = 256;
export const MAX_PART_START = 3600;
export const MAX_LOOP_BLEND = 10;
/** The files a clip is made from (Rust `made_clip::SOURCE_SUFFIXES`). */
export const ANIMATION_FILE_SUFFIXES = ["blend", "fbx", "glb", "gltf"] as const;

/** One action of the user's animation file (`made_clip_actions`). */
export interface ActionInfo {
  name: string;
  frame_start: number;
  frame_end: number;
  fps: number;
  length_seconds: number;
  /** Bones of the file's armature the action has keys for. */
  keyed_bones: string[];
  /** How many of them drive a hero bone (0: the action moves nothing of the character). */
  driving_keyed_bones: number;
}
/** The actions of a file and whether its skeleton is the prepared one. */
export interface ActionList {
  /** The file, resolved. */
  source: string;
  rig: string;
  /** Bones of the file's armature ("The same skeleton as your prepared character (N bones)."). */
  bones: number;
  /** How many of them drive a hero bone: what an action's `driving_keyed_bones` is out of. */
  driving_bones: number;
  /** Null when the skeleton is the prepared one; else the sentence to show (starts with SKELETON_DIFFERS). */
  skeleton_error: string | null;
  actions: ActionInfo[];
}
/** `made_clip_check`: said on the clip before anything is chosen. */
export interface MadeClipCheck {
  target: string;
  /** ADDITIVE, MODEL_SPACE, SINGLE_POSE or SCALED when this clip cannot be made from an animation. */
  refusal: string | null;
  /** NEEDS_PREPARED or NO_FIT when the prepared model cannot be used yet; READY_MADE_MODEL or UNRIGGED_MODEL when this project's model can never be used (no rigged preparation). */
  fit_error: string | null;
}
/** The switches every made clip uses (one place in Rust: `clip_making::SWITCHES`). */
export interface MadeSwitches {
  /** A still track the animation sets to another still value gets it (off: the game's). */
  new_fixed_values: boolean;
  /** A moving track whose values leave its stored range gets a new range (off: never; values clamped). */
  refit_ranges: boolean;
  /** A still track the animation moves becomes a moving one (off: held at its first frame's value). */
  still_to_moving: boolean;
  /** Feet on the floor: one lift for the whole clip (absent while off). */
  floor_lift?: boolean;
  /** The off hand stays on the gun (absent while off). */
  off_hand?: boolean;
}
/** What the off-hand pass did (`clip_author::OffHand`). */
export interface MadeOffHand {
  gun_hand: string;
  off_hand: string;
  out_of_reach_frames: number;
  worst_miss_inches: number;
  game_grip_change_degrees: number;
  game_grip_change_inches: number;
}
/** The packet's stamp: what was sampled in Blender. */
export interface MadePacket {
  version: string;
  source: string;
  source_size: number;
  source_modified: number;
  source_sha256: string;
  action: string;
  mode: ClipMode;
  fit_sha256: string;
  script_sha256: string;
  packet_sha256: string;
  samples: number;
  length_seconds: number;
  fps: number;
}
/** What making one clip did (`made_clip_report`, `make_clip`). Bone names are the clip skeleton's. */
export interface MadeReport {
  action: string;
  mode: ClipMode;
  switches: MadeSwitches | null;
  clip_frames: number;
  clip_seconds: number;
  action_seconds: number;
  /** Action seconds at the first and last clip frame. */
  plays_from: number;
  plays_to: number;
  /** The action's length over the clip's when stretched (1: its own speed). */
  stretch: number;
  /** Bones that follow the animation. */
  follow: string[];
  /** Bones carried rigidly by a followed bone, as the game's clip holds them (hand and leg IK targets). */
  carried: string[];
  /** Bones between followed ones the file has no bone for (held at the hero's rest). */
  between: string[];
  /** Bones that keep the game's motion, counted by kind ('fingers', 'gun', 'cloth', 'face', 'IK targets', 'other'), with up to three names each. */
  kept_count: number;
  kept_kinds: Record<string, number>;
  kept_examples: Record<string, string[]>;
  /** Hero bones of the animation the game's clip does not have. */
  not_in_clip: string[];
  /** Still tracks (fixed values in the game's clip): made moving, given a new fixed value, held, or kept at the game's value. */
  still_made_moving: string[];
  still_new_fixed_value: string[];
  still_held: string[];
  still_kept_game_value: string[];
  ranges_refitted: string[];
  ranges_clamped: string[];
  /** 16-bit words per frame before and after. */
  words_per_frame: [number, number];
  /** How far the last frame was from the first before the loop blend. */
  loop_gap_degrees: number;
  loop_gap_inches: number;
  loop_blend_seconds: number | null;
  /** The horizontal travel removed by "in place" (inches), when on. */
  drift_removed_inches: number | null;
  /** The lowest foot against where the game's clip puts it, lowest and highest over the frames (inches; negative is lower). */
  feet_lowest_inches: number | null;
  feet_highest_inches: number | null;
  /** Feet on the floor: the lift the clip holds (inches; absent when not applied). */
  floor_lift_inches?: number;
  /** The off hand on the gun: what the pass did, or why it did not run (absent when not asked for). */
  off_hand?: MadeOffHand;
  off_hand_skipped?: string;
}
/** `make_clip`'s answer. */
export interface MadeClipAnswer {
  target: string;
  made: MadePacket;
  report: MadeReport;
}

// Messages the page compares (the error's `message`, or the start of it).
export const NEEDS_PREPARED = "Prepare the model first: a clip is made from an animation of the prepared character";
export const NO_FIT = "This character was prepared before clips could be made from an animation. Prepare the model again on Prepare model, then make the clip";
export const READY_MADE_MODEL = "This project uses a ready-made DMX model. A clip is made from an animation of a rigged character that the app prepared: prepare your rigged character file on Prepare model, then make the clip";
export const UNRIGGED_MODEL = "This model was prepared without a skeleton of its own (Unrigged humanoid), so there is no animation of it to make a clip from. A clip is made from an animation of a rigged character that the app prepared: prepare a rigged character file on Prepare model, then make the clip";
export const NOT_MADE = "This clip is not made from its animation yet, or the animation file, the animation or the prepared model changed since. Make the clip again on the Motion page";
export const ONLY_ANIMATION_FILES = "A clip is made from an animation in a Blender, FBX or glTF file (.blend, .fbx, .glb or .gltf)";
export const GLTF_TOO_LONG = "This glTF file's path is too long for Blender to open";
export const CHANGED_WHILE_MAKING = "The animation file or the prepared model changed while the clip was being made. Make the clip again.";
export const ADDITIVE = "This clip cannot be made from an animation. The game adds it on top of other clips (a lean, not a full pose), and the app can only make full poses. A compiled clip file can still replace it.";
export const MODEL_SPACE = "This clip cannot be made from an animation. The game samples some of its bones in the model's space, which the app does not write. A compiled clip file can still replace it.";
export const SINGLE_POSE = "This clip cannot be made from an animation. It holds a single pose, not a motion. A compiled clip file can still replace it.";
export const SCALED = "This clip cannot be made from an animation. Some of its bones change size, which the app does not write. A compiled clip file can still replace it.";
export const SKELETON_DIFFERS = "This file's skeleton differs from your prepared character";
export const SKELETON_ADVICE = "Animate the file you prepared, or a copy of it.";
export const NO_RIG = "This file has no armature with the bones of your prepared character.";
export const NO_ACTION = "The file has no animation named ";
export const PART_TOO_LONG = "The part of the animation that plays does not fit: it starts too late for the clip's length";
export const MADE_NO_BLENDER = "Clips are made from an animation with Blender. Choose blender.exe in Settings";
export const MADE_SUPERSEDED = "superseded";
export const MADE_NOT_A_ROW = "Choose the animation file and the animation the clip is made from";
export const MADE_CHANGED_SINCE_BUILD = "A clip made from an animation changed since this build. Rebuild before installing.";
