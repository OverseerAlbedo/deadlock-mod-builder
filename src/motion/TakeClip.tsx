import { useEffect, useRef, useState, type ReactNode } from 'react';
import { api } from '../api';
import type { Project, ResourceReplacement } from '../models';
import type { AnimationEntry } from '../components/AssetCatalog';
import { isMessage, messageOf } from './madeRows';
import { NewestQueue, isDropped } from './madeQueue';
import { latestTakenStart, takenOf, takenRow, takenSetRows, takenSetUnchanged, withTaken, type TakenChange } from './takenRows';
import { TAKEN_SUPERSEDED, type SourceClip, type SourceHero, type TakenClipCheck, type TakenReport, type TakenSources } from './takenTypes';
import { armsWords, clipSections, initials, setWords, takenLines, timingWords } from './takenWords';

/** What the page remembers for this session (never saved): the hero picked before an animation
 * of it is chosen, per project and clip, and the lists already read, so coming back to a clip asks
 * nothing again. */
const pendingHeroes = new Map<string, string>();
const heroLists = new Map<string, SourceHero[]>();
const sourceLists = new Map<string, TakenSources>();
const checks = new Map<string, TakenClipCheck>();
/** Forget the session's choices and lists (tests). */
export function forgetTakenState() { pendingHeroes.clear(); heroLists.clear(); sourceLists.clear(); checks.clear(); }
const fold = (value: string) => value.replace(/\\/g, '/').toLowerCase();

export const TAKEN_INTRO = (hero: string) => `Take the move from a hero already in the game. ${hero}’s length, events and travel stay the game’s, so it always fits.`;
export const SAME_MOVE_LINE = (hero: string) => `The same move as ${hero}’s: suggested first.`;
const TIMING = [['stretched', 'Fit it to this animation’s length'], ['own_speed', 'Play a part at its own speed']] as const;

type Load<T> = { key: string; value: T | null; error: string | null };
/** One read per key through its own queue (one request at a time, the newest waiting one wins);
 * kept for the session in `store`. Nothing is asked while `key` is empty. */
function useRead<T>(key: string, store: Map<string, T>, read: () => Promise<T>): Load<T> & { loading: boolean } {
  const queue = useRef(new NewestQueue());
  const [state, setState] = useState<Load<T>>(() => ({ key, value: key ? store.get(key) ?? null : null, error: null }));
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    if (!key) { queue.current.drop(); return; }
    const held = store.get(key);
    // A read still running for another key: its late answer must not land here.
    if (held) { queue.current.drop(); setState({ key, value: held, error: null }); return; }
    setState({ key, value: null, error: null });
    queue.current.ask(read).then(
      value => { store.set(key, value); if (alive.current) setState(old => (old.key === key ? { key, value, error: null } : old)); },
      error => { if (alive.current && !isDropped(error) && !isMessage(error, TAKEN_SUPERSEDED)) setState(old => (old.key === key ? { key, value: null, error: messageOf(error) } : old)); });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key names what is read
  }, [key]);
  const current = state.key === key ? state : { key, value: null, error: null };
  return { ...current, loading: Boolean(key) && !current.value && !current.error };
}

/** "Another hero's animation" in the clip's card: the hero, its animation, the timing, the arms and
 * hands, the set, and what the app makes (only from the engine's report). Opening this panel asks
 * whether the clip can be taken and the hero list (lease-free, no Blender); choosing a hero asks
 * for its clips and writes NOTHING; choosing an animation writes the row (one step); each later
 * option is one step; "Take all" writes every row of the set in one step. */
export function TakeClip({ project, entry, entries, heroName, row, disabled, write, writeRows, remove, useCompiled, fail, named }: {
  project: Project; entry: AnimationEntry; entries: AnimationEntry[]; heroName: string; row: ResourceReplacement | null; disabled: boolean;
  /** Write the clip's row as one step; returns the project written, or null when it could not be. */
  write: (row: ResourceReplacement) => Project | null;
  /** Write all animation rows as one step. */
  writeRows: (rows: ResourceReplacement[]) => void;
  remove: () => void; useCompiled: () => void; fail: (title: string, error: unknown) => void;
  /** Told the saved row's other hero's name once an answer holds it (the player's words). */
  named?: (name: string) => void;
}) {
  const target = entry.target;
  const projectRef = useRef(project); projectRef.current = project;
  const saved = takenOf(row);

  const check = useRead(`${project.original_model}|${fold(target)}`, checks, () => api.takenClipCheck(projectRef.current, target));
  const refusal = check.value?.refusal ?? null;
  const open = Boolean(check.value && !refusal);
  const heroes = useRead(open ? fold(project.original_model) : '', heroLists, () => api.takenClipHeroes(projectRef.current));
  const pendingKey = `${project.id}|${fold(target)}`;
  const [picked, setPending] = useState(() => pendingHeroes.get(pendingKey) ?? '');
  // A hero picked that the saved row now names (Take all, undo or redo wrote it) is no longer pending.
  const pending = picked && !(saved && fold(saved.hero) === fold(picked)) ? picked : '';
  const hero = pending || saved?.hero || '';
  const sources = useRead(open && hero ? `${project.original_model}|${fold(target)}|${fold(hero)}` : '', sourceLists, () => api.takenClipSources(projectRef.current, target, hero));
  const heroInfo = heroes.value?.find(h => fold(h.hero) === fold(hero)) ?? null;

  // The report of the saved row (no Blender, no lease), asked again only when `taken` changes.
  const reportQueue = useRef(new NewestQueue());
  const reportKey = saved ? `${fold(target)}|${JSON.stringify(saved)}` : '';
  const [report, setReport] = useState<{ key: string; value: TakenReport | null; error: string | null; loading: boolean }>({ key: '', value: null, error: null, loading: false });
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    if (!reportKey || !row) { reportQueue.current.drop(); return; }
    const key = reportKey, asked = row;
    setReport(old => ({ key, value: old.value, error: null, loading: true }));
    reportQueue.current.ask(() => api.takenClipReport(projectRef.current, target, asked)).then(
      value => { if (alive.current) setReport({ key, value, error: null, loading: false }); },
      error => { if (alive.current && !isDropped(error) && !isMessage(error, TAKEN_SUPERSEDED)) setReport(old => ({ key, value: old.value, error: messageOf(error), loading: false })); });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key holds the clip and `taken`
  }, [reportKey]);
  const shown = saved ? report.value : null;
  const current = report.key === reportKey ? report : null;

  const otherName = heroInfo?.name ?? shown?.source_name ?? '';
  const savedName = saved ? heroes.value?.find(h => fold(h.hero) === fold(saved.hero))?.name ?? shown?.source_name ?? '' : '';
  useEffect(() => { if (savedName) named?.(savedName); }, [savedName, named]);
  const chosenClip = saved && fold(saved.hero) === fold(hero) ? sources.value?.clips.find(c => fold(c.clip) === fold(saved.clip)) ?? null : null;

  function pickHero(info: SourceHero) {
    if (!info.usable) return;
    if (saved && fold(saved.hero) === fold(info.hero)) { pendingHeroes.delete(pendingKey); setPending(''); }
    else { pendingHeroes.set(pendingKey, info.hero); setPending(info.hero); }
  }
  function pickClip(clip: SourceClip) {
    if (disabled || !clip.usable || !hero) return;
    if (saved && fold(saved.hero) === fold(hero) && fold(saved.clip) === fold(clip.clip)) return;
    if (!write(takenRow(target, hero, clip.clip, row))) return;
    pendingHeroes.delete(pendingKey); setPending('');
  }
  function option(change: TakenChange) {
    if (disabled || !row || !saved) return;
    const next = withTaken(row, change);
    if (JSON.stringify(next) !== JSON.stringify(row)) write(next);
  }
  function takeAll() {
    const set = sources.value?.set;
    if (disabled || !set || !hero || !heroInfo?.usable) return;
    const rows = projectRef.current.customization.animations ?? [];
    const next = takenSetRows(rows, set, hero);
    const wanted = set.members.filter(m => m.usable && m.target && m.clip);
    const written = wanted.filter(m => next.some(r => fold(r.target ?? '') === fold(m.target!) && takenOf(r) && fold(takenOf(r)!.clip) === fold(m.clip!)));
    writeRows(next);
    if (written.length < wanted.length) fail('Too many clip replacements', `A project holds at most 128 clip replacements: ${wanted.length - written.length} of the ${wanted.length} could not be added.`);
  }

  const intro = <p className="hint made-intro">{TAKEN_INTRO(heroName)}</p>;
  if (check.error) return <div className="made-panel">{intro}<p className="hint" role="status">Whether this clip can be taken from another hero could not be checked here: {check.error}</p></div>;
  if (!check.value) return <div className="made-panel">{intro}<p className="hint" role="status">Checking whether this clip can be taken from another hero…</p></div>;
  if (refusal) return <div className="made-panel">
    <div className="made-plain" role="note"><Lead text={refusal}/></div>
    <div className="made-actions"><button type="button" onClick={useCompiled}>Use a compiled clip file instead</button></div>
    {saved && <p className="hint made-saved">A clip taken from another hero is saved for this clip. <button type="button" className="tertiary" disabled={disabled} onClick={remove}>Remove</button></p>}
  </div>;

  const stretched = saved?.timing !== 'own_speed';
  const set = sources.value?.set;
  const setUsable = set ? set.members.filter(m => m.usable && m.target && m.clip).length : 0;
  const setText = set ? setWords(set, entries) : null;
  // "Take all" is offered only while the click would change something.
  const setTaken = Boolean(set && hero && takenSetUnchanged(project.customization.animations ?? [], set, hero));
  return <div className="made-panel taken-panel">
    {intro}
    {row && !saved && <p className="hint">{row.made ? 'Your clip made from your animation' : 'Your compiled clip file'} stays in use until you choose an animation here.</p>}
    <div className="made-kv">
      <span className="made-key" id={`taken-hero-${target}`}>Hero</span>
      <div className="made-value">
        <Picker label="Hero" disabled={disabled} empty={!hero} button={hero
          ? <><Mark name={otherName || hero}/><span className="taken-picker-text"><strong>{otherName || hero}</strong></span></>
          : <span className="taken-picker-text muted">{heroes.loading ? 'Loading the heroes…' : 'Choose a hero…'}</span>}>
          {(query, close) => heroes.error ? <p className="hint" role="status">The heroes could not be read: {heroes.error}</p>
            : !heroes.value ? <p className="hint" role="status">Loading the heroes…</p>
              : <ul className="taken-list" role="listbox" aria-label="Heroes">
                {heroes.value.filter(h => h.name.toLowerCase().includes(query.toLowerCase().trim())).map(h =>
                  <Option key={h.hero} selected={fold(h.hero) === fold(hero)} disabled={!h.usable} pick={() => { pickHero(h); close(); }} label={h.name}>
                    <Mark name={h.name}/>
                    <span className="taken-option-text"><strong>{h.name}</strong>{h.reason && <small>{h.reason}</small>}</span>
                  </Option>)}
              </ul>}
        </Picker>
        {heroInfo && !heroInfo.usable && <p className="made-line attention" role="status">{heroInfo.reason}</p>}
      </div>
      {hero && <>
        <span className="made-key">Animation</span>
        <div className="made-value">
          {sources.error ? <p className="made-line attention" role="status">{sources.error}</p>
            : <Picker label="Animation" disabled={disabled || !sources.value} empty={!chosenClip} search={`Search ${otherName ? `${otherName}’s` : 'the hero’s'} animations…`} button={chosenClip
              ? <><span className="taken-picker-text"><strong>{chosenClip.name}</strong><span className="muted"> · {chosenClip.seconds.toFixed(2)} s · {chosenClip.frames} {chosenClip.frames === 1 ? 'frame' : 'frames'}</span></span>{chosenClip.rank === 0 && <SameTag/>}</>
              : <span className="taken-picker-text muted">{sources.loading ? `Loading ${otherName ? `${otherName}’s` : 'the hero’s'} animations…` : 'Choose an animation…'}</span>}>
              {(query, close) => !sources.value ? <p className="hint" role="status">Loading the animations…</p>
                : <ul className="taken-list" role="listbox" aria-label={`${otherName || 'The hero'}’s animations`}>
                  {clipSections(sources.value.clips, query).map(section => <li key={section.title} role="presentation" className="taken-section">
                    <span className="taken-sep" aria-hidden="true">{section.title}</span>
                    <ul role="group" aria-label={section.title}>
                      {section.clips.map(c => <Option key={c.clip} selected={Boolean(chosenClip && fold(chosenClip.clip) === fold(c.clip))} disabled={!c.usable || disabled} pick={() => { pickClip(c); close(); }} label={c.name}>
                        <span className="taken-option-text">
                          <span className="taken-option-head"><strong>{c.name}</strong>{c.rank === 0 && c.usable && <SameTag/>}</span>
                          <small className="mono">{c.label} · {c.seconds.toFixed(2)} s · {c.frames} {c.frames === 1 ? 'frame' : 'frames'}</small>
                          {c.reason && <small className="taken-reason">{c.reason}</small>}
                        </span>
                      </Option>)}
                    </ul>
                  </li>)}
                </ul>}
            </Picker>}
          {chosenClip?.rank === 0 && <p className="made-line ok"><span aria-hidden="true">✓ </span>{SAME_MOVE_LINE(heroName)}</p>}
          {pending && saved && <p className="hint">Your saved clip from {shown?.source_name ?? 'the other hero'} stays in use until you choose an animation of {otherName || 'this hero'}. <button type="button" className="tertiary" onClick={() => { pendingHeroes.delete(pendingKey); setPending(''); }}>Keep the saved hero</button></p>}
        </div>
      </>}
      {saved && !pending && <>
        <span className="made-key">Timing</span>
        <div className="made-value">
          <span className="studio-view-group made-seg" role="group" aria-label="Timing">
            {TIMING.map(([value, label]) => <button key={value} type="button" aria-pressed={(saved.timing ?? 'stretched') === value} disabled={disabled}
              onClick={() => option(value === 'stretched' ? { timing: 'stretched', start_seconds: null } : { timing: 'own_speed' })}>{label}</button>)}
          </span>
          {!stretched && <StartField key={`${saved.start_seconds ?? 0}`} value={saved.start_seconds ?? 0} latest={shown ? latestTakenStart(shown.made.action_seconds, shown.made.clip_seconds) : null} disabled={disabled}
            commit={value => option({ start_seconds: value })}/>}
          {shown && <p className="hint">{timingWords(shown, heroName, stretched)}</p>}
        </div>
        <span className="made-key">Arms and hands</span>
        <div className="made-value">
          <span className="studio-view-group made-seg" role="group" aria-label="Arms and hands">
            <button type="button" aria-pressed={(saved.arms ?? 'source') === 'source'} disabled={disabled} onClick={() => option({ arms: 'source' })}>From {otherName || 'the other hero'}</button>
            <button type="button" aria-pressed={saved.arms === 'own'} disabled={disabled} onClick={() => option({ arms: 'own' })}>Keep {heroName}’s own</button>
          </span>
          <p className="hint">{armsWords(otherName || 'The other hero', heroName)}</p>
        </div>
      </>}
    </div>
    {set && setText && heroInfo?.usable && <div className="made-plain taken-set">
      <span><strong>{setText.lead}</strong> ({setText.members}).{setUsable < set.members.length ? ` ${setUsable} of them can be taken from ${otherName}.` : ''}</span>
      {setUsable > 0 && (setTaken
        ? <span className="hint">All {setUsable} are taken from {otherName}.</span>
        : <button type="button" disabled={disabled} onClick={takeAll}>Take all {setUsable} from {otherName}</button>)}
    </div>}
    {saved && !pending && <>
      {current?.error && <p className="made-line attention" role="status">{current.error}</p>}
      <h4>What you get</h4>
      {shown ? <ul className="made-makes" aria-label="What you get">
        {takenLines(shown, heroName, stretched).map(line => <li key={line.id} className={line.mark}>
          <span className="made-mark" aria-hidden="true">{line.mark === 'note' ? '·' : '✓'}</span>
          <span className="made-text">{line.text}</span>
          {line.side ? <span className="made-side">{line.side}</span> : <span/>}
        </li>)}
      </ul> : current?.loading ? <p className="hint" role="status">Reading what the app makes…</p> : null}
      <p className="hint made-saved">Taken from {shown?.source_name ?? otherName}. <button type="button" className="tertiary" disabled={disabled} onClick={remove}>Remove</button></p>
    </>}
  </div>;
}

/** The first sentence in bold, the rest plain. */
function Lead({ text }: { text: string }): ReactNode {
  const cut = text.indexOf('. ');
  return cut < 0 ? text : <><strong>{text.slice(0, cut + 1)}</strong> {text.slice(cut + 2)}</>;
}
const SameTag = () => <span className="taken-tag">same move</span>;
const Mark = ({ name }: { name: string }) => <span className="taken-mark" aria-hidden="true">{initials(name)}</span>;

/** A searchable drop-down: the button shows the choice; the list opens under it, with a search
 * field, and closes on a choice, Escape, the button or a click outside. Opening it writes nothing. */
function Picker({ label, disabled, empty, button, search = `Search ${label.toLowerCase() === 'hero' ? 'heroes' : 'animations'}…`, children }: {
  label: string; disabled: boolean; empty: boolean; button: ReactNode; search?: string;
  children: (query: string, close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false), [query, setQuery] = useState('');
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const outside = (event: MouseEvent) => { if (box.current && !box.current.contains(event.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [open]);
  const close = () => { setOpen(false); setQuery(''); };
  return <div className="taken-anchor" ref={box} onKeyDown={event => { if (event.key === 'Escape' && open) { event.stopPropagation(); close(); } }}>
    <button type="button" className={`taken-picker${empty ? ' empty' : ''}`} aria-label={label} aria-haspopup="listbox" aria-expanded={open} disabled={disabled && !open} onClick={() => (open ? close() : setOpen(true))}>
      {button}<span className="taken-caret" aria-hidden="true">▾</span>
    </button>
    {open && <div className="taken-pop" role="dialog" aria-label={`Choose: ${label}`}>
      <input className="taken-search" type="search" aria-label={search} placeholder={search} value={query} autoFocus onChange={event => setQuery(event.target.value)}/>
      {children(query, close)}
    </div>}
  </div>;
}
function Option({ selected, disabled, pick, label, children }: { selected: boolean; disabled: boolean; pick: () => void; label: string; children: ReactNode }) {
  const choose = () => { if (!disabled) pick(); };
  return <li role="option" aria-label={label} aria-selected={selected} aria-disabled={disabled} tabIndex={disabled ? -1 : 0} className={`taken-option${selected ? ' selected' : ''}${disabled ? ' refused' : ''}`}
    onClick={choose} onKeyDown={event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); choose(); } }}>{children}</li>;
}
/** The start of the part that plays, in seconds: written once, on Enter or when the field is left. */
function StartField({ value, latest, disabled, commit }: { value: number; latest: number | null; disabled: boolean; commit: (value: number) => void }) {
  const [draft, setDraft] = useState(value.toFixed(2));
  const done = () => {
    const number = Number(draft);
    if (!Number.isFinite(number) || draft.trim() === '') { setDraft(value.toFixed(2)); return; }
    const held = Math.max(0, latest === null ? number : Math.min(latest, number));
    if (Math.abs(held - value) < 0.0005) { setDraft(value.toFixed(2)); return; }
    commit(held);
  };
  return <label className="taken-start">
    <span>Starts at</span>
    <input type="number" aria-label="Where the part starts (seconds)" min={0} max={latest ?? undefined} step={0.01} value={draft} disabled={disabled}
      onChange={event => setDraft(event.target.value)} onBlur={done} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); done(); } }}/>
    <span className="muted">s{latest !== null ? ` (0 to ${latest.toFixed(2)} s)` : ''}</span>
  </label>;
}
