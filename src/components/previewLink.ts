import { createContext, type MutableRefObject } from 'react';
import type { ProjectInspection } from '../build/models';
import type { Project } from '../models';
import { m } from '../i18n';
// i18n-area: physics

/** The moving bones the last Analyze recorded for the preview (none for an asset-only report). */
export function recordedMovingBones(report: ProjectInspection | null | undefined): number {
  if (!report || !('dmx' in report)) return 0;
  return (report.secondary_motion?.preview_nodes ?? report.motion_preview_nodes ?? []).length;
}
/** The last Analyze stopped before the moving parts: mesh mappings needed a choice (or the
 * kept moving-part bones are only in meshes left out of the build), so it recorded no motion.
 * Only with automatic secondary motion: in the other physics modes Analyze plans no moving
 * parts whatever the mappings are, and the other notices say what is true there. */
export function stoppedBeforeMovingParts(report: ProjectInspection | null | undefined, project: Project): boolean {
  if (!report || !('dmx' in report)) return false;
  if (!project.customization.enabled || !project.customization.secondary_motion?.enabled) return false;
  const waiting = report.unresolved_meshes.length > 0 || (report.secondary_meshes_left_out ?? []).length > 0;
  return waiting && recordedMovingBones(report) === 0;
}
/** What the Physics page says in that state, in the preview, on the tabs and in the status (English;
 * each place draws it with `t`). */
export const STOPPED_BEFORE_MOVING_PARTS = m('The last Analyze stopped before the moving parts, because mesh mappings on Build & install needed a choice. The parts swing here after those are set and Analyze has run again.');

/** What the Physics page's preview shows, as the Fine-tune and Moving parts tabs need it to say
 * whether the sliders can be seen working. `nodes`: moving bones the movement view swings (from
 * the last Analyze); `analyzed`: an Analyze report is held; `canMove`: the Movement button works;
 * `stopped`: that Analyze stopped before the moving parts (`stoppedBeforeMovingParts`). */
export interface PreviewState { mode: 'still' | 'movement'; loaded: boolean; nodes: number; analyzed: boolean; analyzing: boolean; canMove: boolean; stopped?: boolean }
/** The preview's own actions, so a button on a tab does exactly what the preview's button does. */
export interface PreviewActions { showMovement?: () => void; load?: () => void }
/** The page provides this around the one preview (the preview element is made by App, so a
 * context reaches it without new props there); the preview reports its state and actions. */
export interface PreviewLink { report: (state: PreviewState | null) => void; actions: MutableRefObject<PreviewActions> }
export const PreviewLinkContext = createContext<PreviewLink | null>(null);
