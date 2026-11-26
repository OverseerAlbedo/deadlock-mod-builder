import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Project, Settings } from '../models';

// Build & install driven in Chinese through the app: Analyze and Build end with their results and
// notifications, a failure keeps its English title, its help and the backend's message, and what
// the page writes (choices, the build record) stays English.
const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),restoreAutosave:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),testSetup:vi.fn(),prebuildWarnings:vi.fn(),checkSettingFields:vi.fn(),heroProportions:vi.fn(),operationDurations:vi.fn(),buildProject:vi.fn(),inspectProject:vi.fn(),installProject:vi.fn(),detectSettings:vi.fn(),setupStatus:vi.fn(),settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),saveProject:vi.fn(),cancel:vi.fn(),latestTimings:vi.fn(),logs:vi.fn(),vpkSlots:vi.fn(),
  open:vi.fn(),save:vi.fn(),ask:vi.fn(),
}));
vi.mock('../api', async original => ({...await original<typeof import('../api')>(),isDesktop:() => true,api:mocks}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl:vi.fn()}));
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:async () => vi.fn(),onCloseRequested:async () => vi.fn(),destroy:vi.fn()})}));
vi.mock('../preparation/PreparationPage', () => ({PreparationPage:() => <div/>}));
import { App } from '../App';
import { BuildPage } from './BuildPage';
import { inspected, reviewedProject } from './testFixtures';
import { MIRROR_KEY, setLanguage } from '../i18n';
import { OFFER_SEEN, forgetLaunch } from '../i18n/startup';

const blank: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:''};
const chinese: Settings = {...blank, language:'zh-CN', deadlock:'C:\\Games\\Deadlock'};
const built = {prepared:null,compiled:true,vpk:'C:\\Out\\pak09_dir.vpk',installed:'C:\\Deadlock\\game\\citadel\\addons\\pak09_dir.vpk',manifest:null};
const failure = {kind:'tool',message:'Compiler stopped',details:'resourcecompiler: material body.vmat failed',help:{cause:'A material could not be compiled.',steps:['Open Logs.'],action:{label:'Open Logs',page:'logs'}}};
const sidebar = () => within(screen.getByRole('navigation',{name:'主导航'}));
const notices = () => within(screen.getByRole('region',{name:'通知'}));
const statusText = () => document.querySelector('.statusbar-text')?.textContent;

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.newProject.mockResolvedValue({path:null,project:structuredClone(reviewedProject)});
  mocks.settings.mockResolvedValue(chinese); mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null);
  mocks.saveSettings.mockResolvedValue(undefined); mocks.ask.mockResolvedValue(true);
  mocks.inspectProject.mockResolvedValue({project:structuredClone(reviewedProject),report:inspected});
  mocks.saveProject.mockImplementation(async (path,project) => ({path,project}));
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage('en'));
  for (const key of [MIRROR_KEY, OFFER_SEEN]) localStorage.removeItem(key);
  for (const key of Object.keys(localStorage)) if (key.startsWith('dmb.lastBuild.')) localStorage.removeItem(key);
  forgetLaunch();
});
async function analyzedInChinese() {
  await act(() => setLanguage('zh-CN'));
  render(<App/>);
  await screen.findByLabelText('项目名称');
  fireEvent.click(sidebar().getByRole('button',{name:'构建与安装'}));
  expect(screen.getByRole('heading',{level:1,name:'构建与安装'})).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'分析项目'}));
  await screen.findByText('可以构建',{exact:true});
}
const buildMod = async () => { await waitFor(()=>expect(screen.getByRole('button',{name:'构建模组'})).toBeEnabled()); fireEvent.click(screen.getByRole('button',{name:'构建模组'})); };

describe('Build & install in Chinese', () => {
  it('Analyze and Build end with their results, their notifications and Done', async () => {
    await analyzedInChinese();
    expect(screen.getByRole('heading',{name:'项目检查清单'})).toBeInTheDocument();
    expect(screen.getByText('当前分析')).toBeInTheDocument();
    // A checklist row's action names its row; the click goes where the English one goes.
    expect(screen.getByRole('button',{name:'配置网格映射'})).toBeEnabled();
    mocks.buildProject.mockResolvedValue({project:reviewedProject,path:'C:\\P\\vindicta.dmb.json',result:built});
    await buildMod();
    // The notification is drawn in Chinese.
    await notices().findByRole('button',{name:/构建完成/});
    expect(mocks.buildProject).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('status',{name:'上次完成的构建'})).toHaveTextContent('构建完成pak09_dir.vpk · 已安装');
    expect(screen.getByRole('region',{name:'上次完成的构建'})).toHaveTextContent('角色在英雄选择界面和对局中的表现');
    await waitFor(() => expect(statusText()).toBe('完成'));
    // The build record keeps the project as it is; what changed since is drawn in Chinese and kept in English.
    const key = Object.keys(localStorage).find(name => name.startsWith('dmb.lastBuild.'))!;
    expect(JSON.parse(localStorage.getItem(key)!).project.name).toBe('Vindicta Balanced');
    fireEvent.change(screen.getByLabelText('LOD 策略'),{target:{value:'preserve'}});
    const changes = screen.getByLabelText('自上次构建以来的更改');
    expect(within(changes).getByText('细节层级')).toBeInTheDocument();
  });

  it('a failed build keeps its failure title, its help and the backend’s message', async () => {
    await analyzedInChinese();
    mocks.buildProject.mockRejectedValueOnce(failure);
    await buildMod();
    expect(await screen.findByRole('alert')).toHaveTextContent('Compiler stopped');
    const row = await notices().findByRole('button',{name:/构建失败/});
    expect(row.querySelector('.sr-only')?.textContent).toBe('错误');
    expect(notices().getByRole('button',{name:'Open Logs'})).toBeInTheDocument();
    fireEvent.click(notices().getByRole('button',{name:'显示完整详情'}));
    expect(within(screen.getByRole('alert')).getByText('A material could not be compiled.')).toBeInTheDocument();
    // The page stays usable for the next try.
    await waitFor(() => expect(screen.getByRole('button',{name:'构建模组'})).toBeEnabled());
  });

  it('what the page saves stays English: the LOD strategy and the extras-only switch', async () => {
    const noModel = {...structuredClone(reviewedProject), dmx:'', customization:{enabled:true,icons:[{target:'panorama/images/heroes/test_card.vtex_c',source:'C:\\Art\\card.png'}]}} as unknown as Project;
    const update = vi.fn();
    await act(() => setLanguage('zh-CN'));
    render(<BuildPage project={noModel} update={update} report={null} fresh={false} prepared={null} original={null} busy={false} inspect={vi.fn()} prepare={vi.fn()} validateOriginal={vi.fn()} build={vi.fn()} result={null} install={vi.fn()} installed={null}/>);
    const item = screen.getByText('请添加替换模型。').closest('li')!;
    expect(within(item).getByRole('button',{name:'添加模型'})).toBeInTheDocument();
    fireEvent.click(within(item).getByRole('button',{name:'不使用替换模型，仅构建附加内容'}));
    expect((update.mock.calls[0][0] as Project).customization.model_enabled).toBe(false);
    fireEvent.change(screen.getByLabelText('LOD 策略'),{target:{value:'single'}});
    const changed = update.mock.calls[1][0] as Project;
    expect(changed.lod_strategy).toBe('single');
    expect(changed.name).toBe('Vindicta Balanced');
    expect(within(screen.getByLabelText('LOD 策略')).getByRole('option',{name:'单一层级'})).toHaveValue('single');
  });

  it('switches live, and the checklist keeps the analysis', async () => {
    await analyzedInChinese();
    await act(() => setLanguage('en'));
    expect(screen.getByRole('heading',{name:'Project checklist'})).toBeInTheDocument();
    expect(screen.getByText('Ready to build',{exact:true})).toBeInTheDocument();
    expect(screen.getByRole('button',{name:'Configure model mapping'})).toBeInTheDocument();
    await act(() => setLanguage('zh-CN'));
    expect(screen.getByRole('heading',{name:'项目检查清单'})).toBeInTheDocument();
    expect(screen.getByText('可以构建',{exact:true})).toBeInTheDocument();
    expect(mocks.inspectProject).toHaveBeenCalledTimes(1);
  });
});
