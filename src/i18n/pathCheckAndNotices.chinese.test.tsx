import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Settings } from '../models';

// The app in Chinese: names the backend's path check reports are drawn as the Settings fields
// name them, the notifications' summary joins its counts with a full-width comma, and a
// notification's dismiss button and the error panel's Close never share a name. In English all
// of it reads as before.
const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),testSetup:vi.fn(),checkSettingFields:vi.fn(),operationDurations:vi.fn(),detectSettings:vi.fn(),setupStatus:vi.fn(),settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),latestTimings:vi.fn(),logs:vi.fn(),vpkSlots:vi.fn(),setTheme:vi.fn(),saveThemePresets:vi.fn(),
  open:vi.fn(),save:vi.fn(),ask:vi.fn(),
}));
vi.mock('../api', async original => ({...await original<typeof import('../api')>(),isDesktop:() => true,api:mocks}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl:vi.fn()}));
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:async () => vi.fn(),onCloseRequested:async () => vi.fn(),destroy:vi.fn()})}));
vi.mock('../preparation/PreparationPage', () => ({PreparationPage:() => <div/>}));
import { App } from '../App';
import { reviewedProject } from '../build/testFixtures';
import { MIRROR_KEY, setLanguage } from './index';
import { OFFER_SEEN, forgetLaunch } from './startup';

const blank: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:''};
const chinese: Settings = {...blank, language:'zh-CN', deadlock:'C:\\Games\\Deadlock'};
const failure = {kind:'tool',message:'Compiler stopped',details:'resourcecompiler: material test.vmat failed',help:{cause:'A material could not be compiled.',steps:['Open Logs.'],action:{label:'Open Logs',page:'logs'}}};
const status = {files_found:{CSDK:true,Deadlock:true,'Original VPK':true,'Resource compiler':false,'Source 2 Viewer CLI':true},model_files_found:true,model_status:'Models are compiled with the Community SDK.',fields:[]};
const sidebar = () => within(screen.getByRole('navigation',{name:/^(Main navigation|主导航)$/}));
const notices = () => within(screen.getByRole('region',{name:/^(Notifications|通知)$/}));
const pathNames = () => [...document.querySelectorAll('.summary-list dt')].map(dt => dt.textContent);

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.newProject.mockResolvedValue({path:null,project:structuredClone(reviewedProject)});
  mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null);
  mocks.saveSettings.mockResolvedValue(undefined); mocks.setTheme.mockResolvedValue(undefined); mocks.saveThemePresets.mockResolvedValue(undefined);
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage('en'));
  for (const key of [MIRROR_KEY, OFFER_SEEN]) localStorage.removeItem(key);
  forgetLaunch();
});
async function startIn(settings: Settings, language: 'zh-CN' | 'en') {
  mocks.settings.mockResolvedValue(settings);
  await act(() => setLanguage(language));
  render(<App/>);
  await screen.findByLabelText(language === 'en' ? 'Project name' : '项目名称');
}

describe('The path check and the notifications in Chinese', () => {
  it('the path check names the backend’s files as the Settings fields do, and keeps the English names in English', async () => {
    await startIn(chinese, 'zh-CN');
    mocks.detectSettings.mockResolvedValue(chinese);
    mocks.setupStatus.mockResolvedValue(status);
    fireEvent.click(sidebar().getByRole('button',{name:'设置'}));
    fireEvent.click(screen.getAllByRole('button',{name:'检测工具'})[0]);
    expect(await screen.findByRole('heading',{name:'路径检查'})).toBeInTheDocument();
    expect(pathNames()).toEqual(expect.arrayContaining(['社区 SDK', 'Deadlock 安装位置', 'Deadlock 游戏资源包', '资源编译器', 'Source 2 Viewer CLI']));
    await act(() => setLanguage('en'));
    expect(pathNames()).toEqual(expect.arrayContaining(['CSDK', 'Deadlock', 'Original VPK', 'Resource compiler', 'Source 2 Viewer CLI']));
  });
  it('the notifications’ summary joins its counts with a full-width comma; Dismiss is 移除 and the error panel’s Close stays 关闭', async () => {
    await startIn(chinese, 'zh-CN');
    mocks.testSetup.mockResolvedValueOnce({passed:true,steps:[{name:'Paths',passed:true,detail:'Every path was found.'}]}).mockRejectedValueOnce(failure);
    fireEvent.click(sidebar().getByRole('button',{name:'设置'}));
    fireEvent.click(screen.getByRole('button',{name:'测试我的设置'}));
    await notices().findByRole('button',{name:/设置测试通过/});
    fireEvent.click(await screen.findByRole('button',{name:'再次测试'}));
    await notices().findByRole('button',{name:/设置测试未通过/});
    const summary = document.querySelector('.notification-summary .sr-only');
    await waitFor(() => expect(summary?.textContent).toBe('1 个错误，1 个成功'));
    // Each notification's × is 移除; the error line's own close button is 关闭, outside the list.
    expect(notices().getAllByRole('button',{name:'移除'})).toHaveLength(2);
    expect(notices().queryByRole('button',{name:'关闭'})).toBeNull();
    const line = screen.getByRole('alert');
    expect(within(line).getByRole('button',{name:'关闭'})).toBeInTheDocument();
    await act(() => setLanguage('en'));
    expect(document.querySelector('.notification-summary .sr-only')?.textContent).toBe('1 error, 1 success');
    expect(notices().getAllByRole('button',{name:'Dismiss'})).toHaveLength(2);
  });
});
