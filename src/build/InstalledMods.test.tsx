import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { InstalledMod } from '../models';
import { InstalledMods } from './InstalledMods';

const api = vi.hoisted(() => ({ scanInstalledMods: vi.fn(), updateInstalledMods: vi.fn() }));
const dialog = vi.hoisted(() => ({ ask: vi.fn() }));
vi.mock('../api', () => ({ api }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: dialog.ask }));

const mods: InstalledMod[] = [
  { path: 'G:\\Deadlock\\game\\citadel\\addons\\Yvvone_pak03_dir.vpk', file: 'Yvvone_pak03_dir.vpk', bytes: 132644291, status: 'needs_update', note: 'Built before the game\'s October 16, 2026 update: camera pivots are rotated in a match.', models: [{ path: 'models/heroes_staging/haze/haze.vmdl_c', hero: 'Haze', convention: 'previous' }] },
  { path: 'G:\\Deadlock\\game\\citadel\\addons\\pak06_dir.vpk', file: 'pak06_dir.vpk', bytes: 45102280, status: 'no_hero_model', note: '', models: [] },
  { path: 'G:\\Deadlock\\game\\citadel\\addons\\pak09_dir.vpk', file: 'pak09_dir.vpk', bytes: 178717543, status: 'up_to_date', note: '', models: [{ path: 'models/heroes_wip/geist/geist.vmdl_c', hero: 'Lady Geist', convention: 'current' }] },
];
beforeEach(() => { vi.clearAllMocks(); dialog.ask.mockResolvedValue(true); });

describe('Installed mods after the October 16, 2026 game update', () => {
  it('lists every VPK with its hero and state, and offers an update only for the affected ones', async () => {
    api.scanInstalledMods.mockResolvedValue(mods);
    render(<InstalledMods />);
    expect(screen.queryByRole('button', { name: /Update/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Check installed mods' }));
    expect(await screen.findByText('Needs update')).toBeVisible();
    expect(screen.getByText('Haze')).toBeVisible();
    expect(screen.getByText('Lady Geist')).toBeVisible();
    expect(screen.getByText('No hero model')).toBeVisible();
    expect(screen.getByText('Up to date')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Update 1 mod' })).toBeEnabled();
  });
  it('updates the affected VPKs after confirmation and rescans', async () => {
    api.scanInstalledMods.mockResolvedValueOnce(mods).mockResolvedValueOnce(mods.map(mod => mod.status === 'needs_update' ? { ...mod, status: 'up_to_date' as const, note: '' } : mod));
    api.updateInstalledMods.mockResolvedValue([{ path: mods[0].path, file: mods[0].file, updated: ['models/heroes_staging/haze/haze.vmdl_c'], converted: [], backup: 'C:\\backups\\Yvvone_pak03_dir.vpk.bak' }]);
    render(<InstalledMods />);
    fireEvent.click(screen.getByRole('button', { name: 'Check installed mods' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Update 1 mod' }));
    await waitFor(() => expect(api.updateInstalledMods).toHaveBeenCalledWith([mods[0].path]));
    expect(dialog.ask).toHaveBeenCalledOnce();
    expect(await screen.findByRole('status')).toHaveTextContent('Updated Yvvone_pak03_dir.vpk (1 model)');
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
    api.updateInstalledMods.mockRejectedValue({ message: 'Yvvone_pak03_dir.vpk is split into several files; rebuild the mod instead.' });
    fireEvent.click(screen.getByRole('button', { name: 'Update 1 mod' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('split into several files');
  });
});
