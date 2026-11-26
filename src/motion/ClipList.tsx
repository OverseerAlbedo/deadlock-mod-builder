// i18n-area: motion
import { useEffect, useId, useRef, type ReactNode } from 'react';
import { tc, tp, tpc, useT } from '../i18n';
import type { ClipGroup } from '../components/AssetCatalog';
import type { Project } from '../models';
import type { FitReport } from './motionTypes';
import { clipState, GROUPS, groupCounts, groupTitle, matches, seconds, STATE_WORDS, type ClipItem } from './motionCatalog';

export type ListFilter = 'all' | 'replaced';

function Row({item, selected, select, state}: {item: ClipItem; selected: boolean; select: () => void; state: ReturnType<typeof clipState>}) {
  // Drawn again by the list (which follows the language) whenever it changes.
  return <li><button type="button" className={`clip-row ${selected ? 'selected' : ''}`} aria-pressed={selected} onClick={select}>
    <span className="clip-row-text"><span className="clip-row-head"><strong title={item.entry.name}>{item.entry.name}</strong>{state !== 'original' && <span className={`clip-row-badge ${state}`}>{tc('clip state', STATE_WORDS[state])}</span>}</span><span className="mono clip-row-meta"><span className="clip-row-label" title={item.entry.label}>{item.entry.label}</span>{' '}<span className="clip-row-length">· {seconds(item.entry.duration)} · {tpc('clip row', item.entry.frames, {one: '{count} frame', other: '{count} frames'})}</span></span></span>
  </button></li>;
}

/** The hero's animations: a search, group chips with counts, All / Replaced, and the rows. With a
 * search or Replaced, matches of every group show under their group's heading. Clips no animation
 * graph of the hero names are folded at the end (shown with a search). */
export function ClipList({project, items, folded, fits, selected, select, group, setGroup, filter, setFilter, query, setQuery, status}: {
  project: Project; items: ClipItem[]; folded: ClipItem[]; fits: Map<string, FitReport | 'checking' | Error>;
  selected: string; select: (target: string) => void; group: ClipGroup; setGroup: (group: ClipGroup) => void;
  filter: ListFilter; setFilter: (filter: ListFilter) => void; query: string; setQuery: (query: string) => void;
  /** Shown instead of the rows (not loaded, loading, failed). */status?: ReactNode;
}) {
  const t = useT();
  const search = useId();
  const replaced = items.filter(item => item.row);
  const pool = (filter === 'replaced' ? replaced : items).filter(item => matches(item, query));
  const counts = groupCounts(pool);
  const across = Boolean(query.trim()) || filter === 'replaced';
  const shown = across ? pool : pool.filter(item => item.entry.group === group);
  const foldedShown = folded.filter(item => (across || item.entry.group === group) && matches(item, query) && filter === 'all');
  // The selected row in view when the page set the selection or the shown rows changed (opening on a
  // replaced clip, a search, a filter): only its own list scrolls, never the page, and focus stays.
  const root = useRef<HTMLElement>(null), clicked = useRef(false);
  useEffect(() => {
    if (clicked.current) { clicked.current = false; return; }
    const element = root.current?.querySelector<HTMLElement>('.clip-row.selected');
    const box = element?.closest<HTMLElement>('.clip-rows');
    if (!element || !box) return;
    const top = element.getBoundingClientRect().top - box.getBoundingClientRect().top + box.scrollTop, bottom = top + element.offsetHeight;
    if (top < box.scrollTop) box.scrollTop = top;
    else if (bottom > box.scrollTop + box.clientHeight) box.scrollTop = bottom - box.clientHeight;
  }, [selected, filter, query, group, shown.length]);
  const row = (item: ClipItem) => <Row key={item.entry.target} item={item} selected={selected === item.entry.target} select={() => { if (item.entry.target !== selected) clicked.current = true; select(item.entry.target); }} state={clipState(project, item, fits)}/>;
  return <section ref={root} className="studio-card clip-list" aria-label={t('Animations')}>
    <div className="clip-list-head">
      <h3>{t('Animations')}</h3>
      <span className="studio-view-group" role="group" aria-label={tc('filter', 'Show')}>
        <button type="button" aria-pressed={filter === 'all'} onClick={() => setFilter('all')}>{t('All {count}', {count: items.length})}</button>
        <button type="button" aria-pressed={filter === 'replaced'} onClick={() => setFilter('replaced')}>{t('Replaced {count}', {count: replaced.length})}</button>
      </span>
    </div>
    <label className="sr-only" htmlFor={search}>{t('Search animations')}</label>
    <input id={search} className="clip-search" type="search" placeholder={t('Search animations…')} value={query} onChange={event => setQuery(event.target.value)}/>
    <div className="clip-chips" role="group" aria-label={t('Groups')}>
      {GROUPS.map(g => <button key={g.id} type="button" className="clip-chip" aria-pressed={!across && group === g.id} onClick={() => { setGroup(g.id); if (filter === 'replaced') setFilter('all'); if (query) setQuery(''); }}>{tc('clip group', g.title)} <small>{counts[g.id]}</small></button>)}
    </div>
    {status ?? <>
      {across
        ? GROUPS.filter(g => counts[g.id] || foldedShown.some(item => item.entry.group === g.id)).map(g => <div key={g.id} className="clip-group">
          <h4>{tc('clip group', g.title)} · {counts[g.id]}</h4>
          <ul className="clip-rows">{shown.filter(item => item.entry.group === g.id).map(row)}{foldedShown.filter(item => item.entry.group === g.id).map(row)}</ul>
        </div>)
        : <div className="clip-group"><h4>{tc('clip group', groupTitle(group))} · {counts[group]}</h4><ul className="clip-rows">{shown.map(row)}</ul></div>}
      {across && !shown.length && !foldedShown.length && <p className="hint">{filter === 'replaced' && !query ? t('No clip is replaced yet.') : t('No clip matches the search.')}</p>}
      {!across && foldedShown.length > 0 && <details className="clip-folded">
        <summary>{tp(foldedShown.length, {one: '{count} more · No animation graph of this hero names these', other: '{count} more · No animation graph of this hero names these'})}</summary>
        <ul className="clip-rows">{foldedShown.map(row)}</ul>
      </details>}
      <p className="hint clip-list-note">{t('Names are made from the game’s clip names; the clip name is shown under each.')}</p>
    </>}
  </section>;
}
