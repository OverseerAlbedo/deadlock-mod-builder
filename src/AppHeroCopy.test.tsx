/** "Make a copy for another hero" in the app: the new project is open and unsaved, the original
 * is never sent anywhere to be changed, Prepare model checks the copy's source for the new hero,
 * the notice lists what was carried and what was not, and the undo history starts fresh. */
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import reference from '../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { HeroCopy, Project, Settings } from './models';

const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),restoreAutosave:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),checkSettingFields:vi.fn(),heroProportions:vi.fn(),operationDurations:vi.fn(),
  settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),openProject:vi.fn(),saveProject:vi.fn(),latestTimings:vi.fn(),logs:vi.fn(),
  heroes:vi.fn(),selectHero:vi.fn(),copyProjectForHero:vi.fn(),chooseFreeSlot:vi.fn(),preparationInspect:vi.fn(),
  open:vi.fn(),save:vi.fn(),ask:vi.fn(),drop:vi.fn(),close:vi.fn(),
}));
vi.mock('./api', async original => ({...await original<typeof import('./api')>(),isDesktop:() => true,api:mocks}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:mocks.drop,onCloseRequested:mocks.close,destroy:vi.fn()})}));
import { App } from './App';
import { COPY_PICKER_LINE } from './components/HeroPicker';

const OLD = 'models/heroes_wip/geist/geist.vmdl', NEW = 'models/heroes_staging/haze/haze.vmdl';
const original: Project = {...structuredClone(reference.cases[0].expected as unknown as Project), name:'My character', hero_name:'Lady Geist', original_model:OLD, preparation_source:'D:\\Characters\\sample\\sample.fbx', vpk_number:41};
const copied: HeroCopy = {
  project: {...structuredClone(original), id:'c'.repeat(32), name:'My character Haze', hero_name:'Haze', original_model:NEW, vpk_number:18, copied_report:'D:\\Workspace\\prepared-1\\preparation-report.json'},
  report: {from_hero:'Lady Geist', carried:['Your model file', 'Body roles on your model\'s bones (14)', 'Material images and settings (6 materials)'], not_carried:['12 sounds', '3 effect files', 'The gun fit'], remade:['Joint matches', 'Mesh mapping', 'VPK slot']},
};
const settings = {steam:'',deadlock:'',pak:'C:\\Game\\pak01_dir.vpk',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:''} as Settings;
beforeEach(() => {
  vi.clearAllMocks(); localStorage.clear(); localStorage.setItem('dmb.setupGuideSeen', '1'); localStorage.setItem('dmb.uiMode', 'expert');
  mocks.newProject.mockResolvedValue({path:'C:\\Projects\\my.dmb.json', project:structuredClone(original)});
  mocks.settings.mockResolvedValue(settings); mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null);
  mocks.drop.mockImplementation(async () => vi.fn()); mocks.close.mockImplementation(async () => vi.fn());
  mocks.ask.mockResolvedValue(true);
  mocks.heroes.mockResolvedValue([{name:'geist', path:OLD, likely_hero:true, hero:'Lady Geist'}, {name:'haze', path:NEW, likely_hero:true, hero:'Haze'}]);
  mocks.copyProjectForHero.mockResolvedValue(structuredClone(copied));
  mocks.chooseFreeSlot.mockResolvedValue({slot:23, filename:'pak23_dir.vpk', occupied_slots:[], previous_outputs:[], scanned_folders:[]});
  mocks.preparationInspect.mockRejectedValue({kind:'validation', message:'Stopped by the test'});
});
afterEach(() => cleanup());

const header = () => within(document.querySelector<HTMLElement>('header.topbar')!);

describe('Make a copy for another hero', () => {
  it('opens the copy as a new unsaved project on the new hero, at Prepare model, and says what it carried', async () => {
    render(<App/>); await screen.findByLabelText('Project name');
    // An edit first, so the copy is seen to start a fresh undo history.
    fireEvent.change(screen.getByLabelText('Project name'), {target:{value:'My character'}});
    fireEvent.change(screen.getByLabelText('Project name'), {target:{value:'My character 2'}});
    expect(header().getByRole('button', {name:'Undo'})).toBeEnabled();
    fireEvent.click(screen.getByRole('button', {name:'Make a copy for another hero…'}));
    const list = await screen.findByRole('list', {name:'Heroes for the copy'});
    // The list says what the choice is for.
    expect(screen.getByText(COPY_PICKER_LINE)).toBeInTheDocument();
    // The project's own hero cannot be chosen for its copy.
    expect(within(list).getByRole('button', {name:/Lady Geist/})).toBeDisabled();
    fireEvent.click(within(list).getByRole('button', {name:/Haze/}));
    // The project had unsaved changes: the copy asks first.
    await waitFor(() => expect(mocks.ask).toHaveBeenCalledWith(expect.stringContaining('The copy includes them'), expect.anything()));
    await waitFor(() => expect(mocks.copyProjectForHero).toHaveBeenCalledOnce());
    // The original is only read: it is sent as it is and never saved here.
    expect(mocks.copyProjectForHero).toHaveBeenCalledWith({...original, name:'My character 2'}, NEW);
    expect(mocks.saveProject).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByRole('button', {name:'Prepare model'})).toHaveAttribute('aria-current', 'page'));
    expect(header().getByText('My character Haze')).toBeInTheDocument();
    expect(screen.getByLabelText('Unsaved project changes')).toBeInTheDocument();
    // The copy got a free slot of its own.
    expect(mocks.chooseFreeSlot).toHaveBeenCalledWith(copied.project);
    // The notice: carried, not carried (with the old hero), made again.
    const notice = screen.getByRole('region', {name:'What the copy carried over'});
    expect(within(notice).getByRole('heading', {name:'Copy for Haze'})).toBeInTheDocument();
    expect(within(notice).getByRole('heading', {name:'Carried over (3)'})).toBeInTheDocument();
    expect(within(notice).getByRole('heading', {name:'Not carried, because they belong to Lady Geist (3)'})).toBeInTheDocument();
    for (const line of ['Your model file', '12 sounds', '3 effect files', 'The gun fit', 'Joint matches']) expect(within(notice).getByText(line)).toBeInTheDocument();
    // Prepare model checks the copy's source for the new hero, the usual way.
    await waitFor(() => expect(mocks.preparationInspect).toHaveBeenCalled());
    const [sent, path] = mocks.preparationInspect.mock.calls[0];
    expect(path).toBe(original.preparation_source);
    expect(sent).toMatchObject({id:copied.project.id, original_model:NEW, vpk_number:23});
    // The copy's undo history starts fresh.
    expect(header().getByRole('button', {name:'Undo'})).toBeDisabled();
    // The notice is also kept in Notifications (the automatic preparation moves on).
    expect(screen.getByText('Copy made for Haze')).toBeInTheDocument();
    fireEvent.click(within(notice).getByRole('button', {name:'Close'}));
    expect(screen.queryByRole('region', {name:'What the copy carried over'})).not.toBeInTheDocument();
  });
  it('shows the notice only while the copy still has the hero it names', async () => {
    mocks.selectHero.mockResolvedValue({...structuredClone(copied.project), hero_name:'Lady Geist', original_model:OLD});
    render(<App/>); await screen.findByLabelText('Project name');
    fireEvent.click(screen.getByRole('button', {name:'Make a copy for another hero…'}));
    fireEvent.click(within(await screen.findByRole('list', {name:'Heroes for the copy'})).getByRole('button', {name:/Haze/}));
    await screen.findByRole('region', {name:'What the copy carried over'});
    fireEvent.click(screen.getByRole('button', {name:'Character & model'}));
    expect(screen.getByRole('region', {name:'What the copy carried over'})).toBeInTheDocument();
    const choose = screen.getByRole('button', {name:'Choose character…'});
    await waitFor(() => expect(choose).toBeEnabled());
    fireEvent.click(choose);
    fireEvent.click(within(await screen.findByRole('list', {name:'Available characters'})).getByRole('button', {name:/Lady Geist/}));
    await waitFor(() => expect(mocks.selectHero).toHaveBeenCalledOnce());
    await waitFor(() => expect(screen.queryByRole('region', {name:'What the copy carried over'})).not.toBeInTheDocument());
  });
  it('changes nothing when the unsaved-changes question is declined', async () => {
    render(<App/>); await screen.findByLabelText('Project name');
    fireEvent.change(screen.getByLabelText('Project name'), {target:{value:'Edited'}});
    mocks.ask.mockResolvedValueOnce(false);
    fireEvent.click(screen.getByRole('button', {name:'Make a copy for another hero…'}));
    fireEvent.click(within(await screen.findByRole('list', {name:'Heroes for the copy'})).getByRole('button', {name:/Haze/}));
    await waitFor(() => expect(mocks.ask).toHaveBeenCalled());
    await waitFor(() => expect(header().getByRole('button', {name:'New'})).toBeEnabled());
    expect(mocks.copyProjectForHero).not.toHaveBeenCalled();
    expect(screen.getByLabelText('Project name')).toHaveValue('Edited');
  });
});
