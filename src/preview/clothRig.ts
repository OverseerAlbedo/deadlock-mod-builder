/** The movement preview on the game's cloth model: the cloth packet's nodes are
 * simulated by `clothSolver.ts` in world space at a fixed tick, and every moving
 * bone is placed from its nodes by the packet's base rule. Bones below a moving
 * bone that are not moving themselves follow it; every other bone plays the clip.
 *
 * Time: the simulation ticks at `SOLVER_OPTIONS.hz` of clip time (the speed
 * buttons scale how fast clip time passes, never the tick), and the picture is
 * drawn one tick behind the newest tick, interpolated between the last two, so
 * it is smooth at any refresh rate. A long pause re-seats the simulation on the
 * animated places, and so does a cut in a clip (a jump starting again). A change
 * of motion blends the poses and the speed over `EASE_SECONDS`. */
import type { MotionHeader, Tuning, Vec3 } from './motion';
import { effectiveTuning, nodeValues, type ClothPacket } from './clothPacket';
import { CAPSULE_STRIDE, SOLVER_OPTIONS, createState, seat, step, type ClothModel, type SolverOptions, type SolverState } from './clothSolver';
import { EASE_SECONDS, ease, type ClothMotion } from './clothMotion';

/** A frame gap (seconds) after which the simulation starts again from the animated pose. */
export const LONG_GAP = .5;
/** The longest clip time one frame advances (seconds): a slow frame slows the preview down. */
export const MAX_FRAME = .1;
/** The world origin moves along with a long run once the character is this far from it (inches). */
const REBASE = 1000;

type M3 = number[];
const quatMatrix = (q: ArrayLike<number>, o = 0): M3 => {
  const x = q[o], y = q[o + 1], z = q[o + 2], w = q[o + 3];
  return [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w), 2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w), 2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)];
};

/** A moving bone's rotation (row-major 3x3) and position from four node
 * positions, its `adjust` rotation (3x3), its target node's position and its
 * offset: y = unit(Y1 − Y0), x = (X1 − X0) made perpendicular to y, z = x × y,
 * rotation = [x y z] · adjust, position = target − rotation · offset. Writes 12
 * numbers (rotation, then position) into `out` at `o`; false for a collapsed frame. */
export function placeBase(p: ArrayLike<number>, x0: number, x1: number, y0: number, y1: number, target: number, adjust: ArrayLike<number>, offset: ArrayLike<number>, out: Float64Array, o = 0): boolean {
  let yx = p[y1 * 3] - p[y0 * 3], yy = p[y1 * 3 + 1] - p[y0 * 3 + 1], yz = p[y1 * 3 + 2] - p[y0 * 3 + 2];
  const yl = Math.hypot(yx, yy, yz);
  if (!(yl > 1e-9)) return false;
  yx /= yl; yy /= yl; yz /= yl;
  let xx = p[x1 * 3] - p[x0 * 3], xy = p[x1 * 3 + 1] - p[x0 * 3 + 1], xz = p[x1 * 3 + 2] - p[x0 * 3 + 2];
  const d = xx * yx + xy * yy + xz * yz;
  xx -= d * yx; xy -= d * yy; xz -= d * yz;
  const xl = Math.hypot(xx, xy, xz);
  if (!(xl > 1e-9)) return false;
  xx /= xl; xy /= xl; xz /= xl;
  const zx = xy * yz - xz * yy, zy = xz * yx - xx * yz, zz = xx * yy - xy * yx;
  const f = [xx, yx, zx, xy, yy, zy, xz, yz, zz];
  for (let r = 0; r < 3; r++) for (let c = 0; c < 3; c++) out[o + r * 3 + c] = f[r * 3] * adjust[c] + f[r * 3 + 1] * adjust[3 + c] + f[r * 3 + 2] * adjust[6 + c];
  for (let r = 0; r < 3; r++) out[o + 9 + r] = p[target * 3 + r] - (out[o + r * 3] * offset[0] + out[o + r * 3 + 1] * offset[1] + out[o + r * 3 + 2] * offset[2]);
  return true;
}

interface Base {motion: number; target: number; x0: number; x1: number; y0: number; y1: number; adjust: M3; offset: number[]; bindRotation: M3; bindPosition: number[]}

/** Why a packet cannot drive this preview (shown as a quiet line), or null. */
export type ClothRigProblem = 'bones' | 'frames';

/** The cloth packet bound to the movement preview's skeleton. */
export class ClothRig {
  readonly model: ClothModel;
  readonly options: SolverOptions;
  /** Motion-header bone carrying each node, and the node's bind position. */
  private readonly nodeMotion: Int32Array;
  private readonly rest: Float64Array;
  private readonly bases: Base[];
  private readonly colliders: {motion: number; a: number[]; b: number[]; radius: number; layers: number}[];
  private readonly packet: ClothPacket;
  private readonly bones: MotionHeader['bones'];
  private readonly depthOrder: number[];
  /** The bone each motion bone is placed by (index into `bases`), or −1. */
  private readonly baseOfBone: Int32Array;
  readonly state: SolverState;
  /** Animated places of this tick and of the tick before, in the world (less `origin`). */
  readonly anim: Float64Array;
  private readonly caps: Float64Array;
  /** Node positions less their tick's world offset: the newest tick and the one before. */
  private charNow: Float64Array;
  private charBefore: Float64Array;
  private readonly drawn: Float64Array;
  private readonly placed: Float64Array;
  private readonly delta: Float64Array;
  private readonly hasDelta: Uint8Array;
  private readonly scratch = new Float64Array(12);
  private origin: Vec3 = [0, 0, 0];
  private seated = false;
  private tuningKey = '';

  private constructor(packet: ClothPacket, header: MotionHeader, boneMotion: Int32Array, options: SolverOptions) {
    this.packet = packet; this.bones = header.bones; this.options = options;
    const n = packet.positions.length, rods = packet.rods;
    this.model = {
      count: n, staticNodes: packet.static_nodes,
      attraction: new Float64Array(n), damping: new Float64Array(n), gravity: new Float64Array(n), travel: new Float64Array(n),
      layers: Int32Array.from(packet.layers),
      rodA: Int32Array.from(rods.a), rodB: Int32Array.from(rods.b),
      rodMin: Float64Array.from(rods.min), rodMax: Float64Array.from(rods.max), rodWeight0: Float64Array.from(rods.weight0),
    };
    this.nodeMotion = Int32Array.from(packet.node_bones, b => boneMotion[b]);
    this.rest = Float64Array.from(packet.positions.flat());
    this.bases = packet.bases.map(b => {
      const bind = packet.bones[b.bone].bind;
      return {motion: boneMotion[b.bone], target: b.target, x0: b.x[0], x1: b.x[1], y0: b.y[0], y1: b.y[1], adjust: quatMatrix(b.adjust), offset: [...b.offset], bindRotation: quatMatrix(bind, 4), bindPosition: [bind[0], bind[1], bind[2]]};
    });
    this.colliders = packet.colliders.map(c => {
      const bind = packet.bones[c.bone].bind, r = quatMatrix(bind, 4);
      const place = (e: number[]) => [0, 1, 2].map(i => r[i * 3] * e[0] + r[i * 3 + 1] * e[1] + r[i * 3 + 2] * e[2] + bind[i]);
      return {motion: boneMotion[c.bone], a: place(c.ends[0]), b: place(c.ends[1]), radius: c.radius, layers: c.layers};
    });
    const depth = (i: number) => { let d = 0; for (let at = this.bones[i].parent; at >= 0; at = this.bones[at].parent) d++; return d; };
    const depths = this.bones.map((_, i) => depth(i));
    this.depthOrder = this.bones.map((_, i) => i).sort((a, b) => depths[a] - depths[b]);
    this.baseOfBone = new Int32Array(this.bones.length).fill(-1);
    this.bases.forEach((b, k) => { this.baseOfBone[b.motion] = k; });
    this.state = createState(n);
    this.anim = new Float64Array(n * 3);
    this.caps = new Float64Array(this.colliders.length * CAPSULE_STRIDE);
    this.charNow = new Float64Array(n * 3); this.charBefore = new Float64Array(n * 3);
    this.drawn = new Float64Array(n * 3);
    this.placed = new Float64Array(this.bases.length * 12);
    this.delta = new Float64Array(this.bones.length * 12);
    this.hasDelta = new Uint8Array(this.bones.length);
    this.setTuning({});
  }

  /** The rig for a packet on the movement preview's skeleton, or why there is none. */
  static create(packet: ClothPacket, header: MotionHeader, tuning: Record<string, Tuning | undefined> = {}, options: SolverOptions = SOLVER_OPTIONS): ClothRig | ClothRigProblem {
    const index = new Map(header.bones.map((b, i) => [b.name, i]));
    const boneMotion = Int32Array.from(packet.bones, b => index.get(b.name) ?? -1);
    if (boneMotion.some(i => i < 0)) return 'bones';
    const rig = new ClothRig(packet, header, boneMotion, options);
    // Every base must give a frame at rest.
    const p = rig.rest, out = new Float64Array(12);
    if (!rig.bases.every(b => placeBase(p, b.x0, b.x1, b.y0, b.y1, b.target, b.adjust, b.offset, out))) return 'frames';
    rig.setTuning(tuning);
    return rig;
  }

  /** The game's per-node values from the current Fine-tune sliders, rewritten in
   * place (the simulation keeps running). */
  setTuning(tuning: Record<string, Tuning | undefined>) {
    const effective = effectiveTuning(tuning), key = JSON.stringify(effective);
    if (key === this.tuningKey) return;
    this.tuningKey = key;
    const m = this.model, first = this.packet.static_nodes;
    this.packet.motion.forEach((motion, k) => {
      const v = nodeValues(motion, motion.kind ? effective[motion.kind] : undefined), i = first + k;
      m.attraction[i] = v.attraction; m.damping[i] = v.damping; m.gravity[i] = v.gravity; m.travel[i] = v.travel;
    });
  }

  /** Every node's animated place from rest-to-pose skins, plus `offset`. */
  places(skins: ArrayLike<number>, offset: Vec3, out: Float64Array, nodes = this.model.count) {
    const rest = this.rest;
    for (let i = 0; i < nodes; i++) {
      const o = this.nodeMotion[i] * 12, k = i * 3, x = rest[k], y = rest[k + 1], z = rest[k + 2];
      out[k] = skins[o] * x + skins[o + 1] * y + skins[o + 2] * z + skins[o + 3] + offset[0];
      out[k + 1] = skins[o + 4] * x + skins[o + 5] * y + skins[o + 6] * z + skins[o + 7] + offset[1];
      out[k + 2] = skins[o + 8] * x + skins[o + 9] * y + skins[o + 10] * z + skins[o + 11] + offset[2];
    }
    return out;
  }

  private capsules(skins: ArrayLike<number>, offset: Vec3) {
    const caps = this.caps;
    this.colliders.forEach((c, k) => {
      const o = c.motion * 12, at = k * CAPSULE_STRIDE;
      for (const [end, shift] of [[c.a, 0], [c.b, 3]] as const) {
        for (let r = 0; r < 3; r++) caps[at + shift + r] = skins[o + r * 4] * end[0] + skins[o + r * 4 + 1] * end[1] + skins[o + r * 4 + 2] * end[2] + skins[o + r * 4 + 3] + offset[r];
      }
      caps[at + 6] = c.radius; caps[at + 7] = c.radius; caps[at + 8] = c.layers;
    });
    return caps;
  }

  private local(offset: Vec3): Vec3 { return [offset[0] - this.origin[0], offset[1] - this.origin[1], offset[2] - this.origin[2]]; }

  /** Moves the world origin along with a long run, so positions keep their precision. */
  private rebase(offset: Vec3) {
    const d = this.local(offset);
    if (Math.abs(d[0]) < REBASE && Math.abs(d[1]) < REBASE && Math.abs(d[2]) < REBASE) return;
    const {x, prev} = this.state;
    for (let k = 0; k < x.length; k += 3) for (let r = 0; r < 3; r++) { x[k + r] -= d[r]; prev[k + r] -= d[r]; }
    this.origin = [this.origin[0] + d[0], this.origin[1] + d[1], this.origin[2] + d[2]];
  }

  private keepCharacter(offset: Vec3) {
    const swap = this.charBefore; this.charBefore = this.charNow; this.charNow = swap;
    const d = this.local(offset), x = this.state.x, c = this.charNow;
    for (let k = 0; k < x.length; k += 3) { c[k] = x[k] - d[0]; c[k + 1] = x[k + 1] - d[1]; c[k + 2] = x[k + 2] - d[2]; }
  }

  /** Puts every node on its animated place, moving as the animation moves from
   * `before` (the skins and offset one tick earlier) to now. */
  seat(skins: ArrayLike<number>, offset: Vec3, before?: {skins: ArrayLike<number>; offset: Vec3}) {
    this.origin = [offset[0], offset[1], offset[2]];
    this.places(skins, this.local(offset), this.anim);
    const earlier = before ? this.places(before.skins, this.local(before.offset), new Float64Array(this.anim.length)) : this.anim;
    seat(this.state, this.anim, earlier);
    this.keepCharacter(offset);
    this.charBefore.set(this.charNow);
    this.seated = true;
  }
  get ready() { return this.seated; }
  /** Forget the simulation: the next pose starts it again. */
  unseat() { this.seated = false; }

  /** One solver tick to the animated pose `skins` with the character at `offset` in the world. */
  tick(skins: ArrayLike<number>, offset: Vec3) {
    if (!this.seated) { this.seat(skins, offset); return; }
    this.rebase(offset);
    const local = this.local(offset);
    this.places(skins, local, this.anim);
    step(this.model, this.state, this.anim, this.capsules(skins, local), this.options);
    this.keepCharacter(offset);
  }

  /** Simulated node positions relative to the character, `alpha` (0..1) of the way
   * from the tick before the newest to the newest (fixed nodes from `skins`). */
  nodes(alpha: number, skins: ArrayLike<number>): Float64Array {
    const out = this.drawn, a = this.charBefore, b = this.charNow, first = this.model.staticNodes * 3;
    this.places(skins, [0, 0, 0], out, this.model.staticNodes);
    for (let k = first; k < out.length; k++) out[k] = a[k] + (b[k] - a[k]) * alpha;
    return out;
  }

  /** The rest-to-pose skins to draw (relative to the character): the clip's
   * `skins` at the drawn time, the moving bones placed from the nodes at
   * `alpha`, and the bones below them following them. */
  pose(alpha: number, skins: ArrayLike<number>, out: Float32Array): Float32Array {
    const p = this.nodes(alpha, skins), placed = this.placed, delta = this.delta, has = this.hasDelta;
    has.fill(0);
    const s = this.scratch;
    for (const bone of this.depthOrder) {
      const o = bone * 12, k = this.baseOfBone[bone];
      if (k >= 0) {
        const b = this.bases[k], q = k * 12;
        if (placeBase(p, b.x0, b.x1, b.y0, b.y1, b.target, b.adjust, b.offset, placed, q)) {
          // Skin = placed · bind⁻¹.
          const R = b.bindRotation, pb = b.bindPosition;
          for (let r = 0; r < 3; r++) for (let c = 0; c < 3; c++) s[r * 4 + c] = placed[q + r * 3] * R[c * 3] + placed[q + r * 3 + 1] * R[c * 3 + 1] + placed[q + r * 3 + 2] * R[c * 3 + 2];
          for (let r = 0; r < 3; r++) s[r * 4 + 3] = placed[q + 9 + r] - (s[r * 4] * pb[0] + s[r * 4 + 1] * pb[1] + s[r * 4 + 2] * pb[2]);
          for (let i = 0; i < 12; i++) out[o + i] = s[i];
          if (changeOf(s, skins, o, delta, o)) { has[bone] = 1; continue; }
        }
      }
      const parent = this.bones[bone].parent;
      if (parent >= 0 && has[parent]) {
        const d = parent * 12;
        for (let r = 0; r < 3; r++) for (let c = 0; c < 4; c++) out[o + r * 4 + c] = delta[d + r * 4] * skins[o + c] + delta[d + r * 4 + 1] * skins[o + 4 + c] + delta[d + r * 4 + 2] * skins[o + 8 + c] + (c === 3 ? delta[d + r * 4 + 3] : 0);
        for (let i = 0; i < 12; i++) delta[o + i] = delta[d + i];
        has[bone] = 1;
      } else {
        for (let i = 0; i < 12; i++) out[o + i] = skins[o + i];
      }
    }
    return out;
  }
}

/** d = a · m⁻¹ (row-major 3x4; m read from `skins` at `o`), false when m is singular. */
function changeOf(a: ArrayLike<number>, m: ArrayLike<number>, o: number, out: Float64Array, w: number): boolean {
  const a00 = m[o], a01 = m[o + 1], a02 = m[o + 2], a10 = m[o + 4], a11 = m[o + 5], a12 = m[o + 6], a20 = m[o + 8], a21 = m[o + 9], a22 = m[o + 10];
  const c00 = a11 * a22 - a12 * a21, c01 = a02 * a21 - a01 * a22, c02 = a01 * a12 - a02 * a11;
  const det = a00 * c00 + a10 * c01 + a20 * c02;
  if (Math.abs(det) < 1e-12) return false;
  const inv = [c00, c01, c02, a12 * a20 - a10 * a22, a00 * a22 - a02 * a20, a02 * a10 - a00 * a12, a10 * a21 - a11 * a20, a01 * a20 - a00 * a21, a00 * a11 - a01 * a10].map(v => v / det);
  const t = [m[o + 3], m[o + 7], m[o + 11]];
  const ti = [0, 1, 2].map(r => -(inv[r * 3] * t[0] + inv[r * 3 + 1] * t[1] + inv[r * 3 + 2] * t[2]));
  for (let r = 0; r < 3; r++) {
    for (let c = 0; c < 3; c++) out[w + r * 4 + c] = a[r * 4] * inv[c] + a[r * 4 + 1] * inv[3 + c] + a[r * 4 + 2] * inv[6 + c];
    out[w + r * 4 + 3] = a[r * 4] * ti[0] + a[r * 4 + 1] * ti[1] + a[r * 4 + 2] * ti[2] + a[r * 4 + 3];
  }
  return true;
}

/** Moving parts switched on or off in the movement view: the next frame is drawn even while paused
 * (`drawn` cleared), and the moving parts start again from the animated pose: the cloth model is
 * seated on the drawn pose, the simple swing (`MotionRig`) restarts there. */
export function switchMovingParts(playback: ClothPlayback | null, swing: {reset(): void}, drawn: {current: boolean}) {
  playback?.reseat();
  swing.reset();
  drawn.current = false;
}

/** What to draw for a frame: the rest-to-pose skins (blended during a change of motion), the
 * character's travel along the ground and its height above it (a fall's depth is left out, so
 * the view keeps the character in sight), whether to show the floor, and how far between the
 * last two ticks the frame lies. */
export interface ClothFrame {skins: Float32Array; offset: Vec3; alpha: number; floor: boolean}

/** A motion and when it started (simulation time). */
type Started = {motion: ClothMotion; start: number};

/** Plays motions on a cloth rig: advances with the frames, ticks the rig at its fixed rate,
 * blends a change of motion over `EASE_SECONDS` (poses, speed and height), and tells what to
 * draw. The simulation sees the travel and the height together; the view, only the height. */
export class ClothPlayback {
  private readonly rig: ClothRig;
  private readonly dt: number;
  private current: Started;
  /** The motion being blended out (itself a blend when the motion changed during a change). */
  private from: Started | null = null;
  private blendStart = 0;
  /** Simulation time of the newest tick and the time the frames have reached. */
  private sim = 0;
  private target = 0;
  /** Travel along the ground and down a fall (without a jump's height), now and a tick before. */
  private ground: Vec3 = [0, 0, 0];
  private groundBefore: Vec3 = [0, 0, 0];
  private readonly drawn: Float32Array;
  private readonly bones: number;
  /** What plays now (rebuilt when the motion or the blend changes). */
  private mix: ClothMotion | null = null;
  constructor(rig: ClothRig, motion: ClothMotion, bones: number) {
    this.rig = rig; this.current = {motion, start: 0}; this.dt = 1 / rig.options.hz; this.bones = bones;
    this.drawn = new Float32Array(bones * 12);
  }
  get motion() { return this.current.motion; }
  /** Simulation time of the newest tick. */
  get tickTime() { return this.sim; }

  /** The motion playing now, in simulation time: the current one, or the blend of the one being
   * blended out into it. */
  private playing(): ClothMotion { return this.mix ??= this.blended(); }
  private blended(): ClothMotion {
    const now = this.current, from = this.from;
    const local = (m: Started, s: number) => Math.max(0, s - m.start);
    if (!from) {
      return {name: now.motion.name, floor: now.motion.floor, sample: (s, out) => now.motion.sample(local(now, s), out),
        travel: (s0, s1) => now.motion.travel(local(now, s0), local(now, s1)), height: s => now.motion.height(local(now, s)),
        cut: (s0, s1) => now.motion.cut(local(now, s0), local(now, s1))};
    }
    const blendStart = this.blendStart, w = (s: number) => ease((s - blendStart) / EASE_SECONDS);
    const a = new Float32Array(this.bones * 12), b = new Float32Array(this.bones * 12);
    return {
      name: now.motion.name, floor: now.motion.floor,
      sample: (s, out) => {
        const old = from.motion.sample(local(from, s), a), next = now.motion.sample(local(now, s), b), k = w(s);
        for (let i = 0; i < out.length; i++) out[i] = old[i] + (next[i] - old[i]) * k;
        return out;
      },
      travel: (s0, s1) => {
        const old = from.motion.travel(local(from, s0), local(from, s1)), next = now.motion.travel(local(now, s0), local(now, s1)), k = w((s0 + s1) / 2);
        return [old[0] + (next[0] - old[0]) * k, old[1] + (next[1] - old[1]) * k, old[2] + (next[2] - old[2]) * k];
      },
      height: s => { const k = w(s); return from.motion.height(local(from, s)) * (1 - k) + now.motion.height(local(now, s)) * k; },
      cut: (s0, s1) => now.motion.cut(local(now, s0), local(now, s1)) || from.motion.cut(local(from, s0), local(from, s1)),
    };
  }
  private world(m: ClothMotion, s: number): Vec3 {
    return [this.ground[0], this.ground[1], this.ground[2] + m.height(s)];
  }
  /** Seats the simulation on the pose at the newest tick, moving as the motion moves now. */
  private seatHere() {
    const m = this.playing(), s = this.sim, skins = m.sample(s, this.drawn), step = m.travel(s, s + this.dt), world = this.world(m, s);
    const rise = m.height(s + this.dt) - m.height(s);
    this.rig.seat(skins, world, {skins, offset: [world[0] - step[0], world[1] - step[1], world[2] - step[2] - rise]});
    this.groundBefore = [...this.ground];
  }
  /** Seats the simulation on the pose drawn now, keeping the clock, the motion and the travel:
   * the moving parts sit on the animated pose (Moving parts switched on or off while paused). */
  reseat() {
    if (!this.rig.ready) { this.reset(); return; }
    const m = this.playing(), s = this.sim, alpha = Math.min(1, Math.max(0, (this.target - s) / this.dt));
    const skins = m.sample(Math.max(0, s - this.dt + alpha * this.dt), this.drawn), step = m.travel(s, s + this.dt), world = this.world(m, s);
    const rise = m.height(s + this.dt) - m.height(s);
    this.rig.seat(skins, world, {skins, offset: [world[0] - step[0], world[1] - step[1], world[2] - step[2] - rise]});
  }
  /** Starts the current motion again from its first pose. */
  reset() {
    this.from = null; this.current = {motion: this.current.motion, start: 0}; this.mix = null; this.sim = 0; this.target = 0;
    this.ground = [0, 0, 0]; this.seatHere();
  }
  /** Changes motion: the new one starts from its first pose and blends in over `EASE_SECONDS`
   * from what plays now (also when that is itself a change in progress). `now` (paused): the
   * new motion's first pose at once. */
  play(motion: ClothMotion, now = false) {
    if (motion === this.current.motion) return;
    if (!this.rig.ready) { this.current = {motion, start: 0}; this.mix = null; this.reset(); return; }
    if (now) {
      this.from = null; this.current = {motion, start: this.sim}; this.mix = null;
      this.seatHere();
      return;
    }
    this.from = {motion: this.playing(), start: 0};
    this.current = {motion, start: this.sim}; this.blendStart = this.sim; this.mix = null;
  }
  /** Advances by a frame of `delta` real seconds at `speed`. */
  advance(delta: number, speed: number, playing: boolean): ClothFrame {
    if (!this.rig.ready) this.reset();
    if (playing) {
      if (delta > LONG_GAP) { this.target = this.sim; this.seatHere(); }
      else this.target += Math.min(Math.max(delta, 0), MAX_FRAME) * speed;
      while (this.sim + this.dt <= this.target + 1e-9) this.tick();
    }
    const alpha = Math.min(1, Math.max(0, (this.target - this.sim) / this.dt));
    const shown = Math.max(0, this.sim - this.dt + alpha * this.dt), m = this.playing();
    const g = this.ground, v = this.groundBefore;
    return {skins: m.sample(shown, this.drawn), offset: [v[0] + (g[0] - v[0]) * alpha, v[1] + (g[1] - v[1]) * alpha, m.height(shown)], alpha, floor: m.floor};
  }
  private tick() {
    const m = this.playing(), s0 = this.sim, s1 = s0 + this.dt;
    const cut = m.cut(s0, s1), step = m.travel(s0, s1);
    this.sim = s1;
    this.groundBefore = [...this.ground];
    this.ground = [this.ground[0] + step[0], this.ground[1] + step[1], this.ground[2] + step[2]];
    if (this.from && s1 - this.blendStart >= EASE_SECONDS) { this.from = null; this.mix = null; }
    if (cut) {
      // A cut starts again from the new pose, keeping the travel's speed; a change in progress is finished.
      this.from = null; this.mix = null;
      const keep = this.groundBefore;
      this.seatHere();
      this.groundBefore = keep;
      return;
    }
    this.rig.tick(m.sample(s1, this.drawn), this.world(m, s1));
  }
}
