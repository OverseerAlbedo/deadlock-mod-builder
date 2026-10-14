import type { Project } from '../models';

/** What a build was made from, kept per project on this PC to list what changed since. */
export interface BuildRecord { time: string; project: Project }
const key = (id: string) => `dmb.lastBuild.${id}`;

/** Names for the project parts people change between builds. */
const TOP: Record<string, string> = {
  original_model: 'Hero', dmx: 'Prepared model', preparation_source: 'Model file', materials: 'Materials',
  meshes: 'Mesh mapping', lod_strategy: 'Detail levels', vpk_number: 'VPK slot', textures: 'Texture folder',
  compile_model: 'Output settings', build_vpk: 'Output settings', install: 'Output settings',
  animation_policy: 'Animation settings', ag2_mode: 'Animation settings',
};
const PREPARATION: Record<string, string> = {
  fit_mode: 'Model fitting', fit_arm_joints: 'Model fitting', fit_finger_curl: 'Finger bend', arm_skinning: 'Arm skinning',
  weapon: 'Gun choice', weapon_source: 'Gun file', physics_roots: 'Moving parts kept', physics_kinds: 'Moving part types',
  breast_motion: 'Breast bones', butt_motion: 'Butt bones', mapping: 'Bone mapping', mapping_anchors: 'Bone mapping',
  source_roles: 'Joint matches', target_roles: 'Joint matches', scene: 'Scene objects', shape_key_mode: 'Shape keys',
};
const CUSTOMIZATION: Record<string, string> = {
  secondary_motion: 'Physics and moving parts', physics: 'Cloth reference', appearance: 'Colors and surfaces',
  equipment: 'Equipment', face: 'Face', clothing: 'Clothing', cloth_setup: 'Cloth setup', icons: 'Icons',
  animations: 'Animations', effects: 'Effects', audio: 'Audio', enabled: 'Customization on or off', model_enabled: 'Character model on or off',
};
/** Bookkeeping that changes on every save or analysis, never a reason the mod looks different. */
const IGNORED = new Set(['schema', 'id', 'name', 'preparation_report', 'expected_dmx_sha256', 'reviewed', 'physics_offered', 'review_lods']);

const same = (a: unknown, b: unknown) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
const humanize = (name: string) => name.replaceAll('_', ' ').replace(/^./, c => c.toUpperCase());

/** Labels of what changed between the last build and now, in a stable order without repeats. */
export function changesSince(before: Project, now: Project): string[] {
  const out: string[] = [];
  const add = (label: string) => { if (!out.includes(label)) out.push(label); };
  const keys = (a: object, b: object) => [...new Set([...Object.keys(a ?? {}), ...Object.keys(b ?? {})])].filter(k => !IGNORED.has(k));
  for (const k of keys(before, now)) {
    const a = (before as unknown as Record<string, unknown>)[k], b = (now as unknown as Record<string, unknown>)[k];
    if (same(a, b)) continue;
    if (k === 'preparation_options') {
      for (const p of keys(a as object, b as object)) if (!same((a as Record<string, unknown>)?.[p], (b as Record<string, unknown>)?.[p])) add(PREPARATION[p] ?? 'Preparation choices');
    } else if (k === 'customization') {
      for (const c of keys(a as object, b as object)) if (!same((a as Record<string, unknown>)?.[c], (b as Record<string, unknown>)?.[c])) add(CUSTOMIZATION[c] ?? humanize(c));
    } else add(TOP[k] ?? humanize(k));
  }
  return out;
}

export function loadBuildRecord(id: string): BuildRecord | null {
  try { const raw = localStorage.getItem(key(id)); return raw ? JSON.parse(raw) as BuildRecord : null; } catch { return null; }
}
export function saveBuildRecord(project: Project, time = new Date().toISOString()) {
  try { localStorage.setItem(key(project.id), JSON.stringify({ time, project } satisfies BuildRecord)); } catch { /* Browser storage can be unavailable; the list is a convenience. */ }
}
