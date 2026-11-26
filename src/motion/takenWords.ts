/** Words of Motion → Another hero's animation, made only from the engine's answers (the report of
 * `taken_clip_report`, the lists of `taken_clip_heroes` / `taken_clip_sources`). No clip or
 * retarget logic: numbers are only put into sentences. The sentences come in the interface
 * language and are only drawn; hero, clip and bone names and the engine's kinds stay as they come. */
// i18n-area: motion
import type { AnimationEntry } from '../components/AssetCatalog';
import { mc, t, tp } from '../i18n';
import { shownHero } from '../i18n/gameNames';
import { nameList } from '../build/words';
import { liftLine, someBones, type MakesLine } from './madeRows';
import type { ClipSet, SourceClip, TakenReport } from './takenTypes';

const secs = (value: number) => t('{value} s', { value: value.toFixed(2) });
/** Faster or slower and by how much ("1.5"), or null at its own speed, from the report's stretch. */
function speed(stretch: number): { faster: boolean; factor: string } | null {
  if (!(stretch > 0) || Math.abs(stretch - 1) < 0.005) return null;
  return stretch > 1 ? { faster: true, factor: stretch.toFixed(1) } : { faster: false, factor: (1 / stretch).toFixed(1) };
}
/** "1.5 times faster" / "1.4 times slower" / null at its own speed, from the report's stretch. */
export function speedFactor(stretch: number): string | null {
  const change = speed(stretch);
  if (!change) return null;
  return change.faster ? t('{factor} times faster', { factor: change.factor }) : t('{factor} times slower', { factor: change.factor });
}

/** The sentence under Timing: how the other hero's clip fills this hero's (from the report). */
export function timingWords(report: TakenReport, hero: string, stretched: boolean): string {
  const made = report.made, other = shownHero(report.source_name);
  if (!stretched) return t('{other}’s clip plays at its own speed, from {from} to {to} of its {action}, over {hero}’s {clip}.', { other, hero, from: secs(made.plays_from), to: secs(made.plays_to), action: secs(made.action_seconds), clip: secs(made.clip_seconds) });
  const change = speed(made.stretch);
  const values = { other, hero, action: secs(made.action_seconds), clip: secs(made.clip_seconds) };
  if (!change) return t('{other}’s whole {action} clip plays in {hero}’s {clip}, at its own speed.', values);
  return change.faster
    ? t('{other}’s whole {action} clip plays in {hero}’s {clip}, {factor} times faster.', { ...values, factor: change.factor })
    : t('{other}’s whole {action} clip plays in {hero}’s {clip}, {factor} times slower.', { ...values, factor: change.factor });
}

/** The line under Arms and hands. */
export const armsWords = (other: string, hero: string) =>
  t('{other}’s arms and hands pose as if holding their own weapon; keep {hero}’s own for their grip.', { other, hero });

/** "What you get", only from the report. A taken clip always keeps this hero's length, events and
 * travel, so there is no pass or fail here. */
export function takenLines(report: TakenReport, hero: string, stretched: boolean): MakesLine[] {
  const made = report.made, other = shownHero(report.source_name);
  const kept = t('kept by the app'), timing = t('timing'), unchanged = t('unchanged');
  const out: MakesLine[] = [
    { id: 'body', mark: 'yours', text: report.driven.length
      ? tp(report.driven.length, { one: 'The body’s motion comes from {other}: {count} bone ({bones})', other: 'The body’s motion comes from {other}: {count} bones ({bones})' }, { other, bones: someBones(report.driven) })
      : tp(0, { one: 'The body’s motion comes from {other}: {count} bone', other: 'The body’s motion comes from {other}: {count} bones' }, { other }), side: t('from {other}', { other }) },
    { id: 'travel', mark: 'kept', text: t('How far and how fast the move goes stays {hero}’s', { hero }), side: kept },
    { id: 'timing-kept', mark: 'kept', text: t('Its timing, sounds and effects stay {hero}’s ({clip}, events at the same times)', { hero, clip: secs(made.clip_seconds) }), side: kept },
  ];
  const change = speed(made.stretch);
  const values = { other, hero, action: secs(made.action_seconds), clip: secs(made.clip_seconds) };
  if (!stretched) out.push({ id: 'speed', mark: 'note', text: t('Plays at its own speed: {from} to {to} of {other}’s {action}', { ...values, from: secs(made.plays_from), to: secs(made.plays_to) }), side: timing });
  else if (change) out.push({ id: 'speed', mark: 'note', text: change.faster
    ? t('Played {factor} times faster to fit: {other}’s {action} in {hero}’s {clip}', { ...values, factor: change.factor })
    : t('Played {factor} times slower to fit: {other}’s {action} in {hero}’s {clip}', { ...values, factor: change.factor }), side: timing });
  if (report.own.length) out.push({ id: 'arms', mark: 'note', text: tp(report.own.length, { one: 'Arms and hands keep {hero}’s own motion: {count} bone ({bones})', other: 'Arms and hands keep {hero}’s own motion: {count} bones ({bones})' }, { hero, bones: someBones(report.own) }), side: unchanged });
  // The engine counts the arm bones of the line above among its kept bones, all of the kind
  // 'other' (none of them has a finger, gun, cloth, face or IK word), so they are taken out here
  // and counted once. When the numbers do not allow that, the line says it includes them.
  const arms = report.own.length;
  const apart = arms > 0 && (made.kept_kinds.other ?? 0) >= arms && made.kept_count >= arms;
  const keptCount = apart ? made.kept_count - arms : made.kept_count;
  const keptKinds = apart ? { ...made.kept_kinds, other: (made.kept_kinds.other ?? 0) - arms } : made.kept_kinds;
  const kinds = Object.entries(keptKinds).filter(([kind, n]) => n > 0 && kind !== 'other').sort((a, b) => b[1] - a[1]).map(([kind]) => kind);
  if ((keptKinds.other ?? 0) > 0 && kinds.length) kinds.push('other');
  const withArms = arms > 0 && !apart;
  const keptValues = { hero, kinds: nameList(kinds) };
  if (keptCount > 0) out.push({ id: 'kept', mark: 'note', text: kinds.length
    ? withArms
      ? tp(keptCount, { one: '{count} bone keep {hero}’s own motion ({kinds}), the arms and hands included', other: '{count} bones keep {hero}’s own motion ({kinds}), the arms and hands included' }, keptValues)
      : tp(keptCount, { one: '{count} bone keep {hero}’s own motion ({kinds})', other: '{count} bones keep {hero}’s own motion ({kinds})' }, keptValues)
    : withArms
      ? tp(keptCount, { one: '{count} bone keep {hero}’s own motion, the arms and hands included', other: '{count} bones keep {hero}’s own motion, the arms and hands included' }, keptValues)
      : tp(keptCount, { one: '{count} bone keep {hero}’s own motion', other: '{count} bones keep {hero}’s own motion' }, keptValues), side: unchanged });
  if (report.not_matched.length) out.push({ id: 'unmatched', mark: 'note', text: tp(report.not_matched.length, { one: '{count} bone of {hero}’s have no bone of that name in {other} and keep their own motion ({bones})', other: '{count} bones of {hero}’s have no bone of that name in {other} and keep their own motion ({bones})' }, { hero, other, bones: someBones(report.not_matched, 3) }), side: unchanged });
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
  // The titles stay English (they are keys); they are shown with `tc('taken section', title)`.
  const sections = [
    { title: mc('taken section', 'Same move'), clips: usable.filter(c => c.rank === 0) },
    { title: mc('taken section', 'Similar · the same kind of move'), clips: usable.filter(c => c.rank === 1) },
    { title: mc('taken section', 'The same group'), clips: usable.filter(c => c.rank === 2) },
    { title: mc('taken section', 'Other animations'), clips: usable.filter(c => c.rank === 3) },
    { title: mc('taken section', 'Cannot be used'), clips: shown.filter(c => !c.usable) },
  ];
  return sections.filter(s => s.clips.length);
}

/** "Dash is 5 animations (Dash ground, Dash air forward, …)": the set's members by this hero's own
 * clip names (from the catalog), else the clip name. */
export function setWords(set: ClipSet, entries: AnimationEntry[]): { lead: string; members: string } {
  const named = set.members.map(member => entries.find(e => member.target && e.target.toLowerCase() === member.target.toLowerCase())?.name ?? member.label);
  const name = set.name.charAt(0).toUpperCase() + set.name.slice(1);
  return { lead: tp(set.members.length, { one: '{name} is {count} animation', other: '{name} is {count} animations' }, { name }), members: nameList(named) };
}

/** A hero's initials for the round mark beside its name ("The Rat King" → "RK"). */
export const initials = (name: string) => name.replace(/^The /, '').split(/[\s&]+/).filter(Boolean).slice(0, 2).map(w => w[0]).join('').toUpperCase();
