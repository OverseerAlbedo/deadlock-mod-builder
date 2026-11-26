import { useMemo, useState } from 'react';
import type { TimingReport, TimingStep } from '../models';
import { PageTitle, Status } from './Primitives';
import { m, tp, useT } from '../i18n';
// i18n-area: common

export function duration(ns: number): string {
  if (!Number.isFinite(ns) || ns < 0) return m('Unavailable');
  if (ns === 0) return '0 ms';
  if (ns < 1_000_000) return '<1 ms';
  if (ns < 1_000_000_000) return `${(ns / 1_000_000).toFixed(1)} ms`;
  if (ns < 60_000_000_000) return `${(ns / 1_000_000_000).toFixed(3)} s`;
  return `${Math.floor(ns / 60_000_000_000)}m ${((ns % 60_000_000_000) / 1_000_000_000).toFixed(1)}s`;
}
export function stepDepth(step: TimingStep, steps: TimingStep[]): number {
  const byId = new Map(steps.map(item => [item.id, item]));
  const seen = new Set<string>([step.id]);
  let current = step, depth = 0;
  while (current.parent_id && depth < 12) {
    const parent = byId.get(current.parent_id);
    if (!parent || seen.has(parent.id)) break;
    seen.add(parent.id); depth++; current = parent;
  }
  return depth;
}
export function Timings({ report, openReport, busy }: { report: TimingReport | null; openReport: () => void; busy: boolean }) {
  const t = useT();
  /** A duration as drawn: the numbers and units as they are, "Unavailable" translated. */
  const shown = (ns: number) => t(duration(ns));
  const [query, setQuery] = useState('');
  const rows = useMemo(() => report?.steps.filter(step => step.name.toLowerCase().includes(query.toLowerCase())) ?? [], [report, query]);
  return <>
    <PageTitle eyebrow={t('Performance')} title={t('Step timings')} subtitle={t('See where time is spent, from the full operation to each individual step.')} action={<button className="primary" onClick={openReport} disabled={busy}>{t('Open timing report…')}</button>} />
    {!report ? <div className="empty-state"><h2>{t('No timing report open')}</h2><p>{t('Analyze a model or open a saved timing report. Older logs without timing records cannot provide durations for unmeasured steps.')}</p></div> : <>
      <div className="report-meta"><div><span className="eyebrow">{t('Operation')}</span><strong>{report.operation}</strong></div><div><span className="eyebrow">{t('Total elapsed')}</span><strong data-testid="total-duration">{shown(report.total_ns)}</strong></div><div><span className="eyebrow">{t('Recorded steps')}</span><strong>{report.steps.length}</strong></div><div><span className="eyebrow">{t('Result')}</span><Status status={report.status} /></div></div>
      <div className="timing-records">
      <div className="toolbar"><input aria-label={t('Filter timing steps')} placeholder={t('Filter steps…')} value={query} onChange={event => setQuery(event.target.value)} /><span className="muted">{query?tp(report.steps.length,{other:'{shown} of {count} recorded steps'},{shown:rows.length}):tp(report.steps.length,{other:'{count} recorded steps'})}</span></div>
      <div className="table-scroll"><table className="timing-table"><thead><tr><th>{t('Step')}</th><th>{t('Status')}</th><th>{t('Elapsed')}</th><th>{t('Own work')}</th></tr></thead><tbody>
        {rows.map(step => <tr key={step.id}><td style={{ paddingLeft: 12 + stepDepth(step, report.steps) * 18 }}><span>{step.name}</span>{step.reason && <small className="step-reason">{step.reason}</small>}{step.error && <details className="step-error"><summary>{t('Error details')}</summary><pre>{step.error}</pre></details>}</td><td><Status status={step.status} /></td><td className="numeric timing-elapsed">{step.status === 'skipped' ? '—' : shown(step.inclusive_ns)}{step.status!=='skipped'&&report.total_ns>0&&Number.isFinite(step.inclusive_ns)&&<span className="timing-bar" aria-hidden="true" style={{width:`${Math.min(100,Math.max(0,step.inclusive_ns/report.total_ns*100))}%`}}/>}</td><td className="numeric">{step.status === 'skipped' ? '—' : shown(step.exclusive_ns)}</td></tr>)}
      </tbody></table></div>
      {rows.length === 0 && <p className="empty-state">{t('No matching steps.')}</p>}
      <p className="hint">{t('Elapsed includes child steps. Own work excludes measured child intervals. Total elapsed is the recorded wall time; nested or overlapping steps are never added to it.')}</p>
      </div>
      <details className="technical"><summary>{t('Report details')}</summary><dl><dt>{t('Run')}</dt><dd className="mono">{report.run_id}</dd><dt>{t('Started')}</dt><dd>{report.started_utc}</dd><dt>{t('Finished')}</dt><dd>{report.finished_utc ?? t('Still running at time of capture')}</dd><dt>{t('Timing schema')}</dt><dd>{report.schema_version}</dd></dl></details>
    </>}
  </>;
}
