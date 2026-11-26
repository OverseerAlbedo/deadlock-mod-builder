/** Which language the app starts in, and when it offers Chinese to an existing user.
 *
 * `Settings.language` is the truth; `MIRROR_KEY` in this PC's storage is a copy
 * that lets the first paint be in the right language before the settings
 * arrive. Before the first render (`startLanguage`, from main.tsx):
 *
 * | mirror      | first start (guide never opened)  | applied before the settings arrive |
 * |-------------|-----------------------------------|------------------------------------|
 * | a language  | any                               | the mirror                         |
 * | none        | yes, Windows in Chinese           | 简体中文 (tentative)               |
 * | none        | otherwise                         | English                            |
 *
 * Once the settings arrive (`decideLanguage`, from App.tsx), the settings win:
 *
 * | Settings.language | first start (no guide flag, no tool path) | result                                  |
 * |-------------------|-------------------------------------------|-----------------------------------------|
 * | a language        | any                                       | that language                           |
 * | empty             | yes                                       | the Windows language, saved when it is not English |
 * | empty             | no (an existing user)                     | English; the one-time offer when Windows is in Chinese |
 */
import { getLanguage, readMirror, setLanguage, windowsLanguage, isLanguage, type Language } from './index';

/** The guide flags App.tsx keeps on this PC (the same keys). */
const GUIDE_SEEN = 'dmb.setupGuideSeen', GUIDE_DONE = 'dmb.setupGuideDone';
/** Set once the offer to switch to Chinese was shown; kept for good. */
export const OFFER_SEEN = 'dmb.languageOfferSeen';

export interface LaunchFacts { guideSeen: boolean; guideDone: boolean }
let launch: LaunchFacts | undefined;
function read(key: string) { try { return localStorage.getItem(key) === '1'; } catch { return false; } }
/** The guide flags as they were when the app started (App.tsx marks the guide seen soon after). */
export function launchFacts(): LaunchFacts {
  if (!launch) launch = { guideSeen: read(GUIDE_SEEN), guideDone: read(GUIDE_DONE) };
  return launch;
}
/** Forgets the captured flags (tests). */
export function forgetLaunch() { launch = undefined; }

/** Paths only a user who set the app up has filled in. */
const TOOL_FIELDS = ['deadlock', 'pak', 'csdk', 'resourcecompiler', 'source2viewer', 'blender', 'steam', 'cswin'] as const;
type ToolSettings = Partial<Record<typeof TOOL_FIELDS[number], string>>;
export function toolsSet(settings: ToolSettings): boolean {
  return TOOL_FIELDS.some(key => Boolean(settings[key]?.trim()));
}

export interface LanguageDecision {
  language: Language;
  /** Save it to the settings (a first start that follows a non-English Windows). */
  persist: boolean;
  /** Offer Chinese once (an existing user on Chinese Windows). */
  offer: boolean;
}
/** What the settings, the guide flags and Windows say (see the second table above). */
export function decideLanguage(saved: string | undefined, firstStart: boolean, windows: Language): LanguageDecision {
  if (saved && isLanguage(saved)) return { language: saved, persist: false, offer: false };
  if (saved) return { language: 'en', persist: false, offer: false };
  if (firstStart) return { language: windows, persist: windows !== 'en', offer: false };
  return { language: 'en', persist: false, offer: windows !== 'en' };
}
/** Whether this start is a first start, from the flags at launch and the loaded settings. */
export function isFirstStart(facts: LaunchFacts, settings: ToolSettings): boolean {
  return !facts.guideSeen && !facts.guideDone && !toolsSet(settings);
}

export function offerSeen(): boolean { return read(OFFER_SEEN); }
export function markOfferSeen() { try { localStorage.setItem(OFFER_SEEN, '1'); } catch { /* no storage */ } }

/** Before the first render: the mirrored language, or Chinese on a first start under Chinese
 * Windows (see the first table above), with its catalog loaded. English loads nothing. */
export async function startLanguage(): Promise<void> {
  const facts = launchFacts();
  const mirror = readMirror();
  const first = mirror ?? (!facts.guideSeen && !facts.guideDone ? windowsLanguage() : 'en');
  if (first !== getLanguage()) await setLanguage(first).catch(() => undefined);
}
