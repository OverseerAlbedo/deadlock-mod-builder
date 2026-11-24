/** Undo and Redo for the project's choices: one history per open project, kept beside the
 * document in `App.tsx` (never in a page, which unmounts).
 *
 * Every hand edit that goes through `updateProject` is recorded as a step holding the whole
 * project before and after (projects are immutable values, so a step costs only what changed).
 * Rapid edits of one control become ONE step: edits that change the same part of the project
 * (`changeKey`) within `MERGE_MS` of each other merge, and everything recorded while a gesture
 * (a drag) is open merges into the gesture's step. A step is sealed when its gesture ends, after
 * an undo or redo, and at a fence, so nothing merges into it any more.
 *
 * Fences: an operation that writes files the project points at (preparing, Analyze, a build, a
 * loaded outfit, another project, another hero) clears the history, so Undo never steps back
 * across it. */
import type { Project } from '../models';

/** Steps kept; the oldest are forgotten. */
export const HISTORY_LIMIT = 100;
/** Edits of the same part of the project closer together than this are one step. */
export const MERGE_MS = 1000;
/** How deep `changeKey` follows a single changed branch. */
const KEY_DEPTH = 8;

export interface Step { before: Project; after: Project; key: string; at: number; sealed: boolean }

type Tree = Record<string, unknown>;
const isTree = (value: unknown): value is Tree => typeof value === 'object' && value !== null;
const same = (a: unknown, b: unknown) => a === b || JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

/** The keys whose values differ between `a` and `b` (by content; equal copies do not count). */
function changedKeys(a: Tree, b: Tree): string[] {
  return [...new Set([...Object.keys(a), ...Object.keys(b)])].filter(key => a[key] !== b[key] && !same(a[key], b[key]));
}

/** The part of the project an edit changed: the path down the single changed branch
 * ("customization/secondary_motion/tuning/hair/stiffness"), ending where two or more keys
 * changed or a value is not an object. Null when nothing changed. */
export function changeKey(before: Project, after: Project): string | null {
  const path: string[] = [];
  let a: unknown = before, b: unknown = after;
  for (let depth = 0; depth <= KEY_DEPTH; depth++) {
    if (!isTree(a) || !isTree(b) || Array.isArray(a) !== Array.isArray(b)) return path.join('/');
    const keys = changedKeys(a, b);
    if (!keys.length) return depth === 0 ? null : path.join('/');
    if (keys.length > 1) return [...path, `{${keys.sort().join(',')}}`].join('/');
    path.push(keys[0]);
    a = a[keys[0]]; b = b[keys[0]];
  }
  return path.join('/');
}

/** The value at a `changeKey` path (undefined when the path ends in a set of keys). */
function valueAt(project: Project, key: string): { found: boolean; value: unknown } {
  if (!key) return { found: true, value: project };
  let value: unknown = project;
  for (const part of key.split('/')) {
    if (part.startsWith('{')) return { found: false, value: undefined };
    value = isTree(value) ? value[part] : undefined;
  }
  return { found: true, value };
}

/** Whether a step ends where it began (a drag that came back to its start). */
function returned(step: Step): boolean {
  const before = valueAt(step.before, step.key), after = valueAt(step.after, step.key);
  if (before.found && after.found) {
    if (!same(before.value, after.value)) return false;
    // The rest of the project may have changed too while the gesture was open.
    return changeKey(step.before, step.after) === null;
  }
  return changeKey(step.before, step.after) === null;
}

export class ProjectHistory {
  undoSteps: Step[] = [];
  redoSteps: Step[] = [];
  private gestureOpen = false;
  private gestureStep: Step | null = null;

  get canUndo() { return this.undoSteps.length > 0; }
  get canRedo() { return this.redoSteps.length > 0; }

  /** Record a hand edit from `before` to `after`. */
  record(before: Project, after: Project, now = Date.now()) {
    if (before === after) return;
    const key = changeKey(before, after);
    if (key === null) return;
    this.redoSteps = [];
    const top = this.undoSteps[this.undoSteps.length - 1];
    const merge = top && !top.sealed && (
      (this.gestureOpen && this.gestureStep === top) ||
      (!this.gestureOpen && top.key === key && now - top.at <= MERGE_MS));
    if (merge) {
      top.after = after; top.at = now;
      if (top.key !== key) top.key = changeKey(top.before, top.after) ?? top.key;
      if (returned(top)) { this.undoSteps.pop(); if (this.gestureStep === top) this.gestureStep = null; }
      return;
    }
    const step: Step = { before, after, key, at: now, sealed: false };
    this.undoSteps.push(step);
    if (this.gestureOpen) this.gestureStep = step;
    if (this.undoSteps.length > HISTORY_LIMIT) this.undoSteps.splice(0, this.undoSteps.length - HISTORY_LIMIT);
  }

  /** A gesture (a slider drag) starts or ends: everything it records is one step. */
  gesture(active: boolean) {
    if (active) { this.seal(); this.gestureOpen = true; this.gestureStep = null; return; }
    if (this.gestureStep) this.gestureStep.sealed = true;
    this.gestureOpen = false; this.gestureStep = null;
  }

  /** Nothing merges into the newest step any more. */
  seal() { const top = this.undoSteps[this.undoSteps.length - 1]; if (top) top.sealed = true; }

  /** The history starts fresh (an operation wrote files the project points at). */
  fence() { this.undoSteps = []; this.redoSteps = []; this.gestureStep = null; }

  /** The step Undo would take back, without taking it. */
  nextUndo(): Step | undefined { return this.undoSteps[this.undoSteps.length - 1]; }
  nextRedo(): Step | undefined { return this.redoSteps[this.redoSteps.length - 1]; }
  /** Move the step Undo took back (call after it was applied). */
  undone() { const step = this.undoSteps.pop(); if (step) { step.sealed = true; this.redoSteps.push(step); } this.gestureStep = null; }
  redone() { const step = this.redoSteps.pop(); if (step) { step.sealed = true; this.undoSteps.push(step); } this.gestureStep = null; }
}
