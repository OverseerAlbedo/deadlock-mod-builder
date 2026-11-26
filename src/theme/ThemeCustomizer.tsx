// i18n-area: settings
/** Settings → Appearance: the theme cards (built in and yours) and the theme
 * editor: three base colors (or every token under Advanced), a color picker
 * with a hex field for each, and a live preview of the whole interface. */
import { useMemo, useState } from 'react';
import { BrandMark } from '../components/BrandMark';
import { isHex } from './colors';
import { THEMES, TOKEN_KEYS, allThemes, deriveTokens, iconOf, newPresetId, themeById, tokensStyle, tokensToMap, type BaseColors, type Theme, type ThemeIcon, type ThemePreset, type ThemeTokens } from './themes';
import { m, tc, useT } from '../i18n';
import { T } from '../i18n/T';

/** A theme's name as shown: a built-in theme's name is translated, a custom theme's is the user's own.
 * The English name stays the value everywhere else (ids, the name check, saved presets). */
export function shownThemeName(theme: Pick<Theme, 'name' | 'custom'>): string {
  return theme.custom ? theme.name : tc('theme name', theme.name);
}

/** The theme cards: one per theme, the active one marked; custom ones can be edited or deleted. */
export function ThemeCards({active, customs, disabled, onChoose, onEdit, onDelete, onCustomize}: {active: string; customs: ThemePreset[]; disabled: boolean; onChoose: (id: string) => void; onEdit: (preset: ThemePreset) => void; onDelete: (id: string) => void; onCustomize: () => void}) {
  const t = useT();
  return <div className="theme-cards" role="list" aria-label={t('Themes')}>
    {allThemes(customs).map(theme => {
      const colors = theme.tokens;
      return <div key={theme.id} role="listitem" className={`theme-card ${theme.id === active ? 'active' : ''}`}>
        <button type="button" className="theme-card-pick" disabled={disabled} aria-pressed={theme.id === active} aria-label={t('{name} theme', {name: shownThemeName(theme)})} onClick={() => onChoose(theme.id)}>
          <span className="theme-card-swatches" aria-hidden="true"><i style={{background: colors.background}}/><i style={{background: colors.studioCard}}/><i style={{background: colors.accent}}/><i style={{background: colors.primaryBg}}/></span>
          <span className="theme-card-name">{theme.id === active && '✓ '}{shownThemeName(theme)}</span>
        </button>
        {theme.custom && <span className="theme-card-tools"><button type="button" className="tertiary" disabled={disabled} aria-label={t('Edit {name}', {name: theme.name})} onClick={() => onEdit(customs.find(c => c.id === theme.id)!)}>{t('Edit')}</button><button type="button" className="tertiary" disabled={disabled} aria-label={t('Delete {name}', {name: theme.name})} onClick={() => onDelete(theme.id)}>{t('Delete')}</button></span>}
      </div>;
    })}
    <button type="button" className="theme-card theme-card-new" disabled={disabled} onClick={onCustomize}>{t('+ Customize…')}</button>
  </div>;
}

/** A color: the picker and the hex code side by side; the hex applies once it is valid. */
function ColorField({label, value, onChange, disabled}: {label: string; value: string; onChange: (hex: string) => void; disabled: boolean}) {
  const t = useT();
  const [text, setText] = useState(value);
  const [last, setLast] = useState(value);
  if (last !== value) { setLast(value); setText(value); }
  const valid = isHex(text);
  return <label className="color-field">
    <span>{label}</span>
    <input type="color" value={isHex(value) ? value : '#000000'} disabled={disabled} aria-label={t('{label} picker', {label})} onChange={event => { setText(event.target.value); onChange(event.target.value); }}/>
    <input type="text" value={text} disabled={disabled} className={valid ? '' : 'invalid'} aria-label={t('{label} hex', {label})} spellCheck={false} maxLength={7}
      onChange={event => { const next = event.target.value.trim(); setText(next); if (isHex(next)) onChange(next.toLowerCase()); }}
      onBlur={() => { if (!valid) setText(value); }}/>
  </label>;
}

const GROUPS: [string, (keyof ThemeTokens)[]][] = [
  [m('Surfaces'), ['background', 'surface', 'surfaceSecondary', 'elevated', 'selected', 'hover', 'studioCard']],
  [m('Borders'), ['border', 'borderSubtle', 'studioBorder']],
  [m('Text'), ['text', 'secondary', 'muted']],
  [m('Chrome'), ['sidebarBg', 'topbarBg']],
  [m('Buttons'), ['primaryBg', 'primaryText', 'primaryHover', 'focus']],
  [m('Accent'), ['accent', 'accentStrong', 'accentText', 'accentDim', 'accentSoft', 'accentSoftBorder', 'accentPanel', 'accentPanelBorder', 'accentSelected', 'accentSelectedBorder', 'accentSelectedText', 'accentOutline']],
];
const TOKEN_LABELS: Partial<Record<keyof ThemeTokens, string>> = {
  background: m('Page background'), surface: m('Surface'), surfaceSecondary: m('Surface 2'), elevated: m('Raised'), selected: m('Selected'), hover: m('Hover'), studioCard: m('Card'),
  border: m('Border'), borderSubtle: m('Border, subtle'), studioBorder: m('Card border'), text: m('Text color'), secondary: m('Secondary text'), muted: m('Muted text'),
  sidebarBg: m('Sidebar'), topbarBg: m('Top bar'), primaryBg: m('Primary button'), primaryText: m('Primary button text'), primaryHover: m('Primary button hover'), focus: m('Focus ring'),
  accent: m('Accent color'), accentStrong: m('Accent, strong'), accentText: m('Text on accent'), accentDim: m('Accent, dim'), accentSoft: m('Accent surface'), accentSoftBorder: m('Accent surface border'),
  accentPanel: m('Accent panel'), accentPanelBorder: m('Accent panel border'), accentSelected: m('Pressed'), accentSelectedBorder: m('Pressed border'), accentSelectedText: m('Pressed text'), accentOutline: m('Outline'),
};

/** What every token looks like, drawn with the editor's current tokens only. */
export function ThemePreview({tokens, icon}: {tokens: ThemeTokens; icon: ThemeIcon}) {
  const t = useT();
  return <div className="theme-preview" style={tokensStyle(tokens)} aria-label={t('Theme preview')}>
    <div className="theme-preview-sidebar">
      <div className="brand"><span className="brand-mark" aria-hidden="true"><BrandMark theme={icon}/></span><div>Deadlock<span>Mod Builder</span></div></div>
      <button type="button" className="nav-item nav-step nav-done" tabIndex={-1}><span className="nav-step-number">✓</span>{t('Character & model')}</button>
      <button type="button" className="nav-item nav-step selected" tabIndex={-1}><span className="nav-step-number">2</span>{t('Prepare model')}</button>
      <button type="button" className="nav-item nav-step" tabIndex={-1}><span className="nav-step-number">3</span>{t('Appearance')}</button>
    </div>
    <div className="theme-preview-page studio-workspace">
      <header className="page-title"><div><span className="studio-eyebrow">{t('Step 2 of 5')}</span><h1>{t('Prepare model')}</h1><p>{t("Your model is fitted to the hero's skeleton.")}</p></div></header>
      <div className="theme-preview-row"><button type="button" className="primary" tabIndex={-1}>{t('Primary')}</button><button type="button" tabIndex={-1}>{t('Default')}</button><button type="button" className="tertiary" tabIndex={-1}>{t('Tertiary')}</button><span className="prep-workspace"><span className="prep-badge">{t('Badge')}</span></span><span className="segmented" role="presentation"><button type="button" aria-checked="true" role="radio" tabIndex={-1}>{t('Still')}</button><button type="button" aria-checked="false" role="radio" tabIndex={-1}>{t('Movement')}</button></span></div>
      <div className="theme-preview-card"><div className="section-heading"><h2>{t('Your model')}</h2><span className="muted">BLEND · GLB</span></div><div className="theme-preview-row"><span className="prep-workspace"><button type="button" className="prep-primary" tabIndex={-1}>{t('Prepare automatically')}</button></span><button type="button" tabIndex={-1}>{t('Load and check only')}</button></div><label className="checkbox checkbox-line"><input type="checkbox" defaultChecked readOnly tabIndex={-1}/> {t("Keep the hero's cloth")}</label><p className="hint"><T text="Secondary text looks like this; muted text like <muted>this</muted>." tags={{muted: chunk => <span className="muted">{chunk}</span>}}/></p></div>
      <div className="theme-preview-row"><span className="build-completed">{t('Completed')}</span><span className="prep-workspace"><span className="prep-matched" style={{margin: 0, padding: '4px 10px'}}>✓ {t('Matched')}</span></span><span className="capability-notice" style={{margin: 0, padding: '4px 10px'}}>{t('Warning')}</span><span className="statusbar-progress" style={{flex: '1 1 120px', height: 6, background: 'var(--elevated)', borderRadius: 3, overflow: 'hidden'}}><span className="statusbar-progress-fill" style={{display: 'block', width: '60%'}}/></span></div>
    </div>
  </div>;
}

/** The editor: start from any theme, set the three base colors (or any token
 * under Advanced), pick the window icon, name it and save it as a preset. */
export function ThemeCustomizer({start, editing, customs, disabled, onSave, onCancel}: {start: Theme; editing?: ThemePreset; customs: ThemePreset[]; disabled: boolean; onSave: (preset: ThemePreset) => void; onCancel: () => void}) {
  const t = useT();
  const seed = editing ? themeById(editing.id, customs) : start;
  const [base, setBase] = useState<BaseColors>(seed.base ?? {accent: seed.tokens.accent, background: seed.tokens.background, text: seed.tokens.text});
  const [overrides, setOverrides] = useState<Partial<ThemeTokens>>(() => {
    // A built-in theme keeps its hand-tuned tokens until a base color changes; an
    // edited custom theme keeps whatever its tokens differ from the derivation.
    if (!seed.custom) return {...seed.tokens};
    const derived = deriveTokens(seed.base ?? {accent: seed.tokens.accent, background: seed.tokens.background, text: seed.tokens.text});
    return Object.fromEntries(TOKEN_KEYS.filter(key => String(seed.tokens[key]) !== String(derived[key])).map(key => [key, seed.tokens[key]])) as Partial<ThemeTokens>;
  });
  // The suggested name is saved as it stands, so it stays English whatever the interface language.
  const [name, setName] = useState(editing?.name ?? (seed.custom ? seed.name : `My ${seed.name}`));
  const [icon, setIcon] = useState<ThemeIcon>(seed.icon);
  const [advanced, setAdvanced] = useState(false);
  const tokens = useMemo(() => ({...deriveTokens(base), ...overrides}), [base, overrides]);
  const setBaseColor = (key: keyof BaseColors, hex: string) => {
    setBase(current => ({...current, [key]: hex}));
    // A changed base color re-derives everything that was not set by hand under Advanced.
    setOverrides(current => { const kept = {...current}; for (const k of TOKEN_KEYS) if (!(handSet.has(k))) delete kept[k]; return kept; });
  };
  const [handSet] = useState(() => new Set<keyof ThemeTokens>());
  const setToken = (key: keyof ThemeTokens, value: string) => { handSet.add(key); setOverrides(current => ({...current, [key]: key === 'hue' ? Number(value) : value})); };
  const names = new Set(allThemes(customs).filter(theme => theme.id !== editing?.id).map(theme => theme.name.toLowerCase()));
  const trimmed = name.trim();
  const nameProblem = !trimmed ? t('Give the theme a name.') : names.has(trimmed.toLowerCase()) ? t('A theme with this name exists.') : trimmed.length > 40 ? t('Up to 40 characters.') : '';
  const save = () => onSave({id: editing?.id ?? newPresetId(), name: trimmed, icon, base: {accent: base.accent, background: base.background, text: base.text}, tokens: tokensToMap(tokens)});
  return <div className="theme-editor" role="region" aria-label={t('Theme editor')}>
    <div className="theme-editor-head">
      <strong>{editing ? t('Editing {name}', {name: editing.name}) : t('New theme from {name}', {name: shownThemeName(seed)})}</strong>
      <label className="theme-editor-name">{t('Name')} <input type="text" value={name} maxLength={40} disabled={disabled} aria-label={t('Theme name')} onChange={event => setName(event.target.value)}/></label>
      <button type="button" className="primary" disabled={disabled || Boolean(nameProblem)} onClick={save}>{editing ? t('Save changes') : t('Save as preset')}</button>
      <button type="button" disabled={disabled} onClick={onCancel}>{t('Cancel')}</button>
      {nameProblem && <span className="muted">{nameProblem}</span>}
    </div>
    <div className="theme-editor-body">
      <div className="theme-editor-fields">
        <h3>{t('Colors')}</h3>
        <ColorField label={t('Accent')} value={base.accent} disabled={disabled} onChange={hex => setBaseColor('accent', hex)}/>
        <ColorField label={t('Background')} value={base.background} disabled={disabled} onChange={hex => setBaseColor('background', hex)}/>
        <ColorField label={t('Text')} value={base.text} disabled={disabled} onChange={hex => setBaseColor('text', hex)}/>
        <p className="hint">{t('Surfaces, borders, buttons and the accent shades follow these three. Success stays green and errors red.')}</p>
        <h3>{t('Window icon')}</h3>
        <div className="theme-icon-choice" role="radiogroup" aria-label={t('Window icon')}>{THEMES.map(option => <label key={option.id} className="checkbox"><input type="radio" name="theme-icon" value={option.icon} checked={icon === option.icon} disabled={disabled} onChange={() => setIcon(option.icon)}/>{option.name === 'Original' ? tc('theme name', 'Grey D') : tc('theme name', option.name)}</label>)}</div>
        <details className="technical" open={advanced} onToggle={event => setAdvanced((event.target as HTMLDetailsElement).open)}><summary>{t('Advanced: every color')}</summary>
          {GROUPS.map(([group, keys]) => <div key={group}><h3>{t(group)}</h3>{keys.map(key => <ColorField key={key} label={t(TOKEN_LABELS[key] ?? key)} value={String(tokens[key])} disabled={disabled} onChange={hex => setToken(key, hex)}/>)}</div>)}
          <h3>{t('Tint')}</h3>
          <label className="color-field"><span>{t('Hue of the grey shades')}</span><input type="range" min={0} max={360} value={tokens.hue} disabled={disabled} aria-label={t('Hue of the grey shades')} onChange={event => setToken('hue', event.target.value)}/><output>{tokens.hue}°</output></label>
        </details>
      </div>
      <ThemePreview tokens={tokens} icon={iconOf(icon)}/>
    </div>
  </div>;
}
