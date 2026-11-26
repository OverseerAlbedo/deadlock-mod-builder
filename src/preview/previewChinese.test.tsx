import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import type { PreviewScene } from './packet';
import type { MotionPacket } from './motion';
import { MIRROR_KEY, setLanguage } from '../i18n';

// The 3D views' own text in Chinese: toolbars, choices and notes; the values they keep (the
// shading choice, the clip played) stay identifiers, and the loaders' messages stay as they come.
const mocks = vi.hoisted(() => ({previewImage: vi.fn(), fileStamps: vi.fn()}));
vi.mock('../api', () => ({api: mocks, basename: (s: string) => s}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));
// WebGL cannot run here: the canvas renders nothing; the controls around it are real.
vi.mock('@react-three/fiber', () => ({Canvas: () => null, useThree: () => ({}), useFrame: () => undefined}));
import Viewport from './Viewport';
import MovementViewport from './MovementViewport';
import { textureStore } from './textures';

const project = {...structuredClone(reference.cases[0].expected), materials: {body: {textures: {color: 'body.png'}}}, customization: {enabled: true}} as unknown as Project;
const scene = {metadata: {schema_version: 1, source: 'my_character.dmx', source_sha256: 's', up_axis: 'Z', warnings: ['A note from the preview packet.'], bounds: {min: [0, 0, 0], max: [1, 1, 1]}, bones: [],
  meshes: [{name: 'body', source_name: 'body', material: 'body', vertex_count: 0, index_count: 0, positions: {offset: 0, count: 0}, normals: {offset: 0, count: 0}, uvs: null, colors: null, indices: {offset: 0, count: 0}, skin: null}]}, buffer: new ArrayBuffer(0), dataOffset: 0} as unknown as PreviewScene;
const packet = {header: {bones: [{name: 'root', parent: -1, position: [0, 0, 0]}], clips: [{name: 'jump', sources: ['jump_start'], fps: 30, frames: 2, segments: [1, 1], offset: 0}, {name: 'run', sources: ['weapon_run_n'], fps: 30, frames: 1, segments: [1], offset: 24}]}, data: new Float32Array(36)} as unknown as MotionPacket;

beforeEach(async () => { vi.clearAllMocks(); textureStore.clear(); await act(() => setLanguage('zh-CN')); });
afterEach(async () => { cleanup(); await act(() => setLanguage('en')); localStorage.removeItem(MIRROR_KEY); });

describe('The 3D views in Chinese', () => {
  it('the still view loads its colour images and keeps the shading choice as its id', async () => {
    mocks.fileStamps.mockResolvedValue([{len: 10, modified_ms: 1}]);
    mocks.previewImage.mockRejectedValue(new Error('body.png is not a PNG or JPEG image.'));
    render(<Viewport scene={scene} project={project}/>);
    const shading = screen.getByLabelText('预览着色');
    fireEvent.change(shading, {target: {value: 'textured'}});
    expect(shading).toHaveValue('textured');
    expect(within(shading).getByRole('option', {name: '映射的贴图'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name: '加载已分配的颜色图片'}));
    // The loader's own message is shown as it comes.
    expect(await screen.findByText('body: body.png is not a PNG or JPEG image.')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', {name: '加载已分配的颜色图片'})).toBeEnabled());
    expect(screen.getByText('A note from the preview packet.')).toBeInTheDocument();
  });
  it('the still view says in Chinese when no image could be loaded', async () => {
    const plain = {...project, materials: {body: {}}} as unknown as Project;
    mocks.fileStamps.mockResolvedValue([]);
    render(<Viewport scene={scene} project={plain}/>);
    fireEvent.change(screen.getByLabelText('预览着色'), {target: {value: 'textured'}});
    fireEvent.click(screen.getByRole('button', {name: '加载已分配的颜色图片'}));
    expect(await screen.findByText('无法加载任何已分配的颜色图片。请先分配 PNG 或 JPEG 颜色贴图。')).toBeInTheDocument();
  });
  it('the movement view names its motions in Chinese, plays them by clip name and switches live', async () => {
    render(<MovementViewport scene={scene} packet={packet} nodes={[]} tuning={{}}/>);
    const motions = within(screen.getByRole('group', {name: '动画'}));
    expect(motions.getAllByRole('button').map(button => button.textContent)).toEqual(['奔跑', '跳跃']);
    expect(motions.getByRole('button', {name: '奔跑'})).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText(/以接近英雄的速度奔跑，地面从脚下经过；这是对游戏布料的近似：请在游戏中检查结果。/)).toBeInTheDocument();
    fireEvent.click(motions.getByRole('button', {name: '跳跃'}));
    expect(motions.getByRole('button', {name: '跳跃'})).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText(/一次约 1.4 米的跳跃/)).toBeInTheDocument();
    await act(() => setLanguage('en'));
    expect(screen.getByRole('button', {name: 'Jump'})).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText(/A jump of about 1.4 m, landing into the stand; an approximation of the game's cloth/)).toBeInTheDocument();
    await act(() => setLanguage('zh-CN'));
    expect(screen.getByRole('button', {name: '暂停'})).toBeInTheDocument();
  });
});
