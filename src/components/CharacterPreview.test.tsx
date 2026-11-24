import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import type { PreviewScene } from '../preview/packet';

const mocks = vi.hoisted(() => ({motionPreview: vi.fn(), previewImage: vi.fn(), fileStamps: vi.fn(), previewCloth: vi.fn()}));
vi.mock('../api', () => ({api: mocks, basename: (s: string) => s}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));
// WebGL cannot run here: the views show what they are given.
type Shared = {shading?: string; onShading?: (mode: string) => void; shared?: {status: string; textures: Record<string, unknown>}; cloth?: {proxy_sha256: string} | null; clothNote?: {text: string} | null};
const stub = (id: string) => ({default: ({shading, onShading, shared, cloth, clothNote}: Shared) => <div data-testid={id}><span data-testid={`${id}-look`}>{shading}</span> {shared?.status} {Object.keys(shared?.textures ?? {}).join(',')}<button onClick={() => onShading?.('solid')}>pick solid</button><button onClick={() => onShading?.('textured')}>pick textured</button>{cloth && <span data-testid={`${id}-cloth`}>{cloth.proxy_sha256}</span>}{clothNote && <span data-testid={`${id}-cloth-note`}>{clothNote.text}</span>}</div>});
vi.mock('../preview/Viewport', () => stub('still'));
vi.mock('../preview/MovementViewport', () => stub('movement'));
import { CharacterPreview } from './CharacterPreview';
import { PhysicsPage } from './CustomizationPages';
import { inspected } from '../build/testFixtures';
import type { ProjectInspection } from '../build/models';
import { textureStore } from '../preview/textures';
import { viewChoices } from '../preview/viewChoices';
import { STOPPED_BEFORE_MOVING_PARTS } from './previewLink';

// The remembered choices outlive a component: every test starts from a new session.
beforeEach(() => viewChoices.clear());

const project = {...structuredClone(reference.cases[0].expected), dmx: 'C:\Work\prepared.dmx'} as unknown as Project;
const packet = () => {
  const header = new TextEncoder().encode(JSON.stringify({bones: [{name: 'root', parent: -1, position: [0, 0, 0]}], clips: [{name: 'run', sources: ['weapon_run_n'], fps: 30, frames: 1, segments: [1], offset: 0}]}));
  const buffer = new ArrayBuffer(Math.ceil((4 + header.length) / 4) * 4 + 48);
  new DataView(buffer).setUint32(0, header.length, true);
  new Uint8Array(buffer, 4).set(header);
  return buffer;
};

describe('Your character', () => {
  it('asks for the clips only after the character load and the Analyze it started have finished', async () => {
    mocks.motionPreview.mockReset();
    mocks.motionPreview.mockResolvedValue(packet());
    const order: string[] = [];
    let shown = () => {}, analyzed = () => {};
    const load = vi.fn(() => new Promise<void>(resolve => { shown = () => { order.push('character'); resolve(); }; }));
    const analyze = vi.fn(() => new Promise<void>(resolve => { analyzed = () => { order.push('analysis'); resolve(); }; }));
    mocks.motionPreview.mockImplementation(async () => { order.push('clips'); return packet(); });
    render(<CharacterPreview project={project} preview={null} load={load} busy={false} analyze={analyze}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    // Nothing else starts while the character loads.
    expect(load).toHaveBeenCalledTimes(1);
    expect(analyze).not.toHaveBeenCalled();
    await act(async () => { shown(); });
    expect(analyze).toHaveBeenCalledTimes(1);
    expect(mocks.motionPreview).not.toHaveBeenCalled();
    await act(async () => { analyzed(); });
    await waitFor(() => expect(mocks.motionPreview).toHaveBeenCalledTimes(1));
    expect(order).toEqual(['character', 'analysis', 'clips']);
  });
  it('loads the character and the hero clips for the movement view', async () => {
    mocks.motionPreview.mockResolvedValue(packet());
    const load = vi.fn();
    render(<CharacterPreview project={project} preview={null} load={load} busy={false}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(load).toHaveBeenCalled();
    await waitFor(() => expect(mocks.motionPreview).toHaveBeenCalledWith(project));
    expect(screen.getByText(/Loading your character and the hero’s stand, run and jump clips/)).toBeInTheDocument();
  });
  it('goes back to the still view when the clips cannot be read', async () => {
    mocks.motionPreview.mockRejectedValue({kind: 'validation', message: 'Prepare the model first'});
    const fail = vi.fn();
    render(<CharacterPreview project={project} preview={null} load={vi.fn()} busy={false} fail={fail}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    await waitFor(() => expect(fail).toHaveBeenCalledWith('The movement preview could not be loaded', expect.anything()));
    expect(screen.getByRole('radio', {name: 'Still'})).toHaveAttribute('aria-checked', 'true');
  });
});

describe('Texture colours in the character preview', () => {
  beforeEach(() => {
    textureStore.clear();
    mocks.previewImage.mockReset();
    mocks.fileStamps.mockReset();
    mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(() => ({len: 10, modified_ms: 1})));
    HTMLImageElement.prototype.decode = () => Promise.resolve();
    URL.createObjectURL ??= () => 'blob:preview';
    URL.revokeObjectURL ??= () => undefined;
  });
  it('shows the stored textures at once when the page is opened again, reading no image', async () => {
    mocks.previewImage.mockResolvedValue(new ArrayBuffer(4));
    const textured = {...project, materials: {body: {textures: {color: 'body.png'}}}} as unknown as Project;
    const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
    const first = render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    await waitFor(() => expect(screen.getByTestId('still')).toHaveTextContent('textured ready body'));
    first.unmount();
    // Leaving the Physics page unmounts the preview; coming back mounts a new one.
    render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still')).toHaveTextContent('textured ready body');
    await waitFor(() => expect(mocks.fileStamps).toHaveBeenCalledTimes(2));
    expect(mocks.previewImage).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('Loading textures…')).toBeNull();
  });
  it('starts textured, loads the images once and keeps them and the choice across Still and Movement', async () => {
    HTMLImageElement.prototype.decode = () => Promise.resolve();
    URL.createObjectURL ??= () => 'blob:preview';
    URL.revokeObjectURL ??= () => undefined;
    mocks.motionPreview.mockResolvedValue(packet());
    mocks.previewImage.mockResolvedValue(new ArrayBuffer(4));
    const textured = {...project, materials: {body: {textures: {color: 'body.png'}}}} as unknown as Project;
    const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
    render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still')).toHaveTextContent('textured');
    await waitFor(() => expect(screen.getByTestId('still')).toHaveTextContent('textured ready body'));
    fireEvent.click(screen.getByRole('button', {name: 'pick solid'}));
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByTestId('movement')).toHaveTextContent('solid ready body');
    fireEvent.click(screen.getByRole('button', {name: 'pick textured'}));
    fireEvent.click(screen.getByRole('radio', {name: 'Still'}));
    expect(await screen.findByTestId('still')).toHaveTextContent('textured ready body');
    expect(mocks.previewImage).toHaveBeenCalledTimes(1);
    expect(mocks.previewImage).toHaveBeenCalledWith('body.png', 1024);
  });
  it('stays solid without colour images', async () => {
    const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
    render(<CharacterPreview project={{...project, materials: {body: {textures: {}}}} as unknown as Project} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still')).toHaveTextContent('solid idle');
    expect(mocks.previewImage).not.toHaveBeenCalled();
  });
});

describe('Returning to the character preview', () => {
  const textured = {...project, materials: {body: {textures: {color: 'body.png'}}}} as unknown as Project;
  const plain = {...project, materials: {body: {textures: {}}}} as unknown as Project;
  const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
  const checked = (name: string) => expect(screen.getByRole('radio', {name})).toHaveAttribute('aria-checked', 'true');
  beforeEach(() => {
    textureStore.clear();
    mocks.motionPreview.mockReset();
    mocks.previewImage.mockReset();
    mocks.fileStamps.mockReset();
    mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(() => ({len: 10, modified_ms: 1})));
    mocks.previewImage.mockResolvedValue(new ArrayBuffer(4));
    HTMLImageElement.prototype.decode = () => Promise.resolve();
    URL.createObjectURL ??= () => 'blob:preview';
    URL.revokeObjectURL ??= () => undefined;
  });
  /** Opens Movement, picks Solid there and leaves the page. */
  async function leaveInMovement(shown: Project = textured) {
    mocks.motionPreview.mockResolvedValue(packet());
    const first = render(<CharacterPreview project={shown} preview={scene} load={vi.fn()} busy={false} analyze={vi.fn()}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByTestId('movement')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name: 'pick solid'}));
    expect(screen.getByTestId('movement-look')).toHaveTextContent(/^solid$/);
    first.unmount();
    mocks.motionPreview.mockClear();
    mocks.previewImage.mockClear();
    mocks.fileStamps.mockClear();
  }

  it('shows Movement and the chosen look again without loading clips or starting an Analyze', async () => {
    await leaveInMovement();
    const load = vi.fn(), analyze = vi.fn();
    render(<CharacterPreview project={textured} preview={scene} load={load} busy={false} analyze={analyze}/>);
    expect(screen.getByTestId('movement-look')).toHaveTextContent(/^solid$/);
    checked('Movement');
    // Not analyzed: the notice offers the Analyze, it does not start one.
    expect(screen.getByRole('button', {name: 'Analyze now'})).toBeInTheDocument();
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
    expect(mocks.motionPreview).not.toHaveBeenCalled();
    expect(analyze).not.toHaveBeenCalled();
    expect(load).not.toHaveBeenCalled();
    expect(mocks.previewImage).not.toHaveBeenCalled();
    // Solid was chosen: not even the texture check runs.
    expect(mocks.fileStamps).not.toHaveBeenCalled();
  });
  it('drops clips that arrive after the prepared model changed', async () => {
    let resolve: (value: ArrayBuffer) => void = () => undefined;
    mocks.motionPreview.mockReturnValueOnce(new Promise<ArrayBuffer>(done => { resolve = done; }));
    const view = render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    const changed = {...textured, dmx: 'C:\\Work\\prepared again.dmx'} as Project;
    view.rerender(<CharacterPreview project={changed} preview={scene} load={vi.fn()} busy={false}/>);
    await act(async () => { resolve(packet()); await Promise.resolve(); });
    checked('Still');
    expect(viewChoices.get(changed, 'physics')?.packet ?? null).toBeNull();
    mocks.motionPreview.mockResolvedValue(packet());
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    await waitFor(() => expect(mocks.motionPreview).toHaveBeenCalledTimes(2));
    expect(mocks.motionPreview).toHaveBeenLastCalledWith(changed);
  });
  it('reuses the held clips when Movement is opened again from Still', async () => {
    await leaveInMovement();
    render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Still'}));
    expect(await screen.findByTestId('still-look')).toHaveTextContent(/^solid$/);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByTestId('movement-look')).toHaveTextContent(/^solid$/);
    expect(mocks.motionPreview).not.toHaveBeenCalled();
  });
  it('keeps Still and the chosen look', async () => {
    const first = render(<CharacterPreview project={plain} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still-look')).toHaveTextContent(/^solid$/);
    fireEvent.click(screen.getByRole('button', {name: 'pick textured'}));
    first.unmount();
    render(<CharacterPreview project={plain} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still-look')).toHaveTextContent(/^textured$/);
    checked('Still');
  });
  it('shows Still, Movement one click away, when the preview was loaded again in the meantime', async () => {
    await leaveInMovement();
    const reloaded = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
    const analyze = vi.fn();
    render(<CharacterPreview project={textured} preview={reloaded} load={vi.fn()} busy={false} analyze={analyze}/>);
    expect(await screen.findByTestId('still-look')).toHaveTextContent(/^solid$/);
    checked('Still');
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
    expect(mocks.motionPreview).not.toHaveBeenCalled();
    expect(analyze).not.toHaveBeenCalled();
    mocks.motionPreview.mockResolvedValue(packet());
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    await waitFor(() => expect(mocks.motionPreview).toHaveBeenCalledTimes(1));
  });
  it('shows Still without a preview to show the movement on', async () => {
    await leaveInMovement();
    const load = vi.fn();
    render(<CharacterPreview project={textured} preview={null} load={load} busy={false}/>);
    checked('Still');
    expect(load).not.toHaveBeenCalled();
    expect(mocks.motionPreview).not.toHaveBeenCalled();
  });
  it('starts from the defaults for another prepared model or another project', async () => {
    await leaveInMovement();
    const other = render(<CharacterPreview project={{...textured, dmx: 'C:\\Work\\prepared again.dmx'}} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still-look')).toHaveTextContent(/^textured$/);
    checked('Still');
    other.unmount();
    // Back to the first model: its choices were replaced by the other model's defaults.
    render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still-look')).toHaveTextContent(/^textured$/);
    checked('Still');
    expect(mocks.motionPreview).not.toHaveBeenCalled();
  });
  it('starts from the defaults for a new preparation and for another project', async () => {
    await leaveInMovement();
    const view = render(<CharacterPreview project={{...textured, preparation_report: 'C:\\Work\\run 2\\report.json'}} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still-look')).toHaveTextContent(/^textured$/);
    view.unmount();
    await leaveInMovement();
    render(<CharacterPreview project={{...textured, id: 'another-project'}} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still-look')).toHaveTextContent(/^textured$/);
    checked('Still');
    expect(mocks.motionPreview).not.toHaveBeenCalled();
  });
  it('forgets everything when the store is cleared (another project opened or the project closed)', async () => {
    await leaveInMovement();
    viewChoices.clear();
    render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    expect(await screen.findByTestId('still-look')).toHaveTextContent(/^textured$/);
    checked('Still');
  });
  it('shows Still when the page was left before the clips arrived', async () => {
    let resolve: (value: ArrayBuffer) => void = () => undefined;
    mocks.motionPreview.mockReturnValue(new Promise<ArrayBuffer>(done => { resolve = done; }));
    const first = render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    first.unmount();
    await act(async () => { resolve(packet()); await Promise.resolve(); });
    mocks.motionPreview.mockClear();
    render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    checked('Still');
    expect(viewChoices.get(textured, 'physics')?.packet ?? null).toBeNull();
    expect(mocks.motionPreview).not.toHaveBeenCalled();
  });
  it('still falls back to Solid when the textures cannot be loaded, also after coming back', async () => {
    mocks.previewImage.mockRejectedValue(new Error('missing body.png'));
    const first = render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    await waitFor(() => expect(screen.getByTestId('still-look')).toHaveTextContent(/^solid$/));
    expect(screen.getByText(/Showing Solid: the textures could not be loaded/)).toBeInTheDocument();
    first.unmount();
    render(<CharacterPreview project={textured} preview={scene} load={vi.fn()} busy={false}/>);
    await waitFor(() => expect(screen.getByText(/Showing Solid: the textures could not be loaded/)).toBeInTheDocument());
    expect(screen.getByTestId('still-look')).toHaveTextContent(/^solid$/);
    // Unset stayed unset: choosing Textures is still possible.
    fireEvent.click(screen.getByRole('button', {name: 'pick textured'}));
    expect(screen.getByTestId('still-look')).toHaveTextContent(/^textured$/);
  });
});

describe('The line above the Fine-tune sliders, with the real preview', () => {
  it('follows the preview: See it move opens Movement, and the line goes quiet once it moves', async () => {
    mocks.motionPreview.mockResolvedValue(packet());
    mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(() => ({len: 10, modified_ms: 1})));
    const automatic = {...project, preparation_options: {preserve_physics_bones: true, physics_roots: ['hair']}, customization: {enabled: true, secondary_motion: {enabled: true}}} as unknown as Project;
    const nodes = [{bone: 'hair_1', root: 'hair', kind: 'hair', goal: .5, drag: .1, gravity: 0, travel: 0}];
    const report = {...inspected, secondary_motion: {preset: 'automatic', source_sha256: 's', proxy_sha256: 'p', proxy_positions: 1, groups: [], preview_nodes: nodes}} as unknown as ProjectInspection;
    const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
    const analyze = vi.fn();
    render(<PhysicsPage project={automatic} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null} fresh report={report}
      preview={<CharacterPreview project={automatic} preview={scene} load={vi.fn()} busy={false} report={report} analyze={analyze}/>}/>);
    expect((await screen.findAllByText('The sliders show in Movement.'))[0]).toBeVisible();
    fireEvent.click(screen.getByRole('button', {name: 'See it move'}));
    expect(screen.getByRole('radio', {name: 'Movement'})).toHaveAttribute('aria-checked', 'true');
    await waitFor(() => expect(mocks.motionPreview).toHaveBeenCalledWith(automatic));
    expect(await screen.findAllByText('1 moving bone follows your sliders.')).not.toHaveLength(0);
    expect(analyze).not.toHaveBeenCalled();
    // Back on Still the line asks again; nothing switches by itself.
    fireEvent.click(screen.getByRole('radio', {name: 'Still'}));
    expect(await screen.findByRole('button', {name: 'See it move'})).toBeVisible();
  });
  it('says that the last Analyze stopped before the moving parts while mesh mappings need a choice, starting nothing', async () => {
    mocks.motionPreview.mockResolvedValue(packet());
    mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(() => ({len: 10, modified_ms: 1})));
    const automatic = {...project, preparation_options: {preserve_physics_bones: true, physics_roots: ['hair']}, customization: {enabled: true, secondary_motion: {enabled: true}}} as unknown as Project;
    const report = {...inspected, secondary_motion: undefined, motion_preview_nodes: undefined, unresolved_meshes: ['face']} as unknown as ProjectInspection;
    const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
    const analyze = vi.fn(), next = vi.fn();
    render(<PhysicsPage project={automatic} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null} fresh report={report} next={next}
      preview={<CharacterPreview project={automatic} preview={scene} load={vi.fn()} busy={false} report={report} analyze={analyze}/>}/>);
    expect((await screen.findAllByText(STOPPED_BEFORE_MOVING_PARTS)).length).toBeGreaterThan(0);
    expect(screen.queryByText(/recorded nothing that swings/)).toBeNull();
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByText('Mesh mappings needed a choice')).toBeVisible();
    expect(screen.queryByText('No moving parts to swing')).toBeNull();
    expect(screen.getByRole('button', {name: 'Analyze again'})).toBeVisible();
    expect(analyze).not.toHaveBeenCalled();
    expect(next).not.toHaveBeenCalled();
  });
  it('says nothing untrue once the mesh mappings are set while the stopped Analyze is still the one held', async () => {
    mocks.motionPreview.mockResolvedValue(packet());
    mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(() => ({len: 10, modified_ms: 1})));
    // The mappings are set now, but no Analyze has run since: the held report still lists the mesh.
    const automatic = {...project, meshes: {face: 'Face'}, preparation_options: {preserve_physics_bones: true, physics_roots: ['hair']}, customization: {enabled: true, secondary_motion: {enabled: true}}} as unknown as Project;
    const report = {...inspected, secondary_motion: undefined, motion_preview_nodes: undefined, unresolved_meshes: ['face']} as unknown as ProjectInspection;
    const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
    render(<PhysicsPage project={automatic} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null} fresh={false} report={report} next={vi.fn()}
      preview={<CharacterPreview project={automatic} preview={scene} load={vi.fn()} busy={false} report={report} analyze={vi.fn()}/>}/>);
    const lines = await screen.findAllByText(STOPPED_BEFORE_MOVING_PARTS);
    expect(lines.length).toBeGreaterThan(0);
    // It says what the last Analyze met and what brings the parts, never that a choice is still open.
    for (const line of lines) {
      expect(line.textContent).toContain('mesh mappings on Build & install needed a choice');
      expect(line.textContent).toContain('after those are set and Analyze has run again');
      expect(line.textContent).not.toMatch(/still need|need a choice/);
    }
  });
  it('keeps the earlier notice in a physics mode that plans no moving parts, whatever the mesh mappings are', async () => {
    mocks.motionPreview.mockResolvedValue(packet());
    mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(() => ({len: 10, modified_ms: 1})));
    // The hero's own physics are kept: automatic secondary motion is not chosen.
    const preserved = {...project, customization: {enabled: true}} as unknown as Project;
    const report = {...inspected, secondary_motion: undefined, motion_preview_nodes: undefined, unresolved_meshes: ['face']} as unknown as ProjectInspection;
    const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
    render(<PhysicsPage project={preserved} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null} fresh report={report} next={vi.fn()}
      preview={<CharacterPreview project={preserved} preview={scene} load={vi.fn()} busy={false} report={report} analyze={vi.fn()}/>}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByText('No moving parts to swing')).toBeVisible();
    expect(screen.queryByText(STOPPED_BEFORE_MOVING_PARTS)).toBeNull();
    expect(screen.queryByText('Mesh mappings needed a choice')).toBeNull();
  });
});

describe('The game cloth model in Movement', () => {
  const nodes = [{bone: 'hair_1', root: 'hair', kind: 'hair', goal: .5, drag: .1, gravity: 0, travel: 0}];
  const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
  const plain = {...project, materials: {body: {textures: {}}}} as unknown as Project;
  const reportFor = (preset: string, proxy = 'p') => ({...inspected, secondary_motion: {preset, source_sha256: 's', proxy_sha256: proxy, proxy_positions: 1, groups: [], preview_nodes: nodes}} as unknown as ProjectInspection);
  const cloth = {version: 1, source_sha256: 's', proxy_sha256: 'p', bones: [{name: 'root', bind: [0, 0, 0, 1, 0, 0, 0, 1]}], static_nodes: 1, positions: [[0, 0, 0], [0, 0, -1]], node_bones: [0, 0], inv_mass: [0, 1], layers: [65535, 65535], motion: [{kind: 'hair', goal: .5, drag: 0, gravity: 0, travel: 0}], rods: {a: [0], b: [1], min: [1], max: [1], weight0: [0]}, colliders: [], bases: []};
  beforeEach(() => { mocks.motionPreview.mockReset(); mocks.previewCloth.mockReset(); mocks.motionPreview.mockResolvedValue(packet()); mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(() => ({len: 10, modified_ms: 1}))); });
  it('asks for the packet of the analyzed proxy when Movement is shown and hands it to the view', async () => {
    mocks.previewCloth.mockResolvedValue(JSON.stringify(cloth));
    render(<CharacterPreview project={plain} preview={scene} load={vi.fn()} busy={false} report={reportFor('automatic')}/>);
    expect(mocks.previewCloth).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByTestId('movement-cloth')).toHaveTextContent('p');
    expect(mocks.previewCloth).toHaveBeenCalledWith(plain, 'p');
    expect(screen.queryByTestId('movement-cloth-note')).toBeNull();
    // Still and back: the held packet is used again, not read again.
    fireEvent.click(screen.getByRole('radio', {name: 'Still'}));
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByTestId('movement-cloth')).toBeInTheDocument();
    expect(mocks.previewCloth).toHaveBeenCalledTimes(1);
  });
  it('reads the packet once per Analyze: a slider change re-renders without reading it again, a new Analyze reads it again', async () => {
    mocks.previewCloth.mockResolvedValue(JSON.stringify(cloth));
    const report = reportFor('automatic');
    const view = render(<CharacterPreview project={plain} preview={scene} load={vi.fn()} busy={false} report={report}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByTestId('movement-cloth')).toBeInTheDocument();
    const tuned = {...plain, customization: {...plain.customization, secondary_motion: {enabled: true, tuning: {hair: {stiffness: .5}}}}} as unknown as Project;
    view.rerender(<CharacterPreview project={tuned} preview={scene} load={vi.fn()} busy={false} report={report}/>);
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
    expect(mocks.previewCloth).toHaveBeenCalledTimes(1);
    view.rerender(<CharacterPreview project={tuned} preview={scene} load={vi.fn()} busy={false} report={reportFor('automatic')}/>);
    await waitFor(() => expect(mocks.previewCloth).toHaveBeenCalledTimes(2));
    // The same packet again keeps playing (the view keeps its packet the whole time).
    expect(screen.getByTestId('movement-cloth')).toBeInTheDocument();
  });
  it('keeps the simple swing with one quiet line when there is no packet, and never shows an error', async () => {
    mocks.previewCloth.mockResolvedValue(null);
    const fail = vi.fn();
    const view = render(<CharacterPreview project={plain} preview={scene} load={vi.fn()} busy={false} report={reportFor('automatic')} fail={fail} analyze={vi.fn()}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByTestId('movement-cloth-note')).toHaveTextContent('Analyze again to see the game’s cloth simulation.');
    // A failed read is a state too.
    mocks.previewCloth.mockRejectedValue(new Error('broken'));
    view.rerender(<CharacterPreview project={plain} preview={scene} load={vi.fn()} busy={false} report={reportFor('automatic', 'q')} fail={fail} analyze={vi.fn()}/>);
    expect(await screen.findByText(/could not be read/)).toBeInTheDocument();
    expect(fail).not.toHaveBeenCalled();
    // Another physics mode never asks.
    mocks.previewCloth.mockClear();
    view.rerender(<CharacterPreview project={plain} preview={scene} load={vi.fn()} busy={false} report={reportFor('tangtang')} fail={fail}/>);
    expect(await screen.findByText(/shown for Automatic secondary motion/)).toBeInTheDocument();
    expect(mocks.previewCloth).not.toHaveBeenCalled();
  });
});

describe('Load 3D previews automatically on Physics & preview', () => {
  const settle = () => act(async () => { await new Promise(resolve => setTimeout(resolve, 150)); });
  const scene = {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
  const plain = {...project, materials: {body: {textures: {}}}} as unknown as Project;
  beforeEach(() => { mocks.motionPreview.mockReset(); mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(() => ({len: 10, modified_ms: 1}))); });
  /** Movement with held clips, then the page left; the preview is loaded again in the meantime. */
  async function leftInMovement() {
    mocks.motionPreview.mockResolvedValue(packet());
    const first = render(<CharacterPreview project={plain} preview={scene} load={vi.fn()} busy={false}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(await screen.findByTestId('movement')).toBeInTheDocument();
    first.unmount();
    mocks.motionPreview.mockClear();
    return {metadata: {meshes: [{material: 'body'}]}} as unknown as PreviewScene;
  }
  it('loads nothing by itself when the setting is off', async () => {
    const load = vi.fn();
    render(<CharacterPreview project={plain} preview={null} load={load} busy={false}/>);
    await settle();
    expect(load).not.toHaveBeenCalled();
    expect(screen.getByRole('button', {name: 'Show my character'})).toBeInTheDocument();
  });
  it('loads the character once when nothing else runs, and not again for the same model', async () => {
    const autoLoad = vi.fn(async () => true), load = vi.fn();
    const view = render(<CharacterPreview project={plain} preview={null} load={load} busy autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).not.toHaveBeenCalled();
    view.rerender(<CharacterPreview project={plain} preview={null} load={load} busy={false} autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).toHaveBeenCalledOnce();
    view.rerender(<CharacterPreview project={plain} preview={null} load={load} busy autoLoad={autoLoad}/>);
    view.rerender(<CharacterPreview project={plain} preview={null} load={load} busy={false} autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).toHaveBeenCalledOnce();
    // Not the button's load, and Movement is not opened.
    expect(load).not.toHaveBeenCalled();
    expect(mocks.motionPreview).not.toHaveBeenCalled();
    // Another prepared model is another input.
    view.rerender(<CharacterPreview project={{...plain, dmx: 'C:\\Work\\prepared again.dmx'}} preview={null} load={load} busy={false} autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).toHaveBeenCalledTimes(2);
  });
  it('starts nothing when the page is left before the load begins', async () => {
    const autoLoad = vi.fn(async () => true);
    const view = render(<CharacterPreview project={plain} preview={null} load={vi.fn()} busy={false} autoLoad={autoLoad}/>);
    view.unmount();
    await settle();
    expect(autoLoad).not.toHaveBeenCalled();
  });
  it('opens Movement again when it was the last choice, loading the clips once and starting no Analyze', async () => {
    const reloaded = await leftInMovement();
    const analyze = vi.fn(), autoLoad = vi.fn(async () => true);
    const view = render(<CharacterPreview project={plain} preview={reloaded} load={vi.fn()} busy autoLoad={autoLoad} analyze={analyze}/>);
    await settle();
    expect(mocks.motionPreview).not.toHaveBeenCalled();
    view.rerender(<CharacterPreview project={plain} preview={reloaded} load={vi.fn()} busy={false} autoLoad={autoLoad} analyze={analyze}/>);
    expect(await screen.findByTestId('movement')).toBeInTheDocument();
    expect(mocks.motionPreview).toHaveBeenCalledOnce();
    expect(analyze).not.toHaveBeenCalled();
    expect(autoLoad).not.toHaveBeenCalled();
    // Back to Still by hand: Movement does not come back by itself.
    fireEvent.click(screen.getByRole('radio', {name: 'Still'}));
    view.rerender(<CharacterPreview project={plain} preview={reloaded} load={vi.fn()} busy autoLoad={autoLoad} analyze={analyze}/>);
    view.rerender(<CharacterPreview project={plain} preview={reloaded} load={vi.fn()} busy={false} autoLoad={autoLoad} analyze={analyze}/>);
    await settle();
    expect(screen.getByRole('radio', {name: 'Still'})).toHaveAttribute('aria-checked', 'true');
    expect(mocks.motionPreview).toHaveBeenCalledOnce();
  });
  it('keeps Still without the setting when the preview was loaded again (Movement one click away)', async () => {
    const reloaded = await leftInMovement();
    render(<CharacterPreview project={plain} preview={reloaded} load={vi.fn()} busy={false}/>);
    await settle();
    expect(screen.getByRole('radio', {name: 'Still'})).toHaveAttribute('aria-checked', 'true');
    expect(mocks.motionPreview).not.toHaveBeenCalled();
  });
});
