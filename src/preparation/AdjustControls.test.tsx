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
