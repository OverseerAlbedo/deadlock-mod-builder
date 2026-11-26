import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { InstalledMod, VpkContents } from '../models';
import { InstalledMods } from './InstalledMods';
import { MIRROR_KEY, setLanguage } from '../i18n';

// Installed mods in Chinese: the scan and the filters work by their Chinese names, file names,
// hero names, labels and the backend's notes are shown as they come, and a typed label is kept as typed.
const api = vi.hoisted(() => ({ scanInstalledMods: vi.fn(), updateInstalledMods: vi.fn(), setInstalledModLabel: vi.fn(), showInstalledMod: vi.fn(), showProjectFile: vi.fn() }));
const dialog = vi.hoisted(() => ({ ask: vi.fn() }));
vi.mock('../api', () => ({ api }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: dialog.ask }));

const contents = (o: Partial<VpkContents>): VpkContents => ({ files: 0, kinds: {}, heroes: [], every_hero: 0, unattributed: 0, ability_objects: [], samples: {}, ...o });
const id = (n: number) => `vpk-tree-v1:${String(n).repeat(64).slice(0, 64)}`;
const ADDONS = 'G:\\Deadlock\\game\\citadel\\addons\\';
const NOTE = 'Built before the game\'s October 16, 2026 update: camera pivots are rotated in a match.';
const mods: InstalledMod[] = [
  { path: `${ADDONS}Sample_pak03_dir.vpk`, file: 'Sample_pak03_dir.vpk', bytes: 132644291, status: 'needs_update', note: NOTE, models: [{ path: 'models/heroes_staging/haze/haze.vmdl_c', hero: 'Haze', convention: 'previous' }],
    identity: id(1), contents: contents({ files: 40, kinds: { character_model: 1, gun: 1, material: 14, texture: 24 }, heroes: [{ name: 'Haze', files: 40 }] }),
    shared: [{ file: 'pak09_dir.vpk', path: `${ADDONS}pak09_dir.vpk`, count: 1, paths: ['models/heroes_staging/haze/haze.vmdl_c'] }],
    made: { project_id: 'a'.repeat(32), project_name: 'Sample', hero: 'Haze', project_file: 'C:\\Projects\\Sample.dmb.json', source: 'install' }, label: 'Main skin' },
  { path: `${ADDONS}pak06_dir.vpk`, file: 'pak06_dir.vpk', bytes: 45102280, status: 'no_hero_model', note: '', models: [], identity: id(2),
    contents: contents({ files: 17, kinds: { sound: 12, icon: 3, effect: 2 }, heroes: [{ name: 'Haze', files: 15 }, { name: 'Lady Geist', files: 2 }] }) },
  { path: `${ADDONS}pak05_dir.vpk`, file: 'pak05_dir.vpk', bytes: 1887436, status: 'no_hero_model', note: '', models: [], identity: id(4),
    contents: contents({ files: 14, kinds: { item_icon: 14 }, every_hero: 14 }) },
  { path: `${ADDONS}pak07_dir.vpk`, file: 'pak07_dir.vpk', bytes: 12582912, status: 'no_hero_model', note: '', models: [], identity: id(5),
    contents: contents({ files: 38, kinds: { map: 30, menu: 8 }, unattributed: 38 }) },
];
const row = (file: string) => screen.getByText(file, { selector: '.installed-file b' }).closest('li') as HTMLElement;

beforeEach(async () => {
  vi.clearAllMocks(); dialog.ask.mockResolvedValue(true);
  await act(() => setLanguage('zh-CN'));
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage('en'));
  localStorage.removeItem(MIRROR_KEY);
});
async function scanned() {
  api.scanInstalledMods.mockResolvedValue(mods);
  render(<InstalledMods/>);
  fireEvent.click(screen.getByRole('button', { name: '检查已安装的模组' }));
  await screen.findByRole('list', { name: '已安装的模组' });
}

describe('Installed mods in Chinese', () => {
  it('the scan ends with the list, named by contents in Chinese, names and notes as they come', async () => {
    await scanned();
    expect(api.scanInstalledMods).toHaveBeenCalledTimes(1);
    expect(screen.getByText(/4 个文件 · /)).toBeVisible();
    expect(screen.getByText(/1 个需要更新 · 1 个更改了相同的游戏文件/)).toBeVisible();
    const first = within(row('Sample_pak03_dir.vpk'));
    expect(first.getByText('Haze')).toBeVisible();
    expect(first.getByText('英雄模型 · 枪械 · 14 个材质')).toBeVisible();
    expect(first.getByText('在此制作 · Sample')).toBeVisible();
    expect(first.getByText('与 pak09_dir.vpk 更改了 1 个相同的游戏文件')).toBeVisible();
    expect(first.getByText('需要针对当前游戏更新')).toBeVisible();
    expect(first.getByText('Main skin')).toBeVisible();
    expect(within(row('pak06_dir.vpk')).getByText('Haze、Lady Geist')).toBeVisible();
    expect(within(row('pak05_dir.vpk')).getByText('所有英雄')).toBeVisible();
    expect(within(row('pak07_dir.vpk')).getByText('地图和菜单文件（38 个文件）')).toBeVisible();
    // The details: the backend's note is shown as it comes.
    fireEvent.click(within(row('Sample_pak03_dir.vpk')).getByRole('button', { name: /详情/ }));
    const details = screen.getByRole('region', { name: 'Sample_pak03_dir.vpk 的详情' });
    expect(details).toHaveTextContent(NOTE);
    expect(details).toHaveTextContent('由本应用根据项目“Sample”为 Haze 构建');
    expect(details).toHaveTextContent('这些文件也存在于其他 VPK 中，但内容不同。对局中使用哪个版本不由这里决定。');
  });

  it('the filters work by their Chinese names; a hero chip keeps the hero’s own name', async () => {
    await scanned();
    const heroes = within(screen.getByRole('group', { name: '英雄' }));
    fireEvent.click(heroes.getByRole('button', { name: /^物品/ }));
    expect(screen.getAllByRole('listitem').filter(item => item.classList.contains('installed-row')).map(item => within(item).getByText(/\.vpk$/).textContent)).toEqual(['pak05_dir.vpk']);
    fireEvent.click(heroes.getByRole('button', { name: /^Lady Geist/ }));
    expect(screen.queryByText('pak05_dir.vpk')).toBeNull();
    expect(screen.getByText('pak06_dir.vpk')).toBeInTheDocument();
    fireEvent.click(heroes.getByRole('button', { name: /^全部/ }));
    fireEvent.click(within(screen.getByRole('group', { name: '制作来源' })).getByRole('button', { name: '在此制作' }));
    expect(screen.getByText('Sample_pak03_dir.vpk')).toBeInTheDocument();
    expect(screen.queryByText('pak06_dir.vpk')).toBeNull();
    fireEvent.change(screen.getByRole('searchbox', { name: '搜索英雄、文件或标签' }), { target: { value: 'nothing like it' } });
    expect(screen.getByText('没有已安装的文件符合这些筛选条件。')).toBeInTheDocument();
  });

  it('a label is saved as typed, an update asks in Chinese and sends the paths, and a failed scan shows the backend’s message', async () => {
    api.setInstalledModLabel.mockImplementation(async (_identity: string, _file: string, label: string) => label.trim());
    api.updateInstalledMods.mockResolvedValue([{ file: 'Sample_pak03_dir.vpk', path: mods[0].path, backup: 'C:\\backups\\Sample_pak03_dir.vpk', updated: ['models/heroes_staging/haze/haze.vmdl_c'] }]);
    await scanned();
    fireEvent.click(within(row('pak06_dir.vpk')).getByRole('button', { name: '为 pak06_dir.vpk 添加标签' }));
    const input = screen.getByLabelText('你的标签');
    expect(input).toHaveAttribute('placeholder', '例如：主皮肤');
    fireEvent.change(input, { target: { value: '朋友的皮肤 v2' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(api.setInstalledModLabel).toHaveBeenCalledWith(id(2), 'pak06_dir.vpk', '朋友的皮肤 v2'));
    expect(await screen.findByText(/已保存。/)).toBeInTheDocument();
    fireEvent.click(within(row('Sample_pak03_dir.vpk')).getByRole('button', { name: '更新 Sample_pak03_dir.vpk' }));
    await waitFor(() => expect(api.updateInstalledMods).toHaveBeenCalledWith([mods[0].path]));
    expect(dialog.ask).toHaveBeenCalledWith(expect.stringContaining('要更新游戏 addons 文件夹中的 Sample_pak03_dir.vpk 吗？'), expect.objectContaining({ title: '更新已安装的模组' }));
    expect(await screen.findByText(/已更新 Sample_pak03_dir\.vpk（1 个模型）。/)).toBeInTheDocument();
    cleanup();
    api.scanInstalledMods.mockRejectedValueOnce({ message: 'The addons folder could not be read: access denied.' });
    render(<InstalledMods/>);
    fireEvent.click(screen.getByRole('button', { name: '检查已安装的模组' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The addons folder could not be read: access denied.');
  });

  it('switches live and keeps the scanned list', async () => {
    await scanned();
    await act(() => setLanguage('en'));
    expect(screen.getByRole('list', { name: 'Installed mods' })).toBeInTheDocument();
    expect(within(row('Sample_pak03_dir.vpk')).getByText('Character model · gun · 14 materials')).toBeVisible();
    await act(() => setLanguage('zh-CN'));
    expect(within(row('Sample_pak03_dir.vpk')).getByText('英雄模型 · 枪械 · 14 个材质')).toBeVisible();
    expect(api.scanInstalledMods).toHaveBeenCalledTimes(1);
  });
});
