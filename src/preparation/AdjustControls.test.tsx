import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AdjustControls } from './WeaponPanel';
import type { WeaponChoice } from './models';

describe('Both guns', () => {
  it('applies one click to both hands, each in its own frame', () => {
    const set = vi.fn();
    const choice: WeaponChoice = {enabled: true, action: 'replace', objects: ['R'], left: {objects: ['L']}};
    render(<AdjustControls choice={choice} hand="both" set={set} busy={false}/>);
    expect(screen.getByRole('group', {name: 'Adjust both guns'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', {name: 'Bigger'}));
    const next = set.mock.lastCall![0] as WeaponChoice;
    expect(next.scale).toBeCloseTo(1.1);
    expect(next.left?.scale).toBeCloseTo(1.1);
    fireEvent.click(screen.getByRole('button', {name: '▲ Up'}));
    const moved = set.mock.lastCall![0] as WeaponChoice;
    expect(moved.offset?.[1]).toBeCloseTo(0.25);
    expect(moved.left?.offset?.[1]).toBeCloseTo(0.25);
  });
});

describe('Pitch, yaw and fine roll', () => {
  it('tilts the muzzle up, aims it right and rolls in 5° steps', () => {
    const set = vi.fn();
    const choice: WeaponChoice = {enabled: true, action: 'replace', objects: ['R']};
    render(<AdjustControls choice={choice} hand="right" set={set} busy={false}/>);
    fireEvent.click(screen.getByRole('button', {name: 'Tilt up 5°'}));
    expect((set.mock.lastCall![0] as WeaponChoice).pitch).toBe(5);
    fireEvent.click(screen.getByRole('button', {name: 'Aim right 5°'}));
    expect((set.mock.lastCall![0] as WeaponChoice).yaw).toBe(5);
    fireEvent.click(screen.getByRole('button', {name: 'Roll ⟲ 5°'}));
    expect((set.mock.lastCall![0] as WeaponChoice).roll).toBe(-5);
  });
});
