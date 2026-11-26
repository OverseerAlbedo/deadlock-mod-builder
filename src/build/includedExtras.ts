// i18n-area: build
import type { Project } from '../models';
import { tp } from '../i18n';
import { effectEditCounts } from '../extras/effectEdits';
import { objectCounts } from '../extras/objectRows';

type Row = { enabled?: boolean };

/**
 * The extras the project asks the next build to include, in words ("1 ability object · 2
 * sounds", in the interface language: only ever drawn): the saved rows that are switched on.
 * Empty while extras are paused or there are none. The game hero's colors are not part of a build with a replacement model, so `model`
 * leaves them out. Counted from the project, so it shows before any Analyze.
 */
export function includedExtras(project: Project, model: boolean): string {
  const state = project.customization as unknown as Record<string, unknown>;
  if (!state.enabled) return '';
  const on = (key: string) => ((state[key] as Row[] | undefined) ?? []).filter(row => row.enabled !== false).length;
  const colors = state.hero_materials as { enabled?: boolean; materials?: Row[] } | undefined;
  const parts: [number, (n: number) => string][] = [
    [on('icons'), n => tp(n, { one: '{count} icon', other: '{count} icons' })],
    [on('item_icons'), n => tp(n, { one: '{count} item icon', other: '{count} item icons' })],
    [on('item_names'), n => tp(n, { one: '{count} item name', other: '{count} item names' })],
    [on('audio'), n => tp(n, { one: '{count} sound', other: '{count} sounds' })],
    [on('animations'), n => tp(n, { one: '{count} animation', other: '{count} animations' })],
    [on('effects'), n => tp(n, { one: '{count} effect file', other: '{count} effect files' })],
    [effectEditCounts(project).enabled, n => tp(n, { one: '{count} effect edit', other: '{count} effect edits' })],
    [objectCounts(project).enabled, n => tp(n, { one: '{count} ability object', other: '{count} ability objects' })],
    [model || colors?.enabled === false ? 0 : (colors?.materials ?? []).filter(row => row.enabled !== false).length, n => tp(n, { one: '{count} hero color', other: '{count} hero colors' })],
  ];
  return parts.filter(([n]) => n > 0).map(([n, words]) => words(n)).join(' · ');
}
