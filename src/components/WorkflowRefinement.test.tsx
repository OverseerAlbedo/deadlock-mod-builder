import { useState } from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Project } from '../models';
import { BuildPage } from '../build/BuildPage';
import { PhysicsPage, MotionPage, ExtrasPage } from './CustomizationPages';
import { FilePathInput } from './FilePathInput';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), includedPhysics: vi.fn(), previewExtra: vi.fn() }));
vi.mock('../api', async original => ({ ...await original<typeof import('../api')>(), api: native }));
const initial = { ...reference.cases[1].expected, customization: { enabled: true, future_extension: { retained: 7 } } } as unknown as Project;
beforeEach(() => { vi.clearAllMocks(); native.validateAssetPath.mockImplementation(async path => path); native.previewExtra.mockRejectedValue(new Error('Fixture preview unavailable')); });

function Customized({ page }: { page: 'physics' | 'motion' | 'extras' }) {
  const [project, update] = useState<Project>({ ...initial, customization: { ...initial.customization, physics: { enabled: true, source: 'C:\\references\\tested.vmdl_c', hero_path: initial.original_model }, animations: page === 'motion' ? [{ enabled: true, source: 'C:\\clips\\jump.vnmclip_c', target: 'animations/jump.vnmclip_c', original_sha256: 'hash' }] : [] } });
  const props = { project, update, disabled: false, fail: vi.fn() };
  return <>{page === 'physics' ? <PhysicsPage {...props} summary={null} /> : page === 'motion' ? <MotionPage {...props} /> : <ExtrasPage {...props} catalog={{hero_path:initial.original_model,icons:[{path:"panorama/images/hero.vtex_c",target:"panorama/images/hero.vtex_c",label:"Hero portrait",supported:true}],audio:[],effects:[],animations:[],variants:[],animation_unavailable:null,warnings:[]}} />}<output data-testid="project-state">{JSON.stringify(project)}</output></>;
}
const saved = () => JSON.parse(screen.getByTestId('project-state').textContent!) as Project;

describe('Refined controls preserve the real project operations', () => {
  it('switches physics modes without losing the reference or unrelated settings', () => {
    render(<Customized page="physics" />);
    expect(screen.getAllByRole('radio')).toHaveLength(3);
    fireEvent.click(screen.getByRole('radio', { name: /Preserve original physics/ }));
    expect(saved().customization.physics).toMatchObject({ enabled: false, source: 'C:\\references\\tested.vmdl_c' });
    fireEvent.click(screen.getByRole('radio', { name: /Replacement cloth setup/ }));
    expect(saved().customization.physics?.enabled).toBe(true);
    expect(saved().customization.future_extension).toEqual({ retained: 7 });
    expect(screen.getByText('Advanced cloth authoring').closest('details')).not.toHaveAttribute('open');
  });
  it('preserves saved clips when returning to original animations and exposes the graph settings', () => {
    render(<Customized page="motion" />);
    fireEvent.click(screen.getByRole('radio', { name: /Preserve original animations/ }));
    expect(saved().animation_policy).toBe('preserve');
    expect(saved().customization.animations?.[0]).toMatchObject({ enabled: false, source: 'C:\\clips\\jump.vnmclip_c', original_sha256: 'hash' });
    fireEvent.click(screen.getByText('Animation graph handling', { selector: 'summary > span' }));
    fireEvent.change(screen.getByLabelText('Animation graph mode'), { target: { value: 'adapter' } });
    expect(saved().ag2_mode).toBe('adapter');
  });
  it('adds an optional asset, validates a drop into its field, and retains customization', async () => {
    render(<Customized page="extras" />);
    fireEvent.click(screen.getByRole('button', { name: /Hero portrait/ }));
    const target = document.querySelector('[data-native-drop]')!;
    await act(async () => { target.dispatchEvent(new CustomEvent('dmb-file-drop', { detail: ['C:\\art\\icon.png'] })); });
    await waitFor(() => expect(saved().customization.icons?.[0].source).toBe('C:\\art\\icon.png'));
    expect(native.validateAssetPath).toHaveBeenCalledWith('C:\\art\\icon.png', 'icon');
    expect(saved().customization.future_extension).toEqual({ retained: 7 });
    expect(saved().customization.icons?.[0].target).toBe('panorama/images/hero.vtex_c');
    fireEvent.click(screen.getByRole('button', { name: 'Remove replacement' }));
    expect(saved().customization.icons).toEqual([]);
  });
  it('keeps a completed path compact, rejects an invalid replacement and ignores drops while busy', async () => {
    const update = vi.fn(), fail = vi.fn();
    native.validateAssetPath.mockRejectedValue(new Error('Not a valid model'));
    const props = { label: 'Tested compiled model', value: 'C:\\references\\tested.vmdl_c', kind: 'physics' as const, update, fail, disabled: false };
    const view = render(<FilePathInput {...props} />);
    expect(screen.getByLabelText('Tested compiled model')).not.toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Edit path' }));
    expect(screen.getByLabelText('Tested compiled model')).toHaveValue(props.value);
    const target = screen.getByLabelText('Tested compiled model').closest('.asset-path')!;
    await act(async () => { target.dispatchEvent(new CustomEvent('dmb-file-drop', { detail: ['C:\\bad.exe'] })); });
    expect(fail).toHaveBeenCalled(); expect(update).not.toHaveBeenCalled();
    expect(screen.getByLabelText('Tested compiled model')).toHaveValue(props.value);
    view.rerender(<FilePathInput {...props} disabled />);
    native.validateAssetPath.mockClear();
    await act(async () => { target.dispatchEvent(new CustomEvent('dmb-file-drop', { detail: ['C:\\other.vmdl_c'] })); });
    expect(native.validateAssetPath).not.toHaveBeenCalled();
  });
  it('keeps build, preparation, compiler and install actions connected with their exact flags', () => {
    const project = { ...initial, dmx: '', customization: { enabled: true, model_enabled: false } } as Project;
    const callbacks = { update: vi.fn(), inspect: vi.fn(), prepare: vi.fn(), validateOriginal: vi.fn(), build: vi.fn(), install: vi.fn(), chooseFreeSlot: vi.fn() };
    const props={...callbacks,project,prepared:null,original:null,busy:false,result:null,installed:null};
    const view=render(<BuildPage {...props} report={null} fresh={false}/>);
    expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Analyze project' })); expect(callbacks.inspect).toHaveBeenCalledOnce();
    view.rerender(<BuildPage {...props} fresh report={{model_enabled:false,icons:[],animations:[],effects:[],audio:[],clothing:null,native_compile_required:true,warnings:[],native_validated:false,in_game_validated:false}}/>);
    fireEvent.click(screen.getByText('Output settings', { selector: 'summary > span' }));
    fireEvent.change(screen.getByLabelText('VPK slot'), { target: { value: '9' } });
    expect(callbacks.update.mock.calls.at(-1)?.[0].vpk_number).toBe(9);
    fireEvent.click(screen.getByRole('button', { name: 'Choose free slot' })); expect(callbacks.chooseFreeSlot).toHaveBeenCalledOnce();
    for (const label of ['Compile replacement model', 'Package a VPK after validation', 'Install after successful validation and packaging']) {
      fireEvent.click(screen.getByLabelText(label));
    }
    expect(callbacks.update).toHaveBeenCalledTimes(4);
    fireEvent.click(screen.getByText('Replace existing output'));
    fireEvent.click(screen.getByLabelText('Replace an existing output VPK for this project and slot'));
    fireEvent.click(screen.getByLabelText('Replace an existing installed VPK at the selected slot'));
    fireEvent.click(screen.getByRole('button', { name: 'Build mod' })); expect(callbacks.build).toHaveBeenCalledWith(true, true);
    fireEvent.click(screen.getByText('Developer / compiler tools', { selector: 'summary > span' }));
    fireEvent.click(screen.getByRole('button', { name: 'Inspect compiler plan' }));
    fireEvent.click(screen.getByRole('button', { name: 'Compile original test' }));
    expect(callbacks.validateOriginal.mock.calls).toEqual([[false], [true]]);
    expect(screen.getByRole('button', { name: 'Prepare source files' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Choose VPK to validate and install…' }));
    expect(callbacks.install).toHaveBeenCalledWith(null, true);
  });
});
