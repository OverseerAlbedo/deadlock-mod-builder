import { describe, expect, it } from 'vitest';
import { CAPSULE_STRIDE, collide, createState, rods, seat, step, travelLimits, type ClothModel } from './clothSolver';

/** A model of `count` nodes, the first `fixed` of them following the animation. */
function model(count: number, fixed: number, rodList: [number, number, number, number, number][] = [], values: Partial<Record<'attraction' | 'damping' | 'gravity' | 'travel', number>> = {}, layers = 0xffff): ClothModel {
  const fill = (v = 0) => new Float64Array(count).map((_, i) => i < fixed ? 0 : v);
  return {
    count, staticNodes: fixed, attraction: fill(values.attraction), damping: fill(values.damping), gravity: fill(values.gravity), travel: fill(values.travel),
    layers: new Int32Array(count).fill(layers),
    rodA: Int32Array.from(rodList.map(r => r[0])), rodB: Int32Array.from(rodList.map(r => r[1])),
    rodMin: Float64Array.from(rodList.map(r => r[2])), rodMax: Float64Array.from(rodList.map(r => r[3])), rodWeight0: Float64Array.from(rodList.map(r => r[4])),
  };
}
const none = new Float64Array(0);
const dist = (x: ArrayLike<number>, a: number, b: number) => Math.hypot(x[b * 3] - x[a * 3], x[b * 3 + 1] - x[a * 3 + 1], x[b * 3 + 2] - x[a * 3 + 2]);

describe('cloth solver', () => {
  it('lets a hanging chain fall to its rest length and stop', () => {
    // A fixed top and three links of 1 in, starting out sideways.
    const m = model(4, 1, [[0, 1, 1, 1, 0], [1, 2, 1, 1, .5], [2, 3, 1, 1, .5]], {damping: 6, gravity: 360});
    const anim = Float64Array.from([0, 0, 10, 1, 0, 10, 2, 0, 10, 3, 0, 10]);
    const s = createState(4);
    seat(s, anim);
    for (let k = 0; k < 600; k++) step(m, s, anim, none);
    // Four passes leave rigid links stretched by gravity by a few percent (each
    // tick's fall of 0.1 in is not fully undone), as a solver with few passes does.
    for (let i = 1; i < 4; i++) {
      expect(dist(s.x, i - 1, i)).toBeGreaterThan(.999);
      expect(dist(s.x, i - 1, i)).toBeLessThan(1.04);
      expect(Math.abs(s.x[i * 3])).toBeLessThan(.01);
      expect(s.x[i * 3 + 2]).toBeLessThan(s.x[(i - 1) * 3 + 2]);
      expect(Math.hypot(s.x[i * 3] - s.prev[i * 3], s.x[i * 3 + 2] - s.prev[i * 3 + 2])).toBeLessThan(1e-4);
    }
  });
  it('holds a rod between its minimum and maximum, moving only the free end of a fixed rod', () => {
    const m = model(2, 1, [[0, 1, .5, 1, 0]]);
    const x = Float64Array.from([0, 0, 10, 0, 0, 7]);
    rods(m, x);
    expect([...x]).toEqual([0, 0, 10, 0, 0, 9]);
    x.set([0, 0, 9.8], 3);
    rods(m, x);
    expect(x[5]).toBeCloseTo(9.5, 12);
    x.set([0, 0, 9.3], 3);
    rods(m, x);
    expect(x[5]).toBeCloseTo(9.3, 12);
  });
  it('shares a rod correction by its weight', () => {
    const m = model(3, 1, [[1, 2, 1, 1, .25]]);
    const x = Float64Array.from([0, 0, 0, 0, 0, 0, 3, 0, 0]);
    rods(m, x);
    expect(x[3]).toBeCloseTo(.5, 12);
    expect(x[6]).toBeCloseTo(1.5, 12);
  });
  it('keeps a held node inside its travel limit at the end of every tick while a capsule overlaps it', () => {
    // A held node (0.03 in) whose animated place lies inside a capsule: the capsule pushes it
    // out in every pass, and the limit, applied last, brings it back.
    const m = model(2, 1, [], {travel: .03, attraction: .3});
    const s = createState(2), anim = Float64Array.from([0, 0, 10, 0, 0, 9]);
    seat(s, anim);
    // One capsule of radius 1 around the node's animated place, for every layer.
    const capsule = new Float64Array(CAPSULE_STRIDE);
    capsule.set([0, 0, 9.2, 0, 0, 8.2, 1, 1, 0xffff]);
    for (let k = 0; k < 120; k++) {
      step(m, s, anim, capsule);
      expect(Math.hypot(s.x[3] - anim[3], s.x[4] - anim[4], s.x[5] - anim[5])).toBeLessThanOrEqual(.03 + 1e-12);
    }
  });
  it('keeps a node within its travel limit of its current animated place while the anchor moves', () => {
    const m = model(2, 1, [], {travel: .5});
    const s = createState(2), anim = Float64Array.from([0, 0, 10, 0, 0, 9]);
    seat(s, anim);
    for (let k = 1; k <= 120; k++) {
      const x = 5 * k; // 300 in/s at 60 Hz
      anim.set([x, 0, 10, x, 0, 9]);
      step(m, s, anim, none);
      expect(dist(Float64Array.from([...anim.subarray(3), ...s.x.subarray(3)]), 0, 1)).toBeLessThanOrEqual(.5 + 1e-9);
    }
    const m2 = model(2, 1, [], {travel: .5}), x = Float64Array.from([0, 0, 0, 3, 0, 0]);
    travelLimits(m2, x, Float64Array.from([0, 0, 0, 0, 0, 0]));
    expect(x[3]).toBeCloseTo(.5, 12);
  });
  it('pushes a node out of a capsule, and lets a node off the capsule layers pass through', () => {
    const caps = Float64Array.from([0, 0, 0, 0, 0, 20, 2, 2, 15]);
    expect(caps.length).toBe(CAPSULE_STRIDE);
    const x = Float64Array.from([0, 0, 10, .5, 0, 9]);
    collide(model(2, 1), x, caps);
    expect(Math.hypot(x[3], x[4])).toBeCloseTo(2, 12);
    const masked = Float64Array.from([0, 0, 10, .5, 0, 9]);
    collide(model(2, 1, [], {}, 0xfff0), masked, caps);
    expect(masked[3]).toBe(.5);
  });
  it('pulls a node its attraction share of the way to its animated place each tick', () => {
    const a = .36, m = model(2, 1, [], {attraction: a});
    const s = createState(2), anim = Float64Array.from([0, 0, 0, 0, 0, 0]);
    seat(s, Float64Array.from([0, 0, 0, 1, 0, 0]));
    step(m, s, anim, none);
    expect(s.x[3]).toBeCloseTo(1 - a, 12);
    // The pull's move becomes speed (Verlet): the next tick moves on by it, then pulls again.
    step(m, s, anim, none);
    expect(s.x[3]).toBeCloseTo((1 - 2 * a) * (1 - a), 12);
  });
  it('carries a part along at a steady world speed without leaving it behind', () => {
    const m = model(2, 1, [], {attraction: .2});
    const s = createState(2), anim = new Float64Array(6);
    anim.set([0, 0, 10, 0, 0, 9]);
    seat(s, anim, Float64Array.from([-5, 0, 10, -5, 0, 9]));
    for (let k = 1; k <= 600; k++) { const x = 5 * k; anim.set([x, 0, 10, x, 0, 9]); step(m, s, anim, none); }
    expect(anim[3] - s.x[3]).toBeCloseTo(0, 6);
  });
});
