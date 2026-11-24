/** A particle solver for the game's cloth model (the movement preview's cloth
 * packet, `clothPacket.ts`). One tick, in world space:
 * 1. fixed nodes (below `staticNodes`) move to their animated places;
 * 2. simulated nodes: Verlet with the node's damping (against its world speed)
 *    and gravity (−z);
 * 3. vertex attraction: each node moves its attraction share of the way to its
 *    animated place;
 * 4. `passes` rounds of rods (held between minimum and maximum, the correction
 *    shared by `weight0`), capsule colliders (a node is skipped when
 *    `layers & capsule mask` is 0), and last the travel limits around the
 *    CURRENT animated places, so a limit wins over a collider.
 * The compiled data has no iteration count or tick rate; 60 Hz and 4 passes are
 * this preview's choice. Pure functions over typed arrays; no allocation per tick. */

export interface ClothModel {
  /** Node count; nodes `0..staticNodes` follow the animation, the rest are simulated. */
  count: number;
  staticNodes: number;
  /** Per node (simulated ones are read): the game's values (`clothPacket.ts GameValues`). */
  attraction: Float64Array;
  damping: Float64Array;
  gravity: Float64Array;
  travel: Float64Array;
  layers: Int32Array;
  rodA: Int32Array;
  rodB: Int32Array;
  rodMin: Float64Array;
  rodMax: Float64Array;
  /** Share of a rod's correction that moves node a (node b moves the rest). */
  rodWeight0: Float64Array;
}

/** Solver ticks per second and constraint passes per tick. Four passes: more passes hold the
 * rigid rings closer to their shape (ring edges stretched over 10% in a run: 19% of samples at
 * 4 passes, 5% at 32) but do not reduce the rolls of rings about their strands (the twist of a
 * long, thin link barely changes any rod length) or the neighbour kinks they cause, they shift the
 * part averages away from the values the defaults were set by, and they cost a tick each. */
export interface SolverOptions {hz: number; passes: number}
export const SOLVER_OPTIONS: SolverOptions = {hz: 60, passes: 4};

export interface SolverState {x: Float64Array; prev: Float64Array}
export function createState(count: number): SolverState {
  return {x: new Float64Array(count * 3), prev: new Float64Array(count * 3)};
}

/** Capsules of one tick: [ax, ay, az, bx, by, bz, radius a, radius b, layer mask] each, in the world. */
export type CapsuleFrame = Float64Array;
export const CAPSULE_STRIDE = 9;

/** Puts every node on its animated place, moving as the animation moved from
 * `before` (the animated places one tick earlier), so a re-seat during a run
 * keeps the run's speed. */
export function seat(s: SolverState, anim: ArrayLike<number>, before: ArrayLike<number> = anim) {
  for (let k = 0; k < s.x.length; k++) { s.x[k] = anim[k]; s.prev[k] = before[k]; }
}

/** One tick. `anim` holds every node's animated place at the end of the tick, in the world. */
export function step(model: ClothModel, s: SolverState, anim: ArrayLike<number>, capsules: CapsuleFrame, o: SolverOptions = SOLVER_OPTIONS) {
  const dt = 1 / o.hz, x = s.x, prev = s.prev, first = model.staticNodes, last = model.count;
  for (let i = 0; i < first; i++) {
    const k = i * 3;
    prev[k] = x[k]; prev[k + 1] = x[k + 1]; prev[k + 2] = x[k + 2];
    x[k] = anim[k]; x[k + 1] = anim[k + 1]; x[k + 2] = anim[k + 2];
  }
  const g = dt * dt;
  for (let i = first; i < last; i++) {
    const k = i * 3, keep = Math.max(0, 1 - model.damping[i] * dt);
    const vx = (x[k] - prev[k]) * keep, vy = (x[k + 1] - prev[k + 1]) * keep, vz = (x[k + 2] - prev[k + 2]) * keep;
    prev[k] = x[k]; prev[k + 1] = x[k + 1]; prev[k + 2] = x[k + 2];
    x[k] = x[k] + vx; x[k + 1] = x[k + 1] + vy; x[k + 2] = x[k + 2] + vz - model.gravity[i] * g;
  }
  attract(model, x, anim);
  for (let pass = 0; pass < o.passes; pass++) {
    rods(model, x);
    collide(model, x, capsules);
    // Last, so a hold always wins over a collider: a held root that starts inside a
    // capsule ends every tick within its travel limit, as the hold visibly works in game.
    travelLimits(model, x, anim);
  }
}

/** Each simulated node moves its attraction share of the way to its animated place. */
export function attract(model: ClothModel, x: Float64Array, anim: ArrayLike<number>) {
  for (let i = model.staticNodes; i < model.count; i++) {
    const a = model.attraction[i];
    if (a <= 0) continue;
    const k = i * 3;
    x[k] += a * (anim[k] - x[k]); x[k + 1] += a * (anim[k + 1] - x[k + 1]); x[k + 2] += a * (anim[k + 2] - x[k + 2]);
  }
}

/** Rods: the distance between two nodes held within [min, max]. */
export function rods(model: ClothModel, x: Float64Array) {
  const n = model.rodA.length;
  for (let r = 0; r < n; r++) {
    const a = model.rodA[r] * 3, b = model.rodB[r] * 3;
    const dx = x[b] - x[a], dy = x[b + 1] - x[a + 1], dz = x[b + 2] - x[a + 2];
    const l = Math.sqrt(dx * dx + dy * dy + dz * dz);
    if (l < 1e-9) continue;
    let goal: number;
    if (l > model.rodMax[r]) goal = model.rodMax[r]; else if (l < model.rodMin[r]) goal = model.rodMin[r]; else continue;
    const c = (l - goal) / l, w = model.rodWeight0[r], w1 = 1 - w;
    x[a] += dx * c * w; x[a + 1] += dy * c * w; x[a + 2] += dz * c * w;
    x[b] -= dx * c * w1; x[b + 1] -= dy * c * w1; x[b + 2] -= dz * c * w1;
  }
}

/** Travel limits: a node stays within its travel of its current animated place. */
export function travelLimits(model: ClothModel, x: Float64Array, anim: ArrayLike<number>) {
  for (let i = model.staticNodes; i < model.count; i++) {
    const r = model.travel[i];
    if (r <= 0) continue;
    const k = i * 3, dx = x[k] - anim[k], dy = x[k + 1] - anim[k + 1], dz = x[k + 2] - anim[k + 2];
    const l = Math.sqrt(dx * dx + dy * dy + dz * dz);
    if (l > r) { const f = r / l; x[k] = anim[k] + dx * f; x[k + 1] = anim[k + 1] + dy * f; x[k + 2] = anim[k + 2] + dz * f; }
  }
}

/** Tapered capsules: a node inside is pushed out along the nearest direction. */
export function collide(model: ClothModel, x: Float64Array, caps: CapsuleFrame) {
  const count = caps.length / CAPSULE_STRIDE;
  for (let c = 0; c < count; c++) {
    const o = c * CAPSULE_STRIDE, ax = caps[o], ay = caps[o + 1], az = caps[o + 2];
    const ex = caps[o + 3] - ax, ey = caps[o + 4] - ay, ez = caps[o + 5] - az, ra = caps[o + 6], rb = caps[o + 7], mask = caps[o + 8];
    const ll = ex * ex + ey * ey + ez * ez, rmax = Math.max(ra, rb);
    for (let i = model.staticNodes; i < model.count; i++) {
      if ((model.layers[i] & mask) === 0) continue;
      const k = i * 3, px = x[k] - ax, py = x[k + 1] - ay, pz = x[k + 2] - az;
      let t = ll > 1e-12 ? (px * ex + py * ey + pz * ez) / ll : 0;
      t = t < 0 ? 0 : t > 1 ? 1 : t;
      const qx = px - ex * t, qy = py - ey * t, qz = pz - ez * t;
      const d2 = qx * qx + qy * qy + qz * qz;
      if (d2 >= rmax * rmax) continue;
      const r = ra + (rb - ra) * t, d = Math.sqrt(d2);
      if (d >= r || d < 1e-9) continue;
      const f = (r - d) / d;
      x[k] += qx * f; x[k + 1] += qy * f; x[k + 2] += qz * f;
    }
  }
}
