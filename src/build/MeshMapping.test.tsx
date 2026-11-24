import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MeshMapping, addToMainNode, effectiveMappings, suggestedMappings, unusedSourceMeshes } from './MeshMapping';
import type { InspectionReport } from './models';

const node = (name: string, lower_lod = false) => ({ name, filename: null, lower_lod });
const report = {
  model: { meshes: [node('ghost'), node('ghost_armglow'), node('gun'), node('ghost_lod1', true), node('gun_lod1', true), node('ghost_lod2', true)], lod_groups: 3, attachments: 0, ag2: [] },
  dmx: { meshes: [{ name: 'Body', element: 'a' }, { name: 'Face', element: 'b' }] },
  project_mappings: {},
} as unknown as InspectionReport;
// The project as saved after choosing Body in the old single-looking list.
const saved = { ghost: 'Body', ghost_lod1: 'Body', ghost_lod2: 'Body', ghost_armglow: '@disable', gun: '@disable', gun_lod1: '@disable' };

function Harness({ start, strategy }: { start: Record<string, string | string[]>; strategy?: string }) {
  const [meshes, setMeshes] = useState(start);
  return <><MeshMapping report={report} meshes={meshes} busy={false} update={setMeshes} strategy={strategy} /><output data-testid="meshes">{JSON.stringify(meshes)}</output></>;
}
const current = () => JSON.parse(screen.getByTestId('meshes').textContent!);

describe('Mesh mapping', () => {
  it('finds source meshes no render node uses and adds them to the main node and its detail levels', () => {
    expect(unusedSourceMeshes(report, saved)).toEqual(['Face']);
    const { meshes, node: main } = addToMainNode(report, saved, ['Face']);
    expect(main).toBe('ghost');
    expect(meshes).toMatchObject({ ghost: ['Body', 'Face'], ghost_lod1: ['Body', 'Face'], ghost_lod2: ['Body', 'Face'], gun: '@disable', ghost_armglow: '@disable' });
  });

  it('shows every source mesh as a checkbox and offers a one-click fix for missing meshes', () => {
    render(<Harness start={saved} />);
    expect(document.querySelector('.mesh-unused')).toHaveTextContent('missing in game: Face');
    // The suggestion offers the same addition to the rows Analyze saved.
    expect(document.querySelector('.mesh-suggestion')).toHaveTextContent('ghost → Body + Face');
    fireEvent.click(screen.getByRole('button', { name: 'Add it to ghost and its lower-detail levels' }));
    expect(current()).toMatchObject({ ghost: ['Body', 'Face'], ghost_lod1: ['Body', 'Face'], ghost_lod2: ['Body', 'Face'] });
    expect(screen.queryByRole('button', { name: /Add it to/ })).not.toBeInTheDocument();
    const ghost = screen.getByRole('group', { name: 'Replacement meshes for ghost' });
    const face = ghost.querySelectorAll('input[type=checkbox]')[1] as HTMLInputElement;
    expect(face).toBeChecked();
    fireEvent.click(face);
    expect(current().ghost).toBe('Body');
  });

  it('offers the fix for an unused mesh while a suggestion is still pending (issue #6)', () => {
    // Custom detail levels: two lower-detail rows have no choice yet, so a suggestion is
    // pending; Face (the moving parts' mesh) is used by no render node.
    const partial = { ghost: 'Body', ghost_armglow: '@disable', gun: '@disable', gun_lod1: '@disable' };
    render(<Harness start={partial} strategy="custom" />);
    expect(screen.getByRole('button', { name: 'Apply suggested mapping' })).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Add it to ghost and its lower-detail levels' }));
    // The rows without a choice follow the main node.
    expect(current()).toMatchObject({ ghost: ['Body', 'Face'], ghost_lod1: ['Body', 'Face'], ghost_lod2: ['Body', 'Face'] });
  });

  it('labels analysis suggestions and makes a changed suggestion explicit', () => {
    const suggested = { ...report, project_mappings: { ghost: ['Body', 'Face'], ghost_armglow: '@disable' } } as unknown as InspectionReport;
    function Suggested() { const [meshes, setMeshes] = useState<Record<string, string | string[]>>({}); return <><MeshMapping report={suggested} meshes={meshes} busy={false} update={setMeshes} /><output data-testid="meshes">{JSON.stringify(meshes)}</output></>; }
    render(<Suggested />);
    expect(screen.getAllByText(/· Suggested/).length).toBe(2);
    fireEvent.change(screen.getByLabelText('Action for ghost_armglow'), { target: { value: '@keep' } });
    expect(current()).toEqual({ ghost_armglow: '@keep' });
  });

  it('proposes a full mapping for a fresh prepared character and applies it only when chosen', () => {
    expect(suggestedMappings(report, {}, 'reuse')).toEqual({ ghost: ['Body', 'Face'], ghost_armglow: '@disable', gun: '@keep', ghost_lod1: ['Body', 'Face'], ghost_lod2: ['Body', 'Face'], gun_lod1: '@keep' });
    expect(suggestedMappings(report, {}, 'preserve')).toMatchObject({ ghost_lod1: '@keep', ghost_lod2: '@keep' });
    // Sources named like the hero's render nodes keep the classic name matching.
    const named = { ...report, dmx: { meshes: [{ name: 'ghost', element: 'a' }] } } as unknown as InspectionReport;
    expect(suggestedMappings(named, {}, 'reuse')).toEqual({});
    render(<Harness start={{}} strategy="reuse" />);
    expect(current()).toEqual({});
    fireEvent.click(screen.getByRole('button', { name: 'Apply suggested mapping' }));
    expect(current()).toMatchObject({ ghost: ['Body', 'Face'], gun: '@keep', ghost_armglow: '@disable' });
    expect(screen.queryByRole('button', { name: 'Apply suggested mapping' })).not.toBeInTheDocument();
  });

  it('suggests the other parts and the left-over meshes when only one source is named like a render node', () => {
    const parts = ['body', 'face', 'gun', 'turret_carry', 'healing_canister'];
    // Analyze marks the hero's carried objects, detail levels included.
    const carried = (name: string) => /^(turret_carry|healing_canister)/.test(name) ? { carried: true } : {};
    const nodes = [...parts.map(name => node(name)), ...parts.flatMap(name => [1, 2, 3].map(level => node(`${name}_lod${level}`, true)))]
      .map(item => ({ ...item, ...carried(item.name) }));
    const analysed = {
      body: 'Body', body_lod1: 'Body', body_lod2: 'Body', body_lod3: 'Body',
      gun: '@keep', gun_lod1: '@keep', gun_lod2: '@keep', gun_lod3: '@keep',
    };
    const partly = {
      model: { meshes: nodes, lod_groups: 4, attachments: 0, ag2: [] },
      dmx: { meshes: [{ name: 'Body', element: 'a' }, { name: 'Hair', element: 'b' }] },
      project_mappings: analysed,
    } as unknown as InspectionReport;
    const suggestion = suggestedMappings(partly, {}, 'reuse');
    const expected: Record<string, string | string[]> = {};
    for (const name of ['body', 'body_lod1', 'body_lod2', 'body_lod3']) expected[name] = ['Body', 'Hair'];
    const levels = (part: string) => [part, `${part}_lod1`, `${part}_lod2`, `${part}_lod3`];
    for (const name of levels('face')) expected[name] = '@disable';
    for (const part of ['turret_carry', 'healing_canister']) for (const name of levels(part)) expected[name] = '@keep';
    expect(suggestion).toEqual(expected);
    // Without the flag the same parts are disabled, as before.
    const unmarked = { ...partly, model: { ...partly.model, meshes: nodes.map(({ name, filename, lower_lod }) => ({ name, filename, lower_lod })) } } as unknown as InspectionReport;
    const plain = suggestedMappings(unmarked, {}, 'reuse');
    for (const part of ['face', 'turret_carry', 'healing_canister']) for (const name of levels(part)) expect(plain[name]).toBe('@disable');
    const applied = { ...suggestion };
    expect(unusedSourceMeshes(partly, applied)).toEqual([]);
    const effective = effectiveMappings(partly, applied);
    expect(nodes.filter(item => effective[item.name] === undefined)).toEqual([]);
    // The callout no longer claims that every suggested row is without a choice.
    function Partly() { const [meshes, setMeshes] = useState<Record<string, string | string[]>>({}); return <><MeshMapping report={partly} meshes={meshes} busy={false} update={setMeshes} strategy="reuse" /><output data-testid="meshes">{JSON.stringify(meshes)}</output></>; }
    render(<Partly />);
    expect(screen.getByText(/for 16 render nodes:/)).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Apply suggested mapping' }));
    expect(current()).toEqual(expected);
    expect(screen.queryByRole('button', { name: 'Apply suggested mapping' })).not.toBeInTheDocument();
  });

  it('in the real flow (Analyze saved its matches as the project\'s rows) one click leaves nothing unused and no row without a choice', () => {
    const parts = ['body', 'face', 'gun', 'turret_carry'];
    const nodes = [...parts.map(name => node(name)), ...parts.flatMap(name => [1, 2].map(level => node(`${name}_lod${level}`, true)))]
      .map(item => /^turret_carry/.test(item.name) ? { ...item, carried: true } : item);
    // What Analyze matched by name, and what it then saved in the project (App: project.meshes = project_mappings).
    const analysed = { body: 'Body', body_lod1: 'Body', body_lod2: 'Body', gun: '@keep', gun_lod1: '@keep', gun_lod2: '@keep' };
    const after = {
      model: { meshes: nodes, lod_groups: 3, attachments: 0, ag2: [] },
      dmx: { meshes: [{ name: 'Body', element: 'a' }, { name: 'Hair', element: 'b' }] },
      project_mappings: analysed,
    } as unknown as InspectionReport;
    function Real() { const [meshes, setMeshes] = useState<Record<string, string | string[]>>({ ...analysed }); return <><MeshMapping report={after} meshes={meshes} busy={false} update={setMeshes} strategy="reuse" /><output data-testid="meshes">{JSON.stringify(meshes)}</output></>; }
    render(<Real />);
    fireEvent.click(screen.getByRole('button', { name: 'Apply suggested mapping' }));
    const applied = current();
    expect(unusedSourceMeshes(after, applied)).toEqual([]);
    expect(nodes.filter(item => effectiveMappings(after, applied)[item.name] === undefined)).toEqual([]);
    expect(applied).toMatchObject({ body: ['Body', 'Hair'], body_lod1: ['Body', 'Hair'], body_lod2: ['Body', 'Hair'], gun: '@keep', face: '@disable', turret_carry: '@keep' });
    expect(screen.queryByRole('button', { name: 'Apply suggested mapping' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Add it to/ })).not.toBeInTheDocument();
    // A row the user set to Keep or Disable, and what a row shows, are never replaced or removed.
    const chosen = { ...analysed, body_lod2: '@disable', face: '@keep', body: ['Body', 'Extra'] };
    const withExtra = { ...after, dmx: { meshes: [...after.dmx.meshes, { name: 'Extra', element: 'c' }] } } as unknown as InspectionReport;
    const suggestion = suggestedMappings(withExtra, chosen, 'reuse');
    expect(suggestion.body).toEqual(['Body', 'Extra', 'Hair']);
    expect(suggestion).not.toHaveProperty('body_lod2');
    expect(suggestion).not.toHaveProperty('face');
    expect(suggestion).not.toHaveProperty('gun');
  });

  it('never adds left-over meshes to a gun, by its name or as a gun part', () => {
    // Before the suggestion is applied the gun is the first node that shows meshes.
    const named = {
      model: { meshes: [node('weapon'), node('body'), node('weapon_lod1', true), node('body_lod1', true)], lod_groups: 2, attachments: 0, ag2: [] },
      dmx: { meshes: [{ name: 'Weapon', element: 'a' }, { name: 'Body', element: 'b' }, { name: 'Hair', element: 'c' }] },
      project_mappings: { weapon: 'Weapon', weapon_lod1: 'Weapon' },
    } as unknown as InspectionReport;
    const mapped = { weapon: 'Weapon', weapon_lod1: 'Weapon' };
    const added = addToMainNode(named, mapped, unusedSourceMeshes(named, mapped));
    expect(added.node).toBe('body');
    expect(added.meshes).toMatchObject({ weapon: 'Weapon', weapon_lod1: 'Weapon', body: ['Body', 'Hair'] });
    const bow = { ...named, model: { ...named.model, meshes: [{ ...node('bow'), gun_part: true }, node('body')] }, project_mappings: { bow: 'Weapon' } } as unknown as InspectionReport;
    expect(addToMainNode(bow, { bow: 'Weapon' }, ['Body', 'Hair']).node).toBe('body');
    render(<MeshMapping report={named} meshes={mapped} busy={false} update={() => undefined} />);
    expect(screen.getByRole('button', { name: 'Add them to body and its lower-detail levels' })).toBeVisible();
  });

  describe('parts of the gun whose names do not say so', () => {
    // A hero whose gun carries ammunition pieces (one switched, one shown by default) and whose
    // first render node is a bow-like gun part; Analyze marks the gun parts, detail levels included.
    const gunPart = (name: string) => /^(bow|ammo_)/.test(name) ? { gun_part: true } : {};
    const names = ['bow', 'body', 'face', 'hero_weapon_model', 'ammo_01', 'ammo_full'];
    const nodes = [...names.map(name => node(name)), ...names.map(name => node(`${name}_lod1`, true))]
      .map(item => ({ ...item, ...gunPart(item.name), ...(/^hero_weapon_model/.test(item.name) ? { gun_part: true } : {}) }));
    const hero = {
      model: { meshes: nodes, lod_groups: 2, attachments: 0, ag2: [] },
      dmx: { meshes: [{ name: 'Body', element: 'a' }, { name: 'Gun', element: 'b' }] },
      project_mappings: {},
    } as unknown as InspectionReport;
    const pieces = ['bow', 'bow_lod1', 'ammo_01', 'ammo_01_lod1', 'ammo_full', 'ammo_full_lod1'];

    it('keeps them with the hero\'s own gun, and never makes one the main body node', () => {
      const suggestion = suggestedMappings(hero, {}, 'reuse');
      expect(suggestion.body).toEqual(['Body', 'Gun']);
      for (const name of pieces) expect(suggestion[name]).toBe('@keep');
      expect(suggestion).toMatchObject({ hero_weapon_model: '@keep', hero_weapon_model_lod1: '@keep', face: '@disable', face_lod1: '@disable' });
      // A gun node the analysis keeps still counts as the hero's own gun.
      const analysed = { ...hero, project_mappings: { hero_weapon_model: '@keep' } } as unknown as InspectionReport;
      for (const name of pieces) expect(suggestedMappings(analysed, {}, 'reuse')[name]).toBe('@keep');
      // Without the flag the same pieces are disabled, as before (the bow then becomes the main node).
      const unmarked = { ...hero, model: { ...hero.model, meshes: nodes.map(({ name, filename, lower_lod }) => ({ name, filename, lower_lod })) } } as unknown as InspectionReport;
      const plain = suggestedMappings(unmarked, {}, 'reuse');
      for (const name of ['ammo_01', 'ammo_full']) expect(plain[name]).toBe('@disable');
    });

    it('disables them when the user\'s gun replaces the hero\'s or the hero\'s gun is disabled', () => {
      const fitted = { hero_weapon_model: 'Gun', hero_weapon_model_lod1: 'Gun' };
      const suggestion = suggestedMappings(hero, fitted, 'reuse');
      for (const name of pieces) expect(suggestion[name]).toBe('@disable');
      expect(suggestion.body).toBe('Body');
      // The same when the fitted gun is only the analysis' carried-over mapping.
      const analysed = { ...hero, project_mappings: fitted } as unknown as InspectionReport;
      for (const name of pieces) expect(suggestedMappings(analysed, {}, 'reuse')[name]).toBe('@disable');
      const removed = suggestedMappings(hero, { hero_weapon_model: '@disable' }, 'reuse');
      for (const name of pieces) expect(removed[name]).toBe('@disable');
      // A node only named as a gun (a holstered copy, not on weapon bones) decides nothing when disabled.
      const holstered = { ...hero, model: { ...hero.model, meshes: [...nodes, node('gun_on_hip')] } } as unknown as InspectionReport;
      expect(suggestedMappings(holstered, { gun_on_hip: '@disable' }, 'reuse').ammo_01).toBe('@keep');
      // A detail level the LOD strategy disables does not decide it.
      const levels = suggestedMappings({ ...hero, project_mappings: { hero_weapon_model_lod1: '@disable' } } as unknown as InspectionReport, {}, 'reuse');
      expect(levels.ammo_01).toBe('@keep');
    });

    it('never changes a saved row', () => {
      // A fitted gun from Prepare model writes the user's gun meshes on every gun part itself.
      const saved = { hero_weapon_model: 'Gun', hero_weapon_model_lod1: 'Gun', ammo_01: 'Gun', ammo_01_lod1: 'Gun', ammo_full: '@keep', bow: '@keep' };
      const suggestion = suggestedMappings(hero, saved, 'reuse');
      for (const name of Object.keys(saved)) expect(suggestion).not.toHaveProperty(name);
      expect(suggestion).toMatchObject({ ammo_full_lod1: '@keep', bow_lod1: '@keep' });
      expect({ ...saved, ...suggestion }).toMatchObject(saved);
      const kept = suggestedMappings(hero, { ammo_01: '@disable' }, 'reuse');
      expect(kept).not.toHaveProperty('ammo_01');
      expect(kept).toMatchObject({ ammo_full: '@keep', ammo_01_lod1: '@disable' });
    });
  });
});
