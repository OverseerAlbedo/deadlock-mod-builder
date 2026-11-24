import { describe, expect, it } from 'vitest';
import type { Project } from '../models';
import { includedExtras } from './includedExtras';

const project = (customization: Record<string, unknown>) => ({ customization } as unknown as Project);
const object = (target: string, enabled = true) => ({ target, enabled, source: 'C:\\Models\\block.gltf', fit: {} });

describe('the extras a build includes', () => {
  it('names the rows that are switched on', () => {
    const state = { enabled: true, ability_objects: [object('models/a.vmdl'), object('models/b.vmdl', false)], audio: [{ target: 'a' }, { target: 'b' }], icons: [{ target: 'c', enabled: false }] };
    expect(includedExtras(project(state), true)).toBe('2 sounds · 1 ability object');
  });

  it('is empty while extras are paused or there are none', () => {
    expect(includedExtras(project({ enabled: false, ability_objects: [object('models/a.vmdl')] }), true)).toBe('');
    expect(includedExtras(project({ enabled: true }), true)).toBe('');
  });

  it('counts the game hero\'s colors only without a replacement model', () => {
    const state = { enabled: true, hero_materials: { materials: [{ target: 'm' }] } };
    expect(includedExtras(project(state), false)).toBe('1 hero color');
    expect(includedExtras(project(state), true)).toBe('');
  });

  it('counts renamed items that are switched on, as item icons are counted', () => {
    const state = { enabled: true, item_icons: [{ target: 'a' }], item_names: [{ item: 'upgrade_close_range', enabled: true, name: 'Dome Shot' }, { item: 'upgrade_clip_size', enabled: false, name: 'Big Clip' }] };
    expect(includedExtras(project(state), true)).toBe('1 item icon · 1 item name');
    state.item_names[1].enabled = true;
    expect(includedExtras(project(state), true)).toBe('1 item icon · 2 item names');
  });
});
