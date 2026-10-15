import { useEffect, useState, type CSSProperties } from 'react';
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
  ['swing', 'Swing range', 'How far it may swing. Parts that already swing freely stay free.'],
];
const percent = (value: number) => value === 0 ? 'Automatic' : `${value > 0 ? '+' : '−'}${Math.round(Math.abs(value) * 100)}%`;
/** Slider steps, in percent: fine enough to land on a value, coarse enough to read. */
const STEP = 5;
const clampPercent = (value: number) => Math.max(-100, Math.min(100, Math.round(value / STEP) * STEP));

/** One setting: a centred slider (automatic in the middle, filled toward the
 * value), − and + steps, the value as a percentage; double-click the slider
 * to go back to automatic. */
function TuningSlider({kind, label, hint, value, disabled, onChange}: {kind: TuningKind; label: string; hint: string; value: number; disabled: boolean; onChange: (percent: number) => void}) {
  const pct = Math.round(value * 100);
  const lo = Math.min(50, 50 + pct / 2), hi = Math.max(50, 50 + pct / 2);
  return <div className={`tuning-slider ${pct ? 'tuned' : ''}`}>
    <div className="tuning-slider-head"><span className="tuning-slider-label">{label}</span><small className="tuning-slider-hint">{hint}</small></div>
    <div className="tuning-slider-track">
      <button type="button" className="tuning-step" aria-label={`${label} less`} disabled={disabled || pct <= -100} onClick={() => onChange(clampPercent(pct - STEP))}>−</button>
      <span className="tuning-slider-range"><input type="range" min={-100} max={100} step={STEP} value={pct} disabled={disabled} aria-label={`${KIND_LABELS[kind]} ${label.toLowerCase()}`} aria-valuetext={percent(value)}
        style={{'--lo': `${lo}%`, '--hi': `${hi}%`} as CSSProperties} onChange={event => onChange(clampPercent(Number(event.target.value)))} onDoubleClick={() => onChange(0)}/></span>
      <button type="button" className="tuning-step" aria-label={`${label} more`} disabled={disabled || pct >= 100} onClick={() => onChange(clampPercent(pct + STEP))}>+</button>
      <output className={pct ? 'tuned' : ''} aria-live="polite">{percent(value)}</output>
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
  const changed = SETTINGS.some(([key]) => (values[key] ?? 0) !== 0);
  return <fieldset className="motion-tuning" aria-label={`Fine-tune ${KIND_LABELS[kind]}`}>
    <legend>{KIND_LABELS[kind]}{changed && <button type="button" className="tertiary" disabled={disabled} onClick={() => update({tuning: withTuning(cfg.tuning, kind, null)})}>Reset</button>}</legend>
    {SETTINGS.map(([key, label, hint]) => <TuningSlider key={key} kind={kind} label={label} hint={hint} value={values[key] ?? 0} disabled={disabled}
      onChange={pct => update({tuning: withTuning(cfg.tuning, kind, {...values, [key]: pct / 100})})}/>)}
  </fieldset>;
}

/** Save the current fine-tuning under a name, or apply one saved on this PC (by part type, so it fits any model). */
export function PhysicsPresets({cfg, update, disabled}: {cfg: SecondaryMotionChoices; update: (choice: Partial<SecondaryMotionChoices>) => void; disabled: boolean}) {
  const [presets, setPresets] = useState<PhysicsPreset[] | null>(null);
  const [name, setName] = useState('');
  const [message, setMessage] = useState('');
  useEffect(() => {
    let active = true;
    void Promise.resolve().then(() => api.physicsPresets()).then(list => { if (active) setPresets(list ?? []); }).catch(() => { if (active) setPresets(null); });
    return () => { active = false; };
  }, []);
  if (presets === null) return null;
  const tuning = cfg.tuning ?? {};
  async function save() {
    try { setPresets(await api.savePhysicsPreset(name.trim(), tuning)); setMessage(`Saved “${name.trim()}”.`); setName(''); }
    catch (error) { setMessage(typeof error === 'object' && error && 'details' in error ? String((error as {details: unknown}).details) : 'The preset could not be saved.'); }
  }
  async function remove(preset: string) {
    try { setPresets(await api.deletePhysicsPreset(preset)); setMessage(`Deleted “${preset}”.`); } catch { setMessage('The preset could not be deleted.'); }
  }
  return <div className="physics-presets">
    <div className="row">
      {presets.length > 0 && <select aria-label="Apply a saved physics preset" disabled={disabled} value="" onChange={event => { const preset = presets.find(p => p.name === event.target.value); if (preset) { update({tuning: preset.tuning}); setMessage(`Applied “${preset.name}”. Analyze and Build to use it.`); } }}>
        <option value="">Apply a saved preset…</option>
        {presets.map(p => <option key={p.name} value={p.name}>{p.name}</option>)}
      </select>}
      <input aria-label="Preset name" placeholder="Preset name" maxLength={40} value={name} disabled={disabled} onChange={event => setName(event.target.value)}/>
      <button type="button" disabled={disabled || !name.trim() || !Object.keys(tuning).length} onClick={() => void save()}>Save as preset</button>
    </div>
    {presets.length > 0 && <details className="technical"><summary>Saved presets</summary><ul className="physics-preset-list">{presets.map(p => <li key={p.name}><span>{p.name}</span><small>{Object.keys(p.tuning).map(k => KIND_LABELS[k as TuningKind] ?? k).join(', ') || 'Automatic'}</small><button type="button" className="tertiary" disabled={disabled} onClick={() => void remove(p.name)}>Delete</button></li>)}</ul></details>}
    {message && <p className="hint" role="status">{message}</p>}
  </div>;
}
