// i18n-area: build
// Wording of Build & install → Installed mods, from the scan's counts (backend
// mod_update.rs and vpk_contents.rs). Nothing here decides a hero or a kind:
// those come from the backend's path rules. The words come in the interface
// language and are only drawn (the search also reads the hero column as shown).
import type { InstalledMod, VpkContents, VpkKind } from '../models';
import { t, tc, tp } from '../i18n';
import { shownHero } from '../i18n/gameNames';
import { nameList } from './words';

/** A kind with its count ("3 textures"). */
const NAMES: Record<VpkKind, (n: number) => string> = {
  character_model: n => tp(n, { one: '{count} character model', other: '{count} character models' }),
  gun: n => tp(n, { one: '{count} gun', other: '{count} guns' }),
  ability_object: n => tp(n, { one: '{count} ability object', other: '{count} ability objects' }),
  other_model: n => tp(n, { one: '{count} other model', other: '{count} other models' }),
  model_data: n => tp(n, { one: '{count} model data file', other: '{count} model data files' }),
  material: n => tp(n, { one: '{count} material', other: '{count} materials' }),
  texture: n => tp(n, { one: '{count} texture', other: '{count} textures' }),
  sound: n => tp(n, { one: '{count} sound', other: '{count} sounds' }),
  sound_event: n => tp(n, { one: '{count} sound event file', other: '{count} sound event files' }),
  icon: n => tp(n, { one: '{count} icon', other: '{count} icons' }),
  item_icon: n => tp(n, { one: '{count} item icon', other: '{count} item icons' }),
  item_names: n => tp(n, { one: '{count} item names file', other: '{count} item names files' }),
  effect: n => tp(n, { one: '{count} effect', other: '{count} effects' }),
  animation: n => tp(n, { one: '{count} animation', other: '{count} animations' }),
  game_data: n => tp(n, { one: '{count} game data file', other: '{count} game data files' }),
  map: n => tp(n, { one: '{count} map file', other: '{count} map files' }),
  menu: n => tp(n, { one: '{count} menu file', other: '{count} menu files' }),
  text: n => tp(n, { one: '{count} text file', other: '{count} text files' }),
  other: n => tp(n, { one: '{count} other file', other: '{count} other files' }),
};
/** One character model or gun, named without its count. */
const ONE: Partial<Record<VpkKind, () => string>> = { character_model: () => t('character model'), gun: () => t('gun') };
/** The kinds a file of plain files is named by. */
const PLAIN_WORDS: Record<string, () => string> = {
  map: () => tc('file kind', 'map'), menu: () => tc('file kind', 'menu'), game_data: () => tc('file kind', 'game data'), text: () => tc('file kind', 'text'),
};
/** Display order of kinds, most telling first. */
export const KIND_ORDER: VpkKind[] = ['character_model', 'gun', 'ability_object', 'other_model', 'material', 'texture', 'sound', 'sound_event', 'icon', 'item_icon', 'item_names', 'effect', 'animation', 'model_data', 'game_data', 'map', 'menu', 'text', 'other'];
/** Kinds that hold no hero content of their own when nothing else does. */
const PLAIN: VpkKind[] = ['map', 'menu', 'game_data', 'text', 'other'];

export const kindName = (kind: VpkKind, count: number) => NAMES[kind](count);
const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);
const files = (n: number) => tp(n, { one: '{count} file', other: '{count} files' });
const empty: VpkContents = { files: 0, kinds: {}, heroes: [], every_hero: 0, unattributed: 0, ability_objects: [], samples: {} };
export const contentsOf = (mod: InstalledMod): VpkContents => mod.contents ?? empty;
const count = (c: VpkContents, kind: VpkKind) => c.kinds[kind] ?? 0;
/** `models/abilities/sleep_dagger.vmdl_c` → "sleep dagger". */
export const readableStem = (path: string) => (path.split('/').pop() ?? path).replace(/\.vmdl(_c)?$/i, '').replace(/[_-]+/g, ' ').trim();

/** The hero column: every hero named by the scan, "Every hero" for item pictures and files
 * every hero uses, "No hero" otherwise. Several heroes are listed, the first three by name. */
export function heroText(mod: InstalledMod): string {
  if (mod.unreadable) return t('Could not be read');
  const c = contentsOf(mod), names = heroNames(mod);
  if (!names.length) return c.every_hero > 0 ? t('Every hero') : t('No hero');
  // Many heroes with a file or two each in a large file (menus with hero pictures): not a hero mod.
  const heroFiles = c.heroes.reduce((n, h) => Math.max(n, h.files), 0);
  if (names.length > 3 && c.files > 20 && heroFiles * 10 < c.files) return tp(names.length, { one: 'A few files each for {count} heroes', other: 'A few files each for {count} heroes' });
  const shown = names.map(shownHero);
  return shown.length <= 3 ? nameList(shown) : tp(shown.length - 3, { one: '{names} + {count} more', other: '{names} + {count} more' }, { names: nameList(shown.slice(0, 3)) });
}
/** The scan's heroes; a reply without contents (an older backend) names the heroes of its models. */
export const heroNames = (mod: InstalledMod): string[] => mod.contents
  ? mod.contents.heroes.map(h => h.name)
  : Array.from(new Set(mod.models.map(model => model.hero ?? '').filter(Boolean)));

/** The shop's item names (`citadel_gc_mod_names_<language>.txt`): "item names (english)" for one
 * language, "item names · 17 languages" for several. */
export function itemNamesText(c: VpkContents): string {
  const n = count(c, 'item_names');
  if (n !== 1) return tp(n, { one: 'item names · {count} languages', other: 'item names · {count} languages' });
  const file = (c.samples.item_names ?? [])[0]?.split('/').pop() ?? '';
  const language = /^citadel_gc_mod_names_(.+)\.txt$/i.exec(file)?.[1];
  return language ? t('item names ({language})', { language }) : t('item names');
}

/** The short "what it changes" line. */
export function summary(mod: InstalledMod): string {
  if (mod.unreadable) return t('Not readable as a VPK');
  const c = contentsOf(mod);
  if (!c.files) return t('No files');
  const kinds = KIND_ORDER.filter(k => count(c, k) > 0);
  // Files without a hero and nothing but map, menu, data or text: say what they mostly hold.
  if (!c.heroes.length && !c.every_hero && kinds.every(k => PLAIN.includes(k))) {
    const named = kinds.filter(k => k !== 'other').sort((a, b) => count(c, b) - count(c, a)).slice(0, 2).map(k => PLAIN_WORDS[k]());
    if (named.length === 2) return capital(tp(c.files, { one: '{first} and {second} files ({count} file)', other: '{first} and {second} files ({count} files)' }, { first: named[0], second: named[1] }));
    return named.length ? capital(tp(c.files, { one: '{kind} files ({count} file)', other: '{kind} files ({count} files)' }, { kind: named[0] })) : t('Other files ({count})', { count: c.files });
  }
  // One ability object and little else: name it and its heroes.
  if (c.ability_objects.length === 1 && !count(c, 'character_model')) {
    const object = c.ability_objects[0], users = object.every_hero ? t('every hero') : object.heroes.length > 1 ? tp(object.heroes.length, { one: 'shared by {count} heroes', other: 'shared by {count} heroes' }) : object.heroes[0] && shownHero(object.heroes[0]);
    const rest = kinds.filter(k => !['ability_object', 'model_data', 'material', 'texture'].includes(k));
    if (!rest.length) return users ? t('Ability object: {name} ({users})', { name: readableStem(object.path), users }) : t('Ability object: {name}', { name: readableStem(object.path) });
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
    const one = ONE[kind];
    parts.push(one && n === 1 ? one() : kindName(kind, n));
  }
  const shown = parts.slice(0, 5);
  return capital(shown.join(' · ') + (parts.length > 5 ? ' · …' : ''));
}

/** Every kind with its count, for the details. */
export function contentsText(mod: InstalledMod): string {
  const c = contentsOf(mod);
  const parts = KIND_ORDER.filter(k => count(c, k) > 0).map(k => kindName(k, count(c, k)));
  return `${nameList(parts)} · ${files(c.files)}`;
}

/** Where the file comes from: this app's project when certain, else "Not made by this app". */
export function fromText(mod: InstalledMod): string {
  const made = mod.made;
  if (!made) return t('Not made by this app');
  if (made.several) return t('Made here · the same files as builds of several projects');
  const extras = Boolean(mod.contents && !count(mod.contents, 'character_model'));
  const name = made.project_name || t('a project of this app');
  if (made.project_file) return extras ? t('Made here · {name} (extras only)', { name }) : t('Made here · {name}', { name });
  return extras ? t('Made here · {name} (extras only) · project not on this PC', { name }) : t('Made here · {name} · project not on this PC', { name });
}

/** The note when other files change some of the same game files (never which one the game uses). */
export function sharedText(mod: InstalledMod): string | null {
  const shared = mod.shared ?? [];
  if (!shared.length) return null;
  if (shared.length === 1) return tp(shared[0].count, { one: 'Changes {count} of the same game file as {file}', other: 'Changes {count} of the same game files as {file}' }, { file: shared[0].file });
  return tp(shared.length, { one: 'Changes some of the same game files as {count} other files', other: 'Changes some of the same game files as {count} other files' });
}

export type StateTone = 'ready' | 'attention' | 'error' | 'neutral';
export function stateOf(mod: InstalledMod): { label: string; tone: StateTone } {
  if (mod.unreadable) return { label: t('Could not be read'), tone: 'error' };
  switch (mod.status) {
    case 'up_to_date': return { label: t('Up to date'), tone: 'ready' };
    case 'needs_update': return { label: t('Needs an update for the current game'), tone: 'attention' };
    case 'cannot_update': return { label: t('Rebuild needed'), tone: 'attention' };
    case 'unknown': return { label: t('Left alone'), tone: 'neutral' };
    default: return { label: t('Not checked: no character model'), tone: 'neutral' };
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
  // The English roster names and the names as shown (the game's own in the interface language).
  return [mod.file, mod.label ?? '', mod.made?.project_name ?? '', ...heroNames(mod), ...heroNames(mod).map(shownHero), heroText(mod)].some(text => text.toLowerCase().includes(q));
}
