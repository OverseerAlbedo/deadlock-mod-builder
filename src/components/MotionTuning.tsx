// i18n-area: physics
import { useEffect, useId, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from 'react';
import { api } from '../api';
import type { MotionTuning, PhysicsPreset, SecondaryMotionChoices } from '../models';
import { m, t, tc, useT } from '../i18n';
type Tuning = Record<string, Record<string, number>>;

export type TuningKind = 'hair'|'streamer'|'tail'|'ear'|'cloth'|'long_skirt'|'accessory'|'breast'|'butt'|'charm';
/** The part types' names (English; shown with `kindName`). The keys are the saved part kinds. */
export const KIND_LABELS: Record<TuningKind, string> = {
  hair: m('Hair'), streamer: m('Streamers and ribbons'), tail: m('Tails'), ear: m('Ears'), cloth: m('Cloth'), long_skirt: m('Long skirts'),
  accessory: m('Accessories'), breast: m('Breasts'), butt: m('Butt'), charm: m('Gun charms'),
};
/** A part type's shown name (a kind the table lacks shows as it is). */
export const kindName = (kind: string) => { const label = KIND_LABELS[kind as TuningKind]; return label ? t(label) : kind; };
/** Names shown as a list ("a, b" in English). */
export const listOf = (names: string[]) => names.reduce((first, next) => tc('list', '{first}, {next}', {first, next}));
/** The four settings: -1 (much less) … 0 (automatic) … 1 (much more). */
const SETTINGS: [keyof MotionTuning, string, string][] = [
  ['stiffness', m('Stiffness'), m('Less swings freely; more holds its shape.')],
  ['damping', m('Damping'), m('More settles faster after a move.')],
  ['gravity', m('Gravity'), m('More hangs and falls; less floats.')],
  ['swing', m('Swing range'), m('How far it may swing. Parts that have no limit stay free.')],
];
const signed = (pct: number) => `${pct > 0 ? '+' : '−'}${Math.abs(pct)}%`;
const percent = (value: number) => value === 0 ? t('Automatic') : signed(Math.round(value * 100));
/** Slider steps, in percent: fine enough to land on a value, coarse enough to read. */
const STEP = 5;
const clampPercent = (value: number) => Math.max(-100, Math.min(100, Math.round(value / STEP) * STEP));
const clampExact = (value: number) => Math.max(-100, Math.min(100, Math.round(value)));

/** How many settings differ from automatic: one part type's values, or every type's. */
export const changedCount = (values: Record<string, number> | undefined) => Object.values(values ?? {}).filter(v => typeof v === 'number' && v !== 0).length;
export const tunedCount = (cfg: SecondaryMotionChoices) => Object.values(cfg.tuning ?? {}).reduce((n, values) => n + changedCount(values), 0);

/** A typed percentage: "37", "+37%", "−37", "auto". Whole percents, clamped to ±100; null when it is not a number. */
export function parsePercent(text: string): number | null {
  const plain = text.trim().replace(/%$/, '').trim().replace(/^[−–]/, '-').replace(/^\+/, '');
  if (/^auto(matic)?$/i.test(plain)) return 0;
  if (!/^-?(\d+(\.\d*)?|\.\d+)$/.test(plain)) return null;
  return clampExact(Number(plain));
}

/** The exact value of one setting: shows Auto or ±n%; while focused it holds the typed text,
 * committed on Enter or when it loses focus (invalid text goes back to the value, Escape too). */
function PercentBox({name, pct, disabled, commit}: {name: string; pct: number; disabled: boolean; commit: (pct: number) => void}) {
  const t = useT();
  const [draft, setDraft] = useState<string | null>(null);
  function done(text: string) {
    setDraft(null);
    const next = parsePercent(text);
    if (next !== null && next !== pct) commit(next);
  }
  return <input className={`tune-box ${pct ? 'tuned' : ''}`} type="text" inputMode="numeric" aria-label={t('{name} in percent', {name})} disabled={disabled} spellCheck={false}
    value={draft ?? (pct ? signed(pct) : t('Auto'))}
    onFocus={event => { setDraft(String(pct)); const box = event.currentTarget; setTimeout(() => box.select()); }}
    onChange={event => setDraft(event.target.value)}
    onBlur={() => { if (draft !== null) done(draft); }}
    onKeyDown={event => {
      if (event.key === 'Enter') { event.preventDefault(); if (draft !== null) done(draft); const box = event.currentTarget; setTimeout(() => box.select()); }
      else if (event.key === 'Escape' && draft !== null) { event.preventDefault(); setDraft(null); }
    }}/>;
}

/** One setting: a long centred slider (automatic in the middle, filled from the middle to the
 * value) and a box for an exact percentage. Dragging and the arrow keys move in 5% steps
 * (Shift + arrow: 1%); a typed value keeps its exact position. Double-click the slider, or
 * click Automatic under it, to go back to automatic. */
export function TuningSlider({kind, label, hint, value, disabled, onChange}: {kind: TuningKind; label: string; hint: string; value: number; disabled: boolean; onChange: (percent: number) => void}) {
  const t = useT();
  const pct = Math.round(value * 100);
  const name = tc('slider name', '{kind} {setting}', {kind: kindName(kind), setting: label.toLowerCase()});
  const lo = Math.min(50, 50 + pct / 2), hi = Math.max(50, 50 + pct / 2);
  const hintId = useId();
  const set = (next: number) => { if (next !== pct) onChange(next); };
  function keys(event: KeyboardEvent<HTMLInputElement>) {
    const fine = event.shiftKey;
    const up = Math.floor(pct / STEP) * STEP + STEP, down = Math.ceil(pct / STEP) * STEP - STEP;
    const next = ({
      ArrowRight: fine ? pct + 1 : up, ArrowUp: fine ? pct + 1 : up,
      ArrowLeft: fine ? pct - 1 : down, ArrowDown: fine ? pct - 1 : down,
      PageUp: Math.floor(pct / 25) * 25 + 25, PageDown: Math.ceil(pct / 25) * 25 - 25,
      Home: -100, End: 100,
    } as Record<string, number>)[event.key];
    if (next === undefined) return;
    event.preventDefault();
    set(clampExact(next));
  }
  return <div className={`tune-row ${pct ? 'tuned' : ''}`}>
    <div className="tune-head"><span className="tune-label">{label}</span><span className="tune-hint" id={hintId}>{hint}</span></div>
    <div className="tune-body">
      <div className="tune-scale">
        <div className="tune-track" style={{'--lo': `${lo}%`, '--hi': `${hi}%`} as CSSProperties}>
          <span className="tune-rail" aria-hidden="true"><span className="tune-fill"/><span className="tune-detent"/></span>
          <input type="range" min={-100} max={100} step={1} value={pct} disabled={disabled} aria-label={name} aria-valuetext={percent(value)} aria-describedby={hintId}
            onChange={event => set(clampPercent(Number(event.target.value)))} onKeyDown={keys} onDoubleClick={() => set(0)}/>
        </div>
        <div className="tune-ticks"><span>−100%</span>
          {pct ? <button type="button" className="tune-auto" disabled={disabled} aria-label={t('{name}: back to automatic', {name})} title={t('Back to automatic')} onClick={() => set(0)}><span aria-hidden="true">↺ </span>{t('Automatic')}</button> : <span className="tune-auto-label">{t('Automatic')}</span>}
          <span>+100%</span></div>
      </div>
      <PercentBox name={name} pct={pct} disabled={disabled} commit={set}/>
    </div>
  </div>;
}

/** The project's tuning with one part type's values changed (zeros and empty types removed). */
export function withTuning(current: Tuning | undefined, kind: TuningKind, values: MotionTuning | null): Tuning {
  const next = {...current};
  const kept = Object.fromEntries(Object.entries(values ?? {}).filter(([, v]) => typeof v === 'number' && v !== 0)) as Record<string, number>;
  if (Object.keys(kept).length) next[kind] = kept; else delete next[kind];
  return next;
}

/** Stiffness, damping, gravity and swing range for one part type, starting at the automatic values. */
export function TuningSliders({kind, cfg, update, disabled}: {kind: TuningKind; cfg: SecondaryMotionChoices; update: (choice: Partial<SecondaryMotionChoices>) => void; disabled: boolean}) {
  const t = useT();
  const values: MotionTuning = cfg.tuning?.[kind] ?? {};
  return <fieldset className="tune-sliders" aria-label={t('Fine-tune {kind}', {kind: kindName(kind)})}>
    {SETTINGS.map(([key, label, hint]) => <TuningSlider key={key} kind={kind} label={t(label)} hint={t(hint)} value={values[key] ?? 0} disabled={disabled}
      onChange={pct => update({tuning: withTuning(cfg.tuning, kind, {...values, [key]: pct / 100})})}/>)}
  </fieldset>;
}

/** A header button that opens a small panel below it. Escape closes it and returns focus to the
 * button; a click outside closes it; the panel's first control takes focus when it opens. */
export function ToolPopover({label, title, disabled, children}: {label: string; title: string; disabled?: boolean; children: (close: () => void) => ReactNode}) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null), panel = useRef<HTMLDivElement>(null);
  const id = useId();
  const close = () => { setOpen(false); button.current?.focus(); };
  useEffect(() => {
    if (!open) return;
    panel.current?.querySelector<HTMLElement>('input:not(:disabled), button:not(:disabled), select:not(:disabled)')?.focus();
    const outside = (event: MouseEvent) => { const target = event.target as Node; if (!panel.current?.contains(target) && !button.current?.contains(target)) setOpen(false); };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [open]);
  return <span className="tool-popover" onKeyDown={event => { if (event.key === 'Escape' && open) { event.preventDefault(); event.stopPropagation(); close(); } }}>
    <button ref={button} type="button" className="compact tool-popover-button" aria-expanded={open} aria-controls={id} disabled={disabled} onClick={() => setOpen(!open)}>{label}<span aria-hidden="true" className="tool-popover-caret">▾</span></button>
    {open && <div ref={panel} id={id} className="tool-popover-panel" role="dialog" aria-label={title}>{children(close)}</div>}
  </span>;
}

/** A presets message as it is kept: the app's own English sentence with its values (drawn translated), or the
 * backend's words (`backend`), drawn as they came. Kept this way, a message follows a change of language. */
export type PresetMessage = {text: string; values?: Record<string, string>; backend?: boolean};
/** The message as it is drawn. */
export const shownPresetMessage = (message: PresetMessage) => message.backend ? message.text : t(message.text, message.values);

/** Save the current fine-tuning under a name, or apply one saved on this PC (by part type, so it fits any model).
 * Two header buttons: Saved presets (apply or delete each) and Save as preset (name and Save). Messages go to
 * `report` (shown under the header) or, without it, below the buttons. */
export function PhysicsPresets({cfg, update, disabled, report}: {cfg: SecondaryMotionChoices; update: (choice: Partial<SecondaryMotionChoices>) => void; disabled: boolean; report?: (message: PresetMessage | null) => void}) {
  const t = useT();
  const [presets, setPresets] = useState<PhysicsPreset[] | null>(null);
  const [name, setName] = useState('');
  const [own, setOwn] = useState<PresetMessage | null>(null);
  const say = report ?? setOwn;
  useEffect(() => {
    let active = true;
    void Promise.resolve().then(() => api.physicsPresets()).then(list => { if (active) setPresets(list ?? []); }).catch(() => { if (active) setPresets(null); });
    return () => { active = false; };
  }, []);
  if (presets === null) return null;
  const tuning = cfg.tuning ?? {};
  const empty = !Object.keys(tuning).length, unnamed = !name.trim();
  async function save(close: () => void) {
    if (disabled || empty || unnamed) return;
    try { setPresets(await api.savePhysicsPreset(name.trim(), tuning)); say({text: m('Saved “{name}”.'), values: {name: name.trim()}}); setName(''); close(); }
    catch (error) { say(typeof error === 'object' && error && 'details' in error ? {text: String((error as {details: unknown}).details), backend: true} : {text: m('The preset could not be saved.')}); }
  }
  async function remove(preset: string) {
    try { setPresets(await api.deletePhysicsPreset(preset)); say({text: m('Deleted “{name}”.'), values: {name: preset}}); } catch { say({text: m('The preset could not be deleted.')}); }
  }
  return <>
    <ToolPopover label={t('Saved presets')} title={t('Saved presets')}>{close => presets.length
      ? <ul className="physics-preset-list">{presets.map(p => <li key={p.name}><span><strong>{p.name}</strong><small>{Object.keys(p.tuning).length ? listOf(Object.keys(p.tuning).map(kindName)) : t('Automatic')}</small></span>
        <button type="button" className="compact" disabled={disabled} aria-label={t('Apply {name}', {name: p.name})} onClick={() => { update({tuning: p.tuning}); say({text: m('Applied “{name}”. Analyze and Build to use it.'), values: {name: p.name}}); close(); }}>{t('Apply')}</button>
        <button type="button" className="tertiary compact" disabled={disabled} aria-label={t('Delete {name}', {name: p.name})} onClick={() => void remove(p.name)}>{t('Delete')}</button></li>)}</ul>
      : <p className="hint">{t('No saved presets yet. Save as preset keeps your slider values on this PC for any model.')}</p>}</ToolPopover>
    <ToolPopover label={t('Save as preset')} title={t('Save as preset')}>{close => <form className="physics-preset-save" onSubmit={event => { event.preventDefault(); void save(close); }}>
      <input className="compact" aria-label={t('Preset name')} placeholder={t('Preset name')} maxLength={40} value={name} disabled={disabled} onChange={event => setName(event.target.value)}/>
      <button type="submit" className="compact primary" disabled={disabled || empty || unnamed}>{t('Save')}</button>
      {(empty || unnamed) && <p className="hint">{empty ? t('Nothing to save yet: every slider is at automatic.') : t('Type a name to save.')}</p>}
    </form>}</ToolPopover>
    {!report && own && <p className="hint physics-preset-message" role="status">{shownPresetMessage(own)}</p>}
  </>;
}
