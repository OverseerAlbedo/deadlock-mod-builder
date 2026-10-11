import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Timings, duration, stepDepth } from './Timings';
import type { TimingReport, TimingStep } from '../models';

function step(id: string, name: string, ns: number, overrides: Partial<TimingStep> = {}): TimingStep {
  return {id, name, parent_id:null, status:'succeeded', started_utc:'2026-10-06T00:00:00Z',finished_utc:'2026-10-06T00:00:10Z',inclusive_ns:ns,exclusive_ns:ns,error:null,reason:null,...overrides};
}
describe('Timing report display', () => {
  it('keeps wall total independent of nested and overlapping intervals, retains every step and status', () => {
    const report: TimingReport = {schema_version:1,operation:'Build',run_id:'run',started_utc:'2026-10-06T00:00:00Z',finished_utc:'2026-10-06T00:00:10Z',status:'failed',total_ns:10e9,events:[],steps:[
      step('outer','Prepare materials',8e9,{exclusive_ns:2e9}),
      step('a','Texture conversion',6e9,{parent_id:'outer'}),
      step('b','Material compilation',5e9,{parent_id:'outer',status:'cached',reason:'Inputs unchanged'}),
      step('bad','Validate cloth',2e9,{status:'failed',error:'Exact value differs'}),
      step('skip','Installation',0,{status:'skipped',reason:'Validation failed'}),
    ]};
    render(<Timings report={report} openReport={() => {}} busy={false} />);
    expect(screen.getByTestId('total-duration')).toHaveTextContent('10.000 s');
    expect(screen.getByText('5 recorded steps')).toBeInTheDocument();
    const rows = screen.getAllByRole('row');
    expect(rows).toHaveLength(6);
    expect(within(rows[1]).getByText('8.000 s')).toBeInTheDocument();
    expect(within(rows[1]).getByText('2.000 s')).toBeInTheDocument();
    expect(within(rows[3]).getByText(/Cached/)).toBeInTheDocument();
    expect(within(rows[3]).getByText('Inputs unchanged')).toBeInTheDocument();
    expect(within(rows[4]).getByText('Exact value differs')).toBeInTheDocument();
    expect(within(rows[5]).getAllByText('—')).toHaveLength(3);
    expect(stepDepth(report.steps[1],report.steps)).toBe(1);
  });
  it('does not invent durations for an absent report', () => {
    render(<Timings report={null} openReport={() => {}} busy={false} />);
    expect(screen.queryByTestId('total-duration')).not.toBeInTheDocument();
    expect(screen.getByText(/Older logs without timing records/)).toBeInTheDocument();
  });
  it('handles failed and in-progress submillisecond durations honestly', () => {
    expect(duration(100)).toBe('<1 ms');
    expect(duration(0)).toBe('0 ms');
    expect(duration(NaN)).toBe('Unavailable');
    expect(duration(61.5e9)).toBe('1m 1.5s');
  });
  it('terminates on malformed parent cycles', () => {
    const a = step('a','A',1,{parent_id:'b'}), b = step('b','B',1,{parent_id:'a'});
    expect(stepDepth(a,[a,b])).toBe(1);
  });
});
