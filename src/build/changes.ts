// i18n-area: build
import type { Project } from '../models';
import { m } from '../i18n';

/** What a build was made from, kept per project on this PC to list what changed since. */
export interface BuildRecord { time: string; project: Project }
const key = (id: string) => `dmb.lastBuild.${id}`;

/** Names for the project parts people change between builds. */
const TOP: Record<string, string> = {
  original_model: m('Hero'), dmx: m('Prepared model'), preparation_source: m('Model file'), materials: m('Materials'),
  meshes: m('Mesh mapping'), lod_strategy: m('Detail levels'), vpk_number: m('VPK slot'), textures: m('Texture folder'),
  compile_model: m('Output settings'), build_vpk: m('Output settings'), install: m('Output settings'),
  animation_policy: m('Animation settings'), ag2_mode: m('Animation settings'),
  // Named exactly as `humanize` would name them, so that they can be translated.
  hero_name: m('Hero name'), allow_unskinned: m('Allow unskinned'), auto_split: m('Auto split'), split_target: m('Split target'),
  auto_map: m('Auto map'), generate_materials: m('Generate materials'), auto_decompile: m('Auto decompile'),
};
const PREPARATION: Record<string, string> = {
  fit_mode: m('Model fitting'), fit_arm_joints: m('Model fitting'), fit_finger_curl: m('Finger bend'), fit_floor: m('Feet on the floor'), fit_leg_joints: m('Leg joints'), arm_skinning: m('Arm skinning'),
  weapon: m('Gun choice'), weapon_source: m('Gun file'), physics_roots: m('Moving parts kept'), physics_kinds: m('Moving part types'),
  breast_motion: m('Breast bones'), butt_motion: m('Butt bones'), mapping: m('Joint matches'), mapping_anchors: m('Joint matches'),
  source_roles: m('Joint matches'), target_roles: m('Joint matches'), scene: m('Scene objects'), shape_key_mode: m('Shape keys'),
};
const CUSTOMIZATION: Record<string, string> = {
  secondary_motion: m('Physics and moving parts'), physics: m('Cloth reference'), appearance: m('Colors and surfaces'),
  equipment: m('Equipment'), face: m('Face'), clothing: m('Clothing'), cloth_setup: m('Cloth setup'), icons: m('Icons'),
  item_icons: m('Item icons'), item_names: m('Item names'),
  animations: m('Animations'), effects: m('Effects'), audio: m('Audio'), enabled: m('Customization on or off'), model_enabled: m('Character model on or off'),
  face_layers: m('Face layers'), hero_materials: m('Hero materials'), effect_edits: m('Effect edits'), ability_objects: m('Ability objects'),
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
  if (!same(ranges(before), ranges(now))) out.push(m('Color ranges'));
  return out.length ? out : [CUSTOMIZATION.appearance];
}

/** Labels of what changed between the last build and now, in a stable order without repeats. The
 * labels stay English (they are keys and joined by the undo toast); translate them where they are drawn. */
export function changesSince(before: Project, now: Project): string[] {
  const out: string[] = [];
  const add = (label: string) => { if (!out.includes(label)) out.push(label); };
  const keys = (a: object, b: object) => [...new Set([...Object.keys(a ?? {}), ...Object.keys(b ?? {})])].filter(k => !IGNORED.has(k));
  for (const k of keys(before, now)) {
    const a = (before as unknown as Record<string, unknown>)[k], b = (now as unknown as Record<string, unknown>)[k];
    if (same(a, b)) continue;
    if (k === 'preparation_options') {
      for (const p of keys(a as object, b as object)) if (!same((a as Record<string, unknown>)?.[p], (b as Record<string, unknown>)?.[p])) add(PREPARATION[p] ?? m('Preparation choices'));
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
