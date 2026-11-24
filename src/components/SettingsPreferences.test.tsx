import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Settings } from '../models';
import { defaultFit } from '../models';
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl: vi.fn()}));
import { SettingsPage } from './SettingsPage';

// Settings → Appearance: Default fit for new models and Load 3D previews automatically.
const saved: Settings = {steam:'',deadlock:'',pak:'',csdk:'',resourcecompiler:'',cswin:'',cswin_resourcecompiler:'',model_backend:'auto',model_game:'',model_format:'auto',optional_cs2:'',source2viewer:'',vpk_tool:'',vpk_backend:'builtin',ag2_adapter:'',workspace:'',last_project:'',blender:''};
const props = {update: vi.fn(), persist: vi.fn(), busy: false, dirty: false, fail: vi.fn(), detect: vi.fn(), status: null, check: vi.fn(), fields: [], chooseTheme: vi.fn()};

describe('Appearance preferences in Settings', () => {
  it('shows Preserve source proportions and automatic previews off for settings without the fields', () => {
    render(<SettingsPage {...props} settings={saved} savePreference={vi.fn()}/>);
    expect(screen.getByLabelText('Default fit for new models')).toHaveValue('preserve_proportions');
    expect(screen.getByLabelText('Load 3D previews automatically')).not.toBeChecked();
    expect(defaultFit(saved)).toBe('preserve_proportions');
    expect(defaultFit({default_fit_mode: 'something else'})).toBe('preserve_proportions');
    expect(defaultFit({default_fit_mode: 'match_target'})).toBe('match_target');
  });
  it('saves each choice at once, not as a tool-path change', () => {
    const savePreference = vi.fn(), update = vi.fn();
    render(<SettingsPage {...props} update={update} settings={{...saved, default_fit_mode: 'match_target', auto_previews: true}} savePreference={savePreference}/>);
    expect(screen.getByLabelText('Default fit for new models')).toHaveValue('match_target');
    expect(screen.getByLabelText('Load 3D previews automatically')).toBeChecked();
    fireEvent.change(screen.getByLabelText('Default fit for new models'), {target: {value: 'preserve_proportions'}});
    expect(savePreference).toHaveBeenLastCalledWith({default_fit_mode: 'preserve_proportions'});
    fireEvent.click(screen.getByLabelText('Load 3D previews automatically'));
    expect(savePreference).toHaveBeenLastCalledWith({auto_previews: false});
    expect(update).not.toHaveBeenCalled();
  });
  it('is disabled while the app is busy', () => {
    render(<SettingsPage {...props} busy settings={saved} savePreference={vi.fn()}/>);
    expect(screen.getByLabelText('Default fit for new models')).toBeDisabled();
    expect(screen.getByLabelText('Load 3D previews automatically')).toBeDisabled();
  });
});
