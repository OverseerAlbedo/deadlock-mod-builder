import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FieldCheck, Project, Settings, SetupStatus } from '../models';
import { SettingsPage } from '../components/SettingsPage';
import { OnboardingPage } from './OnboardingPage';
import { MoveToCsdk13 } from '../components/Notifications';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), openUrl: vi.fn() }));
vi.mock('@tauri-apps/api/core', async original => ({ ...(await original<object>()), invoke: mocks.invoke }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: mocks.openUrl }));
beforeEach(() => { vi.clearAllMocks(); });

const empty: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:''};
/** setup_checks::field_checks(&Settings::default()) and the matching setup status, word for word. */
const NOT_SET = 'The model compiler is not set up. Choose Reduced CSDK 13 as the Community SDK in Settings: it compiles the hero model too, so nothing else needs installing.';
const emptyChecks: FieldCheck[] = [
  {field:'workspace',state:'problem',message:String.raw`Choose a folder where the app can keep project files, for example C:\DeadlockModBuilder\workspaces. Click Detect tools to use the default folder.`},
  {field:'deadlock',state:'problem',message:String.raw`Choose the Deadlock game folder (the one that contains the game folder), for example C:\Program Files (x86)\Steam\steamapps\common\Deadlock. Click Detect tools to find it through Steam.`},
  {field:'pak',state:'problem',message:String.raw`Choose pak01_dir.vpk from the game, for example C:\Program Files (x86)\Steam\steamapps\common\Deadlock\game\citadel\pak01_dir.vpk.`},
  {field:'blender',state:'warning',message:'Blender is not set. It is needed to prepare Blender, FBX and glTF models; Detect tools finds an installed Blender.'},
  {field:'source2viewer',state:'problem',message:String.raw`Choose Source2Viewer-CLI.exe from the Windows x64 command-line download (cli-windows-x64.zip), for example C:\Tools\Source2Viewer\Source2Viewer-CLI.exe.`},
  {field:'csdk',state:'problem',message:String.raw`Choose the extracted Reduced_CSDK_13 folder (it contains the folders game and content), for example C:\CSDK13.`},
  {field:'resourcecompiler',state:'problem',message:String.raw`Choose resourcecompiler.exe in the SDK's game\bin\win64 folder, for example C:\CSDK13\game\bin\win64\resourcecompiler.exe.`},
  {field:'steam',state:'optional',message:'Optional; empty.'},
  {field:'cswin',state:'problem',message:NOT_SET},
  {field:'optional_cs2',state:'optional',message:'Optional; empty.'},
  {field:'vpk_tool',state:'optional',message:'Optional; empty.'},
];
const emptyStatus: SetupStatus = {files_found:{Deadlock:false,'Original VPK':false,CSDK:false,'Resource compiler':false,'Source 2 Viewer CLI':false},model_status:NOT_SET,model_files_found:false,fields:emptyChecks};
const OLD_TOOLS = /CSDK 12|Counter-Strike|DepotDownloader|Set up model compiler/;

function visible(element: Element) {
  try { expect(element).toBeVisible(); return true; } catch { return false; }
}
/** The innermost visible elements whose text names `pattern`. */
function visibleTexts(pattern: RegExp) {
  return [...document.body.querySelectorAll('*')]
    .filter(el => pattern.test(el.textContent ?? '') && ![...el.children].some(child => pattern.test(child.textContent ?? '')))
    .filter(visible).map(el => el.textContent);
}
const noop = () => undefined;
const settingsProps = { update: noop, persist: noop, busy: false, dirty: false, fail: noop, detect: noop, check: noop, onboarding: noop, modelSetup: noop };

describe('What a new user sees', () => {
  it('treats the content of a closed details element as hidden and its summary as visible', () => {
    render(<details><summary>Shown</summary><p>Inside</p></details>);
    expect(visible(screen.getByText('Shown'))).toBe(true);
    expect(visible(screen.getByText('Inside'))).toBe(false);
  });
  for (const expert of [false, true]) {
    it(`Settings (${expert ? 'Expert' : 'Simple'}) offers no model toolset, and Blender is a main field`, () => {
      render(<SettingsPage {...settingsProps} expert={expert} settings={empty} status={emptyStatus}/>);
      expect(visibleTexts(OLD_TOOLS)).toEqual([]);
      const blender = screen.getByLabelText('Blender executable');
      expect(blender.closest('details')).toBeNull();
      expect(screen.getByLabelText('Model compiler toolset').closest('details')?.querySelector('summary')?.textContent).toBe('Downloaded model toolset (older setups)');
      expect(visible(screen.getByLabelText('Model compiler toolset'))).toBe(false);
      // Main order: SDK before its compiler.
      const main = document.querySelector('.settings-fields')!;
      const order = ['Workspace folder', 'Deadlock installation', 'Deadlock game archive', 'Blender executable', 'Source 2 Viewer CLI', 'Community SDK', 'Resource compiler']
        .map(label => [...main.querySelectorAll('input')].indexOf(screen.getByLabelText(label) as HTMLInputElement));
      expect(order).toEqual([0, 1, 2, 3, 4, 5, 6]);
      expect(screen.queryByRole('button', {name: 'Compile models with Reduced CSDK 13'})).toBeNull();
      expect(screen.queryByRole('note', {name: 'Moving to Reduced CSDK 13'})).toBeNull();
    });
  }
  it('the guide step lists the four programs and names the old tools only in one sentence', () => {
    render(<OnboardingPage startStep={1} active project={{name:'New mod',original_model:'',dmx:'',preparation_source:'',customization:{}} as unknown as Project} settings={empty} busy={false} dirty={false} status={emptyStatus}
      updateSettings={noop} saveSettings={noop} detect={noop} check={noop} verify={noop} fail={noop} go={noop} openProject={noop} chooseModel={noop} analyze={noop}/>);
    expect(visibleTexts(OLD_TOOLS)).toEqual(['Reduced CSDK 13 compiles the textures, materials and the character. Older guides also list CSWIN64, Reduced CSDK 12, DepotDownloader or Counter-Strike 2 tools: none of them is needed with Reduced CSDK 13.']);
    const steps = [...document.querySelectorAll('.setup-order > li strong:first-child')].map(e => e.textContent);
    expect(steps.slice(0, 4)).toEqual(['Deadlock', 'Blender 5.2 LTS', 'Source 2 Viewer CLI', 'Reduced CSDK 13']);
    const rows = [...document.querySelectorAll('.tool-downloads > .disclosure-content > .tool-download-row strong')].map(e => e.textContent);
    expect(rows).toEqual(['Deadlock', 'Blender 5.2 LTS', 'Source 2 Viewer CLI', 'Reduced CSDK 13']);
    expect(screen.getByText(/Still needed/).textContent).toContain('Reduced CSDK 13 (Community SDK)');
  });
});

describe('Moving from Reduced CSDK 12', () => {
  // The steps are the sidebar's "Reduced CSDK 12 in use" notification (App.test.tsx
  // "Sidebar notifications"); Settings keeps only the one-line check under the field.
  const older = (backend: string, layout?: 'older' | 'current') => {
    const s = {...empty, csdk: String.raw`C:\CSDK12`, model_backend: backend};
    const fields: FieldCheck[] = [{field:'csdk',state:layout === 'older' ? 'warning' : 'ok',message:layout === 'older' ? 'Reduced CSDK 12 found. Reduced CSDK 13 replaces it and also compiles hero models.' : 'Reduced CSDK 13 found.',layout}];
    return render(<SettingsPage {...settingsProps} expert={false} settings={s} status={{...emptyStatus, fields}}/>);
  };
  const steps = (backend: string, update = vi.fn(), go = vi.fn(), notify = vi.fn()) => {
    const s = {...empty, csdk: String.raw`C:\CSDK12`, model_backend: backend};
    function Harness() {
      const [found, setFound] = useState<string | null>(null);
      return <MoveToCsdk13 settings={s} busy={false} update={update} go={go} notify={notify} fail={noop} found={found} setFound={setFound} ignore={noop}/>;
    }
    render(<Harness/>);
    return {s, update, go, notify};
  };
  it('Settings keeps the one-line check for an older-layout SDK and no longer shows the steps', () => {
    older('auto', 'older');
    expect(screen.queryByRole('note', {name: 'Moving to Reduced CSDK 13'})).toBeNull();
    expect(screen.queryByRole('button', {name: 'Find Reduced CSDK 13'})).toBeNull();
    expect(visible(screen.getByText('Reduced CSDK 12 found. Reduced CSDK 13 replaces it and also compiles hero models.'))).toBe(true);
    // The CSDK 12 warning has no fix: no Fix button, and nothing counts as a problem.
    expect(screen.queryByRole('button', {name: /^Fix /})).toBeNull();
    expect(screen.getByText('All required paths look right.')).toBeInTheDocument();
  });
  it('the steps have no toolset step without a cswin backend', () => {
    steps('auto');
    expect(document.querySelectorAll('ol > li')).toHaveLength(3);
    expect(document.body.textContent).not.toContain('Compile models with Reduced CSDK 13');
  });
  it('Find Reduced CSDK 13 changes nothing when only a Reduced CSDK 12 is found', async () => {
    mocks.invoke.mockImplementation(async (command: string, args: {settings: Settings}) => command === 'detect_settings'
      ? {...args.settings, csdk: String.raw`C:\CSDK12`, resourcecompiler: String.raw`C:\CSDK12\game\bin_cs2\win64\resourcecompiler.exe`}
      : [{field:'csdk',state:'warning',message:'Reduced CSDK 12 found.',layout:'older'}]);
    const {update, go} = steps('auto');
    fireEvent.click(screen.getByRole('button', {name: 'Find Reduced CSDK 13'}));
    expect(await screen.findByText(/No extracted Reduced CSDK 13 was found/)).toBeInTheDocument();
    expect(update).not.toHaveBeenCalled();
    expect(go).not.toHaveBeenCalled();
  });
  it('adds the toolset step when the setup uses the downloaded toolset', () => {
    steps('cswin');
    expect(document.querySelectorAll('ol > li')).toHaveLength(4);
    expect(document.body.textContent).toContain('Compile models with Reduced CSDK 13');
  });
  it('Settings shows nothing extra for Reduced CSDK 13', () => {
    older('cswin', 'current');
    expect(screen.queryByRole('note', {name: 'Moving to Reduced CSDK 13'})).toBeNull();
    expect(screen.queryByText(/Reduced CSDK 12/)).toBeNull();
  });
  it('Find Reduced CSDK 13 takes only an SDK of the current layout and opens Settings', async () => {
    mocks.invoke.mockImplementation(async (command: string, args: {settings: Settings}) => command === 'detect_settings'
      ? {...args.settings, csdk: String.raw`C:\CSDK13`, resourcecompiler: String.raw`C:\CSDK13\game\bin\win64\resourcecompiler.exe`, workspace: 'ignored'}
      : [{field:'csdk',state:'ok',message:'Reduced CSDK 13 found.',layout:'current'}]);
    const {s, update, go, notify} = steps('auto');
    fireEvent.click(screen.getByRole('button', {name: 'Find Reduced CSDK 13'}));
    await waitFor(() => expect(update).toHaveBeenCalledWith({...s, csdk: String.raw`C:\CSDK13`, resourcecompiler: String.raw`C:\CSDK13\game\bin\win64\resourcecompiler.exe`}));
    expect(mocks.invoke).toHaveBeenCalledWith('detect_settings', {settings: {...s, csdk: '', resourcecompiler: ''}, extraRoots: []});
    expect(go).toHaveBeenCalledWith('settings');
    expect(notify).toHaveBeenCalledWith(String.raw`Reduced CSDK 13 found: C:\CSDK13. Save settings to use it.`);
  });
});

describe('An explicit toolset beside Reduced CSDK 13', () => {
  it('offers the switch to auto as a button that only changes the draft settings', () => {
    const update = vi.fn();
    const s = {...empty, csdk: String.raw`C:\CSDK13`, cswin: String.raw`C:\T\toolset`, model_backend: 'cswin'};
    const fields: FieldCheck[] = [{field:'cswin',state:'ok',message:'Hero models are compiled by your downloaded model toolset. Reduced CSDK 13 compiles them too, so the toolset is no longer needed.',backend:'auto'}];
    render(<SettingsPage {...settingsProps} update={update} expert={false} settings={s} status={{...emptyStatus, fields}}/>);
    fireEvent.click(screen.getByRole('button', {name: 'Compile models with Reduced CSDK 13'}));
    expect(update).toHaveBeenCalledWith({...s, model_backend: 'auto'});
    expect(screen.queryByRole('button', {name: /Fix/})).toBeNull();
  });
});
