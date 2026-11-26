// i18n-area: build
import { useMemo, useRef, useState, type RefObject } from 'react';
import { ask } from '@tauri-apps/plugin-dialog';
import { api } from '../api';
import type { InstalledMod, ModUpdateReport } from '../models';
import { Section, StateLabel } from '../components/Primitives';
import {
  contentsOf, contentsText, fromText, heroNames, heroText, hasItems, isOther, kindName, KIND_ORDER, matchesHero, matchesMade, matchesSearch,
  megabytes, readableStem, sharedText, stateOf, summary, type HeroFilter, type MadeFilter,
} from './installedModsText';
import { tc, tp, useT } from '../i18n';
import { T } from '../i18n/T';
import { shownHero, useGameNames } from '../i18n/gameNames';
import { nameList, sentences } from './words';
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
  const t = useT();
  useGameNames();
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
      confirmed = await ask(names.length === 1
        ? t("Update {file} in the game's addons folder? The hero models inside take the game's current attachment values; the previous files are kept as backups.", {file: names[0]})
        : tp(names.length, {one: "Update {count} mod in the game's addons folder? The hero models inside take the game's current attachment values; the previous files are kept as backups.", other: "Update {count} mods in the game's addons folder? The hero models inside take the game's current attachment values; the previous files are kept as backups."}), {title: t('Update installed mods'), kind: 'warning'});
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
  // The chip's key is the filter, never its shown label.
  const chip = (filter: HeroFilter, label: string, n: number) =>
    <button type="button" key={filter.kind === 'hero' ? `hero:${filter.name}` : filter.kind} className="installed-chip" aria-pressed={sameFilter(hero, filter)} onClick={() => setHero(filter)}>{label} <small>{n}</small></button>;
  const madeChoices = [['all', tc('filter', 'All')], ['here', t('Made here')], ['elsewhere', t('From elsewhere')]] as const;

  return <Section title={t('Installed mods')} id="installed-mods" className="installed-section" actions={<div className="row">
      <button disabled={disabled} onClick={() => void scan()}>{working === 'scan' ? t('Checking…') : mods ? t('Scan again') : t('Check installed mods')}</button>
      <button type="button" onClick={() => void reveal()}>{t('Open the addons folder')}</button>
      {affected.length > 0 && <button className="primary" disabled={disabled} onClick={() => void update(affected)}>{working === 'update' ? t('Updating…') : tp(affected.length, {one: 'Update {count} mod', other: 'Update {count} mods'})}</button>}
    </div>}>
    <p className="hint">{t("Every VPK in the game's addons folder, named by what is inside it: the heroes and the kind of change are read from the paths in each file, so the list stays right when a mod loader renames or renumbers files. Mods built before the game update of October 16, 2026 show the camera 90 degrees off in a match; those can be updated in place here, whichever tool made them, and the previous files are kept as backups.")}</p>
    {error && <p className="error-text" role="alert">{error}</p>}
    {reports.length > 0 && <p role="status" className="ok-text">{t("Updated {list}. Start the game and check the camera in a match; the previous files are in the app's backups folder.", {list: nameList(reports.map(report => tp(report.updated.length, {one: '{file} ({count} model)', other: '{file} ({count} models)'}, {file: report.file})))})}</p>}
    {mods && mods.length === 0 && <p role="status">{t("No mods are installed in the game's addons folder.")}</p>}
    {mods && mods.length > 0 && <div className="installed-mods">
      <p className="installed-summary"><span className="installed-count">{tp(mods.length, {one: '{count} file · {size}', other: '{count} files · {size}'}, {size: megabytes(total)})}</span> {t('Read from the files themselves just now')}{affected.length > 0 && ` · ${tp(affected.length, {one: '{count} needs an update', other: '{count} need an update'})}`}{overlapping > 0 && ` · ${tp(overlapping, {one: '{count} change the same game files', other: '{count} change the same game files'})}`}</p>
      <div className="installed-filters">
        <input type="search" className="installed-search" aria-label={t('Search hero, file or label')} placeholder={t('Search hero, file or label…')} value={query} onChange={event => setQuery(event.target.value)}/>
        <div className="installed-chips" role="group" aria-label={t('Hero')}>
          {chip({kind: 'all'}, tc('filter', 'All'), mods.length)}
          {heroes.map(([name, n]) => chip({kind: 'hero', name}, shownHero(name), n))}
          {mods.some(hasItems) && chip({kind: 'items'}, tc('filter', 'Items'), mods.filter(hasItems).length)}
          {mods.some(isOther) && chip({kind: 'other'}, tc('filter', 'Other'), mods.filter(isOther).length)}
        </div>
        <div className="installed-seg" role="group" aria-label={t('Made by')}>
          {madeChoices.map(([value, label]) =>
            <button type="button" key={value} aria-pressed={made === value} onClick={() => setMade(value)}>{label}</button>)}
        </div>
      </div>
      <div className="installed-head" aria-hidden="true"><span>{t('File')}</span><span>{t('Hero and your label')}</span><span>{t('What it changes')}</span><span>{tc('column', 'State')}</span><span/></div>
      {shown.length === 0 && <p role="status" className="muted">{t('No installed file matches these filters.')}</p>}
      <ul className="installed-list" aria-label={t('Installed mods')}>
        {shown.map(mod => {
          const state = stateOf(mod), shared = sharedText(mod), isOpen = open === mod.path;
          return <li key={mod.path} className={`installed-row${isOpen ? ' open' : ''}`}>
            <div className="installed-file"><b className="mono">{mod.file}</b><small>{megabytes(mod.bytes)}</small></div>
            <div className="installed-hero"><b>{heroText(mod)}</b>
              {mod.identity ? <button type="button" className="installed-label-link" aria-label={mod.label ? t('Label of {file}: {label}. Edit', {file: mod.file, label: mod.label}) : t('Add a label to {file}', {file: mod.file})} onClick={() => toggle(mod, true)}>
                {mod.label ? <T text="Label: <name>{label}</name>" tags={{name: chunk => <span className="installed-label-text">{chunk}</span>}} values={{label: mod.label}}/> : t('Add a label')} <span aria-hidden="true">✎</span>
              </button> : <small>{t('No label without a readable file')}</small>}
            </div>
            <div className="installed-what"><span>{summary(mod)}</span><small>{fromText(mod)}</small>{shared && <small className="installed-shared">{shared}</small>}</div>
            <div className="installed-state"><StateLabel tone={state.tone}>{state.label}</StateLabel></div>
            <div className="installed-actions">
              {mod.status === 'needs_update' && <button type="button" className="primary" disabled={disabled} onClick={() => void update([mod])} aria-label={t('Update {file}', {file: mod.file})}>{t('Update')}</button>}
              <button type="button" className="installed-details-button" aria-expanded={isOpen} aria-controls="installed-details" onClick={() => toggle(mod)}>{t('Details')} <span aria-hidden="true">{isOpen ? '▴' : '▾'}</span></button>
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
  const t = useT();
  useGameNames();
  const c = contentsOf(mod), made = mod.made, file = made?.project_file ?? null;
  const heroLine = c.heroes.length ? nameList(c.heroes.map(h => tp(h.files, {one: '{name} ({count} file)', other: '{name} ({count} files)'}, {name: shownHero(h.name)}))) : heroText(mod);
  const more = (n: number) => tp(n, {one: 'and {count} more', other: 'and {count} more'});
  // Who made it: the project, the hero it was built for, then what is not known or certain.
  const builtBy = !made || made.several ? '' : made.project_name
    ? made.hero ? t('by this app from the project {name} for {hero}', {name: made.project_name, hero: shownHero(made.hero)}) : t('by this app from the project {name}', {name: made.project_name})
    : made.hero ? t('by this app from the project whose name is no longer recorded for {hero}', {hero: shownHero(made.hero)}) : t('by this app from the project whose name is no longer recorded');
  const notHere = file ? builtBy : t('{built}; the project is not on this PC', {built: builtBy});
  const built = made?.source === 'output' ? t("{built} (the same files as a VPK in that project's output folder)", {built: notHere}) : notHere;
  return <div className="installed-details" id="installed-details" ref={refs.details} role="region" aria-label={t('Details of {file}', {file: mod.file})}>
    <div className="installed-details-head">
      <h3 className="mono">{mod.file}</h3>
      {made ? <span className="installed-badge">{t('Made here')}</span> : <span className="installed-badge quiet">{t('From elsewhere')}</span>}
      <span className="installed-spacer"/>
      {file && onOpenProject && <button type="button" onClick={() => onOpenProject(file)}>{t('Open its project')}</button>}
      {file && <button type="button" onClick={() => revealProject(file)}>{t('Show its project file')}</button>}
      <button type="button" onClick={reveal}>{t('Show in folder')}</button>
      <button type="button" className="ghost" onClick={close}>{t('Close')}</button>
    </div>
    <dl className="installed-kv">
      <dt>{t('Hero')}</dt><dd>{heroLine}{c.every_hero > 0 && c.heroes.length > 0 && ` · ${tp(c.every_hero, {one: '{count} file for every hero', other: '{count} files for every hero'})}`}{c.unattributed > 0 && c.heroes.length > 0 && ` · ${tp(c.unattributed, {one: '{count} without a hero', other: '{count} without a hero'})}`}</dd>
      <dt>{t('Contents')}</dt><dd>{mod.unreadable ? t('Not readable as a VPK: {reason}', {reason: mod.unreadable}) : contentsText(mod)}</dd>
      {c.ability_objects.length > 0 && <><dt>{t('Ability objects')}</dt><dd>{c.ability_objects.map(o => t('{name} ({users})', {name: readableStem(o.path), users: o.every_hero ? t('every hero') : o.heroes.length ? nameList(o.heroes.map(shownHero)) : t('no hero the game data names')})).join(tc('list separator', '; '))}</dd></>}
      <dt>{t('Built')}</dt><dd>{made?.several
        ? t('by this app: VPKs in the output folders of several projects hold these same files, so no one project is named')
        : made
        ? built
        : t('Not made by this app, or installed by it before it kept a record of installs')}</dd>
      <dt>{tc('column', 'State')}</dt><dd>{stateOf(mod).label}{mod.note && ` · ${mod.note}`}</dd>
      <dt><label htmlFor="installed-label-input">{t('Your label')}</label></dt><dd className="installed-label-field">
        {mod.identity ? <>
          <input id="installed-label-input" ref={refs.label} value={draft} maxLength={80} placeholder={t('For example: main skin')} onChange={event => setDraft(event.target.value)}
            onBlur={save} onKeyDown={event => { if (event.key === 'Enter') save(); if (event.key === 'Escape') reset(); }} aria-describedby="installed-label-note"/>
          <span id="installed-label-note" className="installed-note" role={labelState === 'saved' ? 'status' : undefined}>{labelState === 'saving' ? t('Saving…') : ''}{labelState === 'saved' ? sentences(t('Saved.'), t("Kept with the file's contents, so it stays when the file is renumbered or renamed.")) : t("Kept with the file's contents, so it stays when the file is renumbered or renamed.")}</span>
        </> : <span className="installed-note">{t('A file that cannot be read cannot be labelled.')}</span>}
      </dd>
      {(mod.shared ?? []).length > 0 && <><dt>{t('Same game files')}</dt><dd>
        <p className="installed-note">{t('These files are also in other VPKs, with other contents. Which version a match uses is not decided here.')}</p>
        {(mod.shared ?? []).map(s => <details key={s.path} className="installed-paths"><summary>{tp(s.count, {one: '{count} file also in {file}', other: '{count} files also in {file}'}, {file: s.file})}</summary>
          <ul>{s.paths.map(p => <li key={p} className="mono">{p}</li>)}{s.count > s.paths.length && <li className="muted">{more(s.count - s.paths.length)}</li>}</ul></details>)}
      </dd></>}
      {!mod.unreadable && c.files > 0 && <><dt>{t('Files by kind')}</dt><dd>
        {KIND_ORDER.filter(k => (c.kinds[k] ?? 0) > 0).map(k => <details key={k} className="installed-paths"><summary>{kindName(k, c.kinds[k] ?? 0)}</summary>
          <ul>{(c.samples[k] ?? []).map(p => <li key={p} className="mono">{p}</li>)}{(c.kinds[k] ?? 0) > (c.samples[k] ?? []).length && <li className="muted">{more((c.kinds[k] ?? 0) - (c.samples[k] ?? []).length)}</li>}</ul></details>)}
      </dd></>}
    </dl>
  </div>;
}
