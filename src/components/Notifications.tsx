import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import type { BuildResult } from '../build/models';
import type { ErrorHelp, FieldCheck, Settings } from '../models';
import { api, basename } from '../api';
import { ToolLink } from '../setup/ToolDownloads';

/** The sidebar's Notifications: states that hold while something is true (shown
 * first) and events of this session (newest first). Every row can be dismissed;
 * a state can also be ignored for good. */
export type NotificationKind = 'success' | 'warning' | 'error';
export interface NotificationItem {
  id: string; kind: NotificationKind; title: string;
  /** Short time (HH:MM) for events. */ time?: string;
  /** The full content, shown when the row is clicked. */ body: ReactNode;
  /** Removes the row for this session. */ dismiss: () => void;
}

/** State notifications by kind (the names saved when one is ignored). */
export type StateNotification = 'csdk12';
/** Ignored state notifications, kept on this PC (a JSON list of kinds). */
export const IGNORED_NOTIFICATIONS = 'dmb.ignoredNotifications';
export function readIgnored(): StateNotification[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(IGNORED_NOTIFICATIONS) ?? '[]');
    return Array.isArray(value) ? value.filter((kind): kind is StateNotification => kind === 'csdk12') : [];
  } catch { return []; }
}
export function writeIgnored(kinds: StateNotification[]) {
  try { if (kinds.length) localStorage.setItem(IGNORED_NOTIFICATIONS, JSON.stringify(kinds)); else localStorage.removeItem(IGNORED_NOTIFICATIONS); } catch { /* storage unavailable */ }
}

/** At most this many events are kept (in memory, for this session only). */
export const MAX_EVENTS = 20;
/** What a failed operation showed: the message, the technical details and the help. */
export interface ErrorRecord { title: string; details: string; help: ErrorHelp | null }
/** Something that happened in this session: an operation's result or a failure. */
export interface AppEvent {
  id: string; at: number; kind: NotificationKind; title: string;
  /** Successes and warnings of one operation replace each other (only the latest is kept). */ operation?: string;
  /** The project it was about. */ project?: string;
  /** Plain sentences, shown in full when the row is opened (successes and warnings). */ lines?: string[];
  paths?: { label: string; path: string }[];
  /** A page to go to from the row. */ open?: { label: string; page: string };
  /** Failures keep their own copy of the message, the details and the help. */ error?: ErrorRecord;
}
/** An event before it has an id and a time. */
export type NewEvent = Omit<AppEvent, 'id' | 'at'>;
/** A new event goes first. A success or warning replaces the earlier one of the same
 * operation; failures are all kept. The oldest beyond MAX_EVENTS are dropped. */
export function addEvent(events: AppEvent[], event: AppEvent): AppEvent[] {
  const replaces = event.kind !== 'error' && event.operation;
  return [event, ...events.filter(old => !(replaces && old.kind !== 'error' && old.operation === event.operation))].slice(0, MAX_EVENTS);
}
/** HH:MM of a time in milliseconds. */
export function shortTime(at: number): string {
  const date = new Date(at);
  return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}
/** The CSDK 12 state: exactly while the current (unsaved or saved) settings point at a Reduced CSDK 12. */
export function usesCsdk12(checks: FieldCheck[] | null | undefined): boolean {
  return checks?.find(check => check.field === 'csdk')?.layout === 'older';
}

const LABELS: Record<NotificationKind, string> = { success: 'Success', warning: 'Warning', error: 'Error' };
const PLURALS: Record<NotificationKind, [string, string]> = { error: ['error', 'errors'], warning: ['warning', 'warnings'], success: ['success', 'successes'] };
const SEVERITY: NotificationKind[] = ['error', 'warning', 'success'];
/** The marks of the Settings field checks (FieldStatus). */
const MARKS: Record<NotificationKind, string> = { success: '✓', warning: '!', error: '✕' };
/** A round badge with the kind's mark; the mark is decorative, the label is read out. */
export function Badge({ kind, label = LABELS[kind], className = '' }: { kind: NotificationKind; label?: string; className?: string }) {
  return <span className={`notification-badge notification-badge-${kind} ${className}`.trim()}><span aria-hidden="true">{MARKS[kind]}</span><span className="sr-only">{label}</span></span>;
}

/** What a finished build did, in plain words. */
export function builtText(result: BuildResult): string {
  if (result.installed) return `The mod was built and installed in the game as ${basename(result.installed)}.`;
  if (result.vpk) return `The mod was built as ${basename(result.vpk)}. It was not installed; install it on Build & install.`;
  if (result.compiled) return 'The model was compiled. No VPK was requested.';
  return 'The source files were prepared. No compilation was requested.';
}

/** The rows for this session's events. A failure shows only a short message in the
 * column; Show full details opens the whole error on the page. */
export function eventItems(events: AppEvent[], { go, dismiss, showDetails }: { go: (page: string) => void; dismiss: (id: string) => void; showDetails: (event: AppEvent) => void }): NotificationItem[] {
  return events.map((event): NotificationItem => {
    const common = { id: event.id, kind: event.kind, title: event.title, time: shortTime(event.at), dismiss: () => dismiss(event.id) };
    const { error } = event;
    if (error) return { ...common, body: <>
      <p className="notice-message notice-clamp" title={error.title}>{error.title}</p>
      <div className="notice-actions">
        {error.help?.action && <button className="primary" onClick={() => go(error.help!.action!.page)}>{error.help.action.label}</button>}
        <button onClick={() => showDetails(event)}>Show full details</button>
      </div>
    </> };
    return { ...common, body: <>
      {event.project && <p><strong>{event.project}</strong></p>}
      {event.lines?.map(line => <p key={line}>{line}</p>)}
      {event.paths?.map(({ label, path }) => <p key={label} className="notice-path">{label}: <code>{path}</code></p>)}
      {event.open && <div className="notice-actions"><button onClick={() => go(event.open!.page)}>{event.open.label}</button></div>}
    </> };
  });
}

/** The way from Reduced CSDK 12 to 13 (what Settings showed under the Community SDK field). */
export function MoveToCsdk13({ settings, busy, update, go, notify, fail, found, setFound, ignore }: {
  settings: Settings; busy: boolean; update: (settings: Settings) => void; go: (page: string) => void;
  notify: (message: string) => void; fail: (title: string, error: unknown) => void;
  /** The not-found message, held by the caller so collapsing the row keeps it. */ found: string | null; setFound: (message: string | null) => void;
  /** Keep Reduced CSDK 12: never show this notification again (saved). */ ignore: () => void;
}) {
  const [finding, setFinding] = useState(false);
  /** Detect tools' search with the SDK fields cleared; only an SDK of the current
   * layout goes into the draft settings, and Settings opens so it can be saved. */
  async function find() {
    setFinding(true); setFound(null);
    try {
      const detected = await api.detectSettings({ ...settings, csdk: '', resourcecompiler: '' });
      const next = { ...settings, csdk: detected.csdk, resourcecompiler: detected.resourcecompiler };
      const layout = detected.csdk ? (await api.checkSettingFields(next))?.find(item => item.field === 'csdk')?.layout : undefined;
      if (layout === 'current') { update(next); go('settings'); notify(`Reduced CSDK 13 found: ${detected.csdk}. Save settings to use it.`); }
      else setFound('No extracted Reduced CSDK 13 was found. Extract the whole ZIP first, or choose its folder as Community SDK in Settings.');
    } catch (error) { fail('Could not look for Reduced CSDK 13', error); } finally { setFinding(false); }
  }
  return <>
    <p>You are using Reduced CSDK 12. It still works, but Reduced CSDK 13 replaces it and also compiles the hero model. To move over:</p>
    <ol>
      <li>Get Reduced CSDK 13 (<ToolLink id="csdk" disabled={busy}/>) and extract the whole ZIP to a short folder outside OneDrive, for example <code>C:\CSDK13</code>.</li>
      <li>Click <strong>Find Reduced CSDK 13</strong> below, or choose that folder as Community SDK in Settings. Then click the suggested path under Resource compiler, or Fix the marked path, so the compiler follows.</li>
      {settings.model_backend === 'cswin' && <li>Your setup compiles hero models with a downloaded model toolset. Once Reduced CSDK 13 is the Community SDK, click <strong>Compile models with Reduced CSDK 13</strong> under Model compiler in Settings.</li>}
      <li>Save settings, run Test my setup, then Analyze and build your project again.</li>
    </ol>
    <p>Your projects, prepared models and settings are kept; materials are compiled again on the next build. Once a build works, you can delete the Reduced CSDK 12 folder (the working folders the app made inside it go with it) and, when models compile with Reduced CSDK 13, the downloaded model toolset.</p>
    <div className="notice-actions">
      <button disabled={busy || finding} onClick={() => void find()}>{finding ? 'Looking…' : 'Find Reduced CSDK 13'}</button>
      <button onClick={() => go('settings')}>Open Settings</button>
      <button className="tertiary" title="Keep Reduced CSDK 12 and do not show this again" onClick={ignore}>Ignore</button>
    </div>
    <p className="notice-hint">Ignore keeps Reduced CSDK 12 and does not show this again. Settings can show it again.</p>
    {found && <p className="notice-found">{found}</p>}
  </>;
}

/** The Notifications section of the sidebar: a summary badge, then a list that
 * scrolls inside itself. Each row shows its full content only when clicked. */
export function Notifications({ items, clear, focus }: { items: NotificationItem[]; clear: () => void;
  /** A new failure: opened in place and scrolled into view inside the list. */ focus?: string | null }) {
  const labelId = useId();
  const [open, setOpen] = useState<Set<string>>(() => new Set());
  const rows = useRef(new Map<string, HTMLLIElement>());
  useEffect(() => {
    if (!focus) return;
    setOpen(current => new Set(current).add(focus));
    rows.current.get(focus)?.scrollIntoView?.({ block: 'nearest' });
  }, [focus]);
  const toggle = (id: string) => setOpen(current => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  const counts = SEVERITY.map(kind => [kind, items.filter(item => item.kind === kind).length] as const).filter(([, count]) => count > 0);
  const worst = counts[0]?.[0];
  return <section className="sidebar-notifications" aria-labelledby={labelId}>
    <div className="notice-head">
      <div className="nav-group-label" id={labelId}>Notifications</div>
      {worst && <Badge kind={worst} className="notification-summary" label={counts.map(([kind, count]) => `${count} ${PLURALS[kind][count === 1 ? 0 : 1]}`).join(', ')}/>}
      {items.length > 0 && <button className="tertiary notice-clear" title="Clear the list for now" onClick={clear}>Clear</button>}
    </div>
    <ul className="notice-list">
      {items.length === 0 && <li className="notice-empty">No notifications</li>}
      {items.map(item => {
        const expanded = open.has(item.id);
        const bodyId = `${labelId}-${item.id}`;
        return <li key={item.id} ref={element => { if (element) rows.current.set(item.id, element); else rows.current.delete(item.id); }} className={`notice notice-${item.kind}`}>
          <div className="notice-row">
            <button className="notice-toggle" aria-expanded={expanded} aria-controls={expanded ? bodyId : undefined} onClick={() => toggle(item.id)}>
              <Badge kind={item.kind}/><span className="notice-title">{item.title}</span>{item.time && <span className="notice-time">{item.time}</span>}
            </button>
            <button className="tertiary notice-dismiss" aria-label="Dismiss" title="Dismiss" onClick={item.dismiss}>×</button>
          </div>
          {expanded && <div className="notice-body" id={bodyId}>{item.body}</div>}
        </li>;
      })}
    </ul>
  </section>;
}
