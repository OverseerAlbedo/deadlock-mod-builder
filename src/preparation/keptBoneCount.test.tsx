import { useState } from 'react';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { PhysicsBoneChoices } from './PhysicsBoneChoices';
import { MAX_KEPT_BONES, type PhysicsGroup } from './secondary-bones';
import type { PreparationOptions } from './models';

// Moving parts tab: the kept bones against the limit, a quiet line that turns into a warning over it.
const group = (root: string, count: number): PhysicsGroup => ({root, source_anchor: 'head', target_anchor: 'head',
  source_bones: Array.from({length: count}, (_, i) => `${root}_${String(i).padStart(4, '0')}`), weighted_bones: []});
// Two hair groups 12 bones under the limit together, and a tail of 14 that takes them 2 over.
const half = Math.floor(MAX_KEPT_BONES / 2), rest = MAX_KEPT_BONES - half - 12;
const groups = [group('hair_a', half), group('hair_b', rest), group('tail_a', 14)];
const options: PreparationOptions = {preserve_physics_bones: true, physics_roots: ['hair_a', 'hair_b']};
function Harness({start = options}: {start?: PreparationOptions}) {
  const [value, setValue] = useState(start);
  return <PhysicsBoneChoices groups={groups} options={value} busy={false} change={change => setValue({...value, ...change})}/>;
}
const line = () => document.querySelector('.prep-kept-count');
const count = (kept: number) => `Kept bones: ${kept} of ${MAX_KEPT_BONES}`;

afterEach(cleanup);

describe('Kept bones count', () => {
  it('shows the kept bones against the limit', () => {
    render(<Harness/>);
    expect(screen.getByText(count(MAX_KEPT_BONES - 12))).toBeVisible();
    expect(line()).not.toHaveClass('over');
    expect(line()).toHaveAttribute('aria-live', 'polite');
    expect(screen.queryByText(/over the limit/)).not.toBeInTheDocument();
  });
  it('turns into a warning over the limit and back when a group is left out, disabling nothing', () => {
    render(<Harness/>);
    fireEvent.click(screen.getByRole('checkbox', {name: /tail_a/}));
    expect(screen.getByText(count(MAX_KEPT_BONES + 2))).toBeVisible();
    expect(screen.getByText('2 bones over the limit. Leaving groups out brings it down.')).toBeVisible();
    expect(line()).toHaveClass('over');
    // The same element changes in place: a screen reader hears it, and nothing is disabled by it.
    expect(line()).toHaveAttribute('aria-live', 'polite');
    for (const box of screen.getAllByRole('checkbox')) expect(box).toBeEnabled();
    expect(screen.getByRole('button', {name: 'Keep all detected groups'})).toBeEnabled();
    expect(screen.getByRole('button', {name: 'Clear group selection'})).toBeEnabled();
    fireEvent.click(screen.getByRole('checkbox', {name: /hair_b/}));
    expect(screen.getByText(count(half + 14))).toBeVisible();
    expect(line()).not.toHaveClass('over');
  });
  it('says one bone over in the singular', () => {
    render(<PhysicsBoneChoices groups={[group('hair_a', half), group('hair_b', MAX_KEPT_BONES + 1 - half)]} options={options} busy={false} change={vi.fn()}/>);
    expect(screen.getByText(count(MAX_KEPT_BONES + 1))).toBeVisible();
    expect(screen.getByText('1 bone over the limit. Leaving groups out brings it down.')).toBeVisible();
  });
  it('is not shown when no group is kept or the bones are not kept', () => {
    render(<Harness start={{...options, physics_roots: []}}/>);
    expect(line()).toBeNull();
    cleanup();
    render(<Harness start={{...options, preserve_physics_bones: false}}/>);
    expect(line()).toBeNull();
  });
});
