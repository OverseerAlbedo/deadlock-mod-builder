import { useEffect, useState } from 'react';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Project } from '../models';
import type { PreviewScene } from '../preview/packet';
import { reviewedProject } from '../build/testFixtures';
import { MIRROR_KEY, setLanguage } from '../i18n';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

// Appearance & weapons driven in Chinese: controls are found by their Chinese names, and what the
// page saves (material settings, preset names, accessory actions, project names) stays English.
const probe = vi.hoisted(() => ({mounts: 0, props: null as Record<string, unknown> | null}));
const native = vi.hoisted(() => ({validateAssetPath: vi.fn(async (path: string) => path), previewImage: vi.fn(), fileStamps: vi.fn(), discoverWeaponModels: vi.fn(), newProject: vi.fn(), selectHero: vi.fn(), saveProject: vi.fn(), save: vi.fn()}));
vi.mock('../preview/Viewport', () => ({default: (props: Record<string, unknown>) => {
  probe.props = props;
  useEffect(() => { probe.mounts++; }, []);
  return <div data-testid="viewport"/>;
}}));
vi.mock('./ImagePreview', () => ({ImagePreview: ({path}: {path: string}) => <span data-testid="texture-preview">{path}</span>}));
vi.mock('../api', async original => ({...await original<typeof import('../api')>(), api: native}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn(), save: native.save}));
import { Appearance } from './Appearance';
import { WeaponModels } from './WeaponModels';

const a = 'models/test/materials/outfit.vmat', b = 'models/test/materials/hair.vmat';
const base = {...reviewedProject, original_model: 'models/heroes_wip/test/test.vmdl',
  materials: {[a]: {mode: 'generate', confirmed: true, textures: {color: 'C:\\t\\outfit.png'}}, [b]: {mode: 'generate', confirmed: false, textures: {}}}} as unknown as Project;
function scene(): PreviewScene {
  const names = [a, b];
  const buffer = new ArrayBuffer(names.length * 6 * 4 + 4 * names.length);
  const floats = new Float32Array(buffer, 0, names.length * 6);
  names.forEach((_, i) => floats.set([i, 0, i * 10, 0, 0, 1], i * 6));
  const meshes = names.map((material, i) => ({name: `m${i}`, source_name: `m${i}`, material, vertex_count: 1, index_count: 0, positions: {offset: i * 24, count: 3}, normals: {offset: i * 24 + 12, count: 3}, uvs: null, colors: null, indices: {offset: names.length * 24 + i * 4, count: 0}, skin: null}));
  return {metadata: {schema_version: 1, source: 'my_character.dmx', source_sha256: 's', up_axis: 'Z', warnings: [], bounds: {min: [0, 0, 0], max: [2, 1, 20]}, bones: [{name: 'head', parent: -1, position: [0, 0, 18]}], meshes}, buffer, dataOffset: 0} as unknown as PreviewScene;
}
function Page({project: initial}: {project: Project}) {
  const [project, setProject] = useState(initial);
  return <><Appearance document={{path: null, project}} summary={null} choose={vi.fn()} analyze={vi.fn()} busy={false} drag={null} update={setProject} fail={vi.fn()} preview={scene()} loadPreview={vi.fn()} importPackage={vi.fn()} analyzeMaterials={vi.fn()}/><output data-testid="saved">{JSON.stringify(project)}</output></>;
}
const saved = () => JSON.parse(screen.getByTestId('saved').textContent!) as Project;

beforeEach(async () => { vi.clearAllMocks(); probe.mounts = 0; probe.props = null; await act(() => setLanguage('zh-CN')); });
afterEach(async () => { cleanup(); await act(() => setLanguage('en')); localStorage.removeItem(MIRROR_KEY); });

describe('Appearance & weapons in Chinese', () => {
  it('shows the tabs and menus in Chinese and saves material choices as their English values', async () => {
    render(<Page project={base}/>);
    await screen.findByTestId('viewport');
    expect(screen.getAllByRole('tab').map(tab => tab.textContent)).toEqual(['材质 1 个待检查', '面部 无需设置', '枪械 与模型一致', '配饰与道具 无']);
    for (const name of ['模型', '网格选项', '贴图文件夹', '已保存的输入']) expect(screen.getByRole('button', {name})).toBeInTheDocument();
    // The labels on the character keep the material names and say their state in Chinese.
    const labels = probe.props!.labels as {items: {text: string; detail: string}[]; label: string};
    expect(labels.label).toBe('角色上的材质');
    expect(labels.items.map(item => [item.text, item.detail])).toEqual([['outfit', '已检查'], ['hair', '已选中']]);
    // The selected material (the one to review): a surface preset by its Chinese name is saved in English.
    fireEvent.change(screen.getByLabelText('表面预设'), {target: {value: 'painted metal'}});
    expect(within(screen.getByLabelText('表面预设')).getByRole('option', {name: '喷漆金属'})).toBeInTheDocument();
    expect(saved().materials[b].preset).toBe('painted metal');
    // A material's own value is saved under its English key.
    fireEvent.click(screen.getByLabelText('覆盖粗糙度'));
    expect(saved().materials[b].roughness).toBe(.5);
    fireEvent.click(screen.getByLabelText('我已检查此材质选择'));
    expect(saved().materials[b].confirmed).toBe(true);
    expect(screen.getAllByRole('tab')[0]).toHaveTextContent('材质 已检查');
  });
  it('keeps an accessory action as its id and switches live without remounting the 3D view', async () => {
    render(<Page project={base}/>);
    await screen.findByTestId('viewport');
    fireEvent.click(screen.getByRole('tab', {name: /^配饰与道具/}));
    fireEvent.click(screen.getByRole('button', {name: '添加配饰更改'}));
    fireEvent.click(within(screen.getByRole('radiogroup', {name: '配饰 1 的处理方式'})).getByRole('radio', {name: '隐藏'}));
    expect(saved().customization.equipment?.meshes?.[0]).toMatchObject({action: 'hide'});
    await act(() => setLanguage('en'));
    expect(screen.getByRole('tab', {name: /^Accessories & props/})).toHaveAttribute('aria-selected', 'true');
    expect(within(screen.getByRole('radiogroup', {name: 'Accessory handling 1'})).getByRole('radio', {name: 'Hide'})).toHaveAttribute('aria-checked', 'true');
    await act(() => setLanguage('zh-CN'));
    expect(screen.getByRole('tab', {name: /^配饰与道具/})).toBeInTheDocument();
    expect(probe.mounts).toBe(1);
  });
});

const parent = {...projectFixture.cases[0].expected, name: 'sample', original_model: 'models/heroes_wip/test/test.vmdl', preparation_source: 'C:\\sample.blend', vpk_number: 21, preparation_options: {}} as unknown as Project;
describe('Weapon models in Chinese', () => {
  it('finds the weapon model, creates its project under the English suggested name and asks in Chinese', async () => {
    native.discoverWeaponModels.mockResolvedValue([{path: 'models/heroes_wip/test/test_gun.vmdl', target: 'models/heroes_wip/test/test_gun.vmdl', label: 'Weapon model: test_gun.vmdl', hero: 'Test', supported: true, requires_model_project: true, kind: 'weapon'}]);
    native.newProject.mockResolvedValue({path: null, project: {...parent, id: 'child'}});
    native.selectHero.mockImplementation(async (project: Project, path: string) => ({...project, original_model: path}));
    native.save.mockResolvedValue('C:\\mods\\sample_weapon.dmb.json');
    native.saveProject.mockImplementation(async (path: string, project: Project) => ({path, project}));
    const change = vi.fn(), fail = vi.fn();
    render(<WeaponModels project={parent} variants={[]} change={change} disabled={false} fail={fail}/>);
    fireEvent.click(screen.getByText('武器模型（0）'));
    fireEvent.click(screen.getByRole('button', {name: '查找此英雄的武器模型'}));
    // The label comes from the backend and is shown as it comes.
    expect(await screen.findByRole('option', {name: /Weapon model: test_gun.vmdl/})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name: '创建武器项目'}));
    await vi.waitFor(() => expect(change).toHaveBeenCalled());
    expect(native.save).toHaveBeenCalledWith(expect.objectContaining({title: '保存武器项目', defaultPath: 'sample_weapon.dmb.json'}));
    expect((native.selectHero.mock.calls[0][0] as Project).name).toBe('sample_weapon');
    expect(fail).not.toHaveBeenCalled();
  });
  it('a failure keeps its English failure title', async () => {
    const error = {kind: 'tool', message: 'Decompile failed', details: 'Source 2 Viewer exited with code 1'};
    native.discoverWeaponModels.mockRejectedValue(error);
    const fail = vi.fn();
    render(<WeaponModels project={parent} variants={[]} change={vi.fn()} disabled={false} fail={fail}/>);
    fireEvent.click(screen.getByText('武器模型（0）'));
    fireEvent.click(screen.getByRole('button', {name: '查找此英雄的武器模型'}));
    await vi.waitFor(() => expect(fail).toHaveBeenCalledWith('Weapon models could not be listed', error));
  });
});
