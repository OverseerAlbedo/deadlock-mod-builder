import { useMemo, useRef, useState, type RefObject } from 'react';
import { ask } from '@tauri-apps/plugin-dialog';
import { api } from '../api';
import type { InstalledMod, ModUpdateReport } from '../models';
import { Section, StateLabel } from '../components/Primitives';
import {
  contentsOf, contentsText, fromText, heroNames, heroText, hasItems, isOther, kindName, KIND_ORDER, matchesHero, matchesMade, matchesSearch,
  megabytes, readableStem, sharedText, stateOf, summary, type HeroFilter, type MadeFilter,
} from './installedModsText';
import './installedMods.css';

function describe(error: unknown): string {
  if (error && typeof error === 'object' && 'message' in error) return String((error as {message: unknown}).message);
  return String(error);
}
const sameFilter = (a: HeroFilter, b: HeroFilter) => a.kind === b.kind && (a.kind !== 'hero' || b.kind === 'hero' && a.name === b.name);

/** Installed mods: every VPK in the game's addons folder, named by what is inside it (heroes and
 * kinds of change read from the paths in its directory), whether this app made it, files that
 * change the same game files, a label of the user's own kept against the file's contents, and
 * the in-place update for hero models compiled before the game's October 16, 2026 attachment
 * change (camera pivots rotated by 90 degrees in a match). Nothing is read before Check is
 * pressed: the scan takes the operation lease. */
export function InstalledMods({busy = false, onOpenProject}: {busy?: boolean; onOpenProject?: (path: string) => void}) {
  const [mods, setMods] = useState<InstalledMod[] | null>(null);
  const [working, setWorking] = useState<'scan' | 'update' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reports, setReports] = useState<ModUpdateReport[]>([]);
  const [query, setQuery] = useState('');
  const [hero, setHero] = useState<HeroFilter>({kind: 'all'});
  const [made, setMade] = useState<MadeFilter>('all');
  const [open, setOpen] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [labelState, setLabelState] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle');
  const detailsRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLInputElement>(null);
  const openRef = useRef<string | null>(null);
  const affected = (mods ?? []).filter(mod => mod.status === 'needs_update');
  const disabled = busy || working !== null;

  async function scan() {
    setWorking('scan'); setError(null); setReports([]);
    try { setMods(await api.scanInstalledMods()); } catch (cause) { setError(describe(cause)); } finally { setWorking(null); }
  }
  async function update(targets: InstalledMod[]) {
    const names = targets.map(mod => mod.file);
    let confirmed = true;
    try {
      confirmed = await ask(`Update ${names.length === 1 ? names[0] : `${names.length} mods`} in the game's addons folder? The hero models inside take the game's current attachment values; the previous files are kept as backups.`, {title: 'Update installed mods', kind: 'warning'});
    } catch { /* no dialog outside the desktop app */ }
    if (!confirmed) return;
    setWorking('update'); setError(null);
    try {
      const done = await api.updateInstalledMods(targets.map(mod => mod.path));
      setReports(done);
      setMods(await api.scanInstalledMods());
    } catch (cause) { setError(describe(cause)); } finally { setWorking(null); }
  }
  async function reveal(path?: string) {
    setError(null);
    try { await api.showInstalledMod(path); } catch (cause) { setError(describe(cause)); }
  }
  async function revealProject(path: string) {
    setError(null);
    try { await api.showProjectFile(path); } catch (cause) { setError(describe(cause)); }
  }
  function toggle(mod: InstalledMod, focusLabel = false) {
    const next = open === mod.path && !focusLabel ? null : mod.path;
    openRef.current = next;
    setOpen(next); setDraft(mod.label ?? ''); setLabelState('idle');
    if (next) requestAnimationFrame(() => {
      detailsRef.current?.scrollIntoView?.({block: 'nearest'});
      if (focusLabel) labelRef.current?.focus();
    });
  }
  async function saveLabel(mod: InstalledMod) {
    if (!mod.identity || (mod.label ?? '') === draft.trim()) return;
    setLabelState('saving');
    try {
      const kept = await api.setInstalledModLabel(mod.identity, mod.file, draft);
      // Every file with the same contents shares the label.
      setMods(current => current?.map(row => row.identity === mod.identity ? {...row, label: kept} : row) ?? null);
      // Another file may have been opened meanwhile: its input is left alone.
      if (openRef.current === mod.path) { setDraft(kept ?? ''); setLabelState('saved'); }
    } catch (cause) {
      if (openRef.current === mod.path) setLabelState('error');
      setError(describe(cause));
    }
  }

  const heroes = useMemo(() => {
    const counts = new Map<string, number>();
    for (const mod of mods ?? []) for (const name of heroNames(mod)) counts.set(name, (counts.get(name) ?? 0) + 1);
    return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  }, [mods]);
  const shown = (mods ?? []).filter(mod => matchesHero(mod, hero) && matchesMade(mod, made) && matchesSearch(mod, query));
  const selected = (mods ?? []).find(mod => mod.path === open) ?? null;
  const total = (mods ?? []).reduce((sum, mod) => sum + mod.bytes, 0);
  const overlapping = (mods ?? []).filter(mod => (mod.shared ?? []).length > 0).length;
  const chip = (filter: HeroFilter, label: string, n: number) =>
    <button type="button" key={label} className="installed-chip" aria-pressed={sameFilter(hero, filter)} onClick={() => setHero(filter)}>{label} <small>{n}</small></button>;

  return <Section title="Installed mods" id="installed-mods" className="installed-section" actions={<div className="row">
      <button disabled={disabled} onClick={() => void scan()}>{working === 'scan' ? 'Checking…' : mods ? 'Scan again' : 'Check installed mods'}</button>
      <button type="button" onClick={() => void reveal()}>Open the addons folder</button>
      {affected.length > 0 && <button className="primary" disabled={disabled} onClick={() => void update(affected)}>{working === 'update' ? 'Updating…' : `Update ${affected.length} ${affected.length === 1 ? 'mod' : 'mods'}`}</button>}
    </div>}>
    <p className="hint">Every VPK in the game's addons folder, named by what is inside it: the heroes and the kind of change are read from the paths in each file, so the list stays right when a mod loader renames or renumbers files. Mods built before the game update of October 16, 2026 show the camera 90 degrees off in a match; those can be updated in place here, whichever tool made them, and the previous files are kept as backups.</p>
    {error && <p className="error-text" role="alert">{error}</p>}
    {reports.length > 0 && <p role="status" className="ok-text">Updated {reports.map(report => `${report.file} (${report.updated.length} ${report.updated.length === 1 ? 'model' : 'models'})`).join(', ')}. Start the game and check the camera in a match; the previous files are in the app's backups folder.</p>}
    {mods && mods.length === 0 && <p role="status">No mods are installed in the game's addons folder.</p>}
    {mods && mods.length > 0 && <div className="installed-mods">
      <p className="installed-summary"><span className="installed-count">{mods.length} {mods.length === 1 ? 'file' : 'files'} · {megabytes(total)}</span> Read from the files themselves just now{affected.length > 0 && ` · ${affected.length} ${affected.length === 1 ? 'needs' : 'need'} an update`}{overlapping > 0 && ` · ${overlapping} change the same game files`}</p>
      <div className="installed-filters">
        <input type="search" className="installed-search" aria-label="Search hero, file or label" placeholder="Search hero, file or label…" value={query} onChange={event => setQuery(event.target.value)}/>
        <div className="installed-chips" role="group" aria-label="Hero">
          {chip({kind: 'all'}, 'All', mods.length)}
          {heroes.map(([name, n]) => chip({kind: 'hero', name}, name, n))}
          {mods.some(hasItems) && chip({kind: 'items'}, 'Items', mods.filter(hasItems).length)}
          {mods.some(isOther) && chip({kind: 'other'}, 'Other', mods.filter(isOther).length)}
        </div>
        <div className="installed-seg" role="group" aria-label="Made by">
          {([['all', 'All'], ['here', 'Made here'], ['elsewhere', 'From elsewhere']] as const).map(([value, label]) =>
            <button type="button" key={value} aria-pressed={made === value} onClick={() => setMade(value)}>{label}</button>)}
        </div>
      </div>
      <div className="installed-head" aria-hidden="true"><span>File</span><span>Hero and your label</span><span>What it changes</span><span>State</span><span/></div>
      {shown.length === 0 && <p role="status" className="muted">No installed file matches these filters.</p>}
      <ul className="installed-list" aria-label="Installed mods">
        {shown.map(mod => {
          const state = stateOf(mod), shared = sharedText(mod), isOpen = open === mod.path;
          return <li key={mod.path} className={`installed-row${isOpen ? ' open' : ''}`}>
            <div className="installed-file"><b className="mono">{mod.file}</b><small>{megabytes(mod.bytes)}</small></div>
            <div className="installed-hero"><b>{heroText(mod)}</b>
              {mod.identity ? <button type="button" className="installed-label-link" aria-label={mod.label ? `Label of ${mod.file}: ${mod.label}. Edit` : `Add a label to ${mod.file}`} onClick={() => toggle(mod, true)}>
                {mod.label ? <>Label: <span className="installed-label-text">{mod.label}</span></> : 'Add a label'} <span aria-hidden="true">✎</span>
              </button> : <small>No label without a readable file</small>}
            </div>
            <div className="installed-what"><span>{summary(mod)}</span><small>{fromText(mod)}</small>{shared && <small className="installed-shared">{shared}</small>}</div>
            <div className="installed-state"><StateLabel tone={state.tone}>{state.label}</StateLabel></div>
            <div className="installed-actions">
              {mod.status === 'needs_update' && <button type="button" className="primary" disabled={disabled} onClick={() => void update([mod])} aria-label={`Update ${mod.file}`}>Update</button>}
              <button type="button" className="installed-details-button" aria-expanded={isOpen} aria-controls="installed-details" onClick={() => toggle(mod)}>Details <span aria-hidden="true">{isOpen ? '▴' : '▾'}</span></button>
            </div>
          </li>;
        })}
      </ul>
      {selected && <Details mod={selected} refs={{details: detailsRef, label: labelRef}} draft={draft} setDraft={value => { setDraft(value); setLabelState('idle'); }}
        labelState={labelState} save={() => void saveLabel(selected)} reset={() => { setDraft(selected.label ?? ''); setLabelState('idle'); }}
        reveal={() => void reveal(selected.path)} revealProject={path => void revealProject(path)} onOpenProject={onOpenProject} close={() => { openRef.current = null; setOpen(null); }}/>}
    </div>}
  </Section>;
}

function Details({mod, refs, draft, setDraft, labelState, save, reset, reveal, revealProject, onOpenProject, close}: {
  mod: InstalledMod; refs: {details: RefObject<HTMLDivElement | null>; label: RefObject<HTMLInputElement | null>};
  draft: string; setDraft: (value: string) => void; labelState: 'idle' | 'saving' | 'saved' | 'error';
  save: () => void; reset: () => void; reveal: () => void; revealProject: (path: string) => void; onOpenProject?: (path: string) => void; close: () => void;
}) {
  const c = contentsOf(mod), made = mod.made, file = made?.project_file ?? null;
  const heroLine = c.heroes.length ? c.heroes.map(h => `${h.name} (${h.files} ${h.files === 1 ? 'file' : 'files'})`).join(', ') : heroText(mod);
  return <div className="installed-details" id="installed-details" ref={refs.details} role="region" aria-label={`Details of ${mod.file}`}>
    <div className="installed-details-head">
      <h3 className="mono">{mod.file}</h3>
      {made ? <span className="installed-badge">Made here</span> : <span className="installed-badge quiet">From elsewhere</span>}
      <span className="installed-spacer"/>
      {file && onOpenProject && <button type="button" onClick={() => onOpenProject(file)}>Open its project</button>}
      {file && <button type="button" onClick={() => revealProject(file)}>Show its project file</button>}
      <button type="button" onClick={reveal}>Show in folder</button>
      <button type="button" className="ghost" onClick={close}>Close</button>
    </div>
    <dl className="installed-kv">
      <dt>Hero</dt><dd>{heroLine}{c.every_hero > 0 && c.heroes.length > 0 && ` · ${c.every_hero} ${c.every_hero === 1 ? 'file' : 'files'} for every hero`}{c.unattributed > 0 && c.heroes.length > 0 && ` · ${c.unattributed} without a hero`}</dd>
      <dt>Contents</dt><dd>{mod.unreadable ? `Not readable as a VPK: ${mod.unreadable}` : contentsText(mod)}</dd>
      {c.ability_objects.length > 0 && <><dt>Ability objects</dt><dd>{c.ability_objects.map(o => `${readableStem(o.path)} (${o.every_hero ? 'every hero' : o.heroes.length ? o.heroes.join(', ') : 'no hero the game data names'})`).join('; ')}</dd></>}
      <dt>Built</dt><dd>{made?.several
        ? 'by this app: VPKs in the output folders of several projects hold these same files, so no one project is named'
        : made
        ? `by this app from the project ${made.project_name || 'whose name is no longer recorded'}${made.hero ? ` for ${made.hero}` : ''}${file ? '' : '; the project is not on this PC'}${made.source === 'output' ? ' (the same files as a VPK in that project\'s output folder)' : ''}`
        : 'Not made by this app, or installed by it before it kept a record of installs'}</dd>
      <dt>State</dt><dd>{stateOf(mod).label}{mod.note && ` · ${mod.note}`}</dd>
      <dt><label htmlFor="installed-label-input">Your label</label></dt><dd className="installed-label-field">
        {mod.identity ? <>
          <input id="installed-label-input" ref={refs.label} value={draft} maxLength={80} placeholder="For example: main skin" onChange={event => setDraft(event.target.value)}
            onBlur={save} onKeyDown={event => { if (event.key === 'Enter') save(); if (event.key === 'Escape') reset(); }} aria-describedby="installed-label-note"/>
          <span id="installed-label-note" className="installed-note" role={labelState === 'saved' ? 'status' : undefined}>{labelState === 'saving' ? 'Saving…' : labelState === 'saved' ? 'Saved. ' : ''}Kept with the file's contents, so it stays when the file is renumbered or renamed.</span>
        </> : <span className="installed-note">A file that cannot be read cannot be labelled.</span>}
      </dd>
      {(mod.shared ?? []).length > 0 && <><dt>Same game files</dt><dd>
        <p className="installed-note">These files are also in other VPKs, with other contents. Which version a match uses is not decided here.</p>
        {(mod.shared ?? []).map(s => <details key={s.path} className="installed-paths"><summary>{s.count} {s.count === 1 ? 'file' : 'files'} also in {s.file}</summary>
          <ul>{s.paths.map(p => <li key={p} className="mono">{p}</li>)}{s.count > s.paths.length && <li className="muted">and {s.count - s.paths.length} more</li>}</ul></details>)}
      </dd></>}
      {!mod.unreadable && c.files > 0 && <><dt>Files by kind</dt><dd>
        {KIND_ORDER.filter(k => (c.kinds[k] ?? 0) > 0).map(k => <details key={k} className="installed-paths"><summary>{kindName(k, c.kinds[k] ?? 0)}</summary>
          <ul>{(c.samples[k] ?? []).map(p => <li key={p} className="mono">{p}</li>)}{(c.kinds[k] ?? 0) > (c.samples[k] ?? []).length && <li className="muted">and {(c.kinds[k] ?? 0) - (c.samples[k] ?? []).length} more</li>}</ul></details>)}
      </dd></>}
    </dl>
  </div>;
}
