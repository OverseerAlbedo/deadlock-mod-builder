import { describe, expect, it, vi } from 'vitest';
import type { ActionInfo, MadeReport } from './makeTypes';
import { MAX_LOOP_BLEND, MAX_PART_START } from './makeTypes';
import {
  actionFrames, actionLabel, cleanMade, defaultBones, feetLine, gapWords, isAnimationFile, isMessage, latestPartStart, loopBlend, madeOf, madeRow,
  liftLine, makesLines, needsBlender, partStart, readableBone, someBones, speedWords, withMade, withoutMade,
} from './madeRows';
import { NewestQueue, isDropped } from './madeQueue';

const action = (name: string, driving: number, extra: Partial<ActionInfo> = {}): ActionInfo => ({
  name, frame_start: 0, frame_end: 60, fps: 30, length_seconds: 2, keyed_bones: [], driving_keyed_bones: driving, ...extra,
});
const TARGET = 'models/heroes_wip/sample/clips/stand_idle.vnmclip_c';
const FILE = 'C:\\anims\\my_character_moves.blend';

describe('made rows', () => {
  it('writes exactly the saved format’s keys: mode and bones always, the others only when they differ from the defaults', () => {
    expect(madeRow(TARGET, FILE, action('Wave', 3), { driving_bones: 40 })).toEqual({
      enabled: true, source: FILE, target: TARGET, allow_shared: false, made: { action: 'Wave', mode: 'as_it_looks', bones: 'keyed' },
    });
    const row = madeRow(TARGET, FILE, action('Walk', 38), { driving_bones: 40 });
    expect(row.made).toEqual({ action: 'Walk', mode: 'as_it_looks', bones: 'all' });
    expect(Object.keys(row)).toEqual(['enabled', 'source', 'target', 'allow_shared', 'made']);
    expect(Object.keys(row.made!)).toEqual(['action', 'mode', 'bones']);
    // Every option set: the keys in the saved format's order.
    const full = withMade(withMade(withMade(withMade(row, { part_start_seconds: 0.5 }), { loop_blend_seconds: 0.25 }), { in_place: false }), { mode: 'from_rest' });
    expect(Object.keys(full.made!)).toEqual(['action', 'mode', 'part_start_seconds', 'loop_blend_seconds', 'in_place', 'bones']);
    expect(full.made).toEqual({ action: 'Walk', mode: 'from_rest', part_start_seconds: 0.5, loop_blend_seconds: 0.25, in_place: false, bones: 'all' });
    // Back to the defaults: the keys go away again (0 is a start, not "absent").
    const back = withMade(withMade(withMade(full, { part_start_seconds: 0 }), { loop_blend_seconds: null }), { in_place: true });
    expect(back.made).toEqual({ action: 'Walk', mode: 'from_rest', part_start_seconds: 0, bones: 'all' });
    expect(withMade(back, { part_start_seconds: null }).made).toEqual({ action: 'Walk', mode: 'from_rest', bones: 'all' });
  });
  it('keeps every other field of the row and drops keys the saved format does not have', () => {
    const row = { enabled: false, source: FILE, target: TARGET, allow_shared: true, original_sha256: 'hash', made: { action: 'Wave', mode: 'as_it_looks', bones: 'all', speed: 2 } } as never;
    expect(withMade(row, { bones: 'keyed' })).toEqual({ enabled: false, source: FILE, target: TARGET, allow_shared: true, original_sha256: 'hash', made: { action: 'Wave', mode: 'as_it_looks', bones: 'keyed' } });
    expect(withoutMade(row)).toEqual({ enabled: false, source: FILE, target: TARGET, allow_shared: true, original_sha256: 'hash' });
    // A row without `made` is left as it is.
    const plain = { enabled: true, source: 'C:\\clips\\a.vnmclip_c', target: TARGET };
    expect(withMade(plain, { bones: 'keyed' })).toBe(plain);
  });
  it('keeps values inside the saved format’s limits, rounded to the millisecond', () => {
    expect(cleanMade({ action: 'x'.repeat(300), part_start_seconds: 4000, loop_blend_seconds: 12 })).toEqual({ action: 'x'.repeat(256), mode: 'as_it_looks', part_start_seconds: MAX_PART_START, loop_blend_seconds: MAX_LOOP_BLEND, bones: 'all' });
    expect(cleanMade({ action: 'Wave', part_start_seconds: -1, loop_blend_seconds: 0 })).toEqual({ action: 'Wave', mode: 'as_it_looks', part_start_seconds: 0, bones: 'all' });
    expect(cleanMade({ action: 'Wave', part_start_seconds: 0.1 + 0.2, loop_blend_seconds: Number.NaN, in_place: true })).toEqual({ action: 'Wave', mode: 'as_it_looks', part_start_seconds: 0.3, bones: 'all' });
  });
  it('defaults to every matched bone only when the animation keys nine tenths of the bones', () => {
    expect(defaultBones({ driving_keyed_bones: 36 }, { driving_bones: 40 })).toBe('all');
    expect(defaultBones({ driving_keyed_bones: 35 }, { driving_bones: 40 })).toBe('keyed');
    expect(defaultBones({ driving_keyed_bones: 0 }, { driving_bones: 0 })).toBe('keyed');
    // Out of the bones that drive the hero, not of every bone of the armature (control bones do not count).
    expect(defaultBones({ driving_keyed_bones: 19 }, { driving_bones: 19 })).toBe('all');
  });
  it('carries the mode, Stay in place and Loop to another animation, and starts the part and the bones again', () => {
    const before = { enabled: true, source: FILE, target: TARGET, allow_shared: false, made: { action: 'Wave', mode: 'from_rest' as const, part_start_seconds: 0.5, loop_blend_seconds: 0.3, in_place: false, bones: 'keyed' as const } };
    expect(madeRow(TARGET, FILE, action('Walk', 40), { driving_bones: 40 }, before).made).toEqual({ action: 'Walk', mode: 'from_rest', loop_blend_seconds: 0.3, in_place: false, bones: 'all' });
  });
  it('says when Blender is needed again: another file, animation or mode only', () => {
    const row = madeRow(TARGET, FILE, action('Wave', 3), { driving_bones: 40 });
    expect(needsBlender(null, row)).toBe(true);
    expect(needsBlender({ enabled: true, source: 'C:\\clips\\a.vnmclip_c', target: TARGET }, row)).toBe(true);
    expect(needsBlender(row, withMade(row, { bones: 'all' }))).toBe(false);
    expect(needsBlender(row, withMade(row, { part_start_seconds: 1 }))).toBe(false);
    expect(needsBlender(row, withMade(row, { loop_blend_seconds: 1 }))).toBe(false);
    expect(needsBlender(row, withMade(row, { in_place: false }))).toBe(false);
    expect(needsBlender(row, withMade(row, { mode: 'from_rest' }))).toBe(true);
    expect(needsBlender(row, { ...row, source: 'C:\\anims\\other.blend' })).toBe(true);
    expect(needsBlender(row, { ...row, made: { ...row.made!, action: 'Walk' } })).toBe(true);
  });
  it('reads made rows and the animation files they are made from', () => {
    expect(madeOf({ source: FILE, made: { action: 'Wave' } })).toEqual({ action: 'Wave' });
    expect(madeOf({ source: FILE })).toBeNull();
    expect(madeOf(null)).toBeNull();
    for (const path of [' C:\\a\\B.BLEND ', 'C:\\a\\moves.fbx', 'C:\\a\\Moves.GLB', 'C:\\a\\moves.gltf']) expect(isAnimationFile(path)).toBe(true);
    for (const path of ['C:\\a\\b.blend1', 'C:\\a\\clip.vnmclip_c', 'C:\\a\\model.pmx', '.fbx', '', null]) expect(isAnimationFile(path)).toBe(false);
  });
  it('keeps the part and the loop blend where they fit', () => {
    expect(latestPartStart(2.83, 1.83)).toBeCloseTo(1);
    expect(latestPartStart(1, 1.83)).toBe(0);
    expect(partStart(0.504, 2.83, 1.83)).toBe(0.5);
    expect(partStart(5, 2.83, 1.83)).toBe(1);
    expect(loopBlend(0.001, 1.83)).toBe(0.05);
    expect(loopBlend(3, 1.83)).toBe(1.83);
    expect(loopBlend(30, 60)).toBe(10);
  });
});

const report = (extra: Partial<MadeReport> = {}): MadeReport => ({
  action: 'Wave', mode: 'as_it_looks', switches: { new_fixed_values: true, refit_ranges: true, still_to_moving: true },
  clip_frames: 56, clip_seconds: 1.8333, action_seconds: 2.8333, plays_from: 0.5, plays_to: 2.3333, stretch: 1,
  follow: ['pelvis', 'spine_1', 'neck_0', 'head', 'arm_upper_L'], carried: ['arm_L_IKTARGET'], between: ['spine_0'],
  kept_count: 74, kept_kinds: { fingers: 38, gun: 8, cloth: 12, other: 14, 'IK targets': 2 }, kept_examples: {},
  not_in_clip: [], still_made_moving: ['pelvis'], still_new_fixed_value: [], still_held: [], still_kept_game_value: [],
  ranges_refitted: [], ranges_clamped: [], words_per_frame: [75, 78], loop_gap_degrees: 0.004, loop_gap_inches: 0,
  loop_blend_seconds: null, drift_removed_inches: 0, feet_lowest_inches: -0.79, feet_highest_inches: -0.78, ...extra,
});

describe('made report words', () => {
  it('lists what the app makes from the report alone, with no pass or fail', () => {
    expect(makesLines(report()).map(l => l.text)).toEqual([
      'Length, events and root motion are the game’s own',
      '5 bones follow your animation (pelvis, spine 1, neck 0, head, …)',
      '1 bone is carried with them, as the game’s clip carries them (left arm IK target)',
      '1 bone between them stays at the hero’s rest: your file has no bone for it (spine 0)',
      '74 bones keep the game’s motion (fingers, cloth, gun, IK targets, other)',
      '1 bone the game holds still now moves (pelvis)',
      'In this clip the feet end 0.8 in below the floor',
    ]);
    const quiet = makesLines(report({ carried: [], between: [], kept_count: 0, still_made_moving: [], still_new_fixed_value: ['pelvis', 'hand_L'], feet_lowest_inches: 0.1 }));
    expect(quiet.map(l => l.id)).toEqual(['kept', 'follow', 'fixed']);
    expect(quiet[2].text).toBe('2 bones the game holds still get a new fixed position (pelvis, left hand)');
  });
  it('says when the clip is raised or lowered to put the feet on the floor', () => {
    const lifted = makesLines(report({ floor_lift_inches: 0.7906, feet_lowest_inches: -0.002, feet_highest_inches: 0.0001 }));
    expect(lifted.map(l => l.id)).toEqual(['kept', 'follow', 'carried', 'between', 'game', 'moving', 'lift']);
    expect(lifted[6]).toEqual({ id: 'lift', mark: 'note', text: 'Raised by 0.8 in so the feet stand on the floor' });
    expect(liftLine({ floor_lift_inches: -1.26 })).toBe('Lowered by 1.3 in so the feet stand on the floor');
    expect(liftLine({ floor_lift_inches: 0 })).toBeNull();
    expect(liftLine({ floor_lift_inches: 0.04 })).toBeNull();
    expect(liftLine({})).toBeNull();
    expect(makesLines(report()).some(l => l.id === 'lift')).toBe(false);
  });
  it('mentions the feet only beyond a quarter of an inch', () => {
    expect(feetLine({ feet_lowest_inches: -0.2 })).toBeNull();
    expect(feetLine({ feet_lowest_inches: 0.6 })).toBe('In this clip the feet end 0.6 in above the floor');
    expect(feetLine({ feet_lowest_inches: null })).toBeNull();
  });
  it('says how the speed changes', () => {
    const r = report();
    expect(speedWords(r, false)).toBe('The game fixes this clip’s length at 1.83 s. Your animation plays at its own speed, from 0.50 s to 2.33 s.');
    expect(speedWords({ ...r, stretch: 1.09, action_seconds: 2 }, true)).toBe('The game fixes this clip’s length at 1.83 s. Your 2.00 s animation plays a little faster to fit it.');
    expect(speedWords({ ...r, stretch: 1.5 }, true)).toBe('The game fixes this clip’s length at 1.83 s. Your 2.83 s animation plays 1.5 times as fast to fit it.');
    expect(speedWords({ ...r, stretch: 0.5 }, true)).toBe('The game fixes this clip’s length at 1.83 s. Your 2.83 s animation plays at 50% of its speed to fit it.');
    expect(speedWords({ ...r, stretch: 1.001 }, true)).toBe('The game fixes this clip’s length at 1.83 s. Your 2.83 s animation fits it at its own speed.');
  });
  it('makes bone names readable and names actions with their length', () => {
    expect(readableBone('arm_upper_L')).toBe('left arm upper');
    expect(readableBone('clavicle_R')).toBe('right clavicle');
    expect(readableBone('head')).toBe('head');
    expect(someBones(['a', 'b'], 1)).toBe('a, …');
    expect(actionFrames({ frame_start: 0, frame_end: 85 })).toBe(86);
    expect(actionLabel(action('Wave', 3))).toBe('Wave · 2.00 s · 61 frames');
    expect(actionLabel(action('Pose', 3, { frame_end: 0, length_seconds: 0 }))).toBe('Pose · 0.00 s · 1 frame');
    expect(gapWords({ loop_gap_degrees: 0.004, loop_gap_inches: 0 })).toBe('Your animation already ends where it starts.');
    expect(gapWords({ loop_gap_degrees: 12.3, loop_gap_inches: 0.44 })).toBe('Before the blend, its last frame is 12° and 0.4 in away from its first.');
  });
  it('compares engine messages by their start', () => {
    expect(isMessage({ message: 'This file’s skeleton … (the bone x is missing)' }, 'This file’s skeleton')).toBe(true);
    expect(isMessage('other', 'This')).toBe(false);
  });
});

describe('newest request queue', () => {
  it('runs one request at a time, keeps only the newest waiting one and drops an answer that is not the newest', async () => {
    const queue = new NewestQueue();
    const sent: string[] = [];
    let release: (v: string) => void = () => undefined;
    const first = queue.ask(() => { sent.push('a'); return new Promise<string>(r => { release = r; }); });
    const second = queue.ask(async () => { sent.push('b'); return 'b'; });
    const third = queue.ask(async () => { sent.push('c'); return 'c'; });
    const settled = vi.fn();
    void second.catch(error => settled(isDropped(error)));
    await Promise.resolve(); await Promise.resolve();
    expect(sent).toEqual(['a']);
    release('a');
    await expect(first).rejects.toSatisfy(isDropped);
    await expect(third).resolves.toBe('c');
    expect(sent).toEqual(['a', 'c']);
    expect(settled).toHaveBeenCalledWith(true);
    // drop(): a running request's answer is dropped.
    const late = queue.ask(async () => 'late');
    queue.drop();
    await expect(late).rejects.toSatisfy(isDropped);
  });
});
