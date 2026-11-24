// Motion → Take it from another hero's animation: the saved row's `taken`
// object and the commands' answers. Rust: deadlock-project `taken_clip.rs`
// (the saved format), deadlock-components `hero_clips.rs` (the retarget, the
// build, the install check and what the page reads), src-tauri
// `taken_clips.rs` (the commands). The constants below are tested equal to
// Rust's in `troubleshooting.rs`.
import type { ClipDirection, ClipGroup } from '../components/AssetCatalog';
import type { MadeReport } from './makeTypes';

/** How the other hero's clip fills this hero's clip (whose length the game fixes). */
export type TakenTiming = 'stretched' | 'own_speed';
/** Which bones follow the other hero: the whole body, or all but the arms and hands. */
export type TakenArms = 'source' | 'own';

/**
 * A clip taken from another hero: `taken` on an animation row of
 * `customization.animations`. The row has no `source` and no `made`.
 * Absent unless set; refused on every other kind of row and with unknown keys.
 */
export type TakenClip = {
  /** The other hero: its roster model path (`models/…/<hero>.vmdl`, SourceHero.hero). */
  hero: string;
  /** The other hero's clip: its compiled game path (`….vnmclip_c`, SourceClip.clip). */
  clip: string;
  /** Default 'stretched': the whole clip, faster or slower, over this hero's clip. 'own_speed': the part from `start_seconds` at its own speed. */
  timing?: TakenTiming;
  /** With 'own_speed' only (0 to MAX_TAKEN_START; absent is 0). */
  start_seconds?: number;
  /** Default 'source': the whole body follows the other hero. 'own': the arms and hands keep this hero's own motion (its own grip). */
  arms?: TakenArms;
};
export const MAX_TAKEN_START = 3600;
export const MAX_TAKEN_PATH_CHARS = 512;
/** The arm and hand bones 'own' leaves to this hero (Rust `hero_clips::ARMS`). */
export const TAKEN_ARM_BONES = ["clavicle_L", "arm_upper_L", "arm_lower_L", "hand_L", "clavicle_R", "arm_upper_R", "arm_lower_R", "hand_R"] as const;
/** Clips that belong together, by name (Rust `hero_clips::SETS`). */
export const TAKEN_SETS: Record<string, readonly string[]> = {
  dash: ["dash_ground", "dash_air_forward", "dash_air_back", "dash_air_left", "dash_air_right"],
  slide: ["slide_start", "slide_loop", "slide_getup"],
};

/** `taken_clip_check`: why this hero's clip cannot be taken (TAKEN_TARGET_ADDITIVE or TAKEN_TARGET_NOT_FULL), or null when it can. */
export interface TakenClipCheck { target: string; refusal: string | null }
/** One hero a clip can be taken from (`taken_clip_heroes`). */
export interface SourceHero {
  /** The roster's model path: what `TakenClip.hero` holds. */
  hero: string;
  name: string;
  /** Whether its body can be carried onto this hero (decided from both skeletons' bones). */
  usable: boolean;
  /** Why not, one sentence (TAKEN_BODY_NOT_CARRIED, TAKEN_NO_SOURCE_HERO or TAKEN_SOURCE_UNREADABLE first), when not usable. */
  reason?: string;
}
/** One clip of the other hero for this hero's chosen clip. */
export interface SourceClip {
  /** Its game path: what `TakenClip.clip` holds. */
  clip: string;
  label: string;
  name: string;
  group: ClipGroup;
  direction?: ClipDirection;
  seconds: number;
  frames: number;
  /** 0: the same name; 1: the same group and direction; 2: the same group; 3: the rest. The list comes in this order. */
  rank: 0 | 1 | 2 | 3;
  /** False for layers (TAKEN_SOURCE_ADDITIVE). */
  usable: boolean;
  reason?: string;
}
/** One clip of a set: this hero's clip and the other hero's of the same name. */
export interface SetMember {
  label: string;
  /** This hero's clip (game path), when the hero has it. */
  target?: string;
  /** The other hero's clip of the same name, when that hero has it. */
  clip?: string;
  /** Both are there and both are full poses: a row can be written for it. */
  usable: boolean;
}
export interface ClipSet { name: string; members: SetMember[] }
/** `taken_clip_sources`. */
export interface TakenSources {
  target: string;
  hero: string;
  clips: SourceClip[];
  /** The set this hero's clip belongs to (the dash: dash_ground and the four air dashes), when it is in one. */
  set?: ClipSet;
}
/** What a taken clip makes (`taken_clip_report`). Bone names are this hero's clip skeleton's. */
export interface TakenReport {
  /** The author's own report, as for a made clip (follow, carried, kept, still tracks, lift, …). */
  made: MadeReport;
  /** The other hero's name. */
  source_name: string;
  /** Body bones that follow the other hero. */
  driven: string[];
  /** Body bones left to this hero's own motion by arms 'own'. */
  own: string[];
  /** This hero's bones the other hero has no bone of that name for (they keep this hero's motion). */
  not_matched: string[];
  /** How much faster (positive) or slower (negative) the other hero's clip plays, in percent (0 at its own speed). */
  speed_percent: number;
  /** The one lift of the whole clip that puts the feet on this hero's floor (inches; absent when none). */
  floor_lift_inches?: number;
  /** This hero's leg length over the other hero's. */
  size: number;
}

// Messages the page compares (the error's `message`, or the start of it).
export const TAKEN_TARGET_ADDITIVE = "This clip cannot be taken from another hero. The game adds it on top of other clips (a lean or a layer, not a full pose), and only full poses can be taken. A compiled clip file can still replace it.";
export const TAKEN_TARGET_NOT_FULL = "This clip cannot be taken from another hero. The game stores it in a way the app does not write (a single pose, bones sampled in the model's space, or bones that change size). A compiled clip file can still replace it.";
export const TAKEN_SOURCE_ADDITIVE = "That clip of the other hero cannot be used. The game adds it on top of other clips (a lean or a layer, not a full pose): choose one of that hero's full-pose clips.";
export const TAKEN_SOURCE_MODEL_SPACE = "That clip of the other hero cannot be used. The game samples some of its bones in the model's space, which the app does not read: choose another of that hero's clips.";
export const TAKEN_NO_SOURCE_HERO = "The hero this clip is taken from is not in the game any more. Choose another hero for the clip on the Motion page";
export const TAKEN_NO_SOURCE_CLIP = "The other hero's clip this clip is taken from is not in the game any more. Choose another of that hero's clips on the Motion page";
export const TAKEN_SAME_HERO = "A clip is taken from another hero, not from this hero itself. Choose another hero for the clip on the Motion page.";
export const TAKEN_BODY_NOT_CARRIED = "These two heroes do not share the body a clip is carried on";
export const TAKEN_SOURCE_UNREADABLE = "The other hero's clip could not be read";
export const TAKEN_PART_TOO_LONG = "The part of the other hero's clip that plays does not fit: it starts too late for this clip's length";
export const TAKEN_GAME_CHANGED = "The other hero's clip or a hero's skeleton changed in the game since this build. Rebuild before installing.";
export const TAKEN_CHANGED_SINCE_BUILD = "A clip taken from another hero changed since this build. Rebuild before installing.";
export const TAKEN_ONLY_ANIMATIONS = "Only animations can be taken from another hero;";
export const TAKEN_NOT_BOTH = "A clip is either made from your animation or taken from another hero, not both. Choose one way for this clip.";
export const TAKEN_NO_FILE = "A clip taken from another hero has no file of its own. Remove the file, or choose A compiled clip file for this clip instead.";
export const TAKEN_BAD_SOURCE = "Choose the other hero (its model, .vmdl) and one of its clips (.vnmclip_c) for the clip taken from another hero.";
export const TAKEN_BAD_START = "The part of the other hero's clip that plays must start from 0 to 3600 seconds into it, and only when it plays at its own speed.";
export const TAKEN_SUPERSEDED = "superseded";
export const TAKEN_NOT_A_ROW = "Choose the other hero and the clip this clip is taken from";
export const TAKEN_NOT_A_CLIP = "Choose one of this hero's clips to take from another hero";
