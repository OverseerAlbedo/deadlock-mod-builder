import type { Project } from '../models';
import { effectEditCounts } from '../extras/effectEdits';
import { objectCounts } from '../extras/objectRows';

type Row = { enabled?: boolean };
const words = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/**
 * The extras the project asks the next build to include, in words ("1 ability object · 2
 * sounds"): the saved rows that are switched on. Empty while extras are paused or there are
 * none. The game hero's colors are not part of a build with a replacement model, so `model`
 * leaves them out. Counted from the project, so it shows before any Analyze.
 */
export function includedExtras(project: Project, model: boolean): string {
  const state = project.customization as unknown as Record<string, unknown>;
  if (!state.enabled) return '';
  const on = (key: string) => ((state[key] as Row[] | undefined) ?? []).filter(row => row.enabled !== false).length;
  const colors = state.hero_materials as { enabled?: boolean; materials?: Row[] } | undefined;
  const parts: [number, string, string][] = [
    [on('icons'), 'icon', 'icons'],
    [on('item_icons'), 'item icon', 'item icons'],
    [on('item_names'), 'item name', 'item names'],
    [on('audio'), 'sound', 'sounds'],
    [on('animations'), 'animation', 'animations'],
    [on('effects'), 'effect file', 'effect files'],
    [effectEditCounts(project).enabled, 'effect edit', 'effect edits'],
    [objectCounts(project).enabled, 'ability object', 'ability objects'],
    [model || colors?.enabled === false ? 0 : (colors?.materials ?? []).filter(row => row.enabled !== false).length, 'hero color', 'hero colors'],
  ];
  return parts.filter(([n]) => n > 0).map(([n, one, many]) => words(n, one, many)).join(' · ');
}
