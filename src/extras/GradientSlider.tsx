/** The Colors page's sliders: a rounded gradient track that shows what the value does, a round
 * white-ringed handle and a box for an exact value. One or two handles (a range). Built on
 * native range inputs, so mouse, touch and screen readers work as for any slider; the keys are
 * the page's own: arrows move one step, Shift + arrow the fine step, Page Up/Down ten steps,
 * Home/End the ends. A typed value keeps its exact place; dragging snaps to the step. */
import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import { t } from "../i18n";
// i18n-area: colors

export interface Unit {
  /** The value as shown in the box and read out (aria-valuetext). */
  format: (value: number) => string;
  /** A typed value, or null when it is not one. */
  parse: (text: string) => number | null;
}
const number = (text: string) => {
  const plain = text.trim().replace(/[%°]$/, "").trim().replace(/^[−–]/, "-").replace(/^\+/, "");
  return /^-?(\d+(\.\d*)?|\.\d+)$/.test(plain) ? Number(plain) : null;
};
/** A factor shown in percent (1 = 100%). */
export const percentUnit: Unit = {
  format: (value) => `${Math.round(value * 100)}%`,
  parse: (text) => { const n = number(text); return n === null ? null : n / 100; },
};
/** Degrees, unsigned (0–360). */
export const degreeUnit: Unit = { format: (value) => `${Math.round(value)}°`, parse: number };
/** Degrees with a sign (a hue shift). */
export const shiftUnit: Unit = { format: (value) => `${value > 0 ? "+" : value < 0 ? "−" : ""}${Math.abs(Math.round(value))}°`, parse: number };
/** A plain whole number (an RGB channel). */
export const wholeUnit: Unit = { format: (value) => String(Math.round(value)), parse: number };

const decimals = (step: number) => Math.max(0, Math.ceil(-Math.log10(step) - 1e-9));
const round = (value: number, step: number) => Number((Math.round(value / step) * step).toFixed(decimals(step)));
const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
/** The value a key moves to, or null for other keys. */
export function keyValue(key: string, shift: boolean, value: number, min: number, max: number, step: number, fine: number): number | null {
  const by = shift ? fine : step;
  const eps = by / 1000;
  const next: Record<string, number> = {
    ArrowRight: Math.floor((value + eps) / by) * by + by,
    ArrowUp: Math.floor((value + eps) / by) * by + by,
    ArrowLeft: Math.ceil((value - eps) / by) * by - by,
    ArrowDown: Math.ceil((value - eps) / by) * by - by,
    PageUp: value + step * 10,
    PageDown: value - step * 10,
    Home: min,
    End: max,
  };
  return key in next ? clamp(round(next[key], fine), min, max) : null;
}

/** "20–100%", "20 - 100", "20 to 100" → ["20", "100%"]. */
export function splitRange(text: string): string[] {
  return text
    .replace(/[–—]|\bto\b|\.\./g, "|")
    .replace(/(\d\s*[%°]?)\s*-\s*(?=[\d.])/, "$1|")
    .split("|")
    .map((part) => part.trim())
    .filter(Boolean);
}

/** Calls `onDrag(true)` on a pointer press and `onDrag(false)` when it is let go anywhere. */
function useDrag(onDrag?: (active: boolean) => void) {
  const dragging = useRef(false);
  const latest = useRef(onDrag);
  latest.current = onDrag;
  useEffect(() => () => { if (dragging.current) latest.current?.(false); }, []);
  return {
    dragging,
    onPointerDown() {
      if (dragging.current) return;
      dragging.current = true;
      latest.current?.(true);
      const up = () => {
        dragging.current = false;
        window.removeEventListener("pointerup", up);
        window.removeEventListener("pointercancel", up);
        latest.current?.(false);
      };
      window.addEventListener("pointerup", up);
      window.addEventListener("pointercancel", up);
    },
  };
}

/** The exact value box: holds the typed text while focused; Enter or leaving commits it,
 * Escape (or text that is not a value) puts the value back. */
export function ValueBox({ label, text, disabled, commit }: { label: string; text: string; disabled: boolean; commit: (text: string) => boolean }) {
  const [draft, setDraft] = useState<string | null>(null);
  const done = (value: string) => { setDraft(null); commit(value); };
  return (
    <input
      className="gradient-value"
      type="text"
      inputMode="decimal"
      aria-label={label}
      disabled={disabled}
      spellCheck={false}
      value={draft ?? text}
      onFocus={(event) => { setDraft(text); const box = event.currentTarget; setTimeout(() => box.select()); }}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => { if (draft !== null) done(draft); }}
      onKeyDown={(event) => {
        if (event.key === "Enter") { event.preventDefault(); if (draft !== null) done(draft); setDraft(null); }
        else if (event.key === "Escape" && draft !== null) { event.preventDefault(); setDraft(null); }
      }}
    />
  );
}

interface Common {
  /** The visible label (also the accessible name unless `name` is given). */
  label: string;
  name?: string;
  min: number;
  max: number;
  /** Dragging and arrow keys. */
  step: number;
  /** Shift + arrow, and the precision of typed values. */
  fine?: number;
  unit: Unit;
  /** CSS background of the track (the gradient that shows what the value does). */
  track: string;
  disabled?: boolean;
  /** A drag started (true) or ended (false): the page previews at a lower size meanwhile. */
  onDrag?: (active: boolean) => void;
  /** The value was set by something other than a drag step (key, typed, the end of a drag). */
  onCommit?: () => void;
  className?: string;
}
const fraction = (value: number, min: number, max: number) => (max > min ? (clamp(value, min, max) - min) / (max - min) : 0);

/** One value on a gradient track. `band` draws a ringed band (a hue ± width) under the handle. */
export function GradientSlider({ label, name, value, onChange, handle, band, min, max, step, fine = step, unit, track, disabled = false, onDrag, onCommit, className = "" }: Common & {
  value: number; onChange: (value: number) => void;
  /** The handle's fill (the color the value stands for). */
  handle?: string;
  /** Fractions 0–1 of the track; `from` above `to` wraps around the end. */
  band?: { from: number; to: number } | null;
}) {
  const accessible = name ?? label;
  const drag = useDrag(onDrag);
  const set = (next: number, commit = false) => {
    const exact = clamp(round(next, fine), min, max);
    if (exact !== value) onChange(exact);
    if (commit) onCommit?.();
  };
  function keys(event: KeyboardEvent<HTMLInputElement>) {
    const next = keyValue(event.key, event.shiftKey, value, min, max, step, fine);
    if (next === null) return;
    event.preventDefault();
    set(next, true);
  }
  const style = { "--track": track, "--handle": handle ?? "transparent", "--at": fraction(value, min, max) } as CSSProperties;
  const bands = band ? (band.from <= band.to ? [[band.from, band.to]] : [[band.from, 1], [0, band.to]]) : [];
  return (
    <div className={`gradient-slider ${disabled ? "disabled" : ""} ${className}`}>
      <span className="gradient-label" aria-hidden="true">{label}</span>
      <div className="gradient-track" style={style}>
        {bands.map(([from, to], index) => (
          <span key={index} className="gradient-band" aria-hidden="true" style={{ "--from": from, "--to": to } as CSSProperties} />
        ))}
        <input
          type="range"
          min={min}
          max={max}
          step={fine}
          value={value}
          disabled={disabled}
          aria-label={accessible}
          aria-valuetext={unit.format(value)}
          onPointerDown={drag.onPointerDown}
          onChange={(event) => {
            const raw = Number(event.target.value);
            set(drag.dragging.current ? round(raw, step) : raw);
          }}
          onKeyDown={keys}
          onKeyUp={(event) => { if (event.key.startsWith("Arrow")) onCommit?.(); }}
        />
      </div>
      <ValueBox
        label={t("{name} value", { name: accessible })}
        text={unit.format(value)}
        disabled={disabled}
        commit={(text) => {
          const parsed = unit.parse(text);
          if (parsed === null || !Number.isFinite(parsed)) return false;
          set(parsed, true);
          return true;
        }}
      />
    </div>
  );
}

/** A range on a gradient track: two handles (from, to) with the band between them ringed. The
 * handles never cross: moving one past the other moves both. */
export function GradientRange({ label, name, rangeLabel, values, onChange, handles, min, max, step, fine = step, unit, track, disabled = false, onDrag, onCommit, className = "" }: Common & {
  values: [number, number]; onChange: (values: [number, number]) => void;
  handles?: [string, string];
  /** Added to the value box's name ("Range 1 saturation range value"). */
  rangeLabel?: string;
}) {
  const accessible = name ?? label;
  const drag = useDrag(onDrag);
  const set = (side: 0 | 1, next: number, commit = false) => {
    const exact = clamp(round(next, fine), min, max);
    const out: [number, number] = [...values];
    out[side] = exact;
    if (out[0] > out[1]) out[1 - side] = exact;
    if (out[0] !== values[0] || out[1] !== values[1]) onChange(out);
    if (commit) onCommit?.();
  };
  const style = { "--track": track, "--lo": fraction(values[0], min, max), "--hi": fraction(values[1], min, max) } as CSSProperties;
  const input = (side: 0 | 1) => (
    <input
      key={side}
      type="range"
      className={side ? "upper" : "lower"}
      min={min}
      max={max}
      step={fine}
      value={values[side]}
      disabled={disabled}
      aria-label={side ? t("{name} to", { name: accessible }) : t("{name} from", { name: accessible })}
      aria-valuetext={unit.format(values[side])}
      style={{ "--handle": handles?.[side] ?? "transparent", zIndex: (side === 0) === lowerOnTop ? 2 : 1 } as CSSProperties}
      onPointerDown={drag.onPointerDown}
      onChange={(event) => {
        const raw = Number(event.target.value);
        set(side, drag.dragging.current ? round(raw, step) : raw);
      }}
      onKeyDown={(event) => {
        const next = keyValue(event.key, event.shiftKey, values[side], min, max, step, fine);
        if (next === null) return;
        event.preventDefault();
        set(side, next, true);
      }}
      onKeyUp={(event) => { if (event.key.startsWith("Arrow")) onCommit?.(); }}
    />
  );
  const text = `${unit.format(values[0]).replace(/[%°]$/, "")}–${unit.format(values[1])}`;
  // Keep the upper handle reachable when both sit at the minimum.
  const lowerOnTop = values[0] >= (min + max) / 2;
  return (
    <div className={`gradient-slider two ${disabled ? "disabled" : ""} ${className}`}>
      <span className="gradient-label" aria-hidden="true">{label}</span>
      <div className="gradient-track" style={style}>
        <span className="gradient-span" aria-hidden="true" />
        {input(0)}{input(1)}
      </div>
      <ValueBox
        label={rangeLabel ? t("{name} {range} value", { name: accessible, range: t(rangeLabel) }) : t("{name} value", { name: accessible })}
        text={text}
        disabled={disabled}
        commit={(typed) => {
          const parsed = splitRange(typed).map(unit.parse);
          if (parsed.length !== 2 || parsed.some((v) => v === null || !Number.isFinite(v))) return false;
          const [a, b] = parsed as [number, number];
          const low = clamp(round(Math.min(a, b), fine), min, max), high = clamp(round(Math.max(a, b), fine), min, max);
          if (low !== values[0] || high !== values[1]) onChange([low, high]);
          onCommit?.();
          return true;
        }}
      />
    </div>
  );
}
