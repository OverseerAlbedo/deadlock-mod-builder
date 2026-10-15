import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ThemeCards, ThemeCustomizer } from './ThemeCustomizer';
import { THEMES, deriveTokens, presetToTheme, type ThemePreset } from './themes';

const mine: ThemePreset = {id: 'custom-1', name: 'Mine', icon: 'violet', base: {accent: '#ff00aa', background: '#101010', text: '#ffffff'}, tokens: {...Object.fromEntries(Object.entries(deriveTokens({accent: '#ff00aa', background: '#101010', text: '#ffffff'})).map(([k, v]) => [k, String(v)]))}};

describe('Theme cards', () => {
  it('lists the built-in themes and the custom ones, which alone can be edited or deleted', () => {
    const onChoose = vi.fn(), onDelete = vi.fn(), onEdit = vi.fn();
    render(<ThemeCards active="violet" customs={[mine]} disabled={false} onChoose={onChoose} onEdit={onEdit} onDelete={onDelete} onCustomize={() => undefined}/>);
    const list = screen.getByRole('list', {name: 'Themes'});
    expect(within(list).getAllByRole('listitem')).toHaveLength(4);
    expect(screen.getByRole('button', {name: 'Violet D theme'})).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByRole('button', {name: 'Delete Original'})).toBeNull();
    fireEvent.click(screen.getByRole('button', {name: 'Delete Mine'}));
    expect(onDelete).toHaveBeenCalledWith('custom-1');
    fireEvent.click(screen.getByRole('button', {name: 'Edit Mine'}));
    expect(onEdit).toHaveBeenCalledWith(mine);
    fireEvent.click(screen.getByRole('button', {name: 'Mint D theme'}));
    expect(onChoose).toHaveBeenCalledWith('mint');
  });
});

describe('Theme customizer', () => {
  it('derives the preview from the three base colors, accepts hex codes and saves a named preset', () => {
    const onSave = vi.fn();
    render(<ThemeCustomizer start={THEMES[0]} customs={[]} disabled={false} onSave={onSave} onCancel={() => undefined}/>);
    const preview = screen.getByLabelText('Theme preview');
    expect(preview.style.getPropertyValue('--accent')).toBe('#9fe4c5');
    // A hex typed into the field applies once it is complete and valid.
    const hex = screen.getByLabelText('Accent hex');
    fireEvent.change(hex, {target: {value: '#ff00'}});
    expect(preview.style.getPropertyValue('--accent')).toBe('#9fe4c5');
    fireEvent.change(hex, {target: {value: '#FF00AA'}});
    expect(preview.style.getPropertyValue('--accent')).toBe('#ff00aa');
    expect(preview.style.getPropertyValue('--primary-bg')).toBe('#ff00aa');
    // The picker works the same way.
    fireEvent.change(screen.getByLabelText('Background picker'), {target: {value: '#101010'}});
    expect(preview.style.getPropertyValue('--background')).toBe('#101010');
    expect(preview.style.getPropertyValue('--surface')).not.toBe('#101010');
    // Names must be unique and present.
    const save = screen.getByRole('button', {name: 'Save as preset'});
    fireEvent.change(screen.getByLabelText('Theme name'), {target: {value: 'Original'}});
    expect(save).toBeDisabled();
    fireEvent.change(screen.getByLabelText('Theme name'), {target: {value: 'Neon'}});
    fireEvent.click(screen.getByLabelText('Mint D'));
    fireEvent.click(save);
    expect(onSave).toHaveBeenCalledTimes(1);
    const preset = onSave.mock.calls[0][0] as ThemePreset;
    expect(preset.name).toBe('Neon');
    expect(preset.icon).toBe('mint');
    expect(preset.tokens.accent).toBe('#ff00aa');
    expect(preset.tokens.background).toBe('#101010');
    expect(preset.base).toEqual({accent: '#ff00aa', background: '#101010', text: '#f1f3f5'});
    // Saved presets come back as complete themes.
    const theme = presetToTheme(preset);
    expect(theme.custom).toBe(true);
    expect(theme.tokens.accent).toBe('#ff00aa');
    expect(theme.tokens.hue).toBe(320);
  });
  it('keeps a hand-set advanced color when a base color changes', () => {
    const onSave = vi.fn();
    render(<ThemeCustomizer start={THEMES[1]} customs={[]} disabled={false} onSave={onSave} onCancel={() => undefined}/>);
    const preview = screen.getByLabelText('Theme preview');
    fireEvent.change(screen.getByLabelText('Primary button hex'), {target: {value: '#123456'}});
    fireEvent.change(screen.getByLabelText('Accent hex'), {target: {value: '#00ff88'}});
    expect(preview.style.getPropertyValue('--accent')).toBe('#00ff88');
    expect(preview.style.getPropertyValue('--primary-bg')).toBe('#123456');
  });
});
