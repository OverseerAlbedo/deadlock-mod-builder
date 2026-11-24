/** The hero's effect trees for Extras → Effects. `api.effectTrees` is asked on every opening
 * of the Edit colors view: the backend keeps the trees with its catalog of the hero's assets
 * (also over a new discovery for the same hero and game), so after the first time it answers
 * at once; the hero's assets are discovered again only when the backend refuses (no catalog,
 * a hero switch, a changed game archive). The last answer per hero and catalog load is kept
 * here so the page shows it at once and Replace a file can read it. */
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { Project } from "../models";
import type { EffectTrees } from "./effectTypes";

export const EFFECTS_UNAVAILABLE = "This version of the app cannot list the hero's effects yet.";
export type TreesStatus = "idle" | "loading" | "ready" | "failed" | "cancelled";
export interface TreesState {
  status: TreesStatus;
  data: EffectTrees | null;
  error: unknown;
  /** Counts the answers this view received (a fresh answer means the backend has the trees loaded). */
  answers?: number;
}
const loads = new Map<string, Promise<EffectTrees>>();
const loaded = new Map<string, EffectTrees>();
/** Reload assets (`revision`) asks for the trees again. */
export const treesKey = (project: Project, revision = 0) => (project.original_model ? `${project.original_model}:${revision}` : null);
/** The last trees loaded for this key (null while loading or never asked). */
export const loadedTrees = (key: string | null) => (key ? loaded.get(key) ?? null : null);
export function forgetTrees(key?: string) {
  if (key === undefined) {
    loads.clear();
    loaded.clear();
  } else {
    loads.delete(key);
    loaded.delete(key);
  }
}
export const cancelled = (error: unknown) =>
  typeof error === "object" && error !== null && "kind" in error && ((error as { kind?: unknown }).kind === "cancelled" || (error as { kind?: unknown }).kind === "busy");

async function load(project: Project): Promise<EffectTrees> {
  const native = api as unknown as { effectTrees?: (p: Project) => Promise<EffectTrees>; discoverExtras?: (p: Project) => Promise<unknown> };
  if (!native.effectTrees) throw new Error(EFFECTS_UNAVAILABLE);
  // effect_trees reads the catalog discover_extras keeps in the backend. When the backend
  // already holds this hero's catalog it answers at once (kept trees) or reads them; a new
  // discovery (seconds) runs only when it is refused: no catalog, another hero, or a game
  // archive that changed. A discovery for the same hero and game keeps the trees already
  // read. The two calls never run at once (both take the operation lease).
  try {
    return await native.effectTrees(project);
  } catch (error) {
    if (cancelled(error) || !native.discoverExtras) throw error;
  }
  await native.discoverExtras(project);
  return native.effectTrees(project);
}
/** One request per key at a time; a finished one is not reused (the next opening asks again). */
function request(key: string, project: Project) {
  let known = loads.get(key);
  if (!known) {
    known = Promise.resolve().then(() => load(project));
    const done = known;
    done.then(
      (data) => {
        loaded.set(key, data);
        while (loaded.size > 4) loaded.delete(loaded.keys().next().value!);
      },
      () => undefined,
    ).finally(() => { if (loads.get(key) === done) loads.delete(key); });
    loads.set(key, known);
  }
  return known;
}
/** The trees for `key`. A request starts once per mount, key and `attempt` when `canStart`
 * (not while the page is disabled or discovering); its outcome is kept even if `canStart`
 * turns false meanwhile. Cached trees are shown while the request runs. */
export function useEffectTrees(project: Project, key: string | null, canStart: boolean, attempt = 0): TreesState {
  const [state, setState] = useState<TreesState & { key: string | null }>({ key: null, status: "idle", data: null, error: null });
  const started = useRef("");
  const current = useRef(key);
  current.current = key;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => {
    if (!key || !canStart) return;
    const token = `${key}#${attempt}`;
    if (started.current === token) return;
    started.current = token;
    const cached = loaded.get(key) ?? null;
    setState((old) => ({ key, status: cached ? "ready" : "loading", data: cached, error: null, answers: old.answers }));
    request(key, project).then(
      (data) => { if (mounted.current && current.current === key) setState((old) => ({ key, status: "ready", data, error: null, answers: (old.answers ?? 0) + 1 })); },
      (error) => { if (mounted.current && current.current === key) setState({ key, status: cancelled(error) ? "cancelled" : "failed", data: null, error }); },
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the project is read when the request starts
  }, [key, canStart, attempt]);
  if (state.key === key) return state;
  const cached = loadedTrees(key);
  if (cached) return { status: "ready", data: cached, error: null };
  return { status: key && canStart ? "loading" : "idle", data: null, error: null };
}
