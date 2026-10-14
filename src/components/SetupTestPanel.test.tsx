import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { SetupTest } from '../models';
import { SetupTestPanel } from './SettingsPage';

describe('Test my setup', () => {
  it('shows each check and the fix for the one that failed', async () => {
    const result: SetupTest = {passed: false, steps: [
      {name: 'Paths in Settings', passed: true, detail: 'Every path in Settings points at the right file or folder.'},
      {name: 'Material compiler (Community SDK)', passed: true, detail: 'Compiled a test material with C:\\CSDK12\\game\\bin_cs2\\win64\\resourcecompiler.exe.'},
      {name: 'Model compiler (model toolset)', passed: false, detail: 'Choose the model toolset',
        error: {kind: 'validation', message: 'Choose the model toolset', details: 'Integrity check failed: no toolset', help: {cause: 'The model compiler is not set up.', steps: ['Open Set up model compiler…'], action: {label: 'Set up model compiler', page: 'setup'}}}},
    ]};
    const test = vi.fn().mockResolvedValue(result), go = vi.fn();
    render(<SetupTestPanel test={test} busy={false} go={go}/>);
    fireEvent.click(screen.getByRole('button', {name: 'Test my setup'}));
    expect(await screen.findByText('Something needs fixing before the first build. See below.')).toBeTruthy();
    expect(screen.getByText('✓ Material compiler (Community SDK)')).toBeTruthy();
    expect(screen.getByText('✕ Model compiler (model toolset)')).toBeTruthy();
    expect(screen.getByText('The model compiler is not set up.')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', {name: 'Set up model compiler'}));
    expect(go).toHaveBeenCalledWith('setup');
    expect(screen.getByRole('button', {name: 'Test again'})).toBeTruthy();
  });
});
