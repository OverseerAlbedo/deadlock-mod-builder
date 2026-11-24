/** The movement preview's cloth packet (written by Analyze for Automatic secondary
 * motion, read with `api.previewCloth`): the cloth model the game simulates, and
 * the mapping from the authored values and the Fine-tune sliders to the game's
 * per-node values. The mapping must equal Rust `preview_cloth::game_values`,
 * `node_values` and `effective_tuning` (shared fixtures in
 * `deadlock-cloth-authoring/tests/fixtures/preview-cloth-*.json`). */
import { capTravel, tune, type Tuning } from './motion';

/** The packet layout this page reads (Rust `preview_cloth::VERSION`). */
export const CLOTH_PACKET_VERSION = 1;

/** A travel cap applied after Fine-tune: exactly the held-root travel, or at most `inches`. */
export type ClothCap = {cap: 'held'} | {cap: 'at_most'; inches: number};
/** A simulated node's authored values before Fine-tune (gravity as a scale, travel in inches). */
export interface ClothNodeMotion {kind?: string | null; goal: number; drag: number; gravity: number; travel: number; caps?: ClothCap[]; preview_node?: number; attachment?: number}
/** `bind`: x, y, z, scale, qx, qy, qz, qw in model space. */
export interface ClothBone {name: string; bind: number[]}
export interface ClothRods {a: number[]; b: number[]; min: number[]; max: number[]; weight0: number[]}
/** Sphere centres in the bone's bind frame (inches), one radius, and the layer mask. */
export interface ClothCollider {bone: number; ends: number[][]; radius: number; layers: number}
/** A moving bone placed from its nodes: y = unit(Y1 − Y0), x = (X1 − X0) made
 * perpendicular to y, z = x × y; rotation = that frame · `adjust`; position =
 * node `target` − rotation · `offset`. */
export interface ClothBase {bone: number; target: number; offset: number[]; x: number[]; y: number[]; adjust: number[]}
export interface ClothPacket {
  version: number;
  source_sha256: string;
  proxy_sha256: string;
  bones: ClothBone[];
  /** Nodes below this follow their bone exactly; the rest are simulated. */
  static_nodes: number;
  positions: number[][];
  node_bones: number[];
  inv_mass: number[];
  layers: number[];
  /** One entry per simulated node (index − `static_nodes`). */
  motion: ClothNodeMotion[];
  rods: ClothRods;
  colliders: ClothCollider[];
  bases: ClothBase[];
}

/** Compiled gravity of a gravity scale of 1, in inches per second squared. */
export const GAME_GRAVITY = 360;
/** Compiled point damping per unit of authored drag. */
export const GAME_DAMPING = 30;
/** The smallest travel limit the compiler keeps, as float32 (inches). */
export const MIN_TRAVEL = Math.fround(0.03);
/** A held root's travel limit (Rust `secondary_auto::HELD_ROOT_TRAVEL`). */
export const HELD_ROOT_TRAVEL = 0.03;
/** An attachment limit the build adds after compiling, in compiled units. */
export const ATTACHMENT_ATTRACTION = 0.5, ATTACHMENT_DAMPING = 11.4;

/** The game's per-node values: attraction (share of the way to the animated
 * place per step), damping per second, gravity (in/s²), travel (in; 0 = free). */
export interface GameValues {attraction: number; damping: number; gravity: number; travel: number}

/** Authored values to the game's, with the inputs rounded to float32 as the proxy stores them. */
export function gameValues(goal: number, drag: number, gravity: number, travel: number): GameValues {
  const g = Math.fround(goal), t = Math.fround(travel);
  return {attraction: g * g / (1 + g * g), damping: GAME_DAMPING * Math.fround(drag), gravity: Math.fround(gravity * GAME_GRAVITY), travel: t >= MIN_TRAVEL ? t : 0};
}

/** A node's game values from its authored values, the Fine-tune of its part type
 * (undefined when that type has no non-zero setting: then nothing is tuned), its
 * caps and its attachment limit, in the build's order. */
export function nodeValues(m: ClothNodeMotion, tuning?: Tuning): GameValues {
  let v = {goal: m.goal, drag: m.drag, gravity: m.gravity, travel: m.travel};
  if (tuning) v = tune(v, tuning);
  let travel = v.travel;
  for (const cap of m.caps ?? []) travel = cap.cap === 'held' ? HELD_ROOT_TRAVEL : capTravel(travel, cap.inches);
  const out = gameValues(v.goal, v.drag, v.gravity, travel);
  if (m.attachment !== undefined && m.attachment !== null) {
    out.attraction = Math.max(out.attraction, ATTACHMENT_ATTRACTION);
    out.damping = Math.max(out.damping, ATTACHMENT_DAMPING);
    out.travel = Math.fround(out.travel > 0 ? Math.min(out.travel, m.attachment) : m.attachment);
  }
  return out;
}

/** The Fine-tune settings the build applies: settings that are all zero are left out. */
export function effectiveTuning(tuning: Record<string, Tuning | undefined>): Record<string, Tuning> {
  const out: Record<string, Tuning> = {};
  for (const [kind, t] of Object.entries(tuning)) {
    if (t && [t.stiffness, t.damping, t.gravity, t.swing].some(v => typeof v === 'number' && v !== 0)) out[kind] = t;
  }
  return out;
}

/** A packet the page can play: the right version and consistent lengths. */
export function validPacket(value: unknown): value is ClothPacket {
  const p = value as ClothPacket | null;
  if (!p || typeof p !== 'object' || p.version !== CLOTH_PACKET_VERSION) return false;
  const n = p.positions?.length ?? 0;
  if (!n || p.node_bones?.length !== n || p.layers?.length !== n || !(p.static_nodes >= 0 && p.static_nodes <= n) || p.motion?.length !== n - p.static_nodes) return false;
  const r = p.rods;
  if (!r || [r.b, r.min, r.max, r.weight0].some(a => a?.length !== r.a?.length)) return false;
  const bones = p.bones?.length ?? 0;
  if (p.node_bones.some(b => !(b >= 0 && b < bones))) return false;
  if (r.a.some(i => !(i >= 0 && i < n)) || r.b.some(i => !(i >= 0 && i < n))) return false;
  if ((p.bases ?? []).some(b => !(b.bone < bones) || ![b.target, ...b.x, ...b.y].every(i => i >= 0 && i < n))) return false;
  return (p.colliders ?? []).every(c => c.bone < bones);
}
