import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Settings, StorageReport, TimingReport } from './models';

// The app's own texts in Chinese: toasts, native questions, the error line, Notifications, the
// Character & model page, Logs, Step timings and the Settings parts (storage, tool diagnostics).
// What the app saves, sends or copies stays English or as typed.
const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),testSetup:vi.fn(),checkSettingFields:vi.fn(),operationDurations:vi.fn(),detectSettings:vi.fn(),setupStatus:vi.fn(),settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),saveProject:vi.fn(),openProject:vi.fn(),latestTimings:vi.fn(),logs:vi.fn(),vpkSlots:vi.fn(),setTheme:vi.fn(),saveThemePresets:vi.fn(),
  heroes:vi.fn(),heroProportions:vi.fn(),selectHero:vi.fn(),copyProjectForHero:vi.fn(),chooseFreeSlot:vi.fn(),storageReport:vi.fn(),storageClear:vi.fn(),timings:vi.fn(),capabilities:vi.fn(),
  open:vi.fn(),save:vi.fn(),ask:vi.fn(),invoke:vi.fn(),writeText:vi.fn(),
}));
vi.mock('./api', async original => ({...await original<typeof import('./api')>(),isDesktop:() => true,api:mocks}));
vi.mock('@tauri-apps/api/core', () => ({invoke:mocks.invoke,isTauri:() => true}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl:vi.fn()}));
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:async () => vi.fn(),onCloseRequested:async () => vi.fn(),destroy:vi.fn()})}));
vi.mock('./preparation/PreparationPage', () => ({PreparationPage:() => <div/>}));
import { App } from './App';
import { reviewedProject } from './build/testFixtures';
import { MIRROR_KEY, setLanguage } from './i18n';
import { OFFER_SEEN, forgetLaunch } from './i18n/startup';

const blank: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'C:\\Work',last_project:'',blender:''};
const chinese: Settings = {...blank, language:'zh-CN', deadlock:'C:\\Games\\Deadlock'};
const sidebar = () => within(screen.getByRole('navigation',{name:/^(Main navigation|主导航)$/}));
const notices = () => within(screen.getByRole('region',{name:/^(Notifications|通知)$/}));
const statusText = () => document.querySelector('.statusbar-text')?.textContent;
const nav = (name: string) => fireEvent.click(sidebar().getByRole('button',{name}));
const storage: StorageReport = {
  workspace:'C:\\Work', bytes:2 * 1024 ** 3, reclaimable:1024 ** 3, other_bytes:0, shared:[],
  projects:[{id:'a'.repeat(32), name:'Sample', hero:'Haze', project_file:'C:\\Projects\\sample.dmb.json', open:true, locked:false, workspace:'C:\\Work\\aaa', modified:null, bytes:2 * 1024 ** 3, reclaimable:1024 ** 3,
    usage:[{kind:'build_data', bytes:1024 ** 3, reclaimable:1024 ** 3}]}],
};

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.newProject.mockResolvedValue({path:null,project:structuredClone(reviewedProject)});
  mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null);
  mocks.saveSettings.mockResolvedValue(undefined); mocks.heroProportions.mockResolvedValue(null);
  mocks.saveProject.mockImplementation(async (path, project) => ({path, project}));
  Object.defineProperty(navigator, 'clipboard', {configurable:true, value:{writeText:mocks.writeText}});
  mocks.writeText.mockResolvedValue(undefined); mocks.invoke.mockResolvedValue(null);
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage('en'));
  for (const key of [MIRROR_KEY, OFFER_SEEN, 'dmb.uiMode']) localStorage.removeItem(key);
  forgetLaunch();
});
async function startInChinese(settings: Settings = chinese) {
  mocks.settings.mockResolvedValue(settings);
  await act(() => setLanguage('zh-CN'));
  render(<App/>);
  // The project's own page (not the loading page, which has the same heading).
  await screen.findByRole('region',{name:'项目概要'});
}

describe('The app in Chinese', () => {
  it('Save asks for the file with a Chinese title, saves the English default name and ends with its toast', async () => {
    await startInChinese();
    mocks.save.mockResolvedValue('C:\\Projects\\saved.dmb.json');
    fireEvent.click(screen.getByRole('button',{name:'保存'}));
    await waitFor(() => expect(mocks.saveProject).toHaveBeenCalled());
    expect(mocks.save).toHaveBeenCalledWith({title:'保存项目', defaultPath:`${reviewedProject.name}.dmb.json`, filters:[{name:'Deadlock Mod Builder 项目', extensions:['json']}]});
    expect(mocks.saveProject.mock.calls[0][1].name).toBe(reviewedProject.name);
    expect(await screen.findByText('项目已保存')).toHaveClass('toast');
    await waitFor(() => expect(statusText()).toBe('完成'));
  });
  it('a failure without a backend message shows the app’s words in Chinese and copies them in English', async () => {
    await startInChinese();
    mocks.open.mockResolvedValue('C:\\Projects\\broken.dmb.json');
    mocks.openProject.mockRejectedValue('The file is not a project.');
    fireEvent.click(screen.getByRole('button',{name:'打开…'}));
    const line = await screen.findByRole('alert');
    expect(line).toHaveTextContent('无法打开项目');
    expect(line).toHaveTextContent('详情见“通知”。');
    expect(mocks.open).toHaveBeenCalledWith(expect.objectContaining({title:'打开项目'}));
    expect(await notices().findByRole('button',{name:/无法打开项目/})).toBeInTheDocument();
    fireEvent.click(within(line).getByRole('button',{name:'显示详情'}));
    const full = screen.getByRole('alert');
    expect(within(full).getByText('无法打开项目')).toBeInTheDocument();
    fireEvent.click(within(full).getByRole('button',{name:'复制支持信息'}));
    await waitFor(() => expect(mocks.writeText).toHaveBeenCalled());
    expect(mocks.writeText.mock.calls[0][0]).toMatch(/^Could not open the project/);
    expect(await within(full).findByRole('button',{name:'已复制'})).toBeInTheDocument();
    await waitFor(() => expect(statusText()).toBe('因错误而停止'));
  });
  it('New with unsaved changes asks in Chinese, and keeping the project keeps the name as typed', async () => {
    await startInChinese();
    mocks.ask.mockResolvedValue(false);
    fireEvent.change(screen.getByLabelText('项目名称'), {target:{value:'我的 Mod'}});
    fireEvent.click(screen.getByRole('button',{name:'新建'}));
    await waitFor(() => expect(mocks.ask).toHaveBeenCalledWith('此项目有未保存的更改。要放弃这些更改并继续吗？', {title:'未保存的项目', kind:'warning', okLabel:'放弃更改', cancelLabel:'继续编辑'}));
    expect(mocks.newProject).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText('项目名称')).toHaveValue('我的 Mod');
  });
  it('Undo names what it undid in Chinese and puts the typed name back', async () => {
    await startInChinese();
    fireEvent.change(screen.getByLabelText('项目名称'), {target:{value:'Renamed'}});
    fireEvent.click(screen.getByRole('button',{name:'撤销'}));
    expect(await screen.findByText('已撤销：项目名称')).toHaveClass('toast');
    expect(screen.getByLabelText('项目名称')).toHaveValue(reviewedProject.name);
    fireEvent.click(screen.getByRole('button',{name:'重做'}));
    expect(await screen.findByText('已重做：项目名称')).toHaveClass('toast');
    expect(screen.getByLabelText('项目名称')).toHaveValue('Renamed');
  });
  it('Character & model: the hero picker lists tags in Chinese, sends the hero’s path, and the page switches live', async () => {
    await startInChinese();
    expect(screen.getByRole('heading',{level:1,name:'角色与模型'})).toBeInTheDocument();
    const hero = {name:'abrams', path:'models/heroes_wip/abrams/abrams.vmdl_c', likely_hero:true};
    mocks.heroes.mockResolvedValue([hero]);
    mocks.selectHero.mockImplementation(async project => ({...project, original_model:hero.path, hero_name:'Abrams'}));
    fireEvent.click(screen.getAllByRole('button',{name:'选择英雄…'})[0]);
    const list = await screen.findByRole('list',{name:'可用的英雄'});
    expect(within(list).getByText('游戏中')).toHaveClass('hero-tag');
    fireEvent.click(within(list).getByRole('button'));
    await waitFor(() => expect(mocks.selectHero).toHaveBeenCalledWith(expect.anything(), hero.path));
    expect(await screen.findByText('已选择英雄')).toHaveClass('toast');
    await act(() => setLanguage('en'));
    expect(screen.getByRole('heading',{level:1,name:'Character & model'})).toBeInTheDocument();
    expect(screen.getByText('Hero to replace')).toBeInTheDocument();
  });
  it('the hero picker’s failure keeps the backend’s message and is named in Chinese', async () => {
    await startInChinese();
    mocks.heroes.mockRejectedValue({kind:'io', message:'The game archive could not be read', details:'pak01_dir.vpk: locked', help:null});
    fireEvent.click(screen.getAllByRole('button',{name:'选择英雄…'})[0]);
    expect(await screen.findByRole('alert')).toHaveTextContent('The game archive could not be read');
    expect(await notices().findByRole('button',{name:/无法查找英雄/})).toBeInTheDocument();
  });
  it('a copy for another hero is told in Chinese in its notice and notification, which switch to English live', async () => {
    await startInChinese();
    const hero = {name:'haze', path:'models/heroes/haze/haze.vmdl_c', likely_hero:true};
    mocks.heroes.mockResolvedValue([hero]);
    mocks.copyProjectForHero.mockImplementation(async project => ({project:{...structuredClone(project), id:'c'.repeat(32), hero_name:'Haze', original_model:hero.path}, report:{from_hero:'Vindicta', carried:['Body roles'], not_carried:['12 sounds'], remade:['Joint matches']}}));
    mocks.chooseFreeSlot.mockResolvedValue({slot:23, filename:'pak23_dir.vpk', occupied_slots:[], previous_outputs:[], scanned_folders:[]});
    fireEvent.click(screen.getByRole('button',{name:'为另一位英雄制作副本…'}));
    fireEvent.click(within(await screen.findByRole('list',{name:'副本可用的英雄'})).getByRole('button'));
    await waitFor(() => expect(mocks.copyProjectForHero).toHaveBeenCalledWith(expect.anything(), hero.path));
    expect(await screen.findByRole('heading',{name:'Haze 的副本'})).toBeInTheDocument();
    expect(screen.getByText('已为 Haze 制作副本。请将它另存为新项目。')).toHaveClass('toast');
    const row = notices().getByRole('button',{name:/已为 Haze 制作副本/});
    fireEvent.click(row);
    expect(notices().getByText('Haze 的副本是一个尚未保存的新项目。Vindicta 的项目没有改动。')).toBeInTheDocument();
    expect(notices().getByText('未带过去（它们属于 Vindicta）：12 sounds。')).toBeInTheDocument();
    expect(notices().getByRole('button',{name:'打开“准备模型”'})).toBeInTheDocument();
    await act(() => setLanguage('en'));
    expect(notices().getByRole('button',{name:/Copy made for Haze/})).toBeInTheDocument();
    expect(notices().getByText('Not carried, because they belong to Vindicta: 12 sounds.')).toBeInTheDocument();
    expect(notices().getByRole('button',{name:'Open Prepare model'})).toBeInTheDocument();
  });
  it('Logs: the level filter works by its Chinese names and log lines stay as they come', async () => {
    mocks.logs.mockResolvedValue([
      {sequence:1,timestamp_utc:'2026-11-25T10:00:00Z',level:'info',message:'Reading the archive'},
      {sequence:2,timestamp_utc:'2026-11-25T10:00:01Z',level:'error',message:'resourcecompiler failed'},
    ]);
    await startInChinese();
    nav('日志');
    expect(screen.getByRole('heading',{level:1,name:'日志'})).toBeInTheDocument();
    const level = screen.getByRole('combobox',{name:'日志级别'});
    expect(within(level).getByRole('option',{name:'错误'})).toHaveValue('error');
    fireEvent.change(level, {target:{value:'error'}});
    expect(screen.getByText('筛选后的输出')).toBeInTheDocument();
    expect(screen.getByText('resourcecompiler failed')).toBeInTheDocument();
    expect(screen.queryByText('Reading the archive')).toBeNull();
    expect(screen.getByText('错误', {selector:'.log-severity'})).toBeInTheDocument();
    await act(() => setLanguage('en'));
    expect(screen.getByRole('heading',{level:1,name:'Logs'})).toBeInTheDocument();
    expect(screen.getByText('ERROR', {selector:'.log-severity'})).toBeInTheDocument();
  });
  it('Step timings: opening a report asks with a Chinese title and shows step names as they come', async () => {
    localStorage.setItem('dmb.uiMode', 'expert');
    const report: TimingReport = {schema_version:1, operation:'build', run_id:'r1', status:'succeeded', started_utc:'2026-11-25T10:00:00Z', finished_utc:'2026-11-25T10:00:05Z', total_ns:5e9,
      steps:[{id:'s1', parent_id:null, name:'Compile model', status:'succeeded', inclusive_ns:4e9, exclusive_ns:4e9}]} as TimingReport;
    mocks.open.mockResolvedValue('C:\\Reports\\build.json'); mocks.timings.mockResolvedValue(report);
    await startInChinese();
    nav('步骤耗时');
    fireEvent.click(screen.getByRole('button',{name:'打开耗时报告…'}));
    expect(await screen.findByText('Compile model')).toBeInTheDocument();
    expect(mocks.open).toHaveBeenCalledWith(expect.objectContaining({title:'打开步骤耗时报告', filters:[{name:'耗时报告', extensions:['json']}]}));
    expect(mocks.timings).toHaveBeenCalledWith('C:\\Reports\\build.json');
    expect(screen.getByRole('columnheader',{name:'自身耗时'})).toBeInTheDocument();
    expect(screen.getAllByText('完成').length).toBeGreaterThan(0);
    expect(screen.getByTestId('total-duration')).toHaveTextContent('5.000 s');
    expect(await screen.findByText('耗时报告已打开')).toHaveClass('toast');
    await act(() => setLanguage('en'));
    expect(screen.getByRole('heading',{level:1,name:'Step timings'})).toBeInTheDocument();
    expect(screen.getByRole('columnheader',{name:'Own work'})).toBeInTheDocument();
  });
  it('Step timings: a report that cannot be opened is named in Chinese with the backend’s message', async () => {
    localStorage.setItem('dmb.uiMode', 'expert');
    mocks.open.mockResolvedValue('C:\Reports\broken.json'); mocks.timings.mockRejectedValue({kind:'input', message:'Not a timing report', details:'missing schema_version', help:null});
    await startInChinese();
    nav('步骤耗时');
    fireEvent.click(screen.getByRole('button',{name:'打开耗时报告…'}));
    expect(await screen.findByRole('alert')).toHaveTextContent('Not a timing report');
    expect(await notices().findByRole('button',{name:/无法打开耗时报告/})).toBeInTheDocument();
  });
  it('Storage: measuring and clearing ask and answer in Chinese and send the English kinds', async () => {
    await startInChinese();
    mocks.storageReport.mockResolvedValue(storage);
    mocks.storageClear.mockResolvedValue({result:{removed_bytes:1024 ** 3, removed:['x'], skipped:[]}, report:storage});
    mocks.ask.mockResolvedValue(true);
    nav('设置');
    fireEvent.click(screen.getByRole('button',{name:'统计存储空间'}));
    expect(await screen.findByText('当前已打开')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'详情'}));
    expect(screen.getByText('构建缓存和暂存文件')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'清除 1.00 GB'}));
    await waitFor(() => expect(mocks.storageClear).toHaveBeenCalledWith({project:'a'.repeat(32), kinds:['build_data']}, expect.anything()));
    expect(mocks.ask).toHaveBeenCalledWith('要从“Sample · Haze”中移除 1.00 GB 的构建缓存和暂存文件吗？', {title:'清除存储的数据', kind:'warning', okLabel:'移除', cancelLabel:'保留'});
    expect(await screen.findByText('已移除 1.00 GB。')).toBeInTheDocument();
    await act(() => setLanguage('en'));
    expect(screen.getByText('Removed 1.00 GB.')).toBeInTheDocument();
    expect(screen.getByRole('heading',{name:'Storage'})).toBeInTheDocument();
  });
  it('Storage: a failure keeps the backend’s message and help and names the failure in Chinese', async () => {
    await startInChinese();
    mocks.storageReport.mockRejectedValue({kind:'io', message:'The workspace could not be read', details:'access denied: C:\\Work', help:{cause:'Another program holds the folder.', steps:['Close Explorer windows on the folder.'], action:null}});
    nav('设置');
    fireEvent.click(screen.getByRole('button',{name:'统计存储空间'}));
    expect(await screen.findByRole('alert')).toHaveTextContent('The workspace could not be read');
    const row = await notices().findByRole('button',{name:/无法统计存储的数据/});
    expect(row.querySelector('.sr-only')?.textContent).toBe('错误');
    fireEvent.click(notices().getByRole('button',{name:'显示完整详情'}));
    expect(within(screen.getByRole('alert')).getByText('Another program holds the folder.')).toBeInTheDocument();
    expect(within(screen.getByRole('alert')).getByText('解决方法')).toBeInTheDocument();
  });
  it('Tool diagnostics: Validate Blender reports in Chinese and keeps the path it found', async () => {
    localStorage.setItem('dmb.uiMode', 'expert');
    mocks.invoke.mockImplementation(async (name: string) => name === 'validate_blender' ? {path:'C:\\Blender\\blender.exe', version:'5.2.0', launch_verified:true, conversion_certified:false} : null);
    await startInChinese();
    nav('设置');
    fireEvent.click(screen.getByRole('button',{name:'验证 Blender'}));
    expect(await screen.findByText('Blender 5.2.0 启动成功，其路径已保存在这台电脑上。模型转换另行验证。')).toBeInTheDocument();
    expect(mocks.invoke).toHaveBeenCalledWith('validate_blender', {executable:null});
    await act(() => setLanguage('en'));
    expect(screen.getByText('Blender 5.2.0 launched successfully and its path is saved on this PC. Conversion remains separately validated.')).toBeInTheDocument();
    expect(screen.getByRole('button',{name:'Validate Blender'})).toBeInTheDocument();
  });
  it('the Reduced CSDK 12 notification guides in Chinese and fills the found SDK into the settings', async () => {
    const older = [{field:'csdk', state:'warning', message:'Reduced CSDK 12 is in use.', layout:'older'}];
    const current = [{field:'csdk', state:'ok', message:'Found.', layout:'current'}];
    mocks.checkSettingFields.mockImplementation(async (value: Settings) => value.csdk === 'C:\\CSDK13' ? current : older);
    mocks.detectSettings.mockResolvedValue({...chinese, csdk:'C:\\CSDK13', resourcecompiler:'C:\\CSDK13\\game\\bin\\win64\\resourcecompiler.exe'});
    await startInChinese({...chinese, csdk:'C:\\CSDK12'});
    const row = await notices().findByRole('button',{name:/正在使用 Reduced CSDK 12/});
    fireEvent.click(row);
    expect(notices().getByText('你正在使用 Reduced CSDK 12。它仍然可用，但 Reduced CSDK 13 取代了它，并且还能编译英雄模型。迁移方法：')).toBeInTheDocument();
    fireEvent.click(notices().getByRole('button',{name:'查找 Reduced CSDK 13'}));
    expect(await screen.findByText('已找到 Reduced CSDK 13：C:\\CSDK13。保存设置后即可使用。')).toHaveClass('toast');
    expect(mocks.detectSettings).toHaveBeenCalledWith(expect.objectContaining({csdk:'', resourcecompiler:''}));
    expect(screen.getByRole('heading',{level:1,name:'设置'})).toBeInTheDocument();
  });
});
