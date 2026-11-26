import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// The shared frame and common parts in Chinese: only the words they own are translated; what
// pages pass in, and what is copied or reported, stays as it was given.
const mocks = vi.hoisted(() => ({ validateAssetPath: vi.fn(), open: vi.fn(), writeText: vi.fn() }));
vi.mock('../api', async original => ({ ...await original<typeof import('../api')>(), api: { validateAssetPath: mocks.validateAssetPath } }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open }));
import { StudioBarMenuButton, StudioBarMenuPanel } from './StudioBarMenu';
import { ErrorNotice, Status } from '../components/Primitives';
import { FilePathInput } from '../components/FilePathInput';
import { MIRROR_KEY, setLanguage } from '../i18n';

function Menu() {
  const [open, setOpen] = useState(true);
  return <>
    <StudioBarMenuButton base="c" id="notes" open={open} onToggle={() => setOpen(!open)} label="Notes" tone="attention"/>
    {open && <StudioBarMenuPanel base="c" id="notes" title="Notes" onClose={() => setOpen(false)}><p>Two notes</p></StudioBarMenuPanel>}
  </>;
}
beforeEach(async () => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: mocks.writeText } });
  mocks.writeText.mockResolvedValue(undefined);
  await act(() => setLanguage('zh-CN'));
});
afterEach(async () => { cleanup(); await act(() => setLanguage('en')); localStorage.removeItem(MIRROR_KEY); });

describe('Shared parts in Chinese', () => {
  it('a bar menu panel closes by its Chinese Close button and keeps the page’s title as given', () => {
    render(<Menu/>);
    expect(screen.getByRole('button', { name: /Notes（需要查看）/ })).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Notes' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '关闭“Notes”' }));
    expect(screen.queryByRole('region')).toBeNull();
  });
  it('the error notice copies the English title for support while it shows the translated one', async () => {
    render(<ErrorNotice title="Could not save the project" shownTitle="无法保存项目" details="disk full" help={{ cause: 'The disk is full.', steps: ['Free some space.'], action: null }}/>);
    expect(screen.getByText('无法保存项目')).toBeInTheDocument();
    expect(screen.getByText('解决方法')).toBeInTheDocument();
    expect(screen.getByText('The disk is full.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '复制支持信息' }));
    await waitFor(() => expect(mocks.writeText).toHaveBeenCalledWith('Could not save the project\n\nThe disk is full.\n\ndisk full'));
    expect(await screen.findByRole('button', { name: '已复制' })).toBeInTheDocument();
  });
  it('a step status reads in Chinese and switches back live', async () => {
    render(<Status status="cancelled"/>);
    expect(screen.getByText('已取消')).toBeInTheDocument();
    await act(() => setLanguage('en'));
    expect(screen.getByText('Cancelled')).toBeInTheDocument();
  });
  it('a file field keeps its English name in the failure title when its label is translated', async () => {
    const fail = vi.fn(), update = vi.fn();
    mocks.validateAssetPath.mockRejectedValue({ kind: 'input', message: 'Not a gun file' });
    render(<FilePathInput label="另一文件中的枪械（可选）" name="Gun from another file (optional)" kind="weapon" value="" disabled={false} fail={fail} update={update}/>);
    expect(screen.getByPlaceholderText('将文件拖放到这里，或浏览…')).toBeInTheDocument();
    mocks.open.mockResolvedValue('C:\\Models\\gun.txt');
    fireEvent.click(screen.getByRole('button', { name: '浏览：另一文件中的枪械（可选）' }));
    await waitFor(() => expect(fail).toHaveBeenCalledWith('Could not select gun from another file (optional)', { kind: 'input', message: 'Not a gun file' }));
    expect(mocks.open).toHaveBeenCalledWith(expect.objectContaining({ title: '另一文件中的枪械（可选）' }));
    expect(update).not.toHaveBeenCalled();
  });
});
