import { describe, expect, it } from 'vitest';
import { contrast } from './colors';
import { THEMES, VARIABLES, applyTheme, deriveTokens, presetToTheme, themeById, tokensToMap } from './themes';

describe('Theme Manager', () => {
  it('applies every token of a theme as a CSS variable and names the theme on the root', () => {
    const root = document.createElement('div');
    const theme = applyTheme('violet', [], root);
    expect(theme.id).toBe('violet');
    expect(root.dataset.theme).toBe('violet');
    for (const [key, variable] of Object.entries(VARIABLES)) {
      expect(root.style.getPropertyValue(variable)).toBe(String(theme.tokens[key as keyof typeof theme.tokens]));
    }
    expect(root.style.getPropertyValue('--hue')).toBe('252');
    // Back to the original: no data-theme, the original tokens.
    applyTheme('original', [], root);
    expect(root.dataset.theme).toBeUndefined();
    expect(root.style.getPropertyValue('--accent')).toBe('#9fe4c5');
    expect(root.style.getPropertyValue('--hue')).toBe('153');
  });
  it('derives a complete, readable theme from three colors and round-trips it through a preset', () => {
    const tokens = deriveTokens({accent: '#ff8800', background: '#141018', text: '#f4eef8'});
    for (const key of Object.keys(VARIABLES)) expect(tokens[key as keyof typeof tokens], key).toBeDefined();
    expect(tokens.hue).toBe(32);
    expect(contrast(tokens.primaryBg, tokens.primaryText)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(tokens.text, tokens.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(tokens.secondary, tokens.surface)).toBeGreaterThanOrEqual(4.5);
    // Surfaces step up from the background; borders are lighter still.
    const l = (hex: string) => parseInt(hex.slice(1, 3), 16) + parseInt(hex.slice(3, 5), 16) + parseInt(hex.slice(5, 7), 16);
    expect(l(tokens.surface)).toBeGreaterThan(l(tokens.background));
    expect(l(tokens.border)).toBeGreaterThan(l(tokens.surface));
    const preset = {id: 'custom-x', name: 'Ember', icon: 'mint', tokens: tokensToMap(tokens)};
    const theme = presetToTheme(preset);
    expect(theme.tokens).toEqual(tokens);
    expect(theme.icon).toBe('mint');
    expect(themeById('custom-x', [preset]).name).toBe('Ember');
    const root = document.createElement('div');
    applyTheme('custom-x', [preset], root);
    expect(root.style.getPropertyValue('--accent')).toBe('#ff8800');
    expect(root.dataset.theme).toBe('custom-x');
  });
  it('falls back to the original for an unknown theme and keeps every theme complete', () => {
    expect(themeById('nope').id).toBe('original');
    expect(themeById(undefined).id).toBe('original');
    const keys = Object.keys(VARIABLES);
    for (const theme of THEMES) {
      for (const key of keys) expect(theme.tokens[key as keyof typeof theme.tokens], `${theme.id} ${key}`).toBeDefined();
    }
  });
});
