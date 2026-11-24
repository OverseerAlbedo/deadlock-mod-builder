import { describe, expect, it } from 'vitest';
import type { ResourceReplacement } from '../models';
import { madeRow, withoutMade } from './madeRows';
import { cleanTaken, latestTakenStart, takenOf, takenRow, takenSetRows, takenSetUnchanged, withCompiledFile, withTaken, withoutTaken } from './takenRows';
import type { ClipSet } from './takenTypes';

const HERO = 'models/heroes_staging/other/other.vmdl';
const clip = (name: string) => `models/heroes_staging/other/clips/${name}.vnmclip_c`;
const target = (name: string) => `models/heroes_wip/mine/clips/${name}.vnmclip_c`;

describe('taken rows', () => {
  it('writes the saved format exactly as Rust saves it', () => {
    const row = takenRow(target('dash_ground'), HERO, clip('dash_ground'));
    expect(row).toEqual({ enabled: true, target: target('dash_ground'), allow_shared: false, taken: { hero: HERO, clip: clip('dash_ground'), timing: 'stretched', arms: 'source' } });
    expect(Object.keys(row.taken!)).toEqual(['hero', 'clip', 'timing', 'arms']);
    expect(takenOf(row)).toEqual(row.taken);
    expect(takenOf({ target: 'x' })).toBeNull();
    // Own speed with a start: the start sits between timing and arms; unknown keys are dropped.
    const own = cleanTaken({ hero: HERO, clip: clip('x'), timing: 'own_speed', start_seconds: 0.123456, arms: 'own', speed: 2 } as never);
    expect(own).toEqual({ hero: HERO, clip: clip('x'), timing: 'own_speed', start_seconds: 0.123, arms: 'own' });
    expect(Object.keys(own)).toEqual(['hero', 'clip', 'timing', 'start_seconds', 'arms']);
    // A start without own speed is not kept; limits hold.
    expect(cleanTaken({ hero: HERO, clip: clip('x'), start_seconds: 1 })).toEqual({ hero: HERO, clip: clip('x'), timing: 'stretched', arms: 'source' });
    expect(cleanTaken({ hero: HERO, clip: clip('x'), timing: 'own_speed', start_seconds: 9999 }).start_seconds).toBe(3600);
    expect(cleanTaken({ hero: HERO, clip: clip('x'), timing: 'own_speed', start_seconds: -1 }).start_seconds).toBe(0);
  });

  it('changes one option and keeps the rest of the row', () => {
    const row = { ...takenRow(target('run'), HERO, clip('run')), enabled: false };
    expect(withTaken(row, { arms: 'own' })).toEqual({ ...row, taken: { ...row.taken!, arms: 'own' } });
    const part = withTaken(row, { timing: 'own_speed', start_seconds: 0.25 });
    expect(part.taken).toEqual({ hero: HERO, clip: clip('run'), timing: 'own_speed', start_seconds: 0.25, arms: 'source' });
    expect(withTaken(part, { start_seconds: null }).taken).toEqual({ hero: HERO, clip: clip('run'), timing: 'own_speed', arms: 'source' });
    expect(withTaken(part, { timing: 'stretched' }).taken).toEqual({ hero: HERO, clip: clip('run'), timing: 'stretched', arms: 'source' });
    const plain: ResourceReplacement = { enabled: true, source: 'C:\\clips\\run.vnmclip_c', target: target('run') };
    expect(withTaken(plain, { arms: 'own' })).toBe(plain);
  });

  it('choosing another hero keeps the timing and the arms, and starts the part again', () => {
    const before = withTaken(takenRow(target('run'), HERO, clip('run')), { timing: 'own_speed', start_seconds: 0.4, arms: 'own' });
    const next = takenRow(target('run'), 'models/heroes_wip/third/third.vmdl', 'models/heroes_wip/third/clips/run.vnmclip_c', before);
    expect(next.taken).toEqual({ hero: 'models/heroes_wip/third/third.vmdl', clip: 'models/heroes_wip/third/clips/run.vnmclip_c', timing: 'own_speed', arms: 'own' });
  });

  it('the three ways exclude each other', () => {
    // A compiled file or a made row was there: taking it from another hero leaves no source and no made.
    const made: ResourceReplacement = { enabled: true, source: 'C:\\anim\\moves.blend', target: target('run'), allow_shared: false, made: { action: 'Wave', mode: 'as_it_looks', bones: 'all' } };
    const file: ResourceReplacement = { enabled: true, source: 'C:\\clips\\run.vnmclip_c', target: target('run'), allow_shared: false };
    for (const before of [made, file]) {
      const row = takenRow(target('run'), HERO, clip('run'), before);
      expect(row.source).toBeUndefined();
      expect(row.made).toBeUndefined();
      expect(row.taken).toBeDefined();
    }
    // A compiled file chosen for a taken row: no taken (and no made).
    const taken = takenRow(target('run'), HERO, clip('run'));
    expect(withCompiledFile(taken, 'C:\\clips\\run.vnmclip_c')).toEqual({ enabled: true, target: target('run'), allow_shared: false, source: 'C:\\clips\\run.vnmclip_c' });
    expect(withCompiledFile(made, 'C:\\clips\\run.vnmclip_c')).toEqual(file);
    // Made from an animation: the existing helper writes a new row, so no taken is left.
    const remade = madeRow(target('run'), 'C:\\anim\\moves.blend', { name: 'Wave', frame_start: 0, frame_end: 30, fps: 30, length_seconds: 1, keyed_bones: [], driving_keyed_bones: 10 }, { driving_bones: 10 }, taken);
    expect(remade.taken).toBeUndefined();
    expect(remade.made).toBeDefined();
    // Removing it.
    expect(withoutTaken(taken)).toEqual({ enabled: true, target: target('run'), allow_shared: false });
    expect(withoutMade(taken)).toEqual(taken);
  });

  it('the latest start leaves room for this clip', () => {
    expect(latestTakenStart(1.533, 1.033)).toBeCloseTo(0.5);
    expect(latestTakenStart(1, 1.2)).toBe(0);
  });

  it('one click writes a row for every usable member of a set', () => {
    const set: ClipSet = {
      name: 'dash',
      members: [
        { label: 'dash_ground', target: target('dash_ground'), clip: clip('dash_ground'), usable: true },
        { label: 'dash_air_forward', target: target('dash_air_forward'), clip: clip('dash_air_forward'), usable: true },
        { label: 'dash_air_back', target: target('dash_air_back'), clip: clip('dash_air_back'), usable: true },
        { label: 'dash_air_left', target: target('dash_air_left'), usable: false },
        { label: 'dash_air_right', clip: clip('dash_air_right'), usable: false },
      ],
    };
    const other: ResourceReplacement = { enabled: true, source: 'C:\\clips\\idle.vnmclip_c', target: target('idle'), allow_shared: false };
    const ownArms = withTaken(takenRow(target('DASH_AIR_BACK'), 'models/heroes_wip/third/third.vmdl', 'models/heroes_wip/third/clips/dash_air_back.vnmclip_c'), { arms: 'own' });
    const rows = takenSetRows([other, ownArms], set, HERO);
    expect(rows.map(r => r.target)).toEqual([target('idle'), target('dash_air_back'), target('dash_ground'), target('dash_air_forward')]);
    expect(rows[0]).toBe(other);
    expect(rows[1].taken).toEqual({ hero: HERO, clip: clip('dash_air_back'), timing: 'stretched', arms: 'own' });
    expect(rows[2].taken).toEqual({ hero: HERO, clip: clip('dash_ground'), timing: 'stretched', arms: 'source' });
    // The row limit holds.
    expect(takenSetRows([other], set, HERO, 2)).toHaveLength(2);
  });

  it('knows when taking the whole set again would change nothing', () => {
    const set: ClipSet = {
      name: 'slide',
      members: [
        { label: 'slide_start', target: target('slide_start'), clip: clip('slide_start'), usable: true },
        { label: 'slide_loop', target: target('slide_loop'), clip: clip('slide_loop'), usable: true },
        { label: 'slide_getup', target: target('slide_getup'), usable: false },
      ],
    };
    const other: ResourceReplacement = { enabled: true, source: 'C:\\clips\\idle.vnmclip_c', target: target('idle'), allow_shared: false };
    const rows = takenSetRows([other], set, HERO);
    expect(takenSetUnchanged([other], set, HERO)).toBe(false);
    expect(takenSetUnchanged(rows, set, HERO)).toBe(true);
    // Each row keeps its own timing and arms, and a row with its keys in another order is the same row.
    const own = [other, withTaken(rows[1], { arms: 'own' }), { taken: { arms: 'source', timing: 'own_speed', clip: clip('slide_loop'), hero: HERO }, allow_shared: false, target: target('slide_loop'), enabled: true } as ResourceReplacement];
    expect(takenSetUnchanged(own, set, HERO)).toBe(true);
    // Any difference the click would undo brings it back.
    expect(takenSetUnchanged([other, rows[1], { ...rows[2], enabled: false }], set, HERO)).toBe(false);
    expect(takenSetUnchanged([other, rows[1], withTaken(rows[2], { timing: 'own_speed', start_seconds: 0.5 })], set, HERO)).toBe(false);
    expect(takenSetUnchanged([other, rows[1]], set, HERO)).toBe(false);
    expect(takenSetUnchanged(rows, set, 'models/heroes_wip/third/third.vmdl')).toBe(false);
    expect(takenSetUnchanged(rows, { name: 'none', members: [{ label: 'x', usable: false }] }, HERO)).toBe(false);
  });
});
