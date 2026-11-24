/** Words of Motion → Another hero's animation, made only from the engine's answers (the report of
 * `taken_clip_report`, the lists of `taken_clip_heroes` / `taken_clip_sources`). No clip or
 * retarget logic: numbers are only put into sentences. */
import type { AnimationEntry } from '../components/AssetCatalog';
import { liftLine, someBones, type MakesLine } from './madeRows';
import type { ClipSet, SourceClip, TakenReport } from './takenTypes';

const secs = (value: number) => `${value.toFixed(2)} s`;
const bones = (n: number) => `${n} ${n === 1 ? 'bone' : 'bones'}`;
/** "1.5 times faster" / "1.4 times slower" / null at its own speed, from the report's stretch. */
export function speedFactor(stretch: number): string | null {
  if (!(stretch > 0) || Math.abs(stretch - 1) < 0.005) return null;
  return stretch > 1 ? `${stretch.toFixed(1)} times faster` : `${(1 / stretch).toFixed(1)} times slower`;
}
const possessive = (name: string) => `${name}’s`;

/** The sentence under Timing: how the other hero's clip fills this hero's (from the report). */
export function timingWords(report: TakenReport, hero: string, stretched: boolean): string {
  const m = report.made, other = report.source_name;
  if (!stretched) return `${possessive(other)} clip plays at its own speed, from ${secs(m.plays_from)} to ${secs(m.plays_to)} of its ${secs(m.action_seconds)}, over ${possessive(hero)} ${secs(m.clip_seconds)}.`;
  const factor = speedFactor(m.stretch);
  const head = `${possessive(other)} whole ${secs(m.action_seconds)} clip plays in ${possessive(hero)} ${secs(m.clip_seconds)}`;
  return factor ? `${head}, ${factor}.` : `${head}, at its own speed.`;
}

/** The line under Arms and hands. */
export const armsWords = (other: string, hero: string) =>
  `${possessive(other)} arms and hands pose as if holding their own weapon; keep ${possessive(hero)} own for their grip.`;

/** "What you get", only from the report. A taken clip always keeps this hero's length, events and
 * travel, so there is no pass or fail here. */
export function takenLines(report: TakenReport, hero: string, stretched: boolean): MakesLine[] {
  const m = report.made, other = report.source_name;
  const out: MakesLine[] = [
    { id: 'body', mark: 'yours', text: `The body’s motion comes from ${other}: ${bones(report.driven.length)}${report.driven.length ? ` (${someBones(report.driven)})` : ''}`, side: `from ${other}` },
    { id: 'travel', mark: 'kept', text: `How far and how fast the move goes stays ${possessive(hero)}`, side: 'kept by the app' },
    { id: 'timing-kept', mark: 'kept', text: `Its timing, sounds and effects stay ${possessive(hero)} (${secs(m.clip_seconds)}, events at the same times)`, side: 'kept by the app' },
  ];
  const factor = speedFactor(m.stretch);
  if (!stretched) out.push({ id: 'speed', mark: 'note', text: `Plays at its own speed: ${secs(m.plays_from)} to ${secs(m.plays_to)} of ${possessive(other)} ${secs(m.action_seconds)}`, side: 'timing' });
  else if (factor) out.push({ id: 'speed', mark: 'note', text: `Played ${factor} to fit: ${possessive(other)} ${secs(m.action_seconds)} in ${possessive(hero)} ${secs(m.clip_seconds)}`, side: 'timing' });
  if (report.own.length) out.push({ id: 'arms', mark: 'note', text: `Arms and hands keep ${possessive(hero)} own motion: ${bones(report.own.length)} (${someBones(report.own)})`, side: 'unchanged' });
  // The engine counts the arm bones of the line above among its kept bones, all of the kind
  // 'other' (none of them has a finger, gun, cloth, face or IK word), so they are taken out here
  // and counted once. When the numbers do not allow that, the line says it includes them.
  const arms = report.own.length;
  const apart = arms > 0 && (m.kept_kinds.other ?? 0) >= arms && m.kept_count >= arms;
  const keptCount = apart ? m.kept_count - arms : m.kept_count;
  const keptKinds = apart ? { ...m.kept_kinds, other: (m.kept_kinds.other ?? 0) - arms } : m.kept_kinds;
  const kinds = Object.entries(keptKinds).filter(([kind, n]) => n > 0 && kind !== 'other').sort((a, b) => b[1] - a[1]).map(([kind]) => kind);
  if ((keptKinds.other ?? 0) > 0 && kinds.length) kinds.push('other');
  const withArms = arms > 0 && !apart ? ', the arms and hands included' : '';
  if (keptCount > 0) out.push({ id: 'kept', mark: 'note', text: `${bones(keptCount)} keep ${possessive(hero)} own motion${kinds.length ? ` (${kinds.join(', ')})` : ''}${withArms}`, side: 'unchanged' });
  if (report.not_matched.length) out.push({ id: 'unmatched', mark: 'note', text: `${bones(report.not_matched.length)} of ${possessive(hero)} have no bone of that name in ${other} and keep their own motion (${someBones(report.not_matched, 3)})`, side: 'unchanged' });
  const lift = liftLine(report);
  if (lift) out.push({ id: 'lift', mark: 'note', text: lift });
  return out;
}

/** The other hero's clips in the sections of the animation list: the same move, the same kind of
 * move, the same group, the rest, then those that cannot be used (the engine's ranks and flags). */
export function clipSections(clips: SourceClip[], query: string): { title: string; clips: SourceClip[] }[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const shown = clips.filter(c => words.every(w => `${c.name} ${c.label}`.toLowerCase().includes(w)));
  const usable = shown.filter(c => c.usable);
  const sections = [
    { title: 'Same move', clips: usable.filter(c => c.rank === 0) },
    { title: 'Similar · the same kind of move', clips: usable.filter(c => c.rank === 1) },
    { title: 'The same group', clips: usable.filter(c => c.rank === 2) },
    { title: 'Other animations', clips: usable.filter(c => c.rank === 3) },
    { title: 'Cannot be used', clips: shown.filter(c => !c.usable) },
  ];
  return sections.filter(s => s.clips.length);
}

/** "Dash is 5 animations (Dash ground, Dash air forward, …)": the set's members by this hero's own
 * clip names (from the catalog), else the clip name. */
export function setWords(set: ClipSet, entries: AnimationEntry[]): { lead: string; members: string } {
  const named = set.members.map(m => entries.find(e => m.target && e.target.toLowerCase() === m.target.toLowerCase())?.name ?? m.label);
  const name = set.name.charAt(0).toUpperCase() + set.name.slice(1);
  return { lead: `${name} is ${set.members.length} ${set.members.length === 1 ? 'animation' : 'animations'}`, members: named.join(', ') };
}

/** A hero's initials for the round mark beside its name ("The Rat King" → "RK"). */
export const initials = (name: string) => name.replace(/^The /, '').split(/[\s&]+/).filter(Boolean).slice(0, 2).map(w => w[0]).join('').toUpperCase();
