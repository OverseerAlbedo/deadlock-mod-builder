import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { glide, placeLabels, restAnchors, stickySides } from './ModelLabels';
import { StudioActionBar, StudioTabPanel, StudioTabs, StudioTopBar } from './Studio';
import { partLabels } from '../components/CharacterPreview';

describe('Labels on the model', () => {
  it('keeps a label on its side until its point is clearly past the middle, and glides instead of jumping', () => {
    // First seen left of the middle: the label is on the left.
    expect(stickySides([{x: 190, y: 50}, null], [undefined, undefined], 400)).toEqual([true, undefined]);
    // Just past the middle (within the dead band): it stays on the left.
    expect(stickySides([{x: 215, y: 50}], [true], 400)).toEqual([true]);
    // Clearly past the middle: it changes side; coming back, it stays right until clearly left again.
    expect(stickySides([{x: 240, y: 50}], [true], 400)).toEqual([false]);
    expect(stickySides([{x: 185, y: 50}], [false], 400)).toEqual([false]);
    expect(stickySides([{x: 160, y: 50}], [false], 400)).toEqual([true]);
    // A hidden label remembers its side.
    expect(stickySides([null], [false], 400)).toEqual([false]);
    // The remembered side decides the placement; without it the side of the view does, as before.
    const sizes: [number, number][] = [[60, 20]];
    expect(placeLabels([{x: 215, y: 100}], sizes, 400, 300)).toEqual([{left: 227, top: 90}]);
    expect(placeLabels([{x: 215, y: 100}], sizes, 400, 300, 6, undefined, [true])).toEqual([{left: 143, top: 90}]);
    // A glide covers a share of the way each frame and lands when it is close.
    expect(glide({left: 0, top: 0}, {left: 100, top: 40}, .25)).toEqual({left: 25, top: 10});
    expect(glide({left: 99.8, top: 39.9}, {left: 100, top: 40}, .25)).toEqual({left: 100, top: 40});
    expect(glide({left: 0, top: 0}, {left: 100, top: 40}, 1)).toEqual({left: 100, top: 40});
  });
  it('places each label beside its point, inside the view, without overlapping', () => {
    const placed = placeLabels([{x: 100, y: 100}, {x: 104, y: 104}, null, {x: 395, y: 290}], [[60, 20], [60, 20], [60, 20], [60, 20]], 400, 300);
    expect(placed[2]).toBeNull();
    const [a, b, , c] = placed as {left: number; top: number}[];
    // Left of the middle: the label sits on the left of its point.
    expect(a).toEqual({left: 28, top: 90});
    // The second would sit on the first: it moves below it.
    expect(b.top).toBeGreaterThanOrEqual(a.top + 20);
    // Right of the middle but no room on the right: it goes to the left of its point, inside the view.
    expect(c.left).toBe(395 - 12 - 60);
    expect(c.top).toBeLessThanOrEqual(300 - 20 - 4);
  });
  it('hides what would overlap when asked, keeping the active label first and the default layout unchanged', () => {
    const points = [{x: 100, y: 100}, {x: 104, y: 104}, {x: 300, y: 100}], sizes: [number, number][] = [[60, 20], [60, 20], [60, 20]];
    const pushed = placeLabels(points, sizes, 400, 300);
    expect(placeLabels(points, sizes, 400, 300, 6, undefined)).toEqual(pushed);
    // The second label sits on the first: it is hidden, not pushed down; the active one wins.
    const hidden = placeLabels(points, sizes, 400, 300, 6, {first: [false, true, false]});
    expect(hidden[1]).toEqual({left: 104 - 12 - 60, top: 94});
    expect(hidden[0]).toEqual({left: 112, top: 90});
    expect(hidden[2]).not.toBeNull();
    // With room on neither side the other label is hidden.
    const crowded = placeLabels([{x: 65, y: 100}, {x: 65, y: 100}], [[60, 20], [60, 20]], 130, 300, 6, {first: [true, false]});
    expect(crowded[0]).not.toBeNull();
    expect(crowded[1]).toBeNull();
  });
  it('anchors a label at the middle of its bones, or at its own point', () => {
    const anchors = restAnchors([{id: 'hair', text: 'Hair', bones: ['a', 'b', 'missing']}, {id: 'gun', text: 'Gun', point: [1, 2, 3]}, {id: 'none', text: 'None', bones: ['missing']}],
      new Map<string, [number, number, number]>([['a', [0, 0, 10]], ['b', [2, 0, 20]]]));
    expect(anchors.get('hair')?.toArray()).toEqual([1, 0, 15]);
    expect(anchors.get('gun')?.toArray()).toEqual([1, 2, 3]);
    expect(anchors.has('none')).toBe(false);
  });
  it('makes one label per recorded part type with its part count, rigid parts left out', () => {
    const node = (bone: string, root: string, kind: string) => ({bone, root, kind, goal: .5, drag: .1, gravity: 0, travel: 0});
    const select = vi.fn();
    const set = partLabels([node('t1', 'tail', 'tail'), node('h1', 'hair_l', 'hair'), node('h2', 'hair_l', 'hair'), node('h3', 'hair_r', 'hair'), node('c', 'clip', 'rigid'), node('x', 'odd', 'long_skirt')], 'tail', select)!;
    expect(set.items.map(item => [item.id, item.text, item.detail, item.active])).toEqual([['hair', 'Hair', '2 parts', false], ['tail', 'Tails', '1 part', true], ['long_skirt', 'Long skirts', '1 part', false]]);
    expect(set.items[0].bones).toEqual(['h1', 'h2', 'h3']);
    expect(partLabels([node('c', 'clip', 'rigid')], null, select)).toBeNull();
  });
});

describe('The studio frame', () => {
  it('ties tabs to their panels, keeps hidden panels mounted and moves with the arrow keys', () => {
    function Frame() {
      const [tab, setTab] = useState<'a' | 'b'>('a');
      return <>
        <StudioTopBar title="my_project" badge="Mode" status="3 parts"><button>Tool</button></StudioTopBar>
        <StudioTabs label="Sections" base="t" active={tab} onSelect={setTab} tabs={[{id: 'a', label: 'First', status: '2 changed', tone: 'changed'}, {id: 'b', label: 'Second'}]}/>
        <StudioTabPanel base="t" id="a" active={tab === 'a'}><input aria-label="Kept"/></StudioTabPanel>
        <StudioTabPanel base="t" id="b" active={tab === 'b'}><p>Second panel</p></StudioTabPanel>
        <StudioActionBar tone="ok" title="Ready">{<button>Continue</button>}</StudioActionBar>
      </>;
    }
    render(<Frame/>);
    const first = screen.getByRole('tab', {name: 'First 2 changed'});
    expect(first).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tabpanel')).toHaveAccessibleName('First 2 changed');
    fireEvent.change(screen.getByLabelText('Kept'), {target: {value: 'typed'}});
    fireEvent.keyDown(screen.getByRole('tablist', {name: 'Sections'}), {key: 'ArrowRight'});
    expect(screen.getByRole('tab', {name: 'Second'})).toHaveFocus();
    expect(screen.getByRole('tabpanel')).toHaveTextContent('Second panel');
    fireEvent.keyDown(screen.getByRole('tablist'), {key: 'Home'});
    expect(screen.getByLabelText('Kept')).toHaveValue('typed');
    expect(screen.queryByRole('status')).toBeNull();
  });
});
