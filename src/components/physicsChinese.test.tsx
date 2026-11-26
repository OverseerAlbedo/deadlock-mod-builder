import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import { inspected } from '../build/testFixtures';
import { MIRROR_KEY, setLanguage } from '../i18n';

// Physics & preview driven in Chinese: tabs, chips and choices are found by their Chinese names,
// and what the page saves (part kinds, movement choices, preset names) stays English or as typed.
const native = vi.hoisted(() => ({physicsPresets: vi.fn(), savePhysicsPreset: vi.fn(), deletePhysicsPreset: vi.fn(), validateAssetPath: vi.fn(), motionPreview: vi.fn(), previewImage: vi.fn(), fileStamps: vi.fn(), previewCloth: vi.fn()}));
vi.mock('../api', () => ({api: native, basename: (s: string) => s}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));
vi.mock('../preview/Viewport', () => ({default: () => <div data-testid="still"/>}));
vi.mock('../preview/MovementViewport', () => ({default: () => <div data-testid="movement"/>}));
import { PhysicsPage } from './CustomizationPages';
import { CharacterPreview, partLabels } from './CharacterPreview';
import { viewChoices } from '../preview/viewChoices';

const original = structuredClone(reference.cases[0].expected) as unknown as Project;
const start = {...original, preparation_options: {preserve_physics_bones: true, physics_roots: ['hair_a', 'tail_1']}, customization: {enabled: true, secondary_motion: {enabled: true}}} as unknown as Project;
function Harness({project: initial = start}: {project?: Project}) {
  const [project, update] = useState<Project>(initial);
  return <><PhysicsPage project={project} update={update} disabled={false} fail={vi.fn()} summary={null}/><output data-testid="saved">{JSON.stringify(project)}</output></>;
}
const saved = () => JSON.parse(screen.getByTestId('saved').textContent!) as Project;
const tab = (name: string) => fireEvent.click(screen.getByRole('tab', {name: new RegExp(`^${name}`)}));

beforeEach(async () => {
  vi.clearAllMocks(); viewChoices.clear();
  native.physicsPresets.mockResolvedValue([]);
  await act(() => setLanguage('zh-CN'));
});
afterEach(async () => { cleanup(); await act(() => setLanguage('en')); localStorage.removeItem(MIRROR_KEY); });

describe('Physics & preview in Chinese', () => {
  it('fine-tunes a part type by its Chinese chip and saves the tuning under the English kind', async () => {
    render(<Harness/>);
    expect(screen.getAllByRole('tab').slice(0, 4).map(item => item.textContent)).toEqual(['物理模式 自动', '微调 全部自动', '活动部件 2 个组', '已验证的布料配置 未使用']);
    expect(screen.getByRole('tab', {name: '微调 全部自动'})).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(within(screen.getByRole('tablist', {name: '要微调的部件类型'})).getByRole('tab', {name: /^尾巴/}));
    fireEvent.change(screen.getByRole('slider', {name: '尾巴硬度'}), {target: {value: '40'}});
    expect(saved().customization.secondary_motion?.tuning).toEqual({tail: {stiffness: .4}});
    expect(screen.getByRole('textbox', {name: '尾巴硬度（百分比）'})).toHaveValue('+40%');
    expect(screen.getAllByText('已更改 1 项').length).toBeGreaterThan(0);
  });
  it('saves a part type and a movement as their ids and finds a part by its Chinese type', () => {
    render(<Harness/>);
    tab('活动部件');
    fireEvent.change(screen.getByLabelText('tail_1 的部件类型'), {target: {value: 'tail'}});
    expect(within(screen.getByLabelText('tail_1 的部件类型')).getByRole('option', {name: '尾巴'})).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('hair_a 的摆动方式'), {target: {value: 'stiff'}});
    expect(saved().customization.secondary_motion).toMatchObject({part_kinds: {tail_1: 'tail'}, profiles: {hair_a: 'stiff'}});
    fireEvent.change(screen.getByRole('searchbox', {name: '查找部件'}), {target: {value: '尾巴'}});
    expect(screen.getByText('tail_1')).toBeInTheDocument();
    expect(screen.queryByText('hair_a')).toBeNull();
  });
  it('saves a preset under the name typed and reports it in Chinese', async () => {
    native.savePhysicsPreset.mockResolvedValue([{name: '我的 Soft', tuning: {hair: {damping: .2}}}]);
    render(<Harness project={{...start, customization: {enabled: true, secondary_motion: {enabled: true, tuning: {hair: {damping: .2}}}}} as unknown as Project}/>);
    fireEvent.click(await screen.findByRole('button', {name: /^另存为预设/}));
    fireEvent.change(screen.getByRole('textbox', {name: '预设名称'}), {target: {value: '我的 Soft'}});
    fireEvent.click(screen.getByRole('button', {name: '保存'}));
    await waitFor(() => expect(native.savePhysicsPreset).toHaveBeenCalledWith('我的 Soft', {hair: {damping: .2}}));
    expect(await screen.findByText('已保存“我的 Soft”。')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name: /^已保存的预设/}));
    expect(screen.getByText('我的 Soft')).toBeInTheDocument();
    expect(within(screen.getByRole('dialog',{name:'已保存的预设'})).getByText('头发')).toBeInTheDocument();
    expect(screen.getByRole('button', {name: '应用我的 Soft'})).toBeInTheDocument();
  });
  it('a failed movement preview keeps its English failure title and goes back to Still', async () => {
    const error = {kind: 'validation', message: 'Prepare the model first'};
    native.motionPreview.mockRejectedValue(error);
    const fail = vi.fn();
    render(<CharacterPreview project={{...start, dmx: 'C:\\Work\\prepared.dmx'}} preview={null} load={vi.fn()} busy={false} fail={fail}/>);
    expect(screen.getByRole('region', {name: '你的角色'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('radio', {name: '动态'}));
    await waitFor(() => expect(fail).toHaveBeenCalledWith('The movement preview could not be loaded', error));
    await waitFor(() => expect(screen.getByRole('radio', {name: '静止'})).toHaveAttribute('aria-checked', 'true'));
  });
  it('names the labels on the character in Chinese and keeps the part kinds as ids', () => {
    const node = (bone: string, root: string, kind: string) => ({bone, root, kind}) as unknown as Parameters<typeof partLabels>[0][number];
    const set = partLabels([node('h1', 'hair_l', 'hair'), node('h2', 'hair_r', 'hair'), node('t1', 'tail', 'tail')], 'tail', vi.fn())!;
    expect(set.items.map(item => [item.id, item.text, item.detail])).toEqual([['hair', '头发', '2 个部件'], ['tail', '尾巴', '1 个部件']]);
    expect(set.label).toBe('活动部件类型');
  });
  it('switches live between English and Chinese, keeping the open tab', async () => {
    const report = {...inspected, secondary_motion: {preset: 'automatic', source_sha256: 's', proxy_sha256: 'p', proxy_positions: 18, groups: [], parts: [{root: 'tail_1', kind: 'tail', reason: 'Consistent tail names', roll_locked: true, review_recommended: false}]}};
    render(<PhysicsPage project={start} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null} report={report as never} fresh/>);
    tab('物理模式');
    // The reason is the analysis's own text and stays as it comes.
    tab('活动部件');
    expect(screen.getByText(/Consistent tail names\. 已计划扭转保护。/)).toBeInTheDocument();
    await act(() => setLanguage('en'));
    expect(screen.getByRole('tab', {name: /^Moving parts/})).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByLabelText('Part type for tail_1')).toBeInTheDocument();
    await act(() => setLanguage('zh-CN'));
    expect(screen.getByRole('tab', {name: /^活动部件/})).toHaveAttribute('aria-selected', 'true');
    expect(within(screen.getByLabelText('tail_1 的部件类型')).getByRole('option', {name: '自动——尾巴'})).toBeInTheDocument();
  });
});
