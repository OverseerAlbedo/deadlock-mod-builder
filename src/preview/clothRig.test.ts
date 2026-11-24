import { existsSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { ClothPlayback, ClothRig, placeBase, switchMovingParts } from './clothRig';
import type { ClothMotion } from './clothMotion';
import type { ClothPacket } from './clothPacket';
import { decodeMotion, sampleSkins, type MotionHeader } from './motion';

/** A body bone with three fixed nodes and one moving bone below it carried by a
 * ring of three nodes; its child `tip` is not simulated. The ring's X edge is
 * skewed against its Y edge, so only y-first placement gives the bind. */
function sample(): {packet: ClothPacket; header: MotionHeader} {
  const positions = [[1, 0, 0], [-.5, .87, 0], [-.5, -.87, 0], [1, .2, -1], [0, .5, -1], [0, -.5, -1]];
  const rods = {a: [] as number[], b: [] as number[], min: [] as number[], max: [] as number[], weight0: [] as number[]};
  const rod = (a: number, b: number, w: number) => { const l = Math.hypot(...[0, 1, 2].map(k => positions[b][k] - positions[a][k])); rods.a.push(a); rods.b.push(b); rods.min.push(l); rods.max.push(l); rods.weight0.push(w); };
  for (const s of [0, 1, 2]) for (const m of [3, 4, 5]) rod(s, m, 0);
  rod(3, 4, .5); rod(4, 5, .5); rod(3, 5, .5);
  const motion = {kind: 'hair', goal: .5, drag: 0, gravity: 1, travel: 0};
  return {
    packet: {
      version: 1, source_sha256: 's', proxy_sha256: 'p',
      bones: [{name: 'body', bind: [0, 0, 0, 1, 0, 0, 0, 1]}, {name: 'strand', bind: [0, 0, -1, 1, 0, 0, 0, 1]}],
      static_nodes: 3, positions, node_bones: [0, 0, 0, 1, 1, 1], inv_mass: [0, 0, 0, 1, 1, 1], layers: [65535, 65535, 65535, 65535, 65535, 65535],
      motion: [motion, motion, motion], rods, colliders: [],
      bases: [{bone: 1, target: 3, offset: [1, .2, 0], x: [4, 3], y: [5, 4], adjust: [0, 0, 0, 1]}],
    },
    header: {bones: [{name: 'body', parent: -1, position: [0, 0, 0]}, {name: 'strand', parent: 0, position: [0, 0, -1]}, {name: 'tip', parent: 1, position: [0, 0, -2]}], clips: []},
  };
}
const identity = (bones: number) => { const s = new Float32Array(bones * 12); for (let i = 0; i < bones; i++) { s[i * 12] = 1; s[i * 12 + 5] = 1; s[i * 12 + 10] = 1; } return s; };
/** A turn of `a` radians about x (row-major 3x4). */
const turnX = (a: number) => [1, 0, 0, 0, 0, Math.cos(a), -Math.sin(a), 0, 0, Math.sin(a), Math.cos(a), 0];
const rig = (p = sample()) => { const r = ClothRig.create(p.packet, p.header); if (typeof r === 'string') throw new Error(r); return r; };

describe('cloth rig', () => {
  it('places a bone y axis first: the bind comes back at rest even when the ring is skewed', () => {
    const out = new Float64Array(12);
    const p = sample().packet.positions.flat();
    expect(placeBase(p, 4, 3, 5, 4, 3, [1, 0, 0, 0, 1, 0, 0, 0, 1], [1, .2, 0], out)).toBe(true);
    expect([...out].map(v => +v.toFixed(12) + 0)).toEqual([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, -1]);
    const r = rig(), skins = identity(3);
    r.seat(skins, [0, 0, 0]);
    const pose = r.pose(0, skins, new Float32Array(36));
    expect([...pose].map(v => +v.toFixed(6) + 0)).toEqual([...skins]);
  });
  it('turns a moving bone with its nodes, and its unsimulated child with it', () => {
    const r = rig(), skins = identity(3), turn = turnX(Math.PI / 2);
    // The whole body turned a quarter about x: every bone follows.
    for (let b = 0; b < 3; b++) skins.set(turn, b * 12);
    r.seat(skins, [0, 0, 0]);
    const pose = r.pose(0, skins, new Float32Array(36));
    for (let k = 0; k < 36; k++) expect(pose[k]).toBeCloseTo(skins[k], 6);
    // Only the ring swung out (a free ring placed by hand, at rest there): the
    // strand and the tip turn with it, the body does not.
    const free = sample();
    free.packet.rods = {a: [3, 4, 3], b: [4, 5, 5], min: [0, 0, 0], max: [9, 9, 9], weight0: [.5, .5, .5]};
    free.packet.motion = free.packet.motion.map(m => ({...m, goal: 0, gravity: 0}));
    const loose = rig(free), x = loose.state.x, prev = loose.state.prev, rest = free.packet.positions;
    const flat = identity(3);
    loose.seat(flat, [0, 0, 0]);
    for (const i of [3, 4, 5]) { const q = rest[i]; const t = turnX(-.3); for (let k = 0; k < 3; k++) x[i * 3 + k] = prev[i * 3 + k] = t[k * 4] * q[0] + t[k * 4 + 1] * q[1] + t[k * 4 + 2] * q[2]; }
    loose.tick(flat, [0, 0, 0]);
    const swung = loose.pose(1, flat, new Float32Array(36));
    expect([...swung.subarray(0, 12)].map(v => +v.toFixed(6) + 0)).toEqual([...flat.subarray(0, 12)]);
    expect(swung[12 + 5]).toBeCloseTo(Math.cos(.3), 6);
    expect(swung[12 + 6]).toBeCloseTo(Math.sin(.3), 6);
    for (let k = 0; k < 12; k++) expect(swung[24 + k]).toBeCloseTo(swung[12 + k], 6);
  });
  it('keeps a ring fastened to the body where the body puts it', () => {
    const r = rig(), flat = identity(3);
    r.seat(flat, [0, 0, 0]);
    r.tick(flat, [0, 0, 0]);
    const held = r.pose(1, flat, new Float32Array(36));
    // Gravity pulls a fastened ring by a hair only (four passes of rigid rods).
    for (let k = 0; k < 36; k++) expect(Math.abs(held[k] - flat[k])).toBeLessThan(.01);
  });
  it('rewrites the per-node values on a slider change without moving or resetting the simulation', () => {
    const r = rig(), skins = identity(3);
    r.seat(skins, [0, 0, 0]);
    for (let k = 0; k < 30; k++) { skins.set(turnX(.02 * k), 0); r.tick(skins, [0, 0, 0]); }
    const before = Float64Array.from(r.state.x), attraction = r.model.attraction[3];
    r.setTuning({hair: {stiffness: 1}});
    expect([...r.state.x]).toEqual([...before]);
    expect(r.model.attraction[3]).toBeGreaterThan(attraction);
    expect(r.model.attraction[3]).toBeCloseTo(.5, 9);
    r.tick(skins, [0, 0, 0]);
    let moved = 0;
    for (let k = 9; k < 18; k++) moved = Math.max(moved, Math.abs(r.state.x[k] - before[k]));
    expect(moved).toBeLessThan(.2);
    // The same settings again change nothing; zero settings are the automatic values.
    r.setTuning({hair: {stiffness: 0}});
    expect(r.model.attraction[3]).toBeCloseTo(attraction, 12);
  });
  /** A test motion: a pose function and a straight travel at `speed` (in/s along x). */
  const motionOf = (name: string, pose: (t: number, out: Float32Array) => void, speed = 0, cut: (t0: number, t1: number) => boolean = () => false): ClothMotion => ({
    name, sample: (t, out) => { out.set(identity(3)); pose(t, out); return out; }, travel: (t0, t1) => [speed * (t1 - t0), 0, 0], cut, height: () => 0, floor: true,
  });
  it('ticks at its own rate whatever the frame rate, and re-seats after a long pause and at a cut', () => {
    const run = (frame: number, seconds: number) => {
      const r = rig();
      const play = new ClothPlayback(r, motionOf('run', (t, out) => out.set(turnX(.3 * Math.sin(t * 6)), 0), 300), 3);
      play.reset();
      let shown = play.advance(0, 1, true);
      for (let k = 0; k < Math.round(seconds / frame); k++) shown = play.advance(frame, 1, true);
      return {r, play, shown};
    };
    const a = run(1 / 60, 1), b = run(1 / 144, 1);
    expect(a.play.tickTime).toBeCloseTo(b.play.tickTime, 9);
    for (let k = 0; k < 18; k++) expect(a.r.state.x[k] - b.r.state.x[k]).toBeCloseTo(0, 9);
    expect(b.shown.alpha).toBeGreaterThanOrEqual(0);
    expect(b.shown.alpha).toBeLessThanOrEqual(1);
    // The run carries the nodes along in the world; the drawn nodes stay with the character.
    expect(a.shown.offset[0]).toBeGreaterThan(250);
    expect(Math.abs(a.r.nodes(a.shown.alpha, identity(3))[9] - 1)).toBeLessThan(3);
    // A long pause starts again from the animated pose.
    a.play.advance(2, 1, true);
    const pose = a.play.advance(0, 1, true).skins;
    const anim = a.r.places(pose, [0, 0, 0], new Float64Array(18)), drawn = a.r.nodes(1, pose);
    for (let k = 9; k < 18; k++) expect(Math.abs(drawn[k] - anim[k])).toBeLessThan(.5);
  });
  it('seats the moving parts on the drawn pose at once when Moving parts is switched while paused', () => {
    const r = rig();
    const play = new ClothPlayback(r, motionOf('run', (t, out) => out.set(turnX(.6 * Math.sin(t * 9)), 0), 300), 3);
    play.reset();
    for (let k = 0; k < 47; k++) play.advance(1 / 57, 1, true);
    // Paused mid-swing: the drawn strand is off its animated pose.
    const paused = play.advance(0, 1, false), out = new Float32Array(36);
    const gap = (pose: Float32Array, skins: Float32Array) => Math.max(...[...pose.subarray(12, 36)].map((v, k) => Math.abs(v - skins[12 + k])));
    expect(gap(r.pose(paused.alpha, paused.skins, out), paused.skins)).toBeGreaterThan(.05);
    const tick = play.tickTime, swing = {resets: 0, reset() { this.resets++; }}, drawn = {current: true};
    switchMovingParts(play, swing, drawn);
    expect([drawn.current, swing.resets, play.tickTime]).toEqual([false, 1, tick]);
    // The same frame again: the moving bones sit on the animated pose; the clock and the travel did not move.
    const again = play.advance(0, 1, false);
    expect([...again.skins]).toEqual([...paused.skins]);
    expect(again.offset).toEqual(paused.offset);
    expect(gap(r.pose(again.alpha, again.skins, out), again.skins)).toBeLessThan(1e-4);
    // Playing on from there keeps the run's speed (no kick from the seat).
    const next = play.advance(1 / 57, 1, true);
    expect(next.offset[0] - again.offset[0]).toBeCloseTo(300 / 57, 0);
  });
  it('re-seats at a cut instead of feeding the jump to the simulation, keeping the world speed', () => {
    const r = rig();
    let jumped = false;
    const play = new ClothPlayback(r, motionOf('stop', (_t, out) => { if (jumped) { out.set(turnX(2), 0); out.set(turnX(2), 12); } }, 300, () => jumped), 3);
    play.reset();
    for (let k = 0; k < 30; k++) play.advance(1 / 60, 1, true);
    jumped = true;
    play.advance(1 / 60, 1, true);
    const turned = identity(3); turned.set(turnX(2), 0); turned.set(turnX(2), 12);
    const placed = r.places(turned, [0, 0, 0], new Float64Array(18)), drawn = r.nodes(1, turned);
    for (let k = 9; k < 18; k++) expect(drawn[k]).toBeCloseTo(placed[k], 6);
    for (const i of [3, 4, 5]) expect(r.state.x[i * 3] - r.state.prev[i * 3]).toBeCloseTo(5, 9);
  });
  it('changes motion over the easing time: the speed and the pose blend, never in one step', () => {
    const r = rig();
    const stand = motionOf('idle', () => {}, 0), runs = motionOf('run', (_t, out) => out.set(turnX(.8), 12), 360);
    const play = new ClothPlayback(r, stand, 3);
    play.reset();
    for (let k = 0; k < 30; k++) play.advance(1 / 60, 1, true);
    play.play(runs);
    const speeds: number[] = [], turns: number[] = [];
    let before = play.advance(0, 1, true).offset[0];
    for (let k = 0; k < 40; k++) {
      const f = play.advance(1 / 60, 1, true);
      speeds.push((f.offset[0] - before) * 60); before = f.offset[0];
      turns.push(Math.atan2(f.skins[12 + 9], f.skins[12 + 10]));
    }
    // From standing to the run's speed over about 0.3 s, each frame a small change.
    expect(speeds[0]).toBeLessThan(40);
    expect(speeds[speeds.length - 1]).toBeCloseTo(360, 6);
    for (let k = 1; k < speeds.length; k++) expect(Math.abs(speeds[k] - speeds[k - 1])).toBeLessThan(40);
    const reached = speeds.findIndex(s => s > 359);
    expect(reached / 60).toBeGreaterThan(.25);
    expect(reached / 60).toBeLessThan(.36);
    for (let k = 1; k < turns.length; k++) expect(Math.abs(turns[k] - turns[k - 1])).toBeLessThan(.08);
    // The same motion again changes nothing.
    play.play(runs);
    expect(play.motion).toBe(runs);
  });
  /** Frames of `seconds` at 60 Hz; returns the drawn frames. */
  const frames = (play: ClothPlayback, seconds: number) => Array.from({length: Math.round(seconds * 60)}, () => play.advance(1 / 60, 1, true));
  it('brings a jump left in the air and a fall back to the ground in view', () => {
    const r = rig(), stand = motionOf('idle', () => {});
    const jump: ClothMotion = {...motionOf('jump', () => {}), height: t => t < 1 ? 50 * Math.sin(Math.PI * t) : 0};
    const fall: ClothMotion = {...motionOf('fall', () => {}), travel: (t0, t1) => [0, 0, -1200 * (t1 * t1 - t0 * t0) / 2], floor: false};
    const play = new ClothPlayback(r, jump, 3);
    play.reset();
    const up = frames(play, .5).pop()!;
    expect(up.offset[2]).toBeGreaterThan(40);
    play.play(stand);
    const after = frames(play, .5);
    expect(after.pop()!.offset[2]).toBeCloseTo(0, 9);
    for (let k = 1; k < after.length; k++) expect(Math.abs(after[k].offset[2] - after[k - 1].offset[2])).toBeLessThan(6);
    // A long fall goes far down for the simulation; the view keeps the character at the ground's height.
    play.play(fall);
    const falling = frames(play, 2);
    expect(falling.pop()!.floor).toBe(false);
    for (const f of falling.slice(30)) expect(f.offset[2]).toBe(0);
    play.play(stand);
    const back = frames(play, .5).pop()!;
    expect(back.offset[2]).toBe(0);
    expect(back.floor).toBe(true);
  });
  it('blends a change made during a change from what plays at that moment, never in one step', () => {
    const r = rig();
    const pose = (a: number) => (_t: number, out: Float32Array) => out.set(turnX(a), 12);
    const stand = motionOf('idle', pose(0)), run = motionOf('run', pose(.8), 360), back = motionOf('back', pose(-.8), -300);
    const play = new ClothPlayback(r, stand, 3);
    play.reset();
    frames(play, .5);
    play.play(run);
    const half = frames(play, .15);
    play.play(back);
    const next = frames(play, .5), all = [...half, ...next];
    const turn = (f: {skins: Float32Array}) => Math.atan2(f.skins[12 + 9], f.skins[12 + 10]);
    for (let k = 1; k < all.length; k++) {
      expect(Math.abs(turn(all[k]) - turn(all[k - 1]))).toBeLessThan(.1);
      expect(Math.abs((all[k].offset[0] - all[k - 1].offset[0]) - (all[k - 1].offset[0] - (all[k - 2] ?? all[k - 1]).offset[0]))).toBeLessThan(2);
    }
    expect(turn(all[all.length - 1])).toBeCloseTo(-.8, 6);
  });
  it('shows a new motion at once while paused', () => {
    const r = rig();
    const play = new ClothPlayback(r, motionOf('idle', () => {}), 3);
    play.reset();
    frames(play, .2);
    play.play(motionOf('run', (_t, out) => out.set(turnX(.8), 12), 360), true);
    const f = play.advance(0, 1, false);
    expect(Math.atan2(f.skins[12 + 9], f.skins[12 + 10])).toBeCloseTo(.8, 6);
  });
  it('refuses a packet whose bones the preview skeleton does not have', () => {
    const {packet, header} = sample();
    expect(ClothRig.create(packet, {...header, bones: header.bones.filter(b => b.name !== 'strand')})).toBe('bones');
  });
  // A real packet and a movement packet of the same model (set both to check):
  // at rest every bone placed from its nodes is where the clip puts it.
  const real = process.env.DMB_PREVIEW_CLOTH_PACKET, clips = process.env.DMB_PREVIEW_MOTION;
  it.skipIf(!real || !clips || !existsSync(real) || !existsSync(clips))('places every moving bone of a real packet on its bind at rest', () => {
    const buf = readFileSync(clips!), bytes = new ArrayBuffer(buf.length);
    new Uint8Array(bytes).set(buf);
    const motion = decodeMotion(bytes);
    const r = ClothRig.create(JSON.parse(readFileSync(real!, 'utf8')), motion.header);
    if (typeof r === 'string') throw new Error(r);
    const skins = identity(motion.header.bones.length);
    r.seat(skins, [0, 0, 0]);
    const pose = r.pose(0, skins, new Float32Array(skins.length));
    for (let k = 0; k < skins.length; k++) expect(Math.abs(pose[k] - skins[k])).toBeLessThan(k % 4 === 3 ? .005 : 1e-4);
    // Through a run, the fixed nodes stay exactly on their animated places.
    const clip = motion.header.clips.find(c => c.name === 'run') ?? motion.header.clips[0];
    const frame = new Float32Array(skins.length);
    for (let k = 0; k < 120; k++) {
      r.tick(sampleSkins(motion, clip, k / 60, frame), [5 * k, 0, 0]);
      for (let i = 0; i < r.model.staticNodes * 3; i++) expect(r.state.x[i]).toBe(r.anim[i]);
    }
  });
});
