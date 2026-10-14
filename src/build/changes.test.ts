import { describe, expect, it } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import { changesSince, loadBuildRecord, saveBuildRecord } from './changes';
import { scrubUser } from '../api';
import { progressFraction, timeLeft } from '../App';

const base = reference.cases[0].expected as unknown as Project;
describe('What changed since the last build', () => {
  it('names the parts that changed, once each, and ignores bookkeeping', () => {
    const now = structuredClone(base);
    now.name = 'Renamed'; now.preparation_report = 'new receipt';
    expect(changesSince(base, now)).toEqual([]);
    now.vpk_number = base.vpk_number + 1;
    now.preparation_options = {...base.preparation_options, fit_mode: 'preserve_proportions', fit_arm_joints: false, butt_motion: true};
    now.customization = {...base.customization, secondary_motion: {enabled: true}};
    expect(changesSince(base, now)).toEqual(['Model fitting', 'Butt bones', 'Physics and moving parts', 'VPK slot']);
  });
  it('keeps the last build per project on this PC', () => {
    saveBuildRecord(base, '2026-10-14T10:00:00Z');
    expect(loadBuildRecord(base.id)).toEqual({time: '2026-10-14T10:00:00Z', project: base});
    expect(loadBuildRecord('other')).toBeNull();
  });
});
describe('Support-safe text', () => {
  it('hides the Windows user name in every path spelling', () => {
    expect(scrubUser(String.raw`C:\Users\jayde\AppData and \?\c:\users\Jayde\x and C:/Users/jayde/y and {"p":"C:\Users\jayde\z"} and D:\Games`))
      .toBe(String.raw`C:\Users\<you>\AppData and \?\c:\users\<you>\x and C:/Users/<you>/y and {"p":"C:\Users\<you>\z"} and D:\Games`);
  });
});
describe('Time left', () => {
  it('estimates from earlier runs and says when a run takes longer', () => {
    const start = '2026-10-14T10:00:00Z', at = (s: number) => Date.parse(start) + s * 1000;
    expect(timeLeft(undefined, start, at(10))).toBeNull();
    expect(timeLeft(300, start, at(60))).toBe('about 4 minutes left');
    expect(timeLeft(100, start, at(40))).toBe('about 60 seconds left');
    expect(timeLeft(100, start, at(95))).toBe('almost done');
    expect(timeLeft(100, start, at(200))).toBe('taking longer than usual');
  });
  it('fills the progress bar from earlier runs and never shows full before the end', () => {
    const start = '2026-10-14T10:00:00Z', at = (s: number) => Date.parse(start) + s * 1000;
    expect(progressFraction(undefined, start, at(10))).toBeNull();
    expect(progressFraction(100, start, at(25))).toBe(0.25);
    expect(progressFraction(100, start, at(500))).toBe(0.97);
  });
});
