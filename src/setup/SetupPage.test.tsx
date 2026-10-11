import {fireEvent,render,screen,waitFor} from '@testing-library/react';
import {beforeEach,describe,expect,it,vi} from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type {Project,Settings} from '../models';
import type {AcquisitionResult} from './models';
const mocks=vi.hoisted(()=>({invoke:vi.fn(),open:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke:mocks.invoke}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:mocks.open}));
import {SetupPage} from './SetupPage';
const settings:Settings={steam:'',deadlock:'game',pak:'game/pak01_dir.vpk',csdk:'',resourcecompiler:'material compiler',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'reader',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'work',last_project:'',blender:''};
const project=reference.cases[1].expected as unknown as Project;
const defaults={options:{dd:'C:\\Tools\\dd.exe',toolroot:'C:\\Acquired\\toolset',manifests:'C:\\Acquired\\manifests'},export_root:'C:\\Plans',automatic_root:'C:\\Acquired'};
const plan:AcquisitionResult={plan:{schema:1,kind:'tool-plan',steam_build:'123',branch_snapshot_utc:'date',depots:[{depot:'777',manifest:'888',whole_depot_bytes:5,selected_files:1,selected_bytes:5}],manifest_sha256:{},filelists:{},transfer_uncompressed_bytes:5,installed_bytes:5,files:[{path:'game/bin/compiler.exe',bytes:5,sha1:'a'.repeat(40),directory:false}],overridden_paths:{},native_compile_passed:false,absolute_minimum_proven:false,scope:'Pinned files only',destination:defaults.options.toolroot,estimated_download_bytes:null},plan_digest:'b'.repeat(64),downloaded:false,receipt:null};
const operate=async(_label:string,fn:()=>Promise<void>)=>{await fn();};
beforeEach(()=>{vi.clearAllMocks();mocks.invoke.mockImplementation(async(name,args)=>{if(name==='setup_defaults')return defaults;if(name==='setup_acquire')return {...plan,downloaded:args.action==='download'};throw new Error(`Unexpected command ${name}`);});});
describe('Real setup command wiring',()=>{
  it('sends only the reviewed exact plan digest and invalidates it when destination changes',async()=>{
    render(<SetupPage settings={settings} project={project} updateSettings={vi.fn()} busy={false} operate={operate}/>);
    await screen.findByDisplayValue(defaults.automatic_root);expect(screen.queryByRole('button',{name:'Download these 1 files'})).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Fetch manifests and plan'}));await screen.findByRole('button',{name:'Download these 1 files'});
    expect(mocks.invoke).toHaveBeenCalledWith('setup_acquire',{action:'manifests',root:defaults.automatic_root,dd:defaults.options.dd,auth:{mode:'qr'},approvedPlanDigest:null});
    fireEvent.click(screen.getByRole('button',{name:'Download these 1 files'}));await waitFor(()=>expect(mocks.invoke).toHaveBeenCalledWith('setup_acquire',{action:'download',root:defaults.automatic_root,dd:defaults.options.dd,auth:{mode:'qr'},approvedPlanDigest:plan.plan_digest}));
    fireEvent.change(screen.getByLabelText('Acquisition folder'),{target:{value:'C:\\Different'}});expect(screen.queryByRole('button',{name:'Download these 1 files'})).not.toBeInTheDocument();
  });
  it('uses the native selected settings without saving or claiming compatibility',async()=>{
    const next={...settings,cswin:defaults.options.toolroot,model_backend:'cswin'};const update=vi.fn();mocks.invoke.mockImplementation(async name=>name==='setup_defaults'?defaults:next);
    render(<SetupPage settings={settings} project={project} updateSettings={update} busy={false} operate={operate}/>);await screen.findByDisplayValue(defaults.automatic_root);
    fireEvent.click(screen.getByRole('button',{name:'Use this toolset in settings'}));await waitFor(()=>expect(update).toHaveBeenCalledWith(next));
    expect(mocks.invoke).toHaveBeenCalledWith('setup_use_toolset',{settings,toolroot:defaults.options.toolroot});expect(mocks.invoke.mock.calls.map(call=>call[0])).not.toContain('save_settings');expect(screen.queryByText('Original hero compatibility passed')).not.toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Save settings');
  });
  it('clears a successful integrity result when its source paths change',async()=>{
    mocks.invoke.mockImplementation(async name=>name==='setup_defaults'?defaults:{steam_build:'123',files_verified:7,bytes_verified:128,manifest_integrity_passed:true,native_compile_passed:false,absolute_minimum_proven:false});
    render(<SetupPage settings={settings} project={project} updateSettings={vi.fn()} busy={false} operate={operate}/>);await screen.findByDisplayValue(defaults.automatic_root);
    fireEvent.click(screen.getByRole('button',{name:'Verify file integrity'}));await screen.findByText(/Integrity passed: 7 files/);
    fireEvent.change(screen.getByLabelText('Existing toolset folder'),{target:{value:'changed'}});expect(screen.queryByText(/Integrity passed: 7 files/)).not.toBeInTheDocument();expect(screen.queryByText('Original hero compatibility passed')).not.toBeInTheDocument();
  });
});
