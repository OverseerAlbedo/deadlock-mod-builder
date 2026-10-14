import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import { inspected } from '../build/testFixtures';

const mocks = vi.hoisted(() => ({physicsPresets: vi.fn(), savePhysicsPreset: vi.fn(), deletePhysicsPreset: vi.fn(), validateAssetPath: vi.fn()}));
vi.mock('../api', () => ({api: mocks, basename: (s: string) => s}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));
import { SecondaryMotion } from './SecondaryMotion';
import { PhysicsPresets, TuningSliders, withTuning } from './MotionTuning';

const original = structuredClone(reference.cases[0].expected) as unknown as Project;

describe('Fine-tune movement', () => {
  it('keeps only changed values and drops a reset part type', () => {
    expect(withTuning(undefined, 'hair', {stiffness: .3, damping: 0})).toEqual({hair: {stiffness: .3}});
    expect(withTuning({hair: {stiffness: .3}, tail: {swing: -.5}}, 'hair', null)).toEqual({tail: {swing: -.5}});
  });
  it('moves one setting of one part type from the automatic value', () => {
    const update = vi.fn();
    const {rerender} = render(<TuningSliders kind="hair" cfg={{}} update={update} disabled={false}/>);
    expect(screen.getAllByText('Automatic')).toHaveLength(4);
    fireEvent.change(screen.getByLabelText('Hair stiffness'), {target: {value: '30'}});
    expect(update).toHaveBeenCalledWith({tuning: {hair: {stiffness: .3}}});
    rerender(<TuningSliders kind="hair" cfg={{tuning: {hair: {stiffness: .3}, tail: {gravity: -1}}}} update={update} disabled={false}/>);
    expect(screen.getByText('+30%')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name: 'Reset'}));
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {gravity: -1}}});
  });
  it('lists the analyzed part types of the model on the Physics tab', async () => {
    mocks.physicsPresets.mockResolvedValue([]);
    const project = {...original, preparation_options: {preserve_physics_bones: true, physics_roots: ['bangs', 'tail_root']}, customization: {enabled: true, secondary_motion: {enabled: true}}};
    const report = {...inspected, secondary_motion: {preset: 'automatic', source_sha256: 's', proxy_sha256: 'p', proxy_positions: 18, groups: [], parts: [
      {root: 'bangs', kind: 'hair', reason: 'Named hair', roll_locked: true, review_recommended: false},
      {root: 'tail_root', kind: 'tail', reason: 'Long round strand', roll_locked: true, review_recommended: false}]}} as unknown as typeof inspected;
    render(<SecondaryMotion project={project as Project} update={vi.fn()} disabled={false} report={report} fresh/>);
    fireEvent.click(screen.getByText('Fine-tune movement', {selector: 'summary > span'}));
    expect(screen.getByRole('group', {name: 'Fine-tune Hair'})).toBeInTheDocument();
    expect(screen.getByRole('group', {name: 'Fine-tune Tails'})).toBeInTheDocument();
    expect(screen.queryByRole('group', {name: 'Fine-tune Breasts'})).not.toBeInTheDocument();
    await waitFor(() => expect(mocks.physicsPresets).toHaveBeenCalled());
  });
  it('offers every common part type before the model is analyzed', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    const project = {...original, preparation_options: {preserve_physics_bones: true, physics_roots: ['胸上2.L', 'hair_a'], physics_kinds: {'胸上2.L': 'breast'}}, customization: {enabled: true, secondary_motion: {enabled: true}}};
    render(<SecondaryMotion project={project as Project} update={vi.fn()} disabled={false} report={null} fresh={false}/>);
    fireEvent.click(screen.getByText('Fine-tune movement', {selector: 'summary > span'}));
    for (const name of ['Hair', 'Tails', 'Cloth', 'Breasts']) expect(screen.getByRole('group', {name: `Fine-tune ${name}`})).toBeInTheDocument();
    expect(screen.getByText(/Showing every common part type/)).toBeInTheDocument();
  });
  it('saves the tuning as a preset and applies a saved one', async () => {
    const soft = {name: 'Soft hair', tuning: {hair: {stiffness: -.4}}};
    mocks.physicsPresets.mockResolvedValue([soft]);
    mocks.savePhysicsPreset.mockResolvedValue([soft, {name: 'Mine', tuning: {tail: {swing: .5}}}]);
    const update = vi.fn();
    render(<PhysicsPresets cfg={{tuning: {tail: {swing: .5}}}} update={update} disabled={false}/>);
    fireEvent.change(await screen.findByLabelText('Apply a saved physics preset'), {target: {value: 'Soft hair'}});
    expect(update).toHaveBeenCalledWith({tuning: {hair: {stiffness: -.4}}});
    fireEvent.change(screen.getByLabelText('Preset name'), {target: {value: 'Mine'}});
    fireEvent.click(screen.getByRole('button', {name: 'Save as preset'}));
    await waitFor(() => expect(mocks.savePhysicsPreset).toHaveBeenCalledWith('Mine', {tail: {swing: .5}}));
    expect(await screen.findByText('Saved “Mine”.')).toBeInTheDocument();
  });
});

describe('Source rig springs', () => {
  it('offers the model\'s own spring settings as starting values, off by default', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    const update = vi.fn();
    const project = {...original, preparation_options: {preserve_physics_bones: true, physics_roots: ['hair_a', 'tail'], physics_springs: {hair_a: {source: 'vrm', goal: .45, drag: .2, gravity: 0}, tail: {source: 'vrm', goal: .9, drag: .1, gravity: .5}}}, customization: {enabled: true, secondary_motion: {enabled: true}}};
    render(<SecondaryMotion project={project as Project} update={update} disabled={false} report={null} fresh={false}/>);
    fireEvent.click(screen.getByText('Fine-tune movement', {selector: 'summary > span'}));
    const box = screen.getByLabelText("Start from your model's own VRM spring settings (2 parts)");
    expect(box).not.toBeChecked();
    fireEvent.click(box);
    expect(update).toHaveBeenCalledWith({use_source_springs: true});
  });
});
