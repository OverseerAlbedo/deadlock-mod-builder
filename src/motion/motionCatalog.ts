/** The Motion page's list, made from the hero's clip catalog and the saved replacement rows. Names,
 * groups, graph naming and derived copies come from Rust (`motion::ClipDetails`); here they are only
 * filtered, counted and folded. Nothing here writes a row. */
// i18n-area: motion
import type { AnimationEntry, ClipGroup, ComponentCatalog } from '../components/AssetCatalog';
import { mc, t } from '../i18n';
import type { Project, ResourceReplacement } from '../models';
import type { FitReport } from './motionTypes';

/** The list groups in Rust's order, with the page's titles (English; shown with `tc('clip group', title)`). */
export const GROUPS: readonly {id: ClipGroup; title: string}[] = [
  {id: 'movement', title: mc('clip group', 'Movement')},
  {id: 'jumping', title: mc('clip group', 'Jumping and falling')},
  {id: 'weapon', title: mc('clip group', 'Weapon')},
  {id: 'abilities', title: mc('clip group', 'Abilities')},
  {id: 'hits', title: mc('clip group', 'Hits and deaths')},
  {id: 'other', title: mc('clip group', 'Emotes and other')},
];
export const groupTitle = (id: ClipGroup) => GROUPS.find(g => g.id === id)?.title ?? id;

const fold = (value: string) => value.replace(/\\/g, '/').toLowerCase();

/** A clip of the list with its saved row (by index into `customization.animations`). */
export interface ClipItem {
  entry: AnimationEntry;
  /** The saved row for this clip, and its index. */
  row: ResourceReplacement | null;
  index: number;
  /** A copy the compiler derived from this clip (it stays the game's). */
  derived: AnimationEntry | null;
  /** No animation graph of the hero names it (shown in the folded section unless it has a row). */
  unnamed: boolean;
}

export interface MotionList {
  /** Clips in the list: graph-named, and any clip with a saved row. */
  items: ClipItem[];
  /** Clips no graph names and with no row: the folded section. */
  folded: ClipItem[];
  /** Indices of saved rows the list cannot show (empty source or target, a target outside the
   * catalog, a second row for the same clip): edited in "Other saved rows". */
  others: number[];
}

/** The list for a catalog (null: not loaded, every saved row is "other"). */
export function motionList(catalog: ComponentCatalog | null, rows: ResourceReplacement[]): MotionList {
  if (!catalog) return {items: [], folded: [], others: rows.map((_, i) => i)};
  const byTarget = new Map(catalog.animations.map(entry => [fold(entry.target), entry]));
  const saved = new Map<string, number>();
  const others: number[] = [];
  rows.forEach((row, index) => {
    const key = fold(row.target ?? '');
    // A clip taken from another hero has no file of its own: its `taken` stands in for one.
    if (!(row.source || row.taken) || !row.target || !byTarget.has(key) || saved.has(key)) others.push(index);
    else saved.set(key, index);
  });
  const derived = new Map<string, AnimationEntry>();
  for (const entry of catalog.animations) if (entry.derived_from) derived.set(fold(entry.derived_from), entry);
  const items: ClipItem[] = [], folded: ClipItem[] = [];
  for (const entry of catalog.animations) {
    const key = fold(entry.target);
    const index = saved.get(key) ?? -1;
    const row = index >= 0 ? rows[index] : null;
    // A derived copy is not a row of its own (its parent's card names it), unless a row targets it.
    if (entry.derived_from && byTarget.has(fold(entry.derived_from)) && !row) continue;
    const item: ClipItem = {entry, row, index, derived: derived.get(key) ?? null, unnamed: !entry.graph_named};
    if (item.unnamed && !row) folded.push(item); else items.push(item);
  }
  return {items, folded, others};
}

/** The fit cache key of a row. */
export const fitKey = (target: string, source: string) => `${fold(target)}|${source}`;

export type ClipState = 'original' | 'replaced' | 'off' | 'does_not_fit';
/** A clip's badge: Replaced (an included row), Off (a row not included, or customization off),
 * Does not fit (a checked row the build refuses). */
export function clipState(project: Project, item: ClipItem, fits: Map<string, FitReport | 'checking' | Error>): ClipState {
  if (!item.row) return 'original';
  if (item.row.enabled === false || !project.customization.enabled) return 'off';
  const fit = fits.get(fitKey(item.entry.target, item.row.source ?? ''));
  if (fit && typeof fit === 'object' && !(fit instanceof Error) && !fit.fits) return 'does_not_fit';
  return 'replaced';
}
/** A clip's badge words (English; shown with `tc('clip state', word)`). */
export const STATE_WORDS: Record<ClipState, string> = {original: mc('clip state', 'Original'), replaced: mc('clip state', 'Replaced'), off: mc('clip state', 'Off'), does_not_fit: mc('clip state', 'Does not fit')};

/** Whether an item matches the search words (name, clip name or path, all words). */
export function matches(item: ClipItem, query: string): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const text = `${item.entry.name} ${item.entry.label} ${item.entry.target} ${item.row?.source ?? ''}`.toLowerCase();
  return words.every(word => text.includes(word));
}

/** Counts per group of the given items. */
export function groupCounts(items: ClipItem[]): Record<ClipGroup, number> {
  const out = Object.fromEntries(GROUPS.map(g => [g.id, 0])) as Record<ClipGroup, number>;
  for (const item of items) out[item.entry.group] = (out[item.entry.group] ?? 0) + 1;
  return out;
}

/** "0.80 s" for a clip length. */
export const seconds = (value: number) => t('{value} s', { value: value.toFixed(2) });

/** The clip's path with its middle folders shortened (`models/heroes_staging/haze/…/run_n.vnmclip_c`). */
export function shortPath(path: string): string {
  const parts = path.split('/');
  return parts.length > 4 ? [...parts.slice(0, 3), "…", parts[parts.length - 1]].join("/") : path;
}
