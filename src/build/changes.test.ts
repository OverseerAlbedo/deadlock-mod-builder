import { describe, expect, it } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { AppearanceMaterial, Project } from '../models';
import { changesSince, loadBuildRecord, saveBuildRecord } from './changes';
import { scrubUser } from '../api';
import { changeSecondaryMotion } from '../components/customization';
import { progressFraction, timeLeft } from '../App';

const base = reference.cases[0].expected as unknown as Project;
describe('What changed since the last build', () => {
  it('names renamed items and item icons', () => {
    const now = structuredClone(base);
    now.customization = { ...base.customization, item_names: [{ item: 'upgrade_close_range', enabled: true, name: 'Dome Shot' }] };
    expect(changesSince(base, now)).toEqual(['Item names']);
    now.customization = { ...now.customization, item_icons: [{ target: 'panorama/images/items/weapon/close_quarters_psd.vtex_c', source: 'C:\\Art\\dome.png' }] };
    expect(changesSince(base, now)).toEqual(['Item names', 'Item icons']);
  });
  it('names the customization switch only when it is the one customization change', () => {
    const off: Project = { ...structuredClone(base), customization: { ...base.customization, enabled: false } };
    const on: Project = { ...structuredClone(off), customization: { ...off.customization, enabled: true } };
    expect(changesSince(off, on)).toEqual(['Customization on or off']);
    const rows = {
      item_names: [{ item: 'upgrade_close_range', enabled: true, name: 'Dome Shot' }],
      item_icons: [{ target: 'panorama/images/items/weapon/close_quarters_psd.vtex_c', source: 'C:\\Art\\dome.png' }],
      icons: [{ enabled: true, source: 'C:\\Art\\icon.png', target: 'panorama/images/heroes/sample_card_psd.vtex_c' }],
      audio: [{ enabled: true, source: 'C:\\Art\\cast.wav', target: 'sounds/abilities/sample_cast.vsnd_c' }],
    } as const;
    for (const [field, value] of Object.entries(rows)) {
      const extras = { ...structuredClone(on), customization: { ...on.customization, [field]: value } } as Project;
      const label = changesSince(on, extras);
      expect(changesSince(off, extras)).toEqual(label);
      expect(label).not.toContain('Customization on or off');
    }
    // Another top-level part changing beside the switch keeps the switch named.
    const slot = { ...structuredClone(on), vpk_number: on.vpk_number + 1 };
    expect(changesSince(off, slot).sort()).toEqual(['Customization on or off', 'VPK slot']);
  });
  it('names the parts that changed, once each, and ignores bookkeeping', () => {
    const now = structuredClone(base);
    now.name = 'Renamed'; now.preparation_report = 'new receipt';
    expect(changesSince(base, now)).toEqual([]);
    now.vpk_number = base.vpk_number + 1;
    now.preparation_options = {...base.preparation_options, fit_mode: 'preserve_proportions', fit_arm_joints: false, butt_motion: true};
    now.customization = {...base.customization, secondary_motion: {enabled: true}};
    expect(changesSince(base, now)).toEqual(['Model fitting', 'Butt bones', 'Physics and moving parts', 'VPK slot']);
  });
  it('lists only Mesh mapping when a mapping row changed and Analyze listed the material candidates again', () => {
    const built = { ...structuredClone(base), materials: { body: { preset: 'skin', confirmed: true, textures: { color: 'C:\\Art\\body.png' } } } } as Project;
    const now = structuredClone(built);
    now.meshes = { ...built.meshes, head: '@disable' };
    now.materials = { body: { ...built.materials.body, candidates: { color: ['C:\\Art\\body.png', 'C:\\Art\\body_alt.png'] }, ambiguous: [] } };
    expect(changesSince(built, now)).toEqual(['Mesh mapping']);
    // A changed choice is still named.
    now.materials = { body: { ...now.materials.body, confirmed: false } };
    expect(changesSince(built, now)).toEqual(['Mesh mapping', 'Materials']);
  });
  it('names only the motion choices for a Fine-tune change, and the cloth reference only when one was on', () => {
    const tuned = { tuning: { hair: { stiffness: 0.2 } } };
    for (const physics of [undefined, { enabled: false }, { enabled: false, source: 'C:\\references\\tested.vmdl_c' }]) {
      const before: Project = { ...structuredClone(base), customization: { ...base.customization, enabled: true, physics } };
      const after = changeSecondaryMotion(before, tuned);
      expect(after.customization.physics).toEqual(physics);
      expect(changesSince(before, after)).toEqual(['Physics and moving parts']);
    }
    const on: Project = { ...structuredClone(base), customization: { ...base.customization, enabled: true, physics: { enabled: true, source: 'C:\\references\\tested.vmdl_c' } } };
    const off = changeSecondaryMotion(on, tuned);
    expect(off.customization.physics).toEqual({ enabled: false, source: 'C:\\references\\tested.vmdl_c' });
    expect(changesSince(on, off).sort()).toEqual(['Cloth reference', 'Physics and moving parts']);
  });
  it('names color ranges on replacement materials apart from other color changes', () => {
    const row: AppearanceMaterial = { target: 'materials/body.vmat', hue: 20 };
    const before: Project = { ...structuredClone(base), customization: { ...base.customization, appearance: { enabled: true, materials: [row] } } };
    const ranged = structuredClone(before);
    ranged.customization!.appearance!.materials = [{ ...row, ranges: [{ hue: 25, hue_shift: 180 }] }];
    expect(changesSince(before, ranged)).toEqual(['Color ranges']);
    const both = structuredClone(ranged);
    both.customization!.appearance!.materials = [{ ...row, hue: 40, ranges: [{ hue: 25, hue_shift: 90 }] }];
    expect(changesSince(ranged, both)).toEqual(['Colors and surfaces', 'Color ranges']);
    const hue = structuredClone(before);
    hue.customization!.appearance!.materials = [{ ...row, hue: 40 }];
    expect(changesSince(before, hue)).toEqual(['Colors and surfaces']);
  });
  it('names Model fitting choices for the legs and the floor', () => {
    const now = structuredClone(base);
    now.preparation_options = {...base.preparation_options, fit_leg_joints: 'scaled_joints', fit_floor: 'lift'};
    expect(changesSince(base, now)).toEqual(['Leg joints', 'Feet on the floor']);
  });
  it('keeps the last build per project on this PC', () => {
    saveBuildRecord(base, '2026-10-14T10:00:00Z');
    expect(loadBuildRecord(base.id)).toEqual({time: '2026-10-14T10:00:00Z', project: base});
    expect(loadBuildRecord('other')).toBeNull();
  });
});
describe('Support-safe text', () => {
  it('hides the Windows user name in every path spelling', () => {
    expect(scrubUser(String.raw`C:\Users\sample\AppData and \?\c:\users\Sample\x and C:/Users/sample/y and {"p":"C:\Users\sample\z"} and D:\Games`))
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
