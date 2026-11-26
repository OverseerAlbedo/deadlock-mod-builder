/** Hero and item names as the game itself writes them in the interface language.
 *
 * The names come from the read-only `game_names` command (`api.gameNames`), which reads the
 * game's own names files. They are only ever SHOWN: everything saved, sent to the backend,
 * compared, sorted on, used as a key, logged or copied for support keeps the English name, the
 * item token or the path. With empty maps (English, a game that is not set up, missing names
 * files, or before the answer arrives) every function returns the English it was given.
 *
 * - `useGameNamesSource(ready, pak)`: called once, in App.tsx. It asks the command when the
 *   interface language is not English and the settings are loaded (`ready`), again when the
 *   language changes and when the saved game archive (`pak`) changes. English clears the maps
 *   without asking. A failed call leaves the maps empty; the newest request wins.
 * - `useGameNames()`: in a component that draws names; re-renders it when names arrive or the
 *   language changes.
 * - `shownHero(english)`, `shownHeroLabel(label)`, `shownItem(token, english)`: the name to draw. */
import { useEffect, useSyncExternalStore } from 'react';
import { api } from '../api';
import { getLanguage, useLanguage } from './index';

export type GameNames = {
  /** The language id the answer is for. */
  language: string;
  /** The app's English hero name (the roster's main name) → the game's name in that language. */
  heroes: Record<string, string>;
  /** Lower-case item token → the game's item name in that language. */
  items: Record<string, string>;
  /** Names the game's files did not have (reports and tests only). */
  missing: string[];
};

const EMPTY: GameNames = { language: 'en', heroes: {}, items: {}, missing: [] };
let names: GameNames = EMPTY;
let ticket = 0;
const listeners = new Set<() => void>();

function publish(next: GameNames) {
  names = next;
  for (const listener of [...listeners]) listener();
}
function subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; }
const snapshot = () => names;

/** The names held now, when they are for the language shown (else the empty set). */
export function currentGameNames(): GameNames {
  return names.language === getLanguage() ? names : EMPTY;
}
const own = (map: Record<string, string>, key: string) => Object.prototype.hasOwnProperty.call(map, key) && map[key] ? map[key] : null;

/** The hero's name as the game shows it in the interface language, else the English name. */
export function shownHero(english: string): string {
  if (!english) return english;
  return own(currentGameNames().heroes, english) ?? english;
}
/** A hero label as the hero picker builds it ("Shiv", "Shiv · Shiv Ult", "Lady Geist · Not used
 * in game"): the main name shown in the interface language, the rest kept as it is. */
export function shownHeroLabel(label: string): string {
  const at = label.indexOf(' · ');
  if (at < 0) return shownHero(label);
  return shownHero(label.slice(0, at)) + label.slice(at);
}
/** The item's name as the game shows it in the interface language, else `english`. */
export function shownItem(token: string | null | undefined, english: string): string {
  if (!token) return english;
  return own(currentGameNames().items, token.toLowerCase()) ?? english;
}

/** Re-renders the component when names arrive or the language changes; returns the names in use. */
export function useGameNames(): GameNames {
  useLanguage();
  useSyncExternalStore(subscribe, snapshot, snapshot);
  return currentGameNames();
}

/** Asks the command for `language` now (the newest request wins; English clears without asking). */
export async function loadGameNames(language: string): Promise<void> {
  const mine = ++ticket;
  if (language === 'en') { publish(EMPTY); return; }
  if (names.language !== language) publish({ ...EMPTY, language });
  let answer: GameNames | null | undefined = null;
  try {
    answer = await Promise.resolve().then(() => api.gameNames(language));
  } catch {
    answer = null;
  }
  if (mine !== ticket) return;
  publish({
    language,
    heroes: answer && typeof answer.heroes === 'object' && answer.heroes ? answer.heroes : {},
    items: answer && typeof answer.items === 'object' && answer.items ? answer.items : {},
    missing: answer && Array.isArray(answer.missing) ? answer.missing : [],
  });
}

/** App.tsx: keeps the names in step with the interface language and the saved game archive.
 * `ready`: the settings have been read. */
export function useGameNamesSource(ready: boolean, pak: string): void {
  const language = useLanguage();
  useEffect(() => {
    if (language === 'en') { ticket++; if (names !== EMPTY) publish(EMPTY); return; }
    if (!ready) return;
    void loadGameNames(language);
  }, [ready, language, pak]);
}

/** Tests: forget the names held. */
export function forgetGameNames() {
  ticket++;
  publish(EMPTY);
}
