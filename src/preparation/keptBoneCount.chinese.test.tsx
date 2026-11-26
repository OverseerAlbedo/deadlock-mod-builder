import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { MIRROR_KEY, setLanguage } from '../i18n';
import { PhysicsBoneChoices } from './PhysicsBoneChoices';
import { MAX_KEPT_BONES, type PhysicsGroup } from './secondary-bones';
import type { PreparationOptions } from './models';

// Moving parts tab in Chinese: the kept bones count, its warning over the limit, and what is saved stays the roots.
const group = (root: string, count: number): PhysicsGroup => ({root, source_anchor: 'head', target_anchor: 'head',
  source_bones: Array.from({length: count}, (_, i) => `${root}_${String(i).padStart(4, '0')}`), weighted_bones: []});
const half = Math.floor(MAX_KEPT_BONES / 2);
const groups = [group('hair_a', half), group('hair_b', MAX_KEPT_BONES - half - 12), group('tail_a', 14)];
let saved: PreparationOptions = {};
function Harness() {
  const [value, setValue] = useState<PreparationOptions>({preserve_physics_bones: true, physics_roots: ['hair_a', 'hair_b']});
  return <PhysicsBoneChoices groups={groups} options={value} busy={false} change={change => { saved = {...value, ...change}; setValue(saved); }}/>;
}

beforeEach(async () => { saved = {}; await act(() => setLanguage('zh-CN')); });
afterEach(async () => { cleanup(); await act(() => setLanguage('en')); localStorage.removeItem(MIRROR_KEY); });

describe('Kept bones count in Chinese', () => {
  it('counts in Chinese, warns over the limit and switches live', async () => {
    render(<Harness/>);
    expect(screen.getByText(`保留的骨骼：${MAX_KEPT_BONES - 12} / ${MAX_KEPT_BONES}`)).toBeVisible();
    fireEvent.click(screen.getByRole('checkbox', {name: /tail_a/}));
    expect(screen.getByText(`保留的骨骼：${MAX_KEPT_BONES + 2} / ${MAX_KEPT_BONES}`)).toBeVisible();
    expect(screen.getByText('超出上限 2 根骨骼。少保留几个组即可降低总数。')).toBeVisible();
    expect(saved.physics_roots).toEqual(['hair_a', 'hair_b', 'tail_a']);
    await act(() => setLanguage('en'));
    expect(screen.getByText(`Kept bones: ${MAX_KEPT_BONES + 2} of ${MAX_KEPT_BONES}`)).toBeVisible();
    expect(screen.getByText('2 bones over the limit. Leaving groups out brings it down.')).toBeVisible();
  });
});
