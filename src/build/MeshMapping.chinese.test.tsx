import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { MeshMapping } from './MeshMapping';
import type { InspectionReport } from './models';
import { MIRROR_KEY, setLanguage } from '../i18n';

// Mesh mapping in Chinese: node and mesh names stay as they are, the choices are stored as their
// identifiers, and the suggestion and the one-click fix work by their Chinese names.
const node = (name: string, lower_lod = false) => ({ name, filename: null, lower_lod });
const report = {
  model: { meshes: [node('ghost'), node('ghost_armglow'), node('gun'), node('ghost_lod1', true)], lod_groups: 2, attachments: 0, ag2: [] },
  dmx: { meshes: [{ name: 'Body', element: 'a' }, { name: 'Face', element: 'b' }] },
  project_mappings: {},
} as unknown as InspectionReport;

function Harness({ start }: { start: Record<string, string | string[]> }) {
  const [meshes, setMeshes] = useState(start);
  return <><MeshMapping report={report} meshes={meshes} busy={false} update={setMeshes} strategy="reuse"/><output data-testid="meshes">{JSON.stringify(meshes)}</output></>;
}
const current = () => JSON.parse(screen.getByTestId('meshes').textContent!);

beforeEach(async () => { await act(() => setLanguage('zh-CN')); });
afterEach(async () => { cleanup(); await act(() => setLanguage('en')); localStorage.removeItem(MIRROR_KEY); });

describe('Mesh mapping in Chinese', () => {
  it('applies the suggestion, adds unused meshes and stores the choices as identifiers', () => {
    render(<Harness start={{}}/>);
    const suggestion = document.querySelector('.mesh-suggestion p')!;
    expect(suggestion).toHaveTextContent('为 4 个尚未选择的渲染节点提供建议的映射：ghost → Body + Face；ghost_armglow → 停用；gun → 保留原样；ghost_lod1 → Body + Face。');
    fireEvent.click(screen.getByRole('button', { name: '应用建议的映射' }));
    expect(current()).toEqual({ ghost: ['Body', 'Face'], ghost_armglow: '@disable', gun: '@keep', ghost_lod1: ['Body', 'Face'] });
    fireEvent.change(screen.getByRole('combobox', { name: 'ghost_armglow 的操作' }), { target: { value: '@keep' } });
    expect(current().ghost_armglow).toBe('@keep');
    expect(screen.getAllByText('保留原版几何体')).toHaveLength(2);
  });

  it('names the unused meshes as they are and adds them in one click', () => {
    render(<Harness start={{ ghost: 'Body', ghost_lod1: 'Body', ghost_armglow: '@disable', gun: '@disable' }}/>);
    expect(screen.getByText(/没有任何渲染节点使用，因此游戏中会缺失：/).closest('p')).toHaveTextContent('没有任何渲染节点使用，因此游戏中会缺失：Face');
    fireEvent.click(screen.getByRole('button', { name: '将它们添加到 ghost 及其低细节层级' }));
    expect(current()).toMatchObject({ ghost: ['Body', 'Face'], ghost_lod1: ['Body', 'Face'] });
    expect(screen.getByRole('group', { name: 'ghost 的替换网格' })).toHaveTextContent('BodyFace');
  });
});
