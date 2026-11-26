// Extras → Sounds: the editor for a sound with a replacement file. The frame
// on "Your file" is always as long as the game's sound and chooses the part
// that plays; Volume, Fade in and Fade out change it; "What the game will
// play" and "Yours" are the build's own render (`api.soundEditPreview`).
// i18n-area: sounds
import { useEffect, useId, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { api, basename, errorText } from "../api";
import type { ResourceReplacement } from "../models";
import { T } from "../i18n/T";
import { t, tp, useT } from "../i18n";
import { parseExtraMedia } from "./media";
import {
  EDIT_KEYS,
  VOLUME_DB_RANGE,
  editValues,
  hasEdit,
  useSoundPreview,
  withEdit,
  withoutEdit,
  type EditKey,
  type SoundEditPreview,
  type SoundEditValues,
} from "./soundEdit";
import "./soundEdit.css";

const stopEvent = "dmb-stop-extra-audio";
let context: AudioContext | null = null;

export interface SoundEditorProps {
  hero: string;
  heroName: string;
  name: string;
  target: string;
  row: ResourceReplacement;
  disabled: boolean;
  /** Writes the row through the project's update path. */
  commit: (row: ResourceReplacement) => void;
  close: () => void;
  silence: () => void;
  silenceRefusal: string;
  /** "Replace all N variations with this part" when the sound has variations. */
  variations?: { count: number; sharedCount: number; apply: (values: SoundEditValues, includeShared: boolean) => void } | null;
}

export const seconds = (v: number) => t("{value} s", { value: v.toFixed(2) });
export const decibels = (v: number) => (Math.abs(v) < 0.05 ? "0 dB" : `${v > 0 ? "+" : "−"}${Math.abs(v).toFixed(1)} dB`);
/** A typed value: "2.6", "2,6 s", "+3", "−4.5 dB"; null when it is not a number. */
export function parseTyped(text: string): number | null {
  const plain = text.trim().replace(/(db|s)$/i, "").trim().replace(/^[−–]/, "-").replace(/^\+/, "").replace(",", ".");
  if (!/^-?(\d+(\.\d*)?|\.\d+)$/.test(plain)) return null;
  return Number(plain);
}

/** One edit value: the app's slider (filled from `zero` to the value) and a box for an exact value. */
function EditSlider({
  label, hint, value, min, max, step, zero, format, disabled, draft, commit,
}: {
  label: string; hint: string; value: number; min: number; max: number; step: number; zero: number;
  format: (v: number) => string; disabled: boolean;
  draft: (v: number | null) => void; commit: (v: number) => void;
}) {
  const t = useT();
  const hintId = useId();
  const dragging = useRef(false);
  const [typed, setTyped] = useState<string | null>(null);
  const span = max - min || 1;
  const at = (v: number) => ((Math.min(max, Math.max(min, v)) - min) / span) * 100;
  const lo = Math.min(at(zero), at(value)), hi = Math.max(at(zero), at(value));
  const clamp = (v: number) => Math.min(max, Math.max(min, v));
  function done(text: string) {
    setTyped(null);
    const next = parseTyped(text);
    if (next !== null && next !== value) commit(next);
  }
  function release(event: { currentTarget: HTMLInputElement }) {
    if (!dragging.current) return;
    dragging.current = false;
    draft(null);
    commit(clamp(Number(event.currentTarget.value)));
  }
  return (
    <div className={`tune-row sound-edit-row ${value !== zero ? "tuned" : ""}`}>
      <div className="tune-head"><span className="tune-label">{label}</span><span className="tune-hint" id={hintId}>{hint}</span></div>
      <div className="tune-body">
        <div className="tune-scale">
          <div className="tune-track" style={{ "--lo": `${lo}%`, "--hi": `${hi}%`, "--zero": `${at(zero)}%` } as CSSProperties}>
            <span className="tune-rail" aria-hidden="true"><span className="tune-fill" /><span className="tune-detent" /></span>
            <input type="range" min={min} max={max} step={step} value={clamp(value)} disabled={disabled}
              aria-label={label} aria-valuetext={format(value)} aria-describedby={hintId}
              onPointerDown={() => { dragging.current = true; }}
              onPointerUp={release} onPointerCancel={release} onBlur={release}
              onChange={(event) => {
                const next = Number(event.target.value);
                if (dragging.current) draft(next);
                else commit(next);
              }} />
          </div>
        </div>
        <input className={`tune-box ${value !== zero ? "tuned" : ""}`} type="text" inputMode="decimal" aria-label={t("{label} value", { label })}
          disabled={disabled} spellCheck={false} value={typed ?? format(value)}
          onFocus={(event) => { setTyped(String(value)); const box = event.currentTarget; setTimeout(() => box.select()); }}
          onChange={(event) => setTyped(event.target.value)}
          onBlur={() => { if (typed !== null) done(typed); }}
          onKeyDown={(event) => {
            if (event.key === "Enter") { event.preventDefault(); if (typed !== null) done(typed); }
            else if (event.key === "Escape" && typed !== null) { event.preventDefault(); setTyped(null); }
          }} />
      </div>
    </div>
  );
}

/** Peaks as bars (0…1 each), centred. */
function Bars({ peaks, className }: { peaks: number[]; className: string }) {
  const n = Math.max(peaks.length, 1);
  const d = peaks.map((p, i) => {
    const h = Math.max(p, 0.004) * 46;
    return `M${i + 0.5} ${50 - h}V${50 + h}`;
  }).join("");
  return (
    <svg className={`sound-wave-bars ${className}`} viewBox={`0 0 ${n} 100`} preserveAspectRatio="none" aria-hidden="true">
      <path d={d} strokeWidth={0.62} />
    </svg>
  );
}

/** Plays one WAV at a time; any other Extras player stops it. */
function usePlayer() {
  const [playing, setPlaying] = useState<"" | "game" | "yours">("");
  const node = useRef<AudioBufferSourceNode | null>(null);
  const stop = () => {
    if (node.current) {
      node.current.onended = null;
      try { node.current.stop(); } catch { /* already ended */ }
      node.current.disconnect();
      node.current = null;
    }
    setPlaying("");
  };
  useEffect(() => {
    const onStop = () => stop();
    window.addEventListener(stopEvent, onStop);
    return () => { window.removeEventListener(stopEvent, onStop); stop(); };
  }, []);
  async function play(which: "game" | "yours", bytes: ArrayBuffer) {
    window.dispatchEvent(new Event(stopEvent));
    if (typeof AudioContext === "undefined" || !bytes.byteLength) return;
    const ctx = (context ??= new AudioContext());
    await ctx.resume();
    const buffer = await ctx.decodeAudioData(bytes.slice(0));
    const source = ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(ctx.destination);
    source.onended = () => { if (node.current === source) { node.current = null; setPlaying(""); } };
    node.current = source;
    setPlaying(which);
    source.start();
  }
  return { playing, play, stop };
}

export function SoundEditor({ hero, heroName, name, target, row, disabled, commit, close, silence, silenceRefusal, variations }: SoundEditorProps) {
  const t = useT();
  const [draft, setDraft] = useState<SoundEditValues | null>(null),
    [replacing, setReplacing] = useState(false),
    [shared, setShared] = useState(false),
    [playError, setPlayError] = useState("");
  const shownRow = draft ? withEdit(row, draft) : row;
  const values = editValues(shownRow);
  const request = useMemo(() => ({ hero_path: hero, row: shownRow, buckets: 480 }), [hero, JSON.stringify(shownRow)]);
  const { preview, error, loading } = useSoundPreview(request);
  const player = usePlayer();
  const gameWave = useRef<ArrayBuffer | null>(null);
  const edited = hasEdit(row);
  const game = preview?.game, source = preview?.source;
  const length = game?.duration ?? 0, fileLength = source?.duration ?? 0;
  const maxStart = Math.max(0, fileLength - length);

  function write(changes: SoundEditValues) {
    setDraft(null);
    commit(withEdit(row, changes));
  }
  function change(key: EditKey) {
    return {
      draft: (v: number | null) => setDraft(v === null ? null : { ...editValues(row), ...draft, [key]: v }),
      commit: (v: number) => write({ ...(draft ?? {}), [key]: v }),
    };
  }
  async function playGame() {
    setPlayError("");
    if (player.playing === "game") return player.stop();
    try {
      gameWave.current ??= parseExtraMedia(await api.previewExtra({ hero_path: hero, target, kind: "audio", max_dimension: 512 })).bytes;
      await player.play("game", gameWave.current);
    } catch (e) { setPlayError(errorText(e)); }
  }
  async function playYours() {
    setPlayError("");
    if (player.playing === "yours") return player.stop();
    if (!preview?.wave.byteLength) return;
    try { await player.play("yours", preview.wave); } catch (e) { setPlayError(errorText(e)); }
  }
  const window_ = preview?.result?.report.window ?? null;
  // Each note keeps an English key of its own; its text is drawn in the chosen language.
  const notes: { key: string; text: string }[] = [];
  if (window_?.gain_limited && window_.applied_gain_db !== null)
    notes.push({ key: "held", text: t("Volume held at {volume}: louder would clip, so the loudest peak stays just below full scale.", { volume: decibels(window_.applied_gain_db - (preview?.percent_db ?? 0)) }) });
  if (window_?.looping)
    notes.push({ key: "loops", text: t("This sound loops: fade out is not used (it would dip on every loop), and fade in ends where the loop starts.") });
  else if (window_?.fade_in_shortened || window_?.fade_out_shortened)
    notes.push({ key: "fade", text: t("A fade is longer than the part of your file that plays, so it is held to that part.") });
  if (preview && preview.percent !== 100)
    notes.push({ key: "percent", text: preview.percent_db !== null
      ? t("An earlier volume of {percent}% ({volume}) also applies. Reset edits sets it back to 100%.", { percent: preview.percent, volume: decibels(preview.percent_db) })
      : t("An earlier volume of {percent}% also applies. Reset edits sets it back to 100%.", { percent: preview.percent }) });
  const matched = preview?.matched ?? null;
  const matchedNow = matched && Math.abs(values.volume_db - matched.volume_db) < 0.05;
  const resettable = edited || (row.volume_percent !== undefined && row.volume_percent !== 100);

  return (
    <div className="sound-edit" role="region" aria-label={t("Sound editor")}>
      <div className="sound-edit-bar">
        <strong>{t("Sounds")}</strong>
        <span className="extra-pill ready">{t("Replaced")}</span>
        <span className="sound-edit-name">{heroName} · {name}</span>
        <button type="button" className="extra-secondary sound-edit-back" onClick={close}>{t("Back to sounds")}</button>
      </div>
      <div className="sound-edit-grid">
        <section className="sound-edit-card sound-edit-waves" aria-label={t("Waveforms")}>
          <div className="sound-edit-file">
            <h3><T text="Your file <code>{file}</code>" values={{ file: basename(row.source ?? "") }} />{source && <code> · {seconds(fileLength)}</code>}</h3>
            <div className="sound-edit-listen" role="group" aria-label={t("Listen")}>
              <button type="button" aria-pressed={player.playing === "game"} onClick={() => void playGame()}>
                {player.playing === "game" ? "■" : "▶"} {t("The game’s")}
              </button>
              <button type="button" className="yours" aria-pressed={player.playing === "yours"} disabled={!preview?.wave.byteLength} onClick={() => void playYours()}>
                {player.playing === "yours" ? "■" : "▶"} {t("Yours")}
              </button>
            </div>
          </div>
          <FileWave preview={preview} error={preview ? "" : error} start={values.start_seconds} maxStart={maxStart} disabled={disabled}
            draft={(v) => setDraft(v === null ? null : { ...editValues(row), ...draft, start_seconds: v })}
            commit={(v) => write({ ...(draft ?? {}), start_seconds: v })} />
          <h4 className="sound-edit-caption">{t("What the game will play")}{game ? ` · ${seconds(length)}` : ""}</h4>
          <div className="sound-wave result" data-testid="sound-result">
            {preview?.result ? <div className="sound-wave-area"><Bars peaks={preview.result.peaks} className="result" /></div> : preview?.refusal ? (
              <div className="sound-edit-refusal" role="status">
                <p>{!preview.edited && fileLength > length
                  ? t("Your file ({file}) is longer than the game’s sound ({game}), so nothing plays yet. Drag the frame, or use the part in it as it is.", { file: seconds(fileLength), game: seconds(length) })
                  : preview.refusal}</p>
                {!preview.edited && (
                  <button type="button" className="extra-secondary" disabled={disabled} onClick={() => write({ start_seconds: values.start_seconds })}>
                    {t("Use the part in the frame")}
                  </button>
                )}
              </div>
            ) : <span className="sound-wave-empty">{error || (loading ? t("Rendering…") : "")}</span>}
            <div className="sound-wave-scale"><span>{t("{value} s", { value: 0 })}</span><span>{t("with your volume and fades")}</span><span>{game ? seconds(length) : ""}</span></div>
          </div>
          <h4 className="sound-edit-caption">{t("The game’s sound, for comparison")}</h4>
          <div className="sound-wave game">
            {game && <div className="sound-wave-area"><Bars peaks={game.peaks} className="game" /></div>}
          </div>
          {(error && preview) || playError ? <p className="extra-error" role="alert">{playError || error}</p> : null}
        </section>
        <aside className="sound-edit-card sound-edit-panel" aria-label={t("Edit")}>
          <h3>{t("Edit")}</h3>
          <p className="sound-edit-lead">{t("Changes are heard right away with ▶ Yours.")}</p>
          <fieldset className="tune-sliders sound-edit-sliders" disabled={disabled || !preview}>
            <EditSlider label={t("Starts at")} hint={t("Where in your file the sound begins.")} value={values.start_seconds} min={0} max={Math.max(maxStart, 0.001)}
              step={0.01} zero={0} format={seconds} disabled={disabled || maxStart <= 0} {...change("start_seconds")} />
            <EditSlider label={t("Volume")} hint={t("Louder or quieter than your file.")} value={values.volume_db} min={VOLUME_DB_RANGE[0]} max={VOLUME_DB_RANGE[1]}
              step={0.5} zero={0} format={decibels} disabled={disabled} {...change("volume_db")} />
            <EditSlider label={t("Fade in")} hint={t("A soft start.")} value={values.fade_in_seconds} min={0} max={Math.max(length, 0.01)}
              step={0.01} zero={0} format={seconds} disabled={disabled} {...change("fade_in_seconds")} />
            <EditSlider label={t("Fade out")} hint={window_?.looping ? t("Not used: this sound loops.") : t("A soft end.")} value={values.fade_out_seconds}
              min={0} max={Math.max(length, 0.01)} step={0.01} zero={0} format={seconds} disabled={disabled || Boolean(game && game.loop_start >= 0)} {...change("fade_out_seconds")} />
          </fieldset>
          {notes.length > 0 && <ul className="sound-edit-notes">{notes.map((n) => <li key={n.key}>{n.text}</li>)}</ul>}
          <div className="sound-edit-actions">
            <button type="button" className="extra-secondary" disabled={disabled || !matched}
              title={t("Sets Volume so the part that plays measures as loud as the game’s sound (BS.1770 loudness).")}
              onClick={() => matched && write({ ...(draft ?? {}), volume_db: matched.volume_db })}>
              {t("Match the game’s loudness")}
            </button>
            <button type="button" className="extra-text-button" disabled={disabled || !resettable}
              onClick={() => { setDraft(null); commit(withoutEdit(row)); }}>
              {t("Reset edits")}
            </button>
          </div>
          {matchedNow && matched?.limited_by_clipping && <p className="sound-edit-note">{t("Matched as far as it goes without clipping: the game’s sound is louder still.")}</p>}
          {matchedNow && !matched?.limited_by_clipping && matched?.limited_by_range && <p className="sound-edit-note">{t("Matched as far as the Volume range goes.")}</p>}
          {game && (
            <div className="sound-edit-notice length">
              <T text="<strong>The game fixes this sound’s length: {length}.</strong> A replacement cannot be longer or shorter, so “trimming” means choosing which {length} of your file plays. If your file is shorter, silence fills the rest." values={{ length: seconds(length) }} />
            </div>
          )}
          <div className="sound-edit-notice kept">
            {t("Used by the next build. The edits are kept in the project; your file is not changed.")}
          </div>
          <h4 className="sound-edit-caption">{t("Also here")}</h4>
          <div className="sound-edit-also">
            {variations && variations.count > 1 && (
              <button type="button" className="extra-secondary" aria-expanded={replacing} disabled={disabled} onClick={() => { setReplacing(!replacing); setShared(false); }}>
                {tp(variations.count, { one: "Replace all {count} variations with this part", other: "Replace all {count} variations with this part" })}
              </button>
            )}
            <button type="button" className="extra-secondary" disabled={disabled || Boolean(silenceRefusal)} title={silenceRefusal || undefined} onClick={silence}>
              {t("Silence this sound")}
            </button>
          </div>
          {variations && replacing && (
            <div className="sounds-replace-all" role="group" aria-label={t("Replace all variations with this part")}>
              <p>{t("Every variation gets this file with the same start, volume and fades. A fade longer than a shorter variation is held to it.")}</p>
              {variations.sharedCount > 0 && (
                <label className="sounds-replace-shared">
                  <input type="checkbox" checked={shared} disabled={disabled} onChange={(e) => setShared(e.target.checked)} />
                  {tp(variations.sharedCount, { one: "Also replace the shared variations ({count}). This can also change other heroes.", other: "Also replace the shared variations ({count}). This can also change other heroes." })}
                </label>
              )}
              <button type="button" className="extra-primary" disabled={disabled}
                onClick={() => {
                  // The part in the frame always goes with it, even when it was never moved.
                  const saved = { ...Object.fromEntries(EDIT_KEYS.filter((k) => typeof shownRow[k] === "number").map((k) => [k, shownRow[k] as number])), start_seconds: values.start_seconds };
                  variations.apply(saved, shared);
                  setReplacing(false);
                }}>
                {tp(variations.count, { one: "Replace {count} variations", other: "Replace {count} variations" })}
              </button>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}

/** "Your file" with the frame: drag it (or click the waveform, or use the arrow keys on it). */
function FileWave({ preview, error, start, maxStart, disabled, draft, commit }: {
  preview: SoundEditPreview | null; error: string; start: number; maxStart: number; disabled: boolean;
  draft: (v: number | null) => void; commit: (v: number) => void;
}): ReactNode {
  const t = useT();
  const box = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; from: number; value: number } | null>(null);
  if (!preview)
    return <div className="sound-wave file">{error
      ? <p className="sound-edit-refusal extra-error" role="alert">{error}</p>
      : <span className="sound-wave-empty">{t("Reading your file…")}</span>}</div>;
  const { source, game } = preview;
  const total = Math.max(source.duration, game.duration, 0.001);
  const pct = (s: number) => `${(Math.min(total, Math.max(0, s)) / total) * 100}%`;
  const clamp = (v: number) => Math.round(Math.min(maxStart, Math.max(0, v)) * 1000) / 1000;
  const width = () => box.current?.getBoundingClientRect().width || 1;
  const loops = game.loop_start >= 0
    ? [game.loop_start, game.loop_end > 0 ? game.loop_end : null].filter((f): f is number => f !== null).map((f) => start + f / game.rate)
    : [];
  function down(event: ReactPointerEvent<HTMLDivElement>, onFrame: boolean) {
    if (disabled || maxStart <= 0) return;
    event.preventDefault();
    const rect = box.current?.getBoundingClientRect();
    const value = onFrame || !rect ? start : clamp(((event.clientX - rect.left) / width()) * total - game.duration / 2);
    drag.current = { x: event.clientX, from: value, value };
    (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
    if (!onFrame) draft(value);
  }
  function move(event: ReactPointerEvent<HTMLDivElement>) {
    if (!drag.current) return;
    drag.current.value = clamp(drag.current.from + ((event.clientX - drag.current.x) / width()) * total);
    draft(drag.current.value);
  }
  function up() {
    if (!drag.current) return;
    const value = drag.current.value;
    drag.current = null;
    draft(null);
    commit(value);
  }
  function keys(event: KeyboardEvent<HTMLDivElement>) {
    const step = event.shiftKey ? 0.1 : 0.01;
    const next = ({ ArrowRight: start + step, ArrowUp: start + step, ArrowLeft: start - step, ArrowDown: start - step, Home: 0, End: maxStart } as Record<string, number>)[event.key];
    if (next === undefined || disabled) return;
    event.preventDefault();
    commit(clamp(next));
  }
  return (
    <div className="sound-wave file" onPointerDown={(e) => down(e, false)} onPointerMove={move} onPointerUp={up} onPointerCancel={up}>
      <div className="sound-wave-area" ref={box}>
      <Bars peaks={source.peaks} className="dim" />
      <div className="sound-wave-lit" style={{ clipPath: `inset(0 calc(100% - ${pct(start + game.duration)}) 0 ${pct(start)})` }}>
        <Bars peaks={source.peaks} className="lit" />
      </div>
      <div className="sound-frame" role="slider" tabIndex={disabled ? -1 : 0} aria-label={t("Part that plays")}
        aria-valuemin={0} aria-valuemax={Math.round(maxStart * 100) / 100} aria-valuenow={Math.round(start * 100) / 100}
        aria-valuetext={t("From {start} to {end}", { start: seconds(start), end: seconds(start + game.duration) })} aria-disabled={disabled || maxStart <= 0}
        data-testid="sound-frame" style={{ left: pct(start), width: pct(game.duration) }}
        onPointerDown={(e) => { e.stopPropagation(); down(e, true); }} onKeyDown={keys}>
        {loops.map((at, i) => (
          <span key={i} className="sound-loop" style={{ left: `${((at - start) / game.duration) * 100}%` }} title={i === 0 ? t("Loop start (the game's)") : t("Loop end (the game's)")}>
            <em>{i === 0 ? t("loop") : t("end")}</em>
          </span>
        ))}
      </div>
      </div>
      <div className="sound-wave-scale">
        <span>{t("{value} s", { value: 0 })}</span>
        <span>{maxStart > 0
          ? t("Drag the frame to choose the part that plays · it is always {length} long", { length: seconds(game.duration) })
          : t("Your file is not longer than the game’s sound ({length}): all of it plays, then silence", { length: seconds(game.duration) })}</span>
        <span>{seconds(source.duration)}</span>
      </div>
    </div>
  );
}
