/** The Theme Manager: every theme is one set of design tokens, applied to <html>
 * as CSS variables, and every component paints only with those variables (the
 * accent and chrome tokens, and `--hue`, which every tinted neutral in the
 * stylesheets is relative to). Success, warning and error colors are not
 * tokens: Completed stays green and an error stays red in every theme. The
 * built-in themes are below; custom themes (Settings → Appearance → Customize)
 * are `ThemePreset`s saved in settings.json, derived from three base colors
 * plus any token the user set by hand. */
import type { CSSProperties } from 'react';
import { contrast, hexToHsl, isHex, mix, shiftLightness, withLightness, withSaturation } from './colors';

export interface ThemeTokens {
  /** The hue every tinted neutral and shade in the stylesheets is relative to (153 = the original green-teal). */
  hue: number;
  background: string; surface: string; surfaceSecondary: string; elevated: string; selected: string;
  border: string; borderSubtle: string; hover: string;
  text: string; secondary: string; muted: string; focus: string;
  sidebarBg: string; topbarBg: string; brandMarkBorder: string;
  primaryBg: string; primaryText: string; primaryHover: string;
  accent: string; accentStrong: string; accentText: string; accentDim: string;
  accentSoft: string; accentSoftBorder: string; accentPanel: string; accentPanelBorder: string;
  accentSelected: string; accentSelectedBorder: string; accentSelectedText: string;
  accentOutline: string; accentOutlineFaint: string; accentFaint: string;
  studioBorder: string; studioCard: string;
}
/** Which window icon a theme uses (the built-in Ds). */
export type ThemeIcon = 'original' | 'violet' | 'mint';
export interface Theme { id: string; name: string; description: string; icon: ThemeIcon; custom?: boolean; base?: BaseColors; tokens: ThemeTokens }
/** The three colors a custom theme starts from; everything else is derived. */
export interface BaseColors { accent: string; background: string; text: string }
/** A custom theme as saved in settings.json. */
export interface ThemePreset { id: string; name: string; icon?: string; base?: Record<string, string> | null; tokens: Record<string, string> }

const ORIGINAL: ThemeTokens = {
  hue: 153,
  background: '#0d0f11', surface: '#121518', surfaceSecondary: '#171a1e', elevated: '#1c2024', selected: '#22272d',
  border: '#30363d', borderSubtle: '#24292f', hover: '#282e34',
  text: '#f1f3f5', secondary: '#afb7bf', muted: '#919ba6', focus: '#9fe4c5',
  sidebarBg: '#101214', topbarBg: '#111417', brandMarkBorder: '#636d78',
  primaryBg: '#ebedf0', primaryText: '#171a1e', primaryHover: '#ffffff',
  accent: '#9fe4c5', accentStrong: '#c1f4dc', accentText: '#14291f', accentDim: '#3b6553',
  accentSoft: '#1d3028', accentSoftBorder: '#477762', accentPanel: '#1b2d25', accentPanelBorder: '#365647',
  accentSelected: '#2a3e36', accentSelectedBorder: '#5b7b6d', accentSelectedText: '#cbefde',
  accentOutline: '#73b992', accentOutlineFaint: '#73b99222', accentFaint: '#9fe4c530',
  studioBorder: '#303c44', studioCard: '#171d22',
};
const VIOLET: ThemeTokens = {
  hue: 252,
  background: '#0c0e15', surface: '#12151f', surfaceSecondary: '#171b27', elevated: '#1c2131', selected: '#262c40',
  border: '#323a55', borderSubtle: '#252b3d', hover: '#2b3249',
  text: '#eef0fa', secondary: '#b4bbd3', muted: '#959db8', focus: '#a394ff',
  sidebarBg: '#0e1019', topbarBg: '#10131c', brandMarkBorder: 'transparent',
  primaryBg: '#9d8bff', primaryText: '#150f33', primaryHover: '#b7a9ff',
  accent: '#9d8bff', accentStrong: '#b7a9ff', accentText: '#150f33', accentDim: '#564b9e',
  accentSoft: '#231f3f', accentSoftBorder: '#5a50a8', accentPanel: '#1c1a35', accentPanelBorder: '#3d3870',
  accentSelected: '#2b2754', accentSelectedBorder: '#6a60bd', accentSelectedText: '#ddd6ff',
  accentOutline: '#9d8bff', accentOutlineFaint: '#9d8bff22', accentFaint: '#9d8bff30',
  studioBorder: '#353e5c', studioCard: '#161a27',
};
const MINT: ThemeTokens = {
  hue: 162,
  background: '#0b1213', surface: '#10191a', surfaceSecondary: '#151f20', elevated: '#1a2628', selected: '#223134',
  border: '#2d4044', borderSubtle: '#233235', hover: '#27383b',
  text: '#eef5f3', secondary: '#b2c4be', muted: '#91a5a0', focus: '#39b8ff',
  sidebarBg: '#0d1516', topbarBg: '#0f1819', brandMarkBorder: 'transparent',
  primaryBg: '#5ff2c0', primaryText: '#06302a', primaryHover: '#93f7d6',
  accent: '#5ff2c0', accentStrong: '#93f7d6', accentText: '#06302a', accentDim: '#2f8a74',
  accentSoft: '#163229', accentSoftBorder: '#2f7a66', accentPanel: '#142b26', accentPanelBorder: '#2a5a4e',
  accentSelected: '#1d4038', accentSelectedBorder: '#3f8a77', accentSelectedText: '#cafff1',
  accentOutline: '#5ff2c0', accentOutlineFaint: '#5ff2c022', accentFaint: '#5ff2c030',
  studioBorder: '#2f4448', studioCard: '#141e20',
};

export const THEMES: Theme[] = [
  {id: 'original', name: 'Original', description: 'The app as released: grey chrome, green-teal accents.', icon: 'original', tokens: ORIGINAL},
  {id: 'violet', name: 'Violet D', description: 'After the violet D icon: blue-black surfaces, violet accents.', icon: 'violet', tokens: VIOLET},
  {id: 'mint', name: 'Mint D', description: 'After the mint D icon: teal-black surfaces, mint accents, sky focus.', icon: 'mint', tokens: MINT},
];

/** Token name → CSS variable the stylesheets read. */
export const VARIABLES: Record<keyof ThemeTokens, string> = {
  hue: '--hue',
  background: '--background', surface: '--surface', surfaceSecondary: '--surface-secondary', elevated: '--elevated', selected: '--selected',
  border: '--border', borderSubtle: '--border-subtle', hover: '--hover',
  text: '--text', secondary: '--secondary', muted: '--muted', focus: '--focus',
  sidebarBg: '--sidebar-bg', topbarBg: '--topbar-bg', brandMarkBorder: '--brand-mark-border',
  primaryBg: '--primary-bg', primaryText: '--primary-text', primaryHover: '--primary-hover',
  accent: '--accent', accentStrong: '--accent-strong', accentText: '--accent-text', accentDim: '--accent-dim',
  accentSoft: '--accent-soft', accentSoftBorder: '--accent-soft-border', accentPanel: '--accent-panel', accentPanelBorder: '--accent-panel-border',
  accentSelected: '--accent-selected', accentSelectedBorder: '--accent-selected-border', accentSelectedText: '--accent-selected-text',
  accentOutline: '--accent-outline', accentOutlineFaint: '--accent-outline-faint', accentFaint: '--accent-faint',
  studioBorder: '--studio-border', studioCard: '--studio-card',
};
export const TOKEN_KEYS = Object.keys(VARIABLES) as (keyof ThemeTokens)[];

/** A whole theme from three colors: surfaces step up from the background,
 * text greys mix toward it, the accent family keeps the accent's hue, and the
 * primary button is the accent with whichever text reads on it. */
export function deriveTokens(base: BaseColors): ThemeTokens {
  const accent = isHex(base.accent) ? base.accent : ORIGINAL.accent;
  const background = isHex(base.background) ? base.background : ORIGINAL.background;
  const text = isHex(base.text) ? base.text : ORIGINAL.text;
  const bg = (delta: number) => shiftLightness(background, delta);
  const tint = (s: number, l: number) => withLightness(withSaturation(accent, s), l);
  const dark = tint(.35, .12);
  const accentText = contrast(accent, dark) >= contrast(accent, '#ffffff') ? dark : '#ffffff';
  return {
    hue: Math.round(hexToHsl(accent)[0]),
    background, surface: bg(.02), surfaceSecondary: bg(.035), elevated: bg(.055), selected: bg(.08),
    border: bg(.14), borderSubtle: bg(.10), hover: bg(.10),
    text, secondary: mix(text, background, .72), muted: mix(text, background, .55), focus: accent,
    sidebarBg: bg(.012), topbarBg: bg(.02), brandMarkBorder: bg(.33),
    primaryBg: accent, primaryText: accentText, primaryHover: shiftLightness(accent, .08),
    accent, accentStrong: shiftLightness(accent, .1), accentText, accentDim: tint(.3, .32),
    accentSoft: tint(.28, .15), accentSoftBorder: tint(.3, .4), accentPanel: tint(.25, .14), accentPanelBorder: tint(.28, .27),
    accentSelected: tint(.22, .21), accentSelectedBorder: tint(.3, .44), accentSelectedText: tint(.6, .88),
    accentOutline: accent, accentOutlineFaint: `${accent}22`, accentFaint: `${accent}30`,
    studioBorder: bg(.18), studioCard: bg(.045),
  };
}

const ICONS: ThemeIcon[] = ['original', 'violet', 'mint'];
export const iconOf = (value: string | undefined): ThemeIcon => (ICONS as string[]).includes(value ?? '') ? value as ThemeIcon : 'original';

/** A saved custom theme as a Theme (missing tokens fall back to the original's). */
export function presetToTheme(preset: ThemePreset): Theme {
  const tokens = {...ORIGINAL} as ThemeTokens;
  for (const key of TOKEN_KEYS) {
    const value = preset.tokens[key];
    if (value === undefined) continue;
    if (key === 'hue') { const hue = Number(value); if (Number.isFinite(hue)) tokens.hue = hue; }
    else (tokens as unknown as Record<string, string>)[key] = value;
  }
  const base = preset.base && isHex(preset.base.accent ?? '') && isHex(preset.base.background ?? '') && isHex(preset.base.text ?? '')
    ? {accent: preset.base.accent, background: preset.base.background, text: preset.base.text} : undefined;
  return {id: preset.id, name: preset.name, description: 'Your theme.', icon: iconOf(preset.icon), custom: true, base, tokens};
}
export function tokensToMap(tokens: ThemeTokens): Record<string, string> {
  return Object.fromEntries(TOKEN_KEYS.map(key => [key, String(tokens[key])]));
}
export const newPresetId = () => `custom-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;

export function allThemes(customs: ThemePreset[] = []): Theme[] {
  return [...THEMES, ...customs.map(presetToTheme)];
}
export function themeById(id: string | undefined, customs: ThemePreset[] = []): Theme {
  return allThemes(customs).find(theme => theme.id === id) ?? THEMES[0];
}
/** The tokens as inline CSS variables, for a preview that must not change the app. */
export function tokensStyle(tokens: ThemeTokens): CSSProperties {
  const style: Record<string, string> = {};
  for (const key of TOKEN_KEYS) style[VARIABLES[key]] = String(tokens[key]);
  return style as CSSProperties;
}

/** Apply a theme: its tokens become the CSS variables of `root` (the document
 * element) and `data-theme` names it (absent for the original). */
export function applyTheme(id: string | undefined, customs: ThemePreset[] = [], root: HTMLElement = document.documentElement): Theme {
  const theme = themeById(id, customs);
  for (const key of TOKEN_KEYS) root.style.setProperty(VARIABLES[key], String(theme.tokens[key]));
  root.style.setProperty('color', theme.tokens.text);
  root.style.setProperty('background', theme.tokens.background);
  if (theme.id === 'original') delete root.dataset.theme; else root.dataset.theme = theme.id;
  return theme;
}
