import { useEffect, useState } from 'react';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppearanceMaterial, Project } from '../models';
import type { PreviewScene } from '../preview/packet';
import { reviewedProject } from '../build/testFixtures';
import { revealSection } from './workflowNavigation';
import { addSurface, colorsEdits, freeRows, removeSurface, setSurface } from './surfaceRows';

const probe = vi.hoisted(() => ({mounts: 0, props: null as Record<string, unknown> | null}));
vi.mock('../preview/Viewport', () => ({default: (props: Record<string, unknown>) => {
  probe.props = props;
  useEffect(() => { probe.mounts++; }, []);
  return <div data-testid="viewport"/>;
}}));
vi.mock('./ImagePreview', () => ({ImagePreview: ({path}: {path: string}) => <span data-testid="texture-preview">{path}</span>}));
vi.mock('../api', async original => ({...await original<typeof import('../api')>(), api: {validateAssetPath: vi.fn(async (path: string) => path), previewImage: vi.fn(), fileStamps: vi.fn()}}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn(), save: vi.fn()}));
import { Appearance } from './Appearance';

const a = 'models/test/materials/outfit.vmat', b = 'models/test/materials/hair.vmat', c = 'models/test/materials/legs.vmat';
const base = {...reviewedProject, original_model: 'models/heroes_wip/test/test.vmdl',
  materials: {[a]: {mode: 'generate', confirmed: true, textures: {color: 'C:\\t\\outfit.png'}}, [b]: {mode: 'generate', confirmed: false, textures: {}}, [c]: {mode: 'generate', confirmed: false, textures: {}}}} as unknown as Project;
/** A preview packet with one vertex per material (so each gets a label point). */
function scene(): PreviewScene {
  const names = [a, b, c];
  const buffer = new ArrayBuffer(names.length * 6 * 4 + 4 * names.length);
  const floats = new Float32Array(buffer, 0, names.length * 6);
  names.forEach((_, i) => floats.set([i, 0, i * 10, 0, 0, 1], i * 6));
  const meshes = names.map((material, i) => ({name: `m${i}`, source_name: `m${i}`, material, vertex_count: 1, index_count: 0, positions: {offset: i * 24, count: 3}, normals: {offset: i * 24 + 12, count: 3}, uvs: null, colors: null, indices: {offset: names.length * 24 + i * 4, count: 0}, skin: null}));
  return {metadata: {schema_version: 1, source: 'my_character.dmx', source_sha256: 's', up_axis: 'Z', warnings: [], bounds: {min: [0, 0, 0], max: [2, 1, 20]}, bones: [{name: 'head', parent: -1, position: [0, 0, 18]}], meshes}, buffer, dataOffset: 0} as unknown as PreviewScene;
}
function Page({project: initial, changes = vi.fn(), ...rest}: {project: Project; changes?: (project: Project) => void} & Partial<React.ComponentProps<typeof Appearance>>) {
  const [project, setProject] = useState(initial);
  return <Appearance document={{path: null, project}} summary={null} choose={vi.fn()} analyze={vi.fn()} busy={false} drag={null} update={next => { changes(next); setProject(next); }} fail={vi.fn()} preview={null} loadPreview={vi.fn()} importPackage={vi.fn()} analyzeMaterials={vi.fn()} {...rest}/>;
}
const rows = (project: Project) => project.customization.appearance?.materials ?? [];
beforeEach(() => { probe.mounts = 0; probe.props = null; });

describe('Appearance & weapons in the studio frame', () => {
  it('has four tabs and the four top-bar menus, and keeps the one 3D view mounted across tabs', async () => {
    render(<Page project={base} preview={scene()}/>);
    await screen.findByTestId('viewport');
    expect(screen.getAllByRole('tab').map(tab => tab.textContent)).toEqual(['Materials 2 to review', 'Face nothing to set', 'Gun as in the model', 'Accessories & props none']);
    for (const name of ['Model', 'Mesh options', 'Texture folder', 'Saved inputs']) expect(screen.getByRole('button', {name})).toHaveAttribute('aria-expanded', 'false');
    for (const name of [/^Face/, /^Gun/, /^Accessories/, /^Materials/, /^Gun/]) fireEvent.click(screen.getByRole('tab', {name}));
    expect(probe.mounts).toBe(1);
    // Without a prepared gun (no preview of its own) the Gun tab keeps the page's view beside it.
    expect(screen.getByRole('region', {name: 'Appearance & weapons'})).toHaveClass('tab-gun-plain');
    expect(screen.getByTestId('viewport')).toBeInTheDocument();
    // Picking happens on the Materials tab only, and the Face tab frames the head.
    expect(probe.props!.onPick).toBeUndefined();
    fireEvent.click(screen.getByRole('tab', {name: /^Face/}));
    expect(probe.props!.frame).toMatchObject({center: [0, 0, expect.any(Number)]});
    fireEvent.click(screen.getByRole('button', {name: 'Whole model'}));
    expect(probe.props!.frame).toBeNull();
    expect(probe.mounts).toBe(1);
  });
  it('puts a prepared gun (with its own preview) in place of the view, which stays mounted', async () => {
    render(<Page project={{...base, preparation_report: 'C:\\w\\report.json'}} preview={scene()} weapons={<div data-testid="gun-panel"/>}/>);
    await screen.findByTestId('viewport');
    fireEvent.click(screen.getByRole('tab', {name: /^Gun/}));
    expect(screen.getByRole('region', {name: 'Appearance & weapons'})).toHaveClass('tab-gun');
    expect(screen.getByTestId('gun-panel')).toBeVisible();
    fireEvent.click(screen.getByRole('tab', {name: /^Materials/}));
    expect(screen.getByTestId('viewport')).toBeInTheDocument();
    expect(probe.mounts).toBe(1);
  });
  it('shows how to choose a model openly before one is chosen', () => {
    const choose = vi.fn();
    render(<Page project={{...base, dmx: ''}} choose={choose}/>);
    fireEvent.click(screen.getByRole('button', {name: 'Choose model…'}));
    expect(choose).toHaveBeenCalledOnce();
    expect(screen.getByRole('button', {name: 'Import source package…'})).toBeVisible();
    expect(screen.getByText('Add a replacement model', {selector: 'strong'})).toBeInTheDocument();
  });
  it('hides Saved inputs in Simple mode only', () => {
    const view = render(<Page project={base} expert={false}/>);
    expect(screen.queryByRole('button', {name: 'Saved inputs'})).toBeNull();
    for (const name of ['Model', 'Mesh options', 'Texture folder']) expect(screen.getByRole('button', {name})).toBeInTheDocument();
    view.unmount();
    render(<Page project={base}/>);
    fireEvent.click(screen.getByRole('button', {name: 'Saved inputs'}));
    expect(screen.getByRole('region', {name: 'Saved inputs'})).toHaveTextContent('Material assignments3');
  });
  it('picks a material on the model, labels them, and walks through what is left to review', async () => {
    const changes = vi.fn();
    render(<Page project={base} changes={changes} preview={scene()}/>);
    await screen.findByTestId('viewport');
    // Opens on the first material Build & install asks about; selecting writes nothing.
    const editor = () => screen.getByRole('heading', {level: 3, name: /\.vmat$/});
    expect(editor()).toHaveTextContent('hair.vmat');
    const labels = probe.props!.labels as {items: {id: string; detail: string; active: boolean}[]};
    expect(labels.items.map(item => [item.id, item.detail, item.active])).toEqual([[a, 'reviewed', false], [b, 'selected', true], [c, 'no image found', false]]);
    act(() => (probe.props!.onPick as (name: string) => void)(a));
    expect(editor()).toHaveTextContent('outfit.vmat');
    expect(probe.props!.marked).toBe(a);
    fireEvent.click(screen.getByRole('button', {name: /Next to review/}));
    expect(editor()).toHaveTextContent('hair.vmat');
    fireEvent.click(screen.getByRole('button', {name: /Next to review/}));
    expect(editor()).toHaveTextContent('legs.vmat');
    fireEvent.click(screen.getByRole('button', {name: /Previous/}));
    expect(editor()).toHaveTextContent('hair.vmat');
    expect(changes).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('checkbox', {name: 'I have reviewed this material choice'}));
    expect((changes.mock.lastCall![0] as Project).materials[b]).toEqual({mode: 'generate', confirmed: true, textures: {}});
    // Selected material alone.
    fireEvent.click(screen.getByRole('button', {name: 'Selected material'}));
    expect(probe.props!.only).toBe(b);
  });
  it('opens the tab and the menu a Build & install link points at', () => {
    render(<Page project={base}/>);
    fireEvent.click(screen.getByRole('tab', {name: /^Face/}));
    act(() => revealSection('materials'));
    expect(screen.getByRole('tab', {name: /^Materials/})).toHaveAttribute('aria-selected', 'true');
    act(() => revealSection('replacement-model'));
    expect(screen.getByRole('button', {name: 'Model'})).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('button', {name: 'Replace…'})).toBeVisible();
  });
});

describe('Surface changes on the Materials tab', () => {
  const colored: AppearanceMaterial = {enabled: true, target: a, roughness: .3, hue: 20, color: [1, .5, .5], ranges: [{enabled: true, hue: 20, hue_width: 15, hue_shift: 120}], future_key: {kept: true}};
  const project = (materials: AppearanceMaterial[], extra: Partial<Project['customization']> = {}) => ({...base, customization: {enabled: true, appearance: {enabled: false, materials}, ...extra}} as unknown as Project);
  it('opens and selects without writing, and edits only the values ticked, keeping colour, ranges and unknown keys', () => {
    const changes = vi.fn();
    render(<Page project={project([colored])} changes={changes}/>);
    fireEvent.click(screen.getByRole('button', {name: /outfit\.vmat/}));
    expect(changes).not.toHaveBeenCalled();
    // The colour is shown, not edited here; the button opens Extras → Colors.
    expect(screen.getByText('Tint · Hue +20° · 1 color range')).toBeInTheDocument();
    expect(screen.queryByLabelText(/Adjust colors/)).toBeNull();
    expect(screen.getByRole('checkbox', {name: 'Change roughness'})).toBeChecked();
    fireEvent.click(screen.getByRole('checkbox', {name: 'Change opacity'}));
    let saved = changes.mock.lastCall![0] as Project;
    expect(rows(saved)).toEqual([{...colored, opacity: 1}]);
    // A write switches surface changes on, as the old editor's did.
    expect(saved.customization.appearance?.enabled).toBe(true);
    fireEvent.click(screen.getByRole('checkbox', {name: 'Change glow (emission)'}));
    fireEvent.change(screen.getByRole('spinbutton', {name: 'Glow (emission) on top'}), {target: {value: '3'}});
    expect(rows(changes.mock.lastCall![0] as Project)).toEqual([{...colored, opacity: 1, emission: 3}]);
    fireEvent.click(screen.getByRole('checkbox', {name: 'Change roughness'}));
    saved = changes.mock.lastCall![0] as Project;
    const {roughness: _, ...withoutRoughness} = colored;
    expect(rows(saved)).toEqual([{...withoutRoughness, opacity: 1, emission: 3}]);
    // The material's own roughness is a different value, saved on the material.
    fireEvent.click(screen.getByRole('checkbox', {name: 'Override roughness'}));
    saved = changes.mock.lastCall![0] as Project;
    expect(saved.materials[a].roughness).toBe(.5);
    expect(rows(saved)).toEqual([{...withoutRoughness, opacity: 1, emission: 3}]);
  });
  it('creates a row for a material on the first ticked value and drops it when nothing is left', () => {
    const changes = vi.fn();
    render(<Page project={project([])} changes={changes}/>);
    fireEvent.click(screen.getByRole('button', {name: /legs\.vmat/}));
    fireEvent.click(screen.getByRole('checkbox', {name: 'Change metalness'}));
    expect(rows(changes.mock.lastCall![0] as Project)).toEqual([{enabled: true, target: c, metalness: 0}]);
    fireEvent.click(screen.getByRole('checkbox', {name: 'Change metalness'}));
    expect(rows(changes.mock.lastCall![0] as Project)).toEqual([]);
  });
  it('opens Extras → Colors for the colour and keeps Include surface changes on the tab', () => {
    const openColors = vi.fn(), changes = vi.fn();
    render(<Page project={project([colored])} changes={changes} openColors={openColors}/>);
    fireEvent.click(screen.getByRole('button', {name: /outfit\.vmat/}));
    fireEvent.click(screen.getByRole('button', {name: 'Edit color in Extras → Colors'}));
    expect(openColors).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole('checkbox', {name: 'Include surface changes'}));
    expect((changes.mock.lastCall![0] as Project).customization.appearance).toEqual({enabled: true, materials: [colored]});
  });
  it('keeps rows Extras → Colors does not edit under Other saved surface changes, with their full editor', () => {
    const free: AppearanceMaterial = {enabled: true, target: '', opacity: .5};
    const gone: AppearanceMaterial = {enabled: true, target: 'models/test/materials/old.vmat', hue: 40};
    const duplicate: AppearanceMaterial = {enabled: false, target: a, saturation: 1.5};
    const saved = project([duplicate, colored, free, gone]);
    expect(freeRows(saved)).toEqual([2, 3]);
    expect([0, 1, 2, 3].map(i => colorsEdits(saved, i))).toEqual([false, true, false, false]);
    const changes = vi.fn();
    render(<Page project={saved} changes={changes}/>);
    const other = screen.getByRole('region', {name: 'Other saved surface changes'});
    expect(within(other).queryByLabelText('Material 2')).toBeNull();
    expect(within(other).getByLabelText('Material 1')).toHaveValue(a);
    expect(within(other).getByLabelText('Material 3')).toHaveValue('');
    expect(within(other).getByLabelText('Material 4')).toHaveValue(gone.target);
    // The old editor still edits their colours.
    fireEvent.change(within(other).getByLabelText('Hue shift 4'), {target: {value: '90'}});
    expect(rows(changes.mock.lastCall![0] as Project)).toEqual([duplicate, colored, free, {...gone, hue: 90}]);
    fireEvent.click(screen.getByRole('button', {name: /outfit\.vmat/}));
    expect(screen.getByText(/1 more saved change for this material is under Other saved surface changes/)).toBeInTheDocument();
  });
  it('edits every row in the old editor while the replacement model is off (Extras → Colors then edits hero colours)', () => {
    const off = project([colored], {model_enabled: false});
    expect(colorsEdits(off, 0)).toBe(false);
    render(<Page project={off}/>);
    expect(within(screen.getByRole('region', {name: 'Other saved surface changes'})).getByLabelText('Adjust colors 1 (experimental)')).toBeChecked();
  });
  it('still adds a row by hand (the old "Change a material"), edited under Other saved surface changes', () => {
    const changes = vi.fn();
    render(<Page project={project([colored])} changes={changes}/>);
    expect(screen.queryByRole('region', {name: 'Other saved surface changes'})).toBeNull();
    fireEvent.click(screen.getByRole('button', {name: 'Add a surface change by hand'}));
    expect(rows(changes.mock.lastCall![0] as Project)).toEqual([colored, {enabled: true, target: ''}]);
    fireEvent.change(within(screen.getByRole('region', {name: 'Other saved surface changes'})).getByLabelText('Material 2'), {target: {value: b}});
    expect(rows(changes.mock.lastCall![0] as Project)).toEqual([colored, {enabled: true, target: b}]);
  });
  it('row helpers keep every other key', () => {
    const saved = project([colored]);
    expect(rows(setSurface(saved, 0, 'metalness', .2))).toEqual([{...colored, metalness: .2}]);
    expect(rows(setSurface(saved, 0, 'enabled', false))).toEqual([{...colored, enabled: false}]);
    expect(rows(addSurface(saved, b)!)).toEqual([colored, {enabled: true, target: b}]);
    expect(rows(removeSurface(addSurface(saved, b)!, 1))).toEqual([colored]);
    expect(setSurface(saved, 5, 'opacity', 1)).toBe(saved);
  });
});

describe('Load 3D previews automatically on Appearance & weapons', () => {
  const settle = () => act(async () => { await new Promise(resolve => setTimeout(resolve, 150)); });
  it('loads nothing by itself when the setting is off (no autoLoad)', async () => {
    const loadPreview = vi.fn();
    render(<Page project={base} loadPreview={loadPreview}/>);
    await settle();
    expect(loadPreview).not.toHaveBeenCalled();
    expect(screen.getByRole('button', {name: 'Load preview'})).toBeInTheDocument();
  });
  it('loads the prepared model once, after a busy app is free, and not again for the same model', async () => {
    const autoLoad = vi.fn(async () => true);
    const view = render(<Page project={base} busy autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).not.toHaveBeenCalled();
    view.rerender(<Page project={base} autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).toHaveBeenCalledOnce();
    view.rerender(<Page project={base} busy autoLoad={autoLoad}/>);
    view.rerender(<Page project={base} autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).toHaveBeenCalledOnce();
  });
  it('tries again when another operation took the app first, and never when a preview of this model is shown', async () => {
    const autoLoad = vi.fn(async () => false);
    const view = render(<Page project={base} autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).toHaveBeenCalledOnce();
    autoLoad.mockResolvedValue(true);
    view.rerender(<Page project={base} busy autoLoad={autoLoad}/>);
    view.rerender(<Page project={base} autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).toHaveBeenCalledTimes(2);
    view.unmount();
    const shown = vi.fn(async () => true);
    render(<Page project={base} preview={scene()} previewCurrent autoLoad={shown}/>);
    await settle();
    expect(shown).not.toHaveBeenCalled();
  });
  it('loads again by itself only when the shown preview is of an earlier prepared model', async () => {
    const autoLoad = vi.fn(async () => true);
    render(<Page project={base} preview={scene()} previewCurrent={false} autoLoad={autoLoad}/>);
    await settle();
    expect(autoLoad).toHaveBeenCalledOnce();
  });
  it('starts nothing when the page is left before the load begins, and keeps the one view mounted', async () => {
    const autoLoad = vi.fn(async () => true);
    const view = render(<Page project={base} autoLoad={autoLoad}/>);
    view.unmount();
    await settle();
    expect(autoLoad).not.toHaveBeenCalled();
    render(<Page project={base} preview={scene()} autoLoad={autoLoad}/>);
    await screen.findByTestId('viewport');
    for (const name of [/^Face/, /^Gun/, /^Materials/]) fireEvent.click(screen.getByRole('tab', {name}));
    await settle();
    expect(probe.mounts).toBe(1);
    expect(autoLoad).not.toHaveBeenCalled();
  });
});
