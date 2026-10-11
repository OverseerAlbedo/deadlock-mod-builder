import { useEffect, useMemo, useRef, useState } from 'react';
import type { LogEntry } from '../models';
import { PageTitle } from './Primitives';

const ROW_HEIGHT = 23;
const OVERSCAN = 10;
export function visibleRange(count: number, scrollTop: number, height: number) {
  const start = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  return { start, end: Math.min(count, start + Math.ceil(height / ROW_HEIGHT) + OVERSCAN * 2) };
}
export function LogView({ entries, copy, save, busy }: { entries: LogEntry[]; copy: () => void; save: () => void; busy: boolean }) {
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
    <PageTitle eyebrow="Session output" title="Logs" subtitle="Find a message, inspect an error, or save the full output for troubleshooting." action={<div className="row"><button onClick={copy} disabled={busy}>Copy session log</button><button className="primary" onClick={save} disabled={busy}>Save complete log…</button></div>} />
    <div className="log-panel">
    <div className="toolbar"><input aria-label="Search logs" placeholder="Search logs…" value={query} onChange={event => { setQuery(event.target.value); setScrollTop(0); if (viewport.current) viewport.current.scrollTop = 0; }} /><select aria-label="Log level" value={level} onChange={event => setLevel(event.target.value)}><option value="all">All levels</option><option value="info">Information</option><option value="warning">Warnings</option><option value="error">Errors</option></select><label className="checkbox"><input type="checkbox" checked={follow} onChange={event => setFollow(event.target.checked)} />Follow output</label></div>
    <div className="log-panel-heading"><strong>{query||level!=='all'?'Filtered output':'Session output'}</strong><span className="muted">{lines.length.toLocaleString()} lines · {follow?'Following latest output':'Scroll position held'}</span></div>
    <div className="log-viewport" ref={viewport} tabIndex={0} aria-label="Diagnostic log" onScroll={event => { const element = event.currentTarget; setScrollTop(element.scrollTop); if (element.scrollTop + element.clientHeight < element.scrollHeight - ROW_HEIGHT) setFollow(false); }}>
      {lines.length ? <div style={{ height: lines.length * ROW_HEIGHT, position: 'relative', minWidth: '100%' }}><div style={{ position: 'absolute', top: start * ROW_HEIGHT, minWidth: '100%' }}>{lines.slice(start, end).map(entry => <div className={`log-line log-${entry.level}`} style={{ height: ROW_HEIGHT }} key={entry.key} data-testid="log-line"><span className="log-timestamp">{entry.timestamp_utc ? new Date(entry.timestamp_utc).toLocaleTimeString() : '—'}</span><span className="log-severity">{entry.level === 'error' ? 'ERROR' : entry.level === 'warning' ? 'WARN' : ''}</span><span>{entry.message || ' '}</span></div>)}</div></div> : <p className="log-empty">{entries.length ? 'No matching log lines.' : 'No operations recorded in this session.'}</p>}
    </div>
    <footer className="log-footer"><span>{entries.length.toLocaleString()} recent records retained · Copy includes session records · Save includes complete tool output</span><button className="tertiary" onClick={() => setViewStart((entries.at(-1)?.sequence??-1)+1)}>Clear view</button></footer>
    </div>
  </>;
}
