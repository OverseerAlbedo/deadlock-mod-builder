import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Settings } from './models';

// Operation results and failures in the sidebar's Notifications (the App wiring).
const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),restoreAutosave:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),testSetup:vi.fn(),prebuildWarnings:vi.fn(),checkSettingFields:vi.fn(),heroProportions:vi.fn(),operationDurations:vi.fn(),buildProject:vi.fn(),inspectProject:vi.fn(),installProject:vi.fn(),detectSettings:vi.fn(),setupStatus:vi.fn(),settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),saveProject:vi.fn(),cancel:vi.fn(),latestTimings:vi.fn(),logs:vi.fn(),vpkSlots:vi.fn(),
  open:vi.fn(),save:vi.fn(),ask:vi.fn(),
}));
vi.mock('./api', async original => ({...await original<typeof import('./api')>(),isDesktop:() => true,api:mocks}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:async () => vi.fn(),onCloseRequested:async () => vi.fn(),destroy:vi.fn()})}));
// Model preparation is its own page with its own tests; here only how it reports back.
vi.mock('./preparation/PreparationPage', () => ({PreparationPage:({operate,next}:{operate:(label:string,operation:()=>Promise<unknown>,canCancel?:boolean)=>Promise<unknown>;next?:()=>void}) => <div>
  <button onClick={() => void operate('Your model could not be prepared',async()=>{next?.();},true)}>Stub prepare</button>
  <button onClick={() => void operate('Your model could not be prepared',async()=>{throw {kind:'tool',message:'Blender stopped',details:'Traceback: worker.py line 9'};},true)}>Stub prepare fails</button>
</div>}));
import { App } from './App';
import { inspected, reviewedProject } from './build/testFixtures';

const settings: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:''};
const built = {prepared:null,compiled:true,vpk:'C:\\Out\\pak09_dir.vpk',installed:'C:\\Deadlock\\game\\citadel\\addons\\pak09_dir.vpk',manifest:null};
const failure = {kind:'tool',message:'Compiler stopped',details:'resourcecompiler: material body.vmat failed',help:{cause:'A material could not be compiled.',steps:['Open Logs.'],action:{label:'Open Logs',page:'logs'}}};
const region = () => screen.getByRole('region',{name:'Notifications'});
const inList = () => within(region());
/** The rows as badge label + title, newest first (the time cut off). */
const rows = () => inList().queryAllByRole('button',{expanded:false}).concat(inList().queryAllByRole('button',{expanded:true}))
  .sort((a,b)=>a.compareDocumentPosition(b)&Node.DOCUMENT_POSITION_FOLLOWING?-1:1)
  .map(row=>`${row.querySelector('.sr-only')?.textContent}: ${row.querySelector('.notice-title')?.textContent}`);
const nav = (name: string) => fireEvent.click(within(screen.getByRole('navigation',{name:'Main navigation'})).getByRole('button',{name}));

beforeEach(() => {
  vi.clearAllMocks();
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.newProject.mockResolvedValue({path:null,project:structuredClone(reviewedProject)});
  mocks.settings.mockResolvedValue(settings); mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null);
  mocks.ask.mockResolvedValue(true);
  mocks.inspectProject.mockResolvedValue({project:structuredClone(reviewedProject),report:inspected});
  mocks.saveProject.mockImplementation(async (path,project) => ({path,project}));
});
afterEach(() => { cleanup(); });
async function start() { render(<App/>); await screen.findByLabelText('Project name'); }
async function analyzed() {
  await start(); nav('Build & install');
  fireEvent.click(screen.getByRole('button',{name:'Analyze project'}));
  await screen.findByText('Ready to build',{exact:true});
}
const buildMod = async () => { await waitFor(()=>expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled()); fireEvent.click(screen.getByRole('button',{name:'Build mod'})); };
const reanalyze = async () => { await waitFor(()=>expect(screen.getByRole('button',{name:'Re-analyze project'})).toBeEnabled()); fireEvent.click(screen.getByRole('button',{name:'Re-analyze project'})); };

describe('A failure', () => {
  it('shows one line at the top, opens its notification, and keeps its copy after the next operation', async () => {
    await analyzed();
    mocks.buildProject.mockRejectedValueOnce(failure);
    await buildMod();
    const line = await screen.findByRole('alert');
    expect(line).toHaveTextContent('Compiler stopped');
    expect(line).toHaveTextContent('Details in Notifications.');
    expect(line.querySelector('.notification-badge-error')).not.toBeNull();
    // Not the whole error.
    expect(screen.queryByText('A material could not be compiled.')).toBeNull();
    expect(screen.queryByRole('button',{name:'Copy for support'})).toBeNull();
    // The notification is added, open, and short.
    const row = inList().getByRole('button',{name:/Build failed/});
    await waitFor(()=>expect(row).toHaveAttribute('aria-expanded','true'));
    expect(inList().getByText('Compiler stopped')).toHaveClass('notice-clamp');
    expect(inList().getByRole('button',{name:'Open Logs'})).toBeInTheDocument();
    expect(inList().getByRole('button',{name:'Show full details'})).toBeInTheDocument();
    // The next operation clears the line; the notification stays.
    await reanalyze();
    await waitFor(()=>expect(screen.queryByRole('alert')).toBeNull());
    expect(inList().getByText('Compiler stopped')).toBeInTheDocument();
    // Show full details opens the whole error on the page.
    fireEvent.click(inList().getByRole('button',{name:'Show full details'}));
    const full = screen.getByRole('alert');
    expect(full).toHaveClass('error-notice');
    expect(within(full).getByText('A material could not be compiled.')).toBeInTheDocument();
    expect(within(full).getByText('Open Logs.')).toBeInTheDocument();
    expect(within(full).getByText('resourcecompiler: material body.vmat failed',{exact:false})).toBeInTheDocument();
    expect(within(full).getByRole('button',{name:'Copy for support'})).toBeInTheDocument();
    fireEvent.click(within(full).getByRole('button',{name:'Open Logs'}));
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('heading',{name:'Logs'})).toBeInTheDocument();
  });
  it('opens the whole error from the line, and both close', async () => {
    await analyzed();
    mocks.buildProject.mockRejectedValueOnce(failure);
    await buildMod();
    await screen.findByRole('alert');
    fireEvent.click(screen.getByRole('button',{name:'Close'}));
    expect(screen.queryByRole('alert')).toBeNull();
    expect(inList().getByRole('button',{name:/Build failed/})).toBeInTheDocument();
    mocks.buildProject.mockRejectedValueOnce(failure);
    await buildMod();
    await screen.findByRole('alert');
    fireEvent.click(screen.getByRole('button',{name:'Show details'}));
    const full = screen.getByRole('alert');
    expect(within(full).getByText('A material could not be compiled.')).toBeInTheDocument();
    fireEvent.click(within(full).getByRole('button',{name:'Dismiss'}));
    expect(screen.queryByRole('alert')).toBeNull();
    // Errors accumulate.
    expect(rows().filter(row => row === 'Error: Build failed')).toHaveLength(2);
  });
  it('logs nothing for the busy guard, a cancellation or a declined confirmation', async () => {
    await analyzed();
    const before = rows();
    mocks.buildProject.mockRejectedValueOnce({kind:'busy',message:'Another operation is running',details:'Cancel it or wait for it to finish.'});
    await buildMod();
    const line = await screen.findByRole('alert');
    expect(line).toHaveTextContent('Another operation is running');
    expect(line).not.toHaveTextContent('Details in Notifications.');
    mocks.buildProject.mockRejectedValueOnce({kind:'cancelled',message:'Cancelled',details:''});
    await buildMod();
    await screen.findByText('Operation cancelled');
    mocks.prebuildWarnings.mockResolvedValue(['The SDK folder is long.']);
    mocks.ask.mockResolvedValueOnce(false);
    await buildMod();
    await waitFor(()=>expect(mocks.ask).toHaveBeenCalledWith(expect.stringContaining('Build anyway?'),expect.anything()));
    expect(mocks.buildProject).toHaveBeenCalledTimes(2);
    expect(rows()).toEqual(before);
    expect(rows().some(row => row.startsWith('Error'))).toBe(false);
  });
});

describe('Operation results', () => {
  it('Analyze: a success, replaced by the next; a warning when choices wait; a failure', async () => {
    await analyzed();
    expect(rows()).toEqual(['Success: Analysis finished']);
    await reanalyze();
    await waitFor(()=>expect(mocks.inspectProject).toHaveBeenCalledTimes(2));
    expect(rows()).toEqual(['Success: Analysis finished']);
    mocks.inspectProject.mockResolvedValueOnce({project:structuredClone(reviewedProject),report:{...inspected,needs_review:true}});
    await reanalyze();
    await waitFor(()=>expect(rows()).toEqual(['Warning: Analysis finished']));
    fireEvent.click(inList().getByRole('button',{name:/Analysis finished/}));
    expect(inList().getByText('Some items need your choice before building. Review them on Build & install.')).toBeInTheDocument();
    expect(inList().getByRole('button',{name:'Open Build & install'})).toBeInTheDocument();
    mocks.inspectProject.mockRejectedValueOnce({kind:'validation',message:'Source input changed',details:'Fresh native validation failed.'});
    await reanalyze();
    await waitFor(()=>expect(rows()).toEqual(['Error: Analysis failed','Warning: Analysis finished']));
  });
  it('Build: completed (only the latest kept) and failed', async () => {
    await analyzed();
    mocks.buildProject.mockResolvedValue({project:reviewedProject,path:'C:\\P\\vindicta.dmb.json',result:built});
    await buildMod();
    await inList().findByRole('button',{name:/Build completed/});
    await buildMod();
    await waitFor(()=>expect(mocks.buildProject).toHaveBeenCalledTimes(2));
    await waitFor(()=>expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled());
    expect(rows()).toEqual(['Success: Build completed','Success: Analysis finished']);
    const row = inList().getByRole('button',{name:/Build completed/});
    expect(row.textContent).toMatch(/Build completed\d\d:\d\d$/);
    fireEvent.click(row);
    expect(inList().getByText('Vindicta Balanced')).toBeInTheDocument();
    expect(inList().getByText('The mod was built and installed in the game as pak09_dir.vpk.')).toBeInTheDocument();
    mocks.buildProject.mockRejectedValueOnce(failure);
    await buildMod();
    await waitFor(()=>expect(rows()[0]).toBe('Error: Build failed'));
    expect(region().querySelector('.notification-summary')?.textContent).toBe('✕1 error, 2 successes');
  });
  it('Build: a project the user never saved stays unsaved, so New still asks', async () => {
    await start();
    fireEvent.change(screen.getByLabelText('Project name'),{target:{value:'Edited name'}});
    nav('Build & install'); fireEvent.click(screen.getByRole('button',{name:'Analyze project'})); await screen.findByText('Ready to build',{exact:true});
    mocks.buildProject.mockImplementation(async project => ({project:{...project,vpk_number:9},path:'C:\\W\\workspace\\project.dmb.json',result:built}));
    await buildMod();
    await inList().findByRole('button',{name:/Build completed/});
    await waitFor(()=>expect(screen.getByRole('button',{name:'New'})).toBeEnabled());
    mocks.ask.mockClear(); mocks.ask.mockResolvedValueOnce(false);
    fireEvent.click(screen.getByRole('button',{name:'New'}));
    await waitFor(()=>expect(mocks.ask).toHaveBeenCalledWith('This project has unsaved changes. Discard them and continue?',expect.objectContaining({okLabel:'Discard changes',cancelLabel:'Keep editing'})));
    expect(mocks.newProject).toHaveBeenCalledTimes(1);
    // The next build still saves into the workspace, never as if the workspace file were the user's.
    await buildMod();
    await waitFor(()=>expect(mocks.buildProject).toHaveBeenCalledTimes(2));
    expect(mocks.buildProject.mock.calls[1][3]).toBeNull();
  });
  it('Build: a project with its own file is saved by the build, so New does not ask', async () => {
    mocks.save.mockResolvedValue('C:\\P\\mod.dmb.json');
    await start();
    fireEvent.click(screen.getByRole('button',{name:'Save'}));
    await screen.findByText('Project saved');
    fireEvent.change(screen.getByLabelText('Project name'),{target:{value:'Edited name'}});
    nav('Build & install'); fireEvent.click(screen.getByRole('button',{name:'Analyze project'})); await screen.findByText('Ready to build',{exact:true});
    mocks.buildProject.mockImplementation(async (project,_o,_i,path) => ({project,path,result:built}));
    await buildMod();
    await inList().findByRole('button',{name:/Build completed/});
    expect(mocks.buildProject.mock.calls[0][3]).toBe('C:\\P\\mod.dmb.json');
    await waitFor(()=>expect(screen.getByRole('button',{name:'New'})).toBeEnabled());
    mocks.ask.mockClear();
    fireEvent.click(screen.getByRole('button',{name:'New'}));
    await waitFor(()=>expect(mocks.newProject).toHaveBeenCalledTimes(2));
    expect(mocks.ask).not.toHaveBeenCalledWith('This project has unsaved changes. Discard them and continue?',expect.anything());
  });
  it('the status bar says what is being done while it runs, never the failure title', async () => {
    await analyzed();
    let finish: (value: unknown) => void = () => undefined;
    mocks.buildProject.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    await buildMod();
    const status = () => document.querySelector('.statusbar-text')!.textContent;
    await waitFor(() => expect(status()).toBe('Building the mod…'));
    expect(status()).not.toContain('Build did not complete');
    finish({project:reviewedProject,path:'C:\\P\\mod.dmb.json',result:built});
    await waitFor(() => expect(status()).toBe('Ready'));
    // A failure still carries its own title.
    mocks.buildProject.mockRejectedValueOnce(failure);
    await buildMod();
    await waitFor(()=>expect(rows()[0]).toBe('Error: Build failed'));
  });
  it('the status bar falls back to a plain Working… for an operation it has no text for', async () => {
    const { runningText } = await import('./operationText');
    expect(runningText('Could not select the character')).toBe('Selecting the character…');
    expect(runningText('Your model could not be prepared automatically')).toBe('Preparing your model…');
    expect(runningText('Project analysis did not complete')).toBe('Analyzing…');
    expect(runningText('Something nobody named')).toBe('Working…');
    expect(runningText(null)).toBe('Working…');
  });
  it('Build from the kept list of the last analysis: no second Analyze, and the build is named as the check', async () => {
    const unacknowledged = {...structuredClone(reviewedProject), review_lods:false};
    mocks.inspectProject.mockResolvedValueOnce({project:unacknowledged,report:{...inspected,needs_review:true}});
    mocks.buildProject.mockImplementation(async project => ({project,path:'C:\\P\\mod.dmb.json',result:built}));
    await start(); nav('Build & install');
    fireEvent.click(screen.getByRole('button',{name:'Analyze project'}));
    await screen.findByRole('heading',{name:'1 issue blocking build'});
    expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
    fireEvent.click(screen.getByLabelText('I have reviewed the LOD appearance and performance tradeoffs'));
    expect(screen.getByRole('heading',{name:'Everything on the list from the last analysis is handled'})).toBeInTheDocument();
    await buildMod();
    await screen.findByRole('heading',{name:'The last build checked this project'});
    expect(mocks.inspectProject).toHaveBeenCalledTimes(1);
    expect(mocks.buildProject).toHaveBeenCalledWith(expect.objectContaining({review_lods:true}),true,false,null);
    expect(screen.queryByText('Current analysis')).toBeNull();
  });
  it('Build refused by its own check from the kept list: Build waits for Analyze; other failures change nothing', async () => {
    const unacknowledged = {...structuredClone(reviewedProject), review_lods:false};
    mocks.inspectProject.mockResolvedValueOnce({project:unacknowledged,report:{...inspected,needs_review:true}});
    await start(); nav('Build & install');
    fireEvent.click(screen.getByRole('button',{name:'Analyze project'}));
    await screen.findByRole('heading',{name:'1 issue blocking build'});
    fireEvent.click(screen.getByLabelText('I have reviewed the LOD appearance and performance tradeoffs'));
    // A compiler failure leaves the kept list ready.
    mocks.buildProject.mockRejectedValueOnce(failure);
    await buildMod();
    await screen.findByRole('alert');
    await waitFor(()=>expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled());
    expect(screen.getByRole('heading',{name:'Everything on the list from the last analysis is handled'})).toBeInTheDocument();
    // The build's own check refuses: Build is off and Analyze is the way on.
    mocks.buildProject.mockRejectedValueOnce({kind:'validation',message:'Review mesh mappings, materials, LODs and unskinned meshes before building. Inspection: C:\\w\\inspection.json',details:''});
    await buildMod();
    await screen.findByRole('heading',{name:'The last build found something that still needs a choice'});
    await waitFor(()=>expect(screen.getByRole('button',{name:'Analyze project'})).toBeEnabled());
    expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
    expect(screen.getByRole('button',{name:'Analyze project'})).toHaveClass('primary');
    expect(mocks.inspectProject).toHaveBeenCalledTimes(1);
    // Analyze clears it.
    fireEvent.click(screen.getByRole('button',{name:'Analyze project'}));
    await screen.findByText('Ready to build',{exact:true});
    expect(screen.queryByRole('heading',{name:'The last build found something that still needs a choice'})).toBeNull();
    expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled();
  });
  it('Install: installed and failed', async () => {
    await start(); nav('Build & install');
    mocks.open.mockResolvedValue('C:\\Out\\pak09_dir.vpk');
    mocks.installProject.mockResolvedValueOnce('C:\\Deadlock\\game\\citadel\\addons\\pak09_dir.vpk');
    fireEvent.click(screen.getByRole('button',{name:'Choose VPK to validate and install…'}));
    await waitFor(()=>expect(rows()).toEqual(['Success: Mod installed']));
    expect(await screen.findByText('VPK validated and installed.')).toBeInTheDocument();
    mocks.installProject.mockRejectedValueOnce({kind:'validation',message:'The VPK does not match the game',details:'hash mismatch'});
    await waitFor(()=>expect(screen.getByRole('button',{name:'Choose VPK to validate and install…'})).toBeEnabled());
    fireEvent.click(screen.getByRole('button',{name:'Choose VPK to validate and install…'}));
    await waitFor(()=>expect(rows()).toEqual(['Error: Installation failed','Success: Mod installed']));
  });
  it('Prepare model: prepared and failed', async () => {
    await start(); nav('Prepare model');
    fireEvent.click(screen.getByRole('button',{name:'Stub prepare'}));
    await waitFor(()=>expect(rows()).toEqual(['Success: Model prepared']));
    expect(screen.getByRole('heading',{name:'Appearance & weapons'})).toBeInTheDocument();
    nav('Prepare model');
    fireEvent.click(screen.getByRole('button',{name:'Stub prepare'}));
    await screen.findByRole('heading',{name:'Appearance & weapons'});
    expect(rows()).toEqual(['Success: Model prepared']);
    nav('Prepare model');
    fireEvent.click(screen.getByRole('button',{name:'Stub prepare fails'}));
    await waitFor(()=>expect(rows()).toEqual(['Error: Model preparation failed','Success: Model prepared']));
    expect(screen.getByRole('alert')).toHaveTextContent('Blender stopped');
  });
  it('Test my setup: passed, and failed naming the failed step', async () => {
    await start(); nav('Settings');
    mocks.testSetup.mockResolvedValueOnce({passed:true,steps:[{name:'Paths in Settings',passed:true,detail:'All good.'}]});
    fireEvent.click(screen.getByRole('button',{name:'Test my setup'}));
    await waitFor(()=>expect(rows()).toEqual(['Success: Setup test passed']));
    mocks.testSetup.mockResolvedValueOnce({passed:false,steps:[{name:'Paths in Settings',passed:true,detail:'All good.'},{name:'Compile a test material',passed:false,detail:'The compiler did not start.'}]});
    fireEvent.click(await screen.findByRole('button',{name:'Test again'}));
    await waitFor(()=>expect(rows()).toEqual(['Error: Setup test failed','Success: Setup test passed']));
    await waitFor(()=>expect(inList().getByRole('button',{name:/Setup test failed/})).toHaveAttribute('aria-expanded','true'));
    expect(inList().getByText('Failed: Compile a test material.')).toBeInTheDocument();
    mocks.testSetup.mockRejectedValueOnce({kind:'tool',message:'The test could not run',details:'no compiler'});
    fireEvent.click(await screen.findByRole('button',{name:'Test again'}));
    await waitFor(()=>expect(rows()).toEqual(['Error: Setup test failed','Error: Setup test failed','Success: Setup test passed']));
  });
  it('keeps small confirmations as toasts only', async () => {
    mocks.save.mockResolvedValue('C:\\P\\mod.dmb.json');
    await start();
    fireEvent.click(screen.getByRole('button',{name:'Save'}));
    expect(await screen.findByText('Project saved')).toBeInTheDocument();
    nav('Settings');
    fireEvent.change(screen.getByLabelText('Workspace folder'),{target:{value:'C:\\W'}});
    fireEvent.click(screen.getAllByRole('button',{name:'Save settings'})[0]);
    expect(await screen.findByText('Settings saved')).toBeInTheDocument();
    expect(inList().getByText('No notifications')).toBeInTheDocument();
  });
  it('keeps at most 20 events', async () => {
    await analyzed();
    mocks.buildProject.mockRejectedValue(failure);
    for (let index = 0; index < 21; index++) {
      await buildMod();
      await waitFor(()=>expect(mocks.buildProject).toHaveBeenCalledTimes(index + 1));
    }
    await waitFor(()=>expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled());
    expect(rows()).toHaveLength(20);
    expect(rows().every(row => row === 'Error: Build failed')).toBe(true);
  });
});
