import { useEffect, useRef, useState, type ReactElement } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Project, ResourceReplacement } from '../models';
import type { AnimationEntry } from '../components/AssetCatalog';
import type { PreviewScene } from '../preview/packet';
import { HistoryContext, type HistoryControls } from '../history/useProjectHistory';
import { ProjectHistory } from '../history/projectHistory';
import { MotionPage } from './MotionPage';
import { forgetClipPackets } from './ClipPlayer';
import { AUTHORED_UNTRIED } from './ClipCard';
import { LIST_FAILED, MAKE_AGAIN, MAKE_FAILED, forgetMadeState } from './MakeClip';
import { ADDITIVE, MADE_SUPERSEDED, NEEDS_PREPARED, NOT_MADE, SKELETON_ADVICE, SKELETON_DIFFERS, type ActionList, type MadeReport } from './makeTypes';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

const native = vi.hoisted(() => ({
  motionCatalog: vi.fn(), clipFit: vi.fn(), clipPreview: vi.fn(), validateAssetPath: vi.fn(),
  madeClipCheck: vi.fn(), madeClipActions: vi.fn(), makeClip: vi.fn(), madeClipReport: vi.fn(), madeClipPreview: vi.fn(),
}));
vi.mock('../api', async original => ({ ...await original<typeof import('../api')>(), api: native }));
const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => dialog);
const probe = vi.hoisted(() => ({ mounts: 0, unmounts: 0 }));
vi.mock('./ClipViewport', () => ({ default: function Probe({ main }: { main: { header: { clips: { name: string; which: string }[] } } }) {
  useEffect(() => { probe.mounts++; return () => { probe.unmounts++; }; }, []);
  return <div data-testid="clip-view">{main.header.clips[0].name}:{main.header.clips[0].which}</div>;
} }));

const HERO = 'models/heroes_wip/sample/sample.vmdl_c';
const clip = (label: string, extra: Partial<AnimationEntry> = {}): AnimationEntry => ({
  path: `models/heroes_wip/sample/clips/${label}.vnmclip_c`, target: `models/heroes_wip/sample/clips/${label}.vnmclip_c`, label, supported: true,
  skeleton: 'models/heroes_wip/sample/sample.vnmskel', frames: 56, duration: 1.8333, tracks: 72, events: 2, additive: false,
  name: label.replace(/_/g, ' ').replace(/^./, c => c.toUpperCase()), group: 'movement', graph_named: true, ...extra,
});
const ENTRIES = [clip('stand_idle'), clip('run_n', { name: 'Run, forward', frames: 25, duration: 0.8 }), clip('lean_forward', { additive: true, frames: 2, duration: 0.0333 })];
const target = (label: string) => `models/heroes_wip/sample/clips/${label}.vnmclip_c`;
const FILE = 'C:\\anims\\my_character_moves.blend';
const MODEL = 'C:\\models\\my_character.blend';
const base = { ...reference.cases[1].expected, original_model: HERO, hero_name: 'Sample', dmx: 'C:\\work\\prepared.dmx', preparation_source: MODEL, animation_policy: 'preserve', ag2_mode: 'auto' } as unknown as Project;
const withRows = (rows: ResourceReplacement[]) => ({ ...base, customization: { ...base.customization, enabled: true, animations: rows } }) as Project;
const LIST: ActionList = { source: FILE, rig: 'Armature', bones: 40, driving_bones: 40, skeleton_error: null, actions: [
  { name: 'Wave', frame_start: 0, frame_end: 85, fps: 30, length_seconds: 2.8333, keyed_bones: ['Hips', 'Spine'], driving_keyed_bones: 14 },
  { name: 'Walk', frame_start: 0, frame_end: 30, fps: 30, length_seconds: 1, keyed_bones: [], driving_keyed_bones: 38 },
  { name: 'Camera', frame_start: 0, frame_end: 10, fps: 30, length_seconds: 0.3333, keyed_bones: [], driving_keyed_bones: 0 },
] };
const REPORT: MadeReport = {
  action: 'Wave', mode: 'as_it_looks', switches: { new_fixed_values: true, refit_ranges: true, still_to_moving: true },
  clip_frames: 56, clip_seconds: 1.8333, action_seconds: 2.8333, plays_from: 0, plays_to: 2.8333, stretch: 1.545,
  follow: ['pelvis', 'spine_1', 'neck_0', 'head', 'arm_upper_L'], carried: ['arm_L_IKTARGET'], between: [], kept_count: 74,
  kept_kinds: { fingers: 38, gun: 8, cloth: 12, other: 16 }, kept_examples: {}, not_in_clip: [], still_made_moving: ['pelvis'],
  still_new_fixed_value: [], still_held: [], still_kept_game_value: [], ranges_refitted: [], ranges_clamped: [], words_per_frame: [75, 78],
  loop_gap_degrees: 12.3, loop_gap_inches: 0.4, loop_blend_seconds: null, drift_removed_inches: 72.04, feet_lowest_inches: -0.79, feet_highest_inches: -0.78,
};
const madeRow = (made: Record<string, unknown> = {}): ResourceReplacement => ({ enabled: true, source: FILE, target: target('stand_idle'), allow_shared: false, made: { action: 'Wave', mode: 'as_it_looks', bones: 'keyed', ...made } as never });
function packet(name: string, which = 'game', frames = 4) {
  const header = new TextEncoder().encode(JSON.stringify({ bones: [{ name: 'root', parent: -1, position: [0, 0, 0] }], clips: [{ name, sources: [name], fps: 30, frames, segments: [frames], offset: 0, duration: (frames - 1) / 30, clip_frames: frames, additive: false, blend_may_differ: false, cut: false, which, marks: [] }] }));
  const start = Math.ceil((4 + header.length) / 4) * 4, out = new ArrayBuffer(start + frames * 48);
  new DataView(out).setUint32(0, header.length, true); new Uint8Array(out, 4, header.length).set(header);
  return out;
}
const scene = { metadata: { bounds: { min: [0, 0, 0], max: [1, 1, 2] }, meshes: [], bones: [] } } as unknown as PreviewScene;

/** The page with a history like the app's: every update is recorded, gestures go to it, and the
 * events (writes and operations) are logged in order. */
const events: string[] = [];
let history = new ProjectHistory();
function Page({ initial, preview = null, operate = true }: { initial: Project; preview?: PreviewScene | null; operate?: boolean }) {
  const [project, setProject] = useState(initial);
  const ref = useRef(project);
  const controls: HistoryControls = { canUndo: false, canRedo: false, undo: () => undefined, redo: () => undefined, gesture: on => { events.push(`gesture:${on}`); history.gesture(on); }, fence: () => history.fence() };
  const update = (next: Project) => { events.push('write'); history.record(ref.current, next); ref.current = next; setProject(next); };
  const run = async <T,>(label: string, operation: () => Promise<T>) => { events.push(`operate:${label}`); return operation(); };
  return <HistoryContext.Provider value={controls}>
    <MotionPage project={project} update={update} disabled={false} fail={vi.fn()} preview={preview} load={vi.fn()} next={vi.fn()} operate={operate ? run : undefined} openPreparation={() => events.push('open-preparation')}/>
    <output data-testid="saved">{JSON.stringify(project)}</output>
  </HistoryContext.Provider>;
}
const saved = () => JSON.parse(screen.getByTestId('saved').textContent!) as Project;
const list = () => screen.getByRole('region', { name: 'Animations' });
const rowButton = (name: RegExp) => within(list()).getByRole('button', { name });
const card = (name = 'Stand idle') => screen.getByRole('region', { name: `Clip ${name}` });
const routeButton = (name: string) => within(card()).getByRole('button', { name });
async function open(ui: ReactElement) {
  const view = render(ui);
  await waitFor(() => expect(list().querySelector('.clip-row')).not.toBeNull());
  return view;
}

beforeEach(() => {
  vi.clearAllMocks(); forgetClipPackets(); forgetMadeState(); probe.mounts = 0; probe.unmounts = 0; events.length = 0; history = new ProjectHistory();
  native.motionCatalog.mockImplementation(async () => ({ hero_path: HERO, animations: ENTRIES }));
  native.clipFit.mockResolvedValue({ kind: 'compiled', fits: true, checks: [] });
  native.clipPreview.mockImplementation(async (_p: Project, path: string) => packet(path.split('/').pop()!.replace('.vnmclip_c', '')));
  native.madeClipCheck.mockImplementation(async (_p: Project, t: string) => ({ target: t, refusal: null, fit_error: null }));
  native.madeClipActions.mockResolvedValue(LIST);
  native.makeClip.mockImplementation(async (_p: Project, t: string) => ({ target: t, made: {}, report: REPORT }));
  native.madeClipReport.mockResolvedValue(REPORT);
  native.madeClipPreview.mockImplementation(async () => packet('stand_idle', 'made'));
});

describe('Make it from my animation', () => {
  it('flipping the switch writes nothing, and the compiled route is unchanged', async () => {
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Stand idle/));
    const before = screen.getByTestId('saved').textContent;
    expect(routeButton('A compiled clip file')).toHaveAttribute('aria-pressed', 'true');
    expect(card()).not.toHaveTextContent('does not convert');
    fireEvent.click(routeButton('Make it from my animation'));
    expect(await within(card()).findByRole('button', { name: 'Choose file…' })).toBeInTheDocument();
    expect(native.madeClipCheck).toHaveBeenCalledWith(expect.anything(), target('stand_idle'));
    fireEvent.click(routeButton('A compiled clip file'));
    expect(within(card()).getByRole('button', { name: 'Choose a file…' })).toBeInTheDocument();
    expect(screen.getByTestId('saved').textContent).toBe(before);
    expect(events).toEqual([]);
    expect(native.madeClipActions).not.toHaveBeenCalled();
    expect(native.makeClip).not.toHaveBeenCalled();
  });
  it('says plainly when the clip cannot be made, and offers the compiled file instead', async () => {
    native.madeClipCheck.mockImplementation(async (_p: Project, t: string) => ({ target: t, refusal: ADDITIVE, fit_error: null }));
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Lean forward/));
    fireEvent.click(within(card('Lean forward')).getByRole('button', { name: 'Make it from my animation' }));
    const note = await within(card('Lean forward')).findByRole('note');
    expect(note).toHaveTextContent(ADDITIVE);
    expect(note.querySelector('strong')).toHaveTextContent('This clip cannot be made from an animation.');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(within(card('Lean forward')).queryByRole('button', { name: 'Choose file…' })).not.toBeInTheDocument();
    fireEvent.click(within(card('Lean forward')).getByRole('button', { name: 'Use a compiled clip file instead' }));
    expect(within(card('Lean forward')).getByRole('button', { name: 'A compiled clip file' })).toHaveAttribute('aria-pressed', 'true');
    expect(events).toEqual([]);
  });
  it('asks to prepare the model again with a link to Prepare model', async () => {
    native.madeClipCheck.mockImplementation(async (_p: Project, t: string) => ({ target: t, refusal: null, fit_error: NEEDS_PREPARED }));
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Stand idle/));
    fireEvent.click(routeButton('Make it from my animation'));
    expect(await within(card()).findByRole('note')).toHaveTextContent(NEEDS_PREPARED);
    fireEvent.click(within(card()).getByRole('button', { name: /Open Prepare model/ }));
    expect(events).toEqual(['open-preparation']);
  });
  it('lists a chosen file’s animations as an operation without writing, then choosing one writes ONE step before making the clip', async () => {
    dialog.open.mockResolvedValueOnce(FILE);
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Stand idle/));
    fireEvent.click(routeButton('Make it from my animation'));
    fireEvent.click(await within(card()).findByRole('button', { name: 'Choose file…' }));
    await within(card()).findByText(/The same skeleton as your prepared character \(40 bones\)/);
    expect(dialog.open).toHaveBeenCalledWith(expect.objectContaining({ filters: [{ name: 'Blender, FBX or glTF file', extensions: ['blend', 'fbx', 'glb', 'gltf'] }] }));
    expect(native.madeClipActions).toHaveBeenCalledWith(expect.anything(), FILE);
    expect(events).toEqual([`operate:${LIST_FAILED}`]);
    expect(saved().customization.animations).toEqual([]);
    const select = within(card()).getByRole('combobox', { name: 'Animation' });
    expect(within(select).getAllByRole('option').map(o => o.textContent)).toEqual(['Choose an animation…', 'Wave · 2.83 s · 86 frames', 'Walk · 1.00 s · 31 frames', 'Camera · 0.33 s · 11 frames (moves none of your character’s bones)']);
    let atMake: Project | null = null;
    native.makeClip.mockImplementationOnce(async (p: Project, t: string) => { atMake = p; return { target: t, made: {}, report: REPORT }; });
    fireEvent.change(select, { target: { value: 'Wave' } });
    await waitFor(() => expect(native.makeClip).toHaveBeenCalledTimes(1));
    // The write came first (one step), then the operation; the operation got the written project.
    expect(events).toEqual([`operate:${LIST_FAILED}`, 'write', `operate:${MAKE_FAILED}`]);
    const row = { enabled: true, source: FILE, target: target('stand_idle'), allow_shared: false, made: { action: 'Wave', mode: 'as_it_looks', bones: 'keyed' } };
    expect(saved().customization.animations).toEqual([row]);
    expect(atMake!.customization.animations).toEqual([row]);
    expect(native.makeClip).toHaveBeenCalledWith(expect.anything(), target('stand_idle'), row);
    expect(history.nextUndo()?.before.customization.animations).toEqual([]);
    // The answer is shown, never saved.
    expect(await within(card()).findByRole('list', { name: 'What the app makes' })).toHaveTextContent('5 bones follow your animation (pelvis, spine 1, neck 0, head, …)');
    expect(saved().customization.animations).toEqual([row]);
    expect(card()).toHaveTextContent('In this clip the feet end 0.8 in below the floor');
    expect(card()).toHaveTextContent('72.0 in of travel was removed.');
    expect(card()).toHaveTextContent('The game fixes this clip’s length at 1.83 s. Your 2.83 s animation plays 1.5 times as fast to fit it.');
    expect(card()).not.toHaveTextContent(AUTHORED_UNTRIED);
    expect(rowButton(/Stand idle/)).toHaveTextContent('Replaced');
  });
  it('offers the model’s own Blender file and names the bone of a skeleton that differs', async () => {
    const differs = `${SKELETON_DIFFERS} (the bone spine_02 is missing). ${SKELETON_ADVICE}`;
    native.madeClipActions.mockResolvedValueOnce({ ...LIST, source: MODEL, skeleton_error: differs });
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Stand idle/));
    fireEvent.click(routeButton('Make it from my animation'));
    fireEvent.click(await within(card()).findByRole('button', { name: 'Use my model’s own file' }));
    expect(await within(card()).findByText(differs)).toBeInTheDocument();
    expect(native.madeClipActions).toHaveBeenCalledWith(expect.anything(), MODEL);
    expect(within(card()).queryByRole('combobox', { name: 'Animation' })).not.toBeInTheDocument();
    expect(saved().customization.animations).toEqual([]);
  });
  it('says so in one line when a file has no animation that moves the character', async () => {
    native.madeClipActions.mockResolvedValueOnce({ ...LIST, actions: [LIST.actions[2]] });
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Stand idle/));
    fireEvent.click(routeButton('Make it from my animation'));
    fireEvent.click(await within(card()).findByRole('button', { name: 'Use my model’s own file' }));
    expect(await within(card()).findByText('This file has no animation that moves your character’s bones.')).toBeInTheDocument();
  });
  it('opens a made clip on the make route with its report, writes each option as a step without Blender, and the mode with Blender', async () => {
    await open(<Page initial={withRows([madeRow()])}/>);
    expect(routeButton('Make it from my animation')).toHaveAttribute('aria-pressed', 'true');
    expect(await within(card()).findByRole('list', { name: 'What the app makes' })).toBeInTheDocument();
    expect(native.clipFit).not.toHaveBeenCalled();
    expect(native.madeClipActions).not.toHaveBeenCalled();
    expect(within(card()).getByText('Made from your animation.')).toBeInTheDocument();
    fireEvent.click(within(card()).getByRole('radio', { name: /Every matched bone/ }));
    expect(saved().customization.animations?.[0].made).toEqual({ action: 'Wave', mode: 'as_it_looks', bones: 'all' });
    fireEvent.click(within(card()).getByRole('switch', { name: 'Stay in place' }));
    expect(saved().customization.animations?.[0].made).toEqual({ action: 'Wave', mode: 'as_it_looks', in_place: false, bones: 'all' });
    fireEvent.click(within(card()).getByRole('switch', { name: 'Loop' }));
    expect(saved().customization.animations?.[0].made).toEqual({ action: 'Wave', mode: 'as_it_looks', loop_blend_seconds: 0.25, in_place: false, bones: 'all' });
    expect(card()).toHaveTextContent('Before the blend, its last frame is 12° and 0.4 in away from its first.');
    await waitFor(() => expect(native.madeClipReport).toHaveBeenLastCalledWith(expect.anything(), target('stand_idle'), expect.objectContaining({ made: { action: 'Wave', mode: 'as_it_looks', loop_blend_seconds: 0.25, in_place: false, bones: 'all' } })));
    expect(native.makeClip).not.toHaveBeenCalled();
    expect(events).toEqual(['write', 'write', 'write']);
    fireEvent.click(within(card()).getByRole('radio', { name: /Change from my rest pose/ }));
    await waitFor(() => expect(native.makeClip).toHaveBeenCalledTimes(1));
    expect(events).toEqual(['write', 'write', 'write', 'write', `operate:${MAKE_FAILED}`]);
  });
  it('a drag of the part’s start is one undo step', async () => {
    const now = vi.spyOn(Date, 'now');
    let clock = 1_000_000; now.mockImplementation(() => (clock += 5000));
    try {
      await open(<Page initial={withRows([madeRow({ part_start_seconds: 0 })])}/>);
      const strip = await within(card()).findByRole('slider', { name: 'Where the part starts' });
      const beforeDrag = saved();
      fireEvent.pointerDown(strip);
      for (const value of ['0.2', '0.5', '0.8']) fireEvent.change(strip, { target: { value } });
      fireEvent.pointerUp(strip);
      expect(saved().customization.animations?.[0].made?.part_start_seconds).toBe(0.8);
      expect(events).toEqual(['gesture:true', 'write', 'write', 'write', 'gesture:false']);
      const step = history.nextUndo()!;
      expect(step.before.customization.animations).toEqual(beforeDrag.customization.animations);
      expect(step.after.customization.animations?.[0].made?.part_start_seconds).toBe(0.8);
      expect(within(card()).getByRole('button', { name: 'A part at its own speed' })).toHaveAttribute('aria-pressed', 'true');
      expect(card()).toHaveTextContent('Starts at 0.80 s of 2.83 s');
    } finally { now.mockRestore(); }
  });
  it('asks for the report one request at a time and shows only the newest answer', async () => {
    const answers: ((value: MadeReport) => void)[] = [];
    native.madeClipReport.mockImplementation(() => new Promise<MadeReport>(resolve => { answers.push(resolve); }));
    await open(<Page initial={withRows([madeRow()])}/>);
    await waitFor(() => expect(native.madeClipReport).toHaveBeenCalledTimes(1));
    fireEvent.click(within(card()).getByRole('switch', { name: 'Stay in place' }));
    fireEvent.click(within(card()).getByRole('switch', { name: 'Loop' }));
    // One running, the newest waiting: nothing else was sent.
    expect(native.madeClipReport).toHaveBeenCalledTimes(1);
    await act(async () => { answers[0]({ ...REPORT, follow: ['old'] }); });
    await waitFor(() => expect(native.madeClipReport).toHaveBeenCalledTimes(2));
    expect(native.madeClipReport.mock.calls[1][2].made).toEqual({ action: 'Wave', mode: 'as_it_looks', loop_blend_seconds: 0.25, in_place: false, bones: 'keyed' });
    expect(within(card()).queryByRole('list', { name: 'What the app makes' })).not.toBeInTheDocument();
    await act(async () => { answers[1]({ ...REPORT, follow: ['head'] }); });
    expect(within(card()).getByRole('list', { name: 'What the app makes' })).toHaveTextContent('1 bone follows your animation (head)');
  });
  it('drops a superseded answer quietly', async () => {
    native.madeClipReport.mockRejectedValueOnce({ message: MADE_SUPERSEDED });
    await open(<Page initial={withRows([madeRow()])}/>);
    await waitFor(() => expect(native.madeClipReport).toHaveBeenCalledTimes(1));
    expect(card()).not.toHaveTextContent(MADE_SUPERSEDED);
  });
  it('says in one line when the clip must be made again, and Make it again makes it without clearing anything', async () => {
    native.madeClipReport.mockRejectedValueOnce({ message: `${NOT_MADE}.` });
    const initial = withRows([madeRow({ loop_blend_seconds: 0.3 })]);
    await open(<Page initial={initial}/>);
    expect(await within(card()).findByText(MAKE_AGAIN)).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    fireEvent.click(within(card()).getByRole('button', { name: 'Make it again' }));
    await waitFor(() => expect(native.makeClip).toHaveBeenCalledWith(expect.anything(), target('stand_idle'), initial.customization.animations![0]));
    expect(events).toEqual([`operate:${MAKE_FAILED}`]);
    expect(saved().customization.animations).toEqual(initial.customization.animations);
    // Made again: the report is asked again for the same row and the line goes.
    await waitFor(() => expect(within(card()).queryByText(MAKE_AGAIN)).not.toBeInTheDocument());
    expect(native.madeClipReport).toHaveBeenCalledTimes(2);
  });
  it('a compiled file chosen for a made clip takes its place', async () => {
    native.validateAssetPath.mockImplementation(async (path: string) => path);
    await open(<Page initial={withRows([madeRow()])}/>);
    fireEvent.click(routeButton('A compiled clip file'));
    expect(card()).toHaveTextContent('This clip is made from your animation now.');
    const drop = card().querySelector('[data-native-drop="animation"]')!;
    await act(async () => { drop.dispatchEvent(new CustomEvent('dmb-file-drop', { detail: ['C:\\clips\\stand.vnmclip_c'] })); });
    await waitFor(() => expect(saved().customization.animations).toEqual([{ enabled: true, source: 'C:\\clips\\stand.vnmclip_c', target: target('stand_idle'), allow_shared: false }]));
  });
  it('treats a made row like any replaced clip: include, switch all off and remove', async () => {
    await open(<Page initial={withRows([madeRow()])}/>);
    expect(screen.getByText('1 clip replaced')).toBeInTheDocument();
    fireEvent.click(within(card()).getByLabelText('Include in build'));
    expect(saved().customization.animations?.[0]).toEqual({ ...madeRow(), enabled: false });
    fireEvent.click(within(card()).getByLabelText('Include in build'));
    fireEvent.click(screen.getByRole('button', { name: 'Switch all replacements off' }));
    expect(saved().customization.animations).toEqual([{ ...madeRow(), enabled: false }]);
    fireEvent.click(within(card()).getByRole('button', { name: 'Remove' }));
    expect(saved().customization.animations).toEqual([]);
  });
  it('plays the made clip as Your animation, keeping the one 3D view across the switch, an option and another clip', async () => {
    await open(<Page initial={withRows([madeRow()])} preview={scene}/>);
    const player = screen.getByRole('region', { name: 'Clip player' });
    expect(await screen.findByTestId('clip-view')).toHaveTextContent('stand_idle:game');
    fireEvent.click(within(player).getByRole('button', { name: 'Your animation' }));
    await waitFor(() => expect(screen.getByTestId('clip-view')).toHaveTextContent('stand_idle:made'));
    expect(native.madeClipPreview).toHaveBeenCalledWith(expect.anything(), target('stand_idle'), madeRow());
    expect(native.clipPreview).not.toHaveBeenCalledWith(expect.anything(), expect.anything(), 'yours', expect.anything());
    expect(player).toHaveTextContent('Your animation played on your prepared character: the clip the build writes.');
    // An option: the packet is asked again for the new row; the view stays.
    fireEvent.click(within(card()).getByRole('switch', { name: 'Stay in place' }));
    await waitFor(() => expect(native.madeClipPreview).toHaveBeenLastCalledWith(expect.anything(), target('stand_idle'), expect.objectContaining({ made: expect.objectContaining({ in_place: false }) })));
    fireEvent.click(routeButton('A compiled clip file'));
    fireEvent.click(routeButton('Make it from my animation'));
    fireEvent.click(rowButton(/Run, forward/));
    await waitFor(() => expect(screen.getByTestId('clip-view')).toHaveTextContent('run_n:game'));
    expect(screen.getByRole('region', { name: 'Clip player' })).toBe(player);
    expect(probe.mounts).toBe(1);
    expect(probe.unmounts).toBe(0);
  });
  it('works without the app’s runner, and names nothing outside its own clip', async () => {
    await open(<Page initial={withRows([])} operate={false}/>);
    fireEvent.click(rowButton(/Stand idle/));
    fireEvent.click(routeButton('Make it from my animation'));
    fireEvent.click(await within(card()).findByRole('button', { name: 'Use my model’s own file' }));
    const select = await within(card()).findByRole('combobox', { name: 'Animation' });
    fireEvent.change(select, { target: { value: 'Walk' } });
    await waitFor(() => expect(native.makeClip).toHaveBeenCalledTimes(1));
    expect(saved().customization.animations?.[0].made).toEqual({ action: 'Walk', mode: 'as_it_looks', bones: 'all' });
  });
});
