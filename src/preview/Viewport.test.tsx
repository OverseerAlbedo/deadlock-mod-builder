import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Texture } from 'three';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import type { PreviewScene } from './packet';

const mocks = vi.hoisted(() => ({previewImage: vi.fn(), fileStamps: vi.fn()}));
vi.mock('../api', () => ({api: mocks, basename: (s: string) => s}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));
// WebGL cannot run here: the canvas renders nothing; the controls around it are real.
vi.mock('@react-three/fiber', () => ({Canvas: () => null, useThree: () => ({})}));
import Viewport from './Viewport';
import { refreshTextures, textureRequests, textureStore } from './textures';
import { viewChoices } from './viewChoices';

const project = {...structuredClone(reference.cases[0].expected), materials: {body: {textures: {color: 'body.png'}}}, customization: {enabled: true}} as unknown as Project;
const scene = {metadata: {schema_version: 1, source: 'my_character.dmx', source_sha256: 's', up_axis: 'Z', warnings: [], bounds: {min: [0, 0, 0], max: [1, 1, 1]}, bones: [],
  meshes: [{name: 'body', source_name: 'body', material: 'body', vertex_count: 0, index_count: 0, positions: {offset: 0, count: 0}, normals: {offset: 0, count: 0}, uvs: null, colors: null, indices: {offset: 0, count: 0}, skin: null}]}, buffer: new ArrayBuffer(0), dataOffset: 0} as unknown as PreviewScene;

describe('The still view outside the Physics page', () => {
  it('starts Solid and its Load button reuses the textures the Physics page already loaded', async () => {
    textureStore.clear();
    mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(() => ({len: 10, modified_ms: 1})));
    mocks.previewImage.mockResolvedValue(new ArrayBuffer(4));
    const stored = await refreshTextures(project.id, textureRequests(project, ['body']), async () => new Texture());
    mocks.previewImage.mockClear();
    render(<Viewport scene={scene} project={project}/>);
    const shading = screen.getByLabelText('Preview shading');
    expect(shading).toHaveValue('solid');
    fireEvent.change(shading, {target: {value: 'textured'}});
    fireEvent.click(screen.getByRole('button', {name: 'Load assigned color images'}));
    await waitFor(() => expect(mocks.fileStamps).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByRole('button', {name: 'Load assigned color images'})).toBeEnabled());
    expect(mocks.previewImage).not.toHaveBeenCalled();
    expect(textureStore.cached(project.id, textureRequests(project, ['body'])).body).toBe(stored!.textures.body);
  });
  it('keeps its own Solid default whatever the Physics page remembered, and changes nothing there', () => {
    viewChoices.clear();
    viewChoices.set(project, 'physics', {shading: 'textured', mode: 'still', packet: null, scene: null});
    const first = render(<Viewport scene={scene} project={project}/>);
    const shading = screen.getByLabelText('Preview shading');
    expect(shading).toHaveValue('solid');
    fireEvent.change(shading, {target: {value: 'textured'}});
    expect(shading).toHaveValue('textured');
    first.unmount();
    render(<Viewport scene={scene} project={project}/>);
    expect(screen.getByLabelText('Preview shading')).toHaveValue('solid');
    expect(viewChoices.get(project, 'physics')?.shading).toBe('textured');
    viewChoices.clear();
  });
});
