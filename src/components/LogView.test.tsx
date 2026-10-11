import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { LogView, visibleRange } from './LogView';
import type { LogEntry } from '../models';

describe('Complete log virtual view', () => {
  const entries: LogEntry[] = Array.from({length:10000},(_,sequence) => ({sequence,timestamp_utc:'2026-10-06T00:00:00Z',level:sequence === 9876 ? 'error' : 'info',message:sequence === 9876 ? 'A specific Unicode error: 模型' : `Compiler output ${sequence}`}));
  it('renders a bounded window of a long log and can find an offscreen error', () => {
    render(<LogView entries={entries} copy={() => {}} save={() => {}} busy={false} />);
    expect(screen.getAllByTestId('log-line').length).toBeLessThan(60);
    expect(screen.getByText('Compiler output 9999')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Search logs'), {target:{value:'模型'}});
    expect(screen.getAllByTestId('log-line')).toHaveLength(1);
    expect(screen.getByText('A specific Unicode error: 模型')).toBeInTheDocument();
    expect(screen.getByText(/10,000 recent records retained/)).toBeInTheDocument();
  });
  it('clear view does not remove retained data or change full-log actions', () => {
    const copy = vi.fn(), save = vi.fn();
    const {rerender} = render(<LogView entries={entries} copy={copy} save={save} busy={false} />);
    fireEvent.click(screen.getByText('Clear view'));
    expect(screen.queryAllByTestId('log-line')).toHaveLength(0);
    fireEvent.click(screen.getByText('Copy session log')); fireEvent.click(screen.getByText('Save complete log…'));
    expect(copy).toHaveBeenCalledOnce(); expect(save).toHaveBeenCalledOnce();
    rerender(<LogView entries={[...entries,{sequence:10000,timestamp_utc:'',level:'warning',message:'New output'}]} copy={copy} save={save} busy={false} />);
    expect(screen.getByText('New output')).toBeInTheDocument();
    expect(screen.getByText(/10,001 recent records retained/)).toBeInTheDocument();
  });
  it('bounds the overscan range', () => {
    expect(visibleRange(10000,0,230)).toEqual({start:0,end:30});
    expect(visibleRange(10,0,230)).toEqual({start:0,end:10});
  });
});
