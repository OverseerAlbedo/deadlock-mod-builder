import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { InstalledMod, VpkContents } from '../models';
import { InstalledMods } from './InstalledMods';
import { fromText, heroText, sharedText, stateOf, summary } from './installedModsText';

const api = vi.hoisted(() => ({ scanInstalledMods: vi.fn(), updateInstalledMods: vi.fn(), setInstalledModLabel: vi.fn(), showInstalledMod: vi.fn(), showProjectFile: vi.fn() }));
const dialog = vi.hoisted(() => ({ ask: vi.fn() }));
vi.mock('../api', () => ({ api }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: dialog.ask }));

const contents = (o: Partial<VpkContents>): VpkContents => ({ files: 0, kinds: {}, heroes: [], every_hero: 0, unattributed: 0, ability_objects: [], samples: {}, ...o });
const id = (n: number) => `vpk-tree-v1:${String(n).repeat(64).slice(0, 64)}`;
const ADDONS = 'G:\\Deadlock\\game\\citadel\\addons\\';
const mods: InstalledMod[] = [
  { path: `${ADDONS}Sample_pak03_dir.vpk`, file: 'Sample_pak03_dir.vpk', bytes: 132644291, status: 'needs_update', note: 'Built before the game\'s October 16, 2026 update: camera pivots are rotated in a match.', models: [{ path: 'models/heroes_staging/haze/haze.vmdl_c', hero: 'Haze', convention: 'previous' }],
    identity: id(1), contents: contents({ files: 40, kinds: { character_model: 1, gun: 1, material: 14, texture: 24 }, heroes: [{ name: 'Haze', files: 40 }], samples: { material: ['materials/models/heroes/haze/body.vmat_c'] } }),
    shared: [{ file: 'pak09_dir.vpk', path: `${ADDONS}pak09_dir.vpk`, count: 1, paths: ['models/heroes_staging/haze/haze.vmdl_c'] }],
    made: { project_id: 'a'.repeat(32), project_name: 'Sample', hero: 'Haze', project_file: 'C:\\Projects\\Sample.dmb.json', source: 'install' }, label: 'Main skin' },
  { path: `${ADDONS}pak06_dir.vpk`, file: 'pak06_dir.vpk', bytes: 45102280, status: 'no_hero_model', note: '', models: [], identity: id(2),
    contents: contents({ files: 17, kinds: { sound: 12, icon: 3, effect: 2 }, heroes: [{ name: 'Haze', files: 15 }, { name: 'Lady Geist', files: 2 }] }),
    made: { project_id: 'b'.repeat(32), project_name: 'Second', hero: 'Haze', project_file: null, source: 'output' } },
  { path: `${ADDONS}pak09_dir.vpk`, file: 'pak09_dir.vpk', bytes: 178717543, status: 'up_to_date', note: '', models: [{ path: 'models/heroes_wip/geist/geist.vmdl_c', hero: 'Lady Geist', convention: 'current' }], identity: id(3),
    contents: contents({ files: 10, kinds: { character_model: 1, material: 9 }, heroes: [{ name: 'Lady Geist', files: 10 }] }),
    shared: [{ file: 'Sample_pak03_dir.vpk', path: `${ADDONS}Sample_pak03_dir.vpk`, count: 1, paths: ['models/heroes_staging/haze/haze.vmdl_c'] }] },
  { path: `${ADDONS}pak05_dir.vpk`, file: 'pak05_dir.vpk', bytes: 1887436, status: 'no_hero_model', note: '', models: [], identity: id(4),
    contents: contents({ files: 14, kinds: { item_icon: 14 }, every_hero: 14 }) },
  { path: `${ADDONS}pak07_dir.vpk`, file: 'pak07_dir.vpk', bytes: 12582912, status: 'no_hero_model', note: '', models: [], identity: id(5),
    contents: contents({ files: 38, kinds: { map: 30, menu: 8 }, unattributed: 38 }) },
  { path: `${ADDONS}pak08_dir.vpk`, file: 'pak08_dir.vpk', bytes: 16, status: 'unknown', note: 'Not readable as a VPK: Not a supported VPK v1/v2 archive.', models: [], identity: '', unreadable: 'Not a supported VPK v1/v2 archive.' },
];
beforeEach(() => { vi.clearAllMocks(); dialog.ask.mockResolvedValue(true); });
const row = (file: string) => screen.getByText(file, { selector: '.installed-file b' }).closest('li') as HTMLElement;
async function scanned() {
  api.scanInstalledMods.mockResolvedValue(mods);
  render(<InstalledMods />);
  fireEvent.click(screen.getByRole('button', { name: 'Check installed mods' }));
  await screen.findByRole('list', { name: 'Installed mods' });
}

describe('Installed mods: what every VPK is', () => {
  it('scans only on request and names each file by its contents', async () => {
    api.scanInstalledMods.mockResolvedValue(mods);
    render(<InstalledMods />);
    expect(api.scanInstalledMods).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: /Update/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Check installed mods' }));
    await screen.findByRole('list', { name: 'Installed mods' });
    expect(screen.getByText(/6 files · /)).toBeVisible();
    expect(screen.getByText(/1 needs an update · 2 change the same game files/)).toBeVisible();
    const first = within(row('Sample_pak03_dir.vpk'));
    expect(first.getByText('Haze')).toBeVisible();
    expect(first.getByText('Character model · gun · 14 materials')).toBeVisible();
    expect(first.getByText('Made here · Sample')).toBeVisible();
    expect(first.getByText('Changes 1 of the same game file as pak09_dir.vpk')).toBeVisible();
    expect(first.getByText('Needs an update for the current game')).toBeVisible();
    expect(first.getByText('Main skin')).toBeVisible();
    const extras = within(row('pak06_dir.vpk'));
    expect(extras.getByText('Haze, Lady Geist')).toBeVisible();
    expect(extras.getByText('12 sounds · 3 icons · 2 effects')).toBeVisible();
    expect(extras.getByText('Made here · Second (extras only) · project not on this PC')).toBeVisible();
    expect(extras.getByText('Not checked: no character model')).toBeVisible();
    expect(within(row('pak05_dir.vpk')).getByText('Every hero')).toBeVisible();
    expect(within(row('pak05_dir.vpk')).getByText('14 item icons')).toBeVisible();
    expect(within(row('pak07_dir.vpk')).getByText('No hero')).toBeVisible();
    expect(within(row('pak07_dir.vpk')).getByText('Map and menu files (38 files)')).toBeVisible();
    expect(within(row('pak07_dir.vpk')).getByText('Not made by this app')).toBeVisible();
    expect(within(row('pak09_dir.vpk')).getByText('Up to date')).toBeVisible();
    // An unreadable file is listed with its reason, never dropped.
    expect(within(row('pak08_dir.vpk')).getByText('Could not be read', { selector: 'b' })).toBeVisible();
    expect(within(row('pak08_dir.vpk')).getByText('Not readable as a VPK')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Update 1 mod' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Scan again' })).toBeEnabled();
  });

  it('filters by hero, by where the file comes from and by a search over hero, file and label', async () => {
    await scanned();
    const heroes = within(screen.getByRole('group', { name: 'Hero' }));
    expect(heroes.getAllByRole('button').map(b => b.textContent)).toEqual(['All 6', 'Haze 2', 'Lady Geist 2', 'Items 1', 'Other 2']);
    const files = () => within(screen.getByRole('list', { name: 'Installed mods' })).getAllByRole('listitem').map(li => li.querySelector('.installed-file b')?.textContent);
    fireEvent.click(heroes.getByRole('button', { name: /^Lady Geist/ }));
    expect(files()).toEqual(['pak06_dir.vpk', 'pak09_dir.vpk']);
    fireEvent.click(within(screen.getByRole('group', { name: 'Made by' })).getByRole('button', { name: 'From elsewhere' }));
    expect(files()).toEqual(['pak09_dir.vpk']);
    fireEvent.click(heroes.getByRole('button', { name: /^All/ }));
    fireEvent.click(within(screen.getByRole('group', { name: 'Made by' })).getByRole('button', { name: 'Made here' }));
    expect(files()).toEqual(['Sample_pak03_dir.vpk', 'pak06_dir.vpk']);
    fireEvent.click(within(screen.getByRole('group', { name: 'Made by' })).getByRole('button', { name: 'All' }));
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search hero, file or label' }), { target: { value: 'main sk' } });
    expect(files()).toEqual(['Sample_pak03_dir.vpk']);
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search hero, file or label' }), { target: { value: 'geist' } });
    expect(files()).toEqual(['pak06_dir.vpk', 'pak09_dir.vpk']);
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search hero, file or label' }), { target: { value: 'pak07' } });
    expect(files()).toEqual(['pak07_dir.vpk']);
    fireEvent.click(heroes.getByRole('button', { name: /^Items/ }));
    expect(screen.getByText('No installed file matches these filters.')).toBeVisible();
  });

  it('shows the details of a file and keeps a label against its contents', async () => {
    const onOpenProject = vi.fn();
    api.scanInstalledMods.mockResolvedValue(mods);
    api.setInstalledModLabel.mockResolvedValue('Friend skin');
    render(<InstalledMods onOpenProject={onOpenProject} />);
    fireEvent.click(screen.getByRole('button', { name: 'Check installed mods' }));
    await screen.findByRole('list', { name: 'Installed mods' });
    fireEvent.click(within(row('Sample_pak03_dir.vpk')).getByRole('button', { name: /Details/ }));
    const details = within(screen.getByRole('region', { name: 'Details of Sample_pak03_dir.vpk' }));
    expect(details.getByText('Made here')).toBeVisible();
    expect(details.getByText('1 character model, 1 gun, 14 materials, 24 textures · 40 files')).toBeVisible();
    expect(details.getByText(/by this app from the project Sample for Haze/)).toBeVisible();
    expect(details.getByText('1 file also in pak09_dir.vpk')).toBeVisible();
    expect(details.getByText(/Which version a match uses is not decided here/)).toBeVisible();
    expect(details.getByRole('textbox', { name: 'Your label' })).toHaveValue('Main skin');
    fireEvent.click(details.getByRole('button', { name: 'Open its project' }));
    expect(onOpenProject).toHaveBeenCalledWith('C:\\Projects\\Sample.dmb.json');
    fireEvent.click(details.getByRole('button', { name: 'Show its project file' }));
    expect(api.showProjectFile).toHaveBeenCalledWith('C:\\Projects\\Sample.dmb.json');
    fireEvent.click(details.getByRole('button', { name: 'Show in folder' }));
    expect(api.showInstalledMod).toHaveBeenCalledWith(mods[0].path);
    // A file from elsewhere: no project buttons, and a label typed for it is saved by identity.
    fireEvent.click(within(row('pak09_dir.vpk')).getByRole('button', { name: 'Add a label to pak09_dir.vpk' }));
    const other = within(screen.getByRole('region', { name: 'Details of pak09_dir.vpk' }));
    expect(other.getByText('From elsewhere')).toBeVisible();
    expect(other.queryByRole('button', { name: 'Open its project' })).toBeNull();
    const input = other.getByRole('textbox', { name: 'Your label' });
    await waitFor(() => expect(input).toHaveFocus());
    fireEvent.change(input, { target: { value: '  Friend skin ' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(api.setInstalledModLabel).toHaveBeenCalledWith(id(3), 'pak09_dir.vpk', '  Friend skin '));
    expect(await within(row('pak09_dir.vpk')).findByText('Friend skin')).toBeVisible();
    expect(other.getByText(/Saved\. Kept with the file's contents/)).toBeVisible();
    // An unreadable file cannot be labelled.
    fireEvent.click(within(row('pak08_dir.vpk')).getByRole('button', { name: /Details/ }));
    expect(screen.getByText('A file that cannot be read cannot be labelled.')).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Open the addons folder' }));
    expect(api.showInstalledMod).toHaveBeenLastCalledWith(undefined);
  });

  it('keeps a late label answer out of another file opened meanwhile', async () => {
    await scanned();
    let answer: (value: string | null) => void = () => undefined;
    api.setInstalledModLabel.mockReturnValueOnce(new Promise(resolve => { answer = resolve; }));
    fireEvent.click(within(row('pak09_dir.vpk')).getByRole('button', { name: 'Add a label to pak09_dir.vpk' }));
    const input = screen.getByRole('textbox', { name: 'Your label' });
    fireEvent.change(input, { target: { value: 'First' } });
    fireEvent.blur(input);
    await waitFor(() => expect(api.setInstalledModLabel).toHaveBeenCalledOnce());
    // Another file is opened before the answer comes back.
    fireEvent.click(within(row('pak05_dir.vpk')).getByRole('button', { name: /Details/ }));
    answer('First');
    expect(await within(row('pak09_dir.vpk')).findByText('First')).toBeVisible();
    const other = within(screen.getByRole('region', { name: 'Details of pak05_dir.vpk' }));
    expect(other.getByRole('textbox', { name: 'Your label' })).toHaveValue('');
    expect(other.queryByText(/Saved\./)).toBeNull();
  });

  it('updates one file from its row or the affected ones together after confirmation, and rescans', async () => {
    api.scanInstalledMods.mockResolvedValueOnce(mods).mockResolvedValueOnce(mods.map(mod => mod.status === 'needs_update' ? { ...mod, status: 'up_to_date' as const, note: '' } : mod));
    api.updateInstalledMods.mockResolvedValue([{ path: mods[0].path, file: mods[0].file, updated: ['models/heroes_staging/haze/haze.vmdl_c'], converted: [], backup: 'C:\\backups\\Sample_pak03_dir.vpk.bak' }]);
    render(<InstalledMods />);
    fireEvent.click(screen.getByRole('button', { name: 'Check installed mods' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Update Sample_pak03_dir.vpk' }));
    await waitFor(() => expect(api.updateInstalledMods).toHaveBeenCalledWith([mods[0].path]));
    expect(dialog.ask).toHaveBeenCalledOnce();
    expect(await screen.findByRole('status')).toHaveTextContent('Updated Sample_pak03_dir.vpk (1 model)');
    await waitFor(() => expect(screen.queryByRole('button', { name: /Update 1 mod/ })).toBeNull());
    expect(api.scanInstalledMods).toHaveBeenCalledTimes(2);
  });

  it('does nothing when the confirmation is declined and shows backend errors', async () => {
    api.scanInstalledMods.mockResolvedValue(mods);
    dialog.ask.mockResolvedValueOnce(false);
    render(<InstalledMods />);
    fireEvent.click(screen.getByRole('button', { name: 'Check installed mods' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Update 1 mod' }));
    await waitFor(() => expect(dialog.ask).toHaveBeenCalledOnce());
    expect(api.updateInstalledMods).not.toHaveBeenCalled();
    api.updateInstalledMods.mockRejectedValue({ message: 'Sample_pak03_dir.vpk is split into several files; rebuild the mod instead.' });
    fireEvent.click(screen.getByRole('button', { name: 'Update 1 mod' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('split into several files');
  });
});

describe('Installed mods wording', () => {
  const base = mods[3];
  it('names ability objects, several heroes and replies of an older backend', () => {
    const dagger = { ...base, contents: contents({ files: 3, kinds: { ability_object: 1, material: 1, texture: 1 }, heroes: [{ name: 'Haze', files: 3 }, { name: 'Vyper', files: 3 }, { name: 'Mina', files: 3 }], ability_objects: [{ path: 'models/abilities/sleep_dagger.vmdl_c', heroes: ['Haze', 'Mina', 'Vyper'], every_hero: false, known: true }] }) };
    expect(summary(dagger)).toBe('Ability object: sleep dagger (shared by 3 heroes)');
    expect(heroText(dagger)).toBe('Haze, Vyper, Mina');
    const many = { ...base, contents: contents({ files: 5, kinds: { icon: 5 }, heroes: ['A', 'B', 'C', 'D', 'E'].map(name => ({ name, files: 1 })) }) };
    expect(heroText(many)).toBe('A, B, C + 2 more');
    const menus = { ...base, contents: contents({ files: 2266, kinds: { menu: 2191, icon: 60 }, heroes: ['A', 'B', 'C', 'D', 'E'].map(name => ({ name, files: 2 })) }) };
    expect(heroText(menus)).toBe('A few files each for 5 heroes');
    const older = { ...mods[2], contents: undefined };
    expect(heroText(older)).toBe('Lady Geist');
    expect(summary({ ...base, contents: contents({ files: 2, kinds: { other: 2 }, unattributed: 2 }) })).toBe('Other files (2)');
    expect(fromText({ ...mods[0], made: { ...mods[0].made!, project_name: '' } })).toBe('Made here · a project of this app');
    expect(fromText({ ...mods[0], made: { project_id: '', project_name: '', hero: '', project_file: null, source: 'output', several: true } })).toBe('Made here · the same files as builds of several projects');
    expect(sharedText({ ...base, shared: [mods[0].shared![0], mods[2].shared![0]] })).toBe('Changes some of the same game files as 2 other files');
    expect(stateOf({ ...base, status: 'cannot_update' }).label).toBe('Rebuild needed');
  });
});

describe('Installed mods: item names files', () => {
  const names = (files: string[], note = ''): InstalledMod => ({
    path: `${ADDONS}pak20_dir.vpk`, file: 'pak20_dir.vpk', bytes: 30000, status: 'no_hero_model', note, models: [], identity: id(7),
    contents: contents({ files: files.length, kinds: { item_names: files.length }, every_hero: files.length, samples: { item_names: files.map(f => `resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_${f}.txt`) } }),
  });
  it('names the item names files, counts them for every hero and under Items', async () => {
    expect(summary(names(['english']))).toBe('Item names (english)');
    expect(summary(names(['english', 'german', 'french']))).toBe('Item names · 3 languages');
    expect(heroText(names(['english']))).toBe('Every hero');
    const outdated = names(['english'], "1 of the game's item names is missing from this file (english): those items show without a name until the mod is built again.");
    api.scanInstalledMods.mockResolvedValue([outdated, mods[4]]);
    render(<InstalledMods />);
    fireEvent.click(screen.getByRole('button', { name: 'Check installed mods' }));
    await screen.findByRole('list', { name: 'Installed mods' });
    const heroes = within(screen.getByRole('group', { name: 'Hero' }));
    expect(heroes.getAllByRole('button').map(b => b.textContent)).toEqual(['All 2', 'Items 1', 'Other 1']);
    fireEvent.click(within(row('pak20_dir.vpk')).getByRole('button', { name: /Details/ }));
    const details = within(screen.getByRole('region', { name: 'Details of pak20_dir.vpk' }));
    expect(details.getByText(/1 of the game's item names is missing from this file \(english\)/)).toBeVisible();
    expect(details.getByText('1 item names file · 1 file')).toBeVisible();
  });
});
