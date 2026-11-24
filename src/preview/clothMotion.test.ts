import { describe, expect, it } from 'vitest';
import { EASE_SECONDS, FALLBACK_SPEEDS, FALL_TOP_SPEED, JUMP_GRAVITY, MOTION_LABELS, clothJumpArc, clothMotion, loopCuts, speedsOf, travelInLoop } from './clothMotion';
import { RUN_SPEED, sampleSkins, type MotionClip, type MotionHeader, type MotionPacket } from './motion';

const clip = (name: string, frames: number, segments = [frames]): MotionClip => ({name, sources: [], fps: 60, frames, segments, offset: 0});
const speeds = {forward: 386, back: 323, strafe: 323};

describe('cloth preview motions', () => {
  it('reads the hero speeds from the packet and falls back to the earlier constant', () => {
    const header: MotionHeader = {bones: [], clips: [], speeds};
    expect(speedsOf(header)).toEqual({speeds, fromGame: true});
    expect(speedsOf({bones: [], clips: []})).toEqual({speeds: FALLBACK_SPEEDS, fromGame: false});
    expect(speedsOf({bones: [], clips: [], speeds: {forward: Number.NaN, back: 1, strafe: 1}}).fromGame).toBe(false);
    expect(FALLBACK_SPEEDS.forward).toBe(RUN_SPEED);
  });
  it('lists the motions in the toolbar order', () => {
    expect(MOTION_LABELS.map(([name]) => name)).toEqual(['idle', 'run', 'back', 'left', 'right', 'stop', 'jump', 'fall']);
  });
  it('moves each motion its own way at the hero speeds', () => {
    expect(travelInLoop(clip('run', 32), 1, speeds)).toEqual([386, 0, 0]);
    expect(travelInLoop(clip('back', 32), 1, speeds)).toEqual([-323, 0, 0]);
    expect(travelInLoop(clip('left', 32), 1, speeds)).toEqual([0, 323, 0]);
    expect(travelInLoop(clip('right', 32), 1, speeds)).toEqual([0, -323, 0]);
    expect(travelInLoop(clip('idle', 32), 1, speeds)).toEqual([0, 0, 0]);
  });
  it('jumps for 0.65 s on a parabola under the jump gravity, landing as the landing clip starts', () => {
    const jump = clip('jump', 152, [50, 34, 26, 42]);
    const [a, b] = clothJumpArc(jump)!;
    expect(b - a).toBeCloseTo(.647, 3);
    expect(b).toBeCloseTo((50 + 34) / 60, 9);
    expect(travelInLoop(jump, a, speeds)[2]).toBe(0);
    expect(travelInLoop(jump, b, speeds)[2]).toBe(0);
    expect(travelInLoop(jump, (a + b) / 2, speeds)[2]).toBeCloseTo(JUMP_GRAVITY * (b - a) ** 2 / 8, 9);
  });
  it('falls with the jump gravity up to its top speed, and eases a stop out over the easing time', () => {
    const fall = clip('fall', 180);
    expect(travelInLoop(fall, 1, speeds)[2]).toBeCloseTo(-.5 * JUMP_GRAVITY, 9);
    const at = (u: number) => travelInLoop(fall, u, speeds)[2];
    expect((at(2.9) - at(2.8)) / .1).toBeCloseTo(-FALL_TOP_SPEED, 6);
    const stop = clip('stop', 170, [60, 110]), x = (u: number) => travelInLoop(stop, u, speeds)[0];
    expect((x(1) - x(.99)) / .01).toBeCloseTo(386, 6);
    expect((x(1 + EASE_SECONDS / 2 + .005) - x(1 + EASE_SECONDS / 2 - .005)) / .01).toBeCloseTo(193, 0);
    expect(x(2) - x(1 + EASE_SECONDS)).toBeCloseTo(0, 9);
    expect(x(1 + EASE_SECONDS) - x(1)).toBeCloseTo(386 * EASE_SECONDS / 2, 9);
  });
  it('restarts falls as cuts and keeps a smooth loop continuous', () => {
    const bones = [{name: 'body', parent: -1, position: [0, 0, 40] as [number, number, number]}];
    const smooth = new Float32Array(32 * 12);
    for (let f = 0; f < 32; f++) smooth.set([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, Math.sin(2 * Math.PI * f / 32)], f * 12);
    const packet: MotionPacket = {header: {bones, clips: [clip('run', 32), clip('fall', 32)]}, data: smooth};
    const sample = (c: MotionClip, t: number, out: Float32Array) => sampleSkins(packet, c, t, out);
    const run = clothMotion(packet, packet.header.clips[0], speeds, sample), fall = clothMotion(packet, packet.header.clips[1], speeds, sample);
    const length = 32 / 60;
    expect(run.cut(length - .01, length + .01)).toBe(false);
    expect(run.travel(length - .01, length + .01)[0]).toBeCloseTo(386 * .02, 9);
    expect(fall.cut(length - .01, length + .01)).toBe(true);
    expect(fall.floor).toBe(false);
    expect(loopCuts(packet, packet.header.clips[0])).toBe(false);
  });
  it('gives a jump its height apart from its travel', () => {
    const bones = [{name: 'body', parent: -1, position: [0, 0, 40] as [number, number, number]}];
    const jump = clip('jump', 152, [50, 34, 26, 42]);
    const packet: MotionPacket = {header: {bones, clips: [jump]}, data: new Float32Array(152 * 12)};
    const m = clothMotion(packet, jump, speeds, (c, t, out) => sampleSkins(packet, c, t, out));
    const [a, b] = clothJumpArc(jump)!;
    expect(m.travel(a, (a + b) / 2)).toEqual([0, 0, 0]);
    expect(m.height((a + b) / 2)).toBeCloseTo(travelInLoop(jump, (a + b) / 2, speeds)[2], 9);
    expect(m.height(b + .1)).toBe(0);
    expect(m.floor).toBe(true);
  });
  it('tells a loop that wraps smoothly from one that jumps at its wrap', () => {
    const frames = 30, header: MotionHeader = {bones: [{name: 'body', parent: -1, position: [0, 0, 40]}], clips: []};
    const make = (lift: (f: number) => number): MotionPacket => {
      const data = new Float32Array(frames * 12);
      for (let f = 0; f < frames; f++) data.set([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, lift(f)], f * 12);
      return {header: {...header, clips: [{name: 'idle', sources: [], fps: 30, frames, segments: [frames], offset: 0}]}, data};
    };
    const smooth = make(f => 2 * Math.sin(2 * Math.PI * f / frames)), cut = make(f => f * .2);
    expect(loopCuts(smooth, smooth.header.clips[0])).toBe(false);
    expect(loopCuts(cut, cut.header.clips[0])).toBe(true);
  });
});
