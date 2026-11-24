import { useEffect, useRef, useState, type ReactNode } from 'react';
import { api } from '../api';
import { openFor } from '../dialogs';
import type { Project, ResourceReplacement } from '../models';
import type { AnimationEntry } from '../components/AssetCatalog';
import { useHistory } from '../history/useProjectHistory';
import { StudioOptionCards } from '../studio/StudioChoices';
import { fileNameParts } from './ClipCard';
import { ANIMATION_FILE_SUFFIXES, NOT_MADE, ONLY_ANIMATION_FILES, type ActionInfo, type ActionList, type ClipBones, type ClipMode, type MadeClipCheck, type MadeReport } from './makeTypes';
import {
  DEFAULT_LOOP_BLEND, MIN_LOOP_BLEND, TRACE_INCHES, actionLabel, gapWords, isAnimationFile, isMessage, latestPartStart, loopBlend, madeOf, madeRow,
  makesLines, messageOf, partStart, speedWords, withMade, type MadeChange,
} from './madeRows';
import { NewestQueue, isDropped } from './madeQueue';
import { MAX_LOOP_BLEND, MADE_SUPERSEDED } from './makeTypes';

/** The app's operation runner (busy state, Cancel, the one error line). */
export type Operate = <T>(label: string, operation: () => Promise<T>, canCancel?: boolean, refreshTimings?: boolean, keepError?: boolean) => Promise<T | undefined>;

export const LIST_FAILED = 'Could not read the animations of the file';
export const MAKE_FAILED = 'Could not make the clip from your animation';

const MODES: { value: ClipMode; title: string; badge?: string; badgeTone?: 'ok'; help: string }[] = [
  { value: 'as_it_looks', title: 'As it looks', badge: 'default', badgeTone: 'ok', help: 'Arms, legs and head point the way they do in your animation.' },
  { value: 'from_rest', title: 'Change from my rest pose', help: 'For a rig that rests in the hero’s own pose: the change from your rest pose is added to the hero’s.' },
];
const BONES: { value: ClipBones; title: string; help: string }[] = [
  { value: 'all', title: 'Every matched bone', help: 'The whole body. The hero’s own stance and footwork are replaced: feet may slide.' },
  { value: 'keyed', title: 'Only the bones my animation moves', help: 'The rest keeps the game’s motion. Good for a wave or a head turn.' },
];

/** What the page remembers for this session (never saved): the file chosen before an animation
 * is, per project and clip, and the animations listed per file, so choosing another clip and
 * coming back does not start Blender again. */
const pendingFiles = new Map<string, string>();
const actionLists = new Map<string, ActionList>();
/** Forget the session's choices (tests). */
export function forgetMadeState() { pendingFiles.clear(); actionLists.clear(); }
const fold = (value: string) => value.replace(/\\/g, '/').toLowerCase();
const listKey = (project: Project, source: string) => `${project.id}|${project.dmx}|${fold(source.trim())}`;

/** Said on the card when the engine answers NOT_MADE: the engine's own sentence ends by sending
 * the user to this page, which reads oddly on it. */
export const MAKE_AGAIN = 'This clip is not made yet, or your animation file, its animation or the prepared model changed since it was made.';
/** `notMade`: the engine's NOT_MADE sentence, as it answered (the clip must be made again). */
type ReportState = { key: string; report: MadeReport | null; notMade: string | null; error: string | null; loading: boolean };
type CheckState = { key: string; check: MadeClipCheck | null; error: string | null };

/** "Make it from my animation" in the clip's card: the animation file (Blender, FBX or glTF), its animation, the options and
 * what the app makes. Choosing a file only lists its animations (an operation: Blender); choosing an
 * animation writes the row (one step) and THEN makes the clip (an operation); every other option
 * writes the row and asks the engine for the report again (no Blender). Nothing is written when
 * this panel opens or a file is chosen. */
export function MakeClip({ project, entry, row, disabled, revision, write, remove, operate, made, useCompiled, openPreparation }: {
  project: Project; entry: AnimationEntry; row: ResourceReplacement | null; disabled: boolean;
  /** Bumped after every make, so the report and the player ask again although the row is the same. */
  revision: number;
  /** Write the clip's row as one step; returns the project written, or null when it could not be. */
  write: (row: ResourceReplacement) => Project | null;
  remove: () => void; operate: Operate;
  /** Called after a make settled (the page bumps the revision). */
  made: () => void;
  useCompiled: () => void; openPreparation?: () => void;
}) {
  const target = entry.target;
  const projectRef = useRef(project); projectRef.current = project;
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const saved = madeOf(row);
  const madeRowNow = saved ? row : null;
  const history = useHistory();
  const historyRef = useRef(history); historyRef.current = history;
  const dragging = useRef(false);
  const drag = (on: boolean) => { if (dragging.current === on) return; dragging.current = on; historyRef.current?.gesture(on); };
  useEffect(() => () => { if (dragging.current) historyRef.current?.gesture(false); }, []);

  // Can this clip be made at all, and can the prepared model be used (instant, no operation)?
  const checkKey = `${project.dmx}|${project.preparation_report}|${target}`;
  const [check, setCheck] = useState<CheckState>({ key: '', check: null, error: null });
  useEffect(() => {
    let current = true;
    setCheck({ key: checkKey, check: null, error: null });
    Promise.resolve().then(() => api.madeClipCheck(projectRef.current, target)).then(
      answer => { if (current && alive.current) setCheck({ key: checkKey, check: answer, error: null }); },
      error => { if (current && alive.current) setCheck({ key: checkKey, check: null, error: messageOf(error) }); });
    return () => { current = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key names the clip and the prepared model
  }, [checkKey]);
  const checked = check.key === checkKey ? check : null;

  // The file: the one chosen this session (not saved yet), else the saved row's.
  const pendingKey = `${project.id}|${fold(target)}`;
  const [pending, setPending] = useState(() => pendingFiles.get(pendingKey) ?? '');
  const file = pending || (saved ? row?.source ?? '' : '');
  const [list, setList] = useState<ActionList | null>(() => (file ? actionLists.get(listKey(project, file)) ?? null : null));
  useEffect(() => { setList(file ? actionLists.get(listKey(projectRef.current, file)) ?? null : null); }, [file, project.dmx]);
  const [fileError, setFileError] = useState('');
  const [listing, setListing] = useState(false), [making, setMaking] = useState(false);

  // The report of the clip the saved row makes (instant, no Blender), newest request winning.
  const queue = useRef(new NewestQueue());
  const reportKey = madeRowNow ? `${project.dmx}|${JSON.stringify(madeRowNow)}|${revision}` : '';
  const [report, setReport] = useState<ReportState>({ key: '', report: null, notMade: null, error: null, loading: false });
  useEffect(() => {
    if (!reportKey || !madeRowNow) { queue.current.drop(); return; }
    const key = reportKey, asked = madeRowNow;
    setReport(old => ({ key, report: old.report, notMade: null, error: null, loading: true }));
    queue.current.ask(() => api.madeClipReport(projectRef.current, target, asked)).then(
      value => { if (alive.current) setReport({ key, report: value, notMade: null, error: null, loading: false }); },
      error => {
        if (!alive.current || isDropped(error) || isMessage(error, MADE_SUPERSEDED)) return;
        if (isMessage(error, NOT_MADE)) setReport(old => ({ key, report: old.report, notMade: messageOf(error), error: null, loading: false }));
        else setReport(old => ({ key, report: old.report, notMade: null, error: messageOf(error), loading: false }));
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key holds the row and the revision
  }, [reportKey]);
  const shown = report.report;
  const current = report.key === reportKey ? report : null;

  /** Make the clip in Blender from `next` (already written), as an operation. */
  async function makeFrom(projectNow: Project, next: ResourceReplacement) {
    setMaking(true);
    try { await operate(MAKE_FAILED, async () => { const answer = await api.makeClip(projectNow, target, next); if (alive.current) setReport(old => ({ ...old, report: answer.report, notMade: null, error: null })); return answer; }, true); }
    finally { if (alive.current) setMaking(false); made(); }
  }
  async function listFile(path: string) {
    setFileError('');
    if (!isAnimationFile(path)) { setFileError(`${ONLY_ANIMATION_FILES}.`); return; }
    const projectNow = projectRef.current;
    // The saved file again: nothing is pending, the saved animation stays chosen.
    if (saved && fold((row?.source ?? '').trim()) === fold(path.trim())) { pendingFiles.delete(pendingKey); setPending(''); }
    else { pendingFiles.set(pendingKey, path); setPending(path); }
    const key = listKey(projectNow, path);
    setListing(true);
    try {
      await operate(LIST_FAILED, async () => {
        const answer = await api.madeClipActions(projectNow, path);
        actionLists.set(key, answer);
        if (alive.current) setList(answer);
        return answer;
      }, true);
    } finally { if (alive.current) setListing(false); }
  }
  async function browse() {
    let picked: string | string[] | null = null;
    try { picked = await openFor('animation-blend', { title: 'File with your animation', multiple: false, directory: false, filters: [{ name: 'Blender, FBX or glTF file', extensions: [...ANIMATION_FILE_SUFFIXES] }] }); }
    catch (error) { setFileError(`The file window could not be opened: ${messageOf(error)}`); return; }
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (path) await listFile(path);
  }
  /** Choosing an animation: ONE step writes the row, then the clip is made as an operation. */
  function chooseAction(action: ActionInfo) {
    if (disabled || !list || !file) return;
    const next = madeRow(target, file, action, list, row);
    const projectNow = write(next);
    if (!projectNow) return;
    pendingFiles.delete(pendingKey); setPending('');
    void makeFrom(projectNow, next);
  }
  /** One option: a step, then the report and the player ask again; the mode needs Blender. */
  function option(change: MadeChange) {
    if (disabled || !madeRowNow) return;
    const next = withMade(madeRowNow, change);
    if (JSON.stringify(next) === JSON.stringify(madeRowNow)) return;
    const projectNow = write(next);
    if (projectNow && change.mode && change.mode !== (saved?.mode ?? 'as_it_looks')) void makeFrom(projectNow, next);
  }

  const busy = disabled || listing || making;
  const model = project.preparation_source;
  const ownFile = isAnimationFile(model) && fold(model.trim()) !== fold(file.trim()) ? model : '';
  const intro = <p className="hint made-intro">Animate your prepared character in Blender (or in another program, saved as FBX or glTF), choose the file and the animation here, and the app makes this clip from it. Its length, events and root motion stay the game’s, so it always fits.</p>;

  if (!checked) return <div className="made-panel">{intro}<p className="hint" role="status">Checking whether this clip can be made from an animation…</p></div>;
  if (checked.error) return <div className="made-panel">{intro}<p className="hint" role="status">Whether this clip can be made from an animation could not be checked here: {checked.error}</p></div>;
  const refusal = checked.check?.refusal, fitError = checked.check?.fit_error;
  if (refusal) return <div className="made-panel">
    <div className="made-plain" role="note"><Lead text={refusal}/></div>
    <div className="made-actions"><button type="button" onClick={useCompiled}>Use a compiled clip file instead</button></div>
    {saved && <SavedRemove disabled={disabled} remove={remove}/>}
  </div>;
  if (fitError) return <div className="made-panel">
    <div className="made-plain" role="note">{fitError}</div>
    {openPreparation && <div className="made-actions"><button type="button" onClick={openPreparation}>Open Prepare model <span aria-hidden="true">→</span></button></div>}
    {saved && <SavedRemove disabled={disabled} remove={remove}/>}
  </div>;

  const usable = list ? list.actions.filter(a => a.driving_keyed_bones > 0) : [];
  const chosenAction = saved && !pending ? saved.action : '';
  const stretched = saved ? saved.part_start_seconds === undefined : true;
  const clipSeconds = shown?.clip_seconds ?? entry.duration;
  const actionSeconds = shown?.action_seconds ?? list?.actions.find(a => a.name === chosenAction)?.length_seconds ?? 0;
  const latest = latestPartStart(actionSeconds, clipSeconds);
  const start = saved?.part_start_seconds ?? 0;
  const blend = saved?.loop_blend_seconds;
  const blendMax = Math.max(MIN_LOOP_BLEND, Math.min(MAX_LOOP_BLEND, clipSeconds));
  const dragProps = { onPointerDown: () => drag(true), onPointerUp: () => drag(false), onPointerCancel: () => drag(false), onBlur: () => drag(false) };

  return <div className="made-panel">
    {intro}
    {row && !saved && <p className="hint">{row.taken ? 'Your clip taken from another hero' : 'Your compiled clip file'} stays in use until you choose an animation here.</p>}
    <div className="made-kv">
      <span className="made-key">Animation file</span>
      <div className="made-value">
        <div className="made-file" role="group" aria-label={`Animation file for ${entry.name}`}>
          {file
            ? <span className="clip-file-name mono" title={file}>{(([a, b]) => <><span className="clip-file-start">{a}</span><span className="clip-file-end">{b}</span></>)(fileNameParts(file))}</span>
            : <span className="hint">No file chosen yet.</span>}
          <span className="clip-file-actions">
            <button type="button" disabled={busy} onClick={() => void browse()}>{file ? 'Change file…' : 'Choose file…'}</button>
            {ownFile && <button type="button" disabled={busy} onClick={() => void listFile(ownFile)}>Use my model’s own file</button>}
            {saved && pending && <button type="button" className="tertiary" disabled={busy} onClick={() => { pendingFiles.delete(pendingKey); setPending(''); }}>Keep the saved file</button>}
            {saved && <button type="button" className="tertiary" disabled={disabled} onClick={remove}>Remove</button>}
          </span>
        </div>
        {saved && !pending && <p className="hint made-from">Made from your animation.</p>}
        {fileError && <p className="made-line attention" role="status">{fileError}</p>}
        {listing && <p className="hint" role="status">Reading the animations in the file with Blender…</p>}
        {list && !listing && (list.skeleton_error
          ? <p className="made-line attention" role="status">{list.skeleton_error}</p>
          : <p className="made-line ok"><span aria-hidden="true">✓ </span>The same skeleton as your prepared character ({list.bones} bones).</p>)}
      </div>
      {file && !list?.skeleton_error && <>
        <span className="made-key">Animation</span>
        <div className="made-value">
          {list
            ? usable.length
              ? <select className="made-select" aria-label="Animation" disabled={busy} value={list.actions.some(a => a.name === chosenAction) ? chosenAction : ''}
                onChange={event => { const action = list.actions.find(a => a.name === event.target.value); if (action) chooseAction(action); }}>
                {!list.actions.some(a => a.name === chosenAction) && <option value="" disabled>Choose an animation…</option>}
                {list.actions.map(a => <option key={a.name} value={a.name} disabled={a.driving_keyed_bones === 0}>{actionLabel(a)}{a.driving_keyed_bones === 0 ? ' (moves none of your character’s bones)' : ''}</option>)}
              </select>
              : <p className="made-line attention" role="status">This file has no animation that moves your character’s bones.</p>
            : !list && saved && !pending
              ? <div className="made-file"><span className="made-action-name">{saved.action}{shown ? ` · ${shown.action_seconds.toFixed(2)} s` : ''}</span><span className="clip-file-actions"><button type="button" disabled={busy} onClick={() => void listFile(file)}>List the file’s animations</button></span></div>
              : !list && !listing ? <div className="made-actions"><button type="button" disabled={busy} onClick={() => void listFile(file)}>List the file’s animations</button></div> : null}
        </div>
      </>}
    </div>
    {making && <p className="hint" role="status">Making the clip with Blender…</p>}
    {saved && !pending && !making && current?.notMade && <div className="made-line attention made-again" role="status">
      <span>{MAKE_AGAIN}</span>
      <button type="button" disabled={busy} onClick={() => { if (madeRowNow) void makeFrom(projectRef.current, madeRowNow); }}>Make it again</button>
    </div>}
    {saved && !pending && current?.error && <p className="made-line attention" role="status">{current.error}</p>}
    {saved && !pending && <>
      <div className="made-kv">
        <span className="made-key">Part that plays</span>
        <div className="made-value">
          <span className="studio-view-group made-seg" role="group" aria-label="Part that plays">
            <button type="button" aria-pressed={stretched} disabled={disabled} onClick={() => option({ part_start_seconds: null })}>All of it, fitted to {clipSeconds.toFixed(2)} s</button>
            <button type="button" aria-pressed={!stretched} disabled={disabled} onClick={() => option({ part_start_seconds: partStart(start, actionSeconds, clipSeconds) })}>A part at its own speed</button>
          </span>
          {!stretched && actionSeconds > 0 && <Strip start={start} latest={latest} actionSeconds={actionSeconds} clipSeconds={clipSeconds} disabled={disabled}
            change={value => option({ part_start_seconds: partStart(value, actionSeconds, clipSeconds) })} drag={dragProps}/>}
          {shown && <p className="hint">{speedWords(shown, stretched)}</p>}
        </div>
        <span className="made-key">Stay in place</span>
        <div className="made-value">
          <Switch label="Stay in place" checked={saved.in_place !== false} disabled={disabled} change={on => option({ in_place: on })}/>
          <p className="hint">{saved.in_place !== false ? 'Travel across the floor is removed: the game moves the hero itself. Crouching and hopping stay.' : 'Your animation’s travel across the floor stays in the clip.'}
            {saved.in_place !== false && shown && typeof shown.drift_removed_inches === 'number' && shown.drift_removed_inches > TRACE_INCHES ? ` ${shown.drift_removed_inches.toFixed(1)} in of travel was removed.` : ''}</p>
        </div>
        <span className="made-key">Loop</span>
        <div className="made-value">
          <Switch label="Loop" checked={blend !== undefined} disabled={disabled} change={on => option({ loop_blend_seconds: on ? loopBlend(DEFAULT_LOOP_BLEND, clipSeconds) : null })}/>
          {blend !== undefined && <label className="made-range">
            <span className="sr-only">Blend length</span>
            <input type="range" aria-label="Blend length" min={MIN_LOOP_BLEND} max={blendMax} step={0.05} value={Math.min(blend, blendMax)} disabled={disabled}
              onChange={event => option({ loop_blend_seconds: loopBlend(Number(event.target.value), clipSeconds) })} {...dragProps}/>
            <span className="mono made-range-value">{blend.toFixed(2)} s</span>
          </label>}
          <p className="hint">{blend !== undefined ? `The last ${blend.toFixed(2)} s blends into the start, so the loop has no jump.` : 'Off: the clip plays as your animation ends.'}{shown ? ` ${gapWords(shown)}` : ''}</p>
        </div>
      </div>
      <h4>How your pose is carried over</h4>
      <StudioOptionCards label="How your pose is carried over" name={`made-mode-${target}`} options={MODES} value={saved.mode ?? 'as_it_looks'} onChange={mode => option({ mode })} disabled={busy}/>
      <h4>Which bones follow your animation</h4>
      <StudioOptionCards label="Which bones follow your animation" name={`made-bones-${target}`} options={BONES} value={saved.bones ?? 'all'} onChange={bones => option({ bones })} disabled={disabled}/>
      <h4>What the app makes</h4>
      {shown ? <ul className="made-makes" aria-label="What the app makes">
        {makesLines(shown).map(line => <li key={line.id} className={line.mark}>
          <span className="made-mark" aria-hidden="true">{line.mark === 'note' ? '·' : '✓'}</span>
          <span className="made-text">{line.text}</span>
          {line.side ? <span className="made-side">{line.side}</span> : <span/>}
        </li>)}
      </ul> : current?.loading ? <p className="hint" role="status">Reading what the app makes…</p> : <p className="hint">Shown once the clip is made.</p>}
    </>}
  </div>;
}

/** The first sentence in bold, the rest plain. */
function Lead({ text }: { text: string }): ReactNode {
  const cut = text.indexOf('. ');
  return cut < 0 ? text : <><strong>{text.slice(0, cut + 1)}</strong> {text.slice(cut + 2)}</>;
}
function SavedRemove({ disabled, remove }: { disabled: boolean; remove: () => void }) {
  return <p className="hint made-saved">A clip made from your animation is saved for this clip. <button type="button" className="tertiary" disabled={disabled} onClick={remove}>Remove</button></p>;
}
function Switch({ label, checked, disabled, change }: { label: string; checked: boolean; disabled: boolean; change: (on: boolean) => void }) {
  return <label className="motion-switch made-switch"><input type="checkbox" role="switch" aria-label={label} checked={checked} disabled={disabled} onChange={event => change(event.target.checked)}/><span className="motion-switch-track" aria-hidden="true"/><span className="motion-switch-state" aria-hidden="true">{checked ? 'On' : 'Off'}</span></label>;
}
/** The strip of the animation's length with the part that plays on it; the start moves with the range under it. */
function Strip({ start, latest, actionSeconds, clipSeconds, disabled, change, drag }: {
  start: number; latest: number; actionSeconds: number; clipSeconds: number; disabled: boolean; change: (value: number) => void;
  drag: { onPointerDown: () => void; onPointerUp: () => void; onPointerCancel: () => void; onBlur: () => void };
}) {
  const total = Math.max(actionSeconds, clipSeconds, 0.001);
  const ticks = Math.max(0, Math.min(40, Math.floor(total * 4)));
  return <div className="made-strip-wrap">
    <div className="made-strip" aria-hidden="true">
      {Array.from({ length: ticks }, (_, i) => <i key={i} style={{ left: `${(i + 1) / (ticks + 1) * 100}%` }}/>)}
      <b style={{ left: `${Math.min(start, latest) / total * 100}%`, width: `${Math.min(clipSeconds, total) / total * 100}%` }}/>
    </div>
    {/* The thumb sits at the part's start on the strip's own scale; a start past the latest is held there. */}
    <input className="made-strip-range" type="range" aria-label="Where the part starts" min={0} max={total} step={0.01} value={Math.min(start, latest)} disabled={disabled || latest <= 0}
      onChange={event => change(Number(event.target.value))} {...drag}/>
    <p className="hint mono">Starts at {start.toFixed(2)} s of {actionSeconds.toFixed(2)} s{latest <= 0 ? ' (your animation is not longer than the clip)' : ''}</p>
  </div>;
}
