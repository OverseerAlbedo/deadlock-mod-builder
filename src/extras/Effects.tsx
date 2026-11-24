/** Extras → Effects: recolor the hero's ability and weapon particle effects. The effect list
 * (left), the selected effect's parts with their game and edited color strips (middle) and
 * the editor (right; Simple or Advanced, this page's own switch). Every color calculation is
 * done in Rust: `api.effectPreview` answers what an edit does, and a picked color comes back
 * as the shift to save. Saved as `customization.effect_edits` (effectEdits.ts). The second
 * view of the tab, Replace a file, is the shared replacement library in ExtrasPage. */
import { useCallback, useEffect, useId, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { api, errorHelp } from "../api";
import { ExtraIcon } from "./Icons";
import { ColorPicker } from "./ColorPicker";
import { GradientSlider, percentUnit, shiftUnit } from "./GradientSlider";
import { cssColor, fromHsb, textOn, toHsb } from "./colorMath";
import { previewError } from "./media";
import { treesKey, useEffectTrees, loadedTrees, forgetTrees } from "./effectTreesLoad";
import type { EffectEdit, EffectEditRow, EffectPart, EffectPartSetting, EffectPick, EffectPickRequest, EffectPreview, EffectTag, EffectTree, Rgb } from "./effectTypes";
import { useHistory } from "../history/useProjectHistory";
import { EFFECTS_GAME_CHANGED, EFFECTS_NOT_LOADED, EFFECT_UNKNOWN_PREFIX, LIGHT_MAX, LIGHT_MIN, NO_OWN_COLOR_NOTE } from "./effectTypes";
import {
  FACTOR_MAX,
  KIND_TEXT,
  MAX_ROWS,
  MULTIPLIERS,
  MULTIPLIER_MAX,
  MULTIPLIER_MIN,
  PRESETS,
  TAGS,
  BY_NAME,
  BY_NAME_GROUP,
  advancedText,
  byFileName,
  afterStops,
  alsoPartOf,
  changesSomething,
  cleanEdit,
  editKey,
  gameStops,
  groupTrees,
  isFiltering,
  shownGroups,
  type ShownGroup,
  hasShift,
  hexOf,
  isReplaced,
  mainIndex,
  partUnreached,
  matches,
  reachText,
  replacedPaths,
  replacementConflicts,
  replacementOf,
  rememberMode,
  rowActive,
  rowOf,
  sameRgb,
  savedMode,
  savedRows,
  stripBackground,
  stripText,
  tagCounts,
  toByte,
  toUnit,
  treeNames,
  treeSwatches,
  unreachedReason,
  withColor,
  withEdit,
  withEnabled,
  withPart,
  leftOut,
  sharedLeftOut,
  alsoUsedReach,
  withPick,
  withRows,
  withoutRow,
  withoutShift,
  hiddenParts,
  ownSizeText,
  partSetting,
  withPartSetting,
  type EffectsMode,
  type StripStop,
} from "./effectEdits";
import { EffectPlayer, desktopBridge, type PlayerBridge } from "./EffectPlayer";
import "./effects.css";

/** The desktop app's player bridge (null outside it: the page shows the strips only). */
let defaultBridge: PlayerBridge | null | undefined;
const appBridge = () => (defaultBridge === undefined ? (defaultBridge = desktopBridge()) : defaultBridge);

export type EffectsView = "colors" | "replace";
/** The end of the "Also used by" note: whether the change shows there (shared parts left out keep the game's look). */
const ALSO_USED = {
  shows: "Your change shows there too.",
  kept: "The parts they share with this effect are used by other heroes and stay as the game has them unless you tick them in Advanced.",
  partly: "Your change shows there too where the game uses the same parts; parts shared with other heroes stay as the game has them unless you tick them in Advanced.",
} as const;
const fold = (value: string) => value.toLowerCase();
const unit = (rgb: Rgb) => toUnit(rgb);

/** Edit colors | Replace a file (the old Effects library). Shown above both views. */
export function EffectsViewSwitch({ view, setView }: { view: EffectsView; setView: (view: EffectsView) => void }) {
  return (
    <div className="effects-view-switch" role="radiogroup" aria-label="Effects view">
      {(["colors", "replace"] as const).map((key) => (
        <button key={key} type="button" role="radio" aria-checked={view === key} onClick={() => setView(key)}>
          {key === "colors" ? "Edit colors" : "Replace a file"}
        </button>
      ))}
    </div>
  );
}

/** The Replace a file view's bar: the switch, and the files that also belong to a recolored effect. */
export function EffectsReplaceBar({ project, revision, view, setView }: { project: Project; revision: number; view: EffectsView; setView: (view: EffectsView) => void }) {
  const conflicts = replacementConflicts(project, loadedTrees(treesKey(project, revision)));
  return (
    <div className="effects-replace-bar">
      <div className="effects-replace-bar-row">
        <EffectsViewSwitch view={view} setView={setView} />
        <p>Replace one of the hero's effect files with your own compiled <code>.vpcf_c</code>.</p>
      </div>
      {conflicts.length > 0 && (
        <div className="effects-conflict" role="note">
          <ExtraIcon name="warning" size={16} />
          <div>
            <strong>Also in a recolored effect</strong>
            <ul>
              {conflicts.map((c) => (
                <li key={`${c.tree.target}:${c.path}`}>
                  Your file is used for <b>{c.part}</b>; the color edit of <b>{c.tree.name || c.tree.label}</b> changes that effect's other parts.
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}
    </div>
  );
}

export interface EffectsProps {
  project: Project;
  update: (project: Project) => void;
  disabled: boolean;
  catalog: ComponentCatalog | null;
  discovering: boolean;
  canDiscover: boolean;
  canReload: boolean;
  reload: () => void;
  openSetup?: () => void;
  revision: number;
  hero: string;
  view: EffectsView;
  setView: (view: EffectsView) => void;
  /** The moving preview's bridge: omitted, the desktop app's; null, no player (strips only). */
  playerBridge?: PlayerBridge | null;
}

interface PreviewRequest {
  target: string;
  edit: EffectEdit;
  pick?: EffectPickRequest;
  /** The user's chosen main color (sent on every call; the answer's `main` follows it). */
  main?: Rgb | null;
}
const frame = (callback: () => void) =>
  typeof requestAnimationFrame === "function" ? requestAnimationFrame(callback) : (setTimeout(callback, 16) as unknown as number);
const cancelFrame = (handle: number) =>
  typeof cancelAnimationFrame === "function" ? cancelAnimationFrame(handle) : clearTimeout(handle);

/** `api.effectPreview` at most once per animation frame; the latest request's answer wins.
 * A pick's answer is handed to `picked` (which saves its shift) even when a newer request
 * without a pick followed it. */
function usePreviewer(project: Project, picked: (request: PreviewRequest, pick: EffectPick) => void) {
  const [answer, setAnswer] = useState<{ key: string; preview: EffectPreview } | null>(null);
  const [error, setError] = useState<{ target: string; message: string; reload: boolean; n: number } | null>(null);
  const projectRef = useRef(project);
  projectRef.current = project;
  const pickedRef = useRef(picked);
  pickedRef.current = picked;
  const pending = useRef<PreviewRequest | null>(null);
  const scheduled = useRef(false);
  const handle = useRef(0);
  const seq = useRef(0);
  const lastPick = useRef(0);
  const flush = useCallback(() => {
    scheduled.current = false;
    const request = pending.current;
    pending.current = null;
    if (!request) return;
    const n = ++seq.current;
    api.effectPreview(projectRef.current, request.target, cleanEdit(request.edit), request.pick ?? null, request.main ?? null).then(
      (preview) => {
        let shown = request.edit;
        if (request.pick && preview.pick && n > lastPick.current) {
          lastPick.current = n;
          shown = withPick(request.edit, preview.pick);
          pickedRef.current(request, preview.pick);
        }
        if (n !== seq.current) return;
        setAnswer({ key: editKey(request.target, shown, request.main), preview });
        setError(null);
      },
      (cause) => {
        const message = previewError(cause);
        // The backend's effect list was lost or is stale (another hero, Reload assets, a game update).
        const reload = message === EFFECTS_NOT_LOADED || message === EFFECTS_GAME_CHANGED || message.startsWith(EFFECT_UNKNOWN_PREFIX);
        if (n === seq.current) setError({ target: request.target, message, reload, n });
      },
    );
  }, []);
  const request = useCallback(
    (next: PreviewRequest) => {
      const waiting = pending.current;
      // A pick still waiting is not dropped by a plain request for the same effect.
      pending.current = waiting?.pick && !next.pick && fold(waiting.target) === fold(next.target) ? { ...next, pick: waiting.pick } : next;
      if (!scheduled.current) {
        scheduled.current = true;
        handle.current = frame(flush);
      }
    },
    [flush],
  );
  // A request still waiting for its frame is dropped when the page goes away.
  useEffect(
    () => () => {
      if (scheduled.current) cancelFrame(handle.current);
      scheduled.current = false;
      pending.current = null;
    },
    [],
  );
  return { answer, error, request };
}

/** What the page shows before any answer (and for an edit that changes nothing): the game's colors. */
function identity(tree: EffectTree): EffectPreview {
  return {
    target: tree.target,
    edit: { hue: 0, saturation: 1, brightness: 1, excluded: [], shared: [], colors: [], size: 1, glow: 1, opacity: 1 },
    parts: tree.parts.map((part) => ({
      path: part.path,
      excluded: part.outside_hero,
      left_out: part.outside_hero ? ("shared" as const) : null,
      strips: part.strips.map((strip) => ({
        id: strip.id,
        kind: strip.kind,
        role: strip.role,
        stops: strip.stops.map((s) => ({ position: s.position, alpha: s.alpha, before: s.rgb, before_hex: s.hex, after: s.rgb, after_hex: s.hex, slot: s.slot, rule: "unchanged" as const })),
      })),
      multipliers: [],
      hidden: false,
      controls: [],
    })),
    palette: tree.palette.map((s) => ({ before: s.rgb, before_hex: s.hex, after: s.rgb, after_hex: s.hex })),
    main: tree.main ? { before: tree.main.rgb, before_hex: tree.main.hex, after: tree.main.rgb, after_hex: tree.main.hex } : null,
    changed: { colors: 0, multiplied: 0, not_multiplied: 0, hidden: 0, part_settings: 0 },
  };
}

type Draft = { edit: EffectEdit; main: Rgb | null };

export function Effects({ project, update, disabled, catalog, discovering, canDiscover, canReload, reload, openSetup, revision, hero, view, setView, playerBridge }: EffectsProps) {
  const bridge = playerBridge === undefined ? appBridge() : playerBridge;
  const projectRef = useRef(project);
  projectRef.current = project;
  const updateRef = useRef(update);
  updateRef.current = update;
  const [attempt, setAttempt] = useState(0);
  const key = catalog ? treesKey(project, revision) : null;
  const trees = useEffectTrees(project, key, Boolean(catalog) && !discovering && !disabled, attempt);
  const data = trees.data;
  const [mode, setModeState] = useState<EffectsMode>(savedMode);
  const setMode = (next: EffectsMode) => { setModeState(next); rememberMode(next); };
  const advanced = mode === "advanced";
  const [query, setQuery] = useState("");
  const [tags, setTags] = useState<EffectTag[]>([]);
  const [changedOnly, setChangedOnly] = useState(false);
  const [selected, setSelected] = useState("");
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [picked, setPicked] = useState<{ target: string; color: [number, number, number] } | null>(null);
  const [pickInfo, setPickInfo] = useState<{ target: string; pick: EffectPick } | null>(null);
  const [swatch, setSwatch] = useState<{ target: string; index: number } | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const [notice, setNotice] = useState("");
  /** Folded groups the user opened. */
  const [opened, setOpened] = useState<ReadonlySet<string>>(() => new Set());
  useEffect(() => { setDrafts({}); setPicked(null); setPickInfo(null); setSelected(""); setSwatch(null); setOpened(new Set()); }, [project.id, project.original_model]);

  const active = Boolean(project.customization.enabled);
  const rows = savedRows(project);
  const filter = { query, tags, changedOnly };
  const all = data?.trees ?? [];
  const visible = useMemo(() => all.filter((tree) => matches(tree, filter, rows)), [all, query, tags, changedOnly, rows]); // eslint-disable-line react-hooks/exhaustive-deps
  const filtering = isFiltering(filter);
  const grouped = useMemo(() => (data ? groupTrees(data, visible, filtering) : []), [data, visible, filtering]);
  const groups = shownGroups(grouped, opened, filtering, rows, selected);
  const counts = useMemo(() => tagCounts(all, filter, rows), [all, query, tags, changedOnly, rows]); // eslint-disable-line react-hooks/exhaustive-deps
  const names = useMemo(() => treeNames(data), [data]);
  const flat = groups.flatMap((g) => g.trees);
  const tree = all.find((t) => fold(t.target) === fold(selected)) ?? flat[0] ?? null;
  const target = tree?.target ?? "";
  const replaced = replacedPaths(project);

  /** The edit and main colour shown for a tree: the page's draft while it agrees with the
   * saved row (a draft also keeps parts left out and a chosen main colour that change
   * nothing yet), else the saved row. */
  function shownFor(t: string): Draft {
    const row = rowOf(savedRows(projectRef.current), t);
    const draft = drafts[fold(t)];
    if (draft) {
      const kept = changesSomething(draft.edit) ? JSON.stringify(cleanEdit(draft.edit)) : null;
      const saved = row ? JSON.stringify(cleanEdit(row.edit ?? {})) : null;
      if (kept === saved) return draft;
    }
    return { edit: row?.edit ?? {}, main: row?.main ?? null };
  }
  const shown = tree ? shownFor(tree.target) : { edit: {}, main: null };
  const shownRef = useRef<(t: string) => Draft>(shownFor);
  shownRef.current = shownFor;
  const edit = shown.edit;

  function commit(t: string, next: EffectEdit, main: Rgb | null = shownRef.current(t).main) {
    setDrafts((d) => ({ ...d, [fold(t)]: { edit: next, main } }));
    setNotice("");
    const current = projectRef.current;
    const before = savedRows(current);
    if (!rowOf(before, t) && before.length >= MAX_ROWS && changesSomething(next)) {
      setNotice(`At most ${MAX_ROWS} effects can be changed. Reset one before changing another.`);
      return;
    }
    const after = withEdit(before, t, next, main);
    if (JSON.stringify(after) === JSON.stringify(before)) return;
    const project2 = withRows(current, after, changesSomething(next));
    projectRef.current = project2;
    updateRef.current(project2);
  }
  const previewer = usePreviewer(project, (request, pick) => {
    commit(request.target, withPick(shownRef.current(request.target).edit, pick));
    setPickInfo({ target: request.target, pick });
  });
  const currentKey = tree ? editKey(tree.target, edit, shown.main) : "";
  const neutralEdit = !changesSomething(edit);
  const [previewRetry, setPreviewRetry] = useState(0);
  useEffect(() => {
    if (!tree || neutralEdit) return;
    if (previewer.answer?.key === currentKey) return;
    previewer.request({ target: tree.target, edit, main: shown.main });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key stands for the edit; new trees or Try again ask again
  }, [currentKey, neutralEdit, tree?.target, trees.answers, previewRetry]);
  // A failed preview: the picked color was not saved; when the backend lost its effect index,
  // ask for the trees again once (the preview follows when they arrive).
  const reloadedFor = useRef("");
  useEffect(() => {
    const failure = previewer.error;
    if (!failure) return;
    setPicked(null);
    if (failure.reload && reloadedFor.current !== currentKey) {
      reloadedFor.current = currentKey;
      setAttempt((v) => v + 1);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per failure
  }, [previewer.error]);
  function retryPreview() {
    setPicked(null);
    setAttempt((v) => v + 1);
    setPreviewRetry((v) => v + 1);
  }
  const preview = useMemo(
    () => (!tree ? null : !neutralEdit && previewer.answer && fold(previewer.answer.preview.target) === fold(tree.target) ? previewer.answer.preview : identity(tree)),
    [tree, neutralEdit, previewer.answer],
  );
  const stale = Boolean(tree && !neutralEdit && previewer.answer?.key !== currentKey);
  const previewFailed = previewer.error && tree && fold(previewer.error.target) === fold(tree.target) ? previewer.error.message : "";

  function select(t: string) {
    if (fold(t) === fold(target)) return;
    setSelected(t);
    setPicked(null);
    setNotice("");
    setConfirmReset(false);
  }
  function pick(color: [number, number, number]) {
    if (!tree) return;
    setPicked({ target: tree.target, color });
    previewer.request({ target: tree.target, edit, pick: { color: toByte(color), main: shown.main }, main: shown.main });
  }
  function clearPick() {
    setPicked(null);
    setPickInfo(null);
  }
  function resetEffect() {
    if (!tree) return;
    setDrafts((d) => { const { [fold(tree.target)]: _drop, ...rest } = d; return rest; });
    clearPick();
    const current = projectRef.current;
    const next = withRows(current, withoutRow(savedRows(current), tree.target), false);
    projectRef.current = next;
    update(next);
  }
  function resetAll() {
    setDrafts({});
    clearPick();
    setConfirmReset(false);
    update(withRows(projectRef.current, [], false));
  }
  function setIncluded(value: boolean) {
    if (!tree) return;
    update(withRows(projectRef.current, withEnabled(savedRows(projectRef.current), tree.target, value), value));
  }
  function chooseMain(index: number) {
    if (!tree) return;
    const rgb = tree.palette[index]?.rgb;
    if (!rgb) return;
    const main = tree.main && sameRgb(tree.main.rgb, rgb) ? null : ([...rgb] as Rgb);
    const yours = yoursMain;
    commit(tree.target, edit, main);
    // Keep the color the user made the old main color: the new one becomes it.
    if (hasShift(edit) && yours) previewer.request({ target: tree.target, edit, pick: { color: yours, main }, main });
  }

  const row = tree ? rowOf(rows, tree.target) : null;
  const mainAt = tree ? mainIndex(tree, shown.main) : null;
  const gameMain = tree && mainAt !== null ? tree.palette[mainAt]?.rgb ?? tree.main?.rgb ?? null : null;
  // The answer's `main` follows the chosen main color (sent with every preview).
  const yoursMain = !neutralEdit && preview?.main ? preview.main.after : gameMain;
  const pickerValue = picked && tree && fold(picked.target) === fold(tree.target) ? picked.color : unit(yoursMain ?? [128, 128, 128]);
  const info = pickInfo && tree && fold(pickInfo.target) === fold(tree.target) && hasShift(edit) ? pickInfo.pick : null;
  const conflicts = tree ? tree.parts.filter((p) => isReplaced(replaced, p.path)) : [];
  const sharedKept = tree ? sharedLeftOut(tree, edit).length : 0;

  // ── States of the list ──
  function listBody() {
    if (discovering) return <div className="color-list-empty" role="status"><span className="extra-spinner" /><strong>Finding {hero}'s assets</strong><p>Reading icons, sounds and effects from your game archive.</p></div>;
    if (!catalog) return (
      <div className="color-list-empty">
        <ExtraIcon name="sparkles" size={28} />
        <strong>{canDiscover ? "Load your character's assets" : "Connect your game to get started"}</strong>
        <p>{canDiscover ? "The hero's effects come from your game files. Your saved effect colors are kept." : "Choose the original game VPK in Setup, then return here."}</p>
        {canDiscover ? (
          <button className="extra-primary" onClick={reload} disabled={disabled || !canReload}>Load character assets<ExtraIcon name="arrow" size={16} /></button>
        ) : openSetup && <button className="extra-primary" onClick={openSetup}>Open Setup<ExtraIcon name="arrow" size={16} /></button>}
      </div>
    );
    if (trees.status === "loading" || trees.status === "idle") return (
      <div className="color-list-empty" role="status">
        <span className="extra-spinner" />
        <strong>Reading {hero}'s effects…</strong>
        <p>The first time, every effect and its parts are read from your game files; this takes a few seconds.</p>
        {trees.status === "loading" && <button type="button" className="extra-secondary" onClick={() => void api.cancel().catch(() => undefined)}>Cancel</button>}
      </div>
    );
    if (trees.status === "cancelled") return (
      <div className="color-list-empty">
        <ExtraIcon name="pause" size={24} />
        <strong>Effects not loaded</strong>
        <p>{(trees.error as { kind?: string })?.kind === "busy" ? "Another operation was running. Load them again when it is done." : "Loading was cancelled."}</p>
        <button type="button" className="extra-secondary" onClick={() => setAttempt((v) => v + 1)}>Load effects</button>
      </div>
    );
    if (trees.status === "failed") {
      const help = errorHelp(trees.error);
      const message = previewError(trees.error);
      const reloadHint = /reload assets|load this character/i.test(`${message} ${help?.steps.join(" ") ?? ""} ${(trees.error as { code?: string })?.code ?? ""}`) || /EFFECTS_(NOT_LOADED|GAME_CHANGED)/.test(JSON.stringify(trees.error ?? ""));
      return (
        <div className="color-list-empty">
          <p className="effects-error-line" role="alert"><strong>The effects could not be read.</strong> {message}{help?.steps.length ? ` ${help.steps.join(" ")}` : ""}</p>
          <div className="row-actions">
            <button type="button" className="extra-secondary" onClick={() => { if (key) forgetTrees(key); setAttempt((v) => v + 1); }}>Try again</button>
            {reloadHint && <button type="button" className="extra-secondary" disabled={disabled || !canReload} onClick={reload}>Reload assets</button>}
          </div>
        </div>
      );
    }
    if (!all.length) return <div className="color-list-empty"><ExtraIcon name="sparkles" size={24} /><strong>No effects found</strong><p>{data?.hero_name || hero} has no particle effects this page can recolor in this game version.</p></div>;
    if (!flat.length && !groups.some((g) => g.hidden)) return (
      <div className="color-list-empty">
        <ExtraIcon name="search" size={24} />
        <strong>{changedOnly && !query && !tags.length ? "No changed effects yet" : "No matching effects"}</strong>
        <p>{changedOnly && !query && !tags.length ? "Choose an effect and give it a color." : "Try another name or clear your filters."}</p>
        <button type="button" className="extra-secondary" onClick={() => { setQuery(""); setTags([]); setChangedOnly(false); }}>Show all effects</button>
      </div>
    );
    const toggleGroup = (key: string) => setOpened((list) => { const next = new Set(list); if (next.has(key)) next.delete(key); else next.add(key); return next; });
    return <EffectList groups={groups} rows={rows} active={active} selected={tree?.target ?? ""} select={select} toggleGroup={toggleGroup} />;
  }

  return (
    <div id="extras-panel" role="tabpanel" aria-labelledby="extras-tab-effects" className="colors-studio effects-studio">
      <header className="colors-head">
        <div>
          <h2>Effects</h2>
          <p>Recolor and adjust the hero's ability and weapon effects{data ? ` · ${all.length} effect${all.length === 1 ? "" : "s"} from your game` : ""}</p>
        </div>
        <EffectsViewSwitch view={view} setView={setView} />
        <span className="color-picker-spacer" />
        {rows.length > 0 && <span className="color-count">{rows.length} changed</span>}
        <span className="color-reset-all">
          <button type="button" className="extra-text-button" disabled={disabled || !rows.length} aria-expanded={confirmReset} onClick={() => setConfirmReset(!confirmReset)}>Reset all…</button>
          {confirmReset && (
            <div className="color-confirm" role="dialog" aria-label="Reset all effects">
              <p>Remove your changes from all {rows.length} effect{rows.length === 1 ? "" : "s"}? They keep the game's colors and sizes. Files you replaced stay.</p>
              <div className="row-actions">
                <button type="button" className="extra-remove" onClick={resetAll}>Reset all</button>
                <button type="button" className="extra-secondary" autoFocus onClick={() => setConfirmReset(false)}>Cancel</button>
              </div>
            </div>
          )}
        </span>
      </header>
      {data && data.warnings.length > 0 && (
        <details className="effects-warnings">
          <summary><ExtraIcon name="warning" size={15} />Notes about these effects ({data.warnings.length})</summary>
          {data.warnings.map((w, i) => <p key={i}>{w}</p>)}
        </details>
      )}
      <div className="colors-grid">
        <section className="color-list effects-list" aria-label="Effects">
          <div className="color-list-head">
            <h3>Effects</h3>
            <div className="color-segmented small" role="radiogroup" aria-label="Show effects">
              <button type="button" role="radio" aria-checked={!changedOnly} onClick={() => setChangedOnly(false)}>All {all.length}</button>
              <button type="button" role="radio" aria-checked={changedOnly} onClick={() => setChangedOnly(true)}>Changed {rows.length}</button>
            </div>
          </div>
          <div className="extras-search color-search">
            <ExtraIcon name="search" size={16} />
            <input aria-label="Search effects" placeholder="Search effects…" value={query} onChange={(e) => setQuery(e.target.value)} />
            {query && <button aria-label="Clear search" onClick={() => setQuery("")}><ExtraIcon name="close" size={14} /></button>}
          </div>
          <div className="effects-tags" role="group" aria-label="Show only">
            {TAGS.map((tag) => (
              <button key={tag.key} type="button" className="sounds-filter" aria-pressed={tags.includes(tag.key)} onClick={() => setTags((list) => (list.includes(tag.key) ? list.filter((t) => t !== tag.key) : [...list, tag.key]))}>
                <i className={`sounds-dot ${tag.key}`} aria-hidden="true" />
                {tag.label}
                <small>{counts.get(tag.key) ?? 0}</small>
              </button>
            ))}
          </div>
          {data && all.length > 0 && <p className="effects-list-note">Effects from {data.hero_name || hero}'s own folders, grouped by ability.</p>}
          {listBody()}
        </section>
        <section className="color-view effects-view" aria-label="Selected effect">
          {tree && preview ? (
            <EffectDetails
              tree={tree}
              preview={preview}
              stale={stale}
              previewFailed={previewFailed}
              retry={retryPreview}
              edit={edit}
              advanced={advanced}
              disabled={disabled}
              names={names}
              replaced={replaced}
              conflicts={conflicts.length}
              rows={rows}
              status={row ? (rowActive(row, active) ? "Changed" : "Paused") : ""}
              toggle={(part, on) => commit(tree.target, withPart(edit, part, on))}
              setPart={(path, patch) => commit(tree.target, withPartSetting(edit, path, patch))}
              player={bridge && trees.status === "ready" ? <EffectPlayer key={`${project.id}:${project.original_model}:${revision}`} project={project} tree={tree} edit={cleanEdit(edit)} bridge={bridge} /> : null}
            />
          ) : (
            <div className="color-stage-message effects-stage-empty">
              <strong>{data ? "Choose an effect" : "The effect's parts appear here"}</strong>
              <p>Each part shows the game's colors and yours, so you can see what follows your color.</p>
            </div>
          )}
        </section>
        <aside className="color-inspector effects-inspector" aria-label="Effect editor">
          {tree && preview ? (
            <div className="color-editor">
              <div className="color-editor-head">
                <h3>Colors</h3>
                {row && <span className={`color-pill ${rowActive(row, active) ? "changed" : ""}`}>{rowActive(row, active) ? "Changed" : "Paused"}</span>}
                <span className="color-picker-spacer" />
                <div className="color-segmented small" role="radiogroup" aria-label="Editor">
                  {(["simple", "advanced"] as const).map((m) => (
                    <button key={m} type="button" role="radio" aria-checked={mode === m} onClick={() => setMode(m)}>{m === "simple" ? "Simple" : "Advanced"}</button>
                  ))}
                </div>
              </div>
              {!advanced && advancedText(edit) && (
                <p className="effects-advanced-line" role="note">
                  {advancedText(edit)}{" "}
                  <button type="button" className="extra-text-button" onClick={() => setMode("advanced")}>Show in Advanced</button>
                </p>
              )}
              {!advanced && sharedKept > 0 && (
                <p className="effects-advanced-line" role="note">
                  {sharedKept} part{sharedKept === 1 ? " keeps" : "s keep"} the game's look because other heroes use {sharedKept === 1 ? "it" : "them"} too; Advanced can include {sharedKept === 1 ? "it" : "them"}.{" "}
                  <button type="button" className="extra-text-button" onClick={() => setMode("advanced")}>Show in Advanced</button>
                </p>
              )}
              <Presets
                tree={tree}
                main={gameMain}
                yours={yoursMain}
                hasShift={hasShift(edit)}
                disabled={disabled}
                pick={(rgb) => pick(unit(rgb))}
                title={advanced ? "Whole effect" : "Make it…"}
                hint={advanced ? "the same change as Simple, with exact numbers" : "click a color and the whole effect follows"}
              />
              {advanced ? (
                <ShiftSliders
                  edit={edit}
                  main={gameMain}
                  disabled={disabled || !gameMain}
                  change={(next) => { clearPick(); commit(tree.target, next); }}
                />
              ) : (
                <section className="color-section">
                  <h4>Or pick any color<small>the effect's main color becomes this one; the others follow</small></h4>
                  {gameMain ? (
                    <ColorPicker
                      label="Main color"
                      value={pickerValue}
                      original={unit(gameMain)}
                      originalLabel="Game"
                      changed={hasShift(edit)}
                      disabled={disabled}
                      onChange={pick}
                      onReset={() => { clearPick(); commit(tree.target, withoutShift(edit)); }}
                    />
                  ) : (
                    <p className="hint">This effect has no main color to follow: its colors are black, white or grey, or its parts have no color of their own. {tree.palette.length ? "Choose one of its colors below as the main color, or use Advanced to change single colors." : ""}</p>
                  )}
                </section>
              )}
              {info && <PickResult pick={info} />}
              <MainColor
                key={tree.target}
                tree={tree}
                index={mainAt}
                chosen={Boolean(shown.main)}
                yours={yoursMain}
                disabled={disabled}
                choose={chooseMain}
                open={!gameMain}
              />
              {advanced && (
                <Palette
                  tree={tree}
                  preview={preview}
                  edit={edit}
                  mainAt={mainAt}
                  selected={swatch && fold(swatch.target) === fold(tree.target) ? swatch.index : null}
                  select={(index) => setSwatch({ target: tree.target, index })}
                  disabled={disabled}
                  change={(next) => commit(tree.target, next)}
                  chooseMain={chooseMain}
                />
              )}
              <Multipliers tree={tree} edit={edit} advanced={advanced} disabled={disabled} change={(next) => commit(tree.target, next)} />
              {conflicts.length > 0 && (
                <p className="effects-conflict-line" role="note">
                  <ExtraIcon name="warning" size={15} />
                  Your own file is used for {conflicts.map((p) => p.label).join(", ")} (Replace a file); this edit changes the effect's other parts.
                </p>
              )}
              {notice && <p className="effects-error-line" role="alert">{notice}</p>}
              <div className="extra-replacement-actions effects-actions">
                <label className="extra-toggle">
                  <input type="checkbox" checked={Boolean(row) && row!.enabled !== false} disabled={disabled || !row} onChange={(e) => setIncluded(e.target.checked)} />
                  <span aria-hidden="true" />
                  <strong>Include in build</strong>
                </label>
                <button type="button" className="extra-text-button" disabled={disabled || (!row && !drafts[fold(tree.target)])} onClick={resetEffect}>Reset effect</button>
              </div>
              {!row && <p className="hint effects-foot">Nothing changed yet: this effect keeps the game's colors.</p>}
            </div>
          ) : (
            <div className="extra-editor-empty">
              <span className="extras-eyebrow">EFFECT COLORS</span>
              <h2>Choose an effect</h2>
              <p>Pick one of the hero's effects to give it a new color. Simple maps the whole effect to one color; Advanced changes single colors and leaves parts out.</p>
              <ul>
                <li><ExtraIcon name="check" size={16} />The game's own effect files stay unchanged</li>
                <li><ExtraIcon name="check" size={16} />Only the effects you change go into the mod</li>
                <li><ExtraIcon name="check" size={16} />Parts that keep their own colors are named</li>
              </ul>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}

function EffectList({ groups, rows, active, selected, select, toggleGroup }: { groups: ShownGroup[]; rows: EffectEditRow[]; active: boolean; selected: string; select: (target: string) => void; toggleGroup: (key: string) => void }) {
  const prefix = useId();
  const box = useRef<HTMLDivElement>(null);
  const flat = groups.flatMap((g) => g.trees);
  const optionId = (target: string) => `${prefix}-${flat.findIndex((t) => fold(t.target) === fold(target))}`;
  useEffect(() => {
    const list = box.current;
    const el = [...(list?.querySelectorAll<HTMLElement>("[data-target]") ?? [])].find((item) => fold(item.dataset.target ?? "") === fold(selected));
    if (!list || !el) return;
    const top = el.offsetTop - list.offsetTop, bottom = top + el.offsetHeight;
    // Arrow keys scroll as little as needed. A selection from elsewhere (opening the page,
    // a click, a harness) puts the row near the top: the list is often taller than the part
    // of it in the window, so a row at its bottom edge can sit below the window.
    const byKey = stepped.current;
    stepped.current = false;
    if (byKey) {
      if (top < list.scrollTop) list.scrollTop = top - 28;
      else if (bottom > list.scrollTop + list.clientHeight) list.scrollTop = bottom - list.clientHeight + 4;
    } else if (top < list.scrollTop || bottom > list.scrollTop + list.clientHeight * 0.5) list.scrollTop = Math.max(0, top - 28);
  }, [selected]);
  const stepped = useRef(false);
  function keys(event: KeyboardEvent<HTMLDivElement>) {
    const at = flat.findIndex((t) => fold(t.target) === fold(selected));
    const next = ({ ArrowDown: at + 1, ArrowUp: at < 0 ? 0 : at - 1, Home: 0, End: flat.length - 1, PageDown: at + 8, PageUp: at - 8 } as Record<string, number>)[event.key];
    if (next === undefined || !flat.length) return;
    event.preventDefault();
    stepped.current = true;
    select(flat[Math.max(0, Math.min(flat.length - 1, next))].target);
  }
  return (
    <div ref={box} className="color-list-scroll" role="listbox" aria-label="Effects" tabIndex={0} aria-activedescendant={flat.some((t) => fold(t.target) === fold(selected)) ? optionId(selected) : undefined} onKeyDown={keys}>
      {groups.map((group) => (
        <div role="group" aria-label={group.name} key={group.key || group.name} className="color-list-group">
          {group.folded ? (
            <div className="color-list-group-name effects-group-fold">
              <button type="button" aria-expanded={group.open} aria-label={`${group.name}, ${group.trees.length + group.hidden} effects`} onClick={() => toggleGroup(group.key)}>
                <i className="effects-fold-caret" aria-hidden="true" />
                <span>{group.name}<em> · {group.trees.length + group.hidden}</em></span>
              </button>
            </div>
          ) : (
            <div className="color-list-group-name" aria-hidden="true">{group.name}<em> · {group.trees.length}</em></div>
          )}
          {group.note && <p className="effects-group-note">{group.note}</p>}
          {group.byName && <p className="effects-group-note">{BY_NAME_GROUP}.</p>}
          {group.trees.map((tree) => {
            const row = rowOf(rows, tree.target);
            const swatches = treeSwatches(tree);
            const on = fold(tree.target) === fold(selected);
            return (
              <div key={tree.target} id={optionId(tree.target)} data-target={tree.target} role="option" aria-selected={on} className={`color-row effects-row ${on ? "selected" : ""}`} onClick={() => { stepped.current = true; select(tree.target); }}>
                <span className="effects-row-swatch" role="img" aria-label={swatches.length ? `Game colors ${swatches.map((s) => s.hex).join(", ")}` : "No colors"} title={swatches.map((s) => s.hex).join(" · ") || "No colors"}>
                  {swatches.length ? swatches.map((s, i) => <i key={i} style={{ background: s.hex }} />) : <i className="none" />}
                </span>
                <div className="color-row-text" title={`${tree.name}${byFileName(tree) && !group.byName ? `\n${BY_NAME}` : ""}${tree.also_used_by ? `\n${tree.also_used_by.note}` : ""}`}>
                  <strong>{tree.label}</strong>
                  <small>{tree.parts.length} part{tree.parts.length === 1 ? "" : "s"}</small>
                </div>
                {tree.also_used_by && <span className="color-pill" title={tree.also_used_by.note} aria-label={tree.also_used_by.note}>Also used</span>}
                {row && <span className={`color-pill ${rowActive(row, active) ? "changed" : ""}`}>{rowActive(row, active) ? "Changed" : "Paused"}</span>}
              </div>
            );
          })}
          {group.hidden > 0 && (
            <button type="button" className="extra-text-button effects-group-more" onClick={() => toggleGroup(group.key)}>
              Show {group.hidden} {group.trees.length ? "more " : ""}effect{group.hidden === 1 ? "" : "s"}
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

function Strip({ label, kind, stops, dim }: { label: string; kind: EffectTree["parts"][number]["strips"][number]["kind"]; stops: StripStop[]; dim?: boolean }) {
  const text = stripText(kind, stops);
  return (
    <div className="effects-strip-row">
      <span className="effects-strip-label">{label}</span>
      <span className={`effects-strip ${dim ? "dim" : ""}`} role="img" aria-label={`${label}: ${text}`} title={text} style={{ "--strip": stripBackground(kind, stops) } as CSSProperties}>
        {kind === "gradient" && stops.length > 1 && stops.map((s, i) => <i key={i} className="effects-strip-tick" style={{ left: `${Math.round(s.position * 1000) / 10}%` }} />)}
      </span>
    </div>
  );
}

function EffectDetails({ tree, preview, stale, previewFailed, retry, edit, advanced, disabled, names, replaced, conflicts, status, rows, toggle, setPart, player }: {
  tree: EffectTree; preview: EffectPreview; stale: boolean; previewFailed: string; retry: () => void; edit: EffectEdit; advanced: boolean; disabled: boolean;
  names: Map<string, string>; replaced: Set<string>; conflicts: number; status: string; rows: EffectEditRow[]; toggle: (part: EffectPart, on: boolean) => void;
  setPart: (path: string, patch: Partial<EffectPartSetting> | null) => void;
  player?: ReactNode;
}) {
  const after = new Map(preview.parts.map((p) => [fold(p.path), p]));
  // Saved order decides which of two edited effects writes a part they share.
  const order = new Map(rows.filter((r) => r.enabled !== false).map((r, i) => [fold(r.target), i]));
  const self = order.get(fold(tree.target));
  const hidden = advanced ? hiddenParts(tree, edit).length : 0;
  return (
    <div className="effects-details">
      <header className="effects-details-head">
        <h3>{tree.name || tree.label}</h3>
        <span className="color-count">{tree.parts.length} part{tree.parts.length === 1 ? "" : "s"}</span>
        {status && <span className={`color-pill ${status === "Changed" ? "changed" : ""}`}>{status}</span>}
        {hidden > 0 && <span className="effects-hidden-count">{hidden} part{hidden === 1 ? "" : "s"} hidden</span>}
      </header>
      {byFileName(tree) && <p className="effects-note effects-by-name">{BY_NAME}: the game's data does not link it to {tree.group_name}.</p>}
      {tree.also_used_by && <p className="effects-note">{tree.also_used_by.note}. {ALSO_USED[alsoUsedReach(tree, edit)]}</p>}
      {tree.missing.length > 0 && <p className="effects-note">{tree.missing.length} part{tree.missing.length === 1 ? " is" : "s are"} not in your game files and cannot be shown: {tree.missing.map((m) => m.split("/").pop()).join(", ")}.</p>}
      {/* The moving preview (with its Compare with the game switch), above the parts. */}
      {player}
      <div className="effects-parts-head">
        <h4>Parts and their colors</h4>
        <small>{advanced ? "untick a part to keep its game colors · hide a part, or adjust a 3D part by itself" : "the game's colors, and yours below them"}</small>
        {stale && <span className="effects-updating" role="status">Updating…</span>}
      </div>
      {previewFailed && (
        <p className="effects-error-line" role="alert">
          The preview could not be made: {previewFailed}{" "}
          <button type="button" className="extra-text-button" onClick={retry}>Try again</button>
        </p>
      )}
      {conflicts > 0 && <p className="effects-note">For a part you replaced on Replace a file, your own file is used.</p>}
      <ul className="effects-parts">
        {tree.parts.map((part) => {
          const others = part.in_trees.filter((t) => fold(t) !== fold(tree.target)).map((t, i) => {
            const at = order.get(fold(t));
            return { name: alsoPartOf(names, tree, [t])[0] ?? String(i), edited: at !== undefined && self !== undefined, earlier: at !== undefined && self !== undefined && at < self };
          });
          return (
          <PartRow
            key={part.path}
            tree={tree}
            part={part}
            after={after.get(fold(part.path))}
            left={leftOut(edit, part)}
            advanced={advanced}
            disabled={disabled}
            others={others}
            replaced={isReplaced(replaced, part.path)}
            toggle={(on) => toggle(part, on)}
            setting={partSetting(edit, part.path)}
            setPart={(patch) => setPart(part.path, patch)}
          />
          );
        })}
      </ul>
    </div>
  );
}

function PartRow({ part, after, left, advanced, disabled, others, replaced, toggle, setting, setPart }: {
  tree: EffectTree; part: EffectPart; after: EffectPreview["parts"][number] | undefined; left: "user" | "shared" | null; advanced: boolean; disabled: boolean;
  others: { name: string; edited: boolean; earlier: boolean }[]; replaced: boolean; toggle: (on: boolean) => void;
  setting: EffectPartSetting; setPart: (patch: Partial<EffectPartSetting> | null) => void;
}) {
  const excluded = left !== null;
  // Advanced only: the part's kind, its Shown / Hidden switch and a 3D part's own settings.
  const offers = part.controls;
  const own = advanced && Boolean(offers) && (offers.size.offered || offers.opacity.offered || offers.tint.offered || Boolean(offers.light?.offered));
  const hidden = advanced && Boolean(setting.hidden) && !excluded && Boolean(offers?.hide.offered);
  const [open, setOpen] = useState(() => setting.size !== undefined || setting.opacity !== undefined || Boolean(setting.tint) || setting.light !== undefined);
  const strips = part.strips;
  const quiet = !strips.length;
  const quietText = !part.visible ? (part.children.length ? "Draws nothing itself; holds the parts below" : "Draws nothing") : "No color setting of its own";
  const summary = quiet ? quietText : [...new Set(strips.map((s) => KIND_TEXT[s.kind]))].join(" · ");
  const notes: { tone: string; text: string }[] = [];
  if (left === "user") notes.push({ tone: "kept", text: "Left out: keeps the game's look (its colors, size, brightness and opacity)." });
  else if (left === "shared") notes.push({ tone: "kept", text: advanced ? "Left out: other heroes use this part too, so it keeps the game's look. Ticking it changes it for them too." : "Left out: other heroes use this part too, so it keeps the game's look (Advanced can include it)." });
  else if (part.outside_hero) notes.push({ tone: "fixed", text: "Included: other heroes use this part too, and your change shows on theirs as well." });
  if (part.does_not_follow.length) notes.push({ tone: "fixed", text: `This part keeps its own colors: ${[...new Set(part.does_not_follow.map((n) => n.reason))].join("; ")}.` });
  if (part.may_not_follow.length) notes.push({ tone: "maybe", text: `May keep its colors: ${[...new Set(part.may_not_follow.map((n) => n.reason))].join("; ")}.` });
  if (part.size_limit !== undefined) notes.push({ tone: "", text: `Size limited by the game: it grows to at most ${Math.round(part.size_limit * 1000) / 10}% of the screen.` });
  // A part left out gets no change at all; the left-out note says so. Otherwise say which sliders do not reach it.
  // Advanced: a 3D part with settings of its own says so in its panel instead.
  if (!excluded && !own) for (const why of partUnreached(part)) notes.push({ tone: "", text: why });
  else for (const why of new Set((after?.multipliers ?? []).filter((m) => !m.applied && m.reason).map((m) => m.reason))) notes.push({ tone: "", text: why });
  // The build's rules for a part two changes meet in: the user's own file wins; of two
  // edited effects, the one changed first (earlier in the saved list) wins.
  const first = others.filter((o) => o.edited && o.earlier).map((o) => o.name);
  const later = others.filter((o) => o.edited && !o.earlier).map((o) => o.name);
  const plain = others.filter((o) => !o.edited).map((o) => o.name);
  if (first.length) notes.push({ tone: "fixed", text: `Also part of ${first.join(", ")}, which was changed first: that effect's edit is used for this part.` });
  if (later.length) notes.push({ tone: "", text: `Also part of ${later.join(", ")}, changed later: this effect's edit is used for this part.` });
  if (plain.length) notes.push({ tone: "", text: `Also part of ${plain.join(", ")}.` });
  if (replaced) notes.push({ tone: "fixed", text: "Your own file is used for this part (Replace a file); the color edit changes the effect's other parts." });
  const afterStrips = new Map((after?.strips ?? []).map((s) => [s.id, s]));
  // "No color of its own" explains the Tint; while This part is open the Tint is right below, so its panel says it instead.
  const kindNote = advanced && !(open && part.kind?.note === NO_OWN_COLOR_NOTE) ? part.kind?.note : undefined;
  return (
    <li className={`effects-part ${quiet ? "quiet" : ""} ${excluded ? "excluded" : ""} ${advanced ? "advanced-row" : ""} ${hidden ? "hidden-part" : ""}`} style={{ "--depth": Math.max(0, part.depth - 1) } as CSSProperties}>
      <div className="effects-part-name">
        {advanced && (!quiet || (part.outside_hero && part.visible)) ? (
          <input type="checkbox" aria-label={`Recolor ${part.label}`} checked={!excluded} disabled={disabled} onChange={(e) => toggle(e.target.checked)} />
        ) : advanced ? <span className="effects-part-nobox" aria-hidden="true" /> : null}
        <div>
          <strong title={part.path}>
            {part.label}
            {advanced && part.kind && part.kind.kind !== "none" && <span className="effects-kind" title={part.kind.mixed ? `Draws more than one kind of thing: ${part.kind.label}` : undefined}>{part.kind.label}</span>}
          </strong>
          <small>{summary}</small>
        </div>
      </div>
      {advanced && offers && (offers.hide.offered || own) && (
        <div className="effects-part-switches">
          {offers.hide.offered && (
            <button
              type="button"
              className="effects-shown"
              aria-pressed={Boolean(setting.hidden)}
              aria-label={`${setting.hidden ? "Hidden" : "Shown"}: ${part.label}`}
              title={excluded ? "Left out of the edit: it keeps the game's look" : setting.hidden ? "Show this part again" : "Hide this part"}
              disabled={disabled || excluded}
              onClick={() => setPart({ hidden: setting.hidden ? undefined : true })}
            >
              <i aria-hidden="true" className={setting.hidden ? "off" : "on"} />
              {setting.hidden ? "Hidden" : "Shown"}
            </button>
          )}
          {own && (
            <button type="button" className="effects-this-part" aria-expanded={open} disabled={excluded && !open} onClick={() => setOpen(!open)}>
              This part<i aria-hidden="true" className="effects-fold-caret" />
            </button>
          )}
        </div>
      )}
      {(hidden || kindNote) && (
        <div className="effects-part-lines">
          {hidden && <p className="effects-hidden-line" role="note">Hidden: this part is not drawn. The rest of the effect is unchanged{part.children.length ? "; the parts below it keep working" : ""}.</p>}
          {kindNote && <p className="effects-kind-line">{kindNote}</p>}
        </div>
      )}
      {!quiet && (
        <div className={`effects-part-strips ${hidden ? "dimmed" : ""}`}>
          {strips.map((strip) => {
            const yours = afterStrips.get(strip.id);
            return (
              <div className="effects-strip-pair" key={strip.id}>
                {strips.length > 1 && <span className="effects-strip-role">{strip.role}</span>}
                <Strip label="Game" kind={strip.kind} stops={gameStops(strip.stops)} />
                <Strip label="Yours" kind={strip.kind} stops={yours ? afterStops(yours.stops) : gameStops(strip.stops)} dim={excluded || hidden} />
              </div>
            );
          })}
        </div>
      )}
      {own && open && <PartPanel part={part} after={after} setting={setting} excluded={excluded} disabled={disabled} setPart={setPart} />}
      {notes.length > 0 && (
        <ul className="effects-part-notes">
          {notes.map((n, i) => <li key={i} className={n.tone}>{n.text}</li>)}
        </ul>
      )}
    </li>
  );
}

const WHITE: Rgb = [255, 255, 255];
const NOT_IN_PLAYER = "The moving preview does not show this.";

/** "This part" (Advanced): a 3D part's own Size and Opacity, and a Tint when its file gives
 * it no color. What the part takes, and why not, comes from Rust (`part.controls`). A slider
 * or color drag is one step of the project's undo history. */
function PartPanel({ part, after, setting, excluded, disabled, setPart }: {
  part: EffectPart; after: EffectPreview["parts"][number] | undefined; setting: EffectPartSetting; excluded: boolean; disabled: boolean;
  setPart: (patch: Partial<EffectPartSetting> | null) => void;
}) {
  const history = useHistory();
  const historyRef = useRef(history);
  historyRef.current = history;
  const dragging = useRef(false);
  const drag = (on: boolean) => { if (dragging.current === on) return; dragging.current = on; historyRef.current?.gesture(on); };
  useEffect(() => () => { if (dragging.current) historyRef.current?.gesture(false); }, []);
  const offers = part.controls;
  const any = setting.size !== undefined || setting.opacity !== undefined || Boolean(setting.tint) || Boolean(setting.hidden) || setting.light !== undefined;
  // The effect-wide Size multiplies a size this part's file already sets; elsewhere it does not reach a 3D part.
  const wideReaches = part.multipliers.some((m) => m.multiplier !== "glow" && m.status === "changes");
  const why = (key: "size" | "opacity" | "tint" | "light") => after?.controls?.find((c) => c.control === key && !c.applied && c.status !== "left_out")?.reason;
  // A part that is only a light shows no 3D settings.
  const model = offers.size.offered || offers.opacity.offered || offers.tint.offered;
  const light = offers.light?.offered ? offers.light : null;
  const id = useId();
  return (
    <div className="effects-part-panel" role="group" aria-labelledby={`${id}-title`}>
      <div className="effects-part-panel-head">
        <strong id={`${id}-title`}>This part</strong>
        <span className="color-picker-spacer" />
        <button type="button" className="extra-text-button" disabled={disabled || !any} onClick={() => setPart(null)}>Reset this part</button>
      </div>
      {excluded && <p className="hint">Left out of the edit: this part keeps the game's look, so these settings do nothing until it is ticked again.</p>}
      {offers.tint.offered && (
        <div className="effects-part-tint">
          <ColorPicker
            label="Tint"
            value={toUnit(setting.tint ?? WHITE)}
            original={toUnit(WHITE)}
            originalLabel="No tint"
            yoursLabel="Tint"
            changed={Boolean(setting.tint)}
            disabled={disabled || excluded}
            quick={false}
            onDrag={drag}
            onChange={(c) => setPart({ tint: toByte(c) })}
            onReset={() => setPart({ tint: undefined })}
          />
          <div className="row-actions">
            <button type="button" className="extra-secondary" disabled={disabled || excluded || !setting.tint} onClick={() => setPart({ tint: undefined })}>Remove tint</button>
          </div>
          {setting.tint && !offers.tint.player_shows && <small className="effects-player-gap">{NOT_IN_PLAYER}</small>}
          {why("tint") && <small className="effects-reach none">{why("tint")}.</small>}
        </div>
      )}
      {model && (["size", "opacity"] as const).map((key) => {
        const label = key === "size" ? "Size" : "Opacity";
        if (!offers[key].offered) return <p key={key} className="hint">{label}: {offers[key].reason}.</p>;
        return (
          <div key={key} className="effects-multiplier">
            <GradientSlider
              label={label}
              name={`${part.label} ${label.toLowerCase()}`}
              value={Math.round((setting[key] ?? 1) * 1000) / 1000}
              min={MULTIPLIER_MIN}
              max={MULTIPLIER_MAX}
              step={0.05}
              fine={0.01}
              unit={percentUnit}
              track="linear-gradient(90deg, var(--surface-secondary), var(--accent))"
              handle="var(--accent)"
              disabled={disabled || excluded}
              onDrag={drag}
              onChange={(v) => setPart({ [key]: v })}
            />
            {setting[key] !== undefined && !offers[key].player_shows && <small className="effects-player-gap">{NOT_IN_PLAYER}</small>}
            {why(key) && <small className="effects-reach none">{why(key)}.</small>}
          </div>
        );
      })}
      {model && (
        <p className="hint">
          {wideReaches
            ? "The effect's Size and Opacity on the right also change this part where its file sets them; these multiply on top."
            : "3D parts are not reached by the effect's Size and Opacity sliders on the right, so they have their own here."}
          {offers.tint.offered ? part.kind?.note === NO_OWN_COLOR_NOTE ? " The game's files give this part no color of its own; the tint colors the whole object, on top of the material the game draws it with." : " The tint colors the whole object, on top of the material the game draws it with." : ""}
        </p>
      )}
      {light && (
        <div className="effects-multiplier">
          <GradientSlider
            label="Light brightness"
            name={`${part.label} light brightness`}
            value={Math.round((setting.light ?? 1) * 1000) / 1000}
            min={LIGHT_MIN}
            max={LIGHT_MAX}
            step={0.05}
            fine={0.01}
            unit={percentUnit}
            track="linear-gradient(90deg, var(--surface-secondary), var(--accent))"
            handle="var(--accent)"
            disabled={disabled || excluded}
            onDrag={drag}
            onChange={(v) => setPart({ light: v })}
          />
          {setting.light !== undefined && !light.player_shows && <small className="effects-player-gap">{NOT_IN_PLAYER}</small>}
          {light.note && <small className="effects-reach none">{light.note}.</small>}
          <p className="hint">Changes how strongly this light shines on its surroundings. At 0 the light is dimmer but may not go out; to remove it, hide the part.</p>
        </div>
      )}
    </div>
  );
}

function Presets({ tree, main, yours, hasShift: shifted, disabled, pick, title, hint }: { tree: EffectTree; main: Rgb | null; yours: Rgb | null; hasShift: boolean; disabled: boolean; pick: (rgb: Rgb) => void; title: string; hint: string }) {
  return (
    <section className="color-section">
      <h4>{title}<small>{hint}</small></h4>
      <div className="effects-presets" role="group" aria-label="Color presets">
        {PRESETS.map((p) => {
          const on = shifted && Boolean(yours) && sameRgb(yours, p.rgb);
          return (
            <button key={p.name} type="button" className="effects-preset" aria-pressed={on} aria-label={`${p.name} ${hexOf(p.rgb)}`} title={`${p.name} ${hexOf(p.rgb)}`} disabled={disabled || !main} style={{ background: cssColor(unit(p.rgb)) }} onClick={() => pick(p.rgb)} />
          );
        })}
      </div>
      {!main && tree.palette.length > 0 && <p className="hint">Choose a main color below first.</p>}
    </section>
  );
}

function ShiftSliders({ edit, main, disabled, change }: { edit: EffectEdit; main: Rgb | null; disabled: boolean; change: (edit: EffectEdit) => void }) {
  const hue = edit.hue ?? 0, saturation = edit.saturation ?? 1, brightness = edit.brightness ?? 1;
  // The tracks show what each value does to the main color: the middle of Shift hue and
  // the 100% marks are the game's main color.
  const [h, s, b] = toHsb(unit(main ?? [128, 128, 128]));
  const at = (hsb: [number, number, number]) => cssColor(fromHsb(hsb));
  const shiftTrack = `linear-gradient(90deg, ${[0, 1, 2, 3, 4, 5, 6].map((k) => `${at([h - 180 + 60 * k, Math.max(s, 0.6), Math.max(b, 0.6)])} ${Math.round((k / 6) * 10000) / 100}%`).join(", ")})`;
  const satTrack = `linear-gradient(90deg, ${at([h, 0, b])}, ${at([h, s, b])} 25%, ${at([h, 1, b])})`;
  const briTrack = `linear-gradient(90deg, #000, ${at([h, s, b])} 25%, ${at([h, s * 0.4, 1])})`;
  return (
    <div className="color-sliders-block">
      <GradientSlider label="Shift hue" name="Shift hue" value={Math.round(((((hue + 180) % 360) + 360) % 360 - 180) * 10) / 10} min={-180} max={180} step={1} fine={0.1} unit={shiftUnit} track={shiftTrack} disabled={disabled} onChange={(v) => change({ ...edit, hue: v })} />
      <GradientSlider label="Saturation" name="Effect saturation" value={Math.round(saturation * 1000) / 1000} min={0} max={FACTOR_MAX} step={0.05} fine={0.01} unit={percentUnit} track={satTrack} disabled={disabled} onChange={(v) => change({ ...edit, saturation: v })} />
      <GradientSlider label="Brightness" name="Effect brightness" value={Math.round(brightness * 1000) / 1000} min={0} max={FACTOR_MAX} step={0.05} fine={0.01} unit={percentUnit} track={briTrack} disabled={disabled} onChange={(v) => change({ ...edit, brightness: v })} />
      <p className="hint">The middle of Shift hue and 100% leave the colors as the game has them.</p>
    </div>
  );
}

function PickResult({ pick }: { pick: EffectPick }) {
  return (
    <div className={`effects-pick ${pick.exact ? "" : "inexact"}`} role="status">
      {!pick.exact && (
        <p>
          <span className="effects-chip" style={{ background: cssColor(unit(pick.result)), color: textOn(unit(pick.result)) }}>{pick.result_hex}</span>
          The main color can only get to {pick.result_hex}{pick.reason ? `: ${pick.reason}` : "."}
        </p>
      )}
      {pick.warning && <p className="effects-warning"><ExtraIcon name="warning" size={14} />{pick.warning}</p>}
    </div>
  );
}

function MainColor({ tree, index, chosen, yours, disabled, choose, open: startOpen }: { tree: EffectTree; index: number | null; chosen: boolean; yours: Rgb | null; disabled: boolean; choose: (index: number) => void; open: boolean }) {
  const [open, setOpen] = useState(startOpen);
  const main = index !== null ? tree.palette[index] : null;
  if (!tree.palette.length) return null;
  return (
    <section className="color-section effects-main">
      <div className="effects-main-row">
        <span className="color-section-label">Main color</span>
        {main ? (
          <>
            <span className="effects-chip" style={{ background: main.hex, color: textOn(unit(main.rgb)) }} title={`Game ${main.hex}`}>Game {main.hex}</span>
            {yours && !sameRgb(yours, main.rgb) && <span className="effects-chip" style={{ background: cssColor(unit(yours)), color: textOn(unit(yours)) }} title={`Yours ${hexOf(yours)}`}>Yours {hexOf(yours)}</span>}
            <small>{chosen ? "your choice" : tree.main ? `found: ${Math.round(tree.main.share * 100)}% of the colorful parts` : ""}</small>
          </>
        ) : <small>none found</small>}
        <span className="color-picker-spacer" />
        <button type="button" className="extra-text-button" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? "Done" : "Not the main color?"}</button>
      </div>
      {open && (
        <div className="effects-main-choose">
          <p className="hint">Use one of the effect's colors as the main color: picking a color then maps this one onto it.</p>
          <div className="effects-swatches" role="group" aria-label="Choose the main color">
            {tree.palette.map((s, i) => (
              <button key={i} type="button" className="effects-swatch" aria-pressed={i === index} aria-label={`Use ${s.hex} as the main color`} title={`${s.hex} · ${Math.round(s.weight * 100)}%${s.neutral ? ` · ${s.neutral}` : ""}`} disabled={disabled} style={{ background: s.hex }} onClick={() => choose(i)} />
            ))}
          </div>
        </div>
      )}
    </section>
  );
}

function Palette({ tree, preview, edit, mainAt, selected, select, disabled, change, chooseMain }: {
  tree: EffectTree; preview: EffectPreview; edit: EffectEdit; mainAt: number | null; selected: number | null; select: (index: number) => void;
  disabled: boolean; change: (edit: EffectEdit) => void; chooseMain: (index: number) => void;
}) {
  if (!tree.palette.length) return <section className="color-section"><h4>Colors in this effect</h4><p className="hint">This effect has no colors of its own to change.</p></section>;
  const swatch = selected !== null ? tree.palette[selected] : null;
  const replacement = swatch ? replacementOf(edit, swatch.rgb) : null;
  const yours = selected !== null ? preview.palette[selected]?.after ?? swatch?.rgb : null;
  const partNames = new Map(tree.parts.map((p) => [fold(p.path), p.label]));
  return (
    <section className="color-section">
      <h4>Colors in this effect<small>select one to change only that color</small></h4>
      <div className="effects-swatches" role="group" aria-label="Colors in this effect">
        {tree.palette.map((s, i) => {
          const r = replacementOf(edit, s.rgb);
          return (
            <button key={i} type="button" className="effects-swatch" aria-pressed={i === selected} aria-label={`${s.hex}${r ? `, changed to ${hexOf(r.to)}` : ""}${i === mainAt ? ", main color" : ""}`} title={`${s.hex} · ${Math.round(s.weight * 1000) / 10}% of the color${s.neutral ? ` · ${s.neutral}` : ""}${r ? ` → ${hexOf(r.to)}` : ""}`} disabled={disabled} style={{ background: s.hex }} onClick={() => select(i)}>
              {r && <u style={{ background: cssColor(unit(r.to)) }} />}
              {i === mainAt && <b aria-hidden="true">M</b>}
            </button>
          );
        })}
      </div>
      {swatch && selected !== null ? (
        <div className="effects-one-color">
          <p className="effects-one-head">
            <strong>{swatch.hex}</strong> used by {swatch.parts.map((p) => partNames.get(fold(p)) ?? p.split("/").pop()).join(", ")}
          </p>
          <ColorPicker
            label="Replacement color"
            value={unit(replacement?.to ?? yours ?? swatch.rgb)}
            original={unit(swatch.rgb)}
            originalLabel="Game"
            changed={Boolean(replacement)}
            disabled={disabled}
            onChange={(c) => change(withColor(edit, swatch.rgb, toByte(c)))}
            onReset={() => change(withColor(edit, swatch.rgb, swatch.rgb))}
          />
          <label className="hero-color-check">
            <input type="checkbox" checked={Boolean(replacement?.near)} disabled={disabled || !replacement} onChange={(e) => replacement && change(withColor(edit, swatch.rgb, replacement.to, e.target.checked))} />
            Also change colors close to it
          </label>
          <div className="row-actions">
            <button type="button" className="extra-secondary" disabled={disabled || !replacement} onClick={() => change(withColor(edit, swatch.rgb, null))}>Follow the whole-effect change</button>
            <button type="button" className="extra-secondary" disabled={disabled || selected === mainAt} onClick={() => chooseMain(selected)}>Use this as the main color</button>
          </div>
          <p className="hint">“Game color” keeps this color as the game has it, whatever the whole-effect change does.</p>
        </div>
      ) : (
        (edit.colors?.length ?? 0) > 0 && <p className="hint">{edit.colors!.length} color{edit.colors!.length === 1 ? " has" : "s have"} a color of its own (marked with a dot).</p>
      )}
    </section>
  );
}

function Multipliers({ tree, edit, advanced = false, disabled, change }: { tree: EffectTree; edit: EffectEdit; advanced?: boolean; disabled: boolean; change: (edit: EffectEdit) => void }) {
  // Advanced: under Size, the 3D parts that have a Size of their own.
  const own = advanced ? ownSizeText(tree) : "";
  // The counts describe the default edit: parts other heroes use are not reached until ticked in.
  const outside = tree.parts.filter((p) => p.outside_hero && p.visible).length;
  const ticked = tree.parts.filter((p) => p.outside_hero && p.visible && leftOut(edit, p) === null).length;
  return (
    <section className="color-section">
      <h4>Shape and strength<small>applies to every part that has the setting</small></h4>
      {MULTIPLIERS.map(({ key, label }) => {
        const counts = tree.multipliers?.[key];
        const reason = unreachedReason(key, counts);
        const value = edit[key] ?? 1;
        return (
          <div key={key} className="effects-multiplier">
            <GradientSlider
              label={label}
              name={`Effect ${label.toLowerCase()}`}
              value={Math.round(value * 1000) / 1000}
              min={MULTIPLIER_MIN}
              max={MULTIPLIER_MAX}
              step={0.05}
              fine={0.01}
              unit={percentUnit}
              track="linear-gradient(90deg, var(--surface-secondary), var(--accent))"
              handle="var(--accent)"
              disabled={disabled || Boolean(reason)}
              onChange={(v) => change({ ...edit, [key]: v })}
            />
            <small className={reason ? "effects-reach none" : "effects-reach"}>{reason ?? reachText(counts)}{!reason && outside ? (ticked ? `, plus ${ticked} you included that other heroes use` : ` (${outside} that other heroes use ${outside === 1 ? "is" : "are"} left out)`) : ""}{key === "size" && own ? ` · ${own}` : ""}</small>
          </div>
        );
      })}
    </section>
  );
}
