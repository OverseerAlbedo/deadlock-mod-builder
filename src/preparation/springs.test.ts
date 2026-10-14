import { describe, expect, it } from 'vitest';
import { partSprings } from './springs';

describe('Source rig springs', () => {
  it('averages each moving part over its bones that have spring settings', () => {
    const groups = [
      {root: 'hair_0', source_anchor: 'head', target_anchor: 'head', source_bones: ['hair_0', 'hair_1', 'hair_2'], weighted_bones: []},
      {root: 'skirt_0', source_anchor: 'pelvis', target_anchor: 'pelvis', source_bones: ['skirt_0'], weighted_bones: []},
    ];
    const springs = {hair_0: {source: 'vrm' as const, goal: .45, drag: .2, gravity: 0}, hair_1: {source: 'vrm' as const, goal: .25, drag: .3, gravity: .5}};
    expect(partSprings(groups, springs)).toEqual({hair_0: {source: 'vrm', goal: .35, drag: .25, gravity: .25}});
    expect(partSprings(groups, undefined)).toEqual({});
  });
});
