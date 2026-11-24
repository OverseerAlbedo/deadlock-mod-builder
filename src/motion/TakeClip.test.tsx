import { useEffect, useRef, useState, type ReactElement } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Project, ResourceReplacement } from '../models';
import type { AnimationEntry } from '../components/AssetCatalog';
import type { PreviewScene } from '../preview/packet';
import { HistoryContext, type HistoryControls } from '../history/useProjectHistory';
import { ProjectHistory } from '../history/projectHistory';
import { replacementStatus } from '../extras/models';
import { includedExtras } from '../build/includedExtras';
import { projectReadiness } from '../build/readiness';
import { changesSince } from '../build/changes';
import { MotionPage } from './MotionPage';
import { forgetClipPackets } from './ClipPlayer';
import { forgetMadeState } from './MakeClip';
import { SAME_MOVE_LINE, TAKEN_INTRO, forgetTakenState } from './TakeClip';
import type { MadeReport } from './makeTypes';
import { TAKEN_BODY_NOT_CARRIED, TAKEN_SOURCE_ADDITIVE, TAKEN_SUPERSEDED, TAKEN_TARGET_ADDITIVE, type SourceHero, type TakenReport, type TakenSources } from './takenTypes';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

const native = vi.hoisted(() => ({
  motionCatalog: vi.fn(), clipFit: vi.fn(), clipPreview: vi.fn(), validateAssetPath: vi.fn(),
  madeClipCheck: vi.fn(), madeClipActions: vi.fn(), makeClip: vi.fn(), madeClipReport: vi.fn(), madeClipPreview: vi.fn(),
  takenClipCheck: vi.fn(), takenClipHeroes: vi.fn(), takenClipSources: vi.fn(), takenClipReport: vi.fn(), takenClipPreview: vi.fn(),
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
const OTHER = 'models/heroes_wip/other/other.vmdl';
const BLOCKY = 'models/heroes_wip/blocky/blocky.vmdl';
const target = (label: string) => `models/heroes_wip/sample/clips/${label}.vnmclip_c`;
const theirs = (label: string) => `models/heroes_wip/other/clips/${label}.vnmclip_c`;
const clip = (label: string, extra: Partial<AnimationEntry> = {}): AnimationEntry => ({
  path: target(label), target: target(label), label, supported: true,
  skeleton: 'models/heroes_wip/sample/sample.vnmskel', frames: 32, duration: 1.0333, tracks: 72, events: 2, additive: false,
  name: label.replace(/_/g, ' ').replace(/^./, c => c.toUpperCase()), group: 'movement', graph_named: true, ...extra,
});
const ENTRIES = [clip('dash_ground'), clip('dash_air_forward', { frames: 30, duration: 0.9667 }), clip('stand_idle'), clip('lean_forward', { additive: true, frames: 2, duration: 0.0333 })];
const base = { ...reference.cases[1].expected, original_model: HERO, hero_name: 'Sample', dmx: 'C:\\work\\prepared.dmx', preparation_source: 'C:\\models\\my_character.blend', animation_policy: 'preserve', ag2_mode: 'auto' } as unknown as Project;
const withRows = (rows: ResourceReplacement[]) => ({ ...base, customization: { ...base.customization, enabled: true, animations: rows } }) as Project;

const HEROES: SourceHero[] = [
  { hero: BLOCKY, name: 'Blocky', usable: false, reason: `${TAKEN_BODY_NOT_CARRIED} (its body bones are named differently).` },
  { hero: OTHER, name: 'Other', usable: true },
];
const SOURCES: TakenSources = {
  target: target('dash_ground'), hero: OTHER,
  clips: [
    { clip: theirs('dash_ground'), label: 'dash_ground', name: 'Dash ground', group: 'movement', seconds: 1.5333, frames: 47, rank: 0, usable: true },
    { clip: theirs('dash_air_forward'), label: 'dash_air_forward', name: 'Dash air forward', group: 'movement', seconds: 1.6667, frames: 51, rank: 1, usable: true },
    { clip: theirs('stand_idle'), label: 'stand_idle', name: 'Stand idle', group: 'movement', seconds: 2, frames: 61, rank: 2, usable: true },
    { clip: theirs('loco_lean_forward'), label: 'loco_lean_forward', name: 'Lean forward', group: 'movement', seconds: 1, frames: 29, rank: 3, usable: false, reason: TAKEN_SOURCE_ADDITIVE },
  ],
  set: { name: 'dash', members: [
    { label: 'dash_ground', target: target('dash_ground'), clip: theirs('dash_ground'), usable: true },
    { label: 'dash_air_forward', target: target('dash_air_forward'), clip: theirs('dash_air_forward'), usable: true },
    { label: 'dash_air_back', clip: theirs('dash_air_back'), usable: false },
  ] },
};
const MADE: MadeReport = {
  action: 'Dash ground', mode: 'as_it_looks', switches: null, clip_frames: 32, clip_seconds: 1.0333, action_seconds: 1.5333, plays_from: 0, plays_to: 1.5333, stretch: 1.484,
  follow: [], carried: ['arm_L_IKTARGET'], between: [], kept_count: 40, kept_kinds: { fingers: 30, cloth: 6, other: 4 }, kept_examples: {}, not_in_clip: [],
  still_made_moving: [], still_new_fixed_value: [], still_held: [], still_kept_game_value: [], ranges_refitted: [], ranges_clamped: [], words_per_frame: [70, 74],
  loop_gap_degrees: 0, loop_gap_inches: 0, loop_blend_seconds: null, drift_removed_inches: null, feet_lowest_inches: null, feet_highest_inches: null,
};
const REPORT: TakenReport = { made: MADE, source_name: 'Other', driven: ['pelvis', 'spine_0', 'neck_0', 'head', 'leg_upper_L'], own: [], not_matched: [], speed_percent: 48.4, size: 1 };
const takenRow = (taken: Record<string, unknown> = {}, label = 'dash_ground'): ResourceReplacement =>
  ({ enabled: true, target: target(label), allow_shared: false, taken: { hero: OTHER, clip: theirs(label), timing: 'stretched', arms: 'source', ...taken } as never });
function packet(name: string, which = 'game', frames = 4) {
  const header = new TextEncoder().encode(JSON.stringify({ bones: [{ name: 'root', parent: -1, position: [0, 0, 0] }], clips: [{ name, sources: [name], fps: 30, frames, segments: [frames], offset: 0, duration: (frames - 1) / 30, clip_frames: frames, additive: false, blend_may_differ: false, cut: false, which, marks: [] }] }));
  const start = Math.ceil((4 + header.length) / 4) * 4, out = new ArrayBuffer(start + frames * 48);
  new DataView(out).setUint32(0, header.length, true); new Uint8Array(out, 4, header.length).set(header);
  return out;
}
const scene = { metadata: { bounds: { min: [0, 0, 0], max: [1, 1, 2] }, meshes: [], bones: [] } } as unknown as PreviewScene;

/** The page with a history like the app's, and an Undo button that goes one step back. */
const events: string[] = [];
let history = new ProjectHistory();
function Page({ initial, preview = null }: { initial: Project; preview?: PreviewScene | null }) {
  const [project, setProject] = useState(initial);
  const ref = useRef(project);
  const controls: HistoryControls = { canUndo: false, canRedo: false, undo: () => undefined, redo: () => undefined, gesture: on => { events.push(`gesture:${on}`); history.gesture(on); }, fence: () => history.fence() };
  const update = (next: Project) => { events.push('write'); history.record(ref.current, next); ref.current = next; setProject(next); };
  const undo = () => { const step = history.nextUndo(); if (!step) return; history.undone(); ref.current = step.before; setProject(step.before); };
  const run = async <T,>(label: string, operation: () => Promise<T>) => { events.push(`operate:${label}`); return operation(); };
  return <HistoryContext.Provider value={controls}>
    <MotionPage project={project} update={update} disabled={false} fail={vi.fn()} preview={preview} load={vi.fn()} next={vi.fn()} operate={run}/>
    <button type="button" onClick={undo}>Test undo</button>
    <output data-testid="saved">{JSON.stringify(project)}</output>
  </HistoryContext.Provider>;
}
const saved = () => JSON.parse(screen.getByTestId('saved').textContent!) as Project;
const list = () => screen.getByRole('region', { name: 'Animations' });
const rowButton = (name: RegExp) => within(list()).getByRole('button', { name });
const card = (name = 'Dash ground') => screen.getByRole('region', { name: `Clip ${name}` });
const routeButton = (name: string) => within(card()).getByRole('button', { name });
const picker = (name: 'Hero' | 'Animation') => within(card()).getByRole('button', { name });
async function open(ui: ReactElement) {
  const view = render(ui);
  await waitFor(() => expect(list().querySelector('.clip-row')).not.toBeNull());
  return view;
}
/** Third way, Other chosen as the hero (nothing written). */
async function pickOther() {
  fireEvent.click(routeButton('Another hero’s animation'));
  fireEvent.click(await within(card()).findByRole('button', { name: 'Hero' }));
  fireEvent.click(await within(card()).findByRole('option', { name: 'Other' }));
  await waitFor(() => expect(picker('Animation')).not.toBeDisabled());
}
async function chooseAnimation(name: string) {
  fireEvent.click(picker('Animation'));
  fireEvent.click(within(card()).getByRole('option', { name }));
}

beforeEach(() => {
  vi.clearAllMocks(); forgetClipPackets(); forgetMadeState(); forgetTakenState(); probe.mounts = 0; probe.unmounts = 0; events.length = 0; history = new ProjectHistory();
  native.motionCatalog.mockImplementation(async () => ({ hero_path: HERO, animations: ENTRIES }));
  native.clipFit.mockResolvedValue({ kind: 'compiled', fits: true, checks: [] });
  native.clipPreview.mockImplementation(async (_p: Project, path: string) => packet(path.split('/').pop()!.replace('.vnmclip_c', '')));
  native.madeClipCheck.mockImplementation(async (_p: Project, t: string) => ({ target: t, refusal: null, fit_error: null }));
  native.takenClipCheck.mockImplementation(async (_p: Project, t: string) => ({ target: t, refusal: null }));
  native.takenClipHeroes.mockResolvedValue(HEROES);
  native.takenClipSources.mockResolvedValue(SOURCES);
  native.takenClipReport.mockResolvedValue(REPORT);
  native.takenClipPreview.mockImplementation(async () => packet('dash_ground', 'taken'));
});

describe('Another hero’s animation', () => {
  it('asks nothing on page open; opening the third way, the drop-downs and picking a hero write nothing', async () => {
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Dash ground/));
    expect(routeButton('A compiled clip file')).toHaveAttribute('aria-pressed', 'true');
    for (const call of [native.takenClipCheck, native.takenClipHeroes, native.takenClipSources, native.takenClipReport, native.takenClipPreview]) expect(call).not.toHaveBeenCalled();
    const before = screen.getByTestId('saved').textContent;
    fireEvent.click(routeButton('Another hero’s animation'));
    expect(routeButton('Another hero’s animation')).toHaveAttribute('aria-pressed', 'true');
    expect(card()).toHaveTextContent(TAKEN_INTRO('Sample'));
    fireEvent.click(await within(card()).findByRole('button', { name: 'Hero' }));
    expect(native.takenClipCheck).toHaveBeenCalledWith(expect.anything(), target('dash_ground'));
    expect(native.takenClipHeroes).toHaveBeenCalledTimes(1);
    expect(native.takenClipSources).not.toHaveBeenCalled();
    fireEvent.click(await within(card()).findByRole('option', { name: 'Other' }));
    await waitFor(() => expect(native.takenClipSources).toHaveBeenCalledWith(expect.anything(), target('dash_ground'), OTHER));
    await waitFor(() => expect(picker('Animation')).not.toBeDisabled());
    fireEvent.click(picker('Animation'));
    const choices = within(card()).getByRole('listbox', { name: 'Other’s animations' });
    expect(within(choices).getAllByRole('group').map(g => g.getAttribute('aria-label'))).toEqual(['Same move', 'Similar · the same kind of move', 'The same group', 'Cannot be used']);
    expect(within(choices).getByRole('option', { name: 'Dash ground' })).toHaveTextContent('same move');
    expect(within(choices).getByRole('option', { name: 'Dash ground' })).toHaveTextContent('dash_ground · 1.53 s · 47 frames');
    // A search narrows the list.
    fireEvent.change(within(card()).getByRole('searchbox'), { target: { value: 'air' } });
    expect(within(choices).getAllByRole('option').map(o => o.getAttribute('aria-label'))).toEqual(['Dash air forward']);
    fireEvent.keyDown(within(card()).getByRole('searchbox'), { key: 'Escape' });
    expect(within(card()).queryByRole('listbox')).not.toBeInTheDocument();
    expect(screen.getByTestId('saved').textContent).toBe(before);
    expect(events).toEqual([]);
    expect(native.takenClipReport).not.toHaveBeenCalled();
    expect(native.madeClipActions).not.toHaveBeenCalled();
    expect(native.makeClip).not.toHaveBeenCalled();
  });

  it('choosing the animation writes the row in one step, and each later choice is one step', async () => {
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Dash ground/));
    await pickOther();
    await chooseAnimation('Dash ground');
    expect(events).toEqual(['write']);
    expect(saved().customization.animations).toEqual([takenRow()]);
    expect(history.nextUndo()?.before.customization.animations).toEqual([]);
    expect(card()).toHaveTextContent(SAME_MOVE_LINE('Sample'));
    expect(picker('Animation')).toHaveTextContent('Dash ground · 1.53 s · 47 frames');
    const gets = await within(card()).findByRole('list', { name: 'What you get' });
    expect(native.takenClipReport).toHaveBeenCalledWith(expect.anything(), target('dash_ground'), takenRow());
    expect(gets).toHaveTextContent('The body’s motion comes from Other: 5 bones (pelvis, spine 0, neck 0, head, …)');
    expect(gets).toHaveTextContent('How far and how fast the move goes stays Sample’s');
    expect(gets).toHaveTextContent('Its timing, sounds and effects stay Sample’s (1.03 s, events at the same times)');
    expect(gets).toHaveTextContent('Played 1.5 times faster to fit: Other’s 1.53 s in Sample’s 1.03 s');
    expect(gets).toHaveTextContent('40 bones keep Sample’s own motion (fingers, cloth, other)');
    expect(card()).toHaveTextContent('Other’s whole 1.53 s clip plays in Sample’s 1.03 s, 1.5 times faster.');
    expect(card()).toHaveTextContent('Other’s arms and hands pose as if holding their own weapon; keep Sample’s own for their grip.');
    expect(rowButton(/Dash ground/)).toHaveTextContent('Replaced');
    fireEvent.click(within(card()).getByRole('button', { name: 'Keep Sample’s own' }));
    expect(saved().customization.animations?.[0].taken).toEqual({ hero: OTHER, clip: theirs('dash_ground'), timing: 'stretched', arms: 'own' });
    fireEvent.click(within(card()).getByRole('button', { name: 'Play a part at its own speed' }));
    expect(saved().customization.animations?.[0].taken).toEqual({ hero: OTHER, clip: theirs('dash_ground'), timing: 'own_speed', arms: 'own' });
    const start = within(card()).getByRole('spinbutton', { name: 'Where the part starts (seconds)' });
    fireEvent.change(start, { target: { value: '0.3' } });
    expect(events).toEqual(['write', 'write', 'write']);
    fireEvent.blur(start);
    expect(saved().customization.animations?.[0].taken).toEqual({ hero: OTHER, clip: theirs('dash_ground'), timing: 'own_speed', start_seconds: 0.3, arms: 'own' });
    expect(events).toEqual(['write', 'write', 'write', 'write']);
    // The same choice again writes nothing.
    fireEvent.click(within(card()).getByRole('button', { name: 'Keep Sample’s own' }));
    expect(events).toHaveLength(4);
    // A start past the end of the other clip is held at the latest the report allows (1.53 − 1.03).
    const field = within(card()).getByRole('spinbutton', { name: 'Where the part starts (seconds)' });
    fireEvent.change(field, { target: { value: '9' } });
    fireEvent.keyDown(field, { key: 'Enter' });
    expect(saved().customization.animations?.[0].taken?.start_seconds).toBe(0.5);
    // Back to the whole clip: the start goes.
    fireEvent.click(within(card()).getByRole('button', { name: 'Fit it to this animation’s length' }));
    expect(saved().customization.animations?.[0].taken).toEqual({ hero: OTHER, clip: theirs('dash_ground'), timing: 'stretched', arms: 'own' });
    expect(native.madeClipCheck).not.toHaveBeenCalled();
    expect(events.filter(e => e.startsWith('operate'))).toEqual([]);
  });

  it('greys out heroes and clips that cannot be used, with the engine’s reason, and they cannot be chosen', async () => {
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Dash ground/));
    fireEvent.click(routeButton('Another hero’s animation'));
    fireEvent.click(await within(card()).findByRole('button', { name: 'Hero' }));
    const blocky = await within(card()).findByRole('option', { name: 'Blocky' });
    expect(blocky).toHaveAttribute('aria-disabled', 'true');
    expect(blocky).toHaveTextContent(`${TAKEN_BODY_NOT_CARRIED} (its body bones are named differently).`);
    fireEvent.click(blocky);
    fireEvent.keyDown(blocky, { key: 'Enter' });
    expect(native.takenClipSources).not.toHaveBeenCalled();
    expect(picker('Hero')).toHaveTextContent('Choose a hero…');
    fireEvent.click(within(card()).getByRole('option', { name: 'Other' }));
    await waitFor(() => expect(picker('Animation')).not.toBeDisabled());
    fireEvent.click(picker('Animation'));
    const lean = within(card()).getByRole('option', { name: 'Lean forward' });
    expect(lean).toHaveAttribute('aria-disabled', 'true');
    expect(lean).toHaveTextContent(TAKEN_SOURCE_ADDITIVE);
    fireEvent.click(lean);
    expect(events).toEqual([]);
    expect(saved().customization.animations).toEqual([]);
  });

  it('says why a clip cannot be taken at all and asks for no list', async () => {
    native.takenClipCheck.mockImplementation(async (_p: Project, t: string) => ({ target: t, refusal: TAKEN_TARGET_ADDITIVE }));
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Lean forward/));
    fireEvent.click(within(card('Lean forward')).getByRole('button', { name: 'Another hero’s animation' }));
    const note = await within(card('Lean forward')).findByRole('note');
    expect(note).toHaveTextContent(TAKEN_TARGET_ADDITIVE);
    expect(note.querySelector('strong')).toHaveTextContent('This clip cannot be taken from another hero.');
    expect(native.takenClipHeroes).not.toHaveBeenCalled();
    fireEvent.click(within(card('Lean forward')).getByRole('button', { name: 'Use a compiled clip file instead' }));
    expect(within(card('Lean forward')).getByRole('button', { name: 'A compiled clip file' })).toHaveAttribute('aria-pressed', 'true');
    expect(events).toEqual([]);
  });

  it('Take all writes every row of the set in ONE step, and one undo removes them all', async () => {
    const kept = { enabled: true, source: 'C:\\clips\\stand.vnmclip_c', target: target('stand_idle'), allow_shared: false };
    await open(<Page initial={withRows([kept])}/>);
    fireEvent.click(rowButton(/Dash ground/));
    await pickOther();
    const set = within(card()).getByText(/Dash is 3 animations/).closest('.taken-set') as HTMLElement;
    expect(set).toHaveTextContent('Dash is 3 animations (Dash ground, Dash air forward, dash_air_back). 2 of them can be taken from Other.');
    fireEvent.click(within(set).getByRole('button', { name: 'Take all 2 from Other' }));
    expect(events).toEqual(['write']);
    expect(saved().customization.animations).toEqual([kept, takenRow(), takenRow({}, 'dash_air_forward')]);
    // The card now shows this clip's saved row with its options and report, not a pending hero.
    expect(await within(card()).findByRole('list', { name: 'What you get' })).toBeInTheDocument();
    expect(within(card()).getByRole('button', { name: 'Keep Sample’s own' })).toBeInTheDocument();
    expect(card()).not.toHaveTextContent('stays in use until you choose an animation of');
    // Taking them all again would change nothing, so it is not offered.
    const after = within(card()).getByText(/Dash is 3 animations/).closest('.taken-set') as HTMLElement;
    expect(within(after).queryByRole('button', { name: /^Take all/ })).toBeNull();
    expect(after).toHaveTextContent('All 2 are taken from Other.');
    fireEvent.click(screen.getByRole('button', { name: 'Test undo' }));
    expect(saved().customization.animations).toEqual([kept]);
  });

  it('offers Take all again as soon as one animation of the set differs from what it would write', async () => {
    const both = [takenRow({ arms: 'own' }), takenRow({ timing: 'own_speed' }, 'dash_air_forward')];
    const setOf = () => within(card()).getByText(/Dash is 3 animations/).closest('.taken-set') as HTMLElement;
    // Each row keeps its own choices under Take all, so rows that are already taken from Other stay as they are.
    const view = await open(<Page initial={withRows(both)}/>);
    fireEvent.click(rowButton(/Dash ground/));
    await waitFor(() => expect(setOf()).toHaveTextContent('All 2 are taken from Other.'));
    expect(within(setOf()).queryByRole('button', { name: /^Take all/ })).toBeNull();
    view.unmount(); forgetTakenState();
    // One of them switched off: the click would switch it on, so it is offered.
    await open(<Page initial={withRows([both[0], { ...both[1], enabled: false }])}/>);
    fireEvent.click(rowButton(/Dash ground/));
    const take = await within(card()).findByRole('button', { name: 'Take all 2 from Other' });
    expect(setOf()).not.toHaveTextContent('All 2 are taken');
    fireEvent.click(take);
    expect(saved().customization.animations).toEqual(both);
    expect(setOf()).toHaveTextContent('All 2 are taken from Other.');
  });

  it('a row has one way only: a compiled file or a made clip removes `taken`, and taking removes the file', async () => {
    native.validateAssetPath.mockImplementation(async (path: string) => path);
    await open(<Page initial={withRows([takenRow()])}/>);
    expect(routeButton('Another hero’s animation')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText('1 clip replaced')).toBeInTheDocument();
    fireEvent.click(routeButton('A compiled clip file'));
    expect(card()).toHaveTextContent('This clip is taken from another hero’s animation now. A compiled clip file chosen here replaces that.');
    expect(native.clipFit).not.toHaveBeenCalled();
    const drop = card().querySelector('[data-native-drop="animation"]')!;
    await act(async () => { drop.dispatchEvent(new CustomEvent('dmb-file-drop', { detail: ['C:\\clips\\dash.vnmclip_c'] })); });
    await waitFor(() => expect(saved().customization.animations).toEqual([{ enabled: true, target: target('dash_ground'), allow_shared: false, source: 'C:\\clips\\dash.vnmclip_c' }]));
    // And back: choosing another hero's animation leaves no file.
    fireEvent.click(routeButton('Another hero’s animation'));
    expect(await within(card()).findByText('Your compiled clip file stays in use until you choose an animation here.')).toBeInTheDocument();
    fireEvent.click(await within(card()).findByRole('button', { name: 'Hero' }));
    fireEvent.click(await within(card()).findByRole('option', { name: 'Other' }));
    await waitFor(() => expect(picker('Animation')).not.toBeDisabled());
    await chooseAnimation('Dash air forward');
    expect(saved().customization.animations).toEqual([{ enabled: true, target: target('dash_ground'), allow_shared: false, taken: { hero: OTHER, clip: theirs('dash_air_forward'), timing: 'stretched', arms: 'source' } }]);
    // Make it from my animation: the made row has no `taken`.
    native.madeClipActions.mockResolvedValue({ source: 'C:\\models\\my_character.blend', rig: 'Armature', bones: 40, driving_bones: 40, skeleton_error: null, actions: [{ name: 'Wave', frame_start: 0, frame_end: 30, fps: 30, length_seconds: 1, keyed_bones: [], driving_keyed_bones: 40 }] });
    native.makeClip.mockImplementation(async (_p: Project, t: string) => ({ target: t, made: {}, report: MADE }));
    fireEvent.click(routeButton('Make it from my animation'));
    expect(await within(card()).findByText('Your clip taken from another hero stays in use until you choose an animation here.')).toBeInTheDocument();
    fireEvent.click(within(card()).getByRole('button', { name: 'Use my model’s own file' }));
    fireEvent.change(await within(card()).findByRole('combobox', { name: 'Animation' }), { target: { value: 'Wave' } });
    await waitFor(() => expect(saved().customization.animations?.[0].made).toBeTruthy());
    expect(saved().customization.animations?.[0].taken).toBeUndefined();
  });

  it('treats a taken row as replaced in the list, the counts and the Extras status, and Remove takes it out', async () => {
    await open(<Page initial={withRows([takenRow()])}/>);
    expect(screen.queryByText('Other saved rows')).not.toBeInTheDocument();
    fireEvent.click(within(list()).getByRole('button', { name: /^Replaced/ }));
    expect(within(list()).getAllByRole('button', { name: /Dash ground/ })).toHaveLength(1);
    // Build & install and the undo toast count and name a taken row like any replaced clip.
    expect(includedExtras(saved(), true)).toBe('1 animation');
    expect(projectReadiness(saved(), null, false).rows.find(row => row.key === 'motion')?.value).toBe('1 animation replacement enabled');
    expect(changesSince(withRows([]), saved())).toEqual(['Animations']);
    expect(replacementStatus({ row: takenRow(), entry: { supported: true, shared: false }, missing: false } as never, true)).toBe('Replaced');
    await within(card()).findByRole('list', { name: 'What you get' });
    fireEvent.click(within(card()).getByLabelText('Include in build'));
    expect(saved().customization.animations?.[0]).toEqual({ ...takenRow(), enabled: false });
    // Include in build does not ask for the report again: it depends on `taken` only.
    expect(native.takenClipReport).toHaveBeenCalledTimes(1);
    fireEvent.click(within(card()).getByRole('button', { name: 'Remove' }));
    expect(saved().customization.animations).toEqual([]);
  });

  it('asks for the report one request at a time and shows only the newest answer', async () => {
    const answers: ((value: TakenReport) => void)[] = [];
    native.takenClipReport.mockImplementation(() => new Promise<TakenReport>(resolve => { answers.push(resolve); }));
    await open(<Page initial={withRows([takenRow()])}/>);
    await waitFor(() => expect(native.takenClipReport).toHaveBeenCalledTimes(1));
    expect(card()).toHaveTextContent('Reading what the app makes…');
    fireEvent.click(within(card()).getByRole('button', { name: 'Keep Sample’s own' }));
    fireEvent.click(within(card()).getByRole('button', { name: 'Play a part at its own speed' }));
    expect(native.takenClipReport).toHaveBeenCalledTimes(1);
    await act(async () => { answers[0]({ ...REPORT, driven: ['old'] }); });
    await waitFor(() => expect(native.takenClipReport).toHaveBeenCalledTimes(2));
    expect(native.takenClipReport.mock.calls[1][2].taken).toEqual({ hero: OTHER, clip: theirs('dash_ground'), timing: 'own_speed', arms: 'own' });
    expect(within(card()).queryByRole('list', { name: 'What you get' })).not.toBeInTheDocument();
    await act(async () => { answers[1]({ ...REPORT, driven: ['head'] }); });
    expect(within(card()).getByRole('list', { name: 'What you get' })).toHaveTextContent('The body’s motion comes from Other: 1 bone (head)');
  });

  it('drops a superseded answer quietly', async () => {
    native.takenClipReport.mockRejectedValueOnce({ message: TAKEN_SUPERSEDED });
    await open(<Page initial={withRows([takenRow()])}/>);
    await waitFor(() => expect(native.takenClipReport).toHaveBeenCalledTimes(1));
    expect(card()).not.toHaveTextContent(TAKEN_SUPERSEDED);
  });

  it('plays the taken clip as Yours, one request at a time, keeping the one 3D view across the ways, an option and another clip', async () => {
    const packets: ((value: ArrayBuffer) => void)[] = [];
    await open(<Page initial={withRows([takenRow()])} preview={scene}/>);
    const player = screen.getByRole('region', { name: 'Clip player' });
    expect(await screen.findByTestId('clip-view')).toHaveTextContent('dash_ground:game');
    native.takenClipPreview.mockImplementation(() => new Promise<ArrayBuffer>(resolve => { packets.push(resolve); }));
    fireEvent.click(within(player).getByRole('button', { name: 'Yours' }));
    await waitFor(() => expect(native.takenClipPreview).toHaveBeenCalledTimes(1));
    expect(native.takenClipPreview).toHaveBeenCalledWith(expect.anything(), target('dash_ground'), takenRow());
    // Two options while the first packet is made: one runs, only the newest waits.
    fireEvent.click(within(card()).getByRole('button', { name: 'Keep Sample’s own' }));
    fireEvent.click(within(card()).getByRole('button', { name: 'Play a part at its own speed' }));
    expect(native.takenClipPreview).toHaveBeenCalledTimes(1);
    await act(async () => { packets[0](packet('dash_ground', 'taken')); });
    await waitFor(() => expect(native.takenClipPreview).toHaveBeenCalledTimes(2));
    expect(native.takenClipPreview.mock.calls[1][2].taken).toEqual({ hero: OTHER, clip: theirs('dash_ground'), timing: 'own_speed', arms: 'own' });
    await act(async () => { packets[1](packet('dash_ground', 'taken')); });
    await waitFor(() => expect(screen.getByTestId('clip-view')).toHaveTextContent('dash_ground:taken'));
    expect(player).toHaveTextContent('Other’s move played on your prepared character: the clip the build writes.');
    expect(native.clipPreview).not.toHaveBeenCalledWith(expect.anything(), expect.anything(), 'yours', expect.anything());
    native.takenClipPreview.mockImplementation(async () => packet('dash_ground', 'taken'));
    fireEvent.click(within(player).getByRole('button', { name: 'Both' }));
    await waitFor(() => expect(player).toHaveTextContent('Both: Other’s move on your character in full, Sample’s own clip as a see-through figure, in step.'));
    fireEvent.click(routeButton('A compiled clip file'));
    fireEvent.click(routeButton('Make it from my animation'));
    fireEvent.click(routeButton('Another hero’s animation'));
    fireEvent.click(rowButton(/Stand idle/));
    await waitFor(() => expect(screen.getByTestId('clip-view')).toHaveTextContent('stand_idle:game'));
    expect(screen.getByRole('region', { name: 'Clip player' })).toBe(player);
    expect(probe.mounts).toBe(1);
    expect(probe.unmounts).toBe(0);
  });

  it('a late list answer for a hero no longer picked does not replace the saved hero’s list', async () => {
    const third: SourceHero = { hero: 'models/heroes_wip/third/third.vmdl', name: 'Third', usable: true };
    native.takenClipHeroes.mockResolvedValue([...HEROES, third]);
    let late: ((value: TakenSources) => void) | null = null;
    native.takenClipSources.mockImplementation((_p: Project, _t: string, hero: string) => hero === third.hero ? new Promise<TakenSources>(resolve => { late = resolve; }) : Promise.resolve(SOURCES));
    await open(<Page initial={withRows([takenRow()])}/>);
    await waitFor(() => expect(picker('Animation')).toHaveTextContent('Dash ground · 1.53 s · 47 frames'));
    fireEvent.click(picker('Hero'));
    fireEvent.click(within(card()).getByRole('option', { name: 'Third' }));
    await waitFor(() => expect(late).not.toBeNull());
    fireEvent.click(within(card()).getByRole('button', { name: 'Keep the saved hero' }));
    await act(async () => { late!({ ...SOURCES, hero: third.hero, clips: [] }); });
    expect(picker('Animation')).toHaveTextContent('Dash ground · 1.53 s · 47 frames');
    expect(picker('Animation')).not.toBeDisabled();
  });

  it('a saved row keeps its hero until an animation of a newly picked hero is chosen', async () => {
    const third: SourceHero = { hero: 'models/heroes_wip/third/third.vmdl', name: 'Third', usable: true };
    native.takenClipHeroes.mockResolvedValue([...HEROES, third]);
    await open(<Page initial={withRows([takenRow()])}/>);
    await within(card()).findByRole('list', { name: 'What you get' });
    fireEvent.click(picker('Hero'));
    fireEvent.click(within(card()).getByRole('option', { name: 'Third' }));
    await waitFor(() => expect(native.takenClipSources).toHaveBeenLastCalledWith(expect.anything(), target('dash_ground'), third.hero));
    expect(card()).toHaveTextContent('Your saved clip from Other stays in use until you choose an animation of Third.');
    expect(events).toEqual([]);
    expect(saved().customization.animations).toEqual([takenRow()]);
    fireEvent.click(within(card()).getByRole('button', { name: 'Keep the saved hero' }));
    expect(picker('Hero')).toHaveTextContent('Other');
    expect(events).toEqual([]);
  });
});
