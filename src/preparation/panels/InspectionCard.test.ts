import { describe, expect, it } from 'vitest';
import type { PreparationAnalysis } from '../models';
import { foundSummary } from './InspectionCard';

const analysis = { analysis: {}, suggestions: {} } as unknown as PreparationAnalysis;

describe('What the check found', () => {
  it('names the buttons the page shows: Prepare model before a preparation, Check for problems and Prepare again after it', () => {
    expect(foundSummary(analysis, 0)).toBe('Every joint was matched automatically. Click Prepare model below, or look through the tabs first.');
    expect(foundSummary(analysis, 0, true)).toBe('Every joint was matched automatically. To change something, use the tabs, then Check for problems or Prepare again on Fit & review.');
    expect(foundSummary(analysis, 2, true)).toBe('2 joints still need a match below. Everything else was matched automatically.');
  });
});
