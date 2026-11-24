import { useRef, type KeyboardEvent, type ReactNode } from 'react';
import type { StudioTone } from './Studio';
import './studioChoices.css';

/** One chip of a chip strip: its name, its current choice and a tone for that choice. */
export interface StudioChip<T extends string> { id: T; label: string; detail?: string; tone?: StudioTone }
const chipId = (base: string, id: string) => `${base}-chip-${id}`;
/** The one panel all chips control (only the open group is rendered in it). */
export const chipPanelId = (base: string) => `${base}-chip-panel`;

/** A chip strip that opens one group at a time (a tab list: arrow keys, Home and End move between
 * the chips). Each chip shows its group's current choice. */
export function StudioChips<T extends string>({label, base, chips, active, onSelect}: {label: string; base: string; chips: StudioChip<T>[]; active: T; onSelect: (id: T) => void}) {
  const refs = useRef(new Map<string, HTMLButtonElement>());
  function keys(event: KeyboardEvent<HTMLDivElement>) {
    const at = chips.findIndex(chip => chip.id === active);
    const next = ({ArrowRight: at + 1, ArrowLeft: at - 1, Home: 0, End: chips.length - 1} as Record<string, number>)[event.key];
    if (next === undefined) return;
    event.preventDefault(); event.stopPropagation();
    const chip = chips[(next + chips.length) % chips.length];
    onSelect(chip.id); refs.current.get(chip.id)?.focus();
  }
  return <div className="studio-chips" role="tablist" aria-label={label} onKeyDown={keys}>
    {chips.map(chip => <button key={chip.id} type="button" role="tab" id={chipId(base, chip.id)} aria-controls={chipPanelId(base)} aria-selected={chip.id === active} tabIndex={chip.id === active ? 0 : -1}
      ref={element => { if (element) refs.current.set(chip.id, element); else refs.current.delete(chip.id); }} className={`studio-chip ${chip.tone ?? ''}`} onClick={() => onSelect(chip.id)}>
      {chip.tone === 'attention' && <span className="studio-chip-mark" aria-hidden="true"/>}<span>{chip.label}</span>{chip.detail && <>{' '}<small>{chip.detail}</small></>}
    </button>)}
  </div>;
}

/** The open group of a chip strip. Only the open group is rendered. */
export function StudioChipPanel({base, id, children, className = ''}: {base: string; id: string; children: ReactNode; className?: string}) {
  return <div role="tabpanel" id={chipPanelId(base)} aria-labelledby={chipId(base, id)} className={`studio-chip-panel ${className}`}>{children}</div>;
}

/** One option card: a title, an optional badge (`default`, `recommended…`, a caution) and a line
 * of help. */
export interface StudioOption<T extends string> { value: T; title: string; badge?: string; badgeTone?: 'ok' | 'quiet' | 'attention' | 'accent'; help?: ReactNode }

/** Option cards instead of a drop-down: a radio group with the drop-down's name (`label`). With
 * no value chosen, no card is checked. */
export function StudioOptionCards<T extends string>({label, name, options, value, onChange, disabled}: {label: string; name: string; options: StudioOption<T>[]; value: T | ''; onChange: (value: T) => void; disabled?: boolean}) {
  return <div className="studio-options" role="radiogroup" aria-label={label}>
    {options.map(option => <label key={option.value} className={`studio-option ${option.value === value ? 'selected' : ''} ${disabled ? 'disabled' : ''}`}>
      <input type="radio" name={name} value={option.value} checked={option.value === value} disabled={disabled} onChange={() => onChange(option.value)}/>
      <span className="studio-option-text">
        <span className="studio-option-title"><strong>{option.title}</strong>{option.badge && <small className={`studio-option-badge ${option.badgeTone ?? 'accent'}`}>{option.badge}</small>}</span>
        {option.help && <span className="studio-option-help">{option.help}</span>}
      </span>
    </label>)}
  </div>;
}
