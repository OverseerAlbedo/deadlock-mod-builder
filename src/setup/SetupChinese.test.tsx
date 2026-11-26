import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type {Project,Settings} from '../models';
import type {AcquisitionResult,ManualPreview} from './models';

// The model toolset page, its manual download guide and the Steam QR window in Chinese: the
// operations keep their English names, the commands and what is sent stay as they are.
const mocks=vi.hoisted(()=>({invoke:vi.fn(),open:vi.fn(),listen:vi.fn(),writeText:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke:mocks.invoke}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:mocks.open}));
vi.mock('@tauri-apps/api/event',()=>({listen:mocks.listen}));
import {SetupPage} from './SetupPage';
import {ManualGuide} from './ManualGuide';
import {SteamQrDialog} from './SteamQrDialog';
import {MIRROR_KEY,setLanguage} from '../i18n';

const settings:Settings={steam:'',deadlock:'game',pak:'game/pak01_dir.vpk',csdk:'',resourcecompiler:'material compiler',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'reader',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'work',last_project:'',blender:''};
const project=reference.cases[1].expected as unknown as Project;
const defaults={options:{dd:'C:\\Tools\\dd.exe',toolroot:'C:\\Acquired\\toolset',manifests:'C:\\Acquired\\manifests'},export_root:'C:\\Plans',automatic_root:'C:\\Acquired'};
const plan:AcquisitionResult={plan:{schema:1,kind:'tool-plan',steam_build:'123',branch_snapshot_utc:'date',depots:[{depot:'777',manifest:'888',whole_depot_bytes:5,selected_files:1,selected_bytes:5}],manifest_sha256:{},filelists:{},transfer_uncompressed_bytes:5,installed_bytes:5,files:[{path:'game/bin/compiler.exe',bytes:5,sha1:'a'.repeat(40),directory:false}],overridden_paths:{},native_compile_passed:false,absolute_minimum_proven:false,scope:'Pinned files only',destination:defaults.options.toolroot,estimated_download_bytes:null},plan_digest:'b'.repeat(64),downloaded:false,receipt:null};
const preview:ManualPreview={guide:'1. Open PowerShell.',commands:'',argv:{qr:{metadata:[],downloads:[['dd','-app','730','-depot','2347771','-filelist','a.txt']],metadata_command:'dd -app 730 -qr -manifest-only -dir C:\\Lists',download_commands:['dd -app 730 -qr -depot 2347771 -filelist a.txt']},username:{metadata:[],downloads:[['dd','-app','730','-depot','2347771','-filelist','a.txt']],metadata_command:'dd -app 730 -username YOUR_STEAM_USERNAME -manifest-only',download_commands:['dd -app 730 -username YOUR_STEAM_USERNAME -depot 2347771 -filelist a.txt']}},plan:null,plan_digest:null,expected_files:'',export_dir:'C:\\Plans\\plan',compatible:false,downloaded:false,whole_bytes:20*2**30,selected_bytes:3*2**30};

beforeEach(async()=>{
  for(const mock of Object.values(mocks))mock.mockReset();
  mocks.listen.mockResolvedValue(vi.fn());
  mocks.invoke.mockImplementation(async(name:string,args:{action?:string})=>{if(name==='setup_defaults')return defaults;if(name==='setup_acquire')return {...plan,downloaded:args.action==='download'};throw new Error(`Unexpected command ${name}`);});
  Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:mocks.writeText}});
  mocks.writeText.mockResolvedValue(undefined);
  await act(()=>setLanguage('zh-CN'));
});
afterEach(async()=>{cleanup();await act(()=>setLanguage('en'));localStorage.removeItem(MIRROR_KEY);});

describe('Model tool setup in Chinese',()=>{
  it('fetches the plan with the operation’s English name and shows it in Chinese',async()=>{
    const labels:string[]=[];
    const operate=async(label:string,fn:()=>Promise<void>)=>{labels.push(label);await fn();};
    render(<SetupPage settings={settings} project={project} updateSettings={vi.fn()} busy={false} operate={operate}/>);
    await screen.findByDisplayValue(defaults.automatic_root);
    expect(screen.getByRole('heading',{level:1,name:'模型工具设置'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'获取清单并生成计划'}));
    expect(await screen.findByRole('button',{name:'下载这 1 个文件'})).toBeInTheDocument();
    expect(screen.getByRole('heading',{name:'版本 123'})).toBeInTheDocument();
    expect(labels).toEqual(['Read pinned tool manifests']);
    expect(mocks.invoke).toHaveBeenCalledWith('setup_acquire',{action:'manifests',root:defaults.automatic_root,dd:defaults.options.dd,auth:{mode:'qr'},approvedPlanDigest:null});
    // Text from the backend stays as it comes.
    expect(screen.getByText('Pinned files only')).toBeInTheDocument();
  });
  it('a path field is found by its Chinese name and the file window gets the Chinese title',async()=>{
    const operate=vi.fn(async(_label:string,fn:()=>Promise<void>)=>{await fn();});
    mocks.open.mockResolvedValue('C:\\Elsewhere');
    render(<SetupPage settings={settings} project={project} updateSettings={vi.fn()} busy={false} operate={operate}/>);
    await screen.findByDisplayValue(defaults.automatic_root);
    fireEvent.click(screen.getByRole('button',{name:'浏览：获取文件夹'}));
    await waitFor(()=>expect(mocks.open).toHaveBeenCalledWith(expect.objectContaining({title:'获取文件夹',directory:true})));
    expect(operate.mock.calls[0][0]).toBe('Choose acquisition folder');
    expect(await screen.findByDisplayValue('C:\\Elsewhere')).toBeInTheDocument();
  });
  it('saves the settings the backend chose and confirms in Chinese',async()=>{
    const next={...settings,cswin:defaults.options.toolroot,model_backend:'cswin'};const update=vi.fn();const save=vi.fn(async()=>{});
    mocks.invoke.mockImplementation(async(name:string)=>name==='setup_defaults'?defaults:next);
    render(<SetupPage settings={settings} project={project} updateSettings={update} saveSettings={save} busy={false} operate={async(_label,fn)=>{await fn();}}/>);
    await screen.findByDisplayValue(defaults.automatic_root);
    fireEvent.click(screen.getAllByRole('button',{name:'在设置中使用此工具集'}).at(-1)!);
    await waitFor(()=>expect(save).toHaveBeenCalledWith(next));
    expect(update).toHaveBeenCalledWith(next);
    expect(await screen.findByText('模型编译器已选择并保存。现在构建会使用此工具集。')).toBeInTheDocument();
  });
  it('a failure to load the recipe shows the backend’s message inside the Chinese sentence',async()=>{
    mocks.invoke.mockRejectedValue({kind:'io',message:'Recipe file missing'});
    render(<SetupPage settings={settings} project={project} updateSettings={vi.fn()} busy={false} operate={async(_label,fn)=>{await fn();}}/>);
    expect(await screen.findByRole('alert')).toHaveTextContent('无法加载设置的默认值：Recipe file missing');
  });
  it('switches live with the language',async()=>{
    render(<SetupPage settings={settings} project={project} updateSettings={vi.fn()} busy={false} operate={async(_label,fn)=>{await fn();}}/>);
    await screen.findByDisplayValue(defaults.automatic_root);
    await act(()=>setLanguage('en'));
    expect(screen.getByRole('heading',{level:1,name:'Model tool setup'})).toBeInTheDocument();
    expect(screen.getByRole('button',{name:'Fetch manifests and plan'})).toBeInTheDocument();
  });
});
describe('The manual download guide in Chinese',()=>{
  it('keeps the commands exactly and fills the typed account name into them',async()=>{
    render(<ManualGuide preview={preview}/>);
    expect(screen.getByRole('heading',{name:'打开 PowerShell'})).toBeInTheDocument();
    expect(screen.getByText(/^运行这条命令并等待它完成（共 3.0 GB）。如果某条命令出错停止/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('radio',{name:'Steam 用户名'}));
    fireEvent.change(screen.getByLabelText('Steam 账户名'),{target:{value:'sample_account'}});
    expect(screen.getByLabelText('获取文件列表的命令')).toHaveTextContent('dd -app 730 -username sample_account -manifest-only');
    fireEvent.click(screen.getAllByRole('button',{name:'复制'})[0]);
    await waitFor(()=>expect(mocks.writeText).toHaveBeenCalledWith('dd -app 730 -username sample_account -manifest-only'));
    expect(await screen.findByRole('button',{name:'已复制'})).toBeInTheDocument();
    // The plain-text guide comes from the backend as it is.
    expect(screen.getByText('1. Open PowerShell.')).toBeInTheDocument();
    await act(()=>setLanguage('en'));
    expect(screen.getByRole('heading',{name:'Open PowerShell'})).toBeInTheDocument();
  });
});
describe('The Steam QR window in Chinese',()=>{
  it('shows the code with Chinese instructions and hides on request',async()=>{
    let send:((event:{payload:{rows:string[]}|null})=>void)|undefined;
    mocks.listen.mockImplementation(async(_name:string,handler:typeof send)=>{send=handler;return vi.fn();});
    render(<SteamQrDialog/>);
    await waitFor(()=>expect(send).toBeDefined());
    act(()=>send!({payload:{rows:['101','010','101']}}));
    expect(screen.getByRole('dialog',{name:'扫码登录 Steam'})).toBeInTheDocument();
    expect(screen.getByRole('img',{name:'Steam 登录二维码'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'隐藏，直到下次登录'}));
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
