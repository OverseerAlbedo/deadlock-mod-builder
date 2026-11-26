/** Rows of `customization.animations` made from an animation (Motion → Make it from my animation),
 * and the words the page shows for the engine's report. Pure functions: nothing here reads or
 * writes a project. A made row is `{enabled, source: <animation file>, target, allow_shared: false,
 * made}`; `made` holds only the keys of the saved format (`deadlock-project made_clip.rs`), in its
 * order, with `mode` and `bones` always written and `part_start_seconds`, `loop_blend_seconds` and
 * `in_place` written only when they differ from the engine's defaults, exactly as Rust saves them.
 * The words for the report come in the interface language and are only drawn; bone names and the
 * kinds of bones the engine names stay as they come. */
// i18n-area: motion
import type { ResourceReplacement } from '../models';
import { t, tc, tp } from '../i18n';
import { nameList } from '../build/words';
import {
  ANIMATION_FILE_SUFFIXES, MAX_ACTION_CHARS, MAX_LOOP_BLEND, MAX_PART_START,
  type ActionInfo, type ActionList, type ClipBones, type ClipMode, type MadeClip, type MadeReport,
} from './makeTypes';

/** The loop blend a new Loop starts with (seconds). */
export const DEFAULT_LOOP_BLEND = 0.25;
/** The shortest loop blend the page offers (seconds; the saved format takes anything above 0). */
export const MIN_LOOP_BLEND = 0.05;
/** "Every matched bone" is the default when the animation has keys for at least this share of the bones. */
export const ALL_BONES_SHARE = 0.9;
/** Travel removed by Stay in place below this (inches) is a trace, not worth a line. */
export const TRACE_INCHES = 0.1;
/** Feet ending closer to the floor than this (inches) get no line. */
export const FEET_INCHES = 0.25;

/** A file a clip can be made from: Blender, FBX or glTF (`ANIMATION_FILE_SUFFIXES`). */
export const isAnimationFile = (path: string | undefined | null) => {
  const text = (path ?? '').trim().toLowerCase();
  const dot = text.lastIndexOf('.');
  return dot > 0 && (ANIMATION_FILE_SUFFIXES as readonly string[]).includes(text.slice(dot + 1));
};
/** The row's `made` object, when it is a made clip. */
export const madeOf = (row: ResourceReplacement | null | undefined): MadeClip | null =>
  row?.made && typeof row.made === 'object' && typeof row.made.action === 'string' ? row.made : null;

const round = (value: number, step: number) => Math.round(value / step) * step;
const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));
/** Seconds kept to the millisecond (no float noise in the saved file). */
const ms = (value: number) => Number(round(value, 0.001).toFixed(3));

/** `made` with only the saved format's keys, in its order, limits applied. Unknown keys are
 * dropped (Rust refuses them). */
export function cleanMade(made: MadeClip): MadeClip {
  const out: MadeClip = { action: made.action.slice(0, MAX_ACTION_CHARS), mode: made.mode === 'from_rest' ? 'from_rest' : 'as_it_looks' };
  if (typeof made.part_start_seconds === 'number' && Number.isFinite(made.part_start_seconds))
    out.part_start_seconds = ms(clamp(made.part_start_seconds, 0, MAX_PART_START));
  if (typeof made.loop_blend_seconds === 'number' && Number.isFinite(made.loop_blend_seconds) && made.loop_blend_seconds > 0)
    out.loop_blend_seconds = ms(clamp(made.loop_blend_seconds, 0.001, MAX_LOOP_BLEND));
  if (made.in_place === false) out.in_place = false;
  out.bones = made.bones === 'keyed' ? 'keyed' : 'all';
  return out;
}

/** The default for "which bones": every matched bone when the animation has keys for at least
 * nine tenths of the bones that drive the hero, else only the bones it moves. */
export function defaultBones(action: Pick<ActionInfo, 'driving_keyed_bones'>, list: Pick<ActionList, 'driving_bones'>): ClipBones {
  return list.driving_bones > 0 && action.driving_keyed_bones >= ALL_BONES_SHARE * list.driving_bones ? 'all' : 'keyed';
}

/** The row for a clip made from `action` of `source`. Choosing another animation keeps the mode,
 * Stay in place and Loop of the row it replaces; the part that plays starts again from the whole
 * animation, and which bones follow is the default for the new animation. */
export function madeRow(target: string, source: string, action: ActionInfo, list: Pick<ActionList, 'driving_bones'>, before?: ResourceReplacement | null): ResourceReplacement {
  const old = madeOf(before);
  const made: MadeClip = { action: action.name, mode: old?.mode ?? 'as_it_looks', bones: defaultBones(action, list) };
  if (old?.loop_blend_seconds !== undefined) made.loop_blend_seconds = old.loop_blend_seconds;
  if (old?.in_place === false) made.in_place = false;
  return { enabled: true, source, target, allow_shared: false, made: cleanMade(made) };
}

/** A change of one option: `null` removes a key (whole animation stretched, no loop blend). */
export interface MadeChange {
  mode?: ClipMode; bones?: ClipBones; in_place?: boolean;
  part_start_seconds?: number | null; loop_blend_seconds?: number | null;
}
/** The row with one option changed (every other field of the row kept). */
export function withMade(row: ResourceReplacement, change: MadeChange): ResourceReplacement {
  const old = madeOf(row);
  if (!old) return row;
  const made: MadeClip = { ...old };
  if (change.mode) made.mode = change.mode;
  if (change.bones) made.bones = change.bones;
  if (change.in_place !== undefined) made.in_place = change.in_place;
  if (change.part_start_seconds === null) delete made.part_start_seconds;
  else if (change.part_start_seconds !== undefined) made.part_start_seconds = change.part_start_seconds;
  if (change.loop_blend_seconds === null) delete made.loop_blend_seconds;
  else if (change.loop_blend_seconds !== undefined) made.loop_blend_seconds = change.loop_blend_seconds;
  return { ...row, made: cleanMade(made) };
}

/** The row without `made` (a compiled clip file chosen for a made row). */
export function withoutMade(row: ResourceReplacement): ResourceReplacement {
  const { made: _made, ...rest } = row;
  return rest;
}

/** Whether going from `before` to `after` needs Blender again (another file, animation or mode);
 * every other option is made from the sampled animation without it. */
export function needsBlender(before: ResourceReplacement | null | undefined, after: ResourceReplacement): boolean {
  const a = madeOf(before), b = madeOf(after);
  if (!b) return false;
  return !a || (before?.source ?? '').trim() !== (after.source ?? '').trim() || a.action !== b.action || (a.mode ?? 'as_it_looks') !== (b.mode ?? 'as_it_looks');
}

/** The latest start of a part at its own speed: the animation's length less the clip's (0 when the
 * animation is not longer than the clip). */
export const latestPartStart = (actionSeconds: number, clipSeconds: number) => Math.max(0, Math.min(MAX_PART_START, actionSeconds - clipSeconds));
/** A start position for the strip, rounded to 0.01 s and kept where the part fits. */
export const partStart = (value: number, actionSeconds: number, clipSeconds: number) => Number(clamp(round(value, 0.01), 0, latestPartStart(actionSeconds, clipSeconds)).toFixed(2));
/** A loop blend length for the slider, rounded to 0.01 s, from MIN_LOOP_BLEND to the clip's length (at most MAX_LOOP_BLEND). */
export const loopBlend = (value: number, clipSeconds: number) => Number(clamp(round(value, 0.01), MIN_LOOP_BLEND, Math.max(MIN_LOOP_BLEND, Math.min(MAX_LOOP_BLEND, clipSeconds))).toFixed(2));

/** Frames of an action, counted the way the clip list counts them (both ends included). */
export const actionFrames = (action: Pick<ActionInfo, 'frame_start' | 'frame_end'>) => Math.max(1, Math.round(action.frame_end - action.frame_start) + 1);
const secs = (value: number) => t('{value} s', { value: value.toFixed(2) });
/** "Wave · 2.00 s · 61 frames". */
export const actionLabel = (action: ActionInfo) => `${action.name} · ${secs(action.length_seconds)} · ${tp(actionFrames(action), { one: '{count} frame', other: '{count} frames' })}`;

/** How the speed changes, in words, from the report. */
export function speedWords(report: Pick<MadeReport, 'stretch' | 'clip_seconds' | 'action_seconds' | 'plays_from' | 'plays_to'>, stretched: boolean): string {
  const clip = secs(report.clip_seconds);
  if (!stretched) return t('The game fixes this clip’s length at {clip}. Your animation plays at its own speed, from {from} to {to}.', { clip, from: secs(report.plays_from), to: secs(report.plays_to) });
  const s = report.stretch;
  const values = { clip, action: secs(report.action_seconds) };
  if (!(s > 0) || Math.abs(s - 1) < 0.005) return t('The game fixes this clip’s length at {clip}. Your {action} animation fits it at its own speed.', values);
  if (s > 1) return s < 1.15 ? t('The game fixes this clip’s length at {clip}. Your {action} animation plays a little faster to fit it.', values) : t('The game fixes this clip’s length at {clip}. Your {action} animation plays {factor} times as fast to fit it.', { ...values, factor: s.toFixed(1) });
  return s > 1 / 1.15 ? t('The game fixes this clip’s length at {clip}. Your {action} animation plays a little slower to fit it.', values) : t('The game fixes this clip’s length at {clip}. Your {action} animation plays at {percent}% of its speed to fit it.', { ...values, percent: Math.round(s * 100) });
}

const SIDES: Record<string, string> = { l: 'left', r: 'right', left: 'left', right: 'right' };
/** A bone name as words: "arm_upper_L" → "left arm upper", "spine_2" → "spine 2". */
export function readableBone(name: string): string {
  const words = name.replace(/([a-z])([A-Z])/g, '$1 $2').split(/[_\s.:-]+/).filter(Boolean).map(w => w.toLowerCase()).map(w => w === 'iktarget' ? 'IK target' : w);
  const at = words.length > 1 ? words.findIndex(w => SIDES[w]) : -1;
  if (at < 0) return words.join(' ');
  return [SIDES[words[at]], ...words.filter((_, i) => i !== at)].join(' ');
}
/** Up to `count` readable names, "…" when there are more. */
export function someBones(names: string[], count = 4): string {
  const shown = names.slice(0, count).map(readableBone);
  return names.length > count ? `${nameList(shown)}${tc('list separator', ', ')}…` : nameList(shown);
}

/** One line of "What the app makes". */
export interface MakesLine { id: string; mark: 'kept' | 'yours' | 'note'; text: string; side?: string }
/** "What the app makes", only from the report. A made clip always keeps the game's length, events
 * and root motion, so there is no pass or fail here. */
export function makesLines(report: MadeReport): MakesLine[] {
  const fromFile = t('from your file');
  const out: MakesLine[] = [{ id: 'kept', mark: 'kept', text: t('Length, events and root motion are the game’s own'), side: t('kept by the app') }];
  out.push({ id: 'follow', mark: 'yours', text: report.follow.length
    ? tp(report.follow.length, { one: '{count} bone follows your animation ({bones})', other: '{count} bones follow your animation ({bones})' }, { bones: someBones(report.follow) })
    : tp(0, { one: '{count} bone follows your animation', other: '{count} bones follow your animation' }), side: fromFile });
  if (report.carried.length) out.push({ id: 'carried', mark: 'note', text: tp(report.carried.length, { one: '{count} bone is carried with them, as the game’s clip carries them ({bones})', other: '{count} bones are carried with them, as the game’s clip carries them ({bones})' }, { bones: someBones(report.carried, 3) }) });
  if (report.between.length) out.push({ id: 'between', mark: 'note', text: tp(report.between.length, { one: '{count} bone between them stays at the hero’s rest: your file has no bone for it ({bones})', other: '{count} bones between them stay at the hero’s rest: your file has no bone for them ({bones})' }, { bones: someBones(report.between, 3) }) });
  // Kinds by count, "other" last (it names no kind).
  const kinds = Object.entries(report.kept_kinds).filter(([kind, n]) => n > 0 && kind !== 'other').sort((a, b) => b[1] - a[1]).map(([kind]) => kind);
  if ((report.kept_kinds.other ?? 0) > 0 && kinds.length) kinds.push('other');
  if (report.kept_count > 0) out.push({ id: 'game', mark: 'note', text: kinds.length
    ? tp(report.kept_count, { one: '{count} bone keep the game’s motion ({kinds})', other: '{count} bones keep the game’s motion ({kinds})' }, { kinds: nameList(kinds) })
    : tp(report.kept_count, { one: '{count} bone keep the game’s motion', other: '{count} bones keep the game’s motion' }), side: t('unchanged') });
  if (report.still_made_moving.length) out.push({ id: 'moving', mark: 'note', text: tp(report.still_made_moving.length, { one: '{count} bone the game holds still now moves ({bones})', other: '{count} bones the game holds still now move ({bones})' }, { bones: someBones(report.still_made_moving, 3) }), side: fromFile });
  if (report.still_new_fixed_value.length) out.push({ id: 'fixed', mark: 'note', text: tp(report.still_new_fixed_value.length, { one: '{count} bone the game holds still gets a new fixed position ({bones})', other: '{count} bones the game holds still get a new fixed position ({bones})' }, { bones: someBones(report.still_new_fixed_value, 3) }), side: fromFile });
  const lift = liftLine(report);
  if (lift) out.push({ id: 'lift', mark: 'note', text: lift });
  const feet = feetLine(report);
  if (feet) out.push({ id: 'feet', mark: 'note', text: feet });
  return out;
}

/** "Raised by 0.8 in so the feet stand on the floor" (or lowered), when the clip holds a lift. */
export function liftLine(report: Pick<MadeReport, 'floor_lift_inches'>): string | null {
  const lift = report.floor_lift_inches;
  if (typeof lift !== 'number' || !Number.isFinite(lift)) return null;
  const amount = Math.abs(lift).toFixed(1);
  if (amount === '0.0') return null;
  return lift > 0 ? t('Raised by {amount} in so the feet stand on the floor', { amount }) : t('Lowered by {amount} in so the feet stand on the floor', { amount });
}

/** "In this clip the feet end 0.8 in below the floor", when more than a quarter of an inch. */
export function feetLine(report: Pick<MadeReport, 'feet_lowest_inches'>): string | null {
  // Lowest and highest over the frames of the lowest foot against the game's clip: below when it
  // goes deeper than the limit, above only when it never comes nearer than the limit.
  const low = report.feet_lowest_inches;
  if (typeof low !== 'number') return null;
  if (low < -FEET_INCHES) return t('In this clip the feet end {amount} in below the floor', { amount: (-low).toFixed(1) });
  if (low > FEET_INCHES) return t('In this clip the feet end {amount} in above the floor', { amount: low.toFixed(1) });
  return null;
}

/** The loop's gap before the blend, in words. */
export function gapWords(report: Pick<MadeReport, 'loop_gap_degrees' | 'loop_gap_inches'>): string {
  const deg = report.loop_gap_degrees, inch = report.loop_gap_inches;
  if (deg < 0.5 && inch < 0.05) return t('Your animation already ends where it starts.');
  return t('Before the blend, its last frame is {degrees}° and {inches} in away from its first.', { degrees: deg.toFixed(deg < 10 ? 1 : 0), inches: inch.toFixed(1) });
}

/** The error's message (Tauri errors are objects with `message`). */
export function messageOf(error: unknown): string {
  return error && typeof error === 'object' && 'message' in error ? String((error as { message: unknown }).message) : String(error);
}
/** Whether an error is the engine's `constant` sentence (the message, or the start of it). */
export const isMessage = (error: unknown, constant: string) => messageOf(error).startsWith(constant);
