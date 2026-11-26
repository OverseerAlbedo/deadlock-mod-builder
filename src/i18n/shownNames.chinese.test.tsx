import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { InstalledMod, Project, VpkContents } from '../models';

// Chinese names and words the pages draw themselves: the game's hero names in Installed mods and
// in Another hero's animation (found by either name, while filters and saved rows keep English),
// body-role names next to their identifiers, the effect player's "list not loaded" line as one
// Chinese sentence, and counts joined with a full-width comma. In English everything is as before.
const api = vi.hoisted(() => ({
  scanInstalledMods: vi.fn(), updateInstalledMods: vi.fn(), setInstalledModLabel: vi.fn(), showInstalledMod: vi.fn(), showProjectFile: vi.fn(), gameNames: vi.fn(),
  takenClipCheck: vi.fn(), takenClipHeroes: vi.fn(), takenClipSources: vi.fn(), takenClipReport: vi.fn(),
}));
vi.mock('../api', () => ({ api, isDesktop: () => false, basename: (path: string) => path.split(/[\\/]/).pop() ?? path }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: vi.fn(async () => true) }));
import { InstalledMods } from '../build/InstalledMods';
import { countList } from '../build/words';
import { shownPickerLabel } from '../components/HeroPicker';
import { boneLabelText, roleIdText } from '../preparation/mapping';
import { TakeClip, forgetTakenState } from '../motion/TakeClip';
import type { AnimationEntry } from '../components/AssetCatalog';
import { EffectPlayer, type PlayerBridge } from '../extras/EffectPlayer';
import { EFFECTS_NOT_LOADED } from '../extras/effectTypes';
import { BOMB, effectTrees } from '../extras/effectFixtures';
import { MIRROR_KEY, setLanguage } from './index';
import { forgetGameNames, loadGameNames } from './gameNames';

const NAMES = { language: 'zh-CN', heroes: { Haze: '岚梦', 'Lady Geist': '盖斯特夫人', Abrams: '亚伯拉姆斯' }, items: {}, missing: [] };
const contents = (o: Partial<VpkContents>): VpkContents => ({ files: 0, kinds: {}, heroes: [], every_hero: 0, unattributed: 0, ability_objects: [], samples: {}, ...o });
const ADDONS = 'G:\\Deadlock\\game\\citadel\\addons\\';
const mods: InstalledMod[] = [
  { path: `${ADDONS}pak03_dir.vpk`, file: 'pak03_dir.vpk', bytes: 1048576, status: 'up_to_date', note: '', models: [], identity: `vpk-tree-v1:${'1'.repeat(64)}`,
    contents: contents({ files: 4, kinds: { character_model: 1, material: 3 }, heroes: [{ name: 'Haze', files: 4 }] }),
    made: { project_id: 'a'.repeat(32), project_name: 'Sample', hero: 'Haze', project_file: 'C:\\Projects\\Sample.dmb.json', source: 'install' } },
  { path: `${ADDONS}pak04_dir.vpk`, file: 'pak04_dir.vpk', bytes: 1048576, status: 'no_hero_model', note: '', models: [], identity: `vpk-tree-v1:${'2'.repeat(64)}`,
    contents: contents({ files: 2, kinds: { sound: 2 }, heroes: [{ name: 'Lady Geist', files: 2 }] }) },
];
const row = (file: string) => screen.getByText(file, { selector: '.installed-file b' }).closest('li') as HTMLElement;
const shownFiles = () => [...document.querySelectorAll('.installed-file b')].map(b => b.textContent);

beforeEach(async () => {
  vi.clearAllMocks();
  api.gameNames.mockResolvedValue(NAMES);
  await act(() => setLanguage('zh-CN'));
  await act(() => loadGameNames('zh-CN'));
});
afterEach(async () => {
  cleanup();
  forgetGameNames();
  forgetTakenState();
  await act(() => setLanguage('en'));
  localStorage.removeItem(MIRROR_KEY);
});

describe('Names the pages draw themselves, in Chinese', () => {
  it('Installed mods: hero chips and rows show the game’s names, the search finds a row by either name, and a chip filters on the English name', async () => {
    api.scanInstalledMods.mockResolvedValue(mods);
    render(<InstalledMods/>);
    fireEvent.click(screen.getByRole('button', { name: '检查已安装的模组' }));
    await screen.findByRole('list', { name: '已安装的模组' });
    const chips = within(screen.getByRole('group', { name: '英雄' }));
    expect(chips.getByRole('button', { name: /^岚梦/ })).toBeInTheDocument();
    expect(chips.getByRole('button', { name: /^盖斯特夫人/ })).toBeInTheDocument();
    expect(chips.queryByRole('button', { name: /^Haze/ })).toBeNull();
    expect(within(row('pak03_dir.vpk')).getByText('岚梦')).toBeVisible();
    fireEvent.click(chips.getByRole('button', { name: /^盖斯特夫人/ }));
    expect(shownFiles()).toEqual(['pak04_dir.vpk']);
    fireEvent.click(chips.getByRole('button', { name: /^全部/ }));
    const search = screen.getByRole('searchbox', { name: '搜索英雄、文件或标签' });
    fireEvent.change(search, { target: { value: '岚梦' } });
    expect(shownFiles()).toEqual(['pak03_dir.vpk']);
    fireEvent.change(search, { target: { value: 'geist' } });
    expect(shownFiles()).toEqual(['pak04_dir.vpk']);
    // The details name the hero the mod was built for as the game does, closed up to the Chinese text.
    fireEvent.change(search, { target: { value: '' } });
    fireEvent.click(within(row('pak03_dir.vpk')).getByRole('button', { name: /详情/ }));
    const details = screen.getByRole('region', { name: 'pak03_dir.vpk 的详情' });
    expect(details).toHaveTextContent('岚梦（4 个文件）');
    expect(details).not.toHaveTextContent('Haze');
    await act(() => setLanguage('en'));
    expect(within(screen.getByRole('group', { name: 'Hero' })).getByRole('button', { name: /^Haze/ })).toBeInTheDocument();
  });

  it('Another hero’s animation: the list shows the game’s names with the English beside them, finds both, and saves the hero’s path', async () => {
    const OTHER = 'models/heroes_staging/haze/haze.vmdl';
    api.takenClipCheck.mockImplementation(async (_p: Project, path: string) => ({ target: path, refusal: null }));
    api.takenClipHeroes.mockResolvedValue([
      { hero: OTHER, name: 'Haze', usable: true },
      { hero: 'models/heroes_wip/abrams/abrams.vmdl', name: 'Abrams', usable: false, reason: 'Its body cannot be carried onto this hero.' },
    ]);
    api.takenClipSources.mockResolvedValue({ hero: OTHER, name: 'Haze', clips: [] });
    const project = { id: 'b'.repeat(32), original_model: 'models/heroes_wip/geist/geist.vmdl', hero_name: 'Lady Geist', customization: { animations: [] } } as unknown as Project;
    const entry = { target: 'anims/dash_ground.vnmclip', path: 'anims/dash_ground.vnmclip', label: 'Dash ground', name: 'Dash ground' } as unknown as AnimationEntry;
    const write = vi.fn(() => null);
    render(<TakeClip project={project} entry={entry} entries={[entry]} heroName="盖斯特夫人" row={null} disabled={false} write={write} writeRows={vi.fn()} remove={vi.fn()} useCompiled={vi.fn()} fail={vi.fn()}/>);
    fireEvent.click(await screen.findByRole('button', { name: '英雄' }));
    const list = await screen.findByRole('listbox', { name: '英雄' });
    expect(within(list).getByRole('option', { name: '岚梦' })).toHaveTextContent('岚梦Haze');
    expect(within(list).getByRole('option', { name: '亚伯拉姆斯' })).toHaveTextContent('Its body cannot be carried onto this hero.');
    const search = screen.getByRole('searchbox', { name: '搜索英雄…' });
    fireEvent.change(search, { target: { value: '岚梦' } });
    expect(within(list).getAllByRole('option').map(option => option.getAttribute('aria-label'))).toEqual(['岚梦']);
    fireEvent.change(search, { target: { value: 'abr' } });
    expect(within(list).getAllByRole('option').map(option => option.getAttribute('aria-label'))).toEqual(['亚伯拉姆斯']);
    fireEvent.change(search, { target: { value: '' } });
    fireEvent.click(within(list).getByRole('option', { name: '岚梦' }));
    await waitFor(() => expect(api.takenClipSources).toHaveBeenCalledWith(project, entry.target, OTHER));
    expect(write).not.toHaveBeenCalled();
  });

  it('Body roles: a role is named in Chinese with its identifier after it; English shows the identifier as before', async () => {
    expect(roleIdText('pelvis')).toBe('骨盆 / 髋部 (pelvis)');
    expect(roleIdText('upperarm_L')).toBe('左上臂 (upperarm_L)');
    expect(boneLabelText('thigh R')).toBe('右大腿 (thigh_R)');
    expect(boneLabelText('Left thumb')).toBe('左手拇指');
    expect(boneLabelText('Hair group')).toBe('Hair group');
    await act(() => setLanguage('en'));
    expect(roleIdText('upperarm_L')).toBe('upperarm_L');
    expect(boneLabelText('thigh R')).toBe('thigh R');
    expect(boneLabelText('Left thumb')).toBe('Left thumb');
  });

  it('a hero label shows the game’s name and the app’s own words after it; other words stay as they come', async () => {
    expect(shownPickerLabel('Haze · Alternate model')).toBe('岚梦 · 其他模型');
    expect(shownPickerLabel('Lady Geist · Not used in game')).toBe('盖斯特夫人 · 游戏中未使用');
    expect(shownPickerLabel('Haze · book fx model')).toBe('岚梦 · book fx model');
    await act(() => setLanguage('en'));
    expect(shownPickerLabel('Haze · Alternate model')).toBe('Haze · Alternate model');
    expect(shownPickerLabel('Lady Geist · Not used in game')).toBe('Lady Geist · Not used in game');
  });

  it('counts in one line are joined with a full-width comma in Chinese and ", " in English', async () => {
    expect(countList(['1 个错误', '4 个成功'])).toBe('1 个错误，4 个成功');
    await act(() => setLanguage('en'));
    expect(countList(['1 error', '4 successes'])).toBe('1 error, 4 successes');
  });

  it('the effect player’s “list not loaded” line reads as one Chinese sentence before Try again', async () => {
    const tree = effectTrees.trees.find(item => item.target === BOMB)!;
    const project = { id: 'c'.repeat(32), original_model: 'models/heroes_wip/test/test.vmdl' } as unknown as Project;
    const bridge = {
      play: vi.fn(async () => { throw { message: EFFECTS_NOT_LOADED }; }),
      edit: vi.fn(), control: vi.fn(), stop: vi.fn(async () => undefined), frame: vi.fn(async () => new ArrayBuffer(0)),
      listen: vi.fn(() => () => undefined), draw: vi.fn(),
    } as unknown as PlayerBridge;
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={bridge}/>);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('特效列表尚未加载，请重新打开“附加内容 → 特效”。下方的颜色条仍会显示你的编辑。');
    expect(within(alert).getByRole('button', { name: '重试' })).toBeInTheDocument();
    await act(() => setLanguage('en'));
    expect(screen.getByRole('alert')).toHaveTextContent(`${EFFECTS_NOT_LOADED} The color strips below still show your edit.`);
  });
});
