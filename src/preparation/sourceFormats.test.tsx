import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MAIN_TOOLS } from '../setup/ToolDownloads';
import { describe, expect, it, vi } from 'vitest';
import type { FieldCheck, Settings } from '../models';
import { SettingsPage } from '../components/SettingsPage';
import { extensions } from '../components/FilePathInput';
import { GUN_EXTENSIONS, isSourceModel, SOURCE_EXTENSIONS, SOURCE_FORMATS_TEXT } from './sourceFormats';

vi.mock('@tauri-apps/api/core', async original => ({ ...(await original<object>()), invoke: vi.fn() }));
const mocks = vi.hoisted(() => ({ openUrl: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: mocks.openUrl }));

describe('MMD models (.pmx, .pmd)', () => {
  it('are character sources and gun files, in any letter case', () => {
    expect(SOURCE_EXTENSIONS).toEqual(['blend', 'glb', 'gltf', 'fbx', 'pmx', 'pmd']);
    expect(GUN_EXTENSIONS).toEqual(expect.arrayContaining(['pmx', 'pmd']));
    for (const path of ['C:\\Models\\キャラ.pmx', 'C:\\Models\\OLD.PMD', 'C:\\a.b\\hero.blend', 'hero.FBX']) expect(isSourceModel(path)).toBe(true);
    for (const path of ['C:\\Models\\motion.vmd', 'C:\\Models\\hero.dmx', 'C:\\Models\\pmx', 'C:\\Models\\a.pmx.zip']) expect(isSourceModel(path)).toBe(false);
    expect(SOURCE_FORMATS_TEXT).toContain('MMD (.pmx, .pmd)');
  });
  it('are taken for a gun file, not (yet) for clothing or a rigid prop', () => {
    expect(extensions.weapon).toEqual(expect.arrayContaining(['pmx', 'pmd']));
    for (const kind of ['garment', 'prop'] as const) {
      expect(extensions[kind]).not.toContain('pmx');
      expect(extensions[kind]).not.toContain('pmd');
    }
  });
});

const settings: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:String.raw`C:\Blender\blender.exe`};
const noop = () => undefined;
const props = { update: noop, persist: noop, busy: false, dirty: false, fail: noop, detect: noop, check: noop, onboarding: noop, modelSetup: noop, settings, status: null };
const blender: FieldCheck = {field:'blender',state:'ok',message:'Blender found.'};

describe('The MMD Tools note in Settings', () => {
  it('sits under Blender as a note, never an alert, when the add-on is missing', () => {
    const missing: FieldCheck = {field:'mmd_tools',state:'optional',message:'MMD Tools is not installed for Blender 5.3. Only MMD models (.pmx, .pmd) need it: in Blender 5.3, open Edit → Preferences → Get Extensions, search for MMD Tools and click Install.'};
    render(<SettingsPage {...props} fields={[blender, missing]}/>);
    const note = screen.getByLabelText('MMD Tools');
    expect(note).toHaveTextContent('MMD Tools is not installed for Blender 5.3.');
    expect(note.closest('.settings-path-field')?.querySelector('input')).toBe(screen.getByLabelText('Blender executable'));
    expect(screen.queryByRole('alert')).toBeNull();
  });
  it('links the MMD Tools page when the add-on is missing', async () => {
    mocks.openUrl.mockResolvedValue(undefined);
    const missing: FieldCheck = {field:'mmd_tools',state:'optional',message:'MMD Tools is not installed for Blender 5.3.'};
    render(<SettingsPage {...props} fields={[blender, missing]}/>);
    const link = within(screen.getByLabelText('MMD Tools')).getByRole('link', {name: /Get MMD Tools/});
    expect(link).toHaveAttribute('href', 'https://extensions.blender.org/add-ons/mmd-tools/');
    fireEvent.click(link);
    await waitFor(() => expect(mocks.openUrl).toHaveBeenCalledWith('https://extensions.blender.org/add-ons/mmd-tools/'));
    // Not a fifth install in the program list.
    expect(MAIN_TOOLS).not.toContain('mmdtools');
  });
  it('says when the add-on is found, and is absent without a check', () => {
    const { unmount } = render(<SettingsPage {...props} fields={[blender, {field:'mmd_tools',state:'ok',message:'MMD Tools found for Blender 5.2: MMD models (.pmx, .pmd) can be prepared.'}]}/>);
    expect(screen.getByLabelText('MMD Tools')).toHaveTextContent('✓ MMD Tools found for Blender 5.2');
    expect(screen.queryByRole('link', {name: /Get MMD Tools/})).toBeNull();
    unmount();
    render(<SettingsPage {...props} fields={[blender]}/>);
    expect(screen.queryByLabelText('MMD Tools')).toBeNull();
  });
});
