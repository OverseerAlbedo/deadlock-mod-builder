import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Project, Settings } from './models';

// The game's own hero names in the interface language, through the app: shown in the hero
// picker (with the English beside them), on Character & model, Build & install and in the copy
// notice, while the project, the operations and the notifications keep the English names and paths.
const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),testSetup:vi.fn(),checkSettingFields:vi.fn(),operationDurations:vi.fn(),detectSettings:vi.fn(),setupStatus:vi.fn(),settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),saveProject:vi.fn(),openProject:vi.fn(),latestTimings:vi.fn(),logs:vi.fn(),vpkSlots:vi.fn(),setTheme:vi.fn(),saveThemePresets:vi.fn(),
  heroes:vi.fn(),heroProportions:vi.fn(),selectHero:vi.fn(),copyProjectForHero:vi.fn(),chooseFreeSlot:vi.fn(),capabilities:vi.fn(),prebuildWarnings:vi.fn(),inspectProject:vi.fn(),
  gameNames:vi.fn(),
  open:vi.fn(),save:vi.fn(),ask:vi.fn(),
}));
vi.mock('./api', async original => ({...await original<typeof import('./api')>(),isDesktop:() => true,api:mocks}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl:vi.fn()}));
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:async () => vi.fn(),onCloseRequested:async () => vi.fn(),destroy:vi.fn()})}));
vi.mock('./preparation/PreparationPage', () => ({PreparationPage:() => <div/>}));
import { App } from './App';
import { reviewedProject } from './build/testFixtures';
import { MIRROR_KEY, setLanguage } from './i18n';
import { OFFER_SEEN, forgetLaunch } from './i18n/startup';
import { forgetGameNames, type GameNames } from './i18n/gameNames';

const blank: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'C:\\Work',last_project:'',blender:''};
const pak = 'C:\\Games\\Deadlock\\game\\citadel\\pak01_dir.vpk';
const chinese: Settings = {...blank, language:'zh-CN', deadlock:'C:\\Games\\Deadlock', pak};
const english: Settings = {...chinese, language:'en'};
// Made-up names in the shape the command answers with.
const names: GameNames = {language:'zh-CN', heroes:{Vindicta:'维迪塔', Haze:'海兹', Shiv:'希瓦'}, items:{}, missing:[]};
const empty: GameNames = {language:'zh-CN', heroes:{}, items:{}, missing:[]};
const haze = {name:'haze', path:'models/heroes_staging/haze/haze.vmdl_c', likely_hero:true};
const shivUlt = {name:'shiv_ult', path:'models/heroes_staging/shiv_ult/shiv_ult.vmdl_c', likely_hero:true};
const sidebar = () => within(screen.getByRole('navigation',{name:/^(Main navigation|主导航)$/}));
const heroCard = () => document.querySelector('#character-selection .hero-name') as HTMLElement;
const search = () => document.querySelector('.hero-selector .toolbar input:not([type])') as HTMLInputElement;
const results = () => [...document.querySelectorAll('.hero-results li strong')].map(node => node.textContent);

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  forgetGameNames();
  mocks.newProject.mockResolvedValue({path:null,project:structuredClone(reviewedProject)});
  mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null);
  mocks.saveSettings.mockResolvedValue(undefined); mocks.heroProportions.mockResolvedValue(null);
  mocks.saveProject.mockImplementation(async (path, project) => ({path, project}));
  mocks.gameNames.mockResolvedValue(structuredClone(names));
  mocks.heroes.mockResolvedValue([haze, shivUlt]);
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage('en'));
  for (const key of [MIRROR_KEY, OFFER_SEEN]) localStorage.removeItem(key);
  forgetLaunch();
  forgetGameNames();
});
async function start(settings: Settings, language: 'zh-CN' | 'en') {
  mocks.settings.mockResolvedValue(settings);
  await act(() => setLanguage(language));
  render(<App/>);
  await screen.findByRole('region',{name:language === 'en' ? 'Project summary' : '项目概要'});
}

describe('Game hero names', () => {
  it('the hero picker shows the game’s Chinese names with the English beside them, finds a hero by either name and sends the same path', async () => {
    await start(chinese, 'zh-CN');
    await waitFor(() => expect(heroCard()).toHaveTextContent('维迪塔Vindicta'));
    expect(heroCard().querySelector('.hero-english')).toHaveTextContent('Vindicta');
    expect(mocks.gameNames).toHaveBeenCalledWith('zh-CN');
    fireEvent.click(screen.getAllByRole('button',{name:'选择英雄…'})[0]);
    await waitFor(() => expect(results()).toHaveLength(2));
    // Sorted on the English labels as before; a label with a suffix keeps its suffix.
    expect(results()[0]).toMatch(/^海兹Haze/);
    expect(results()[1]).toMatch(/^希瓦Shiv · Shiv Ult/);
    fireEvent.change(search(), {target:{value:'海兹'}});
    expect(results()).toHaveLength(1);
    expect(results()[0]).toMatch(/^海兹Haze/);
    fireEvent.change(search(), {target:{value:'haze'}});
    expect(results()).toHaveLength(1);
    fireEvent.change(search(), {target:{value:'希瓦'}});
    expect(results()).toEqual([expect.stringMatching(/^希瓦Shiv · Shiv Ult/)]);
    fireEvent.change(search(), {target:{value:'海兹'}});
    mocks.selectHero.mockImplementation(async (project: Project) => ({...project, original_model:haze.path, hero_name:'Haze'}));
    fireEvent.click(within(screen.getByRole('list',{name:'可用的英雄'})).getByRole('button'));
    await waitFor(() => expect(mocks.selectHero).toHaveBeenCalledWith(expect.anything(), haze.path));
    await waitFor(() => expect(heroCard()).toHaveTextContent('海兹Haze'));
    // What is saved keeps the English name and the path.
    mocks.save.mockResolvedValue('C:\\Projects\\haze.dmb.json');
    fireEvent.click(screen.getByRole('button',{name:'保存'}));
    await waitFor(() => expect(mocks.saveProject).toHaveBeenCalled());
    const savedProject = mocks.saveProject.mock.calls[0][1] as Project;
    expect(savedProject.hero_name).toBe('Haze');
    expect(savedProject.original_model).toBe(haze.path);
    expect(JSON.stringify(savedProject)).not.toMatch(/海兹|维迪塔/);
  });

  it('the chosen hero’s Chinese name shows on Build & install and in the copy notice, while the operations and the notification keep English', async () => {
    await start(chinese, 'zh-CN');
    await waitFor(() => expect(heroCard()).toHaveTextContent('维迪塔'));
    mocks.copyProjectForHero.mockImplementation(async (project: Project) => ({project:{...structuredClone(project), id:'c'.repeat(32), hero_name:'Haze', original_model:haze.path}, report:{from_hero:'Vindicta', carried:['Body roles'], not_carried:[], remade:['Joint matches']}}));
    mocks.chooseFreeSlot.mockResolvedValue({slot:23, filename:'pak23_dir.vpk', occupied_slots:[], previous_outputs:[], scanned_folders:[]});
    fireEvent.click(screen.getByRole('button',{name:'为另一位英雄制作副本…'}));
    fireEvent.click(within(await screen.findByRole('list',{name:'副本可用的英雄'})).getAllByRole('button')[0]);
    await waitFor(() => expect(mocks.copyProjectForHero).toHaveBeenCalledWith(expect.anything(), haze.path));
    expect(await screen.findByRole('heading',{name:'海兹的副本'})).toBeInTheDocument();
    expect(screen.getByText('已为海兹制作副本。请将它另存为新项目。')).toHaveClass('toast');
    // The kept notification keeps the English name (it is the record of what happened).
    expect(within(screen.getByRole('region',{name:'通知'})).getByRole('button',{name:/已为 Haze 制作副本/})).toBeInTheDocument();
    fireEvent.click(sidebar().getByRole('button',{name:'构建与安装'}));
    expect(await screen.findByRole('heading',{level:1,name:'构建与安装'})).toBeInTheDocument();
    // The Character row of the checklist names the hero as the game does.
    expect([...document.querySelectorAll('.validation-detail')].map(node => node.textContent)).toContain('海兹');
  });

  it('with empty maps (the game not set up) Chinese shows the English names and nothing fails', async () => {
    mocks.gameNames.mockResolvedValue(structuredClone(empty));
    await start({...chinese, pak:''}, 'zh-CN');
    await waitFor(() => expect(mocks.gameNames).toHaveBeenCalledWith('zh-CN'));
    expect(heroCard().textContent).toBe('Vindicta');
    fireEvent.click(screen.getAllByRole('button',{name:'选择英雄…'})[0]);
    await waitFor(() => expect(results()).toHaveLength(2));
    expect(document.querySelector('.hero-results .hero-english')).toBeNull();
    expect(results()[0]).toMatch(/^Haze/);
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('a failed call leaves the English names', async () => {
    mocks.gameNames.mockRejectedValue(new Error('not available'));
    await start(chinese, 'zh-CN');
    await waitFor(() => expect(mocks.gameNames).toHaveBeenCalled());
    expect(heroCard().textContent).toBe('Vindicta');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('never asks while the interface is English, asks at once on a live switch to Chinese, and shows English again without asking', async () => {
    await start(english, 'en');
    expect(heroCard().textContent).toBe('Vindicta');
    expect(mocks.gameNames).not.toHaveBeenCalled();
    fireEvent.click(sidebar().getByRole('button',{name:'Settings'}));
    fireEvent.change(screen.getByRole('combobox',{name:/^Language/}), {target:{value:'zh-CN'}});
    await waitFor(() => expect(mocks.gameNames).toHaveBeenCalledTimes(1));
    expect(mocks.gameNames).toHaveBeenCalledWith('zh-CN');
    // The picker's sentence about game names shows in Chinese only.
    expect(await screen.findByText('英雄和物品名称与游戏在该语言下显示的名称一致。')).toBeInTheDocument();
    fireEvent.click(sidebar().getByRole('button',{name:'角色与模型'}));
    await waitFor(() => expect(heroCard()).toHaveTextContent('维迪塔Vindicta'));
    fireEvent.click(sidebar().getByRole('button',{name:'设置'}));
    fireEvent.change(screen.getByRole('combobox',{name:/^语言/}), {target:{value:'en'}});
    expect(await sidebar().findByRole('button',{name:'Settings'})).toBeInTheDocument();
    expect(screen.queryByText("Hero and item names follow the game's own names in the chosen language.")).toBeNull();
    fireEvent.click(sidebar().getByRole('button',{name:'Character & model'}));
    await waitFor(() => expect(heroCard().textContent).toBe('Vindicta'));
    expect(mocks.gameNames).toHaveBeenCalledTimes(1);
  });

  it('asks again when the saved game archive changes', async () => {
    await start(chinese, 'zh-CN');
    await waitFor(() => expect(mocks.gameNames).toHaveBeenCalledTimes(1));
    mocks.detectSettings.mockResolvedValue({...chinese, pak:'D:\\Deadlock\\game\\citadel\\pak01_dir.vpk'});
    mocks.setupStatus.mockResolvedValue({files_found:{},model_files_found:false,model_status:'',fields:[]});
    fireEvent.click(sidebar().getByRole('button',{name:'设置'}));
    fireEvent.click(screen.getAllByRole('button',{name:'检测工具'})[0]);
    await waitFor(() => expect(mocks.detectSettings).toHaveBeenCalled());
    // Detected paths are only a draft: nothing is asked until they are saved.
    expect(mocks.gameNames).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getAllByRole('button',{name:'保存设置'})[0]);
    await waitFor(() => expect(mocks.saveSettings).toHaveBeenCalled());
    await waitFor(() => expect(mocks.gameNames).toHaveBeenCalledTimes(2));
  });
});
