/** Rows of `customization.animations` taken from another hero (Motion → Take it from another
 * hero's animation). Pure functions: nothing here reads or writes a project. A taken row is
 * `{enabled, target, allow_shared: false, taken}` with no `source` and no `made`; `taken` holds only
 * the keys of the saved format (`deadlock-project taken_clip.rs`), in its order, with `timing` and
 * `arms` always written and `start_seconds` only with 'own_speed', exactly as Rust saves them.
 * The three ways exclude each other: a taken row has no file and no `made`; choosing a compiled
 * file (`withCompiledFile`) or making the clip from an animation (`madeRows.madeRow`, a new row)
 * leaves no `taken`. */
import type { ResourceReplacement } from '../models';
import { MAX_TAKEN_START, type ClipSet, type TakenArms, type TakenClip, type TakenTiming } from './takenTypes';

/** The row's `taken` object, when it is a taken clip. */
export const takenOf = (row: ResourceReplacement | null | undefined): TakenClip | null =>
  row?.taken && typeof row.taken === 'object' && typeof row.taken.hero === 'string' && typeof row.taken.clip === 'string' ? row.taken : null;

const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));
/** Seconds kept to the millisecond (no float noise in the saved file). */
const ms = (value: number) => Number((Math.round(value * 1000) / 1000).toFixed(3));

/** `taken` with only the saved format's keys, in its order, limits applied. Unknown keys are
 * dropped (Rust refuses them); a start without 'own_speed' is dropped. */
export function cleanTaken(taken: TakenClip): TakenClip {
  const timing: TakenTiming = taken.timing === 'own_speed' ? 'own_speed' : 'stretched';
  const out: TakenClip = { hero: taken.hero, clip: taken.clip, timing };
  if (timing === 'own_speed' && typeof taken.start_seconds === 'number' && Number.isFinite(taken.start_seconds))
    out.start_seconds = ms(clamp(taken.start_seconds, 0, MAX_TAKEN_START));
  out.arms = taken.arms === 'own' ? 'own' : 'source';
  return out;
}

/** The row for this hero's clip `target` taken from the other hero's clip `clip`. Choosing another
 * hero or clip keeps the timing and the arms of the row it replaces; a part at its own speed starts
 * again from the beginning. Whatever the row was before (a compiled file, made from an animation),
 * it has no `source` and no `made` now. */
export function takenRow(target: string, hero: string, clip: string, before?: ResourceReplacement | null): ResourceReplacement {
  const old = takenOf(before);
  const taken: TakenClip = { hero, clip, timing: old?.timing ?? 'stretched', arms: old?.arms ?? 'source' };
  return { enabled: true, target, allow_shared: false, taken: cleanTaken(taken) };
}

/** A change of one option: `start_seconds: null` starts the part from the beginning. */
export interface TakenChange { timing?: TakenTiming; start_seconds?: number | null; arms?: TakenArms }
/** The row with one option changed (every other field of the row kept). Switching to 'stretched'
 * drops the start. */
export function withTaken(row: ResourceReplacement, change: TakenChange): ResourceReplacement {
  const old = takenOf(row);
  if (!old) return row;
  const taken: TakenClip = { ...old };
  if (change.timing) taken.timing = change.timing;
  if (change.arms) taken.arms = change.arms;
  if (change.start_seconds === null) delete taken.start_seconds;
  else if (change.start_seconds !== undefined) taken.start_seconds = change.start_seconds;
  return { ...row, taken: cleanTaken(taken) };
}

/** The row without `taken`. */
export function withoutTaken(row: ResourceReplacement): ResourceReplacement {
  const { taken: _taken, ...rest } = row;
  return rest;
}
/** The row with a compiled clip file chosen: no `taken` and no `made` any more. */
export function withCompiledFile(row: ResourceReplacement, source: string): ResourceReplacement {
  const { taken: _taken, made: _made, ...rest } = row;
  return { ...rest, source };
}

/** The latest start of a part at its own speed: the other clip's length less this clip's (0 when
 * the other clip is not longer). */
export const latestTakenStart = (sourceSeconds: number, clipSeconds: number) => Math.max(0, Math.min(MAX_TAKEN_START, sourceSeconds - clipSeconds));

/** One click for a set (the dash): the rows after taking every usable member of `set` from `hero`.
 * Each member is its own row (`takenRow`, keeping a replaced row's timing and arms); rows of other
 * clips are kept in place; a member's existing row is replaced where it is, new ones are added at
 * the end. Members either hero lacks, or that are layers, are left as they are. `limit` is the
 * saved format's row limit (128). */
export function takenSetRows(rows: ResourceReplacement[], set: ClipSet, hero: string, limit = 128): ResourceReplacement[] {
  const key = (t: string | undefined) => (t ?? '').toLowerCase();
  const out = [...rows];
  for (const member of set.members) {
    if (!member.usable || !member.target || !member.clip) continue;
    const at = out.findIndex(r => key(r.target) === key(member.target));
    if (at >= 0) out[at] = takenRow(member.target, hero, member.clip, out[at]);
    else if (out.length < limit) out.push(takenRow(member.target, hero, member.clip));
  }
  return out;
}

/** The same value whatever the order of the object keys (a row read from a file may hold them in
 * another order than `takenRow` writes them). */
function same(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (Array.isArray(a) || Array.isArray(b)) return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((v, i) => same(v, b[i]));
  if (!a || !b || typeof a !== 'object' || typeof b !== 'object') return false;
  const x = a as Record<string, unknown>, y = b as Record<string, unknown>;
  const kx = Object.keys(x).filter(k => x[k] !== undefined), ky = Object.keys(y).filter(k => y[k] !== undefined);
  return kx.length === ky.length && kx.every(k => same(x[k], y[k]));
}
/** Whether taking every usable member of `set` from `hero` (`takenSetRows`) would change nothing:
 * each such member already has its row, and that row is exactly what the click would write. */
export function takenSetUnchanged(rows: ResourceReplacement[], set: ClipSet, hero: string): boolean {
  const key = (t: string | undefined) => (t ?? '').toLowerCase();
  const members = set.members.filter(m => m.usable && m.target && m.clip);
  return members.length > 0 && members.every(m => {
    const row = rows.find(r => key(r.target) === key(m.target));
    return Boolean(row) && same(takenRow(m.target!, hero, m.clip!, row), row);
  });
}
