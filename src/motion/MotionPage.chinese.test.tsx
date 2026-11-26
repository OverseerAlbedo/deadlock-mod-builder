import { useRef, useState, type ReactElement } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Project, ResourceReplacement } from '../models';
import type { AnimationEntry } from '../components/AssetCatalog';
import { HistoryContext, type HistoryControls } from '../history/useProjectHistory';
import { ProjectHistory } from '../history/projectHistory';
import { MotionPage } from './MotionPage';
import { forgetClipPackets } from './ClipPlayer';
import { LIST_FAILED, MAKE_FAILED, forgetMadeState } from './MakeClip';
import { forgetTakenState } from './TakeClip';
import { ADDITIVE, type ActionList, type MadeReport } from './makeTypes';
import { TAKEN_BODY_NOT_CARRIED, type SourceHero, type TakenReport, type TakenSources } from './takenTypes';
import { MIRROR_KEY, setLanguage } from '../i18n';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

// The Motion page driven in Chinese: the three ways, the options and the lists work by their
// Chinese names, while the rows written, the operation labels and failure titles stay English
// and what the engine says (refusals, reasons, names) is shown as it comes.
const native = vi.hoisted(() => ({
  motionCatalog: vi.fn(), clipFit: vi.fn(), clipPreview: vi.fn(), validateAssetPath: vi.fn(),
  madeClipCheck: vi.fn(), madeClipActions: vi.fn(), makeClip: vi.fn(), madeClipReport: vi.fn(), madeClipPreview: vi.fn(),
  takenClipCheck: vi.fn(), takenClipHeroes: vi.fn(), takenClipSources: vi.fn(), takenClipReport: vi.fn(), takenClipPreview: vi.fn(),
}));
vi.mock('../api', async original => ({ ...await original<typeof import('../api')>(), api: native }));
const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => dialog);
vi.mock('./ClipViewport', () => ({ default: function Probe() { return <div data-testid="clip-view"/>; } }));

const HERO = 'models/heroes_wip/sample/sample.vmdl_c';
const OTHER = 'models/heroes_wip/other/other.vmdl';
const target = (label: string) => `models/heroes_wip/sample/clips/${label}.vnmclip_c`;
const theirs = (label: string) => `models/heroes_wip/other/clips/${label}.vnmclip_c`;
const clip = (label: string, extra: Partial<AnimationEntry> = {}): AnimationEntry => ({
  path: target(label), target: target(label), label, supported: true,
  skeleton: 'models/heroes_wip/sample/sample.vnmskel', frames: 56, duration: 1.8333, tracks: 72, events: 2, additive: false,
  name: label.replace(/_/g, ' ').replace(/^./, c => c.toUpperCase()), group: 'movement', graph_named: true, ...extra,
});
const ENTRIES = [clip('stand_idle'), clip('dash_ground', { frames: 32, duration: 1.0333 }), clip('reload_idle', { group: 'weapon' }), clip('lean_forward', { additive: true, frames: 2, duration: 0.0333 })];
const FILE = 'C:\\anims\\my_character_moves.blend';
const base = { ...reference.cases[1].expected, original_model: HERO, hero_name: 'Sample', dmx: 'C:\\work\\prepared.dmx', preparation_source: 'C:\\models\\my_character.blend', animation_policy: 'preserve', ag2_mode: 'auto' } as unknown as Project;
const withRows = (rows: ResourceReplacement[]) => ({ ...base, customization: { ...base.customization, enabled: true, animations: rows } }) as Project;
const LIST: ActionList = { source: FILE, rig: 'Armature', bones: 40, driving_bones: 40, skeleton_error: null, actions: [
  { name: 'Wave', frame_start: 0, frame_end: 85, fps: 30, length_seconds: 2.8333, keyed_bones: ['Hips', 'Spine'], driving_keyed_bones: 14 },
  { name: 'Camera', frame_start: 0, frame_end: 10, fps: 30, length_seconds: 0.3333, keyed_bones: [], driving_keyed_bones: 0 },
] };
const MADE: MadeReport = {
  action: 'Wave', mode: 'as_it_looks', switches: null, clip_frames: 56, clip_seconds: 1.8333, action_seconds: 2.8333, plays_from: 0, plays_to: 2.8333, stretch: 1.545,
  follow: ['pelvis', 'spine_1', 'neck_0', 'head', 'arm_upper_L'], carried: [], between: [], kept_count: 74, kept_kinds: { fingers: 38, other: 16 }, kept_examples: {}, not_in_clip: [],
  still_made_moving: [], still_new_fixed_value: [], still_held: [], still_kept_game_value: [], ranges_refitted: [], ranges_clamped: [], words_per_frame: [75, 78],
  loop_gap_degrees: 12.3, loop_gap_inches: 0.4, loop_blend_seconds: null, drift_removed_inches: 72.04, feet_lowest_inches: -0.79, feet_highest_inches: -0.78,
};
const HEROES: SourceHero[] = [{ hero: 'models/heroes_wip/blocky/blocky.vmdl', name: 'Blocky', usable: false, reason: `${TAKEN_BODY_NOT_CARRIED} (its body bones are named differently).` }, { hero: OTHER, name: 'Other', usable: true }];
const SOURCES: TakenSources = { target: target('dash_ground'), hero: OTHER, clips: [
  { clip: theirs('dash_ground'), label: 'dash_ground', name: 'Dash ground', group: 'movement', seconds: 1.5333, frames: 47, rank: 0, usable: true },
  { clip: theirs('stand_idle'), label: 'stand_idle', name: 'Stand idle', group: 'movement', seconds: 2, frames: 61, rank: 2, usable: true },
] };
const TAKEN: TakenReport = { made: { ...MADE, clip_seconds: 1.0333, action_seconds: 1.5333, stretch: 1.484 }, source_name: 'Other', driven: ['pelvis', 'head'], own: [], not_matched: [], speed_percent: 48.4, size: 1 };
function packet(name: string) {
  const header = new TextEncoder().encode(JSON.stringify({ bones: [{ name: 'root', parent: -1, position: [0, 0, 0] }], clips: [{ name, sources: [name], fps: 30, frames: 4, segments: [4], offset: 0, duration: 0.1, clip_frames: 4, additive: false, blend_may_differ: false, cut: false, which: 'game', marks: [] }] }));
  const start = Math.ceil((4 + header.length) / 4) * 4, out = new ArrayBuffer(start + 4 * 48);
  new DataView(out).setUint32(0, header.length, true); new Uint8Array(out, 4, header.length).set(header);
  return out;
}

const events: string[] = [];
const fail = vi.fn();
function Page({ initial }: { initial: Project }) {
  const [project, setProject] = useState(initial);
  const history = useRef(new ProjectHistory()).current;
  const controls: HistoryControls = { canUndo: false, canRedo: false, undo: () => undefined, redo: () => undefined, gesture: on => history.gesture(on), fence: () => history.fence() };
  const update = (next: Project) => { events.push('write'); setProject(next); };
  const run = async <T,>(label: string, operation: () => Promise<T>) => { events.push(`operate:${label}`); return operation(); };
  return <HistoryContext.Provider value={controls}>
    <MotionPage project={project} update={update} disabled={false} fail={fail} load={vi.fn()} next={vi.fn()} operate={run}/>
    <output data-testid="saved">{JSON.stringify(project)}</output>
  </HistoryContext.Provider>;
}
const saved = () => JSON.parse(screen.getByTestId('saved').textContent!) as Project;
const list = () => screen.getByRole('region', { name: '动画' });
const card = (name: string) => screen.getByRole('region', { name: `动画片段 ${name}` });
async function open(ui: ReactElement) {
  render(ui);
  await waitFor(() => expect(list().querySelector('.clip-row')).not.toBeNull());
}

beforeEach(async () => {
  vi.clearAllMocks(); forgetClipPackets(); forgetMadeState(); forgetTakenState(); events.length = 0;
  native.motionCatalog.mockImplementation(async () => ({ hero_path: HERO, animations: ENTRIES }));
  native.clipFit.mockResolvedValue({ kind: 'compiled', fits: true, checks: [] });
  native.clipPreview.mockImplementation(async () => packet('game'));
  native.madeClipCheck.mockImplementation(async (_p: Project, path: string) => ({ target: path, refusal: null, fit_error: null }));
  native.madeClipActions.mockResolvedValue(LIST);
  native.makeClip.mockImplementation(async (_p: Project, path: string) => ({ target: path, made: {}, report: MADE }));
  native.madeClipReport.mockResolvedValue(MADE);
  native.madeClipPreview.mockImplementation(async () => packet('made'));
  native.takenClipCheck.mockImplementation(async (_p: Project, path: string) => ({ target: path, refusal: null }));
  native.takenClipHeroes.mockResolvedValue(HEROES);
  native.takenClipSources.mockResolvedValue(SOURCES);
  native.takenClipReport.mockResolvedValue(TAKEN);
  native.takenClipPreview.mockImplementation(async () => packet('taken'));
  await act(() => setLanguage('zh-CN'));
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage('en'));
  localStorage.removeItem(MIRROR_KEY);
});

describe('The Motion page in Chinese', () => {
  it('Make it from my animation lists the file and makes the clip as operations with their English labels, and writes the row as it is', async () => {
    dialog.open.mockResolvedValueOnce(FILE);
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(within(list()).getByRole('button', { name: /Stand idle/ }));
    fireEvent.click(within(card('Stand idle')).getByRole('button', { name: '用我的动画制作' }));
    fireEvent.click(await within(card('Stand idle')).findByRole('button', { name: '选择文件…' }));
    await within(card('Stand idle')).findByText('与你准备好的角色骨架相同（40 根骨骼）。');
    // The file window shows its words in Chinese; the purpose and the file types stay as they are.
    expect(dialog.open).toHaveBeenCalledWith(expect.objectContaining({ title: '包含你的动画的文件', filters: [{ name: 'Blender、FBX 或 glTF 文件', extensions: ['blend', 'fbx', 'glb', 'gltf'] }] }));
    const select = within(card('Stand idle')).getByRole('combobox', { name: '动画' });
    expect(within(select).getAllByRole('option').map(option => option.textContent)).toEqual(['选择动画…', 'Wave · 2.83 秒 · 86 帧', 'Camera · 0.33 秒 · 11 帧（不会移动你角色的任何骨骼）']);
    fireEvent.change(select, { target: { value: 'Wave' } });
    await waitFor(() => expect(native.makeClip).toHaveBeenCalledTimes(1));
    expect(events).toEqual([`operate:${LIST_FAILED}`, 'write', `operate:${MAKE_FAILED}`]);
    expect(LIST_FAILED).toBe('Could not read the animations of the file');
    expect(saved().customization.animations).toEqual([{ enabled: true, source: FILE, target: target('stand_idle'), allow_shared: false, made: { action: 'Wave', mode: 'as_it_looks', bones: 'keyed' } }]);
    const makes = await within(card('Stand idle')).findByRole('list', { name: '本应用制作的内容' });
    // Bone names stay as the engine gives them; the sentence and its count are Chinese.
    expect(makes).toHaveTextContent('5 根骨骼跟随你的动画（pelvis、spine 1、neck 0、head、…）');
    expect(makes).toHaveTextContent('74 根骨骼保留游戏的动作（fingers、other）');
    expect(card('Stand idle')).toHaveTextContent('游戏将此动画片段的长度固定为 1.83 秒。你的 2.83 秒动画会以 1.5 倍速播放以契合。');
    expect(card('Stand idle')).toHaveTextContent('已移除 72.0 英寸的位移。');
    // Each option writes its identifier.
    fireEvent.click(within(card('Stand idle')).getByRole('radio', { name: /所有匹配的骨骼/ }));
    expect(saved().customization.animations?.[0].made).toEqual({ action: 'Wave', mode: 'as_it_looks', bones: 'all' });
    fireEvent.click(within(card('Stand idle')).getByRole('switch', { name: '循环' }));
    expect(saved().customization.animations?.[0].made).toEqual({ action: 'Wave', mode: 'as_it_looks', loop_blend_seconds: 0.25, bones: 'all' });
    expect(within(list()).getByRole('button', { name: /Stand idle/ })).toHaveTextContent('已替换');
  });

  it('Another hero’s animation is driven by its Chinese names; the row keeps the hero and clip paths, and the engine’s reason stays as it comes', async () => {
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(within(list()).getByRole('button', { name: /Dash ground/ }));
    fireEvent.click(within(card('Dash ground')).getByRole('button', { name: '其他英雄的动画' }));
    expect(card('Dash ground')).toHaveTextContent('采用游戏中已有英雄的动作。Sample 的长度、事件和位移保持游戏原样，因此始终匹配。');
    fireEvent.click(await within(card('Dash ground')).findByRole('button', { name: '英雄' }));
    const heroes = await within(card('Dash ground')).findByRole('listbox', { name: '英雄' });
    expect(heroes).toHaveTextContent(`${TAKEN_BODY_NOT_CARRIED} (its body bones are named differently).`);
    fireEvent.click(within(heroes).getByRole('option', { name: 'Other' }));
    await waitFor(() => expect(within(card('Dash ground')).getByRole('button', { name: '动画' })).not.toBeDisabled());
    expect(events).toEqual([]);
    fireEvent.click(within(card('Dash ground')).getByRole('button', { name: '动画' }));
    const clips = await within(card('Dash ground')).findByRole('listbox', { name: 'Other 的动画' });
    expect(within(clips).getAllByRole('group').map(group => group.getAttribute('aria-label'))).toEqual(['相同动作', '同一分组']);
    fireEvent.click(within(clips).getByRole('option', { name: 'Dash ground' }));
    expect(events).toEqual(['write']);
    expect(saved().customization.animations).toEqual([{ enabled: true, target: target('dash_ground'), allow_shared: false, taken: { hero: OTHER, clip: theirs('dash_ground'), timing: 'stretched', arms: 'source' } }]);
    const gets = await within(card('Dash ground')).findByRole('list', { name: '得到的结果' });
    expect(gets).toHaveTextContent('身体的动作来自 Other：2 根骨骼（pelvis、head）');
    expect(gets).toHaveTextContent('加快 1.5 倍以契合：Other 的 1.53 秒放入 Sample 的 1.03 秒');
    fireEvent.click(within(card('Dash ground')).getByRole('button', { name: '保留 Sample 自己的' }));
    expect(saved().customization.animations?.[0].taken).toEqual({ hero: OTHER, clip: theirs('dash_ground'), timing: 'stretched', arms: 'own' });
    fireEvent.click(within(card('Dash ground')).getByRole('button', { name: '按原速播放其中一部分' }));
    expect(saved().customization.animations?.[0].taken).toEqual({ hero: OTHER, clip: theirs('dash_ground'), timing: 'own_speed', arms: 'own' });
  });

  it('a refusal from the engine is shown as it comes, and a failure keeps its English title', async () => {
    native.madeClipCheck.mockImplementation(async (_p: Project, path: string) => ({ target: path, refusal: ADDITIVE, fit_error: null }));
    await open(<Page initial={withRows([])}/>);
    fireEvent.click(within(list()).getByRole('button', { name: /Lean forward/ }));
    fireEvent.click(within(card('Lean forward')).getByRole('button', { name: '用我的动画制作' }));
    expect(await within(card('Lean forward')).findByRole('note')).toHaveTextContent(ADDITIVE);
    fireEvent.click(within(card('Lean forward')).getByRole('button', { name: '改用编译好的动画片段文件' }));
    expect(within(card('Lean forward')).getByRole('button', { name: '编译好的动画片段文件' })).toHaveAttribute('aria-pressed', 'true');
    // A project already holding 128 rows: the page's failure keeps its English title and text.
    cleanup();
    const full = Array.from({ length: 128 }, (_, index) => ({ enabled: true, source: `C:\\clips\\${index}.vnmclip_c`, target: `models/elsewhere/${index}.vnmclip_c`, allow_shared: false }));
    native.validateAssetPath.mockImplementation(async (path: string) => path);
    dialog.open.mockResolvedValueOnce('C:\\clips\\new.vnmclip_c');
    await open(<Page initial={withRows(full)}/>);
    fireEvent.click(within(list()).getByRole('button', { name: /^武器/ }));
    fireEvent.click(within(list()).getByRole('button', { name: /Reload idle/ }));
    fireEvent.click(within(card('Reload idle')).getByRole('button', { name: '选择文件…' }));
    await waitFor(() => expect(fail).toHaveBeenCalledWith('Too many clip replacements', 'A project holds at most 128 clip replacements.'));
    expect(dialog.open).toHaveBeenCalledWith(expect.objectContaining({ title: '替换动画片段', filters: [{ name: '动画片段', extensions: expect.any(Array) }] }));
  });

  it('switches live: groups, the switch and the badge follow the language, and the list keeps its choice', async () => {
    await open(<Page initial={withRows([])}/>);
    expect(within(list()).getByRole('group', { name: '分组' })).toHaveTextContent('移动');
    expect(within(list()).getByRole('button', { name: /^武器/ })).toBeInTheDocument();
    fireEvent.click(within(list()).getByRole('button', { name: /^武器/ }));
    expect(within(list()).getByRole('heading', { name: '武器 · 1' })).toBeInTheDocument();
    await act(() => setLanguage('en'));
    expect(within(screen.getByRole('region', { name: 'Animations' })).getByRole('heading', { name: 'Weapon · 1' })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'How the replacement is made' })).toBeInTheDocument();
    await act(() => setLanguage('zh-CN'));
    expect(screen.getByRole('group', { name: '替换内容的制作方式' })).toHaveTextContent('编译好的动画片段文件用我的动画制作其他英雄的动画');
    expect(screen.getByText('原版动作')).toBeInTheDocument();
    expect(events).toEqual([]);
  });
});
