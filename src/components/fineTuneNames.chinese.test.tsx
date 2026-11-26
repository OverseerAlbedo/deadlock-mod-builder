import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import { MIRROR_KEY, setLanguage } from '../i18n';

// Physics & preview in Chinese: a changed part-type chip is named in one piece, with no space
// before the full-width comma, and a part's movement choice "Automatic" fits its narrow select.
const native = vi.hoisted(() => ({physicsPresets: vi.fn(), savePhysicsPreset: vi.fn(), deletePhysicsPreset: vi.fn(), validateAssetPath: vi.fn(), motionPreview: vi.fn(), previewImage: vi.fn(), fileStamps: vi.fn(), previewCloth: vi.fn()}));
vi.mock('../api', () => ({api: native, basename: (s: string) => s}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));
vi.mock('../preview/Viewport', () => ({default: () => <div data-testid="still"/>}));
vi.mock('../preview/MovementViewport', () => ({default: () => <div data-testid="movement"/>}));
import { PhysicsPage } from './CustomizationPages';
import { viewChoices } from '../preview/viewChoices';

const original = structuredClone(reference.cases[0].expected) as unknown as Project;
const start = {...original, preparation_options: {preserve_physics_bones: true, physics_roots: ['hair_a', 'tail_1']}, customization: {enabled: true, secondary_motion: {enabled: true}}} as unknown as Project;
function Harness() {
  const [project, update] = useState<Project>(start);
  return <PhysicsPage project={project} update={update} disabled={false} fail={vi.fn()} summary={null}/>;
}
const chips = () => within(screen.getByRole('tablist', {name: /^(要微调的部件类型|Part types to fine-tune)$/}));

beforeEach(async () => {
  vi.clearAllMocks(); viewChoices.clear();
  native.physicsPresets.mockResolvedValue([]);
  await act(() => setLanguage('zh-CN'));
});
afterEach(async () => { cleanup(); await act(() => setLanguage('en')); localStorage.removeItem(MIRROR_KEY); });

describe('Fine-tune chips and movement choices in Chinese', () => {
  it('names a changed chip “尾巴，已更改 1 项” and the same chip “Tails, 1 changed” in English', async () => {
    render(<Harness/>);
    fireEvent.click(chips().getByRole('tab', {name: /^尾巴/}));
    fireEvent.change(screen.getByRole('slider', {name: '尾巴硬度'}), {target: {value: '40'}});
    expect(chips().getByRole('tab', {name: '尾巴，已更改 1 项'})).toHaveAttribute('aria-selected', 'true');
    await act(() => setLanguage('en'));
    expect(chips().getByRole('tab', {name: 'Tails, 1 changed'})).toHaveAttribute('aria-selected', 'true');
  });
  it('offers 自动 as a part’s automatic movement and still saves the identifier', () => {
    render(<Harness/>);
    fireEvent.click(screen.getByRole('tab', {name: /^活动部件/}));
    const select = screen.getByLabelText('hair_a 的摆动方式');
    expect(within(select).getByRole('option', {name: '自动'})).toHaveValue('automatic');
  });
});
