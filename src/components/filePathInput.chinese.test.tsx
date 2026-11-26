import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Settings } from '../models';

// The shared file field in Chinese: its label is shown translated, while the failure title of a
// refused file (the notification's name and what is logged) is built from its English name.
const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),testSetup:vi.fn(),checkSettingFields:vi.fn(),operationDurations:vi.fn(),detectSettings:vi.fn(),setupStatus:vi.fn(),settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),saveProject:vi.fn(),openProject:vi.fn(),latestTimings:vi.fn(),logs:vi.fn(),vpkSlots:vi.fn(),setTheme:vi.fn(),saveThemePresets:vi.fn(),
  heroes:vi.fn(),heroProportions:vi.fn(),selectHero:vi.fn(),validateAssetPath:vi.fn(),capabilities:vi.fn(),
  open:vi.fn(),save:vi.fn(),ask:vi.fn(),invoke:vi.fn(),
}));
vi.mock('../api', async original => ({...await original<typeof import('../api')>(),isDesktop:() => true,api:mocks}));
vi.mock('@tauri-apps/api/core', () => ({invoke:mocks.invoke,isTauri:() => true}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl:vi.fn()}));
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:async () => vi.fn(),onCloseRequested:async () => vi.fn(),destroy:vi.fn()})}));
vi.mock('../preparation/PreparationPage', () => ({PreparationPage:() => <div/>}));
import { App } from '../App';
import { reviewedProject } from '../build/testFixtures';
import { MIRROR_KEY, setLanguage } from '../i18n';
import { OFFER_SEEN, forgetLaunch } from '../i18n/startup';

const blank: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'C:\\Work',last_project:'',blender:''};
const chinese: Settings = {...blank, language:'zh-CN', deadlock:'C:\\Games\\Deadlock'};
const notices = () => within(screen.getByRole('region',{name:/^(Notifications|通知)$/}));

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.newProject.mockResolvedValue({path:null,project:{...structuredClone(reviewedProject), preparation_source:'C:\\Models\\character.blend'}});
  mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null);
  mocks.saveSettings.mockResolvedValue(undefined); mocks.heroProportions.mockResolvedValue(null);
  mocks.settings.mockResolvedValue(chinese); mocks.invoke.mockResolvedValue(null);
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage('en'));
  for (const key of [MIRROR_KEY, OFFER_SEEN]) localStorage.removeItem(key);
  forgetLaunch();
});

describe('The shared file field in Chinese', () => {
  it('shows its Chinese label and names a refused file’s failure by its English name', async () => {
    mocks.validateAssetPath.mockRejectedValue({kind:'invalid', message:'The file is not a supported gun file.'});
    await act(() => setLanguage('zh-CN'));
    render(<App/>);
    const field = await screen.findByLabelText('另一文件中的枪械（可选）');
    fireEvent.change(field, {target:{value:'C:\\Models\\notes.txt'}});
    fireEvent.blur(field);
    // The failure line shows the backend's words as they came.
    expect(await screen.findByRole('alert')).toHaveTextContent('The file is not a supported gun file.');
    expect(mocks.validateAssetPath).toHaveBeenCalledWith('C:\\Models\\notes.txt', 'weapon');
    // The notification (what the app keeps and logs) is named by the English failure title.
    expect(await notices().findByRole('button',{name:/Could not select gun from another file \(optional\)/})).toBeInTheDocument();
    expect(screen.getByLabelText('另一文件中的枪械（可选）')).toHaveValue('');
  });
});
