import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { api } from '../api';
import type { Project } from '../models';
import type { PreviewScene } from '../preview/packet';
import { useAutoLoad } from '../preview/autoLoad';
import { StudioView } from '../studio/Studio';
import type { AnimationEntry } from '../components/AssetCatalog';
import { decodeClipPacket, readout, shownLength } from './clipPacket';
import { isDropped } from './madeQueue';
import { MADE_SUPERSEDED } from './makeTypes';
import type { PlayerClock } from './ClipViewport';
import type { ClipMark, ClipPacket } from './motionTypes';
const ClipViewport = lazy(() => import('./ClipViewport'));

export type PlayMode = 'game' | 'yours' | 'both';
/** The packet of a clip made from the user's animation, and the key that names what it depends on. */
export interface MadePreview { key: string; fetch: () => Promise<ArrayBuffer>;
  /** For a clip taken from another hero: that hero's name ("Yours" then plays it; the notes name it). */
  from?: string }
const SPEEDS: readonly [number, string][] = [[1, '1×'], [.5, '½×'], [.25, '¼×']];
/** Event kinds marked on the time bar (tags and transition rules are graph details). */
const MARKED = new Set(['sound', 'foot', 'effect', 'game']);

/** Decoded packets of this session, newest last (a few clips, so going back is instant). */
const kept = new Map<string, Promise<ClipPacket>>();
const KEEP = 8;
function packet(project: Project, target: string, which: 'game' | 'yours', source: string): Promise<ClipPacket> {
  const key = `${project.dmx}|${target}|${which}|${source}`;
  const held = kept.get(key);
  if (held) { kept.delete(key); kept.set(key, held); return held; }
  const loading = api.clipPreview(project, target, which, which === 'yours' ? source : undefined).then(decodeClipPacket);
  kept.set(key, loading);
  loading.catch(() => { if (kept.get(key) === loading) kept.delete(key); });
  while (kept.size > KEEP) kept.delete(kept.keys().next().value as string);
  return loading;
}
/** Forget kept packets (tests). */
export function forgetClipPackets() { kept.clear(); }

/** The packets last shown (kept on screen while the next clip loads, so the 3D view stays mounted). */
type Shown = {key: string; mode: PlayMode; game: ClipPacket | null; yours: ClipPacket | null};
type Request = {key: string; loading: boolean; error: string | null};

/** The selected clip played on the prepared character: the game's clip, yours, or both (yours in
 * full, the game's as a see-through figure, in step). The character loads on a click (or by
 * itself with Settings → Load 3D previews automatically); a clip's packet loads when the clip is
 * selected and the character is shown, the newest request winning. This card never remounts when
 * the selection changes: only what it shows does. */
export function ClipPlayer({project, entry, source, made = null, preview, load, autoLoad, previewCurrent = true, busy}: {
  project: Project; entry: AnimationEntry | null; /** The saved replacement's file, when there is one. */source: string;
  /** A clip made from the user's animation: its packet (`madeClipPreview`, asked one at a time) and
   * what it depends on. "Your animation" then plays it instead of the saved file. */
  made?: MadePreview | null;
  preview: PreviewScene | null; load?: () => void; autoLoad?: () => Promise<boolean>; previewCurrent?: boolean; busy: boolean;
}) {
  const [mode, setMode] = useState<PlayMode>('game');
  const [playing, setPlaying] = useState(true), [speed, setSpeed] = useState(1), [loop, setLoop] = useState(true);
  const hasYours = Boolean(source || made);
  const shownMode: PlayMode = hasYours ? mode : 'game';
  const wantGame = shownMode !== 'yours', wantYours = shownMode !== 'game';
  const key = entry && preview && previewCurrent ? `${project.dmx}|${entry.target}|${shownMode}|${made ? `made:${made.key}` : source}` : '';
  const [shown, setShown] = useState<Shown | null>(null);
  const [request, setRequest] = useState<Request>({key: '', loading: false, error: null});
  const latest = useRef(0);
  useEffect(() => {
    if (!key || !entry) return;
    const asking = ++latest.current, mode = shownMode;
    setRequest({key, loading: true, error: null});
    const asked = {...project};
    const yours = !wantYours ? null : made ? made.fetch().then(decodeClipPacket) : packet(asked, entry.target, 'yours', source);
    Promise.all([wantGame ? packet(asked, entry.target, 'game', '') : null, yours])
      .then(([game, yours]) => { if (asking === latest.current) { setShown({key, mode, game, yours}); setRequest({key, loading: false, error: null}); } })
      .catch(error => { if (asking === latest.current && !isDropped(error) && errorText(error) !== MADE_SUPERSEDED) setRequest({key, loading: false, error: errorText(error)}); });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key holds what the packets depend on
  }, [key]);
  // Load 3D previews automatically: the character once per prepared model while this page is shown.
  useAutoLoad({on: Boolean(autoLoad), key: project.dmx && (!preview || !previewCurrent) ? `dmx:${project.dmx}` : null, busy, load: () => autoLoad?.() ?? false});
  const visible = key ? shown : null;
  const main = visible ? (visible.mode === 'game' ? visible.game : visible.yours) : null;
  const ghost = visible?.mode === 'both' ? visible.game : null;
  const pending = request.key === key && request.loading && visible?.key !== key;
  const failed = request.key === key && request.error ? request.error : null;
  const clip = main?.header.clips[0] ?? null;
  // The shared clock: the render loop moves it; the controls read it without a React render per frame.
  const clock = useRef<PlayerClock>({time: 0, length: 0, playing: true, speed: 1, loop: true, dirty: true});
  clock.current.playing = playing; clock.current.speed = speed; clock.current.loop = loop;
  clock.current.ended = () => setPlaying(false);
  const length = clip ? shownLength(clip) : 0;
  useEffect(() => { clock.current.length = length; clock.current.time = Math.min(clock.current.time, length); clock.current.dirty = true; }, [length]);
  useEffect(() => { clock.current.time = 0; clock.current.dirty = true; }, [entry?.target]);
  const bar = useRef<HTMLInputElement>(null), text = useRef<HTMLSpanElement>(null);
  clock.current.tick = () => {
    if (!clip) return;
    if (bar.current) bar.current.value = String(length ? clock.current.time / length * 1000 : 0);
    if (text.current) text.current.textContent = readout(clip, clock.current.time);
  };
  useEffect(() => { clock.current.tick?.(); });
  function seek(value: number) { clock.current.time = value / 1000 * length; clock.current.dirty = true; clock.current.tick?.(); }
  function toggle() {
    if (!playing && clock.current.time >= length && length > 0) clock.current.time = 0;
    setPlaying(!playing);
  }
  const marks: ClipMark[] = (clip?.marks ?? []).filter(m => MARKED.has(m.kind));
  const words = [...new Set(marks.map(m => m.word.toLowerCase()))];
  const switcher = <span className="studio-view-group" role="group" aria-label="Clip to play">
    {([['game', 'The game’s clip'], ['yours', made?.from !== undefined ? 'Yours' : made ? 'Your animation' : 'Your clip'], ['both', 'Both']] as const).map(([id, label]) =>
      <button key={id} type="button" aria-pressed={shownMode === id} disabled={id !== 'game' && !hasYours} title={id !== 'game' && !hasYours ? 'Choose your replacement first' : undefined} onClick={() => setMode(id)}>{label}</button>)}
  </span>;
  const title = <span className="clip-view-title"><strong>{entry?.name ?? 'No clip selected'}</strong> <span className="muted">on your character</span></span>;
  let stage;
  if (!project.dmx) stage = <p className="hint studio-view-wait">Prepare your model first (step 2): the clip plays on your prepared character. The list, replacing and the fit checks work without it.</p>;
  else if (!preview || !previewCurrent) stage = <div className="studio-view-wait clip-wait"><p className="hint">See the selected clip played on your prepared character.</p><button type="button" className="primary" disabled={busy || !load} onClick={load}>Show my character</button></div>;
  else if (!entry) stage = <p className="hint studio-view-wait">Choose a clip from the list.</p>;
  else if (!main) stage = failed ? <div className="studio-view-wait clip-wait" role="status"><p className="hint">This clip could not be played here: {failed}</p></div> : <p className="hint studio-view-wait" role="status">Loading the clip…</p>;
  else stage = <>
    <Suspense fallback={<p className="hint studio-view-wait">Loading the 3D view…</p>}><ClipViewport scene={preview} main={main} ghost={ghost} clock={clock}/></Suspense>
    {pending && <p className="clip-overlay-note" role="status">Loading the clip…</p>}
    {failed && <div className="clip-overlay" role="status"><p className="hint">This clip could not be played here: {failed}</p></div>}
  </>;
  const notes: string[] = [];
  if (clip) {
    const hero = project.hero_name || 'this hero';
    notes.push(made?.from !== undefined
      ? shownMode === 'both' ? `Both: ${made.from || 'the other hero'}’s move on your character in full, ${hero}’s own clip as a see-through figure, in step. This is the clip the build writes.` : shownMode === 'yours' ? `${made.from || 'The other hero'}’s move played on your prepared character: the clip the build writes.` : 'The game’s clip played on your prepared character.'
      : made
      ? shownMode === 'both' ? 'Both: your animation in full, the game’s clip as a see-through figure, in step. This is the clip the build writes.' : shownMode === 'yours' ? 'Your animation played on your prepared character: the clip the build writes.' : 'The game’s clip played on your prepared character.'
      : shownMode === 'both' ? 'Both: your clip in full, the game’s clip as a see-through figure, in step.' : shownMode === 'yours' ? 'Your clip played on your prepared character.' : 'The game’s clip played on your prepared character.');
    notes.push('It plays in place; hair and cloth are not simulated here (see Physics & preview).');
    if (marks.length) notes.push(`Marks on the time bar are the clip’s events (${words.slice(0, 3).join(', ')}${words.length > 3 ? '…' : ''}).`);
    if (clip.additive) notes.push(`An additive clip, shown over the standing pose${clip.over ? ` (${clip.over})` : ''}${clip.blend_may_differ ? '; the game’s blend may differ' : ''}.`);
    if (clip.cut) notes.push(`Only the first ${clip.frames} of its ${clip.clip_frames} frames are shown.`);
  }
  return <StudioView title="Clip player" className="clip-player" bar={<>{title}<span className="studio-view-push"/>{switcher}</>} footer={<>
    {clip && <div className="clip-controls" role="group" aria-label="Playback">
      <button type="button" className="clip-play" aria-pressed={!playing} onClick={toggle}>{playing ? 'Pause' : 'Play'}</button>
      <span className="studio-view-group" role="group" aria-label="Speed">{SPEEDS.map(([value, label]) => <button key={value} type="button" aria-pressed={speed === value} aria-label={`Speed ${label}`} onClick={() => setSpeed(value)}>{label}</button>)}</span>
      <span className="clip-timebar">
        <input ref={bar} type="range" min={0} max={1000} step={1} defaultValue={0} aria-label="Time in the clip" onChange={event => seek(Number(event.target.value))}/>
        {marks.map((mark, i) => <span key={i} className={`clip-mark ${mark.kind}`} style={{left: `${mark.at * 100}%`}} title={`${mark.word} at ${(mark.at * length).toFixed(2)} s`} aria-hidden="true"/>)}
      </span>
      <span ref={text} className="clip-readout mono" aria-live="off">{readout(clip, 0)}</span>
      <button type="button" className="clip-loop" aria-pressed={loop} onClick={() => setLoop(!loop)}>Loop</button>
    </div>}
    {notes.length > 0 && <p className="hint">{notes.join(' ')}</p>}
    {clip && marks.length > 0 && <p className="sr-only">{marks.length} events: {marks.map(m => `${m.word} at ${(m.at * length).toFixed(2)} s`).join(', ')}</p>}
  </>}>
    {stage}
  </StudioView>;
}
function errorText(error: unknown) {
  return error && typeof error === 'object' && 'message' in error ? String((error as {message: unknown}).message) : String(error);
}
