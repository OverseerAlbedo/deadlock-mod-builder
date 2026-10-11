import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MeshMapping, addToMainNode, suggestedMappings, unusedSourceMeshes } from './MeshMapping';
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
    expect(screen.getByText('Face', { selector: 'p *, p' , exact: false })).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Add it to ghost and its lower-detail levels' }));
    expect(current()).toMatchObject({ ghost: ['Body', 'Face'], ghost_lod1: ['Body', 'Face'], ghost_lod2: ['Body', 'Face'] });
    expect(screen.queryByRole('button', { name: /Add it to/ })).not.toBeInTheDocument();
    const ghost = screen.getByRole('group', { name: 'Replacement meshes for ghost' });
    const face = ghost.querySelectorAll('input[type=checkbox]')[1] as HTMLInputElement;
    expect(face).toBeChecked();
    fireEvent.click(face);
    expect(current().ghost).toBe('Body');
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
});
