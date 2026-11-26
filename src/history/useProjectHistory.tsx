/** The React side of the project history: the hook `App.tsx` keeps beside the document, the
 * context pages use to drive it (the Extras → Colors page's Undo and Redo, a drag marked as one
 * step), the app's one Ctrl+Z / Ctrl+Y handler, and the two buttons in the header. */
import { createContext, useContext, useEffect, useMemo, useRef, useState } from 'react';
import type { Project } from '../models';
import { changesSince } from '../build/changes';
import { ProjectHistory } from './projectHistory';
import { t, useT } from '../i18n';
// i18n-area: shell

export interface HistoryControls {
  canUndo: boolean;
  canRedo: boolean;
  undo: () => void;
  redo: () => void;
  /** A drag or another gesture starts (true) or ends (false): its edits are one step. */
  gesture: (active: boolean) => void;
  /** The history starts fresh (an operation replaced what the project points at). */
  fence: () => void;
}

export const HistoryContext = createContext<HistoryControls | null>(null);
/** The app's history, or null outside a provider. */
export const useHistory = () => useContext(HistoryContext);

/** What an undo or redo changed, in a few words ("Physics and moving parts"), or ''. */
export function stepLabel(before: Project, after: Project): string {
  const labels = changesSince(before, after);
  if (!labels.length) return before.name !== after.name ? 'Project name' : '';
  return labels.length > 2 ? `${labels.slice(0, 2).join(', ')} and more` : labels.join(', ');
}
/** The same in the interface language, for the toast (the words come from `changesSince`). */
export function shownStepLabel(before: Project, after: Project): string {
  const labels = changesSince(before, after).map(label => t(label));
  if (!labels.length) return before.name !== after.name ? t('Project name') : '';
  if (labels.length > 2) return t('{first}, {second} and more', { first: labels[0], second: labels[1] });
  return labels.length === 2 ? t('{first}, {second}', { first: labels[0], second: labels[1] }) : labels[0];
}

/** One history for the open project.
 * `read`: the project as it is now (the document ref, never a render's copy).
 * `write`: put a project in place the way a hand edit does (unsaved, every view reacts);
 *   returns false when it could not.
 * `blocked`: an operation is running (it holds a copy of the project), so nothing moves. */
export function useProjectHistory({ read, write, notify, blocked = false }: {
  read: () => Project | undefined;
  write: (project: Project) => boolean;
  notify?: (message: string) => void;
  blocked?: boolean;
}) {
  const history = useRef(new ProjectHistory()).current;
  const [version, setVersion] = useState(0);
  const bump = () => setVersion(v => v + 1);
  const latest = useRef({ read, write, notify, blocked });
  latest.current = { read, write, notify, blocked };
  const controls = useMemo(() => {
    const travel = (direction: 'undo' | 'redo') => {
      const { read, write, notify, blocked } = latest.current;
      const step = direction === 'undo' ? history.nextUndo() : history.nextRedo();
      if (blocked || !step || !read()) return;
      const target = direction === 'undo' ? step.before : step.after;
      if (!write(target)) return;
      if (direction === 'undo') history.undone(); else history.redone();
      bump();
      const label = shownStepLabel(step.before, step.after);
      notify?.(direction === 'undo'
        ? (label ? t('Undone: {what}', { what: label }) : t('Undone'))
        : (label ? t('Redone: {what}', { what: label }) : t('Redone')));
    };
    return {
      canUndo: history.canUndo && !blocked,
      canRedo: history.canRedo && !blocked,
      undo: () => travel('undo'),
      redo: () => travel('redo'),
      gesture: (active: boolean) => history.gesture(active),
      fence: () => { history.fence(); bump(); },
    } satisfies HistoryControls;
    // `version` re-reads canUndo / canRedo after the history moved.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [history, version, blocked, history.canUndo, history.canRedo]);
  /** Record a hand edit (call it with the project before and after, as it is applied). */
  const record = (before: Project, after: Project) => { const had = [history.canUndo, history.canRedo]; history.record(before, after); if (had[0] !== history.canUndo || had[1] !== history.canRedo) bump(); };
  return { controls, record, history };
}

/** A text field, where Ctrl+Z belongs to the field (sliders, ticks and buttons are not). */
export function typing(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element || typeof element.tagName !== 'string') return false;
  if (element.isContentEditable || element.tagName === 'TEXTAREA') return true;
  return element.tagName === 'INPUT' && !['range', 'checkbox', 'radio', 'button', 'submit', 'reset', 'color'].includes((element as HTMLInputElement).type);
}

/** The app's one Ctrl+Z / Ctrl+Y (and Ctrl+Shift+Z) handler. */
export function useUndoKeys(controls: HistoryControls | null) {
  const ref = useRef(controls);
  ref.current = controls;
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.defaultPrevented || typing(event.target)) return;
      const letter = event.key.toLowerCase();
      const undo = letter === 'z' && !event.shiftKey, redo = letter === 'y' || (letter === 'z' && event.shiftKey);
      if (!undo && !redo) return;
      event.preventDefault();
      if (undo) ref.current?.undo(); else ref.current?.redo();
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, []);
}

/** Undo and Redo in the app's header. */
export function HistoryButtons({ controls }: { controls: HistoryControls }) {
  const t = useT();
  return <div className="history-controls" role="group" aria-label={t('Undo and redo')}>
    <button type="button" className="topbar-button history-button" disabled={!controls.canUndo} aria-keyshortcuts="Control+Z" title={t('Undo ({keys})', {keys: 'Ctrl+Z'})} aria-label={t('Undo')} onClick={controls.undo}><span aria-hidden="true">↶</span></button>
    <button type="button" className="topbar-button history-button" disabled={!controls.canRedo} aria-keyshortcuts="Control+Y Control+Shift+Z" title={t('Redo ({keys} or {other})', {keys: 'Ctrl+Y', other: 'Ctrl+Shift+Z'})} aria-label={t('Redo')} onClick={controls.redo}><span aria-hidden="true">↷</span></button>
  </div>;
}
