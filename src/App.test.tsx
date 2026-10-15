import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import reference from '../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { AnalysisResult, Project, Settings, TimingReport } from './models';

const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),restoreAutosave:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),testSetup:vi.fn(),prebuildWarnings:vi.fn(),checkSettingFields:vi.fn(),heroProportions:vi.fn(),operationDurations:vi.fn(),buildProject:vi.fn(),  inspectProject:vi.fn(),analyzeMaterials:vi.fn(),importSourcePackage:vi.fn(),detectSettings:vi.fn(),setupStatus:vi.fn(),settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),openProject:vi.fn(),saveProject:vi.fn(),analyze:vi.fn(),cancel:vi.fn(),latestTimings:vi.fn(),timings:vi.fn(),logs:vi.fn(),logText:vi.fn(),saveLog:vi.fn(),
  invoke:vi.fn(),open:vi.fn(),save:vi.fn(),ask:vi.fn(),drop:vi.fn(),close:vi.fn(),desktop:true,
  discoverExtras:vi.fn(),previewExtra:vi.fn(),validateAssetPath:vi.fn(),
}));
vi.mock('./api', async original => ({...await original<typeof import('./api')>(),isDesktop:() => mocks.desktop,api:mocks}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/api/core', async original => {
  const core=await original<typeof import('@tauri-apps/api/core')>();
  return {...core,invoke:(command:string,args:Record<string,unknown>)=>command==='validate_blender'?mocks.invoke(command,args):core.invoke(command,args)};
});
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:mocks.drop,onCloseRequested:mocks.close,destroy:vi.fn()})}));
import { App, AUTOSAVE_MS } from './App';
import { inspected, reviewedProject } from './build/testFixtures';

const draft = reference.cases[0].expected as unknown as Project;
const settings: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:''};
const timings: TimingReport = {schema_version:1,operation:'DMX analysis',run_id:'test',started_utc:'2026-10-06T00:00:00Z',finished_utc:'2026-10-06T00:00:01Z',status:'succeeded',total_ns:1e9,steps:[],events:[]};
const analyzed: AnalysisResult = {summary:{path:'C:\\Models\\模型.dmx',encoding:'binary 9',format:'model',format_version:22,meshes:[{name:'body',element:1,vertices:100,indexed_vertices:300,weight_values:400,joint_index_values:400,has_skin_weights:true,vertex_attributes:[]}],materials:['body'],material_identities:[],bones:['root'],elements:10},timings};
beforeEach(() => {
  vi.clearAllMocks(); mocks.desktop = true;
  mocks.invoke.mockReset();
  mocks.newProject.mockResolvedValue({path:null,project:structuredClone(draft)});
  mocks.settings.mockResolvedValue(settings); mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null);
  mocks.drop.mockImplementation(async () => vi.fn()); mocks.close.mockImplementation(async () => vi.fn());
  mocks.ask.mockResolvedValue(true); mocks.analyze.mockResolvedValue(analyzed);
  mocks.saveSettings.mockResolvedValue(undefined);
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
describe('Connected desktop workspace', () => {
  it('keeps New and Save as available with discard protection and a separate save destination', async () => {
    const stored = {...structuredClone(draft),name:'Existing Project',customization:{future:{preserved:true}}};
    mocks.open.mockResolvedValue('C:\\Projects\\original.dmb.json');
    mocks.openProject.mockResolvedValue({path:'C:\\Projects\\original.dmb.json',project:stored});
    mocks.save.mockResolvedValue('C:\\Projects\\copy.dmb.json');
    mocks.saveProject.mockImplementation(async (path,project) => ({path,project}));
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.click(screen.getByRole('button',{name:'Open…'}));
    await screen.findByDisplayValue('Existing Project');
    fireEvent.click(screen.getByRole('button',{name:'Save as…'}));
    await waitFor(()=>expect(mocks.saveProject).toHaveBeenCalledWith('C:\\Projects\\copy.dmb.json',stored));
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({defaultPath:'C:\\Projects\\original.dmb.json'}));
    await waitFor(()=>expect(screen.getByRole('button',{name:'New'})).toBeEnabled());
    fireEvent.change(screen.getByLabelText('Project name'),{target:{value:'Unsaved changes'}});
    mocks.ask.mockResolvedValueOnce(false);
    fireEvent.click(screen.getByRole('button',{name:'New'}));
    await waitFor(()=>expect(mocks.ask).toHaveBeenCalledOnce());
    await waitFor(()=>expect(screen.getByRole('button',{name:'New'})).toBeEnabled());
    expect(mocks.newProject).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText('Project name')).toHaveValue('Unsaved changes');
    fireEvent.click(screen.getByRole('button',{name:'New'}));
    await waitFor(()=>expect(mocks.newProject).toHaveBeenCalledTimes(2));
    await waitFor(()=>expect(screen.getByLabelText('Project name')).toHaveValue(draft.name));
    expect(screen.getByRole('heading',{name:'Character & model'})).toBeInTheDocument();
  });
  it('retains all saved project fields while editing the name and saving', async () => {
    const stored = structuredClone(reference.cases[1].expected) as unknown as Project;
    stored.customization = {physics:{enabled:true,source:'cloth.vmdl_c'},future_extension:{values:[1,2,3]}};
    mocks.open.mockResolvedValue('C:\\Projects\\existing.json');
    mocks.openProject.mockResolvedValue({path:'C:\\Projects\\existing.json',project:stored});
    mocks.saveProject.mockImplementation(async (path,project) => ({path,project}));
    render(<App />);
    await screen.findByLabelText('Project name');
    fireEvent.click(screen.getByRole('button',{name:'Open…'}));
    await screen.findByDisplayValue('vindicta');
    fireEvent.change(screen.getByLabelText('Project name'),{target:{value:'Vindicta Balanced'}});
    fireEvent.click(screen.getByRole('button',{name:'Save'}));
    await waitFor(() => expect(mocks.saveProject).toHaveBeenCalledOnce());
    expect(mocks.saveProject.mock.calls[0]).toEqual(['C:\\Projects\\existing.json',{...stored,name:'Vindicta Balanced'}]);
    expect(await screen.findByRole('status')).toHaveTextContent('Project saved');
  });
  it('routes native browse and drop through identical Rust analysis and exposes measured timings', async () => {
    mocks.open.mockResolvedValue(analyzed.summary.path);
    render(<App />);
    await screen.findByLabelText('Project name');
    fireEvent.click(screen.getByRole('button',{name:'Appearance & weapons'}));
    fireEvent.click(screen.getByRole('button',{name:'Choose model…'}));
    await screen.findByText('✓ Model read');
    expect(mocks.analyze).toHaveBeenCalledWith(analyzed.summary.path);
    expect(screen.getByText('Compatibility validation still required')).toBeInTheDocument();
    const handler = mocks.drop.mock.calls[0][0];
    fireEvent.click(screen.getByRole('button',{name:'Character & model'}));
    act(() => handler({payload:{type:'enter',paths:[analyzed.summary.path],position:{x:1,y:1}}}));
    act(() => handler({payload:{type:'over',position:{x:2,y:2}}}));
    expect(screen.getByRole('button',{name:'Character & model'})).toHaveAttribute('aria-current','page');
    await act(async () => handler({payload:{type:'drop',paths:[analyzed.summary.path],position:{x:1,y:1}}}));
    expect(mocks.analyze).toHaveBeenCalledTimes(2);
    expect(screen.getByRole('button',{name:'Appearance & weapons'})).toHaveAttribute('aria-current','page');
    fireEvent.click(screen.getByRole('button',{name:'Step timings'}));
    expect(screen.getByTestId('total-duration')).toHaveTextContent('1.000 s');
  });
  it('keeps Extras and the selected icon mounted while a native image drag crosses the window into its replacement box', async () => {
    const project = structuredClone(reference.cases[1].expected) as unknown as Project;
    const target = 'panorama/images/heroes/hero_test.vtex_c', source = 'C:\\Art\\portrait.png';
    mocks.newProject.mockResolvedValue({path:null,project});
    mocks.settings.mockResolvedValue({...settings,pak:'C:\\Deadlock\\pak01_dir.vpk'});
    mocks.discoverExtras.mockResolvedValue({hero_path:project.original_model,icons:[{path:target,target,label:'Hero portrait',supported:true,width:128,height:128}],audio:[],effects:[],animations:[],variants:[],animation_unavailable:null,warnings:[]});
    mocks.validateAssetPath.mockImplementation(async path => path);
    const header = new TextEncoder().encode(JSON.stringify({mime_type:'image/png',details:{}}));
    const packet = new ArrayBuffer(4+header.length);
    new DataView(packet).setUint32(0,header.length,true);
    new Uint8Array(packet,4).set(header);
    mocks.previewExtra.mockResolvedValue(packet);
    vi.stubGlobal('URL', class extends URL { static createObjectURL() { return 'blob:test-icon'; } static revokeObjectURL() {} });
    vi.stubGlobal('devicePixelRatio',2);
    mocks.save.mockResolvedValue('C:\\Projects\\icon.dmb.json');
    mocks.saveProject.mockImplementation(async (path,project) => ({path,project}));
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.click(screen.getByRole('button',{name:'Extras'}));
    fireEvent.click(await screen.findByRole('button',{name:/Hero portrait/}));
    const box = screen.getByText('Drop your icon here');
    const hitTest = vi.spyOn(document,'elementFromPoint').mockReturnValue(null);
    const handler = mocks.drop.mock.calls[0][0];
    // Windows sends enter before the cursor reaches the box. The page must
    // remain mounted so hit testing can still find that box on drop.
    act(() => handler({payload:{type:'enter',paths:[source],position:{x:2,y:2}}}));
    act(() => handler({payload:{type:'over',position:{x:4,y:4}}}));
    expect(screen.getByRole('button',{name:'Extras'})).toHaveAttribute('aria-current','page');
    expect(box).toBeInTheDocument();
    act(() => handler({payload:{type:'leave'}}));
    expect(box).toBeInTheDocument();
    act(() => handler({payload:{type:'enter',paths:[source],position:{x:2,y:2}}}));
    hitTest.mockReturnValue(box);
    act(() => handler({payload:{type:'over',position:{x:600,y:400}}}));
    await act(async () => handler({payload:{type:'drop',paths:[source],position:{x:600,y:400}}}));
    expect(hitTest).toHaveBeenLastCalledWith(300,200);
    expect(mocks.validateAssetPath).toHaveBeenCalledExactlyOnceWith(source,'icon');
    expect(screen.getByRole('button',{name:'Extras'})).toHaveAttribute('aria-current','page');
    expect(await screen.findByText('portrait.png',{selector:'.extra-dropzone strong'})).toBeVisible();
    expect(mocks.analyze).not.toHaveBeenCalled();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Save'}));
    await waitFor(() => expect(mocks.saveProject).toHaveBeenCalledWith('C:\\Projects\\icon.dmb.json',expect.objectContaining({customization:expect.objectContaining({icons:[expect.objectContaining({target,source,fit:'contain'})]})})));
  });
  it('rejects unsupported drops without calling the parser', async () => {
    render(<App />); await screen.findByLabelText('Project name');
    const handler = mocks.drop.mock.calls[0][0];
    act(() => handler({payload:{type:'drop',paths:['C:\\Models\\bad.exe'],position:{x:1,y:1}}}));
    expect(mocks.analyze).not.toHaveBeenCalled();
    expect(screen.getByRole('alert')).toHaveTextContent('Drop one local file with the .dmx extension, or a Blender, GLB, glTF, or FBX source.');
  });
  it('keeps failed-analysis timing evidence and surfaces complete error details', async () => {
    const report = {...timings,status:'failed' as const};
    mocks.analyze.mockRejectedValue({kind:'validation',message:'Model is malformed',details:'Unexpected binary block at byte 500'});
    mocks.open.mockResolvedValue('C:\\Models\\bad.dmx');
    render(<App />); await screen.findByLabelText('Project name');
    mocks.latestTimings.mockResolvedValue(report);
    fireEvent.click(screen.getByRole('button',{name:'Appearance & weapons'}));
    fireEvent.click(screen.getByRole('button',{name:'Choose model…'}));
    await screen.findByRole('alert');
    expect(screen.getByRole('alert')).toHaveTextContent('Unexpected binary block at byte 500');
    fireEvent.click(screen.getByRole('button',{name:'Step timings'}));
    expect(screen.getByText(/Failed/)).toBeInTheDocument();
    expect(screen.getByTestId('total-duration')).toHaveTextContent('1.000 s');
  });
  it('imports verified source data and material plans through the native service before saving', async () => {
    const imported={...structuredClone(draft),dmx:'C:\\workspace\\source.dmx',original_model:'models/heroes/test.vmdl',source_overlay:'C:\\workspace\\overlay',expected_dmx_sha256:'a'.repeat(64),customization:{future:{retained:true}}};
    const planned={...imported,materials:{'materials/test.vmat':{preset:'cloth',mode:'generate',confirmed:false,textures:{color:'C:\\textures\\cloth.png'}}}};
    mocks.open.mockResolvedValue('C:\\source-package.zip');mocks.importSourcePackage.mockResolvedValue(imported);mocks.analyzeMaterials.mockResolvedValue(planned);
    mocks.save.mockResolvedValue('C:\\project.json');mocks.saveProject.mockImplementation(async(path,project)=>({path,project}));
    render(<App/>);await screen.findByLabelText('Project name');fireEvent.click(screen.getByRole('button',{name:'Appearance & weapons'}));
    fireEvent.click(screen.getByRole('button',{name:'Import source package…'}));await screen.findByText('Source package imported and verified');
    expect(mocks.importSourcePackage).toHaveBeenCalledWith('C:\\source-package.zip',draft);
    fireEvent.click(screen.getByRole('button',{name:'Analyze materials'}));await screen.findByText('Material analysis complete. Review assignments before building.');
    expect(mocks.analyzeMaterials).toHaveBeenCalledWith(imported);fireEvent.click(screen.getByRole('button',{name:'Save'}));
    await waitFor(()=>expect(mocks.saveProject).toHaveBeenCalledWith('C:\\project.json',planned));
  });
  it('detects real tool paths, shows presence checks, and saves only on request', async () => {
    const detected={...settings,deadlock:'C:\\Deadlock',pak:'C:\\Deadlock\\pak01_dir.vpk'};
    mocks.detectSettings.mockResolvedValue(detected);mocks.setupStatus.mockResolvedValue({files_found:{Deadlock:true,'Resource compiler':false},model_status:'model setup needed'});
    render(<App/>);await screen.findByLabelText('Project name');fireEvent.click(screen.getByRole('button',{name:'Settings'}));fireEvent.click(screen.getByRole('button',{name:'Detect tools'}));
    await screen.findByText('Detection complete. Review and save settings.');expect(mocks.detectSettings).toHaveBeenCalledWith(settings,[]);expect(screen.getByDisplayValue('C:\\Deadlock')).toBeInTheDocument();
    expect(screen.getByText('Not found')).toBeInTheDocument();expect(mocks.saveSettings).not.toHaveBeenCalled();fireEvent.click(screen.getByRole('button',{name:'Save settings'}));await waitFor(()=>expect(mocks.saveSettings).toHaveBeenCalledWith(detected));
  });
  it('treats a successfully validated Blender path as saved immediately and clears stale validation after editing it', async () => {
    const path = 'C:\\Program Files\\Blender Foundation\\Blender 5.2\\blender.exe';
    mocks.open.mockResolvedValue(path);
    mocks.invoke.mockResolvedValue({path,version:'5.2.2',launch_verified:true,conversion_certified:false});
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));
    fireEvent.click(screen.getByText('Advanced configuration',{selector:'summary'}));
    fireEvent.click(screen.getByRole('button',{name:'Browse Blender executable'}));
    await waitFor(()=>expect(screen.getByLabelText('Blender executable')).toHaveValue(path));
    expect(screen.getByText('Settings unsaved')).toBeVisible();
    fireEvent.click(screen.getByText('Tool diagnostics',{selector:'summary span'}));
    fireEvent.click(screen.getByRole('button',{name:'Validate Blender'}));
    await screen.findByText(/Blender 5.2.2 launched successfully and its path is saved/);
    expect(mocks.invoke).toHaveBeenCalledWith('validate_blender',{executable:path});
    await waitFor(()=>expect(screen.queryByText('Settings unsaved')).not.toBeInTheDocument());
    expect(screen.getByRole('button',{name:'Save settings'})).toBeDisabled();
    // Validation commits only Blender natively; the frontend must not write a
    // whole settings snapshot or silently overwrite other pending edits.
    expect(mocks.saveSettings).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));
    expect(screen.queryByText('Tool settings have unsaved changes')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));
    fireEvent.click(screen.getByText('Advanced configuration',{selector:'summary'}));
    fireEvent.click(screen.getByText('Tool diagnostics',{selector:'summary span'}));
    fireEvent.click(screen.getByRole('button',{name:'Validate Blender'}));
    await screen.findByText(/Blender 5.2.2 launched successfully and its path is saved/);
    await waitFor(()=>expect(screen.getByRole('button',{name:'Validate Blender'})).toBeEnabled());
    fireEvent.change(screen.getByLabelText('Blender executable'),{target:{value:'C:\\Other Blender\\blender.exe'}});
    expect(screen.queryByText(/Blender 5.2.2 launched successfully and its path is saved/)).not.toBeInTheDocument();
    expect(screen.getByText('Settings unsaved')).toBeVisible();
  });
  it('preserves other pending settings after Blender validation and offers saving them from preparation', async () => {
    const path = 'C:\\Blender 5.2\\blender.exe';
    mocks.invoke.mockResolvedValue({path,version:'5.2.2',launch_verified:true,conversion_certified:false});
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.change(screen.getByLabelText('Project name'),{target:{value:'Keep mapping and cloth project'}});
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));
    fireEvent.change(screen.getByLabelText('Workspace folder'),{target:{value:'C:\\Pending workspace'}});
    fireEvent.click(screen.getByText('Advanced configuration',{selector:'summary'}));
    fireEvent.change(screen.getByLabelText('Blender executable'),{target:{value:path}});
    fireEvent.click(screen.getByText('Tool diagnostics',{selector:'summary span'}));
    fireEvent.click(screen.getByRole('button',{name:'Validate Blender'}));
    await screen.findByText(/Blender 5.2.2 launched successfully and its path is saved/);
    expect(screen.getByText('Settings unsaved')).toBeVisible();
    expect(screen.getByLabelText('Workspace folder')).toHaveValue('C:\\Pending workspace');
    expect(mocks.saveSettings).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));
    expect(screen.getByText('Tool settings have unsaved changes')).toBeVisible();
    await waitFor(()=>expect(screen.getByRole('button',{name:'Save settings'})).toBeEnabled());
    fireEvent.click(screen.getByRole('button',{name:'Save settings'}));
    await waitFor(()=>expect(mocks.saveSettings).toHaveBeenCalledWith({...settings,blender:path,workspace:'C:\\Pending workspace'}));
    await waitFor(()=>expect(screen.queryByText('Tool settings have unsaved changes')).not.toBeInTheDocument());
    fireEvent.click(screen.getByRole('button',{name:'Character & model'}));
    expect(screen.getByLabelText('Project name')).toHaveValue('Keep mapping and cloth project');
  });
  it('does not claim Blender is saved when validation cannot persist the path', async () => {
    const path = 'C:\\Blender 5.2\\blender.exe';
    mocks.invoke.mockRejectedValue({kind:'io',message:'Settings could not be saved',details:'The settings folder could not be written.'});
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));
    fireEvent.click(screen.getByText('Advanced configuration',{selector:'summary'}));
    fireEvent.change(screen.getByLabelText('Blender executable'),{target:{value:path}});
    fireEvent.click(screen.getByText('Tool diagnostics',{selector:'summary span'}));
    fireEvent.click(screen.getByRole('button',{name:'Validate Blender'}));
    expect(await screen.findByRole('alert')).toHaveTextContent('Settings could not be saved');
    expect(screen.queryByText(/launched successfully and its path is saved/)).not.toBeInTheDocument();
    expect(screen.getByText('Settings unsaved')).toBeVisible();
    expect(screen.getByLabelText('Blender executable')).toHaveValue(path);
    expect(mocks.saveSettings).not.toHaveBeenCalled();
  });
  it('opens the new-user guide from Settings and keeps the current project intact', async () => {
    render(<App/>); await screen.findByLabelText('Project name');
    fireEvent.change(screen.getByLabelText('Project name'),{target:{value:'Vindicta Balanced'}});
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));
    fireEvent.click(screen.getByRole('button',{name:'Open new-user setup'}));
    expect(screen.getByRole('heading',{name:'New-user setup'})).toBeInTheDocument();
    expect(screen.getByText('Vindicta Balanced',{selector:'dd'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'2. Game and tools'}));
    expect(screen.getByRole('button',{name:'Detect tools'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Back to Settings'}));
    expect(screen.getAllByLabelText('Workspace folder')).toHaveLength(1);
    fireEvent.click(screen.getByRole('button',{name:'Open new-user setup'}));
    expect(screen.getByRole('button',{name:'2. Game and tools'})).toHaveAttribute('aria-current','step');
    fireEvent.click(screen.getByRole('button',{name:'4. Hair and cloth'}));
    expect(screen.getByText('Fine-tune movement',{selector:'strong'})).toBeInTheDocument();
    expect(screen.queryByText(/Vindicta R12/)).toBeNull();
    fireEvent.click(screen.getByRole('button',{name:'Open Physics'}));
    expect(screen.getByRole('heading',{name:'Physics & preview'})).toBeInTheDocument();
    // The guide stays in the sidebar while it is in progress, at the step left off.
    const guide=screen.getByRole('button',{name:'New-user setup'});
    expect(guide.closest('.nav-group')?.textContent).toContain('Getting started');
    fireEvent.click(guide);
    expect(screen.getByRole('button',{name:'4. Hair and cloth'})).toHaveAttribute('aria-current','step');
    fireEvent.click(screen.getByRole('button',{name:'5. Build'}));
    fireEvent.click(screen.getByRole('button',{name:'Done'}));
    expect(screen.getByRole('heading',{name:'Character & model'})).toBeInTheDocument();
    expect(screen.queryByRole('button',{name:'New-user setup'})).toBeNull();
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));
    fireEvent.click(screen.getByRole('button',{name:'Open new-user setup'}));
    expect(screen.getByRole('button',{name:'New-user setup'})).toBeInTheDocument();
    expect(mocks.saveSettings).not.toHaveBeenCalled();
    expect(mocks.newProject).toHaveBeenCalledTimes(1);
  });
  it('does not load fake backend data when opened in an ordinary browser', () => {
    mocks.desktop = false;
    render(<App />);
    expect(screen.getByText('Desktop connection required')).toBeInTheDocument();
    expect(mocks.newProject).not.toHaveBeenCalled();
    expect(screen.getByRole('button',{name:'Open…'})).toBeDisabled();
    expect(screen.queryByRole('button',{name:'Build Mod'})).not.toBeInTheDocument();
  });
  it('saves and checks tools before finishing without a first project or losing the draft', async () => {
    const complete = {files_found:{Deadlock:true,'Original VPK':true,CSDK:true,'Resource compiler':true,'Source 2 Viewer CLI':true},model_status:'model files found',model_files_found:true};
    mocks.setupStatus.mockResolvedValue(complete);
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.change(screen.getByLabelText('Project name'),{target:{value:'Keep My Project'}});
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));fireEvent.click(screen.getByRole('button',{name:'Open new-user setup'}));fireEvent.click(screen.getByRole('button',{name:'2. Game and tools'}));
    fireEvent.change(screen.getByLabelText('Workspace folder'),{target:{value:'C:\\ModWorkspace'}});
    expect(screen.getByRole('button',{name:'Done for now'})).toBeDisabled();
    fireEvent.click(screen.getByRole('button',{name:'Save and check tools'}));
    await screen.findByRole('heading',{name:'Tool locations checked · settings saved'});
    expect(mocks.saveSettings).toHaveBeenCalledWith({...settings,workspace:'C:\\ModWorkspace'});
    expect(mocks.saveSettings.mock.invocationCallOrder[0]).toBeLessThan(mocks.setupStatus.mock.invocationCallOrder[0]);
    fireEvent.click(screen.getByRole('button',{name:'Done for now'}));
    expect(screen.getByLabelText('Project name')).toHaveValue('Keep My Project');
    expect(mocks.newProject).toHaveBeenCalledTimes(1);expect(mocks.analyze).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));fireEvent.click(screen.getByRole('button',{name:'Open new-user setup'}));
    fireEvent.click(screen.getByRole('button',{name:'Continue to first project'}));
    expect(screen.getByRole('button',{name:'3. Character and model'})).toHaveAttribute('aria-current','step');
    expect(screen.getByRole('heading',{name:'Choose a character and replacement model'})).toBeInTheDocument();
  });
  it('keeps unsaved tool edits in the guide when saving fails', async () => {
    mocks.saveSettings.mockRejectedValue({kind:'validation',message:'Settings are read-only',details:'Settings could not be saved.'});
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));fireEvent.click(screen.getByRole('button',{name:'Open new-user setup'}));fireEvent.click(screen.getByRole('button',{name:'2. Game and tools'}));
    fireEvent.change(screen.getByLabelText('Workspace folder'),{target:{value:'C:\\ModWorkspace'}});
    fireEvent.click(screen.getByRole('button',{name:'Save and check tools'}));await screen.findByRole('alert');
    expect(mocks.setupStatus).not.toHaveBeenCalled();
    expect(screen.getByRole('button',{name:'Done for now'})).toBeDisabled();
    expect(screen.getByRole('button',{name:'Continue to first project'})).toBeDisabled();
    expect(screen.getByLabelText('Workspace folder')).toHaveValue('C:\\ModWorkspace');
  });
  it('clears prior setup success after a failed recheck or edited path', async () => {
    mocks.settings.mockResolvedValue({...settings,workspace:'C:\\ModWorkspace'});
    mocks.setupStatus.mockResolvedValue({files_found:{Deadlock:true,'Original VPK':true,CSDK:true,'Resource compiler':true,'Source 2 Viewer CLI':true},model_status:'model files found',model_files_found:true});
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));fireEvent.click(screen.getByRole('button',{name:'Open new-user setup'}));fireEvent.click(screen.getByRole('button',{name:'2. Game and tools'}));
    fireEvent.click(screen.getByRole('button',{name:'Save and check tools'}));await screen.findByRole('heading',{name:'Tool locations checked · settings saved'});
    mocks.setupStatus.mockRejectedValue({kind:'validation',message:'Tool check unavailable',details:'Could not read tool files.'});
    fireEvent.click(screen.getByRole('button',{name:'Save and check tools'}));await screen.findByRole('alert');
    expect(screen.queryByRole('heading',{name:'Tool locations checked · settings saved'})).not.toBeInTheDocument();
    expect(screen.getByRole('heading',{name:'Check your tool locations'})).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Source 2 Viewer CLI'),{target:{value:'C:\\AnotherReader.exe'}});
    expect(screen.getByRole('heading',{name:'Save your settings to finish this stage'})).toBeInTheDocument();
    expect(screen.getByRole('button',{name:'Done for now'})).toBeDisabled();
  });
  it('invalidates readiness after project edits and clears a prior approval when reanalysis fails', async () => {
    mocks.newProject.mockResolvedValue({path:null,project:structuredClone(reviewedProject)});
    mocks.inspectProject.mockResolvedValue({project:structuredClone(reviewedProject),report:inspected});
    render(<App/>);await screen.findByLabelText('Project name');
    const navigate=(name:string)=>fireEvent.click(within(screen.getByRole('navigation',{name:'Main navigation'})).getByRole('button',{name}));
    navigate('Build & install');fireEvent.click(screen.getByRole('button',{name:'Analyze project'}));
    await screen.findByText('Ready to build',{exact:true});
    expect(mocks.inspectProject).toHaveBeenCalledWith(reviewedProject);
    navigate('Character & model');fireEvent.change(screen.getByLabelText('Project name'),{target:{value:'Changed project'}});
    navigate('Build & install');expect(screen.queryByText('Ready to build',{exact:true})).not.toBeInTheDocument();
    mocks.inspectProject.mockRejectedValue({kind:'validation',message:'Source input changed',details:'Fresh native validation failed.'});
    fireEvent.click(screen.getByRole('button',{name:'Analyze project'}));await screen.findByRole('alert');
    expect(screen.getByRole('alert')).toHaveTextContent('Fresh native validation failed');
    expect(screen.queryByText('Ready to build',{exact:true})).not.toBeInTheDocument();
    expect(screen.getByText('Analyze to check compatibility')).toBeInTheDocument();
  });

});

describe('Phase 1 helpers in the app', () => {
  it('opens the setup guide on first start, and once per session when a required path is wrong', async () => {
    localStorage.removeItem('dmb.setupGuideSeen');
    render(<App/>);
    expect(await screen.findByRole('heading',{name:'New-user setup'})).toBeInTheDocument();
    expect(localStorage.getItem('dmb.setupGuideSeen')).toBe('1');
    cleanup(); sessionStorage.clear();
    mocks.checkSettingFields.mockResolvedValue([{field:'csdk',state:'problem',message:'Nested'}]);
    render(<App/>);
    expect(await screen.findByRole('heading',{name:'New-user setup'})).toBeInTheDocument();
    expect(screen.getByRole('button',{name:'2. Game and tools'})).toHaveAttribute('aria-current','step');
    cleanup();
    render(<App/>);await screen.findByLabelText('Project name');
    expect(screen.queryByRole('heading',{name:'New-user setup'})).not.toBeInTheDocument();
  });
  it('opens the five steps with Ctrl+1 to Ctrl+5', async () => {
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.keyDown(window,{key:'5',ctrlKey:true});
    expect(await screen.findByRole('heading',{name:'Build & install'})).toBeInTheDocument();
    fireEvent.keyDown(window,{key:'1',ctrlKey:true});
    expect(await screen.findByRole('heading',{name:'Character & model'})).toBeInTheDocument();
  });
  it('shows the proportion notice when the hero has extreme proportions', async () => {
    mocks.heroProportions.mockResolvedValue({extremes:['arms'],arm_left:0.955,arm_right:1.19,legs:1.825});
    mocks.newProject.mockResolvedValue({path:null,project:{...structuredClone(draft),original_model:'models/heroes_staging/shiv/shiv.vmdl_c',hero_name:'shiv'}});
    render(<App/>);
    expect(await screen.findByRole('note')).toHaveTextContent('a long right arm');
  });
});

describe('Simple and Expert interface', () => {
  it('starts Simple with the five steps only, and the top bar switches to Expert', async () => {
    localStorage.removeItem('dmb.uiMode');
    render(<App/>);await screen.findByLabelText('Project name');
    for (const name of ['Motion','Extras','Step timings']) expect(screen.queryByRole('button',{name})).not.toBeInTheDocument();
    // A sidebar section with nothing to show in Simple has no heading either.
    expect(screen.queryByText('More customization')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Settings'}));
    expect(screen.queryByText('Tool diagnostics',{selector:'summary span'})).not.toBeInTheDocument();
    fireEvent.click(within(screen.getByRole('radiogroup',{name:'Interface mode'})).getByRole('radio',{name:'Expert'}));
    expect(screen.getByRole('button',{name:'Extras'})).toBeInTheDocument();
    expect(screen.getByText('More customization')).toBeInTheDocument();
    expect(screen.getByText('Tool diagnostics',{selector:'summary span'})).toBeInTheDocument();
    expect(localStorage.getItem('dmb.uiMode')).toBe('expert');
  });
});

describe('Gun file before automatic preparation', () => {
  it('asks for a separate gun file first, so it is prepared together with the character', async () => {
    mocks.newProject.mockResolvedValue({path:null,project:{...structuredClone(draft),original_model:'models/heroes_staging/haze/haze.vmdl_c',hero_name:'haze'}});
    mocks.open.mockResolvedValueOnce('C:\Models\hero.blend').mockResolvedValueOnce('C:\Models\gun.blend');
    mocks.validateAssetPath.mockImplementation(async (path:string) => path);
    const seen:string[] = [];const listener=(event:Event)=>seen.push((event as CustomEvent<{path:string}>).detail.path);
    window.addEventListener('dmb-model-source', listener);
    render(<App/>);await screen.findByLabelText('Project name');
    fireEvent.click(screen.getAllByRole('button',{name:'Choose model file…'})[0]);
    await waitFor(()=>expect(seen).toEqual(['C:\Models\hero.blend']));
    expect(mocks.ask).toHaveBeenCalledWith(expect.stringContaining('Is your gun in a separate file'),expect.objectContaining({okLabel:'Choose gun file…'}));
    expect(mocks.validateAssetPath).toHaveBeenCalledWith('C:\Models\gun.blend','weapon');
    window.removeEventListener('dmb-model-source', listener);
  });
});

describe('Autosave and recovery', () => {
  it('offers the newest autosave after a crash and restores it as unsaved changes', async () => {
    mocks.sessionRecovery.mockResolvedValue({unclean:true,autosaves:[{file:'1760000000000.json',id:draft.id,name:'Crashed Yvonne',path:'C:\Projects\yvonne.dmb.json',saved_ms:1760000000000}]});
    mocks.restoreAutosave.mockResolvedValue({path:'C:\Projects\yvonne.dmb.json',project:{...structuredClone(draft),name:'Crashed Yvonne'}});
    render(<App/>);
    await waitFor(()=>expect(screen.getByLabelText('Project name')).toHaveValue('Crashed Yvonne'));
    expect(mocks.ask).toHaveBeenCalledWith(expect.stringContaining('did not close normally'),expect.objectContaining({okLabel:'Restore'}));
    expect(mocks.restoreAutosave).toHaveBeenCalledWith(draft.id,'1760000000000.json');
    expect(mocks.dismissRecovery).toHaveBeenCalled();
    expect(screen.getByLabelText('Unsaved project changes')).toBeInTheDocument();
  });
  it('autosaves only unsaved changes and forgets autosaves after saving', async () => {
    const intervals = vi.spyOn(window,'setInterval');
    mocks.save.mockResolvedValue('C:\Projects\yvonne.dmb.json');
    mocks.saveProject.mockImplementation(async (path,project) => ({path,project}));
    render(<App/>);await screen.findByLabelText('Project name');
    const tick = () => { for (const call of intervals.mock.calls) if (call[1] === AUTOSAVE_MS) (call[0] as () => void)(); };
    tick();
    expect(mocks.autosaveProject).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText('Project name'),{target:{value:'Autosaved'}});
    tick();
    await waitFor(()=>expect(mocks.autosaveProject).toHaveBeenCalledWith(expect.objectContaining({project:expect.objectContaining({name:'Autosaved'})})));
    fireEvent.click(screen.getByRole('button',{name:'Save'}));
    await waitFor(()=>expect(mocks.clearAutosaves).toHaveBeenCalledWith(draft.id));
    intervals.mockRestore();
  });
});
