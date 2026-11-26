import { useEffect, useMemo, useRef, useState } from 'react';
import type { LogEntry } from '../models';
import { PageTitle } from './Primitives';
import { tc, tp, useT } from '../i18n';
// i18n-area: common

const ROW_HEIGHT = 23;
const OVERSCAN = 10;
export function visibleRange(count: number, scrollTop: number, height: number) {
  const start = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  return { start, end: Math.min(count, start + Math.ceil(height / ROW_HEIGHT) + OVERSCAN * 2) };
}
export function LogView({ entries, copy, save, busy }: { entries: LogEntry[]; copy: () => void; save: () => void; busy: boolean }) {
  const t = useT();
  const [query, setQuery] = useState('');
  const [level, setLevel] = useState('all');
  const [follow, setFollow] = useState(true);
  const [viewStart, setViewStart] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(420);
  const viewport = useRef<HTMLDivElement>(null);
  const lines = useMemo(() => entries.filter(entry=>entry.sequence>=viewStart).flatMap(entry => entry.message.split(/\r?\n/).map((message, index) => ({ ...entry, message, key: `${entry.sequence}-${index}` }))).filter(entry => (level === 'all' || entry.level === level) && entry.message.toLowerCase().includes(query.toLowerCase())), [entries, viewStart, query, level]);
  const { start, end } = visibleRange(lines.length, scrollTop, height);
  useEffect(() => {
    if (!viewport.current) return;
    const observer = new ResizeObserver(([entry]) => setHeight(entry.contentRect.height));
    observer.observe(viewport.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (follow && viewport.current) {
      viewport.current.scrollTop = Math.max(0, lines.length * ROW_HEIGHT - height);
      setScrollTop(viewport.current.scrollTop);
    }
  }, [lines.length, follow, height]);
  return <>
    <PageTitle eyebrow={t('Session output')} title={t('Logs')} subtitle={t('Find a message, inspect an error, or save the full output for troubleshooting.')} action={<div className="row"><button onClick={copy} disabled={busy}>{t('Copy session log')}</button><button className="primary" onClick={save} disabled={busy}>{t('Save complete log…')}</button></div>} />
    <div className="log-panel">
    <div className="toolbar"><input aria-label={t('Search logs')} placeholder={t('Search logs…')} value={query} onChange={event => { setQuery(event.target.value); setScrollTop(0); if (viewport.current) viewport.current.scrollTop = 0; }} /><select aria-label={t('Log level')} value={level} onChange={event => setLevel(event.target.value)}><option value="all">{t('All levels')}</option><option value="info">{t('Information')}</option><option value="warning">{t('Warnings')}</option><option value="error">{t('Errors')}</option></select><label className="checkbox"><input type="checkbox" checked={follow} onChange={event => setFollow(event.target.checked)} />{t('Follow output')}</label></div>
    <div className="log-panel-heading"><strong>{query||level!=='all'?t('Filtered output'):t('Session output')}</strong><span className="muted">{follow?tp(lines.length,{other:'{total} lines · Following latest output'},{total:lines.length.toLocaleString()}):tp(lines.length,{other:'{total} lines · Scroll position held'},{total:lines.length.toLocaleString()})}</span></div>
    <div className="log-viewport" ref={viewport} tabIndex={0} aria-label={t('Diagnostic log')} onScroll={event => { const element = event.currentTarget; setScrollTop(element.scrollTop); if (element.scrollTop + element.clientHeight < element.scrollHeight - ROW_HEIGHT) setFollow(false); }}>
      {lines.length ? <div style={{ height: lines.length * ROW_HEIGHT, position: 'relative', minWidth: '100%' }}><div style={{ position: 'absolute', top: start * ROW_HEIGHT, minWidth: '100%' }}>{lines.slice(start, end).map(entry => <div className={`log-line log-${entry.level}`} style={{ height: ROW_HEIGHT }} key={entry.key} data-testid="log-line"><span className="log-timestamp">{entry.timestamp_utc ? new Date(entry.timestamp_utc).toLocaleTimeString() : '—'}</span><span className="log-severity">{entry.level === 'error' ? tc('log level', 'ERROR') : entry.level === 'warning' ? tc('log level', 'WARN') : ''}</span><span>{entry.message || ' '}</span></div>)}</div></div> : <p className="log-empty">{entries.length ? t('No matching log lines.') : t('No operations recorded in this session.')}</p>}
    </div>
    <footer className="log-footer"><span>{tp(entries.length, {other: '{total} recent records retained · Copy includes session records · Save includes complete tool output'}, {total: entries.length.toLocaleString()})}</span><button className="tertiary" onClick={() => setViewStart((entries.at(-1)?.sequence??-1)+1)}>{t('Clear view')}</button></footer>
    </div>
  </>;
}
