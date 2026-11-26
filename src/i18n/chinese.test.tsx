import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Settings } from '../models';

// The app driven in Chinese: flows that depend on labels keep working, and what the app
// saves or names stays English (labels that are identifiers, saved names, failure titles).
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
import { MIRROR_KEY, getLanguage, setLanguage } from './index';
import { OFFER_SEEN, forgetLaunch } from './startup';

const blank: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:''};
const chinese: Settings = {...blank, language:'zh-CN', deadlock:'C:\\Games\\Deadlock'};
const failure = {kind:'tool',message:'Compiler stopped',details:'resourcecompiler: material test.vmat failed',help:{cause:'A material could not be compiled.',steps:['Open Logs.'],action:{label:'Open Logs',page:'logs'}}};
const sidebar = () => within(screen.getByRole('navigation',{name:/^(Main navigation|主导航)$/}));
const notices = () => within(screen.getByRole('region',{name:/^(Notifications|通知)$/}));
const statusText = () => document.querySelector('.statusbar-text')?.textContent;
/** Windows as the page sees it. */
function windowsIn(locale: string) { Object.defineProperty(navigator, 'language', {configurable:true, get:() => locale}); }

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
  Reflect.deleteProperty(navigator, 'language');
  forgetLaunch();
});
/** The app as a user who chose Chinese starts it: the mirrored language is loaded before the first render. */
async function startInChinese(settings: Settings = chinese) {
  mocks.settings.mockResolvedValue(settings);
  await act(() => setLanguage('zh-CN'));
  render(<App/>);
  await screen.findByLabelText('项目名称');
  expect(sidebar().getByRole('button',{name:'设置'})).toBeInTheDocument();
}
const openSettings = () => fireEvent.click(sidebar().getByRole('button',{name:'设置'}));

describe('In Chinese', () => {
  it('Test my setup started from Settings ends with its result, its notification and Done', async () => {
    await startInChinese();
    mocks.testSetup.mockResolvedValue({passed:true,steps:[{name:'Paths',passed:true,detail:'Every path was found.'}]});
    openSettings();
    fireEvent.click(screen.getByRole('button',{name:'测试我的设置'}));
    expect(await screen.findByText('你的设置可以正常工作。请构建一次英雄，确认模型能够编译。')).toBeInTheDocument();
    expect(mocks.testSetup).toHaveBeenCalledWith(chinese);
    // The notification's title and its badge are read out in Chinese.
    const row = await notices().findByRole('button',{name:/设置测试通过/});
    expect(row.querySelector('.sr-only')?.textContent).toBe('成功');
    await waitFor(() => expect(statusText()).toBe('完成'));
  });
  it('Detect tools started from Settings fills the paths, shows the path check and ends with Done', async () => {
    await startInChinese();
    const detected = {...chinese, pak:'C:\\Games\\Deadlock\\game\\citadel\\pak01_dir.vpk'};
    mocks.detectSettings.mockResolvedValue(detected);
    mocks.setupStatus.mockResolvedValue({files_found:{Deadlock:true,'Original VPK':true},model_files_found:false,model_status:'No model compiler is set.',fields:[]});
    openSettings();
    fireEvent.click(screen.getAllByRole('button',{name:'检测工具'})[0]);
    expect(await screen.findByRole('heading',{name:'路径检查'})).toBeInTheDocument();
    expect(mocks.detectSettings).toHaveBeenCalledWith(chinese, []);
    expect(screen.getByText('未保存的更改')).toBeInTheDocument();
    expect(screen.getAllByText('已找到')).toHaveLength(2);
    await waitFor(() => expect(statusText()).toBe('完成'));
  });
  it('a failure still gets its failure title, its help and its details', async () => {
    await startInChinese();
    mocks.testSetup.mockRejectedValue(failure);
    openSettings();
    fireEvent.click(screen.getByRole('button',{name:'测试我的设置'}));
    expect(await screen.findByRole('alert')).toHaveTextContent('Compiler stopped');
    const row = await notices().findByRole('button',{name:/设置测试未通过/});
    expect(row.querySelector('.sr-only')?.textContent).toBe('错误');
    expect(notices().getByRole('button',{name:'Open Logs'})).toBeInTheDocument();
    fireEvent.click(notices().getByRole('button',{name:'显示完整详情'}));
    expect(within(screen.getByRole('alert')).getByText('A material could not be compiled.')).toBeInTheDocument();
    await waitFor(() => expect(statusText()).toBe('因错误而停止'));
  });
  it('the theme editor saves a new theme under its English suggested name or the name typed', async () => {
    await startInChinese();
    openSettings();
    expect(screen.getByRole('button',{name:'原版主题'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'+ 自定义…'}));
    expect(screen.getByText('基于原版的新主题')).toBeInTheDocument();
    expect(screen.getByLabelText('主题名称')).toHaveValue('My Original');
    fireEvent.click(screen.getByRole('button',{name:'另存为预设'}));
    await waitFor(() => expect(mocks.saveThemePresets).toHaveBeenCalledTimes(1));
    expect(mocks.saveThemePresets.mock.calls[0][0].map((preset: {name: string}) => preset.name)).toEqual(['My Original']);
    // A typed name is kept as typed and never translated.
    fireEvent.click(screen.getByRole('button',{name:'+ 自定义…'}));
    fireEvent.change(screen.getByLabelText('主题名称'), {target:{value:'夜间 Night'}});
    fireEvent.click(screen.getByRole('button',{name:'另存为预设'}));
    await waitFor(() => expect(mocks.saveThemePresets).toHaveBeenCalledTimes(2));
    expect(mocks.saveThemePresets.mock.calls[1][0].map((preset: {name: string}) => preset.name).sort()).toEqual(['My Original', '夜间 Night']);
    expect(screen.getByRole('button',{name:'夜间 Night 主题'})).toBeInTheDocument();
  });
  it('the picker switches the whole app at once and back, keeping the unsaved project', async () => {
    mocks.settings.mockResolvedValue({...blank, deadlock:'C:\\Games\\Deadlock'});
    render(<App/>);
    fireEvent.change(await screen.findByLabelText('Project name'), {target:{value:'Renamed here'}});
    expect(screen.getByLabelText('Unsaved project changes')).toBeInTheDocument();
    fireEvent.click(sidebar().getByRole('button',{name:'Settings'}));
    fireEvent.change(screen.getByRole('combobox',{name:/^Language/}), {target:{value:'zh-CN'}});
    expect(await sidebar().findByRole('button',{name:'设置'})).toBeInTheDocument();
    expect(screen.getByRole('heading',{level:1,name:'设置'})).toBeInTheDocument();
    expect(document.documentElement.lang).toBe('zh-Hans');
    await waitFor(() => expect(mocks.saveSettings).toHaveBeenLastCalledWith(expect.objectContaining({language:'zh-CN'})));
    expect(screen.getByLabelText('项目有未保存的更改')).toBeInTheDocument();
    expect(screen.getByRole('combobox',{name:/^语言/})).toHaveValue('zh-CN');
    fireEvent.change(screen.getByRole('combobox',{name:/^语言/}), {target:{value:'en'}});
    expect(await sidebar().findByRole('button',{name:'Settings'})).toBeInTheDocument();
    await waitFor(() => expect(mocks.saveSettings).toHaveBeenLastCalledWith(expect.objectContaining({language:'en'})));
    expect(screen.getByLabelText('Unsaved project changes')).toBeInTheDocument();
    fireEvent.click(sidebar().getByRole('button',{name:'Character & model'}));
    expect(await screen.findByLabelText('Project name')).toHaveValue('Renamed here');
  });
  it('a first start on Chinese Windows opens the guide in Chinese with 简体中文 chosen, and saves it', async () => {
    localStorage.removeItem('dmb.setupGuideSeen'); forgetLaunch(); windowsIn('zh-CN');
    mocks.settings.mockResolvedValue(blank);
    render(<App/>);
    expect(await screen.findByRole('heading',{level:1,name:'新手引导'})).toBeInTheDocument();
    const choices = within(screen.getByRole('radiogroup',{name:/选择语言/}));
    expect(choices.getByRole('radio',{name:/简体中文/})).toHaveAttribute('aria-checked','true');
    expect(choices.getByRole('radio',{name:/简体中文/})).toHaveTextContent('Windows 显示语言');
    await waitFor(() => expect(mocks.saveSettings).toHaveBeenCalledWith(expect.objectContaining({language:'zh-CN'})));
    // The card switches live too.
    fireEvent.click(choices.getByRole('radio',{name:/English/}));
    expect(await screen.findByRole('heading',{level:1,name:'New-user setup'})).toBeInTheDocument();
    await waitFor(() => expect(mocks.saveSettings).toHaveBeenLastCalledWith(expect.objectContaining({language:'en'})));
  });
  it('a first start on English Windows stays English and saves nothing', async () => {
    localStorage.removeItem('dmb.setupGuideSeen'); forgetLaunch(); windowsIn('en-US');
    mocks.settings.mockResolvedValue(blank);
    render(<App/>);
    expect(await screen.findByRole('heading',{level:1,name:'New-user setup'})).toBeInTheDocument();
    expect(screen.getByRole('radio',{name:/English/})).toHaveAttribute('aria-checked','true');
    expect(getLanguage()).toBe('en');
    expect(mocks.saveSettings).not.toHaveBeenCalled();
  });
  it('an existing user on Chinese Windows stays English and is offered Chinese once', async () => {
    windowsIn('zh-CN');
    mocks.settings.mockResolvedValue({...blank, deadlock:'C:\\Games\\Deadlock'});
    const first = render(<App/>);
    const offer = await screen.findByRole('region',{name:'Language · 语言'});
    expect(sidebar().getByRole('button',{name:'Settings'})).toBeInTheDocument();
    expect(localStorage.getItem(OFFER_SEEN)).toBe('1');
    fireEvent.click(within(offer).getByRole('button',{name:'切换为简体中文'}));
    expect(await sidebar().findByRole('button',{name:'设置'})).toBeInTheDocument();
    await waitFor(() => expect(mocks.saveSettings).toHaveBeenCalledWith(expect.objectContaining({language:'zh-CN'})));
    expect(screen.queryByRole('region',{name:'Language · 语言'})).toBeNull();
    first.unmount();
    // Started again without a saved language (as if the save had not reached the file): no second offer.
    await act(() => setLanguage('en')); forgetLaunch();
    render(<App/>);
    await screen.findByLabelText('Project name');
    expect(screen.queryByRole('region',{name:'Language · 语言'})).toBeNull();
  });
  it('Keep English in the offer keeps English and saves that choice', async () => {
    windowsIn('zh-TW');
    mocks.settings.mockResolvedValue({...blank, blender:'C:\\Blender\\blender.exe'});
    render(<App/>);
    const offer = await screen.findByRole('region',{name:'Language · 语言'});
    fireEvent.click(within(offer).getByRole('button',{name:/Keep English/}));
    await waitFor(() => expect(mocks.saveSettings).toHaveBeenCalledWith(expect.objectContaining({language:'en'})));
    expect(getLanguage()).toBe('en');
    expect(screen.queryByRole('region',{name:'Language · 语言'})).toBeNull();
  });
});
