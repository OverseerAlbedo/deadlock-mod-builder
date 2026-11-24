/** Undo and Redo of the whole project in the app: the header buttons, Ctrl+Z / Ctrl+Y, the
 * fences after operations, and what an undo does to the unsaved mark and the analysis. */
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import reference from '../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project, Settings } from './models';

const mocks = vi.hoisted(() => ({
  sessionRecovery:vi.fn(),dismissRecovery:vi.fn(),restoreAutosave:vi.fn(),autosaveProject:vi.fn(),clearAutosaves:vi.fn(),checkSettingFields:vi.fn(),heroProportions:vi.fn(),operationDurations:vi.fn(),
  inspectProject:vi.fn(),settings:vi.fn(),saveSettings:vi.fn(),newProject:vi.fn(),openProject:vi.fn(),saveProject:vi.fn(),latestTimings:vi.fn(),logs:vi.fn(),vpkSlots:vi.fn(),
  open:vi.fn(),save:vi.fn(),ask:vi.fn(),drop:vi.fn(),close:vi.fn(),
}));
vi.mock('./api', async original => ({...await original<typeof import('./api')>(),isDesktop:() => true,api:mocks}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open:mocks.open,save:mocks.save,ask:mocks.ask}));
vi.mock('@tauri-apps/api/event', () => ({listen:vi.fn(async () => vi.fn())}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow:() => ({onDragDropEvent:mocks.drop,onCloseRequested:mocks.close,destroy:vi.fn()})}));
import { App } from './App';
import { inspected, reviewedProject } from './build/testFixtures';

const draft = reference.cases[0].expected as unknown as Project;
const settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:''} as Settings;
let clock = 0;
beforeEach(() => {
  vi.clearAllMocks(); localStorage.clear(); localStorage.setItem('dmb.setupGuideSeen', '1'); localStorage.setItem('dmb.uiMode', 'expert');
  mocks.newProject.mockResolvedValue({path:null,project:structuredClone(draft)});
  mocks.settings.mockResolvedValue(settings); mocks.logs.mockResolvedValue([]); mocks.latestTimings.mockResolvedValue(null);
  mocks.drop.mockImplementation(async () => vi.fn()); mocks.close.mockImplementation(async () => vi.fn());
  mocks.ask.mockResolvedValue(true);
  // Edits further apart than the merge window are separate steps.
  clock = 1_000_000; const real = Date.now.bind(Date); vi.spyOn(Date, 'now').mockImplementation(() => real() + clock);
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const header = () => within(document.querySelector<HTMLElement>('header.topbar')!);
const undoButton = () => header().getByRole('button', {name:'Undo'});
const redoButton = () => header().getByRole('button', {name:'Redo'});
const rename = (value: string) => { clock += 5000; fireEvent.change(screen.getByLabelText('Project name'), {target:{value}}); };
const navigate = (name: string) => fireEvent.click(within(screen.getByRole('navigation', {name:'Main navigation'})).getByRole('button', {name}));

describe('Undo and Redo of the project', () => {
  it('takes an edit back and brings it again, with the header buttons, and says what it undid', async () => {
    render(<App/>); await screen.findByLabelText('Project name');
    expect(undoButton()).toBeDisabled(); expect(redoButton()).toBeDisabled();
    expect(undoButton()).toHaveAttribute('title', 'Undo (Ctrl+Z)');
    rename('First name');
    expect(undoButton()).toBeEnabled();
    fireEvent.click(undoButton());
    expect(screen.getByLabelText('Project name')).toHaveValue(draft.name);
    expect(screen.getByRole('status')).toHaveTextContent('Undone: Project name');
    expect(undoButton()).toBeDisabled(); expect(redoButton()).toBeEnabled();
    fireEvent.click(redoButton());
    expect(screen.getByLabelText('Project name')).toHaveValue('First name');
    expect(screen.getByRole('status')).toHaveTextContent('Redone: Project name');
  });
  it('merges quick edits of one field, keeps slower ones apart, and Ctrl+Z / Ctrl+Y move one step', async () => {
    // Extras shows its tabs only once a hero is chosen.
    mocks.newProject.mockResolvedValue({path:null,project:{...structuredClone(draft), original_model:'models/heroes/test.vmdl'}});
    render(<App/>); await screen.findByLabelText('Project name');
    rename('A');
    // Typing: the next keystrokes come quickly and are the same step.
    clock += 200; fireEvent.change(screen.getByLabelText('Project name'), {target:{value:'Ab'}});
    clock += 200; fireEvent.change(screen.getByLabelText('Project name'), {target:{value:'Abc'}});
    rename('Second');
    // The Colors page is mounted (Extras): there is still only one handler, so one key is one step.
    navigate('Extras'); fireEvent.click(screen.getByRole('tab', {name:/Colors/}));
    fireEvent.keyDown(document.body, {key:'z', ctrlKey:true});
    navigate('Character & model');
    expect(screen.getByLabelText('Project name')).toHaveValue('Abc');
    fireEvent.keyDown(document.body, {key:'z', ctrlKey:true});
    expect(screen.getByLabelText('Project name')).toHaveValue(draft.name);
    fireEvent.keyDown(document.body, {key:'y', ctrlKey:true});
    expect(screen.getByLabelText('Project name')).toHaveValue('Abc');
    fireEvent.keyDown(document.body, {key:'Z', ctrlKey:true, shiftKey:true});
    expect(screen.getByLabelText('Project name')).toHaveValue('Second');
  });
  it('leaves Ctrl+Z in a text field to the field', async () => {
    render(<App/>); await screen.findByLabelText('Project name');
    rename('Typed');
    const field = screen.getByLabelText('Project name');
    fireEvent.keyDown(field, {key:'z', ctrlKey:true});
    expect(field).toHaveValue('Typed');
    expect(undoButton()).toBeEnabled();
  });
  it('marks the project unsaved like an edit, also after a save', async () => {
    mocks.save.mockResolvedValue('C:\\Projects\\a.dmb.json');
    mocks.saveProject.mockImplementation(async (path: string, project: Project) => ({path, project}));
    render(<App/>); await screen.findByLabelText('Project name');
    rename('Saved name');
    fireEvent.click(header().getByRole('button', {name:'Save'}));
    await waitFor(() => expect(screen.queryByLabelText('Unsaved project changes')).not.toBeInTheDocument());
    // Saving is not a fence: the edit before it can still be taken back.
    fireEvent.click(undoButton());
    expect(screen.getByLabelText('Project name')).toHaveValue(draft.name);
    expect(screen.getByLabelText('Unsaved project changes')).toBeInTheDocument();
  });
  it('starts fresh after an operation (Analyze), and an undo or redo makes the analysis current or stale as an edit does', async () => {
    mocks.newProject.mockResolvedValue({path:null,project:structuredClone(reviewedProject)});
    render(<App/>); await screen.findByLabelText('Project name');
    rename('Before analysis');
    const analyzed = {...structuredClone(reviewedProject), name:'Before analysis'};
    mocks.inspectProject.mockResolvedValue({project:analyzed, report:inspected});
    navigate('Build & install'); fireEvent.click(screen.getByRole('button', {name:'Analyze project'}));
    await screen.findByText('Ready to build', {exact:true});
    // The analysis is a fence: the rename before it cannot be undone any more.
    expect(undoButton()).toBeDisabled();
    navigate('Character & model'); rename('After analysis');
    navigate('Build & install'); expect(screen.queryByText('Ready to build', {exact:true})).not.toBeInTheDocument();
    fireEvent.click(undoButton());
    expect(screen.getByText('Ready to build', {exact:true})).toBeInTheDocument();
    fireEvent.click(redoButton());
    expect(screen.queryByText('Ready to build', {exact:true})).not.toBeInTheDocument();
    expect(screen.getByLabelText('Unsaved project changes')).toBeInTheDocument();
  });
  it('starts fresh when another project is opened or a new one is made', async () => {
    mocks.open.mockResolvedValue('C:\\Projects\\other.dmb.json');
    mocks.openProject.mockResolvedValue({path:'C:\\Projects\\other.dmb.json', project:{...structuredClone(draft), id:'b'.repeat(32), name:'Other'}});
    render(<App/>); await screen.findByLabelText('Project name');
    rename('Edited');
    fireEvent.click(header().getByRole('button', {name:'Open…'}));
    await screen.findByDisplayValue('Other');
    expect(undoButton()).toBeDisabled();
    rename('Edited again');
    fireEvent.click(header().getByRole('button', {name:'New'}));
    await waitFor(() => expect(screen.getByLabelText('Project name')).toHaveValue(draft.name));
    expect(undoButton()).toBeDisabled(); expect(redoButton()).toBeDisabled();
  });
});
