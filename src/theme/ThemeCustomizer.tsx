/** Settings → Appearance: the theme cards (built in and yours) and the theme
 * editor: three base colors (or every token under Advanced), a color picker
 * with a hex field for each, and a live preview of the whole interface. */
import { useMemo, useState } from 'react';
import { BrandMark } from '../components/BrandMark';
import { isHex } from './colors';
import { THEMES, TOKEN_KEYS, allThemes, deriveTokens, iconOf, newPresetId, themeById, tokensStyle, tokensToMap, type BaseColors, type Theme, type ThemeIcon, type ThemePreset, type ThemeTokens } from './themes';

/** The theme cards: one per theme, the active one marked; custom ones can be edited or deleted. */
export function ThemeCards({active, customs, disabled, onChoose, onEdit, onDelete, onCustomize}: {active: string; customs: ThemePreset[]; disabled: boolean; onChoose: (id: string) => void; onEdit: (preset: ThemePreset) => void; onDelete: (id: string) => void; onCustomize: () => void}) {
  return <div className="theme-cards" role="list" aria-label="Themes">
    {allThemes(customs).map(theme => {
      const t = theme.tokens;
      return <div key={theme.id} role="listitem" className={`theme-card ${theme.id === active ? 'active' : ''}`}>
        <button type="button" className="theme-card-pick" disabled={disabled} aria-pressed={theme.id === active} aria-label={`${theme.name} theme`} onClick={() => onChoose(theme.id)}>
          <span className="theme-card-swatches" aria-hidden="true"><i style={{background: t.background}}/><i style={{background: t.studioCard}}/><i style={{background: t.accent}}/><i style={{background: t.primaryBg}}/></span>
          <span className="theme-card-name">{theme.id === active && '✓ '}{theme.name}</span>
        </button>
        {theme.custom && <span className="theme-card-tools"><button type="button" className="tertiary" disabled={disabled} aria-label={`Edit ${theme.name}`} onClick={() => onEdit(customs.find(c => c.id === theme.id)!)}>Edit</button><button type="button" className="tertiary" disabled={disabled} aria-label={`Delete ${theme.name}`} onClick={() => onDelete(theme.id)}>Delete</button></span>}
      </div>;
    })}
    <button type="button" className="theme-card theme-card-new" disabled={disabled} onClick={onCustomize}>+ Customize…</button>
  </div>;
}

/** A color: the picker and the hex code side by side; the hex applies once it is valid. */
function ColorField({label, value, onChange, disabled}: {label: string; value: string; onChange: (hex: string) => void; disabled: boolean}) {
  const [text, setText] = useState(value);
  const [last, setLast] = useState(value);
  if (last !== value) { setLast(value); setText(value); }
  const valid = isHex(text);
  return <label className="color-field">
    <span>{label}</span>
    <input type="color" value={isHex(value) ? value : '#000000'} disabled={disabled} aria-label={`${label} picker`} onChange={event => { setText(event.target.value); onChange(event.target.value); }}/>
    <input type="text" value={text} disabled={disabled} className={valid ? '' : 'invalid'} aria-label={`${label} hex`} spellCheck={false} maxLength={7}
      onChange={event => { const next = event.target.value.trim(); setText(next); if (isHex(next)) onChange(next.toLowerCase()); }}
      onBlur={() => { if (!valid) setText(value); }}/>
  </label>;
}

const GROUPS: [string, (keyof ThemeTokens)[]][] = [
  ['Surfaces', ['background', 'surface', 'surfaceSecondary', 'elevated', 'selected', 'hover', 'studioCard']],
  ['Borders', ['border', 'borderSubtle', 'studioBorder']],
  ['Text', ['text', 'secondary', 'muted']],
  ['Chrome', ['sidebarBg', 'topbarBg']],
  ['Buttons', ['primaryBg', 'primaryText', 'primaryHover', 'focus']],
  ['Accent', ['accent', 'accentStrong', 'accentText', 'accentDim', 'accentSoft', 'accentSoftBorder', 'accentPanel', 'accentPanelBorder', 'accentSelected', 'accentSelectedBorder', 'accentSelectedText', 'accentOutline']],
];
const TOKEN_LABELS: Partial<Record<keyof ThemeTokens, string>> = {
  background: 'Page background', surface: 'Surface', surfaceSecondary: 'Surface 2', elevated: 'Raised', selected: 'Selected', hover: 'Hover', studioCard: 'Card',
  border: 'Border', borderSubtle: 'Border, subtle', studioBorder: 'Card border', text: 'Text color', secondary: 'Secondary text', muted: 'Muted text',
  sidebarBg: 'Sidebar', topbarBg: 'Top bar', primaryBg: 'Primary button', primaryText: 'Primary button text', primaryHover: 'Primary button hover', focus: 'Focus ring',
  accent: 'Accent color', accentStrong: 'Accent, strong', accentText: 'Text on accent', accentDim: 'Accent, dim', accentSoft: 'Accent surface', accentSoftBorder: 'Accent surface border',
  accentPanel: 'Accent panel', accentPanelBorder: 'Accent panel border', accentSelected: 'Pressed', accentSelectedBorder: 'Pressed border', accentSelectedText: 'Pressed text', accentOutline: 'Outline',
};

/** What every token looks like, drawn with the editor's current tokens only. */
export function ThemePreview({tokens, icon}: {tokens: ThemeTokens; icon: ThemeIcon}) {
  return <div className="theme-preview" style={tokensStyle(tokens)} aria-label="Theme preview">
    <div className="theme-preview-sidebar">
      <div className="brand"><span className="brand-mark" aria-hidden="true"><BrandMark theme={icon}/></span><div>Deadlock<span>Mod Builder</span></div></div>
      <button type="button" className="nav-item nav-step nav-done" tabIndex={-1}><span className="nav-step-number">✓</span>Character &amp; model</button>
      <button type="button" className="nav-item nav-step selected" tabIndex={-1}><span className="nav-step-number">2</span>Prepare model</button>
      <button type="button" className="nav-item nav-step" tabIndex={-1}><span className="nav-step-number">3</span>Appearance</button>
    </div>
    <div className="theme-preview-page studio-workspace">
      <header className="page-title"><div><span className="studio-eyebrow">Step 2 of 5</span><h1>Prepare model</h1><p>Your model is fitted to the hero's skeleton.</p></div></header>
      <div className="theme-preview-row"><button type="button" className="primary" tabIndex={-1}>Primary</button><button type="button" tabIndex={-1}>Default</button><button type="button" className="tertiary" tabIndex={-1}>Tertiary</button><span className="prep-workspace"><span className="prep-badge">Badge</span></span><span className="segmented" role="presentation"><button type="button" aria-checked="true" role="radio" tabIndex={-1}>Still</button><button type="button" aria-checked="false" role="radio" tabIndex={-1}>Movement</button></span></div>
      <div className="theme-preview-card"><div className="section-heading"><h2>Your model</h2><span className="muted">BLEND · GLB</span></div><div className="theme-preview-row"><span className="prep-workspace"><button type="button" className="prep-primary" tabIndex={-1}>Prepare automatically</button></span><button type="button" tabIndex={-1}>Load and check only</button></div><label className="checkbox checkbox-line"><input type="checkbox" defaultChecked readOnly tabIndex={-1}/> Keep the hero's cloth</label><p className="hint">Secondary text looks like this; muted text like <span className="muted">this</span>.</p></div>
      <div className="theme-preview-row"><span className="build-completed">Completed</span><span className="prep-workspace"><span className="prep-matched" style={{margin: 0, padding: '4px 10px'}}>✓ Matched</span></span><span className="capability-notice" style={{margin: 0, padding: '4px 10px'}}>Warning</span><span className="statusbar-progress" style={{flex: '1 1 120px', height: 6, background: 'var(--elevated)', borderRadius: 3, overflow: 'hidden'}}><span className="statusbar-progress-fill" style={{display: 'block', width: '60%'}}/></span></div>
    </div>
  </div>;
}

/** The editor: start from any theme, set the three base colors (or any token
 * under Advanced), pick the window icon, name it and save it as a preset. */
export function ThemeCustomizer({start, editing, customs, disabled, onSave, onCancel}: {start: Theme; editing?: ThemePreset; customs: ThemePreset[]; disabled: boolean; onSave: (preset: ThemePreset) => void; onCancel: () => void}) {
  const seed = editing ? themeById(editing.id, customs) : start;
  const [base, setBase] = useState<BaseColors>(seed.base ?? {accent: seed.tokens.accent, background: seed.tokens.background, text: seed.tokens.text});
  const [overrides, setOverrides] = useState<Partial<ThemeTokens>>(() => {
    // A built-in theme keeps its hand-tuned tokens until a base color changes; an
    // edited custom theme keeps whatever its tokens differ from the derivation.
    if (!seed.custom) return {...seed.tokens};
    const derived = deriveTokens(seed.base ?? {accent: seed.tokens.accent, background: seed.tokens.background, text: seed.tokens.text});
    return Object.fromEntries(TOKEN_KEYS.filter(key => String(seed.tokens[key]) !== String(derived[key])).map(key => [key, seed.tokens[key]])) as Partial<ThemeTokens>;
  });
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
  const names = new Set(allThemes(customs).filter(t => t.id !== editing?.id).map(t => t.name.toLowerCase()));
  const trimmed = name.trim();
  const nameProblem = !trimmed ? 'Give the theme a name.' : names.has(trimmed.toLowerCase()) ? 'A theme with this name exists.' : trimmed.length > 40 ? 'Up to 40 characters.' : '';
  const save = () => onSave({id: editing?.id ?? newPresetId(), name: trimmed, icon, base: {accent: base.accent, background: base.background, text: base.text}, tokens: tokensToMap(tokens)});
  return <div className="theme-editor" role="region" aria-label="Theme editor">
    <div className="theme-editor-head">
      <strong>{editing ? `Editing ${editing.name}` : `New theme from ${seed.name}`}</strong>
      <label className="theme-editor-name">Name <input type="text" value={name} maxLength={40} disabled={disabled} aria-label="Theme name" onChange={event => setName(event.target.value)}/></label>
      <button type="button" className="primary" disabled={disabled || Boolean(nameProblem)} onClick={save}>{editing ? 'Save changes' : 'Save as preset'}</button>
      <button type="button" disabled={disabled} onClick={onCancel}>Cancel</button>
      {nameProblem && <span className="muted">{nameProblem}</span>}
    </div>
    <div className="theme-editor-body">
      <div className="theme-editor-fields">
        <h3>Colors</h3>
        <ColorField label="Accent" value={base.accent} disabled={disabled} onChange={hex => setBaseColor('accent', hex)}/>
        <ColorField label="Background" value={base.background} disabled={disabled} onChange={hex => setBaseColor('background', hex)}/>
        <ColorField label="Text" value={base.text} disabled={disabled} onChange={hex => setBaseColor('text', hex)}/>
        <p className="hint">Surfaces, borders, buttons and the accent shades follow these three. Success stays green and errors red.</p>
        <h3>Window icon</h3>
        <div className="theme-icon-choice" role="radiogroup" aria-label="Window icon">{THEMES.map(t => <label key={t.id} className="checkbox"><input type="radio" name="theme-icon" value={t.icon} checked={icon === t.icon} disabled={disabled} onChange={() => setIcon(t.icon)}/>{t.name === 'Original' ? 'Grey D' : t.name}</label>)}</div>
        <details className="technical" open={advanced} onToggle={event => setAdvanced((event.target as HTMLDetailsElement).open)}><summary>Advanced: every color</summary>
          {GROUPS.map(([group, keys]) => <div key={group}><h3>{group}</h3>{keys.map(key => <ColorField key={key} label={TOKEN_LABELS[key] ?? key} value={String(tokens[key])} disabled={disabled} onChange={hex => setToken(key, hex)}/>)}</div>)}
          <h3>Tint</h3>
          <label className="color-field"><span>Hue of the grey shades</span><input type="range" min={0} max={360} value={tokens.hue} disabled={disabled} aria-label="Hue of the grey shades" onChange={event => setToken('hue', event.target.value)}/><output>{tokens.hue}°</output></label>
        </details>
      </div>
      <ThemePreview tokens={tokens} icon={iconOf(icon)}/>
    </div>
  </div>;
}
