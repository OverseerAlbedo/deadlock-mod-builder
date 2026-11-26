/** Prepare model inside the app, in Chinese: a failed preparation keeps its English failure
 * title, its message and its help, and the page around it stays Chinese. */
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project, Settings } from '../models';

const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),restoreAutosave:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),checkSettingFields:vi.fn(),heroProportions:vi.fn(),operationDurations:vi.fn(),
  settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),openProject:vi.fn(),saveProject:vi.fn(),latestTimings:vi.fn(),logs:vi.fn(),preparationInspect:vi.fn(),
  open:vi.fn(),save:vi.fn(),ask:vi.fn(),drop:vi.fn(),close:vi.fn(),
}));
vi.mock('../api', async original => ({...await original<typeof import('../api')>(),isDesktop:() => true,api:mocks}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:mocks.drop,onCloseRequested:mocks.close,destroy:vi.fn()})}));
import { App } from '../App';
import { MIRROR_KEY, setLanguage } from '../i18n';
import { OFFER_SEEN, forgetLaunch } from '../i18n/startup';

const project: Project = {...structuredClone(reference.cases[0].expected as unknown as Project), name:'Sample', hero_name:'Haze', original_model:'models/heroes_staging/haze/haze.vmdl', preparation_source:'D:\\Characters\\sample\\sample.fbx', preparation_options:{}, model_input_mode:'rigged_humanoid'};
const settings = {steam:'',deadlock:'C:\\Games\\Deadlock',pak:'C:\\Game\\pak01_dir.vpk',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:'',language:'zh-CN'} as Settings;
const failure = {kind:'tool', message:'Blender stopped while reading the model', details:'worker.py: import failed', help:{cause:'Blender could not read the model file.', steps:['Open the file in Blender once.']}};
const notices = () => within(screen.getByRole('region', {name:/^(Notifications|通知)$/}));
const statusText = () => document.querySelector('.statusbar-text')?.textContent;

beforeEach(async () => {
  vi.clearAllMocks(); localStorage.clear(); localStorage.setItem('dmb.setupGuideSeen', '1'); localStorage.setItem('dmb.uiMode', 'expert');
  mocks.newProject.mockResolvedValue({path:null, project:structuredClone(project)});
  mocks.settings.mockResolvedValue(settings); mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null); mocks.saveSettings.mockResolvedValue(undefined);
  mocks.drop.mockImplementation(async () => vi.fn()); mocks.close.mockImplementation(async () => vi.fn());
  mocks.preparationInspect.mockRejectedValue(failure);
  await act(() => setLanguage('zh-CN'));
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage('en'));
  for (const key of [MIRROR_KEY, OFFER_SEEN]) localStorage.removeItem(key);
  forgetLaunch();
});

describe('Prepare model in the app, in Chinese', () => {
  it('a failed automatic preparation keeps its failure title, message and help', async () => {
    render(<App/>); await screen.findByLabelText('项目名称');
    fireEvent.click(within(screen.getByRole('navigation', {name:'主导航'})).getByRole('button', {name:'准备模型'}));
    expect(await screen.findByRole('heading', {level:1, name:/准备模型/})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name:'自动准备'}));
    // The message comes from the backend and is shown as it comes.
    expect(await screen.findByRole('alert')).toHaveTextContent('Blender stopped while reading the model');
    // The notification shows the failure title in Chinese; its badge is read out in Chinese.
    const row = await notices().findByRole('button', {name:/模型准备失败/});
    expect(row.querySelector('.sr-only')?.textContent).toBe('错误');
    fireEvent.click(notices().getByRole('button', {name:'显示完整详情'}));
    expect(within(screen.getByRole('alert')).getByText('Blender could not read the model file.')).toBeInTheDocument();
    await waitFor(() => expect(statusText()).toBe('因错误而停止'));
    // The page itself stays Chinese.
    expect(screen.getByRole('button', {name:'仅加载并检查'})).toBeInTheDocument();
  });
});
