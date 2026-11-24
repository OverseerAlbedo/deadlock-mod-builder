// Wording of Build & install → Installed mods, from the scan's counts (backend
// mod_update.rs and vpk_contents.rs). Nothing here decides a hero or a kind:
// those come from the backend's path rules.
import type { InstalledMod, VpkContents, VpkKind } from '../models';

const NAMES: Record<VpkKind, [string, string]> = {
  character_model: ['character model', 'character models'],
  gun: ['gun', 'guns'],
  ability_object: ['ability object', 'ability objects'],
  other_model: ['other model', 'other models'],
  model_data: ['model data file', 'model data files'],
  material: ['material', 'materials'],
  texture: ['texture', 'textures'],
  sound: ['sound', 'sounds'],
  sound_event: ['sound event file', 'sound event files'],
  icon: ['icon', 'icons'],
  item_icon: ['item icon', 'item icons'],
  item_names: ['item names file', 'item names files'],
  effect: ['effect', 'effects'],
  animation: ['animation', 'animations'],
  game_data: ['game data file', 'game data files'],
  map: ['map file', 'map files'],
  menu: ['menu file', 'menu files'],
  text: ['text file', 'text files'],
  other: ['other file', 'other files'],
};
/** Display order of kinds, most telling first. */
export const KIND_ORDER: VpkKind[] = ['character_model', 'gun', 'ability_object', 'other_model', 'material', 'texture', 'sound', 'sound_event', 'icon', 'item_icon', 'item_names', 'effect', 'animation', 'model_data', 'game_data', 'map', 'menu', 'text', 'other'];
/** Kinds that hold no hero content of their own when nothing else does. */
const PLAIN: VpkKind[] = ['map', 'menu', 'game_data', 'text', 'other'];

export const kindName = (kind: VpkKind, count: number) => `${count} ${NAMES[kind][count === 1 ? 0 : 1]}`;
const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);
const empty: VpkContents = { files: 0, kinds: {}, heroes: [], every_hero: 0, unattributed: 0, ability_objects: [], samples: {} };
export const contentsOf = (mod: InstalledMod): VpkContents => mod.contents ?? empty;
const count = (c: VpkContents, kind: VpkKind) => c.kinds[kind] ?? 0;
/** `models/abilities/sleep_dagger.vmdl_c` → "sleep dagger". */
export const readableStem = (path: string) => (path.split('/').pop() ?? path).replace(/\.vmdl(_c)?$/i, '').replace(/[_-]+/g, ' ').trim();

/** The hero column: every hero named by the scan, "Every hero" for item pictures and files
 * every hero uses, "No hero" otherwise. Several heroes are listed, the first three by name. */
export function heroText(mod: InstalledMod): string {
  if (mod.unreadable) return 'Could not be read';
  const c = contentsOf(mod), names = heroNames(mod);
  if (!names.length) return c.every_hero > 0 ? 'Every hero' : 'No hero';
  // Many heroes with a file or two each in a large file (menus with hero pictures): not a hero mod.
  const heroFiles = c.heroes.reduce((n, h) => Math.max(n, h.files), 0);
  if (names.length > 3 && c.files > 20 && heroFiles * 10 < c.files) return `A few files each for ${names.length} heroes`;
  return names.length <= 3 ? names.join(', ') : `${names.slice(0, 3).join(', ')} + ${names.length - 3} more`;
}
/** The scan's heroes; a reply without contents (an older backend) names the heroes of its models. */
export const heroNames = (mod: InstalledMod): string[] => mod.contents
  ? mod.contents.heroes.map(h => h.name)
  : Array.from(new Set(mod.models.map(model => model.hero ?? '').filter(Boolean)));

/** The shop's item names (`citadel_gc_mod_names_<language>.txt`): "item names (english)" for one
 * language, "item names · 17 languages" for several. */
export function itemNamesText(c: VpkContents): string {
  const n = count(c, 'item_names');
  if (n !== 1) return `item names · ${n} languages`;
  const file = (c.samples.item_names ?? [])[0]?.split('/').pop() ?? '';
  const language = /^citadel_gc_mod_names_(.+)\.txt$/i.exec(file)?.[1];
  return language ? `item names (${language})` : 'item names';
}

/** The short "what it changes" line. */
export function summary(mod: InstalledMod): string {
  if (mod.unreadable) return 'Not readable as a VPK';
  const c = contentsOf(mod);
  if (!c.files) return 'No files';
  const kinds = KIND_ORDER.filter(k => count(c, k) > 0);
  // Files without a hero and nothing but map, menu, data or text: say what they mostly hold.
  if (!c.heroes.length && !c.every_hero && kinds.every(k => PLAIN.includes(k))) {
    const named = kinds.filter(k => k !== 'other').sort((a, b) => count(c, b) - count(c, a)).slice(0, 2)
      .map(k => ({ map: 'map', menu: 'menu', game_data: 'game data', text: 'text' } as Record<string, string>)[k]);
    return named.length ? `${capital(named.join(' and '))} files (${c.files} ${c.files === 1 ? 'file' : 'files'})` : `Other files (${c.files})`;
  }
  // One ability object and little else: name it and its heroes.
  if (c.ability_objects.length === 1 && !count(c, 'character_model')) {
    const object = c.ability_objects[0], users = object.every_hero ? 'every hero' : object.heroes.length > 1 ? `shared by ${object.heroes.length} heroes` : object.heroes[0];
    const rest = kinds.filter(k => !['ability_object', 'model_data', 'material', 'texture'].includes(k));
    if (!rest.length) return `Ability object: ${readableStem(object.path)}${users ? ` (${users})` : ''}`;
  }
  const model = count(c, 'character_model') + count(c, 'gun') + count(c, 'other_model') + count(c, 'ability_object') > 0;
  const parts: string[] = [];
  for (const kind of kinds) {
    const n = count(c, kind);
    if (kind === 'model_data' && model) continue;
    if (kind === 'texture' && count(c, 'material')) continue;
    if (kind === 'sound_event' && count(c, 'sound')) continue;
    if (kind === 'text' && kinds.length > 1) continue;
    if (kind === 'item_names') { parts.push(itemNamesText(c)); continue; }
    parts.push(kind === 'character_model' || kind === 'gun' ? (n === 1 ? NAMES[kind][0] : kindName(kind, n)) : kindName(kind, n));
  }
  const shown = parts.slice(0, 5);
  return capital(shown.join(' · ') + (parts.length > 5 ? ' · …' : ''));
}

/** Every kind with its count, for the details. */
export function contentsText(mod: InstalledMod): string {
  const c = contentsOf(mod);
  const parts = KIND_ORDER.filter(k => count(c, k) > 0).map(k => kindName(k, count(c, k)));
  return `${parts.join(', ')} · ${c.files} ${c.files === 1 ? 'file' : 'files'}`;
}

/** Where the file comes from: this app's project when certain, else "Not made by this app". */
export function fromText(mod: InstalledMod): string {
  const made = mod.made;
  if (!made) return 'Not made by this app';
  if (made.several) return 'Made here · the same files as builds of several projects';
  const extras = mod.contents && !count(mod.contents, 'character_model') ? ' (extras only)' : '';
  const name = made.project_name || 'a project of this app';
  return `Made here · ${name}${extras}${made.project_file ? '' : ' · project not on this PC'}`;
}

/** The note when other files change some of the same game files (never which one the game uses). */
export function sharedText(mod: InstalledMod): string | null {
  const shared = mod.shared ?? [];
  if (!shared.length) return null;
  if (shared.length === 1) return `Changes ${shared[0].count} of the same game ${shared[0].count === 1 ? 'file' : 'files'} as ${shared[0].file}`;
  return `Changes some of the same game files as ${shared.length} other files`;
}

export type StateTone = 'ready' | 'attention' | 'error' | 'neutral';
export function stateOf(mod: InstalledMod): { label: string; tone: StateTone } {
  if (mod.unreadable) return { label: 'Could not be read', tone: 'error' };
  switch (mod.status) {
    case 'up_to_date': return { label: 'Up to date', tone: 'ready' };
    case 'needs_update': return { label: 'Needs an update for the current game', tone: 'attention' };
    case 'cannot_update': return { label: 'Rebuild needed', tone: 'attention' };
    case 'unknown': return { label: 'Left alone', tone: 'neutral' };
    default: return { label: 'Not checked: no character model', tone: 'neutral' };
  }
}

export const megabytes = (bytes: number) => `${(bytes / 1048576).toFixed(bytes < 10 * 1048576 ? 1 : 0)} MB`;

/** Hero filter chips: each hero, Items (item pictures and item names), Other (no hero at all). */
export type HeroFilter = { kind: 'all' } | { kind: 'hero'; name: string } | { kind: 'items' } | { kind: 'other' };
export const hasItems = (mod: InstalledMod) => count(contentsOf(mod), 'item_icon') + count(contentsOf(mod), 'item_names') > 0;
export const isOther = (mod: InstalledMod) => !heroNames(mod).length && !contentsOf(mod).every_hero;
export function matchesHero(mod: InstalledMod, filter: HeroFilter): boolean {
  switch (filter.kind) {
    case 'all': return true;
    case 'hero': return heroNames(mod).includes(filter.name);
    case 'items': return hasItems(mod);
    case 'other': return isOther(mod);
  }
}
export type MadeFilter = 'all' | 'here' | 'elsewhere';
export const matchesMade = (mod: InstalledMod, filter: MadeFilter) => filter === 'all' || (filter === 'here') === Boolean(mod.made);
export function matchesSearch(mod: InstalledMod, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [mod.file, mod.label ?? '', mod.made?.project_name ?? '', ...heroNames(mod), heroText(mod)].some(text => text.toLowerCase().includes(q));
}
