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
  fit_mode: 'Model fitting', fit_arm_joints: 'Model fitting', fit_finger_curl: 'Finger bend', fit_floor: 'Feet on the floor', fit_leg_joints: 'Leg joints', arm_skinning: 'Arm skinning',
  weapon: 'Gun choice', weapon_source: 'Gun file', physics_roots: 'Moving parts kept', physics_kinds: 'Moving part types',
  breast_motion: 'Breast bones', butt_motion: 'Butt bones', mapping: 'Joint matches', mapping_anchors: 'Joint matches',
  source_roles: 'Joint matches', target_roles: 'Joint matches', scene: 'Scene objects', shape_key_mode: 'Shape keys',
};
const CUSTOMIZATION: Record<string, string> = {
  secondary_motion: 'Physics and moving parts', physics: 'Cloth reference', appearance: 'Colors and surfaces',
  equipment: 'Equipment', face: 'Face', clothing: 'Clothing', cloth_setup: 'Cloth setup', icons: 'Icons',
  item_icons: 'Item icons', item_names: 'Item names',
  animations: 'Animations', effects: 'Effects', audio: 'Audio', enabled: 'Customization on or off', model_enabled: 'Character model on or off',
};
/** Bookkeeping that changes on every save or analysis, never a reason the mod looks different. */
const IGNORED = new Set(['schema', 'id', 'name', 'preparation_report', 'expected_dmx_sha256', 'reviewed', 'physics_offered', 'review_lods']);

const same = (a: unknown, b: unknown) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
const humanize = (name: string) => name.replaceAll('_', ' ').replace(/^./, c => c.toUpperCase());

/** Material assignments without what Analyze computes (Rust `MaterialChoices::remove_computed`:
 * `existing`, `ambiguous`, `candidates`). The build's save clears the candidates, and the next
 * Analyze lists them again without any choice changing. */
function materialChoices(value: unknown) {
  return Object.fromEntries(Object.entries((value ?? {}) as Record<string, Record<string, unknown>>)
    .map(([name, material]) => { const { candidates: _c, ambiguous: _a, existing: _e, ...rest } = material ?? {}; return [name, rest]; }));
}

type Rows = { materials?: Record<string, unknown>[] } | undefined;
/** Appearance rows: their color ranges are named apart from the other color and surface values. */
function appearanceChanges(before: unknown, now: unknown): string[] {
  const rows = (v: unknown) => ((v as Rows)?.materials ?? []);
  const ranges = (v: unknown) => rows(v).map(r => [r.target ?? '', r.ranges ?? null]);
  const rest = (v: unknown) => ({ ...(v as object ?? {}), materials: rows(v).map(({ ranges: _ranges, ...r }) => r) });
  const out: string[] = [];
  if (!same(rest(before), rest(now))) out.push(CUSTOMIZATION.appearance);
  if (!same(ranges(before), ranges(now))) out.push('Color ranges');
  return out.length ? out : [CUSTOMIZATION.appearance];
}

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
    } else if (k === 'materials') {
      if (!same(materialChoices(a), materialChoices(b))) add(TOP.materials);
    } else if (k === 'customization') {
      const changed = keys(a as object, b as object).filter(c => !same((a as Record<string, unknown>)?.[c], (b as Record<string, unknown>)?.[c]));
      // The first extras change switches customization on in the same step: the switch is
      // named only when it is the one customization change.
      for (const c of changed) {
        const x = (a as Record<string, unknown>)?.[c], y = (b as Record<string, unknown>)?.[c];
        if (c === 'enabled' && changed.length > 1) continue;
        if (c === 'appearance') appearanceChanges(x, y).forEach(add);
        else add(CUSTOMIZATION[c] ?? humanize(c));
      }
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
