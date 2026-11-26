// Extras → Objects: replace an object a hero's ability throws or shows with a model of your
// own. The hero's objects (Rust `ability_objects::catalog`) are asked for only when this tab is
// opened, once per project and hero; selecting one writes nothing. Choosing a file writes ONE
// project step (the row) and THEN prepares the file as an operation, because a project write
// during an operation would start the undo history afresh. The 3D view draws the game's object
// and the user's fitted object from `abilityObjectFit`: the first answer for an object reads
// the game's file under the operation lease (slow), so it starts on a click or with Load 3D
// previews automatically, never by selecting; after that each fit change asks again, one
// request at a time, the newest waiting one winning.
// i18n-area: objects
import { lazy, Suspense, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { Project } from "../models";
import { api, basename } from "../api";
import { m, mc, t, tc, tp, useLanguage, useT } from "../i18n";
import { T } from "../i18n/T";
import { shownHero, useGameNames } from "../i18n/gameNames";
import { openFor } from "../dialogs";
import { useHistory } from "../history/useProjectHistory";
import { useAutoLoad } from "../preview/autoLoad";
import { StudioTopBar, StudioView } from "../studio/Studio";
import { ExtraIcon } from "./Icons";
import { previewError } from "./media";
import {
  FitQueue,
  isMessage,
  kindText,
  nameFromTarget,
  needsSettings,
  objectGroups,
  parseFitAnswer,
  type FitAnswer,
  type ObjectItem,
} from "./objectCatalog";
import {
  acceptedSource,
  cleanFit,
  objectRowOf,
  objectStatus,
  percentOf,
  savedObjectRows,
  turned,
  unturned,
  withBase,
  withEnabled,
  withFit,
  withFollow,
  withObjectRows,
  withScale,
  withSize,
  withSource,
  withoutRow,
  type TurnAxis,
} from "./objectRows";
import {
  MAX_ABILITY_OBJECTS,
  OBJECT_NOT_PREPARED,
  OBJECT_SCALE_MAX,
  OBJECT_SCALE_MIN,
  OBJECT_SOURCE_SUFFIXES,
  OBJECT_UNSUPPORTED_FILE,
  OBJECTS_NO_GAME,
  SCENERY_NOTE,
  type ObjectCatalog,
  type ObjectFit,
  type ObjectPreparation,
  type ObjectRow,
} from "./objectTypes";
import type { ObjectViewMode } from "./ObjectScene";
import "./objects.css";

const ObjectScene = lazy(() => import("./ObjectScene"));
import type { ObjectMotion } from "./ObjectScene";
import { MovingPanel, ObjectPlayer, STILL_LINE, type PlayerPlays } from "./ObjectMoving";
import type { ObjectClip, ObjectFollow } from "./objectTypes";
const fold = (value: string) => value.toLowerCase();
/** What the view moves: your fitted model, or the game's object (no file chosen; or a file not
 * prepared yet, or your model at rest). */
const playerPlays = (chosen: boolean, answer: FitAnswer): PlayerPlays =>
  !chosen ? "no_file" : !answer.yours || answer.view.moving?.still ? "game" : "yours";

/** The size slider's range in percent; the value box takes the whole saved range. */
export const SLIDER_MIN = 25;
export const SLIDER_MAX = 400;
export const PREPARE_FAILED = m("Your object file could not be prepared");
export const SHOW_FAILED = m("The game's object could not be shown");

// Kept for the session (the tab, and the Extras page, come and go).
const catalogs = new Map<string, ObjectCatalog>();
/** Objects whose game file the backend has read (the first fit answer): later fits are quick. */
const answered = new Set<string>();
/** The last view answer per object. */
const answers = new Map<string, FitAnswer>();
/** The last preparation answer per object (shown, never saved). */
const preparations = new Map<string, ObjectPreparation>();
/** Tests: forget everything kept. */
export function forgetObjects() {
  catalogs.clear();
  answered.clear();
  answers.clear();
  preparations.clear();
}
/** The number of the hero's listed objects, or null before the tab was opened. */
export function keptObjectCount(heroModel: string) {
  return catalogs.get(fold(heroModel))?.counts.listed ?? null;
}
const keepAnswer = (target: string, answer: FitAnswer) => {
  answers.delete(target);
  answers.set(target, answer);
  while (answers.size > 8) answers.delete(answers.keys().next().value!);
};

/** The app's `run`: `keepError` keeps the failure line of the operation before (automatic loads). */
export type Operate = <T>(label: string, operation: () => Promise<T>, canCancel?: boolean, refreshTimings?: boolean, keepError?: boolean) => Promise<T | undefined>;
export interface ObjectsProps {
  project: Project;
  update: (project: Project) => void;
  disabled: boolean;
  /** Whether a game archive is set. */
  canLoad: boolean;
  /** The hero's name. */
  hero: string;
  /** Bumped by Reload assets: read the objects again. */
  revision: number;
  /** Whether the Objects tab is the one shown (it stays mounted once opened). */
  shown: boolean;
  /** The app's operation runner (busy state, Cancel, the one error line). */
  operate?: Operate;
  /** Settings → Appearance → Load 3D previews automatically. */
  autoPreviews?: boolean;
  openSetup?: () => void;
  reviewBuild?: () => void;
  fail: (title: string, error: unknown) => void;
  /** Called after the catalog was read (the tab shows its count). */
  loaded?: () => void;
}
type ViewState = { target: string; status: "loading" | "failed"; message: string } | null;

const materialName = (path: string) => basename(path).replace(/\.vmat(_c)?$/i, "").replace(/_/g, " ");
export const OWN_COLORS_SHOW = m("Your object's own colors and images show in game.");
/** What the game draws an object in, when not every effect overrides its material (in the language shown). */
export const materialNote = (override?: string) =>
  override
    ? t("Some of the effects that draw this object use a material of their own ({material}), so your object's colors and images show only in the others.", { material: materialName(override) })
    : t(OWN_COLORS_SHOW);
/** The heroes' names as a list: "A", "A and B", "A, B and C" (the names as they come). */
/** Hero names (English, from the catalog) in a list, each as the game shows it in the interface language. */
const listText = (heroes: string[]) => {
  const names = heroes.map(shownHero);
  return names.length < 2 ? names.join("") : t("{names} and {last}", { names: names.slice(0, -1).join(tc("list separator", ", ")), last: names[names.length - 1] });
};

export function Objects({ project, update, disabled, canLoad, hero, revision, shown, operate, autoPreviews = false, openSetup, reviewBuild, fail, loaded }: ObjectsProps) {
  const heroKey = fold(project.original_model);
  const [catalog, setCatalog] = useState<ObjectCatalog | null>(() => catalogs.get(heroKey) ?? null);
  const [loadState, setLoadState] = useState<"idle" | "loading" | "ready" | "failed">(catalog ? "ready" : "idle");
  const [loadError, setLoadError] = useState("");
  const [replacedOnly, setReplacedOnly] = useState(false);
  const [selected, setSelected] = useState("");
  const [mode, setMode] = useState<ObjectViewMode>("both");
  const [collisionShown, setCollisionShown] = useState(true);
  const [reset, setReset] = useState(0);
  const [view, setView] = useState<ViewState>(null);
  const [, setVersion] = useState(0);
  const bump = () => setVersion((v) => v + 1);
  const [unprepared, setUnprepared] = useState<Set<string>>(() => new Set());
  const [preparing, setPreparing] = useState<string | null>(null);
  const [localBusy, setLocalBusy] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [pickError, setPickError] = useState("");
  const language = useLanguage();
  const t = useT();
  useGameNames();
  const projectRef = useRef(project);
  projectRef.current = project;
  const updateRef = useRef(update);
  updateRef.current = update;
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const busy = disabled || localBusy;
  const active = Boolean(project.customization.enabled);
  const saved = savedObjectRows(project);
  const replaced = saved.filter((r) => r.source).length;

  // The catalog: asked once per project and hero while this tab lives, and after Reload assets.
  const attempted = useRef("");
  const loadedRef = useRef(loaded);
  loadedRef.current = loaded;
  function loadCatalog(reload: boolean) {
    const asked = projectRef.current, key = fold(asked.original_model);
    setLoadState("loading");
    setLoadError("");
    void Promise.resolve()
      .then(() => api.abilityObjectCatalog(asked, reload))
      .then(
        (value) => {
          catalogs.set(key, value);
          if (!alive.current || fold(projectRef.current.original_model) !== key) return;
          setCatalog(value);
          setLoadState("ready");
          loadedRef.current?.();
        },
        (error) => {
          if (!alive.current) return;
          setLoadError(previewError(error));
          setLoadState("failed");
        },
      );
  }
  useEffect(() => {
    const key = `${project.id}:${heroKey}`;
    if (!canLoad || attempted.current === key) return;
    attempted.current = key;
    loadCatalog(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per project and hero
  }, [canLoad, project.id, heroKey]);
  const seenRevision = useRef(revision);
  useEffect(() => {
    if (revision === seenRevision.current) return;
    seenRevision.current = revision;
    answered.clear();
    answers.clear();
    lastAsked.current = "";
    bump();
    if (canLoad) loadCatalog(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- on Reload assets only
  }, [revision, canLoad]);

  // The group titles are in the language shown: built again when it changes.
  // eslint-disable-next-line react-hooks/exhaustive-deps -- the language is read inside
  const groups = useMemo(() => objectGroups(catalog, saved, hero, replacedOnly), [catalog, saved, hero, replacedOnly, language]);
  const everything = useMemo(() => objectGroups(catalog, saved, hero, false).flatMap((g) => g.items), [catalog, saved, hero]);
  const item = everything.find((i) => i.key === selected) ?? null;
  const scenery = Boolean(item && catalog?.scenery?.some((target) => fold(target) === fold(item.target)));
  // A first selection once the list is there (it writes nothing): a replaced object, else the first offered one.
  useEffect(() => {
    if (selected || !everything.length) return;
    const first = everything.find((i) => i.saved?.source) ?? everything.find((i) => i.entry?.offered) ?? everything[0];
    setSelected(first.key);
  }, [everything, selected]);

  // The view.
  const latestTarget = useRef("");
  const lastAsked = useRef("");
  const queue = useRef<FitQueue<{ target: string; answer: FitAnswer; unprepared: boolean }> | null>(null);
  function received(result: { target: string; answer: FitAnswer; unprepared: boolean }) {
    keepAnswer(result.target, result.answer);
    setUnprepared((old) => {
      if (old.has(result.target) === result.unprepared) return old;
      const next = new Set(old);
      if (result.unprepared) next.add(result.target);
      else next.delete(result.target);
      return next;
    });
    setView((old) => (old?.target === result.target ? null : old));
    bump();
  }
  if (!queue.current)
    queue.current = new FitQueue(
      (result) => { if (alive.current) received(result); },
      (error) => { if (alive.current) setView({ target: latestTarget.current, status: "failed", message: previewError(error) }); },
    );
  /** The view's data for `target` with its saved file and fit. A saved file that is not prepared
   * (or changed since) draws the game's object alone, and the page offers to prepare it. */
  function fitCall(p: Project, target: string) {
    const row = objectRowOf(savedObjectRows(p), target);
    const fit = cleanFit(row?.fit), source = row?.source || null;
    const ask = (from: string | null) => Promise.resolve().then(() => api.abilityObjectFit(p, target, from, fit)).then(parseFitAnswer);
    return ask(source).then(
      (answer) => ({ target: fold(target), answer, unprepared: false }),
      (error) => {
        if (source && isMessage(previewError(error), OBJECT_NOT_PREPARED)) return ask(null).then((answer) => ({ target: fold(target), answer, unprepared: true }));
        throw error;
      },
    );
  }
  const keyFor = (p: Project, target: string) => {
    const row = objectRowOf(savedObjectRows(p), target);
    return `${fold(target)}\n${row?.source ?? ""}\n${JSON.stringify(cleanFit(row?.fit))}${row?.follow ? `\n${JSON.stringify(row.follow)}` : ""}`;
  };
  // Every change of the selected object's file or fit (also by Undo) asks again, once its view has answered.
  const offered = Boolean(item?.entry?.offered);
  const fitKey = item && offered && answered.has(item.key) ? keyFor(project, item.target) : null;
  useEffect(() => {
    if (!fitKey || !item || fitKey === lastAsked.current) return;
    lastAsked.current = fitKey;
    latestTarget.current = item.key;
    const p = projectRef.current, target = item.target;
    queue.current!.request(() => fitCall(p, target));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed by the object, its file and fit
  }, [fitKey]);

  /** The local stand-in for the app's runner (pages shown without one). */
  async function runLocally<T>(label: string, operation: () => Promise<T>): Promise<T | undefined> {
    if (localBusy) return undefined;
    setLocalBusy(true);
    try { return await operation(); } catch (error) { fail(label, error); return undefined; } finally { if (alive.current) setLocalBusy(false); }
  }
  const run: Operate = (label, operation, canCancel, refreshTimings, keepError) => (operate ? operate(label, operation, canCancel, refreshTimings, keepError) : runLocally(label, operation));

  /** Show this object: the first answer reads the game's file under the operation lease. A
   * failure is one quiet line in the view; false when the app was busy and nothing started. */
  async function show(target: string, auto = false): Promise<boolean> {
    const key = fold(target);
    let started = false;
    setView({ target: key, status: "loading", message: "" });
    await run(SHOW_FAILED, async () => {
      started = true;
      const p = projectRef.current;
      try {
        latestTarget.current = key;
        lastAsked.current = keyFor(p, target);
        queue.current!.drop();
        const result = await fitCall(p, target);
        answered.add(key);
        if (alive.current) received(result);
      } catch (error) {
        lastAsked.current = "";
        if (alive.current) setView({ target: key, status: "failed", message: previewError(error) });
      }
    }, true, true, auto);
    if (!started && alive.current) setView((old) => (old?.target === key && old.status === "loading" ? null : old));
    return started;
  }
  const viewing = view?.target === selected ? view : null;
  const hasAnswer = Boolean(item && answers.has(item.key));
  useAutoLoad({
    on: autoPreviews && shown,
    key: item && offered && !answered.has(item.key) && !hasAnswer && !viewing && !preparing ? `object:${project.id}:${item.key}` : null,
    busy,
    load: () => (item ? show(item.target, true) : false),
  });

  // Rows.
  function write(next: Project) { updateRef.current(next); }
  const rowsNow = () => savedObjectRows(projectRef.current);
  function setFit(target: string, fit: ObjectFit) { write(withObjectRows(projectRef.current, withFit(rowsNow(), target, fit), true)); }
  function setFollow(target: string, follow: ObjectFollow) { write(withObjectRows(projectRef.current, withFollow(rowsNow(), target, follow), true)); }
  const [motion, setMotion] = useState<ObjectMotion | null>(null);
  const [frame, setFrame] = useState(0);
  const [clips, setClips] = useState<ObjectClip[]>([]);
  async function chooseFile(target: string) {
    setPickError("");
    let picked: string | string[] | null = null;
    try {
      picked = await openFor("object-file", { title: t("Choose your object's model file"), multiple: false, directory: false, filters: [{ name: t("Blender, FBX, GLB or glTF"), extensions: [...OBJECT_SOURCE_SUFFIXES] }] });
    } catch (error) {
      fail(m("The file window could not be opened"), error);
      return;
    }
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    if (!acceptedSource(path)) { setPickError(OBJECT_UNSUPPORTED_FILE); return; }
    await prepareFile(target, path);
  }
  /** Writes the row (one step) when it changes, then prepares the file as an operation. */
  async function prepareFile(target: string, path: string) {
    const current = projectRef.current;
    const before = savedObjectRows(current);
    const rows = withSource(before, target, path);
    if (rows === before) return;
    const next = withObjectRows(current, rows, true);
    // The view is refreshed after the preparation, not by the write.
    lastAsked.current = keyFor(next, target);
    if (JSON.stringify(rows) !== JSON.stringify(before) || !current.customization.enabled) write(next);
    const key = fold(target);
    preparations.delete(key);
    setPreparing(key);
    setView({ target: key, status: "loading", message: "" });
    let started = false, prepared = false;
    await run(PREPARE_FAILED, async () => {
      started = true;
      queue.current!.drop();
      try {
        const answer = await api.prepareAbilityObject(next, target, path);
        preparations.set(key, answer.prepared);
        prepared = true;
        if (alive.current) setUnprepared((set) => { if (!set.has(key)) return set; const out = new Set(set); out.delete(key); return out; });
      } catch (error) {
        if (alive.current) setView((old) => (old?.target === key ? null : old));
        throw error;
      }
      // The view refreshes by itself; a view failure stays a quiet line, the preparation succeeded.
      try {
        latestTarget.current = key;
        lastAsked.current = keyFor(next, target);
        const result = await fitCall(next, target);
        answered.add(key);
        if (alive.current) received(result);
      } catch (error) {
        lastAsked.current = "";
        if (alive.current) setView({ target: key, status: "failed", message: previewError(error) });
      }
    }, true);
    if (alive.current) {
      setPreparing(null);
      if (!started) setView((old) => (old?.target === key && old.status === "loading" ? null : old));
      if (!prepared) {
        // Failed, cancelled or never started: the saved file is not prepared. The view keeps the
        // game's object only, the panel offers Prepare it again, and the next change asks again.
        lastAsked.current = "";
        const old = answers.get(key);
        if (old) keepAnswer(key, { ...old, yours: null });
        setUnprepared((set) => (set.has(key) ? set : new Set(set).add(key)));
        bump();
      }
    }
  }
  function remove(target: string) {
    write(withObjectRows(projectRef.current, withoutRow(rowsNow(), target), false));
    preparations.delete(fold(target));
  }
  function clearAll() {
    write(withObjectRows(projectRef.current, [], false));
    setConfirmClear(false);
  }

  const status = catalog
    ? tp(catalog.counts.listed, { one: "{hero} · {count} object from this hero's abilities, read from your game", other: "{hero} · {count} objects from this hero's abilities, read from your game" }, { hero })
    : loadState === "loading"
      ? t("{hero} · reading this hero's objects from your game…", { hero })
      : t("{hero} · objects are read from your game when this tab opens", { hero });
  return (
    <div id="extras-panel-objects" role="tabpanel" aria-labelledby="extras-tab-objects" className="objects-studio" hidden={!shown}>
      <StudioTopBar
        title={t("Objects")}
        badge={replaced ? tp(replaced, { one: "{count} replaced", other: "{count} replaced" }) : t("Nothing replaced")}
        badgeTone={replaced ? undefined : "quiet"}
        status={status}
        below={confirmClear ? (
          <div className="objects-confirm" role="group" aria-label={t("Remove every object replacement")}>
            <span>{saved.length === 1 ? t("Remove the object replacement? Your files stay where they are.") : tp(saved.length, { other: "Remove all {count} object replacements? Your files stay where they are." })}</span>
            <button type="button" className="extra-text-button objects-confirm-yes" disabled={busy} onClick={clearAll}>{t("Remove all")}</button>
            <button type="button" className="extra-text-button" onClick={() => setConfirmClear(false)}>{t("Keep them")}</button>
          </div>
        ) : undefined}
      >
        {saved.length > 0 && !confirmClear && (
          <button type="button" className="extra-text-button" disabled={busy} onClick={() => setConfirmClear(true)}>
            {t("Remove all replacements…")}
          </button>
        )}
      </StudioTopBar>
      <div className="objects-grid">
        <section className="objects-card objects-list" aria-label={t("Objects")}>
          <div className="objects-card-head">
            <h2>{t("Objects")}</h2>
            <div className="objects-seg" role="group" aria-label={t("Show")}>
              <button type="button" aria-pressed={!replacedOnly} onClick={() => setReplacedOnly(false)}>{t("All")}</button>
              <button type="button" aria-pressed={replacedOnly} onClick={() => setReplacedOnly(true)}>{t("Replaced")}</button>
            </div>
          </div>
          <ObjectList
            groups={groups}
            selected={selected}
            select={setSelected}
            active={active}
            empty={
              !canLoad && !catalog ? (
                <div className="objects-empty">
                  <p>{OBJECTS_NO_GAME}.</p>
                  {openSetup && <button type="button" className="extra-secondary" onClick={openSetup}>{t("Open Settings")}</button>}
                </div>
              ) : loadState === "loading" && !catalog ? (
                <div className="objects-empty" role="status"><span className="extra-spinner" /> <p>{t("Reading {hero}'s objects from your game…", { hero })}</p></div>
              ) : replacedOnly && catalog ? (
                <div className="objects-empty"><p>{t("No object is replaced yet. Choose one under All.")}</p></div>
              ) : catalog ? (
                <div className="objects-empty"><p>{t("None of {hero}'s abilities has an object the app can list.", { hero })}</p></div>
              ) : null
            }
          />
          {(loadState === "idle" || loadState === "failed") && canLoad && (
            <div className="objects-load">
              {loadState === "failed" && (
                <p className="objects-line" role="status">
                  {loadError}
                  {needsSettings(loadError) && openSetup && <> <button type="button" className="extra-text-button" onClick={openSetup}>{t("Open Settings")}</button></>}
                </p>
              )}
              <button type="button" className="extra-text-button" onClick={() => loadCatalog(loadState === "failed")}>
                <ExtraIcon name="refresh" size={15} />
                {t("Load this hero's objects")}
              </button>
            </div>
          )}
          {catalog && (
            <p className="objects-foot">
              {catalog.counts.offered < catalog.counts.listed
                ? tp(catalog.counts.listed, { one: "{count} object belongs to {hero}'s abilities; {offered} can be replaced so far.", other: "{count} objects belong to {hero}'s abilities; {offered} can be replaced so far." }, { hero, offered: catalog.counts.offered })
                : tp(catalog.counts.listed, { one: "{count} object belongs to {hero}'s abilities.", other: "{count} objects belong to {hero}'s abilities." }, { hero })}
              {catalog.counts.shared_shapes > 0 && t(" Plain shapes that many heroes share are never listed: changing one would change most of the game.")}
            </p>
          )}
        </section>
        <ObjectView
          item={item}
          mode={mode}
          setMode={setMode}
          collisionShown={collisionShown}
          setCollisionShown={setCollisionShown}
          reset={reset}
          resetView={() => setReset((v) => v + 1)}
          view={viewing}
          busy={busy}
          preparing={preparing === selected}
          show={() => item && void show(item.target)}
          openSetup={openSetup}
          motion={motion}
          frame={frame}
          player={item?.entry?.kind?.moving && answers.get(item.key) ? (
            <ObjectPlayer key={item.key} target={item.target} load={(target, clip) => api.abilityObjectMotion(projectRef.current, target, clip)} onMotion={setMotion} onFrame={setFrame} onClips={setClips}
              keptAnimation={Boolean(item.entry.kind.game_blocks?.includes("animation"))} plays={playerPlays(Boolean(item.saved?.source), answers.get(item.key)!)} />
          ) : null}
        />
        <ObjectPanel
          item={item}
          answer={item ? answers.get(item.key) ?? null : null}
          hero={hero}
          scenery={scenery}
          active={active}
          busy={busy}
          preparing={preparing === selected}
          unprepared={Boolean(item && unprepared.has(item.key))}
          pickError={pickError}
          full={saved.length >= MAX_ABILITY_OBJECTS}
          chooseFile={() => item && void chooseFile(item.target)}
          prepareAgain={() => item?.saved?.source && void prepareFile(item.target, item.saved.source)}
          remove={() => item && remove(item.target)}
          setFit={(fit) => item && setFit(item.target, fit)}
          setFollow={(follow) => item && setFollow(item.target, follow)}
          clips={item?.entry?.kind?.moving ? clips : []}
          setEnabled={(on) => item && write(withObjectRows(projectRef.current, withEnabled(rowsNow(), item.target, on), true))}
          reviewBuild={reviewBuild}
        />
      </div>
    </div>
  );
}

/** A row's badge word (`objectStatus`, English) as shown. */
const statusText = (status: "Replaced" | "Paused") => (status === "Paused" ? t("Paused") : t("Replaced"));
/** A row's accessible name: its name, whether it is offered and its badge. */
function rowLabel(name: string, off: boolean, status: "Replaced" | "Paused" | null) {
  const state = status ? statusText(status).toLowerCase() : "";
  if (off && state) return t("{name}, not offered yet, {state}", { name, state });
  if (off) return t("{name}, not offered yet", { name });
  if (state) return t("{name}, {state}", { name, state });
  return name;
}
function ObjectList({ groups, selected, select, active, empty }: { groups: ReturnType<typeof objectGroups>; selected: string; select: (key: string) => void; active: boolean; empty: ReactNode }) {
  const t = useT();
  if (!groups.length) return <>{empty}</>;
  return (
    <div className="objects-groups">
      {groups.map((group) => (
        <section key={group.key} className="objects-group" aria-label={group.title}>
          <h3>{group.title}</h3>
          <div role="list">
            {group.items.map((item) => {
              const entry = item.entry;
              const status = objectStatus(item.saved, active);
              const off = entry ? !entry.offered : false;
              const names = nameFromTarget(item.target);
              const name = entry?.name ?? names.name;
              const label = rowLabel(name, off, status);
              return (
                <div role="listitem" key={item.key}>
                  <button
                    type="button"
                    className={`objects-row ${selected === item.key ? "selected" : ""} ${off ? "off" : ""}`}
                    aria-pressed={selected === item.key}
                    aria-label={label}
                    title={off && entry?.reason ? entry.reason : undefined}
                    onClick={() => select(item.key)}
                  >
                    <span className="objects-row-text">
                      <strong>{name}</strong>
                      <span className="mono">{entry?.file ?? names.file}</span>
                      <small>
                        {entry ? kindText(entry) : t("Not in the hero's list now")}
                        {entry && entry.also_used_by.length > 0 && <> · <span className="objects-also">{t("also used by {names}", { names: listText(entry.also_used_by) })}</span></>}
                      </small>
                      {off && entry?.reason && <small className="objects-reason">{entry.reason}</small>}
                    </span>
                    {off ? (
                      <span className="objects-badge quiet">{t("Not offered yet")}</span>
                    ) : status ? (
                      <span className={`objects-badge ${status === "Paused" ? "quiet" : ""}`}>{statusText(status)}</span>
                    ) : null}
                  </button>
                </div>
              );
            })}
          </div>
        </section>
      ))}
    </div>
  );
}

/** The view's line about the drawn collision (objects with collision only). */
export const COLLISION_NOTE = m("Amber dashes: the game's collision, what blocks players and bullets. Keep your object close to it, or players are stopped by something they cannot see.");
const TURN_BUTTONS: [TurnAxis, string][] = [["roll", m("⟲ Roll 90°")], ["tip", m("⤒ Tip up 90°")], ["turn", m("⟳ Turn round 90°")]];
const VIEW_MODES = [["game", m("The game's")], ["yours", m("Yours")], ["both", m("Both")]] as const;
function ObjectView({ item, mode, setMode, collisionShown, setCollisionShown, reset, resetView, view, busy, preparing, show, openSetup, motion = null, frame = 0, player = null }: {
  item: ObjectItem | null; mode: ObjectViewMode; setMode: (mode: ObjectViewMode) => void; collisionShown: boolean; setCollisionShown: (on: boolean) => void;
  reset: number; resetView: () => void; view: ViewState; busy: boolean; preparing: boolean; show: () => void; openSetup?: () => void;
  motion?: ObjectMotion | null; frame?: number; player?: ReactNode;
}) {
  const t = useT();
  const answer = item ? answers.get(item.key) ?? null : null;
  // The 3D view is mounted with the first answer and kept: another object, the mode or a fit
  // change only change what it draws. Without an answer for this object it is hidden.
  const last = useRef<FitAnswer | null>(null);
  if (answer) last.current = answer;
  const drawn = last.current;
  const offered = Boolean(item?.entry?.offered);
  const fit = cleanFit(item?.saved?.fit);
  const yours = Boolean(answer?.yours);
  // Only objects with collision (a kind a switch opened) have shapes in the answer.
  const hasCollision = Boolean(answer?.view.collision?.length);
  const title = item ? item.entry?.name ?? nameFromTarget(item.target).name : t("Objects");
  let overlay: ReactNode = null;
  if (!item) overlay = <p className="objects-view-line">{t("Choose an object in the list.")}</p>;
  else if (!offered) overlay = <p className="objects-view-line">{item.entry ? t("This object cannot be replaced yet, so there is nothing to compare.") : t("This object is not in the hero's list now.")}</p>;
  else if (view?.status === "loading" || (preparing && !answer))
    overlay = <p className="objects-view-line" role="status"><span className="extra-spinner" /> {preparing ? t("Preparing your file…") : t("Reading the game's object…")}</p>;
  else if (view?.status === "failed" && !answer)
    overlay = (
      <div className="objects-view-line" role="status">
        <p>{t("The view could not load: {message}", { message: view.message })}</p>
        <p className="objects-view-actions">
          <button type="button" className="extra-secondary" disabled={busy} onClick={show}>{t("Try again")}</button>
          {needsSettings(view.message) && openSetup && <button type="button" className="extra-text-button" onClick={openSetup}>{t("Open Settings")}</button>}
        </p>
      </div>
    );
  else if (!answer)
    overlay = (
      <div className="objects-view-line">
        <button type="button" className="extra-primary" disabled={busy} onClick={show}>
          <ExtraIcon name="cube" size={17} />
          {t("Show this object")}
        </button>
        <p>{t("The first time, the app reads the game's object from your game; that takes a moment.")}</p>
      </div>
    );
  const footer = !answer
    ? null
    : mode === "game"
      ? t("The game's object alone, faint, with dashed edges.")
      : mode === "yours"
        ? yours ? t("Your object alone, fitted as it will be built.") : t("No file chosen yet: choose one to see yours.")
        : yours ? t("Both: your object solid, the game's faint, with dashed edges.") : t("The game's object, faint, with dashed edges. Choose a file to see yours beside it.");
  return (
    <StudioView
      className="objects-card objects-view"
      title={t("Object view")}
      bar={
        <>
          <strong className="objects-view-title">{title}</strong>
          {item && offered && <span className="objects-view-sub">{!item.saved?.source ? t("the game's object") : hasCollision ? t("your object inside the game's collision") : fit.size === "keep" ? t("your object at its own size") : t("your object at the size the game draws it")}</span>}
        </>
      }
      barEnd={
        <>
          <span className="objects-seg" role="group" aria-label={t("Show in the view")}>
            {VIEW_MODES.map(([value, label]) => (
              <button key={value} type="button" aria-pressed={mode === value} onClick={() => setMode(value)}>{t(label)}</button>
            ))}
          </span>
          {hasCollision && (
            <button type="button" className="objects-collision-toggle" aria-pressed={collisionShown} onClick={() => setCollisionShown(!collisionShown)}>
              {t("Collision shown")}
            </button>
          )}
        </>
      }
      footer={
        <div className="objects-view-foot">
          {footer && <p>{footer}</p>}
          {hasCollision && collisionShown && <p className="objects-collision-note">{t(COLLISION_NOTE)}</p>}
          {view?.status === "failed" && answer && <p role="status">{t("The view could not refresh: {message}", { message: view.message })}</p>}
          <button type="button" className="extra-secondary objects-reset" disabled={!answer} onClick={resetView}>{t("Reset view")}</button>
        </div>
      }
    >
      <div className={`objects-stage ${answer ? "" : "empty"}`}>
        {drawn && (
          <div className="objects-canvas" aria-hidden={!answer}>
            <Suspense fallback={<p className="objects-view-line" role="status"><span className="extra-spinner" /> {t("Loading the 3D view…")}</p>}>
              <ObjectScene answer={answer ?? drawn} mode={mode} reset={reset} frameKey={(answer ?? drawn).view.target} collision={hasCollision && collisionShown} motion={player ? motion : null} frame={frame} />
            </Suspense>
          </div>
        )}
        {overlay && <div className="objects-overlay">{overlay}</div>}
      </div>
      {player}
      {player && answer?.view.moving?.still && <p className="objects-view-line" role="note">{t(STILL_LINE)}</p>}
    </StudioView>
  );
}

/** What stays the game's for objects of the first kind. */
export const FIRST_KIND_KEPT: string[] = [m("When it appears, how it moves and what it hits"), m("Its trail and the effects around it")];
/** A kept item as shown: the page's own two in the language shown, the engine's as they come. */
const keptText = (text: string) => ((FIRST_KIND_KEPT as readonly string[]).includes(text) ? t(text) : text);
/** "two", "three" … as words in English (the language shown), a number past ten. */
const NUMBER_WORDS = [mc("number word", "zero"), mc("number word", "one"), mc("number word", "two"), mc("number word", "three"), mc("number word", "four"), mc("number word", "five"), mc("number word", "six"), mc("number word", "seven"), mc("number word", "eight"), mc("number word", "nine"), mc("number word", "ten")];
const countWord = (n: number) => (NUMBER_WORDS[n] ? tc("number word", NUMBER_WORDS[n]) : String(n));
/** What stays the game's: the engine's list for a kind a switch opened (an object an effect draws
 * also keeps how it moves), the first kind's two items otherwise. */
export function keptItems(entry: ObjectRow): string[] {
  const kind = entry.kind;
  if (!kind) return FIRST_KIND_KEPT;
  return [...(kind.features.includes("placed") ? [] : [FIRST_KIND_KEPT[0]]), ...kind.kept, FIRST_KIND_KEPT[1]];
}
function ObjectPanel({ item, answer, hero, active, busy, preparing, unprepared, pickError, full, chooseFile, prepareAgain, remove, setFit, setEnabled, reviewBuild, setFollow = () => {}, clips = [], scenery = false }: {
  setFollow?: (follow: ObjectFollow) => void; clips?: ObjectClip[];
  /** The catalog marks this object as a scenery model. */
  scenery?: boolean;
  item: ObjectItem | null; answer: FitAnswer | null; hero: string; active: boolean; busy: boolean; preparing: boolean; unprepared: boolean; pickError: string; full: boolean;
  chooseFile: () => void; prepareAgain: () => void; remove: () => void; setFit: (fit: ObjectFit) => void; setEnabled: (on: boolean) => void; reviewBuild?: () => void;
}) {
  const t = useT();
  const history = useHistory();
  const dragging = useRef(false);
  const historyRef = useRef(history);
  historyRef.current = history;
  const drag = (on: boolean) => { if (dragging.current === on) return; dragging.current = on; historyRef.current?.gesture(on); };
  useEffect(() => () => { if (dragging.current) historyRef.current?.gesture(false); }, []);
  const fit = cleanFit(item?.saved?.fit);
  const percent = percentOf(fit);
  const [draft, setDraft] = useState(String(percent));
  useEffect(() => setDraft(String(percent)), [percent]);
  if (!item)
    return (
      <aside className="objects-card objects-panel" aria-label={t("Object details")}>
        <h2>{t("Pick an object")}</h2>
        <p className="objects-note">{t("Choose one of {hero}'s objects in the list to replace it with a model of your own.", { hero })}</p>
      </aside>
    );
  const entry = item.entry, row = item.saved;
  const status = objectStatus(row, active);
  const name = entry?.name ?? nameFromTarget(item.target).name;
  const offered = Boolean(entry?.offered);
  const min = Math.round(OBJECT_SCALE_MIN * 100), max = Math.round(OBJECT_SCALE_MAX * 100);
  const prepared = preparations.get(item.key);
  const commitDraft = (text: string) => {
    const value = Number(text);
    if (text.trim() !== "" && Number.isFinite(value) && value >= min && value <= max) setFit(withScale(fit, value / 100));
  };
  return (
    <aside className="objects-card objects-panel" aria-label={t("Object details")}>
      <header className="objects-panel-head">
        <div>
          <h2>{name}</h2>
          <p className="mono">{item.target}</p>
          {offered && entry?.kind && <p className="objects-note objects-kind">{entry.kind.label}.</p>}
          {entry?.found_by && <p className="objects-note objects-found-by">{entry.found_by}</p>}
        </div>
        {!offered && entry ? <span className="objects-badge quiet">{t("Not offered yet")}</span> : status && <span className={`objects-badge ${status === "Paused" ? "quiet" : ""}`}>{statusText(status)}</span>}
      </header>
      {!entry && (
        <p className="objects-plain">{t("This object is not in {hero}'s list now: the game changed, or the list is not loaded yet. Your saved replacement stays until you remove it.", { hero })}</p>
      )}
      {entry && !offered && (
        <div className="objects-plain" role="note">
          <strong>{t("Not offered yet.")}</strong> {entry.reason ?? t("This object cannot be replaced yet.")}
          <p className="objects-note">{t("{kind}.", { kind: kindText(entry) })}</p>
        </div>
      )}
      {entry && !offered && scenery && <p className="objects-plain" role="note">{SCENERY_NOTE}</p>}
      {offered && (
        <>
          <h3>{t("Your object")}</h3>
          <div className="objects-file">
            <span className="objects-file-name" title={row?.source || undefined}>{row?.source ? basename(row.source) : t("No file chosen")}</span>
            <button type="button" className="extra-secondary" disabled={busy || (!row && full)} onClick={chooseFile}>{row?.source ? t("Change file…") : t("Choose file…")}</button>
            {row && <button type="button" className="extra-text-button" disabled={busy} onClick={remove}>{t("Remove")}</button>}
          </div>
          <p className="objects-note">{entry!.kind?.moving ? t("A model file: .blend, .fbx, .glb or .gltf. Each separate object in it can follow its own moving part.") : t("A model file with one object: .blend, .fbx, .glb or .gltf. It needs no skeleton.")}</p>
          {!row && full && <p className="objects-line" role="status">{tp(MAX_ABILITY_OBJECTS, { other: "{count} objects are replaced already. Remove one before adding another." })}</p>}
          {pickError && <p className="objects-line" role="status">{pickError}.</p>}
          {preparing && <p className="objects-line" role="status"><span className="extra-spinner" /> {t("Preparing your file…")}</p>}
          {!preparing && unprepared && row?.source && (
            <div className="objects-plain" role="status">
              {t("Your file is not prepared yet, or it changed since it was prepared. The view shows the game's object alone.")}
              <p><button type="button" className="extra-secondary" disabled={busy} onClick={prepareAgain}>{t("Prepare it again")}</button></p>
            </div>
          )}
          {!preparing && prepared && row?.source && (
            <p className="objects-note">
              {t("Prepared: {meshes}, {triangles}.", {
                meshes: tp(prepared.counts.meshes, { one: "{count} mesh", other: "{count} meshes" }),
                triangles: tp(prepared.counts.triangles, { one: "{count} triangle", other: "{count} triangles" }),
              })}
              {prepared.warnings.length > 0 && ` ${prepared.warnings.join(" ")}`}
            </p>
          )}
          {row && (
            <>
              <h3>{t("Fit")}</h3>
              <div className="objects-fit-row">
                <span className="objects-fit-label" id="objects-size-label">{t("Size")}</span>
                <span className="objects-seg" role="group" aria-labelledby="objects-size-label">
                  <button type="button" aria-pressed={fit.size !== "keep"} disabled={busy} onClick={() => setFit(withSize(fit, "match"))}>{t("Match the game's object")}</button>
                  <button type="button" aria-pressed={fit.size === "keep"} disabled={busy} onClick={() => setFit(withSize(fit, "keep"))}>{t("Keep my size")}</button>
                </span>
              </div>
              <div className="objects-fit-row objects-scale">
                <label className="objects-fit-label" htmlFor="objects-scale">{t("Adjust size")}</label>
                <input
                  id="objects-scale"
                  aria-label={t("Adjust size")}
                  type="range"
                  min={SLIDER_MIN}
                  max={SLIDER_MAX}
                  step={1}
                  value={Math.min(SLIDER_MAX, Math.max(SLIDER_MIN, percent))}
                  disabled={busy}
                  onPointerDown={() => drag(true)}
                  onPointerUp={() => drag(false)}
                  onPointerCancel={() => drag(false)}
                  onBlur={() => drag(false)}
                  onChange={(e) => setFit(withScale(fit, Number(e.target.value) / 100))}
                />
                <span className="objects-percent">
                  <input
                    aria-label={t("Adjust size percent")}
                    type="number"
                    min={min}
                    max={max}
                    step={1}
                    value={draft}
                    disabled={busy}
                    onChange={(e) => { setDraft(e.target.value); commitDraft(e.target.value); }}
                    onBlur={() => setDraft(String(percent))}
                  />
                  %
                </span>
              </div>
              {entry?.kind?.stands_on && (
                <div className="objects-fit-row">
                  <span className="objects-fit-label" id="objects-base-label">{t("Stands on")}</span>
                  <span className="objects-seg" role="group" aria-labelledby="objects-base-label">
                    <button type="button" aria-pressed={!fit.on_base} disabled={busy} onClick={() => setFit(withBase(fit, false))}>{t("Centred on the game's")}</button>
                    <button type="button" aria-pressed={Boolean(fit.on_base)} disabled={busy} onClick={() => setFit(withBase(fit, true))}>{t("The game's base")}</button>
                  </span>
                </div>
              )}
              <div className="objects-fit-row">
                <span className="objects-fit-label">{t("Turn")}</span>
                <span className="objects-turns">
                  {TURN_BUTTONS.map(([axis, label]) => (
                    <button key={axis} type="button" className="extra-secondary" disabled={busy} onClick={() => setFit(turned(fit, axis))}>
                      {t(label)}
                      {fit.turns?.[axis] ? <small> ×{fit.turns[axis]}</small> : null}
                    </button>
                  ))}
                  <button type="button" className="extra-text-button" disabled={busy || !fit.turns} onClick={() => setFit(unturned(fit))}>{t("Reset turns")}</button>
                </span>
              </div>
            </>
          )}
          {entry!.material_always_overridden && (
            <p className="objects-warn" role="note">
              {entry!.material_override
                ? <T text="<strong>The game draws this object in a material of its own ({material}).</strong> Your object's shape shows in game; its colors and images do not." values={{ material: materialName(entry!.material_override) }} />
                : <T text="<strong>The game draws this object in a material of its own.</strong> Your object's shape shows in game; its colors and images do not." />}
            </p>
          )}
          {!entry!.material_always_overridden && entry!.game_look?.always && (
            <p className="objects-warn objects-look-note" role="note">{entry!.game_look.line}</p>
          )}
          {!entry!.material_always_overridden && !entry!.game_look?.always && (
            <p className="objects-note objects-material-note">{materialNote(entry!.material_override)}</p>
          )}
          {!entry!.material_always_overridden && entry!.game_look && !entry!.game_look.always && (
            <p className="objects-note objects-look-note" role="note">{entry!.game_look.line}</p>
          )}
          {entry!.also_used_by.length > 0 && (
            <p className="objects-plain" role="note">
              {entry!.also_used_by.length === 1
                ? <T text="<strong>Also used by: {names}.</strong> That hero shows your object too, because the game uses one file for both." values={{ names: listText(entry!.also_used_by) }} />
                : <T text="<strong>Also used by: {names}.</strong> Those heroes show your object too, because the game uses one file for all {total}." values={{ names: listText(entry!.also_used_by), total: countWord(entry!.also_used_by.length + 1) }} />}
            </p>
          )}
          {scenery && <p className="objects-plain" role="note">{SCENERY_NOTE}</p>}
          {entry!.kind?.moving && item?.saved?.source && (
            <MovingPanel entry={entry!} follow={item.saved.follow} view={answer?.view.moving} clips={clips} setFollow={setFollow} />
          )}
          <h3>{t("Stays the game's")}</h3>
          <ul className="objects-keeps">
            {keptItems(entry!).map((text) => <li key={text}>{keptText(text)}</li>)}
          </ul>
          {entry!.kind?.collision && answer?.yours && answer.view.collision_fit && (
            <p className={answer.view.collision_fit.fits ? "objects-fits" : "objects-warn"} role="note">{answer.view.collision_fit.line}</p>
          )}
        </>
      )}
      {row && (
        <div className="objects-build">
          <label className="extra-toggle">
            <input type="checkbox" checked={row.enabled !== false} disabled={busy} onChange={(e) => setEnabled(e.target.checked)} />
            <span aria-hidden="true" />
            <strong>{t("Include in build")}</strong>
          </label>
          <span className="objects-note">{row.enabled === false ? t("Left out of the next build.") : row.source ? t("Used by the next build.") : t("Choose a file first.")}</span>
          {reviewBuild && <button type="button" className="extra-secondary" onClick={reviewBuild}>{t("Open Build & install →")}</button>}
          {!offered && <button type="button" className="extra-text-button" disabled={busy} onClick={remove}>{t("Remove")}</button>}
        </div>
      )}
    </aside>
  );
}
