import { describe, expect, it } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import { changeKey, HISTORY_LIMIT, MERGE_MS, ProjectHistory } from './projectHistory';

const base = reference.cases[0].expected as unknown as Project;
const tuned = (project: Project, kind: string, value: number): Project => ({
  ...project,
  customization: { ...project.customization, secondary_motion: { ...project.customization.secondary_motion, tuning: { ...project.customization.secondary_motion?.tuning, [kind]: { stiffness: value } } } },
});
const named = (project: Project, name: string): Project => ({ ...project, name });

describe('the project history', () => {
  it('names the part of the project an edit changed', () => {
    expect(changeKey(base, named(base, 'x'))).toBe('name');
    expect(changeKey(base, tuned(base, 'hair', 0.2))).toBe('customization/secondary_motion');
    const once = tuned(base, 'hair', 0.2);
    expect(changeKey(once, tuned(once, 'hair', 0.4))).toBe('customization/secondary_motion/tuning/hair/stiffness');
    expect(changeKey(once, tuned(once, 'tail', 0.4))).toBe('customization/secondary_motion/tuning/tail');
    // An equal copy is no change at all.
    expect(changeKey(base, structuredClone(base))).toBeNull();
    expect(changeKey(base, { ...base, name: 'x', vpk_number: 3 })).toBe('{name,vpk_number}');
  });
  it('undoes and redoes a step, and a new edit drops the redo', () => {
    const history = new ProjectHistory();
    const a = named(base, 'a');
    history.record(base, a, 0);
    expect(history.canUndo).toBe(true);
    expect(history.nextUndo()?.before).toBe(base);
    history.undone();
    expect(history.canUndo).toBe(false);
    expect(history.nextRedo()?.after).toBe(a);
    history.record(base, named(base, 'b'), 10_000);
    expect(history.canRedo).toBe(false);
  });
  it('merges quick edits of the same part, keeps other parts and slower edits apart', () => {
    const history = new ProjectHistory();
    const one = tuned(base, 'hair', 0.1);
    history.record(base, one, 0);
    const two = tuned(one, 'hair', 0.2);
    history.record(one, two, 100);
    const three = tuned(two, 'hair', 0.3);
    history.record(two, three, 100 + MERGE_MS);
    // 0.1 → 0.2 → 0.3 typed quickly: the first step started a new tuning map (another key), the
    // next two are one step because they change the same value within the window.
    expect(history.undoSteps.map(step => step.after)).toEqual([one, three]);
    const other = tuned(three, 'tail', 0.5);
    history.record(three, other, 150 + MERGE_MS);
    expect(history.undoSteps).toHaveLength(3);
    const late = tuned(other, 'tail', 0.6);
    history.record(other, late, 150 + MERGE_MS * 3);
    expect(history.undoSteps).toHaveLength(4);
  });
  it('makes a drag one step, whatever its length, and a change right after it a new one', () => {
    const history = new ProjectHistory();
    history.gesture(true);
    let current = base;
    for (let i = 1; i <= 30; i++) { const next = tuned(current, 'hair', i / 100); history.record(current, next, i * 5000); current = next; }
    history.gesture(false);
    expect(history.undoSteps).toHaveLength(1);
    expect(history.undoSteps[0].before).toBe(base);
    expect(history.undoSteps[0].after).toBe(current);
    const after = tuned(current, 'hair', 0.5);
    history.record(current, after, 30 * 5000 + 10);
    expect(history.undoSteps).toHaveLength(2);
  });
  it('forgets a drag that came back to where it started', () => {
    const history = new ProjectHistory();
    const one = tuned(base, 'hair', 0.2);
    history.record(base, one, 0);
    history.gesture(true);
    const two = tuned(one, 'hair', 0.4);
    history.record(one, two, 10);
    history.record(two, tuned(two, 'hair', 0.2), 20);
    history.gesture(false);
    expect(history.undoSteps).toHaveLength(1);
    expect(history.undoSteps[0].after).toBe(one);
  });
  it('seals a step after an undo and a redo, so the next edit is its own step', () => {
    const history = new ProjectHistory();
    const a = named(base, 'a');
    history.record(base, a, 0);
    history.undone(); history.redone();
    history.record(a, named(a, 'ab'), 10);
    expect(history.undoSteps).toHaveLength(2);
  });
  it('starts fresh at a fence and keeps at most the limit', () => {
    const history = new ProjectHistory();
    let current = base;
    for (let i = 0; i < HISTORY_LIMIT + 20; i++) { const next = named(current, `n${i}`); history.record(current, next, i * MERGE_MS * 2); current = next; }
    expect(history.undoSteps).toHaveLength(HISTORY_LIMIT);
    expect(history.undoSteps[0].before.name).toBe('n19');
    history.undone();
    history.fence();
    expect(history.canUndo).toBe(false);
    expect(history.canRedo).toBe(false);
  });
});
