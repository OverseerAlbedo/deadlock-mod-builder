import { describe, expect, it } from 'vitest';
import gameFixture from '../../src-tauri/crates/deadlock-cloth-authoring/tests/fixtures/preview-cloth-game-values.json';
import nodeFixture from '../../src-tauri/crates/deadlock-cloth-authoring/tests/fixtures/preview-cloth-node-values.json';
import { effectiveTuning, gameValues, nodeValues, validPacket, type ClothNodeMotion, type ClothPacket } from './clothPacket';
import type { Tuning } from './motion';

describe('cloth packet values', () => {
  it('maps authored values to the game values as Rust game_values does', () => {
    expect(gameFixture.length).toBeGreaterThanOrEqual(8);
    for (const c of gameFixture) {
      const v = gameValues(c.goal, c.drag, c.gravity, c.travel);
      expect(v.attraction).toBeCloseTo(c.attraction, 12);
      expect(v.damping).toBeCloseTo(c.damping, 9);
      expect(v.gravity).toBeCloseTo(c.game_gravity, 9);
      expect(v.travel).toBeCloseTo(c.game_travel, 12);
    }
  });
  it('applies Fine-tune, caps and attachment limits as Rust node_values does', () => {
    expect(nodeFixture.length).toBeGreaterThanOrEqual(20);
    for (const c of nodeFixture) {
      const effective = effectiveTuning(c.tuning as Record<string, Tuning>);
      const motion = {...c.motion, ...(c.attachment === null ? {} : {attachment: c.attachment})} as ClothNodeMotion;
      const v = nodeValues(motion, motion.kind ? effective[motion.kind] : undefined);
      for (const key of ['attraction', 'damping', 'gravity', 'travel'] as const) expect(v[key], `${key} ${JSON.stringify(c.motion)} ${JSON.stringify(c.tuning)}`).toBeCloseTo(c.expected[key], 9);
    }
  });
  it('keeps a held root at the held travel whatever Swing range says, and leaves untuned kinds alone', () => {
    const held: ClothNodeMotion = {kind: 'hair', goal: 1, drag: 0, gravity: 0, travel: .2, caps: [{cap: 'held'}]};
    expect(nodeValues(held, {swing: 1}).travel).toBeCloseTo(.03, 7);
    expect(nodeValues(held, {swing: -1}).travel).toBeCloseTo(.03, 7);
    // No setting for a kind: no clamping either (the build does not tune it).
    expect(nodeValues({kind: 'tail', goal: 0, drag: 0, gravity: 0, travel: 0}).attraction).toBe(0);
    expect(effectiveTuning({hair: {stiffness: 0, swing: 0}, ear: {swing: .2}, tail: undefined})).toEqual({ear: {swing: .2}});
  });
  it('accepts only a consistent packet of the current version', () => {
    const packet: ClothPacket = {version: 1, source_sha256: 's', proxy_sha256: 'p', bones: [{name: 'a', bind: [0, 0, 0, 1, 0, 0, 0, 1]}], static_nodes: 1, positions: [[0, 0, 0], [0, 0, -1]], node_bones: [0, 0], inv_mass: [0, 1], layers: [65535, 65535], motion: [{kind: 'hair', goal: .5, drag: 0, gravity: 0, travel: 0}], rods: {a: [0], b: [1], min: [1], max: [1], weight0: [0]}, colliders: [], bases: []};
    expect(validPacket(packet)).toBe(true);
    expect(validPacket({...packet, version: 2})).toBe(false);
    expect(validPacket({...packet, motion: []})).toBe(false);
    expect(validPacket({...packet, rods: {...packet.rods, b: [5]}})).toBe(false);
    expect(validPacket(null)).toBe(false);
  });
});
