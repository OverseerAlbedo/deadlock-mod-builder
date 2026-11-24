import { useEffect, useState, type ReactElement } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Project, ResourceReplacement } from '../models';
import type { AnimationEntry, ComponentCatalog } from '../components/AssetCatalog';
import type { PreviewScene } from '../preview/packet';
import type { FitReport } from './motionTypes';
import { MotionPage } from './MotionPage';
import { forgetClipPackets } from './ClipPlayer';
import { AUTHORED_UNTRIED, authoredSource } from './ClipCard';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

const native = vi.hoisted(() => ({ motionCatalog: vi.fn(), clipFit: vi.fn(), clipPreview: vi.fn(), validateAssetPath: vi.fn() }));
vi.mock('../api', async original => ({ ...await original<typeof import('../api')>(), api: native }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));
const probe = vi.hoisted(() => ({ mounts: 0, unmounts: 0, mains: [] as string[] }));
// The 3D view, as a probe that counts its mounts and says which clip it shows.
vi.mock('./ClipViewport', () => ({ default: function Probe({ main }: { main: { header: { clips: { name: string }[] } } }) {
  useEffect(() => { probe.mounts++; return () => { probe.unmounts++; }; }, []);
  probe.mains.push(main.header.clips[0].name);
  return <div data-testid="clip-view">{main.header.clips[0].name}</div>;
} }));

const HERO = 'models/heroes_wip/sample/sample.vmdl_c';
const clip = (label: string, group: AnimationEntry['group'], extra: Partial<AnimationEntry> = {}): AnimationEntry => ({
  path: `models/heroes_wip/sample/clips/${label}.vnmclip_c`, target: `models/heroes_wip/sample/clips/${label}.vnmclip_c`, label, supported: true,
  skeleton: 'models/heroes_wip/sample/sample.vnmskel', frames: 25, duration: .8, tracks: 72, events: 2, additive: false,
  name: label.replace(/_/g, ' ').replace(/^./, c => c.toUpperCase()), group, graph_named: true, ...extra,
});
const ENTRIES = [
  clip('run_n', 'movement', { name: 'Run, forward' }), clip('run_s', 'movement', { name: 'Run, backward' }), clip('stand_idle', 'movement'),
  clip('jump_start', 'jumping'), clip('reload_idle', 'weapon'), clip('aim_idle', 'weapon', { additive: true }),
  clip('aim_idle.vnmclip+non_additive', 'weapon', { name: 'Aim idle (full-pose copy)', derived_from: 'models/heroes_wip/sample/clips/aim_idle.vnmclip_c' }),
  clip('flinch_back', 'hits'), clip('bindpose', 'other', { name: 'Bind pose', graph_named: false }), clip('walk_old', 'movement', { name: 'Walk old', graph_named: false }),
];
const catalog: ComponentCatalog = { hero_path: HERO, icons: [], animations: ENTRIES, effects: [], audio: [], variants: [], animation_unavailable: null, warnings: [], includes_animations: true };
const target = (label: string) => `models/heroes_wip/sample/clips/${label}.vnmclip_c`;
const base = { ...reference.cases[1].expected, original_model: HERO, hero_name: 'Sample', dmx: 'C:\\work\\prepared.dmx', animation_policy: 'preserve', ag2_mode: 'auto' } as unknown as Project;
const withRows = (rows: ResourceReplacement[], enabled = true) => ({ ...base, customization: { ...base.customization, enabled, animations: rows } }) as Project;
const fits = (overrides: Partial<FitReport> = {}): FitReport => ({ kind: 'compiled', fits: true, checks: [
  { id: 'skeleton', label: 'Same skeleton and bone tracks (72)', status: 'pass' }, { id: 'length', label: 'Same length: 0.80 s, 25 frames', status: 'pass' },
  { id: 'events', label: 'Same events, root motion and timing marks', status: 'pass' }, { id: 'unchanged', label: 'Nothing else in the file is changed', status: 'pass' }], ...overrides });
/** A clip packet in the movement preview's binary form, one bone, `frames` frames. */
function packet(name: string, frames = 25, extra: Record<string, unknown> = {}) {
  const header = new TextEncoder().encode(JSON.stringify({ bones: [{ name: 'root', parent: -1, position: [0, 0, 0] }], clips: [{ name, sources: [name], fps: 30, frames, segments: [frames], offset: 0, duration: (frames - 1) / 30, clip_frames: frames, additive: false, blend_may_differ: false, cut: false, which: 'game', marks: [{ at: .25, kind: 'sound', word: 'Footstep' }], ...extra }] }));
  const start = Math.ceil((4 + header.length) / 4) * 4, out = new ArrayBuffer(start + frames * 48);
  new DataView(out).setUint32(0, header.length, true); new Uint8Array(out, 4, header.length).set(header);
  return out;
}
const scene = { metadata: { bounds: { min: [0, 0, 0], max: [1, 1, 2] }, meshes: [], bones: [] } } as unknown as PreviewScene;

function Page({ initial, disabled = false, preview = null }: { initial: Project; disabled?: boolean; preview?: PreviewScene | null }) {
  const [project, update] = useState(initial);
  return <><MotionPage project={project} update={update} disabled={disabled} fail={vi.fn()} preview={preview} load={vi.fn()} next={vi.fn()}/>
    <output data-testid="saved">{JSON.stringify(project)}</output></>;
}
const saved = () => JSON.parse(screen.getByTestId('saved').textContent!) as Project;
const list = () => screen.getByRole('region', { name: 'Animations' });
const rowButton = (name: RegExp) => within(list()).getByRole('button', { name });
/** Renders the page and waits for its own list read. */
async function open(ui: ReactElement) {
  const view = render(ui);
  await waitFor(() => expect(list().querySelector('.clip-row')).not.toBeNull());
  return view;
}

beforeEach(() => {
  vi.clearAllMocks(); forgetClipPackets(); probe.mounts = 0; probe.unmounts = 0; probe.mains = [];
  native.validateAssetPath.mockImplementation(async (path: string) => path);
  native.motionCatalog.mockImplementation(async () => ({ hero_path: catalog.hero_path, animations: catalog.animations }));
  native.clipFit.mockResolvedValue(fits());
  native.clipPreview.mockImplementation(async (_p: Project, path: string) => packet(path.split('/').pop()!.replace('.vnmclip_c', '')));
});

describe('Motion page list', () => {
  it('groups the hero’s clips with counts, searches every group and filters the replaced ones', async () => {
    await open(<Page initial={withRows([{ enabled: true, source: 'C:\\clips\\mine.vnmclip_c', target: target('reload_idle'), allow_shared: false }])}/>);
    const chips = within(list()).getByRole('group', { name: 'Groups' });
    // The page opens on the replaced clip, in its group.
    await waitFor(() => expect(within(chips).getByRole('button', { name: /Weapon 2/ })).toHaveAttribute('aria-pressed', 'true'));
    expect(rowButton(/Reload idle/)).toHaveAttribute('aria-pressed', 'true');
    expect(within(list()).getByRole('button', { name: 'All 7' })).toBeInTheDocument();
    fireEvent.click(within(chips).getByRole('button', { name: /Movement 3/ }));
    expect(rowButton(/Run, forward/)).toHaveTextContent('run_n · 0.80 s · 25 frames');
    const label = rowButton(/Run, forward/).querySelector('.clip-row-label')!;
    expect(label).toHaveAttribute('title', 'run_n');
    expect(label.nextElementSibling).toHaveTextContent('· 0.80 s · 25 frames');
    // A derived copy is no row of its own.
    fireEvent.click(within(chips).getByRole('button', { name: /Weapon/ }));
    expect(within(list()).queryByRole('button', { name: /full-pose copy/ })).not.toBeInTheDocument();
    expect(rowButton(/Reload idle/)).toHaveTextContent('Replaced');
    fireEvent.change(within(list()).getByRole('searchbox', { name: 'Search animations' }), { target: { value: 'run' } });
    expect(within(list()).getAllByRole('heading', { level: 4 }).map(h => h.textContent)).toEqual(['Movement · 2']);
    fireEvent.change(within(list()).getByRole('searchbox', { name: 'Search animations' }), { target: { value: '' } });
    fireEvent.click(within(list()).getByRole('button', { name: 'Replaced 1' }));
    expect(within(list()).getAllByRole('button', { name: /frames/ }).map(b => b.textContent)).toEqual([expect.stringContaining('Reload idle')]);
  });
  it('brings a row the page chose into view inside its own list, never the page, and keeps the focus', async () => {
    const spies = [
      vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) { return { top: this.classList.contains('clip-row') ? 400 : 0 } as DOMRect; }),
      vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(100),
      vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(40),
    ];
    const page = vi.spyOn(window, 'scrollTo').mockImplementation(() => undefined);
    try {
      await open(<Page initial={withRows([{ enabled: true, source: 'C:\\clips\\mine.vnmclip_c', target: target('reload_idle'), allow_shared: false }])}/>);
      const box = () => rowButton(/Reload idle/).closest('.clip-rows') as HTMLElement;
      await waitFor(() => expect(box().scrollTop).toBe(340));
      box().scrollTop = 0;
      fireEvent.click(rowButton(/Aim idle/));
      expect(box().scrollTop).toBe(0);
      const search = within(list()).getByRole('searchbox', { name: 'Search animations' });
      search.focus();
      fireEvent.change(search, { target: { value: 'idle' } });
      await waitFor(() => expect((rowButton(/Aim idle/).closest('.clip-rows') as HTMLElement).scrollTop).toBe(340));
      expect(document.activeElement).toBe(search);
      expect(page).not.toHaveBeenCalled();
    } finally { spies.forEach(s => s.mockRestore()); page.mockRestore(); }
  });
  it('folds the clips no animation graph names, and always shows a clip with a saved row', async () => {
    await open(<Page initial={withRows([{ enabled: true, source: 'C:\\clips\\walk.vnmclip_c', target: target('walk_old'), allow_shared: false }])}/>);
    expect(rowButton(/Walk old/)).toBeInTheDocument();
    fireEvent.click(within(list()).getByRole('button', { name: /Emotes and other/ }));
    const folded = within(list()).getByText(/No animation graph of this hero names these/);
    expect(folded.closest('details')).not.toHaveAttribute('open');
    expect(folded.textContent).toBe('1 more · No animation graph of this hero names these');
    expect(screen.queryByText(/unused/i)).not.toBeInTheDocument();
  });
  it('names the derived full-pose copy on its parent’s card, which stays the game’s', async () => {
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(within(list()).getByRole('button', { name: /Weapon/ }));
    fireEvent.click(rowButton(/Aim idle/));
    expect(screen.getByRole('region', { name: 'Clip Aim idle' })).toHaveTextContent('full-pose copy of this clip that its compiler derived from it; that copy stays the game’s');
    expect(screen.getByRole('region', { name: 'Clip Aim idle' })).toHaveTextContent('An additive layer');
    expect(screen.queryByText(/Used by/)).not.toBeInTheDocument();
  });
});

describe('Motion page opening', () => {
  it('reads only its own clip list on open, once per project and hero, also while the app is busy', async () => {
    native.motionCatalog.mockReturnValue(new Promise(() => undefined));
    const initial = withRows([{ enabled: true, source: 'C:\clips\mine.vnmclip_c', target: target('run_n') }]);
    const view = render(<Page initial={initial} disabled/>);
    await waitFor(() => expect(native.motionCatalog).toHaveBeenCalledTimes(1));
    expect(native.motionCatalog).toHaveBeenCalledWith(expect.objectContaining({ original_model: HERO }), false);
    view.rerender(<Page initial={initial}/>);
    expect(native.motionCatalog).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Loading the hero’s animations…')).toBeInTheDocument();
    expect(native.clipFit).not.toHaveBeenCalled();
    expect(native.clipPreview).not.toHaveBeenCalled();
    // Before the list loads, every saved row stays editable.
    expect(screen.getByRole('heading', { name: 'Other saved rows' })).toBeInTheDocument();
    expect(screen.getByLabelText('Include animation 1')).toBeChecked();
  });
  it('says why the list could not be read and reads it again on Try again', async () => {
    native.motionCatalog.mockRejectedValueOnce({ message: 'Choose the current Deadlock pak01_dir.vpk in Setup' });
    render(<Page initial={withRows([])}/>);
    expect(await within(list()).findByText(/could not be read: Choose the current Deadlock/)).toBeInTheDocument();
    fireEvent.click(within(list()).getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(list().querySelector('.clip-row')).not.toBeNull());
    expect(native.motionCatalog).toHaveBeenLastCalledWith(expect.anything(), true);
  });
  it('loads no clip until the character is shown, and says what is missing without a prepared model', async () => {
    await open(<Page initial={{ ...withRows([]), dmx: '' }}/>);
    expect(screen.getByText(/Prepare your model first/)).toBeInTheDocument();
    expect(native.clipPreview).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Choose a file…' })).toBeEnabled();
  });
});

describe('Replacing a clip', () => {
  it('writes the same row a picked destination wrote, and Remove takes it away', async () => {
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(rowButton(/Run, backward/));
    const card = screen.getByRole('region', { name: 'Clip Run, backward' });
    expect(card).toHaveTextContent('Original movement, ready to keep.');
    // Replaced compiled clips play in game: no card calls clip replacements experimental.
    expect(card).not.toHaveTextContent(/experimental/i);
    expect(card).not.toHaveTextContent(AUTHORED_UNTRIED);
    const drop = card.querySelector('[data-native-drop="animation"]')!;
    await act(async () => { drop.dispatchEvent(new CustomEvent('dmb-file-drop', { detail: ['C:\\clips\\back.vnmclip_c'] })); });
    await waitFor(() => expect(saved().customization.animations).toEqual([{ enabled: true, source: 'C:\\clips\\back.vnmclip_c', target: target('run_s'), allow_shared: false }]));
    expect(screen.getByRole('region', { name: 'Clip Run, backward' })).not.toHaveTextContent(/experimental/i);
    expect(screen.getByRole('region', { name: 'Clip Run, backward' })).not.toHaveTextContent(AUTHORED_UNTRIED);
    expect(native.validateAssetPath).toHaveBeenCalledWith('C:\\clips\\back.vnmclip_c', 'animation');
    expect(saved().customization.enabled).toBe(true);
    await waitFor(() => expect(native.clipFit).toHaveBeenCalledWith(expect.anything(), target('run_s'), 'C:\\clips\\back.vnmclip_c'));
    fireEvent.click(within(screen.getByRole('region', { name: 'Clip Run, backward' })).getByRole('button', { name: 'Remove' }));
    expect(saved().customization.animations).toEqual([]);
  });
  it('shows each fit state: fits, a failing check with the build’s reason, unreadable and authored', async () => {
    const rows = [{ enabled: true, source: 'C:\\clips\\a.vnmclip_c', target: target('run_n') }, { enabled: true, source: 'C:\\clips\\b.vnmclip_c', target: target('run_s') }, { enabled: true, source: 'C:\\clips\\c.vnmclip_c', target: target('stand_idle') }, { enabled: true, source: 'C:\\clips\\d.vnmclip', target: target('jump_start') }];
    native.clipFit.mockImplementation(async (_p: Project, _t: string, source: string) => source.endsWith('a.vnmclip_c') ? fits()
      : source.endsWith('b.vnmclip_c') ? fits({ fits: false, reason: 'Animation replacement changes protected clip metadata.', checks: fits().checks.map(c => c.id === 'events' ? { ...c, label: 'Events differ from the original (1 of 2)', status: 'fail' as const } : c) })
        : source.endsWith('c.vnmclip_c') ? { kind: 'unreadable', fits: false, reason: 'Animation clip requires exactly one DATA block.', checks: [{ id: 'readable', label: 'The file could not be read as a compiled animation clip', status: 'fail' }] }
          : { kind: 'authored', fits: true, checks: [{ id: 'document', label: 'A .vnmclip that names an animation DMX', status: 'pass' }, { id: 'skeleton', label: 'Same skeleton and bone tracks', status: 'after_compile' }] });
    await open(<Page initial={withRows(rows)}/>);
    const card = (name: string) => screen.getByRole('region', { name: `Clip ${name}` });
    expect(await within(card('Run, forward')).findByText('Fits:')).toBeInTheDocument();
    fireEvent.click(rowButton(/Run, backward/));
    expect(await within(card('Run, backward')).findByText('The build stops on a clip that does not fit.')).toBeInTheDocument();
    expect(card('Run, backward')).toHaveTextContent('Animation replacement changes protected clip metadata.');
    expect(within(card('Run, backward')).getByRole('list', { name: 'Fit checks' })).toHaveTextContent('Events differ from the original (1 of 2)does not fit');
    expect(rowButton(/Run, backward/)).toHaveTextContent('Does not fit');
    expect(screen.getByText(/1 does not fit/)).toBeInTheDocument();
    fireEvent.click(rowButton(/Stand idle/));
    expect(await within(card('Stand idle')).findByText(/exactly one DATA block/)).toBeInTheDocument();
    fireEvent.click(within(list()).getByRole('button', { name: /Jumping and falling/ }));
    fireEvent.click(rowButton(/Jump start/));
    expect(await within(card('Jump start')).findByText('Checked after compiling, at build.')).toBeInTheDocument();
    expect(card('Jump start')).toHaveTextContent('checked at build');
    // Only a clip compiled from the user's own animation keeps a line asking for a check in a match.
    expect(card('Jump start')).toHaveTextContent(AUTHORED_UNTRIED);
    // One check per clip and file.
    fireEvent.click(within(list()).getByRole('button', { name: /Movement/ }));
    fireEvent.click(rowButton(/Run, forward/));
    expect(card('Run, forward')).not.toHaveTextContent(AUTHORED_UNTRIED);
    expect(native.clipFit).toHaveBeenCalledTimes(4);
  });
  it('keeps draft rows, unknown clips and second rows for one clip in Other saved rows', async () => {
    await open(<Page initial={withRows([
      { enabled: true, source: 'C:\\clips\\a.vnmclip_c', target: target('run_n') },
      { enabled: true, source: '', target: '' },
      { enabled: true, source: 'C:\\clips\\x.vnmclip_c', target: 'models/heroes_wip/other/clips/x.vnmclip_c' },
      { enabled: false, source: 'C:\\clips\\b.vnmclip_c', target: target('run_n') },
    ])}/>);
    const others = screen.getByRole('heading', { name: 'Other saved rows' }).closest('section')!;
    expect(within(others).queryByLabelText('Include animation 1')).not.toBeInTheDocument();
    for (const n of [2, 3, 4]) expect(within(others).getByLabelText(`Include animation ${n}`)).toBeInTheDocument();
    expect(within(others).queryByRole('button', { name: 'Add animation' })).not.toBeInTheDocument();
    fireEvent.click(within(others).getByLabelText('Include animation 4'));
    expect(saved().customization.animations?.[3]).toEqual({ enabled: true, source: 'C:\\clips\\b.vnmclip_c', target: target('run_n') });
    expect(saved().customization.animations).toHaveLength(4);
  });
});

describe('Top bar', () => {
  const rows = [{ enabled: true, source: 'C:\\clips\\a.vnmclip_c', target: target('run_n'), original_sha256: 'hash' }, { enabled: true, source: 'C:\\clips\\b.vnmclip_c', target: target('run_s') }];
  it('says what is replaced and switches every replacement off as the old preserve choice did', async () => {
    await open(<Page initial={{ ...withRows(rows), animation_policy: 'disable_legacy' }}/>);
    expect(screen.getByText('2 clips replaced')).toBeInTheDocument();
    expect(screen.getByRole('note')).toHaveTextContent('Advanced policy active: legacy animation references are disabled.');
    fireEvent.click(screen.getByRole('button', { name: 'Switch all replacements off' }));
    expect(saved().animation_policy).toBe('preserve');
    expect(saved().customization.animations).toEqual(rows.map(row => ({ ...row, enabled: false })));
    expect(screen.getByText('Original movement')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Switch all replacements off' })).not.toBeInTheDocument();
  });
  it('removes all replacements only after a confirmation', async () => {
    await open(<Page initial={withRows(rows)}/>);
    fireEvent.click(screen.getByRole('button', { name: 'Remove all replacements…' }));
    const dialog = screen.getByRole('alertdialog', { name: 'Remove all replacements' });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(saved().customization.animations).toHaveLength(2);
    fireEvent.click(screen.getByRole('button', { name: 'Remove all replacements…' }));
    fireEvent.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Remove all' }));
    expect(saved().customization.animations).toEqual([]);
  });
  it('opens Animation graph handling as a menu panel that closes only on its button, Close or Escape', async () => {
    await open(<Page initial={withRows(rows)}/>);
    const button = screen.getByRole('button', { name: 'Animation graph handling' });
    fireEvent.click(button);
    const panel = screen.getByRole('region', { name: 'Animation graph handling' });
    expect(within(panel).getByRole('radiogroup', { name: 'Animation policy' })).toBeInTheDocument();
    expect(within(panel).getByRole('radio', { name: /Preserve original animations/ })).toBeChecked();
    expect(within(panel).getByRole('radio', { name: /Automatic/ })).toBeChecked();
    fireEvent.click(within(panel).getByRole('radio', { name: /Disable legacy animation references/ }));
    expect(saved().animation_policy).toBe('disable_legacy');
    expect(within(panel).getByRole('note')).toHaveTextContent('Advanced policy active: legacy animation references are disabled.');
    fireEvent.click(within(panel).getByRole('radio', { name: /Legacy compiler/ }));
    expect(saved().ag2_mode).toBe('cswin');
    fireEvent.click(rowButton(/Run, backward/));
    expect(screen.getByRole('region', { name: 'Animation graph handling' })).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole('region', { name: 'Animation graph handling' }), { key: 'Escape' });
    expect(screen.queryByRole('region', { name: 'Animation graph handling' })).not.toBeInTheDocument();
    fireEvent.click(button); fireEvent.click(screen.getByRole('button', { name: 'Close Animation graph handling' }));
    expect(screen.queryByRole('region', { name: 'Animation graph handling' })).not.toBeInTheDocument();
  });
});

describe('Clip player', () => {
  it('keeps one 3D view mounted while the selection, the played clip and the menu change, newest packet winning', async () => {
    await open(<Page initial={withRows([{ enabled: true, source: 'C:\\clips\\a.vnmclip_c', target: target('run_n') }])} preview={scene}/>);
    expect(await screen.findByTestId('clip-view')).toHaveTextContent('run_n');
    expect(native.clipPreview).toHaveBeenCalledWith(expect.anything(), target('run_n'), 'game', undefined);
    const player = screen.getByRole('region', { name: 'Clip player' });
    expect(player).toHaveTextContent('0.00 / 0.80 s · frame 1 of 25');
    expect(player.querySelectorAll('.clip-mark')).toHaveLength(1);
    fireEvent.click(within(player).getByRole('button', { name: 'Both' }));
    await waitFor(() => expect(native.clipPreview).toHaveBeenCalledWith(expect.anything(), target('run_n'), 'yours', 'C:\\clips\\a.vnmclip_c'));
    // A slow request for one clip, then another clip: the newest one is shown.
    let slow: (value: ArrayBuffer) => void = () => undefined;
    native.clipPreview.mockImplementationOnce(() => new Promise(resolve => { slow = resolve; }));
    fireEvent.click(rowButton(/Run, backward/));
    fireEvent.click(rowButton(/Stand idle/));
    await waitFor(() => expect(screen.getByTestId('clip-view')).toHaveTextContent('stand_idle'));
    await act(async () => { slow(packet('run_s')); });
    expect(screen.getByTestId('clip-view')).toHaveTextContent('stand_idle');
    fireEvent.click(screen.getByRole('button', { name: 'Animation graph handling' }));
    fireEvent.click(screen.getByRole('button', { name: 'Animation graph handling' }));
    expect(screen.getByRole('region', { name: 'Clip player' })).toBe(player);
    expect(probe.mounts).toBe(1);
    expect(probe.unmounts).toBe(0);
  });
  it('offers Your clip and Both only with a replacement, and states an additive clip and a cut', async () => {
    native.clipPreview.mockResolvedValue(packet('aim_idle', 12, { additive: true, over: 'stand_idle', blend_may_differ: true, cut: true, clip_frames: 2000 }));
    await open(<Page initial={withRows([])} preview={scene}/>);
    fireEvent.click(within(list()).getByRole('button', { name: /Weapon/ }));
    fireEvent.click(rowButton(/Aim idle/));
    const player = screen.getByRole('region', { name: 'Clip player' });
    await waitFor(() => expect(player).toHaveTextContent('shown over the standing pose (stand_idle); the game’s blend may differ'));
    expect(player).toHaveTextContent('Only the first 12 of its 2000 frames are shown.');
    expect(player).toHaveTextContent('frame 1 of 2000');
    expect(within(player).getByRole('button', { name: 'Your clip' })).toBeDisabled();
    expect(within(player).getByRole('button', { name: 'Both' })).toBeDisabled();
  });
});

describe('Replacements compiled at build', () => {
  it('reads only a saved .vnmclip (an animation document) as compiled from the user’s own animation', () => {
    expect(authoredSource('C:/clips/mine.vnmclip')).toBe(true);
    expect(authoredSource(' C:/clips/MINE.VNMCLIP ')).toBe(true);
    expect(authoredSource('C:/clips/mine.vnmclip_c')).toBe(false);
    expect(authoredSource('')).toBe(false);
    expect(authoredSource(undefined)).toBe(false);
  });
});
