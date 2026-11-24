// Types of the effect editor's commands (`api.effectTrees`, `api.effectPreview`),
// as the Rust side serialises them (deadlock-components effect_trees.rs,
// effect_colors.rs, effect_catalog.rs, effect_build.rs), and the exact message
// texts of its errors. No logic: every color calculation is done in Rust and
// returned by `effectPreview`.

/** 0–255 per channel. */
export type Rgb = [number, number, number];
export type EffectMultiplier = 'size' | 'opacity' | 'glow';
/** How a strip is drawn: one color; two colors each particle picks between; a fade from the part's starting color; gradient stops at positions. */
export type EffectStripKind = 'constant' | 'random' | 'fade' | 'gradient';
/** What decided a color in a preview. */
export type EffectColorRule = 'replaced' | 'near' | 'shifted' | 'excluded' | 'unchanged';
export type EffectNoteKind =
  | 'inherited' | 'control_point' | 'other_input' | 'other_field' | 'other_function' | 'status_effect' | 'unchecked_texture'
  | 'model_material' | 'projected_material' | 'colored_texture';
/**
 * `changes`: an existing setting is multiplied; `adds`: a sprite, rope or trail without the setting gets it
 * (with the multiplier as its value); `no_setting`: this kind of part
 * (a model) has no such setting; `game_input`: the game sets the value while playing, so it is left alone.
 */
export type EffectSiteStatus = 'changes' | 'adds' | 'no_setting' | 'game_input';
/** `elsewhere`: no ability of the hero names these, other game effects or definitions do; `unlinked`: nothing in the game's data names them. */
export type EffectGroupKind = 'ability' | 'weapon' | 'melee' | 'innate' | 'hero' | 'elsewhere' | 'unlinked';
export type EffectBlend = 'additive' | 'normal' | 'mixed' | 'none';
export type EffectTag = 'hero' | 'ability' | 'weapon';

export interface EffectStop {
  /** 0–1 along the strip. */
  position: number;
  rgb: Rgb;
  hex: string;
  alpha?: number;
  /** The color's place in the file (the stop's id), or null for an estimate (a fade's start). */
  slot: string | null;
  /** For an estimate: the slot it was taken from (absent when the part starts white). */
  source?: string;
  /** Share of the tree's estimated visible color (0–1). */
  weight: number;
}
export interface EffectStrip {
  /** Stable within the part. */
  id: string;
  kind: EffectStripKind;
  /** Plain words, for example "color over its life". */
  role: string;
  function: string;
  stops: EffectStop[];
}
export interface EffectSwatch {
  rgb: Rgb;
  hex: string;
  /** Every distinct color merged into this swatch (`rgb` first). */
  members: Rgb[];
  /** Share of the tree's estimated visible color (0–1); swatches are sorted by it. */
  weight: number;
  parts: string[];
  slots: number;
  neutral?: 'grey' | 'white' | 'black';
}
export interface EffectMain {
  rgb: Rgb;
  hex: string;
  /** Index into the tree's palette. */
  swatch: number;
  /** Share of the colorful weight its hue family holds (0–1). */
  share: number;
}
export interface EffectNote {
  kind: EffectNoteKind;
  function: string;
  location: string;
  /** Plain words without "may not" / "does not"; the list the note is in says which. */
  reason: string;
}
export interface EffectSite {
  multiplier: EffectMultiplier;
  renderer: string;
  location: string;
  value: number | null;
  applies: boolean;
  status: EffectSiteStatus;
  reason: string;
}
export interface EffectPart {
  path: string;
  label: string;
  parent: string | null;
  depth: number;
  children: string[];
  /** The Effects catalog's association, or null outside it. */
  catalog: string | null;
  /** The catalog's folder-based `shared` flag (real reuse is the tree's `also_used_by`). */
  shared: boolean;
  /** Outside the hero's own folders (not in its Effects catalog, or in it as a linked shared file): an edit leaves it out unless it is listed in `EffectEdit.shared`. */
  outside_hero: boolean;
  /** Other trees of this hero that hold this file. */
  in_trees: string[];
  renderers: string[];
  blend: EffectBlend;
  visible: boolean;
  /** Share of the tree's estimated visible color (0–1). */
  weight: number;
  strips: EffectStrip[];
  /** Uncertain: may keep its color after a recolor. */
  may_not_follow: EffectNote[];
  /** Certain: keeps its own color. */
  does_not_follow: EffectNote[];
  multipliers: EffectSite[];
  /** Sprite size cap (fraction of the screen): the part does not grow past it ("size limited by the game"). */
  size_limit?: number;
  /** What the part draws (Advanced shows its label). */
  kind: EffectPartKind;
  /** Which of the part's own settings (`EffectEdit.parts`) it takes, and why not. */
  controls: EffectOffers;
}
/** A part's kind, named by its main renderer; `none` draws nothing. */
export type EffectPartKindName = 'line' | 'sprites' | 'trail' | 'model' | 'light' | 'decal' | 'other' | 'none';
export interface EffectPartKind {
  kind: EffectPartKindName;
  /** "Line", "Glow sprites", "3D model", …; a mixed part lists its kinds, main first ("Glow sprites + Light"). */
  label: string;
  mixed: boolean;
  /** One line about the kind (a light follows the effect's colors; a 3D model with no color of its own). */
  note?: string;
}
export interface EffectOffer {
  offered: boolean;
  /** Why not, in plain words. */
  reason?: string;
  /** False when the moving preview does not show this setting. */
  player_shows: boolean;
  /** When offered: what stays as it is (some of a part's lights), in plain words. */
  note?: string;
}
export interface EffectOffers {
  hide: EffectOffer;
  size: EffectOffer;
  opacity: EffectOffer;
  tint: EffectOffer;
  /** Light brightness: offered when at least one of the part's lights holds a brightness number the file sets itself. */
  light?: EffectOffer;
}
/** One of a part's own settings. */
export type EffectControl = 'hide' | 'size' | 'opacity' | 'tint' | 'light';
/** Light brightness limits (Rust `deadlock_project::effect_edit::LIGHT_MAX`); 1 is not saved, 0 dims. */
export const LIGHT_MIN = 0;
export const LIGHT_MAX = 50;
/** `EffectOffer.reason` for a part without a light (Rust `effect_edit::LIGHT_NONE`). */
export const LIGHT_NONE = 'no light in this part';
/** The settings of one part (`EffectEdit.parts`); every field absent when unused. */
export interface EffectPartSetting {
  /** Not drawn (its renderers are switched off; its child parts keep working). */
  hidden?: boolean;
  /** 3D-model size multiplier, above 0 and at most 16 (1 is not saved). */
  size?: number;
  /** 3D-model opacity multiplier, above 0 and at most 16 (1 is not saved). */
  opacity?: number;
  /** A color for a 3D model whose file has none. */
  tint?: Rgb;
  /** Light brightness multiplier, 0 to 50 (1 is not saved; 0 dims the light, it may not go out). */
  light?: number;
}
export type EffectUserKind = 'hero' | 'every_hero' | 'item' | 'other';
export interface EffectRecordUse {
  kind: EffectUserKind;
  record: string;
  name: string;
  heroes?: string[];
  /** The game definition file (kind `other`). */
  file?: string;
  /** Ready to show, for example "Borrowed Decree (Graves)". */
  text: string;
  /** The tree's files it names. */
  parts: string[];
}
export interface EffectUse {
  path: string;
  label: string;
  parts: string[];
}
export interface EffectAlsoUsed {
  records: EffectRecordUse[];
  effects: EffectUse[];
  /** "Also used by: …", ready to show. */
  note: string;
}
export interface EffectLeftOut {
  record: string;
  reason: string;
}
export interface EffectSiteCounts {
  changes: number;
  adds: number;
  no_setting: number;
  game_input: number;
  /** Parts the multiplier reaches. */
  parts_reached: number;
  visible_parts: number;
}
export interface EffectTree {
  /** The top's compiled path: the tree's id. */
  target: string;
  label: string;
  name: string;
  group: string;
  group_name: string;
  /** How the group was found: an own ability names the tree (`definition`), a model that ability names or the part whose low-violence version it is (`reference`), its file name only (`name`: a name stem, the ability's internal name or a hero-level word), its weapon or melee folder, the hero's model or animations, or none (elsewhere, unlinked). */
  grouped_by: 'definition' | 'reference' | 'name' | 'folder' | 'model' | 'none';
  tags: EffectTag[];
  records: string[];
  parts: EffectPart[];
  missing: string[];
  palette: EffectSwatch[];
  main: EffectMain | null;
  also_used_by?: EffectAlsoUsed;
  left_out?: EffectLeftOut[];
  shared_with_trees: string[];
  /** Parts with `outside_hero`. The `multipliers` counts describe the default edit, without these parts. */
  outside_parts: number;
  follow: { may_not: number; does_not: number };
  multipliers: Record<EffectMultiplier, EffectSiteCounts>;
}
export interface EffectGroup {
  key: string;
  name: string;
  order: number;
  kind: EffectGroupKind;
  record?: string;
  /** The game's name when the group's name is generic ("Weapon"). */
  record_name?: string;
  slot?: string;
  /** One line shown under the group's name. */
  note?: string;
  /** Shown folded until opened; search, the Changed filter and edited effects still reach into it. */
  folded?: boolean;
  /** 0 for an ability whose effects are not files of the hero's folders (its note says so). */
  trees: number;
}
export interface EffectCounts {
  trees: number;
  parts: number;
  files: number;
  linked: number;
  unlinked: number;
  with_note: number;
  files_in_several_trees: number;
  may_not_follow: number;
  does_not_follow: number;
  left_out: number;
}
export interface EffectTrees {
  schema: number;
  policy: string;
  hero: string;
  hero_name: string;
  sources: { definitions: boolean; names: boolean };
  groups: EffectGroup[];
  trees: EffectTree[];
  counts: EffectCounts;
  warnings: string[];
}

export interface EffectColorReplacement {
  from: Rgb;
  to: Rgb;
  /** Also colors close to `from` (hue within 20°, saturation and value within 0.3; greys within 0.12 of value); each keeps its difference from `from`. */
  near?: boolean;
}
/** The edit the page sends (and later saves). Every field is optional; `{}` changes nothing. */
export interface EffectEdit {
  /** Degrees, -360 to 360. */
  hue?: number;
  /** 0–4. */
  saturation?: number;
  /** 0–4. */
  brightness?: number;
  /** Parts (paths) that keep the game's colors entirely (neither the shift nor replacements apply). */
  excluded?: string[];
  /** Parts outside the hero's own folders (`outside_hero`) to change too; left out otherwise. `excluded` wins; paths that are no such part are ignored. Absent from saved JSON when empty. */
  shared?: string[];
  colors?: EffectColorReplacement[];
  /** Multipliers above 0, at most 16. `glow` is the page's Brightness. */
  size?: number;
  glow?: number;
  opacity?: number;
  /** Settings of single parts (path → settings; paths compared case-blind, at most 256). Absent when none. */
  parts?: Record<string, EffectPartSetting>;
}
/**
 * One saved effect edit (`customization.effect_edits`; Rust `deadlock_project::effect_edit::EffectEditRow`).
 * At most 256 rows, one per `target` (compared case-blind); unknown keys in the row or its `edit` are refused.
 * Limits of `edit`: hue -360–360, saturation and brightness 0–4, size, glow and opacity above 0 and at most 16,
 * at most 64 `colors` and 256 `excluded` and 256 `shared` parts (each a `.vpcf_c`, no repeats).
 */
export interface EffectEditRow {
  /** The tree's top `.vpcf_c` (`EffectTree.target`). */
  target: string;
  /** Default true. */
  enabled?: boolean;
  /** Default `{}` (no change). */
  edit?: EffectEdit;
  /** The main color the user chose instead of the detected one; absent when not corrected. */
  main?: Rgb;
}
export interface EffectPickRequest {
  color: Rgb;
  /** The main color the user chose instead of the tree's own. */
  main?: Rgb | null;
}
export interface EffectPick {
  hue: number;
  saturation: number;
  brightness: number;
  result: Rgb;
  result_hex: string;
  exact: boolean;
  reason?: string;
  warning?: string;
}
export interface EffectColorChange {
  before: Rgb;
  before_hex: string;
  after: Rgb;
  after_hex: string;
}
export interface EffectStopPreview {
  position: number;
  alpha?: number;
  before: Rgb;
  before_hex: string;
  after: Rgb;
  after_hex: string;
  slot: string | null;
  rule: EffectColorRule;
}
export interface EffectStripPreview {
  id: string;
  kind: EffectStripKind;
  role: string;
  stops: EffectStopPreview[];
}
export interface EffectSitePreview {
  multiplier: EffectMultiplier;
  renderer: string;
  location: string;
  before: number | null;
  after: number | null;
  applied: boolean;
  status: EffectSiteStatus;
  reason: string;
}
export interface EffectPartPreview {
  path: string;
  /** True for every left-out part. */
  excluded: boolean;
  /** Why it is left out (nothing about it changes): by the user (`excluded`) or as a part shared with other heroes not chosen in `shared`. */
  left_out: 'user' | 'shared' | null;
  strips: EffectStripPreview[];
  multipliers: EffectSitePreview[];
  /** This edit hides the part. */
  hidden: boolean;
  /** The part's own settings in this edit (empty without any). */
  controls: EffectControlPreview[];
}
export interface EffectControlPreview {
  control: EffectControl;
  renderer: string;
  location: string;
  before: number | null;
  after: number | null;
  /** The tint's color. */
  color?: Rgb;
  applied: boolean;
  /** `not_offered`: the part does not take this setting; `left_out`: the part is left out of the edit. */
  status: EffectSiteStatus | 'not_offered' | 'left_out';
  reason: string;
}
export interface EffectPreview {
  target: string;
  /** The edit as applied, every field present (after a pick, its hue, saturation and brightness are the pick's). */
  edit: Required<Omit<EffectEdit, 'colors' | 'shared' | 'parts'>> & { colors: Required<EffectColorReplacement>[]; shared?: string[]; parts?: Record<string, EffectPartSetting> };
  pick?: EffectPick;
  parts: EffectPartPreview[];
  /** One per palette swatch, in the tree's palette order. */
  palette: EffectColorChange[];
  main: EffectColorChange | null;
  changed: { colors: number; multiplied: number; not_multiplied: number; hidden: number; part_settings: number };
}

/** What a build did with one part of an edited effect (Rust `effect_build::Outcome`). */
export type EffectPartOutcomeKind = 'written' | 'unchanged' | 'excluded' | 'shared_left_out' | 'user_file' | 'other_effect' | 'same_as_other_effect';
export interface EffectPartOutcome {
  path: string;
  outcome: EffectPartOutcomeKind;
  /** For `other_effect` and `same_as_other_effect`: the effect (top path) whose edit is used. */
  follows?: string;
  /** For `user_file` and `other_effect`: what happens, in plain words. */
  note?: string;
}
/** One built effect edit: in the build's inspection (`effect_edits`) and receipt (`effect_edits.trees`). */
export interface EffectEditedTree {
  target: string;
  edit: EffectPreview['edit'];
  parts: EffectPartOutcome[];
}

/**
 * Error messages the effect commands return in `AppError.message` (exact text; compare with `===`).
 * A Rust test checks that these match `src-tauri/src/catalogs.rs` and `effect_trees.rs`.
 */
export const EFFECTS_NOT_LOADED = 'The effect list is not loaded. Open Extras → Effects again';
export const EFFECTS_GAME_CHANGED = 'The game files changed since the effects were loaded. Click Reload assets in Extras';
/** `message` starts with this when a preview names an effect the loaded list does not have. */
export const EFFECT_UNKNOWN_PREFIX = "This effect is not one of this hero's effects. Load the effects again";
/** A 3D part's kind note when its Tint is offered (Rust `effect_trees::NO_OWN_COLOR_NOTE`). */
export const NO_OWN_COLOR_NOTE = "This part has no color of its own in the game's files.";
