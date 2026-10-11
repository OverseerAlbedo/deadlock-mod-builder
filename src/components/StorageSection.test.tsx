import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Project, Settings, StorageReport } from '../models';

const mocks = vi.hoisted(() => ({storageReport: vi.fn(), storageClear: vi.fn(), storageFindProjects: vi.fn(), ask: vi.fn(), open: vi.fn()}));
vi.mock('../api', async original => ({...await original<typeof import('../api')>(), api: mocks}));
vi.mock('@tauri-apps/plugin-dialog', () => ({ask: mocks.ask, open: mocks.open}));
import { StorageSection, formatBytes } from './StorageSection';

const settings = {workspace: 'C:\\Work', keep_backups: 1, keep_previous_preparation: true, clear_build_data_after_build: false} as Settings;
const open = {id: 'a'.repeat(32), name: 'Ghist'} as Project;
const report: StorageReport = {
  workspace: 'C:\\Work', bytes: 3 * 1024 ** 3, reclaimable: 2 * 1024 ** 3, other_bytes: 0,
  projects: [
    {id: 'a'.repeat(32), name: 'Ghist', hero: 'Lady Geist', project_file: 'C:\\Projects\\ghist.dmb.json', open: true, locked: false, workspace: 'C:\\Work\\aaa', modified: null, bytes: 2 * 1024 ** 3, reclaimable: 1024 ** 3,
      usage: [{kind: 'build_data', bytes: 1024 ** 3, reclaimable: 1024 ** 3}, {kind: 'output', bytes: 1024 ** 3, reclaimable: 0}]},
    {id: 'b'.repeat(32), name: '', hero: '', project_file: null, open: false, locked: false, workspace: 'C:\\Work\\bbb', modified: null, bytes: 1024 ** 3, reclaimable: 1024 ** 3,
      usage: [{kind: 'preparation', bytes: 1024 ** 3, reclaimable: 1024 ** 3}]},
  ],
  shared: [{key: 'k', model: 'models/heroes/spectre/spectre.vmdl', bytes: 90 * 1024 ** 2, used_by: []}],
};
const operate = async <T,>(_label: string, operation: () => Promise<T>) => operation();

describe('Storage settings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.storageReport.mockResolvedValue(report);
    mocks.ask.mockResolvedValue(true);
    mocks.storageClear.mockResolvedValue({result: {removed_bytes: 1024 ** 3, removed: ['x'], skipped: []}, report});
  });
  it('formats sizes', () => {
    expect(formatBytes(512 * 1024)).toBe('512 KB');
    expect(formatBytes(5.5 * 1024 ** 2)).toBe('5.5 MB');
    expect(formatBytes(21.3 * 1024 ** 3)).toBe('21.30 GB');
  });
  it('measures, clears one kind and protects the open project', async () => {
    const update = vi.fn();
    render(<StorageSection settings={settings} update={update} project={open} busy={false} dirty={false} operate={operate}/>);
    fireEvent.change(screen.getByLabelText('Backups kept per file'), {target: {value: '2'}});
    expect(update).toHaveBeenCalledWith(expect.objectContaining({keep_backups: 2}));
    fireEvent.click(screen.getByRole('button', {name: 'Measure storage'}));
    await screen.findByText('Ghist · Lady Geist');
    expect(mocks.storageReport).toHaveBeenCalledWith(open);
    expect(screen.getByText('Open now')).toBeInTheDocument();
    expect(screen.getByText('No saved project file known')).toBeInTheDocument();
    const rows = screen.getAllByRole('row');
    const ghist = rows.find(row => within(row).queryByText('Ghist · Lady Geist'))!;
    expect(within(ghist).getByRole('button', {name: 'Remove…'})).toBeDisabled();
    fireEvent.click(within(ghist).getByRole('button', {name: 'Details'}));
    expect(screen.getByText('Kept')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name: 'Clear 1.00 GB'}));
    await waitFor(() => expect(mocks.storageClear).toHaveBeenCalledWith({project: 'a'.repeat(32), kinds: ['build_data']}, open));
    await screen.findByText('Removed 1.00 GB.');
    fireEvent.click(screen.getByRole('button', {name: 'Remove unused'}));
    await waitFor(() => expect(mocks.storageClear).toHaveBeenLastCalledWith({unused_shared: true}, open));
  });
  it('does nothing when a removal is not confirmed', async () => {
    mocks.ask.mockResolvedValue(false);
    render(<StorageSection settings={settings} update={vi.fn()} project={null} busy={false} dirty={true} operate={operate}/>);
    expect(screen.getByText('Save settings to apply these choices.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name: 'Measure storage'}));
    await screen.findByText('Project bbbbbbbb');
    const row = screen.getAllByRole('row').find(item => within(item).queryByText('Project bbbbbbbb'))!;
    fireEvent.click(within(row).getByRole('button', {name: 'Remove…'}));
    await waitFor(() => expect(mocks.ask).toHaveBeenCalled());
    expect(mocks.storageClear).not.toHaveBeenCalled();
  });
});
