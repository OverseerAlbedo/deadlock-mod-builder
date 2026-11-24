import { useEffect, useId, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from 'react';
import { api } from '../api';
import type { MotionTuning, PhysicsPreset, SecondaryMotionChoices } from '../models';
type Tuning = Record<string, Record<string, number>>;

export type TuningKind = 'hair'|'streamer'|'tail'|'ear'|'cloth'|'long_skirt'|'accessory'|'breast'|'butt'|'charm';
export const KIND_LABELS: Record<TuningKind, string> = {
  hair: 'Hair', streamer: 'Streamers and ribbons', tail: 'Tails', ear: 'Ears', cloth: 'Cloth', long_skirt: 'Long skirts',
  accessory: 'Accessories', breast: 'Breasts', butt: 'Butt', charm: 'Gun charms',
};
/** The four settings: -1 (much less) … 0 (automatic) … 1 (much more). */
const SETTINGS: [keyof MotionTuning, string, string][] = [
  ['stiffness', 'Stiffness', 'Less swings freely; more holds its shape.'],
  ['damping', 'Damping', 'More settles faster after a move.'],
  ['gravity', 'Gravity', 'More hangs and falls; less floats.'],
  ['swing', 'Swing range', 'How far it may swing. Parts that have no limit stay free.'],
];
const signed = (pct: number) => `${pct > 0 ? '+' : '−'}${Math.abs(pct)}%`;
const percent = (value: number) => value === 0 ? 'Automatic' : signed(Math.round(value * 100));
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
  const [draft, setDraft] = useState<string | null>(null);
  function done(text: string) {
    setDraft(null);
    const next = parsePercent(text);
    if (next !== null && next !== pct) commit(next);
  }
  return <input className={`tune-box ${pct ? 'tuned' : ''}`} type="text" inputMode="numeric" aria-label={`${name} in percent`} disabled={disabled} spellCheck={false}
    value={draft ?? (pct ? signed(pct) : 'Auto')}
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
  const pct = Math.round(value * 100);
  const name = `${KIND_LABELS[kind]} ${label.toLowerCase()}`;
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
          {pct ? <button type="button" className="tune-auto" disabled={disabled} aria-label={`${name}: back to automatic`} title="Back to automatic" onClick={() => set(0)}><span aria-hidden="true">↺ </span>Automatic</button> : <span className="tune-auto-label">Automatic</span>}
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
  const values: MotionTuning = cfg.tuning?.[kind] ?? {};
  return <fieldset className="tune-sliders" aria-label={`Fine-tune ${KIND_LABELS[kind]}`}>
    {SETTINGS.map(([key, label, hint]) => <TuningSlider key={key} kind={kind} label={label} hint={hint} value={values[key] ?? 0} disabled={disabled}
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

/** Save the current fine-tuning under a name, or apply one saved on this PC (by part type, so it fits any model).
 * Two header buttons: Saved presets (apply or delete each) and Save as preset (name and Save). Messages go to
 * `report` (shown under the header) or, without it, below the buttons. */
export function PhysicsPresets({cfg, update, disabled, report}: {cfg: SecondaryMotionChoices; update: (choice: Partial<SecondaryMotionChoices>) => void; disabled: boolean; report?: (message: string) => void}) {
  const [presets, setPresets] = useState<PhysicsPreset[] | null>(null);
  const [name, setName] = useState('');
  const [own, setOwn] = useState('');
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
    try { setPresets(await api.savePhysicsPreset(name.trim(), tuning)); say(`Saved “${name.trim()}”.`); setName(''); close(); }
    catch (error) { say(typeof error === 'object' && error && 'details' in error ? String((error as {details: unknown}).details) : 'The preset could not be saved.'); }
  }
  async function remove(preset: string) {
    try { setPresets(await api.deletePhysicsPreset(preset)); say(`Deleted “${preset}”.`); } catch { say('The preset could not be deleted.'); }
  }
  return <>
    <ToolPopover label="Saved presets" title="Saved presets">{close => presets.length
      ? <ul className="physics-preset-list">{presets.map(p => <li key={p.name}><span><strong>{p.name}</strong><small>{Object.keys(p.tuning).map(k => KIND_LABELS[k as TuningKind] ?? k).join(', ') || 'Automatic'}</small></span>
        <button type="button" className="compact" disabled={disabled} aria-label={`Apply ${p.name}`} onClick={() => { update({tuning: p.tuning}); say(`Applied “${p.name}”. Analyze and Build to use it.`); close(); }}>Apply</button>
        <button type="button" className="tertiary compact" disabled={disabled} aria-label={`Delete ${p.name}`} onClick={() => void remove(p.name)}>Delete</button></li>)}</ul>
      : <p className="hint">No saved presets yet. Save as preset keeps your slider values on this PC for any model.</p>}</ToolPopover>
    <ToolPopover label="Save as preset" title="Save as preset">{close => <form className="physics-preset-save" onSubmit={event => { event.preventDefault(); void save(close); }}>
      <input className="compact" aria-label="Preset name" placeholder="Preset name" maxLength={40} value={name} disabled={disabled} onChange={event => setName(event.target.value)}/>
      <button type="submit" className="compact primary" disabled={disabled || empty || unnamed}>Save</button>
      {(empty || unnamed) && <p className="hint">{empty ? 'Nothing to save yet: every slider is at automatic.' : 'Type a name to save.'}</p>}
    </form>}</ToolPopover>
    {!report && own && <p className="hint physics-preset-message" role="status">{own}</p>}
  </>;
}
