import { useEffect, useRef } from 'react';
import { api } from '../api';

/** Settings → Appearance → Load 3D previews automatically: one preview load per input, started by
 * itself while its page is shown.
 *
 * - `on`: the setting is on and the page (or tab) is shown.
 * - `key`: what is to be loaded (the model, the inspection, the view); null when nothing is to be
 *   loaded (no input yet, or the preview is already there).
 * - `busy`: another operation runs or the app is busy; the load waits and then starts once.
 * - `load`: the same load the button starts. It resolves to false when it did not start (another
 *   operation took the app first); the load is then tried again at the next idle moment. A key
 *   whose load started is never loaded again by itself while this component lives, also when it
 *   failed (the page shows the same quiet error as after a click).
 * - `cancelOnLeave`: a load still running when `on` goes false (the page was left) is cancelled,
 *   and may start again when the page is shown again.
 *
 * The load starts after a short pause, so operations other parts of the page start when they
 * mount (for example the saved gun on Appearance & weapons) go first. */
export function useAutoLoad({on, key, busy, load, cancelOnLeave = false}: {on: boolean; key: string | null; busy: boolean; load: () => Promise<boolean | void> | boolean | void; cancelOnLeave?: boolean}) {
  const tried = useRef(new Set<string>());
  const running = useRef<string | null>(null);
  const loadRef = useRef(load); loadRef.current = load;
  useEffect(() => {
    if (!on || key === null || busy || tried.current.has(key) || running.current !== null) return;
    const timer = window.setTimeout(() => {
      tried.current.add(key); running.current = key;
      void Promise.resolve().then(() => loadRef.current()).then(started => { if (started === false) tried.current.delete(key); }, () => undefined)
        .finally(() => { if (running.current === key) running.current = null; });
    }, AUTO_LOAD_PAUSE_MS);
    return () => window.clearTimeout(timer);
  }, [on, key, busy]);
  useEffect(() => {
    if (on || !cancelOnLeave || running.current === null) return;
    // Left while loading: stop it, and allow it again when the page is shown again.
    tried.current.delete(running.current); running.current = null;
    void Promise.resolve().then(() => api.cancel?.()).catch(() => undefined);
  }, [on, cancelOnLeave]);
}
/** The pause before an automatic load starts. */
export const AUTO_LOAD_PAUSE_MS = 60;

/** A number per object (a conversion result, a preview), for keys that must change with it. */
const ids = new WeakMap<object, number>();
let nextId = 0;
export function idOf(value: object | null | undefined): number {
  if (!value) return 0;
  let id = ids.get(value);
  if (id === undefined) { id = ++nextId; ids.set(value, id); }
  return id;
}
