/** Surface changes (`customization.appearance.materials`) as the Materials tab shows them.
 * A row applies on top of a material at build: roughness, metalness, opacity and emission
 * (each optional; unset keeps the material's own value) and the colour values, which
 * Extras → Colors edits. A row is never merged into the material's own generation values
 * (`project.materials[…]` roughness, metalness, tint): those are a different thing.
 * Every write keeps the rest of the row (colour values, `ranges`, unknown keys) and sets
 * `appearance.enabled: true` like `AppearanceOverrides`' `setRows`; reading writes nothing. */
import type { AppearanceMaterial, Project } from '../models';
import { appearanceRowIndex, APPEARANCE_LIMIT, colorValues } from '../extras/colorAdapter';
import { modelEnabled } from '../build/readiness';
import { changeCustomization } from './customization';
import { m } from '../i18n';
// i18n-area: appearance

export const SURFACE_KEYS = ['roughness', 'metalness', 'opacity', 'emission'] as const;
export type SurfaceKey = typeof SURFACE_KEYS[number];
/** The value a "Change …" tick starts at (the old Override ticks' starting values). */
export const SURFACE_START: Record<SurfaceKey, number> = {roughness: .5, metalness: 0, opacity: 1, emission: 0};
export const SURFACE_MAX: Record<SurfaceKey, number> = {roughness: 1, metalness: 1, opacity: 1, emission: 64};
export const SURFACE_LABELS: Record<SurfaceKey, string> = {roughness: m('Roughness'), metalness: m('Metalness'), opacity: m('Opacity'), emission: m('Glow (emission)')};
/** The "Change …" tick of each value and its slider, as whole labels (English text is the key). */
export const SURFACE_CHANGE: Record<SurfaceKey, string> = {roughness: m('Change roughness'), metalness: m('Change metalness'), opacity: m('Change opacity'), emission: m('Change glow (emission)')};
export const SURFACE_ON_TOP: Record<SurfaceKey, string> = {roughness: m('Roughness on top'), metalness: m('Metalness on top'), opacity: m('Opacity on top'), emission: m('Glow (emission) on top')};

const rowsOf = (project: Project) => project.customization.appearance?.materials ?? [];
export const surfaceLimit = APPEARANCE_LIMIT;

/** The indexes of the rows that name this material, in saved order. */
export function rowsFor(project: Project, material: string): number[] {
  return rowsOf(project).flatMap((row, index) => row.target === material ? [index] : []);
}
/** Whether Extras → Colors edits this row's colour: the replacement model is on, the row names a
 * material of the prepared model, and it is the row the Colors page picks for it
 * (`appearanceRowIndex`: the last enabled one, else the last one). */
export function colorsEdits(project: Project, index: number): boolean {
  const rows = rowsOf(project), row = rows[index];
  return Boolean(row?.target && project.materials[row.target] && modelEnabled(project) && appearanceRowIndex(rows, row.target) === index);
}
/** Rows the Materials editor does not reach through a material: no target, or a material the
 * prepared model no longer has. They keep the old row editor. */
export function freeRows(project: Project): number[] {
  return rowsOf(project).flatMap((row, index) => !row.target || !project.materials[row.target] ? [index] : []);
}
/** Whether this row carries colour values (tint, hue, saturation, brightness, ranges). */
export const hasColor = (row: AppearanceMaterial) => colorValues(row) !== null;

function setRows(project: Project, materials: AppearanceMaterial[]): Project {
  const appearance = project.customization.appearance ?? {};
  return changeCustomization(project, {appearance: {...appearance, enabled: true, materials}});
}
/** The project with one value of a row set (a number) or removed (undefined). */
export function setSurface(project: Project, index: number, key: SurfaceKey | 'enabled', value: number | boolean | undefined): Project {
  const rows = rowsOf(project);
  if (!rows[index]) return project;
  const next: AppearanceMaterial = {...rows[index]};
  if (value === undefined) delete next[key]; else next[key] = value as never;
  return setRows(project, rows.map((row, i) => i === index ? next : row));
}
/** The project with a new, empty row for this material (null at the row limit). */
export function addSurface(project: Project, material: string): Project | null {
  const rows = rowsOf(project);
  return rows.length >= APPEARANCE_LIMIT ? null : setRows(project, [...rows, {enabled: true, target: material}]);
}
export function removeSurface(project: Project, index: number): Project {
  return setRows(project, rowsOf(project).filter((_, i) => i !== index));
}
/** "Include surface changes" (`appearance.enabled`), as the old tick wrote it. */
export function includeSurfaces(project: Project, enabled: boolean): Project {
  const appearance = project.customization.appearance ?? {};
  return changeCustomization(project, {appearance: {...appearance, enabled}});
}
