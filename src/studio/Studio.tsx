import { useRef, type KeyboardEvent, type ReactNode } from 'react';
import './studio.css';

/** The studio frame shared by the step pages (Physics & preview first; Prepare model and
 * Appearance & weapons use the same pieces): a top bar, a tab strip with a status word per tab,
 * a view card beside the tab panels and an action bar at the bottom. Colours come from the
 * theme tokens only. */
export function StudioFrame({label, className = '', children}: {/** The frame's accessible name. */label: string; className?: string; children: ReactNode}) {
  return <section className={`studio-frame ${className}`} aria-label={label}>{children}</section>;
}

/** The top bar: what is open (`title`), a badge (`badge`), a short status line, and the page's
 * tools on the right (`children`). `below` spans the whole bar under them (a confirmation, a message). */
export function StudioTopBar({title, badge, badgeTone, status, children, below}: {title: ReactNode; badge?: ReactNode; /** The badge's colour; the accent when unset. */badgeTone?: StudioTone; status?: ReactNode; children?: ReactNode; below?: ReactNode}) {
  return <div className="studio-topbar">
    <div className="studio-topbar-id"><strong className="studio-topbar-title">{title}</strong>{badge && <span className={badgeTone ? `studio-badge ${badgeTone}` : 'studio-badge'}>{badge}</span>}{status && <span className="studio-topbar-status">{status}</span>}</div>
    {children && <div className="studio-topbar-tools">{children}</div>}
    {below && <div className="studio-topbar-below">{below}</div>}
  </div>;
}

/** A status word's colour: `ok` (green, a state that is fine), `changed` (accent, something the
 * user changed), `attention` (amber, something to do), `quiet` (muted, not in use). */
export type StudioTone = 'ok' | 'changed' | 'attention' | 'quiet';
export interface StudioTab<T extends string> { id: T; label: string; status?: string; tone?: StudioTone; title?: string }
const tabId = (base: string, id: string) => `${base}-tab-${id}`;
const panelId = (base: string, id: string) => `${base}-panel-${id}`;

/** The tab strip (a tab list: arrow keys, Home and End move between the tabs). `base` makes the
 * ids that tie each tab to its `StudioTabPanel`. */
export function StudioTabs<T extends string>({label, base, tabs, active, onSelect}: {label: string; base: string; tabs: StudioTab<T>[]; active: T; onSelect: (id: T) => void}) {
  const refs = useRef(new Map<string, HTMLButtonElement>());
  function keys(event: KeyboardEvent<HTMLDivElement>) {
    const at = tabs.findIndex(tab => tab.id === active);
    const next = ({ArrowRight: at + 1, ArrowLeft: at - 1, Home: 0, End: tabs.length - 1} as Record<string, number>)[event.key];
    if (next === undefined) return;
    event.preventDefault();
    const tab = tabs[(next + tabs.length) % tabs.length];
    onSelect(tab.id); refs.current.get(tab.id)?.focus();
  }
  return <div className="studio-tabs" role="tablist" aria-label={label} onKeyDown={keys}>
    {tabs.map(tab => <button key={tab.id} type="button" role="tab" id={tabId(base, tab.id)} aria-controls={panelId(base, tab.id)} aria-selected={tab.id === active} tabIndex={tab.id === active ? 0 : -1} title={tab.title}
      ref={element => { if (element) refs.current.set(tab.id, element); else refs.current.delete(tab.id); }} className="studio-tab" onClick={() => onSelect(tab.id)}>
      <span>{tab.label}</span>{tab.status && <>{' '}<small className={`studio-tab-status ${tab.tone ?? 'quiet'}`}>{tab.status}</small></>}
    </button>)}
  </div>;
}

/** One tab's panel. Inactive panels stay mounted (hidden), so what they hold (a pending result,
 * a typed value) survives a tab switch. */
export function StudioTabPanel({base, id, active, className = '', children}: {base: string; id: string; active: boolean; className?: string; children: ReactNode}) {
  return <div role="tabpanel" id={panelId(base, id)} aria-labelledby={tabId(base, id)} hidden={!active} className={`studio-tab-panel ${className}`}>{children}</div>;
}

/** A card in a tab panel, with an optional title row (`title`, a badge, actions on the right). */
export function StudioCard({title, badge, actions, sub, className = '', children}: {title?: ReactNode; badge?: ReactNode; actions?: ReactNode; sub?: ReactNode; className?: string; children?: ReactNode}) {
  return <div className={`studio-card ${className}`}>
    {(title || actions) && <div className="studio-card-head">{title && <h3>{title}</h3>}{badge && <span className="studio-badge">{badge}</span>}{actions && <span className="studio-card-actions">{actions}</span>}</div>}
    {sub && <p className="studio-card-sub">{sub}</p>}
    {children}
  </div>;
}

/** The view card: a bar on top (view choices; `barEnd` sits at its right end), the 3D view, and a
 * footer line. `barRef`/`footRef` give a view rendered inside the stage a place to put its own
 * toolbar and notes (through a portal), so they read as one bar. */
export function StudioView({title, bar, barEnd, barRef, footer, footRef, className = '', children}: {title: string; bar?: ReactNode; barEnd?: ReactNode; barRef?: (element: HTMLElement | null) => void; footer?: ReactNode; footRef?: (element: HTMLElement | null) => void; className?: string; children: ReactNode}) {
  return <section className={`studio-view ${className}`} aria-label={title}>
    <div className="studio-view-bar">{bar}<span className="studio-view-slot" ref={barRef}/>{barEnd && <span className="studio-view-bar-end">{barEnd}</span>}</div>
    <div className="studio-view-stage">{children}</div>
    <div className="studio-view-foot">{footer}<div className="studio-view-foot-slot" ref={footRef}/></div>
  </section>;
}

/** The action bar at the bottom of the frame: a state mark, a headline and a line of text, and
 * the page's actions on the right. Not a live region (the page's own messages are). */
export function StudioActionBar({tone = 'quiet', title, text, children}: {tone?: StudioTone; title: ReactNode; text?: ReactNode; children?: ReactNode}) {
  const mark = {ok: '✓', changed: '●', attention: '!', quiet: '—'}[tone];
  return <footer className={`studio-action-bar ${tone}`}>
    <span className="studio-action-mark" aria-hidden="true">{mark}</span>
    <div className="studio-action-text"><strong>{title}</strong>{text && <p>{text}</p>}</div>
    {children && <div className="studio-action-buttons">{children}</div>}
  </footer>;
}
