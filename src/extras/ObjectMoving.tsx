/** Extras → Objects, for an object the game animates (several bones; shown only for rows a
 * switch opened, `kind.moving`): "How your model moves" (the three ways as cards), "Your parts"
 * (what each separate object of the file follows, the engine's suggestion marked), and the player
 * under the view (the game's own animations of the object on the fitted model). Every change is
 * one step through the row helpers (`withFollow`); selecting writes nothing. The motion is asked
 * without the operation lease, newest request wins (an older answer is dropped). */
// i18n-area: objects
import { useEffect, useRef, useState } from "react";
import { m, t, tp, useT } from "../i18n";
import { sentences } from "../build/words";
import type { ObjectMotion } from "./ObjectScene";
import type { ObjectClip, ObjectFollow, ObjectFollowWay, ObjectMotionHeader, ObjectMovingView, ObjectRow } from "./objectTypes";
import { OBJECT_MOTION_KEPT, OBJECT_MOTION_NOT_READ } from "./objectTypes";
import { cleanFollow, withPartBone } from "./objectRows";

export const WAYS: { way: ObjectFollowWay; title: string; text: string }[] = [
  { way: "one_piece", title: m("In one piece"), text: m("The whole model follows the object's main part. Simplest: nothing inside it moves.") },
  { way: "by_parts", title: m("By parts"), text: m("Each separate object in your file follows one of the game object's moving parts.") },
  { way: "same_skeleton", title: m("My model has the same skeleton"), text: m("For a model you rigged on this object's own bones. Advanced.") },
];
export const RIGID_LINE = m("A part moves and turns with what it follows; it does not bend or stretch.");
export const SUGGEST_LINE = m("The app suggests the nearest moving part; change any of them here.");
export const SAME_SKELETON_CLOSED_LINE = m("Not offered yet.");
/** The badge on the way the app picks for this file when the row has none. */
export const SUGGESTED_WAY = m("suggested for this object");
/** Two whole sentences one after the other (a space in English, none between two Chinese ones). */
export const twoSentences = (first: string, second: string) => sentences(first, second);

/** The way a row follows: its own, else by parts when the file holds more than one separate object. */
export function wayOf(follow: ObjectFollow | undefined, parts: number): ObjectFollowWay {
  return follow?.way ?? (parts > 1 ? "by_parts" : "one_piece");
}

export function MovingPanel({ entry, follow, view, clips, setFollow }: {
  entry: ObjectRow;
  follow: ObjectFollow | undefined;
  view: ObjectMovingView | undefined;
  clips: ObjectMotionHeader["clips"];
  setFollow: (follow: ObjectFollow) => void;
}) {
  const t = useT();
  const parts = view?.parts ?? [];
  // Body-group states: the app picks each state's bones (Rust's line), so no bone is offered by hand.
  const perState = entry.kind?.moving?.per_state;
  const way = follow?.way ?? view?.way ?? wayOf(follow, parts.length);
  const carriers = view?.carriers ?? [];
  const bone = (name: string) => <option key={name} value={name}>{name}</option>;
  // The way the app takes for this file when the row names none.
  const suggestedWay = parts.length ? wayOf(undefined, parts.length) : null;
  return (
    <section className="objects-moving" aria-label={t("How your model moves")}>
      {parts.length > 0 && <p className="objects-note">{partsLine(parts)}</p>}
      <h3>{t("How your model moves")}</h3>
      <div className="objects-ways" role="radiogroup" aria-label={t("How your model moves")}>
        {WAYS.map((w) => {
          const closed = w.way === "same_skeleton" && !view?.same_skeleton_open;
          return (
            <button key={w.way} type="button" role="radio" aria-checked={way === w.way} disabled={closed} className={`objects-way ${way === w.way ? "chosen" : ""}`}
              onClick={() => way !== w.way && setFollow(cleanFollow({ way: w.way })!)}>
              <strong>{t(w.title)}{w.way === suggestedWay && <span className="objects-suggested-way">{t(SUGGESTED_WAY)}</span>}</strong>
              <span>{closed ? twoSentences(t(w.text), t(SAME_SKELETON_CLOSED_LINE)) : t(w.text)}</span>
            </button>
          );
        })}
      </div>
      {view?.refused && <p className="objects-warn" role="note">{view.refused}</p>}
      {perState && way !== "same_skeleton" && <p className="objects-note">{perState}</p>}
      {!perState && way === "one_piece" && carriers.length > 0 && (
        <label className="objects-follow-one">
          {t("Follows")}
          <select value={follow?.bone ?? view?.default_bone ?? ""} onChange={(e) => setFollow({ way: "one_piece", bone: e.target.value })}>
            {carriers.map((c) => bone(c.name))}
          </select>
          {(follow?.bone ?? view?.default_bone) === view?.default_bone && <span className="objects-suggested">{t("suggested: carries most of the game's object")}</span>}
        </label>
      )}
      {!perState && way === "by_parts" && (
        <>
          <h3>{t("Your parts")}</h3>
          <ul className="objects-parts">
            {parts.map((part) => {
              const chosen = follow?.parts?.[part] ?? view?.bound[part] ?? view?.suggested[part] ?? "";
              return (
                <li key={part}>
                  <span className="objects-part-name">{part}</span>
                  <span className="objects-follows" aria-hidden="true">{t("follows")}</span>
                  <select aria-label={t("{part} follows", { part })} value={chosen} onChange={(e) => setFollow(withPartBone(follow, part, e.target.value))}>
                    {carriers.map((c) => bone(c.name))}
                  </select>
                  {chosen === view?.suggested[part] && <span className="objects-suggested">{t("suggested")}</span>}
                </li>
              );
            })}
          </ul>
        </>
      )}
      {way !== "same_skeleton" && <p className="objects-note">{way === "by_parts" && !perState ? twoSentences(t(RIGID_LINE), t(SUGGEST_LINE)) : t(RIGID_LINE)}</p>}
      {clips.length > 0 && (
        // How the game's graph plays each animation, under "Stays the game's" item one.
        <ul className="objects-clips" aria-label={t("Its animations")}>
          {clips.map((c) => <li key={c.file}>{clipLine(c, entry.ability_name)}</li>)}
        </ul>
      )}
    </section>
  );
}

/** "3 separate objects found in the file: Body, Cap, Liquid." */
export function partsLine(parts: string[]): string {
  return tp(parts.length, { one: "{count} separate object found in the file: {names}.", other: "{count} separate objects found in the file: {names}." }, { names: parts.join(", ") });
}

/** Parses a motion answer: `[u32 header length][JSON {mime_type, details}][data]`. */
export function parseMotion(bytes: ArrayBuffer): { header: ObjectMotionHeader; motion: ObjectMotion | null } {
  const length = new DataView(bytes).getUint32(0, true);
  const meta = JSON.parse(new TextDecoder().decode(new Uint8Array(bytes, 4, length))) as { details: ObjectMotionHeader };
  const header = meta.details;
  const data = new Float32Array(bytes.slice(4 + length));
  const motion = header.frames > 0 && data.length === header.frames * header.bones.length * 12 ? { bones: header.bones, frames: header.frames, data } : null;
  return { header, motion };
}

/** The player under the view: play / pause, the animation, a time bar. Owns no project state.
 * `keptAnimation`: the build keeps the game's own animation blocks (the row's kind says so);
 * when the player cannot read them it says so calmly (`OBJECT_MOTION_KEPT`).
 * `plays`: what the view moves (your fitted model, the game's object, or no file chosen yet). */
export function ObjectPlayer({ target, load, onMotion, onFrame, onClips, keptAnimation = false, plays = "no_file" }: {
  target: string;
  plays?: PlayerPlays;
  load: (target: string, clip: string | null) => Promise<ArrayBuffer>;
  onMotion: (motion: ObjectMotion | null) => void;
  onFrame: (frame: number) => void;
  onClips: (clips: ObjectMotionHeader["clips"]) => void;
  keptAnimation?: boolean;
}) {
  const t = useT();
  const [header, setHeader] = useState<ObjectMotionHeader | null>(null);
  const [clip, setClip] = useState<string | null>(null);
  const [playing, setPlaying] = useState(true);
  const [time, setTime] = useState(0);
  const asked = useRef(0);
  useEffect(() => {
    const ask = ++asked.current;
    load(target, clip).then((bytes) => {
      if (ask !== asked.current) return; // newest wins
      const { header, motion } = parseMotion(bytes);
      setHeader(header);
      onClips(header.clips);
      onMotion(motion);
      setTime(0);
    }, () => { if (ask === asked.current) { setHeader(null); onMotion(null); } });
  }, [target, clip]); // eslint-disable-line react-hooks/exhaustive-deps
  const frames = header?.frames ?? 0;
  const shown = header?.clips.find((c) => c.file === header.clip);
  // A clip the game plays once stops on its last pose (as the game holds it); Play starts it again.
  const loops = shown?.loops === true;
  useEffect(() => {
    if (!playing || frames < 2) return;
    let start: number | null = null;
    let id = requestAnimationFrame(function tick(now) {
      start ??= now - (time / (header?.fps ?? 30)) * 1000;
      const run = ((now - start) / 1000) * (header?.fps ?? 30);
      const at = loops ? run % frames : Math.min(run, frames - 1);
      setTime(at);
      onFrame(at);
      if (!loops && at >= frames - 1) { setPlaying(false); return; }
      id = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(id);
  }, [playing, frames]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!header) return null;
  if (!frames) return <p className="objects-view-line" role="status">{keptAnimation ? OBJECT_MOTION_KEPT : header.note ?? OBJECT_MOTION_NOT_READ}</p>;
  const play = () => {
    if (!playing && !loops && time >= frames - 1) { setTime(0); onFrame(0); }
    setPlaying(!playing);
  };
  return (
    <div className="objects-player-block">
      <div className="objects-player" aria-label={t("The game's animation")}>
        <button type="button" className="extra-secondary objects-play" aria-label={playing ? t("Pause") : t("Play")} onClick={play}>{playing ? `❚❚ ${t("Pause")}` : `▶ ${t("Play")}`}</button>
        <span className="objects-seg" role="group" aria-label={t("Animation")}>
          {header.clips.map((c) => (
            <button key={c.file} type="button" aria-pressed={c.file === header.clip} onClick={() => c.file !== header.clip && setClip(c.file)}>{c.label}</button>
          ))}
        </span>
        <input type="range" aria-label={t("Time")} min={0} max={Math.max(frames - 1, 0)} step={1} value={Math.floor(time)} onChange={(e) => { setPlaying(false); setTime(Number(e.target.value)); onFrame(Number(e.target.value)); }} />
        <span className="objects-time">{t("{at} / {length} s", { at: (time / header.fps).toFixed(1), length: ((frames - 1) / header.fps).toFixed(1) })}</span>
      </div>
      <p className="objects-view-line">{shown && playsLine(shown) ? twoSentences(playerLine(plays), playsLine(shown)) : playerLine(plays)}</p>
      {(header.lines ?? []).map((line) => <p key={line} className="objects-view-line" role="note">{line}</p>)}
    </div>
  );
}

/** One clip in "Its animations": name, length, and how the game's graph plays it. */
export function clipLine(clip: ObjectClip, ability: string | undefined): string {
  const values = { clip: clip.label, seconds: clip.duration.toFixed(1), ability: ability ?? "" };
  const line = clip.additive
    ? ability
      ? t("{clip} ({seconds} s, played over the others): the game plays it while {ability} runs.", values)
      : t("{clip} ({seconds} s, played over the others): the game plays it while the ability runs.", values)
    : ability
      ? t("{clip} ({seconds} s): the game plays it while {ability} runs.", values)
      : t("{clip} ({seconds} s): the game plays it while the ability runs.", values);
  const plays = playsLine(clip);
  return plays ? twoSentences(line, plays) : line;
}

/** Under the player when the fit view could not bind your model (shown at rest, the game's object moves). */
export const STILL_LINE = m("Your model is shown at rest here: its lighter preview copy could not be matched to its parts. The build is not affected.");
export const PLAYER_LINE = m("The game's own animations of this object, played on your model, so you can see each part move before you build.");
/** The player's line when no file is chosen yet: the game's own object plays. */
export const PLAYER_LINE_NO_FILE = m("The game's own animations of this object. Choose a file to see them played on your model.");
/** The player's line when your model stays at rest (`STILL_LINE` says why): the game's object plays. */
export const PLAYER_LINE_GAME = m("The game's own animations of this object, played on the game's object.");
/** Which line the player shows: `yours` only when your fitted model is what plays. */
export type PlayerPlays = "yours" | "game" | "no_file";
const playerLine = (plays: PlayerPlays) => t(plays === "yours" ? PLAYER_LINE : plays === "game" ? PLAYER_LINE_GAME : PLAYER_LINE_NO_FILE);
/** How the object's graph plays a clip, from its clip node (in the language shown). */
export function playsLine(clip: ObjectClip): string {
  if (clip.loops === true) return t("It loops.");
  if (clip.loops === false) return t("The game plays it once and holds its last pose.");
  return "";
}
