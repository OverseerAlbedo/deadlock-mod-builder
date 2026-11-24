import { describe, expect, it } from 'vitest';
import type { MadeReport } from './makeTypes';
import { TAKEN_ARM_BONES, type TakenReport } from './takenTypes';
import { takenLines } from './takenWords';

const MADE = {
  action: 'Run', mode: 'as_it_looks', switches: null, clip_frames: 30, clip_seconds: 1, action_seconds: 1, plays_from: 0, plays_to: 1, stretch: 1,
  follow: [], carried: [], between: [], kept_count: 70, kept_kinds: { fingers: 30, cloth: 20, other: 20 }, kept_examples: {}, not_in_clip: [],
  still_made_moving: [], still_new_fixed_value: [], still_held: [], still_kept_game_value: [], ranges_refitted: [], ranges_clamped: [], words_per_frame: [70, 70],
  loop_gap_degrees: 0, loop_gap_inches: 0, loop_blend_seconds: null, drift_removed_inches: null, feet_lowest_inches: null, feet_highest_inches: null,
} as unknown as MadeReport;
const report = (own: string[], made: Partial<MadeReport> = {}): TakenReport =>
  ({ made: { ...MADE, ...made }, source_name: 'Other', driven: ['pelvis', 'head'], own, not_matched: [], speed_percent: 0, size: 1 });
const text = (lines: ReturnType<typeof takenLines>, id: string) => lines.find(l => l.id === id)?.text;

describe('What you get: the arm bones are counted once', () => {
  it('counts the kept bones as before when the whole body follows the other hero', () => {
    const lines = takenLines(report([]), 'Haze', true);
    expect(text(lines, 'arms')).toBeUndefined();
    expect(text(lines, 'kept')).toBe('70 bones keep Haze’s own motion (fingers, cloth, other)');
  });

  it('leaves the arms line’s bones out of the general line when the arms keep the hero’s own motion', () => {
    // The engine's kept count holds the eight arm bones too (as "other").
    const lines = takenLines(report([...TAKEN_ARM_BONES], { kept_count: 78, kept_kinds: { fingers: 30, cloth: 20, other: 28 } }), 'Haze', true);
    expect(text(lines, 'arms')).toMatch(/^Arms and hands keep Haze’s own motion: 8 bones/);
    expect(text(lines, 'kept')).toBe('70 bones keep Haze’s own motion (fingers, cloth, other)');
  });

  it('drops "other" from the kinds when only the arm bones were of that kind', () => {
    const lines = takenLines(report([...TAKEN_ARM_BONES], { kept_count: 38, kept_kinds: { fingers: 30, other: 8 } }), 'Haze', true);
    expect(text(lines, 'kept')).toBe('30 bones keep Haze’s own motion (fingers)');
  });

  it('says the general line includes the arms when the numbers do not let them be taken out', () => {
    const lines = takenLines(report([...TAKEN_ARM_BONES], { kept_count: 34, kept_kinds: { fingers: 30, other: 4 } }), 'Haze', true);
    expect(text(lines, 'kept')).toBe('34 bones keep Haze’s own motion (fingers, other), the arms and hands included');
  });
});
