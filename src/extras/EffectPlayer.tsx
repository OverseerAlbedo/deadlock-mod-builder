/** Extras → Effects: the selected effect playing, above "Parts and their colors". "Yours"
 * always plays; Compare puts the game's version beside it. The helper process lives in Rust
 * (`effect_player.rs`), which makes "yours" from exactly the files a build writes; this view
 * only sends the edit and view choices and draws the newest frame once per animation frame.
 * When the computer cannot run the player, one quiet line replaces it and the strips below
 * stay as they are. */
import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent as ReactPointerEvent, type WheelEvent } from "react";
// i18n-area: effects
import { api, isDesktop } from "../api";
import { m, t, tp, useT, type Values } from "../i18n";
import { sentences } from "../build/words";
import type { Project } from "../models";
import { EFFECTS_NOT_LOADED, type EffectEdit, type EffectTree } from "./effectTypes";
import {
  DEFAULT_VIEW,
  PLAYER_BACKGROUNDS,
  PLAYER_SPEEDS,
  parseFrame,
  type PlayerAnswer,
  type PlayerControl,
  type PlayerEvent,
  type PlayerFrame,
  type PlayerLoaded,
  type PlayerView,
} from "./effectPlayerTypes";

/** The commands the view uses (the desktop app's, or a mock in tests and the harness). */
export interface PlayerBridge {
  play(project: Project, target: string, edit: EffectEdit, view: PlayerView): Promise<PlayerAnswer>;
  edit(project: Project, target: string, edit: EffectEdit): Promise<PlayerAnswer>;
  control(control: PlayerControl): Promise<PlayerAnswer>;
  stop(): Promise<void>;
  /** The newest frame after `after` (an empty buffer when none came within `waitMs`). */
  frame(after: number, waitMs: number): Promise<ArrayBuffer>;
  /** Status events; returns the function that stops listening. */
  listen(handler: (event: PlayerEvent) => void): () => void;
  /** Draws one frame's JPEG into the canvas (default: `createImageBitmap`). */
  draw?(canvas: HTMLCanvasElement, frame: PlayerFrame): Promise<void>;
}

/** The desktop app's bridge, or null outside it (the page then shows the strips only). */
/** A stop still on its way: the next play waits for it (leaving and coming back quickly). */
let stopping: Promise<unknown> = Promise.resolve();
export function desktopBridge(): PlayerBridge | null {
  if (!isDesktop()) return null;
  return {
    play: (project, target, edit, view) => stopping.then(() => api.effectPlayerPlay(project, target, edit, view)),
    edit: (project, target, edit) => api.effectPlayerEdit(project, target, edit),
    control: (control) => api.effectPlayerControl(control),
    stop: () => (stopping = api.effectPlayerStop().catch(() => undefined)).then(() => undefined),
    frame: (after, waitMs) => api.effectPlayerFrame(after, waitMs),
    listen: (handler) => {
      let stop: (() => void) | null = null;
      let gone = false;
      void import("@tauri-apps/api/event")
        .then(({ listen }) =>
          listen<PlayerEvent>("effect-player", (e) => handler(e.payload)).then((un) => {
            if (gone) un();
            else stop = un;
          }),
        )
        .catch(() => undefined); // no events: answers still carry every state
      return () => {
        gone = true;
        stop?.();
      };
    },
  };
}

/** Frame rates offered; the saved choice lives in `localStorage` (on this PC, not in the project). */
export const FRAME_RATES = [30, 60, 120, 144] as const;
export const FPS_KEY = "dmb.effectPlayerFps";
export function savedFps(): number {
  try {
    const value = Number(localStorage.getItem(FPS_KEY));
    return (FRAME_RATES as readonly number[]).includes(value) ? value : DEFAULT_VIEW.fps;
  } catch {
    return DEFAULT_VIEW.fps;
  }
}
export function rememberFps(fps: number) {
  try {
    localStorage.setItem(FPS_KEY, String(fps));
  } catch {
    /* blocked storage: the choice lasts for this session */
  }
}
/** The other view choices, for this session only (never saved). */
let sessionView: Omit<PlayerView, "fps" | "width" | "height" | "paused"> = { compare: false, loop: true, speed: 1, background: DEFAULT_VIEW.background, brightness: 0 };
export function resetSessionView() {
  sessionView = { compare: false, loop: true, speed: 1, background: DEFAULT_VIEW.background, brightness: 0 };
}

/** The line under the player: a message from the helper as it comes, or the page's own sentence (English, drawn translated). */
type Line = string | { text: string; values?: Values };
/** A line as drawn. The backend's "list not loaded" message (recognised by its constant, which stays
 * as it is) is drawn as the page's own sentence, so it reads as one with the sentence after it. */
const lineText = (line: Line) => (typeof line === "string" ? (line === EFFECTS_NOT_LOADED ? t("The effect list is not loaded. Open Extras → Effects again") : line) : t(line.text, line.values));
/** The backgrounds' accessible names, whole per background. */
const BACKGROUND_NAMES: Record<string, string> = { Black: m("Black background"), Slate: m("Slate background") };
type Status = "starting" | "loading" | "playing" | "paused" | "finished" | "unavailable" | "failed";
const frameCallback = (cb: () => void) => (typeof requestAnimationFrame === "function" ? requestAnimationFrame(cb) : (setTimeout(cb, 16) as unknown as number));

async function drawJpeg(canvas: HTMLCanvasElement, frame: PlayerFrame) {
  const bitmap = await createImageBitmap(new Blob([frame.jpeg as BlobPart], { type: "image/jpeg" }));
  try {
    if (canvas.width !== bitmap.width || canvas.height !== bitmap.height) {
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
    }
    canvas.getContext("2d")?.drawImage(bitmap, 0, 0);
  } finally {
    bitmap.close();
  }
}

/** The plain-words notes under the picture, from the helper's `loaded` report. */
export function playerNotes(loaded: PlayerLoaded | null, compare: boolean): string[] {
  const notes: string[] = [];
  const n = loaded?.notes;
  const short = (path: string) => path.split("/").pop()?.replace(/\.vpcf_c$/, "").replace(/_/g, " ") ?? path;
  if (n?.untintedModels?.parts.length) {
    notes.push(t("3D model parts ({parts}) are drawn without their colors here; in the game they keep their own colors.", { parts: n.untintedModels.parts.map(short).join(", ") }));
  }
  if (n?.decals?.parts.length) {
    notes.push(n.decals.surface === "floor" ? t("Decals are shown on a flat floor; in the game they also land on walls and slopes.") : t("Decals have no surface to land on here and are not shown."));
  }
  if (n?.screenSpace) notes.push(t("A screen effect: in the game it covers the player's view."));
  for (const line of n?.cannotShow ?? []) notes.push(line);
  if (compare) notes.push(loaded?.compareNote || t("The game's version and yours play in step."));
  if (compare && loaded?.gameVersion === false) notes.push(t("The game has no version of this effect to compare with."));
  return notes;
}
/** Nothing can be drawn: the reason, else null. */
export function nothingToDraw(loaded: PlayerLoaded | null, coverage: number | null): string | null {
  const cannot = loaded?.notes?.cannotShow ?? [];
  if (loaded?.bounds?.empty) return cannot[0] ?? t("This effect draws nothing on its own here: in the game it is placed on a character or starts other effects.");
  if (coverage === 0 && cannot.length) return cannot[0];
  return null;
}

export interface EffectPlayerProps {
  project: Project;
  tree: EffectTree;
  /** The page's current edit for this effect (saved or not). */
  edit: EffectEdit;
  bridge: PlayerBridge;
}

export function EffectPlayer({ project, tree, edit, bridge }: EffectPlayerProps) {
  const t = useT();
  const [status, setStatus] = useState<Status>("starting");
  const [line, setLine] = useState<Line>("");
  const [notice, setNotice] = useState("");
  const [loaded, setLoaded] = useState<PlayerLoaded | null>(null);
  const [view, setViewState] = useState<PlayerView>(() => ({ ...DEFAULT_VIEW, ...sessionView, fps: savedFps() }));
  const timeText = useRef<HTMLSpanElement>(null);
  const shownTime = useRef(0);
  const [coverage, setCoverage] = useState<number | null>(null);
  const [driver, setDriver] = useState("");
  const [updating, setUpdating] = useState(false);
  const canvas = useRef<HTMLCanvasElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const alive = useRef(true);
  const projectRef = useRef(project);
  projectRef.current = project;
  const viewRef = useRef(view);
  viewRef.current = view;
  const generation = useRef(0);
  const statusRef = useRef<Status>(status);
  statusRef.current = status;
  const playing = useRef(false);
  const coverageSince = useRef(0);

  const setView = (next: Partial<PlayerView>) => {
    setViewState((old) => {
      const merged = { ...old, ...next };
      sessionView = { compare: merged.compare, loop: merged.loop, speed: merged.speed, background: merged.background, brightness: merged.brightness };
      return merged;
    });
  };
  const after = useCallback((answer: PlayerAnswer) => {
    if (!alive.current) return false;
    const state = answer.state;
    if (state.state === "unavailable") {
      setStatus("unavailable");
      setLine(state.reason === "missing" ? { text: m("The moving preview is not part of this copy of the app.") } : { text: m("The moving preview cannot run on this computer: {message}"), values: { message: state.message } });
      playing.current = false;
      return false;
    }
    if (state.state === "failed") {
      setStatus("failed");
      setLine(state.message);
      playing.current = false;
      return false;
    }
    if (state.state === "ready") setDriver(state.driver);
    if (answer.notice) setNotice(answer.notice);
    if (answer.error) setLine({ text: m("The preview could not show this: {error}"), values: { error: answer.error } });
    if (answer.loaded) setLoaded(answer.loaded);
    if (answer.generation !== undefined) generation.current = Math.max(generation.current, answer.generation);
    return true;
  }, []);

  // ── Play the selected effect (and stop when the view goes away) ──
  const target = tree.target;
  const editKey = JSON.stringify(edit);
  const latestEdit = useRef(edit);
  latestEdit.current = edit;
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      playing.current = false;
      void bridge.stop().catch(() => undefined);
    };
  }, [bridge]);
  useEffect(() => {
    let current = true;
    setStatus((s) => (s === "unavailable" ? s : playing.current ? "loading" : "starting"));
    setLine("");
    setLoaded(null);
    setCoverage(null);
    coverageSince.current = 0;
    const size = measured();
    const sentKey = JSON.stringify(latestEdit.current);
    bridge.play(projectRef.current, target, latestEdit.current, { ...viewRef.current, ...size, paused: false }).then(
      (answer) => {
        if (!current || !after(answer)) return;
        playing.current = true;
        setStatus(viewRef.current.paused ? "paused" : "playing");
        setViewState((v) => ({ ...v, paused: false }));
        lastSentEdit.current = sentKey;
        startFrames();
        // An edit made while it was starting follows now.
        frameCallback(sendEdit);
      },
      (cause) => {
        if (!current || !alive.current) return;
        // The effects were not loaded or the game changed: the strips say so; the player waits.
        setStatus("failed");
        setLine(typeof cause === "object" && cause && "message" in cause ? String((cause as { message: unknown }).message) : String(cause));
      },
    );
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new effect (or Try again) plays again; edits go through the coalescer
  }, [target, attempt, bridge]);

  // ── Edits while dragging: one call at a time, the newest waiting one wins ──
  const lastSentEdit = useRef("");
  const inFlight = useRef(false);
  const waiting = useRef(false);
  const sendEdit = useCallback(() => {
    if (!playing.current || inFlight.current) {
      waiting.current = true;
      return;
    }
    const key = JSON.stringify(latestEdit.current);
    if (key === lastSentEdit.current) return;
    inFlight.current = true;
    waiting.current = false;
    lastSentEdit.current = key;
    setUpdating(true);
    bridge.edit(projectRef.current, target, latestEdit.current).then(
      (answer) => {
        inFlight.current = false;
        after(answer);
        if (waiting.current || JSON.stringify(latestEdit.current) !== lastSentEdit.current) frameCallback(sendEdit);
        else setUpdating(false);
      },
      () => {
        inFlight.current = false;
        setUpdating(false);
      },
    );
  }, [after, bridge, target]);
  useEffect(() => {
    if (!playing.current) return;
    frameCallback(sendEdit);
  }, [editKey, sendEdit]);

  // ── Frames: pull the newest, draw once per animation frame ──
  const newest = useRef<PlayerFrame | null>(null);
  const drawing = useRef(false);
  const scheduled = useRef(false);
  const counter = useRef(0);
  const pulling = useRef(false);
  const draw = useCallback(() => {
    scheduled.current = false;
    const frame = newest.current;
    const target = canvas.current;
    if (!frame || !target || drawing.current) return;
    newest.current = null;
    drawing.current = true;
    (bridge.draw ?? drawJpeg)(target, frame)
      .catch(() => undefined)
      .finally(() => {
        drawing.current = false;
        if (newest.current && !scheduled.current) {
          scheduled.current = true;
          frameCallback(draw);
        }
      });
    // No React render per frame: the time is written straight into its text.
    shownTime.current = frame.time;
    if (timeText.current) timeText.current.textContent = t("{value} s", { value: frame.time.toFixed(2) });
    if (frame.generation >= generation.current) {
      const drawsSomething = frame.coverage > 0;
      if (drawsSomething) coverageSince.current = performance.now();
      setCoverage((old) => (drawsSomething ? (old !== null && old > 0 ? old : frame.coverage) : old === null ? 0 : old));
    }
  }, [bridge]);
  const startFrames = useCallback(() => {
    if (pulling.current) return;
    pulling.current = true;
    (async () => {
      while (alive.current && playing.current) {
        let buffer: ArrayBuffer;
        try {
          buffer = await bridge.frame(counter.current, 500);
        } catch {
          // A failed pull (the helper restarting, a busy moment): try again shortly.
          await new Promise((resolve) => setTimeout(resolve, 250));
          continue;
        }
        const frame = parseFrame(buffer);
        if (!frame || frame.counter <= counter.current) continue;
        counter.current = frame.counter;
        if (frame.finished && statusRef.current === "playing") setStatus("finished");
        else if (!frame.finished && !frame.paused && statusRef.current === "finished" && frame.generation >= generation.current) setStatus("playing");
        newest.current = frame; // older ones are dropped here
        if (!scheduled.current) {
          scheduled.current = true;
          frameCallback(draw);
        }
        // At most one pull per animation frame: a 144 fps stream on a 60 Hz screen costs what 60 do.
        await new Promise<void>((resolve) => frameCallback(() => resolve()));
      }
      pulling.current = false;
    })();
  }, [bridge, draw]);

  // ── Status events ──
  useEffect(
    () =>
      bridge.listen((event) => {
        if (!alive.current) return;
        if (event.type === "finished" && event.generation >= generation.current) setStatus("finished");
        else if (event.type === "restarted") {
          setLine(event.message);
          startFrames();
        } else if (event.type === "failed") {
          setStatus("failed");
          setLine(event.message);
          playing.current = false;
        } else if (event.type === "ready") setDriver(event.driver);
        else if (event.type === "loaded") setLoaded(event.loaded);
      }),
    [bridge, startFrames],
  );

  // ── Size: the stage's pixels (capped), sent when it settles ──
  function measured() {
    const el = stage.current;
    const ratio = Math.min(2, typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1);
    if (!el || !el.clientWidth) return { width: viewRef.current.width, height: viewRef.current.height };
    // At most 960x540: the largest size measured through to the page.
    const width = Math.min(960, Math.max(320, Math.round(el.clientWidth * ratio)));
    const height = Math.min(540, Math.max(180, Math.round(width * 9 / 16)));
    return { width: width & ~1, height: height & ~1 };
  }
  useEffect(() => {
    const el = stage.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    let timer = 0;
    const observer = new ResizeObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        const size = measured();
        if (size.width === viewRef.current.width && size.height === viewRef.current.height) return;
        setView(size);
        if (playing.current) void bridge.control({ command: "size", ...size }).then(after, () => undefined);
      }, 200);
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
      window.clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bridge]);

  // ── Controls ──
  const control = (c: PlayerControl) => {
    setLine("");
    return bridge.control(c).then(after, () => false);
  };
  function togglePause() {
    if (status === "finished") {
      setStatus("playing");
      void control({ command: "restart" });
      return;
    }
    const paused = status !== "paused";
    setView({ paused });
    setStatus(paused ? "paused" : "playing");
    void control({ command: paused ? "pause" : "resume" });
  }
  function setLoop(on: boolean) {
    setView({ loop: on });
    void control({ command: "loop", on });
    if (on && status === "finished") {
      setStatus("playing");
      void control({ command: "restart" });
    }
  }
  function setCompare(on: boolean) {
    setView({ compare: on });
    void control({ command: "compare", on });
  }
  function setFps(fps: number) {
    rememberFps(fps);
    setNotice("");
    setView({ fps });
    void control({ command: "fps", value: fps });
  }
  // Drag to turn, wheel to zoom: deltas summed and sent once per animation frame.
  const pendingOrbit = useRef({ dx: 0, dy: 0, zoom: 0, queued: false });
  const flushMotion = useCallback(() => {
    const p = pendingOrbit.current;
    p.queued = false;
    if (p.dx || p.dy) void bridge.control({ command: "orbit", dx: p.dx, dy: p.dy }).catch(() => undefined);
    if (p.zoom) void bridge.control({ command: "zoom", steps: p.zoom }).catch(() => undefined);
    p.dx = p.dy = p.zoom = 0;
  }, [bridge]);
  const queueMotion = (dx: number, dy: number, zoom: number) => {
    const p = pendingOrbit.current;
    p.dx += dx;
    p.dy += dy;
    p.zoom += zoom;
    if (!p.queued) {
      p.queued = true;
      frameCallback(flushMotion);
    }
  };
  const drag = useRef<{ x: number; y: number } | null>(null);
  const onPointerDown = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    drag.current = { x: e.clientX, y: e.clientY };
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };
  const onPointerMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    if (!drag.current) return;
    queueMotion(e.clientX - drag.current.x, drag.current.y - e.clientY, 0);
    drag.current = { x: e.clientX, y: e.clientY };
  };
  const onPointerUp = () => {
    drag.current = null;
  };
  const onWheel = (e: WheelEvent<HTMLCanvasElement>) => {
    queueMotion(0, 0, -e.deltaY / 100);
  };
  const onKey = (e: KeyboardEvent<HTMLCanvasElement>) => {
    const step = e.shiftKey ? 40 : 12;
    const keys: Record<string, () => void> = {
      ArrowLeft: () => queueMotion(-step, 0, 0),
      ArrowRight: () => queueMotion(step, 0, 0),
      ArrowUp: () => queueMotion(0, step, 0),
      ArrowDown: () => queueMotion(0, -step, 0),
      "+": () => queueMotion(0, 0, 1),
      "=": () => queueMotion(0, 0, 1),
      "-": () => queueMotion(0, 0, -1),
      " ": togglePause,
    };
    const run = keys[e.key];
    if (!run) return;
    e.preventDefault();
    run();
  };

  const name = tree.name || tree.label;
  if (status === "unavailable") {
    return (
      <p className="effects-player-off" role="note">
        {sentences(lineText(line), t("The color strips below show your edit."))}
      </p>
    );
  }
  if (status === "failed") {
    return (
      <div className="effects-player-off" role="alert">
        <span>{sentences(line ? lineText(line) : t("The moving preview stopped."), t("The color strips below still show your edit."))}</span>
        <button type="button" className="extra-secondary small" onClick={() => setAttempt((a) => a + 1)}>{t("Try again")}</button>
      </div>
    );
  }
  const empty = status === "playing" || status === "paused" || status === "finished" ? nothingToDraw(loaded, coverage) : null;
  const notes = playerNotes(loaded, view.compare);
  const busy = status === "starting" || status === "loading";
  const label = view.compare ? t("Moving preview of {name}: the game's version on the left, yours on the right", { name }) : t("Moving preview of {name}: your version", { name });
  return (
    <section className="effects-player" aria-label={t("Effect preview")}>
      <div className="effects-player-bar">
        <button type="button" className="effects-player-button" aria-pressed={status === "paused"} disabled={busy} onClick={togglePause}>
          {status === "finished" ? t("Play again") : status === "paused" ? t("Play") : t("Pause")}
        </button>
        <button type="button" className="effects-player-button" aria-pressed={view.loop} disabled={busy} onClick={() => setLoop(!view.loop)}>{t("Loop")}</button>
        <div className="effects-player-group" role="radiogroup" aria-label={t("Speed")}>
          <span>{t("Speed")}</span>
          {PLAYER_SPEEDS.map((s) => (
            <button key={s.value} type="button" role="radio" aria-checked={view.speed === s.value} disabled={busy} onClick={() => { setView({ speed: s.value }); void control({ command: "speed", value: s.value }); }}>{s.label}</button>
          ))}
        </div>
        <span className="color-picker-spacer" />
        <label className="extra-toggle effects-player-compare">
          <input type="checkbox" checked={view.compare} disabled={busy} onChange={(e) => setCompare(e.target.checked)} />
          <span aria-hidden="true" />
          <strong>{t("Compare with the game effect")}</strong>
        </label>
      </div>
      <div ref={stage} className="effects-player-stage" style={{ background: `#${view.background}` }}>
        <canvas
          ref={canvas}
          role="img"
          aria-label={label}
          tabIndex={0}
          className={empty ? "hidden" : ""}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onWheel={onWheel}
          onKeyDown={onKey}
        />
        {view.compare ? (
          <>
            <span className="effects-player-tag left">{t("Game")}</span>
            <span className="effects-player-tag right">{t("Yours")}</span>
          </>
        ) : <span className="effects-player-tag left">{t("Yours")}</span>}
        {busy && (
          <div className="effects-player-wait" role="status">
            <span className="extra-spinner" />
            {status === "starting" ? t("Starting the effect player…") : t("Loading {name}…", { name })}
          </div>
        )}
        {empty && !busy && <div className="effects-player-empty" role="note"><strong>{t("Nothing to draw here")}</strong><span>{empty}</span></div>}
        {!busy && !empty && <span className="effects-player-time" aria-live="off">{status === "finished" ? `${t("finished")} · ` : status === "paused" ? `${t("paused")} · ` : ""}<span ref={timeText}>{t("{value} s", { value: shownTime.current.toFixed(2) })}</span></span>}
        {updating && !busy && <span className="effects-player-updating" role="status">{t("Updating yours…")}</span>}
      </div>
      <div className="effects-player-bar view">
        <div className="effects-player-group" role="radiogroup" aria-label={t("Background")}>
          <span>{t("Background")}</span>
          {PLAYER_BACKGROUNDS.map((b) => (
            <button key={b.color} type="button" role="radio" className="effects-player-swatch" aria-checked={view.background === b.color} aria-label={BACKGROUND_NAMES[b.name] ? t(BACKGROUND_NAMES[b.name]) : `${b.name} background`} title={t(b.name)} style={{ background: `#${b.color}` }} onClick={() => { setView({ background: b.color }); void control({ command: "background", color: b.color }); }} />
          ))}
        </div>
        <label className="effects-player-brightness">
          <span>{t("View brightness")}</span>
          <input type="range" aria-label={t("View brightness")} title={t("Only changes this view, never the mod")} min={0} max={4} step={0.5} value={view.brightness} aria-valuetext={tp(view.brightness, { one: "+{count} stops", other: "+{count} stops" })} onChange={(e) => { const stops = Number(e.target.value); setView({ brightness: stops }); void control({ command: "brightness", stops }); }} />
          <small>{view.brightness ? `+${view.brightness}` : "0"}</small>
        </label>
        <label className="effects-player-fps">
          <span>{t("Frame rate")}</span>
          <select value={view.fps} onChange={(e) => setFps(Number(e.target.value))}>
            {FRAME_RATES.map((r) => <option key={r} value={r}>{r} fps</option>)}
          </select>
        </label>
        <span className="effects-player-actions">
          <button type="button" className="extra-secondary small" disabled={busy} onClick={() => void control({ command: "fit" })}>{t("Fit to effect")}</button>
          <button type="button" className="extra-secondary small" disabled={busy} onClick={() => void control({ command: "reset_view" })}>{t("Reset view")}</button>
        </span>
      </div>
      {line && <p className="effects-player-line" role="status">{lineText(line)}</p>}
      {notice && <p className="effects-player-line" role="status">{notice}</p>}
      <ul className="effects-player-notes">
        <li>{t("Approximate preview: no map lighting, and the effect is not attached to the hero. Drag to turn it, scroll to zoom. Background, view brightness and frame rate only change this view, never the mod.")}</li>
        {notes.map((n, i) => <li key={i}>{n}</li>)}
      </ul>
      {driver && <span className="effects-player-driver" hidden>{driver}</span>}
    </section>
  );
}
