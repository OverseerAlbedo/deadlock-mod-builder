import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project, SecondaryMotionChoices } from '../models';
import { inspected } from '../build/testFixtures';

const mocks = vi.hoisted(() => ({physicsPresets: vi.fn(), savePhysicsPreset: vi.fn(), deletePhysicsPreset: vi.fn(), validateAssetPath: vi.fn()}));
vi.mock('../api', () => ({api: mocks, basename: (s: string) => s}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));
import { MotionFineTune, MotionStatus, MotionTools, MovingParts, type MotionProps } from './SecondaryMotion';
import { parsePercent, PhysicsPresets, TuningSliders, withTuning } from './MotionTuning';
/** The Automatic motion controls together: on the page they sit in the top bar and three tabs. */
function SecondaryMotion(props: MotionProps) { return <><MotionTools {...props}/><MotionFineTune {...props}/><MovingParts {...props}/><MotionStatus {...props}/></>; }

const original = structuredClone(reference.cases[0].expected) as unknown as Project;
const motion = (roots: string[], choices: SecondaryMotionChoices = {}, options: Record<string, unknown> = {}) =>
  ({...original, preparation_options: {preserve_physics_bones: true, physics_roots: roots, ...options}, customization: {enabled: true, secondary_motion: {enabled: true, ...choices}}}) as unknown as Project;
const tabs = () => screen.getAllByRole('tab').map(tab => tab.textContent);
const chip = (name: string) => screen.getByRole('tab', {name: new RegExp(`^${name}`)});

describe('Fine-tune movement', () => {
  it('keeps only changed values and drops a reset part type', () => {
    expect(withTuning(undefined, 'hair', {stiffness: .3, damping: 0})).toEqual({hair: {stiffness: .3}});
    expect(withTuning({hair: {stiffness: .3}, tail: {swing: -.5}}, 'hair', null)).toEqual({tail: {swing: -.5}});
  });
  it('moves one setting of one part type from the automatic value, in 5% steps when dragged', () => {
    const update = vi.fn();
    const {rerender} = render(<TuningSliders kind="hair" cfg={{}} update={update} disabled={false}/>);
    expect(screen.getAllByText('Automatic')).toHaveLength(4);
    expect(screen.getAllByDisplayValue('Auto')).toHaveLength(4);
    fireEvent.change(screen.getByLabelText('Hair stiffness'), {target: {value: '30'}});
    expect(update).toHaveBeenCalledWith({tuning: {hair: {stiffness: .3}}});
    fireEvent.change(screen.getByLabelText('Hair damping'), {target: {value: '37'}});
    expect(update).toHaveBeenLastCalledWith({tuning: {hair: {damping: .35}}});
    rerender(<TuningSliders kind="hair" cfg={{tuning: {hair: {stiffness: .3}, tail: {gravity: -1}}}} update={update} disabled={false}/>);
    expect(screen.getByLabelText('Hair stiffness in percent')).toHaveValue('+30%');
    expect(screen.getByLabelText('Hair stiffness')).toHaveAttribute('aria-valuetext', '+30%');
    fireEvent.click(screen.getByRole('button', {name: 'Hair stiffness: back to automatic'}));
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {gravity: -1}}});
    fireEvent.doubleClick(screen.getByLabelText('Hair stiffness'));
    expect(update).toHaveBeenCalledTimes(4);
  });
  it('takes an exact percentage typed in the value box', () => {
    const update = vi.fn();
    render(<TuningSliders kind="tail" cfg={{tuning: {tail: {gravity: .2}}}} update={update} disabled={false}/>);
    const box = screen.getByLabelText('Tails stiffness in percent');
    fireEvent.focus(box);
    expect(box).toHaveValue('0');
    fireEvent.change(box, {target: {value: '37'}});
    fireEvent.keyDown(box, {key: 'Enter'});
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {gravity: .2, stiffness: .37}}});
    fireEvent.blur(box);
    expect(update).toHaveBeenCalledTimes(1);
    fireEvent.focus(box); fireEvent.change(box, {target: {value: '250'}}); fireEvent.blur(box);
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {gravity: .2, stiffness: 1}}});
    fireEvent.focus(box); fireEvent.change(box, {target: {value: '-37%'}}); fireEvent.blur(box);
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {gravity: .2, stiffness: -.37}}});
    const gravity = screen.getByLabelText('Tails gravity in percent');
    fireEvent.focus(gravity); fireEvent.change(gravity, {target: {value: 'lots'}}); fireEvent.blur(gravity);
    expect(gravity).toHaveValue('+20%');
    fireEvent.focus(gravity); fireEvent.change(gravity, {target: {value: '60'}}); fireEvent.keyDown(gravity, {key: 'Escape'});
    expect(gravity).toHaveValue('+20%');
    expect(update).toHaveBeenCalledTimes(3);
  });
  it('reads typed percentages', () => {
    expect(parsePercent('37')).toBe(37);
    expect(parsePercent('+37%')).toBe(37);
    expect(parsePercent('−12')).toBe(-12);
    expect(parsePercent('250')).toBe(100);
    expect(parsePercent('-300')).toBe(-100);
    expect(parsePercent('auto')).toBe(0);
    expect(parsePercent('12.6')).toBe(13);
    expect(parsePercent('')).toBeNull();
    expect(parsePercent('abc')).toBeNull();
  });
  it('keeps a typed value exact and moves the arrow keys from it to the next 5% step', () => {
    const update = vi.fn();
    render(<TuningSliders kind="tail" cfg={{tuning: {tail: {stiffness: .37}}}} update={update} disabled={false}/>);
    const slider = screen.getByLabelText('Tails stiffness');
    expect(slider).toHaveValue('37');
    fireEvent.keyDown(slider, {key: 'ArrowRight'});
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {stiffness: .4}}});
    fireEvent.keyDown(slider, {key: 'ArrowLeft'});
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {stiffness: .35}}});
    fireEvent.keyDown(slider, {key: 'ArrowUp', shiftKey: true});
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {stiffness: .38}}});
    fireEvent.keyDown(slider, {key: 'End'});
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {stiffness: 1}}});
    fireEvent.keyDown(slider, {key: 'Home'});
    expect(update).toHaveBeenLastCalledWith({tuning: {tail: {stiffness: -1}}});
  });
  it('lists the analyzed part types of the model as chips', async () => {
    mocks.physicsPresets.mockResolvedValue([]);
    const report = {...inspected, secondary_motion: {preset: 'automatic', source_sha256: 's', proxy_sha256: 'p', proxy_positions: 18, groups: [], parts: [
      {root: 'bangs', kind: 'hair', reason: 'Named hair', roll_locked: true, review_recommended: false},
      {root: 'tail_root', kind: 'tail', reason: 'Long round strand', roll_locked: true, review_recommended: false}]}} as unknown as typeof inspected;
    render(<SecondaryMotion project={motion(['bangs', 'tail_root'])} update={vi.fn()} disabled={false} report={report} fresh/>);
    // Breasts stays offered: its switch asks Model preparation for breast bones.
    expect(tabs()).toEqual(['Hair', 'Tails', 'Breasts Off']);
    expect(screen.getByRole('group', {name: 'Fine-tune Hair'})).toBeInTheDocument();
    fireEvent.click(chip('Tails'));
    expect(screen.getByRole('group', {name: 'Fine-tune Tails'})).toBeInTheDocument();
    expect(screen.queryByRole('group', {name: 'Fine-tune Hair'})).not.toBeInTheDocument();
    fireEvent.click(chip('Breasts'));
    expect(screen.queryByRole('group', {name: 'Fine-tune Breasts'})).not.toBeInTheDocument();
    expect(screen.getByLabelText('Breast motion')).not.toBeChecked();
    await waitFor(() => expect(mocks.physicsPresets).toHaveBeenCalled());
  });
  it('offers every common part type before the model is analyzed', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    render(<SecondaryMotion project={motion(['胸上2.L', 'hair_a'], {}, {physics_kinds: {'胸上2.L': 'breast'}})} update={vi.fn()} disabled={false} report={null} fresh={false}/>);
    expect(tabs()).toEqual(['Hair', 'Streamers and ribbons', 'Tails', 'Ears', 'Cloth', 'Long skirts', 'Accessories', 'Breasts']);
    fireEvent.click(chip('Breasts'));
    expect(screen.getByRole('group', {name: 'Fine-tune Breasts'})).toBeInTheDocument();
    expect(screen.getByText(/Showing every common part type/)).toBeInTheDocument();
  });
  it('moves between part types with the arrow keys, Home and End', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    render(<SecondaryMotion project={motion(['hair_a'])} update={vi.fn()} disabled={false} report={null} fresh={false}/>);
    const list = screen.getByRole('tablist', {name: 'Part types to fine-tune'});
    expect(chip('Hair')).toHaveAttribute('aria-selected', 'true');
    expect(chip('Hair')).toHaveAttribute('tabindex', '0');
    expect(chip('Tails')).toHaveAttribute('tabindex', '-1');
    fireEvent.keyDown(list, {key: 'ArrowRight'});
    expect(chip('Streamers')).toHaveAttribute('aria-selected', 'true');
    expect(chip('Streamers')).toHaveFocus();
    expect(screen.getByRole('tabpanel')).toHaveAccessibleName(/^Streamers and ribbons/);
    fireEvent.keyDown(list, {key: 'End'});
    expect(chip('Breasts')).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(list, {key: 'ArrowRight'});
    expect(chip('Hair')).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(list, {key: 'ArrowLeft'});
    expect(chip('Breasts')).toHaveFocus();
    fireEvent.keyDown(list, {key: 'Home'});
    expect(chip('Hair')).toHaveFocus();
  });
  it('marks changed types, counts every changed slider and resets one type or all', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    const update = vi.fn();
    render(<SecondaryMotion project={motion(['hair_a', 'tail_1'], {tuning: {tail: {stiffness: -.75, gravity: .95}, hair: {damping: .35}}})} update={update} disabled={false} report={null} fresh={false}/>);
    expect(screen.getByText('3 changed')).toBeInTheDocument();
    expect(chip('Hair')).toHaveAccessibleName('Hair, 1 changed');
    expect(chip('Tails')).toHaveAccessibleName('Tails, 2 changed');
    expect(chip('Cloth')).toHaveAccessibleName('Cloth');
    // The first changed type opens first.
    expect(chip('Hair')).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(chip('Tails'));
    const panel = screen.getByRole('tabpanel');
    expect(within(panel).getByText('2 changed')).toBeInTheDocument();
    fireEvent.click(within(panel).getByRole('button', {name: 'Reset tails'}));
    expect(update).toHaveBeenLastCalledWith({tuning: {hair: {damping: .35}}});
    fireEvent.click(chip('Cloth'));
    expect(within(screen.getByRole('tabpanel')).queryByRole('button', {name: /Reset cloth/})).toBeNull();
    fireEvent.click(screen.getByRole('button', {name: 'Reset all…'}));
    const dialog = screen.getByRole('alertdialog', {name: 'Reset all fine-tuning'});
    expect(dialog).toHaveTextContent('3 settings across all part types will be cleared');
    fireEvent.click(within(dialog).getByRole('button', {name: 'Keep my values'}));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    fireEvent.click(screen.getByRole('button', {name: 'Reset all…'}));
    fireEvent.click(within(screen.getByRole('alertdialog')).getByRole('button', {name: 'Reset all'}));
    expect(update).toHaveBeenLastCalledWith({tuning: {}});
  });
  it('says when every slider is at automatic', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    render(<SecondaryMotion project={motion(['hair_a'])} update={vi.fn()} disabled={false} report={null} fresh={false}/>);
    expect(screen.getByText('All automatic')).toHaveAttribute('title', 'Every slider is at automatic');
    expect(screen.queryByRole('button', {name: 'Reset all…'})).toBeNull();
  });
  it('keeps the chip of a switched-off butt and switches it back on', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    const update = vi.fn();
    const off = motion(['glute_l', 'hair_a'], {part_kinds: {glute_l: 'rigid'}, tuning: {butt: {swing: .5}}});
    const {rerender} = render(<SecondaryMotion project={off} update={update} disabled={false} report={null} fresh={false}/>);
    expect(chip('Butt')).toHaveAccessibleName('Butt Off, 1 changed');
    fireEvent.click(chip('Butt'));
    expect(screen.getByLabelText('Butt motion')).not.toBeChecked();
    // Its sliders stay visible but do nothing while it is off; its values can still be reset.
    expect(screen.getByLabelText('Butt swing range')).toBeDisabled();
    expect(screen.getByText(/Butt motion is off, so these sliders do nothing/)).toBeInTheDocument();
    expect(screen.getByRole('button', {name: 'Reset butt'})).toBeEnabled();
    fireEvent.click(screen.getByLabelText('Butt motion'));
    expect(update).toHaveBeenLastCalledWith({part_kinds: {glute_l: 'butt'}});
    rerender(<SecondaryMotion project={motion(['glute_l', 'hair_a'], {part_kinds: {glute_l: 'butt'}, tuning: {butt: {swing: .5}}})} update={update} disabled={false} report={null} fresh={false}/>);
    expect(chip('Butt')).toHaveAccessibleName('Butt, 1 changed');
    expect(screen.getByLabelText('Butt motion')).toBeChecked();
    expect(screen.getByLabelText('Butt swing range')).toBeEnabled();
  });
  it('shows only the preset explanation for a TangTang preset', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    render(<SecondaryMotion project={motion(['hair_a'], {preset: 'tangtang_r2'})} update={vi.fn()} disabled={false} report={null} fresh={false}/>);
    expect(screen.queryByRole('tablist')).toBeNull();
    expect(screen.queryByRole('slider')).toBeNull();
    expect(screen.queryByLabelText('Part type for hair_a')).toBeNull();
    expect(screen.getByText(/exact R2 setup/)).toBeInTheDocument();
  });
  it('shows the no-moving-parts message in place of the chips, keeping the breast switch', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    const preparation = vi.fn();
    render(<SecondaryMotion project={{...original, preparation_options: {}, customization: {enabled: true, secondary_motion: {enabled: true}}} as unknown as Project} update={vi.fn()} preparation={preparation} disabled={false} report={null} fresh={false}/>);
    expect(screen.queryByRole('tablist')).toBeNull();
    expect(screen.getByRole('status')).toHaveTextContent('No prepared physics groups');
    fireEvent.click(screen.getByLabelText('Breast motion'));
    expect(preparation).toHaveBeenCalledWith({breast_motion: true});
  });
  it('hides only the preset choice in Simple mode', async () => {
    mocks.physicsPresets.mockResolvedValue([{name: 'Soft hair', tuning: {hair: {stiffness: -.4}}}]);
    render(<SecondaryMotion expert={false} project={motion(['hair_a'], {tuning: {hair: {damping: .2}}})} update={vi.fn()} disabled={false} report={null} fresh={false}/>);
    expect(screen.queryByLabelText('Secondary motion preset')).toBeNull();
    expect(screen.queryByText(/Build creates and compiles motion/)).toBeNull();
    expect(screen.getByRole('tablist')).toBeInTheDocument();
    expect(screen.getByRole('button', {name: 'Reset all…'})).toBeInTheDocument();
    expect(await screen.findByRole('button', {name: 'Saved presets'})).toBeInTheDocument();
    expect(screen.getByRole('button', {name: 'Save as preset'})).toBeInTheDocument();
    expect(screen.getByLabelText('Part type for hair_a')).toBeInTheDocument();
  });
  it('saves the tuning as a preset, applies and deletes a saved one from the header popovers', async () => {
    const soft = {name: 'Soft hair', tuning: {hair: {stiffness: -.4}}};
    mocks.physicsPresets.mockResolvedValue([soft]);
    mocks.savePhysicsPreset.mockResolvedValue([soft, {name: 'Mine', tuning: {tail: {swing: .5}}}]);
    mocks.deletePhysicsPreset.mockResolvedValue([{name: 'Mine', tuning: {tail: {swing: .5}}}]);
    const update = vi.fn();
    render(<SecondaryMotion project={motion(['hair_a'], {tuning: {tail: {swing: .5}}})} update={update} disabled={false} report={null} fresh={false}/>);
    const saved = await screen.findByRole('button', {name: 'Saved presets'});
    fireEvent.click(saved);
    expect(saved).toHaveAttribute('aria-expanded', 'true');
    const list = screen.getByRole('dialog', {name: 'Saved presets'});
    expect(within(list).getByText('Hair')).toBeInTheDocument();
    expect(within(list).getByRole('button', {name: 'Apply Soft hair'})).toHaveFocus();
    fireEvent.click(within(list).getByRole('button', {name: 'Apply Soft hair'}));
    expect(update).toHaveBeenCalledWith({tuning: {hair: {stiffness: -.4}}});
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(saved).toHaveFocus();
    expect(screen.getByRole('status')).toHaveTextContent('Applied “Soft hair”. Analyze and Build to use it.');
    // Escape and a click outside close it.
    fireEvent.click(saved);
    fireEvent.keyDown(screen.getByRole('dialog'), {key: 'Escape'});
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(saved).toHaveFocus();
    fireEvent.click(saved);
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole('dialog')).toBeNull();
    const save = screen.getByRole('button', {name: 'Save as preset'});
    fireEvent.click(save);
    const form = screen.getByRole('dialog', {name: 'Save as preset'});
    expect(within(form).getByLabelText('Preset name')).toHaveFocus();
    expect(within(form).getByRole('button', {name: 'Save'})).toBeDisabled();
    expect(within(form).getByText('Type a name to save.')).toBeInTheDocument();
    fireEvent.change(within(form).getByLabelText('Preset name'), {target: {value: 'Mine'}});
    fireEvent.submit(within(form).getByLabelText('Preset name'));
    await waitFor(() => expect(mocks.savePhysicsPreset).toHaveBeenCalledWith('Mine', {tail: {swing: .5}}));
    expect(await screen.findByText('Saved “Mine”.')).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.click(saved);
    await act(async () => { fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', {name: 'Delete Soft hair'})); });
    expect(mocks.deletePhysicsPreset).toHaveBeenCalledWith('Soft hair');
    expect(await screen.findByText('Deleted “Soft hair”.')).toBeInTheDocument();
  });
  it('says when there is nothing to save or no saved preset yet', async () => {
    mocks.physicsPresets.mockResolvedValue([]);
    render(<PhysicsPresets cfg={{}} update={vi.fn()} disabled={false}/>);
    fireEvent.click(await screen.findByRole('button', {name: 'Saved presets'}));
    expect(screen.getByText(/No saved presets yet/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name: 'Save as preset'}));
    expect(screen.getByText('Nothing to save yet: every slider is at automatic.')).toBeInTheDocument();
  });
});

describe('Source rig springs', () => {
  it('offers the model\'s own spring settings as starting values, off by default', () => {
    mocks.physicsPresets.mockResolvedValue([]);
    const update = vi.fn();
    render(<SecondaryMotion project={motion(['hair_a', 'tail'], {}, {physics_springs: {hair_a: {source: 'vrm', goal: .45, drag: .2, gravity: 0}, tail: {source: 'vrm', goal: .9, drag: .1, gravity: .5}}})} update={update} disabled={false} report={null} fresh={false}/>);
    const box = screen.getByLabelText("Start from your model's own VRM spring settings (2 parts)");
    expect(box).not.toBeChecked();
    fireEvent.click(box);
    expect(update).toHaveBeenCalledWith({use_source_springs: true});
  });
});
