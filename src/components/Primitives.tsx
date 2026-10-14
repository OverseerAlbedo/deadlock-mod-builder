import { useId, type PropsWithChildren, type ReactNode } from 'react';
import { useState as useCopyState } from 'react';
import type { ErrorHelp, StepStatus } from '../models';
import { scrubUser } from '../api';

const statuses: Record<StepStatus, [string, string]> = {
  running: ['●', 'Running'], succeeded: ['✓', 'Complete'], failed: ['×', 'Failed'],
  cancelled: ['−', 'Cancelled'], cached: ['↺', 'Cached'], skipped: ['—', 'Skipped'],
};
export function Status({ status }: { status: StepStatus }) {
  const [icon, label] = statuses[status] ?? ['?', 'Unknown'];
  return <span className={`status status-${status}`}><span aria-hidden="true">{icon}</span> {label}</span>;
}
export function PageTitle({ title, subtitle, action, eyebrow }: { title: string; subtitle: string; action?: ReactNode; eyebrow?: string }) {
  return <header className="page-title"><div>{eyebrow&&<span className="studio-eyebrow">{eyebrow}</span>}<h1>{title}</h1><p>{subtitle}</p></div>{action}</header>;
}
export function Section({ title, children, actions, className = '', id: suppliedId }: PropsWithChildren<{ title: string; actions?: ReactNode; className?: string; id?: string }>) {
  const id = useId();
  return <section className={`section ${className}`} id={suppliedId} aria-labelledby={id}><div className="section-heading"><h2 id={id}>{title}</h2>{actions}</div>{children}</section>;
}
export type StateTone = 'ready' | 'attention' | 'error' | 'neutral' | 'working';
export function StateLabel({ tone, children }: PropsWithChildren<{ tone: StateTone }>) {
  const icon = { ready: '✓', attention: '!', error: '×', neutral: '—', working: '●' }[tone];
  return <span className={`state-label state-${tone}`} title={typeof children === 'string' ? children : undefined}><span aria-hidden="true">{icon}</span><span>{children}</span></span>;
}
export function Disclosure({ title, hint, children, id, open }: PropsWithChildren<{ title: string; hint?: string; id?: string; open?: boolean }>) {
  return <details className="disclosure" id={id} open={open}><summary><span>{title}</span>{hint && <small>{hint}</small>}</summary><div className="disclosure-content">{children}</div></details>;
}
export function Field({ label, value, onChange, hint, disabled = false }: {
  label: string; value: string; onChange: (value: string) => void; hint?: string; disabled?: boolean;
}) {
  const id = useId();
  return <div className="field"><label htmlFor={id}>{label}</label><input id={id} value={value} disabled={disabled} onChange={event => onChange(event.target.value)} aria-describedby={hint ? `${id}-hint` : undefined} />{hint && <p className="hint" id={`${id}-hint`}>{hint}</p>}</div>;
}
/** An error with what went wrong in plain words, the steps to fix it and a
 * button to the page where the fix happens (help from troubleshooting.rs). */
export function ErrorNotice({ title, details, dismiss, help, go }: { title: string; details: string; dismiss?: () => void; help?: ErrorHelp | null; go?: (page: string) => void }) {
  const [copied, setCopied] = useCopyState(false);
  // Copied text never carries the Windows user name; it is meant to be shared.
  const copy = () => { void navigator.clipboard?.writeText(scrubUser([title, help?.cause, details].filter(Boolean).join('\n\n'))).then(() => setCopied(true)).catch(() => setCopied(false)); };
  return <div className="error-notice" role="alert"><div className="row"><strong>{title}</strong><div className="row"><button className="tertiary" onClick={copy}>{copied ? 'Copied' : 'Copy for support'}</button>{dismiss && <button className="tertiary" onClick={dismiss}>Dismiss</button>}</div></div>
    {help && <div className="error-help">
      <p className="error-cause">{help.cause}</p>
      {help.steps.length > 0 && <><p className="error-help-title">What to do</p><ol>{help.steps.map(step => <li key={step}>{step}</li>)}</ol></>}
      {help.action && go && <button className="primary" onClick={() => { go(help.action!.page); }}>{help.action.label}</button>}
    </div>}
    <details><summary>Technical details</summary><pre>{details}</pre></details></div>;
}
export function NavIcon({ type }: { type: 'project' | 'model' | 'time' | 'log' | 'settings' }) {
  const paths = {
    project: <><path d="M3 5h5l2 2h11v13H3z" /><path d="M3 11h18" /></>,
    model: <><path d="m12 3 9 5-9 5-9-5 9-5Z" /><path d="M3 8v9l9 5 9-5V8M12 13v9" /></>,
    time: <><circle cx="12" cy="12" r="9" /><path d="M12 6v6l4 2" /></>,
    log: <><path d="m4 7 4 5-4 5M12 17h8" /></>,
    settings: <><path d="M4 7h16M4 17h16" /><circle cx="9" cy="7" r="3" /><circle cx="16" cy="17" r="3" /></>,
  };
  return <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">{paths[type]}</svg>;
}
