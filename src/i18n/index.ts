/** The interface language.
 *
 * The English text in the code is the key: `t('Save settings')`. A language's
 * catalog (`locales/<language>/<area>.json`, every area file of the language
 * loaded together the first time it is needed) maps that text to the
 * translation; anything it lacks, or leaves empty, shows the English text.
 * English has no catalog.
 *
 * - `t(text, values?)`: plain text; `{name}` is filled from `values`.
 * - `tc(context, text, values?)`: the same English needing another translation
 *   in another place (a verb and a noun, a theme's name). Catalog key
 *   `context|text`; without an entry it shows `text`.
 * - `tp(count, {one, other, …}, values?)`: text with a count. The English
 *   forms follow English plural rules; the catalog entry (key `#|` + the
 *   English `other` form) holds the forms of its language (`zero`, `one`,
 *   `two`, `few`, `many`, `other`, exact `=N`), chosen by `Intl.PluralRules`.
 *   `{count}` is the number. `tpc(context, count, forms, values?)` is the
 *   same with a context (key `#context|other`).
 * - `m(text)` / `mc(context, text)`: mark English text in a constant table so
 *   the checker finds it; they return the text unchanged. The table keeps
 *   English and is translated where it is shown, with `t(value)` /
 *   `tc(context, value)`.
 * - `<T text="… <code>{x}</code> …" tags={{code: …}} values={{x}}/>` (T.tsx):
 *   a sentence with markup, translated as one piece.
 * - `useT()` / `useLanguage()`: call one of them in a component that shows
 *   text; it re-renders the component when the language changes.
 *
 * Translated text is only ever shown: never stored, compared, used as a key,
 * sent to the backend or written to a file or log.
 * `scripts/i18n-check.mjs` reads every call and compares it with the catalogs. */
import { useSyncExternalStore } from 'react';

export type PluralForm = Intl.LDMLPluralRule | `=${number}`;
export type PluralForms = Partial<Record<PluralForm, string>> & { other: string };
export type Entry = string | PluralForms;
export type Catalog = Record<string, Entry>;
export type Values = Record<string, string | number>;

/** The languages offered, each named in its own language; `html` is the page's `lang`. */
export const LANGUAGES = [
  { id: 'en', name: 'English', html: 'en' },
  { id: 'zh-CN', name: '简体中文', html: 'zh-Hans' },
] as const;
export type Language = typeof LANGUAGES[number]['id'];
/** The page's copy of the language, read before the first render (the settings file is the truth). */
export const MIRROR_KEY = 'dmb.language';

/** Whether `value` names one of `LANGUAGES`. */
export function isLanguage(value: unknown): value is Language {
  return LANGUAGES.some(language => language.id === value);
}
/** The catalog key of a text with a context. */
export const contextKey = (context: string, text: string) => `${context}|${text}`;
/** The catalog key of a text with a count (by its English `other` form). */
export const pluralKey = (forms: PluralForms, context = '') => `#${context}|${forms.other}`;

let language: Language = 'en';
let catalog: Catalog = {};
let switching = 0;
const listeners = new Set<() => void>();
const loaders = import.meta.glob<Catalog>('./locales/*/*.json', { import: 'default' });

/** Ideographs and full-width punctuation (U+3000–U+303F, U+3400–U+9FFF, U+F900–U+FAFF, U+FF00–U+FFEF). */
const CJK = /[　-〿㐀-鿿豈-﫿＀-￯]/;
const isCjk = (char: string | undefined) => !!char && CJK.test(char);
/** Whether the single space before (`[0]`) and after (`[1]`) the placeholder at
 * `text[start, end)` is dropped when `value` goes in: only a space between a
 * Chinese template character and a Chinese edge of the value. Shared by `fill` and `<T>`. */
export function closedGaps(text: string, start: number, end: number, value: string): [boolean, boolean] {
  const before = text[start - 1] === ' ' && text[start - 2] !== ' ' && isCjk(text[start - 2]) && isCjk(value[0]);
  const after = text[end] === ' ' && text[end + 1] !== ' ' && isCjk(text[end + 1]) && isCjk(value[value.length - 1]);
  return [before, after];
}
/** Fills `{name}` placeholders; an unknown name stays as written. A single space between a
 * Chinese template character and a Chinese edge of the inserted value is dropped (`closedGaps`). */
export function fill(text: string, values?: Values): string {
  if (!values) return text;
  const pattern = /\{(\w+)\}/g;
  let out = '';
  let last = 0;
  let skip = false;
  for (const match of text.matchAll(pattern)) {
    const start = match.index;
    const end = start + match[0].length;
    const name = match[1];
    if (!(name in values)) {
      out += text.slice(last + (skip ? 1 : 0), end);
      last = end; skip = false;
      continue;
    }
    const value = String(values[name]);
    const [before, after] = closedGaps(text, start, end, value);
    let piece = text.slice(last + (skip ? 1 : 0), start);
    if (before && piece.endsWith(' ')) piece = piece.slice(0, -1);
    out += piece + value;
    last = end; skip = after;
  }
  return out + text.slice(last + (skip ? 1 : 0));
}
/** The catalog's text for `key`, or `english` when it has none. */
export function lookup(key: string, english: string): string {
  const own = catalog[key];
  return typeof own === 'string' && own ? own : english;
}
export function t(text: string, values?: Values): string { return fill(lookup(text, text), values); }
export function tc(context: string, text: string, values?: Values): string { return fill(lookup(contextKey(context, text), text), values); }

const pluralRules = new Map<string, Intl.PluralRules>();
function rule(tag: string, count: number) {
  let rules = pluralRules.get(tag);
  if (!rules) { rules = new Intl.PluralRules(tag); pluralRules.set(tag, rules); }
  return rules.select(count);
}
function pick(forms: PluralForms, tag: string, count: number): string {
  return forms[`=${count}`] || forms[rule(tag, count)] || forms.other;
}
function plural(key: string, count: number, english: PluralForms, values?: Values): string {
  const own = catalog[key];
  const text = own && typeof own === 'object' && own.other ? pick(own, language, count) : pick(english, 'en', count);
  return fill(text, { count, ...values });
}
export function tp(count: number, forms: PluralForms, values?: Values): string { return plural(pluralKey(forms), count, forms, values); }
export function tpc(context: string, count: number, forms: PluralForms, values?: Values): string { return plural(pluralKey(forms, context), count, forms, values); }
/** Marks English text in a constant table for the checker; translate it where it is shown. */
export const m = <S extends string>(text: S): S => text;
/** Marks English text with a context in a constant table; show it with `tc(context, value)`. */
export const mc = <S extends string>(_context: string, text: S): S => text;

export function getLanguage(): Language { return language; }
function subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; }
/** The current language; re-renders the component when it changes. */
export function useLanguage(): Language { return useSyncExternalStore(subscribe, getLanguage, getLanguage); }
/** `t`, with a re-render when the language changes. */
export function useT(): typeof t { useLanguage(); return t; }

function writeMirror(next: Language) { try { localStorage.setItem(MIRROR_KEY, next); } catch { /* no storage */ } }
/** The language mirrored on this PC (undefined when none or unknown). */
export function readMirror(): Language | undefined {
  try { const value = localStorage.getItem(MIRROR_KEY); return isLanguage(value) ? value : undefined; } catch { return undefined; }
}
/** Uses a catalog directly (tests; a language not in `LANGUAGES` is allowed here). */
export function applyCatalog(next: string, entries: Catalog) {
  switching++;
  language = next as Language; catalog = entries;
  if (typeof document !== 'undefined') document.documentElement.lang = LANGUAGES.find(item => item.id === next)?.html ?? next;
  for (const listener of [...listeners]) listener();
}
/** Every area file of a language, merged. */
export async function loadCatalog(next: Language): Promise<Catalog> {
  if (next === 'en') return {};
  const files = Object.keys(loaders).filter(path => path.startsWith(`./locales/${next}/`)).sort();
  const parts = await Promise.all(files.map(path => loaders[path]()));
  return Object.assign({}, ...parts) as Catalog;
}
/** Switches the language at once (the catalog loads the first time; nothing reloads) and
 * mirrors it on this PC. An unknown language means English. The newest call wins. */
export async function setLanguage(next: string): Promise<void> {
  const chosen: Language = isLanguage(next) ? next : 'en';
  const ticket = ++switching;
  const entries = await loadCatalog(chosen);
  if (ticket !== switching) return;
  applyCatalog(chosen, entries);
  writeMirror(chosen);
}
/** The language Windows shows, as the page sees it: any Chinese locale is 简体中文, everything else English. */
export function windowsLanguage(locale: string | undefined = typeof navigator === 'undefined' ? undefined : navigator.language): Language {
  return locale && /^zh\b/i.test(locale) ? 'zh-CN' : 'en';
}
