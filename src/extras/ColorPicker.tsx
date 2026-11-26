/** The tint picker: a wide swatch (the game's or the material's own color | yours), a hex box,
 * HSB or RGB sliders on gradient tracks, quick buttons and the recent colors. */
import { useEffect, useId, useRef, useState } from "react";
import { GradientSlider, percentUnit, degreeUnit, wholeUnit } from "./GradientSlider";
import { cssColor, fromHsb, keepHsb, parseHex, recentColors, rememberColor, textOn, toHex, type Hsb } from "./colorMath";
import type { Rgb } from "./colorAdapter";
import { m, useT } from "../i18n";
// i18n-area: colors

/** The label pill on a swatch: dark text on a light veil over light colors, white on a dark veil otherwise. */
const pill = (color: Rgb) => ({ background: textOn(color) === "#000" ? "rgb(255 255 255 / 0.6)" : "rgb(0 0 0 / 0.5)" });
export const HUE_TRACK = "linear-gradient(90deg, #f00 0%, #ff0 16.67%, #0f0 33.33%, #0ff 50%, #00f 66.67%, #f0f 83.33%, #f00 100%)";

/** The channel names of the RGB sliders. */
const CHANNELS = [m("Red"), m("Green"), m("Blue")] as const;
export function ColorPicker({ label, value, original, originalLabel, changed, disabled, onChange, onReset, onDrag, quick = true, yoursLabel }: {
  /** English (marked with `m`); shown in the language shown, also inside the picker's own names. */
  label: string;
  /** The color shown as yours (the saved tint, or the original when none is saved). */
  value: Rgb;
  original: Rgb;
  /** "Game" for a hero material, "Original" for the replacement's. */
  originalLabel: string;
  changed: boolean;
  disabled: boolean;
  onChange: (color: Rgb) => void;
  /** Back to the original color (the saved tint is removed). */
  onReset: () => void;
  onDrag?: (active: boolean) => void;
  /** Show Black and "<original> color" (false for the new color of a picked range). */
  quick?: boolean;
  /** The label on your half of the swatch (default "Yours"). */
  yoursLabel?: string;
}) {
  const t = useT();
  const shown = t(label);
  const recentId = useId();
  const [mode, setMode] = useState<"hsb" | "rgb">("hsb");
  const [hsb, setHsb] = useState<Hsb>(() => keepHsb(value, null));
  const [hexDraft, setHexDraft] = useState<string | null>(null);
  const [recent, setRecent] = useState<string[]>(recentColors);
  const [copied, setCopied] = useState(false);
  // Follow the color when it changes from outside (another material, Reset, a recent color).
  useEffect(() => setHsb((current) => keepHsb(value, current)), [value]);
  const hex = toHex(value);
  const remember = (color: Rgb) => setRecent(rememberColor(color));
  // The color last set here (the props catch up a render later; a commit right after a change must remember the new one).
  const latest = useRef(value);
  latest.current = value;
  const set = (color: Rgb, commit = false) => {
    latest.current = color;
    onChange(color);
    if (commit) remember(color);
  };
  const setFromHsb = (next: Hsb) => {
    setHsb(next);
    latest.current = fromHsb(next);
    onChange(latest.current);
  };
  const drag = (active: boolean) => {
    onDrag?.(active);
    if (!active) remember(latest.current);
  };
  const [h, s, b] = hsb;
  function commitHex(text: string) {
    setHexDraft(null);
    const parsed = parseHex(text);
    if (parsed) set(parsed, true);
  }
  async function copy() {
    try {
      await navigator.clipboard?.writeText(hex);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* no clipboard: nothing to do */
    }
  }
  const sliders = mode === "hsb"
    ? [
        <GradientSlider key="h" label={t("Hue")} name={t("{label} hue", { label: shown })} value={Math.round(h)} min={0} max={360} step={1} unit={degreeUnit} track={HUE_TRACK} handle={cssColor(fromHsb([h, 1, 1]))} disabled={disabled} onDrag={drag} onCommit={() => remember(latest.current)} onChange={(v) => setFromHsb([v, s, b])} />,
        <GradientSlider key="s" label={t("Saturation")} name={t("{label} saturation", { label: shown })} value={Math.round(s * 100) / 100} min={0} max={1} step={0.01} unit={percentUnit} track={`linear-gradient(90deg, ${cssColor(fromHsb([h, 0, b]))}, ${cssColor(fromHsb([h, 1, b]))})`} handle={cssColor(value)} disabled={disabled} onDrag={drag} onCommit={() => remember(latest.current)} onChange={(v) => setFromHsb([h, v, b])} />,
        <GradientSlider key="b" label={t("Brightness")} name={t("{label} brightness", { label: shown })} value={Math.round(b * 100) / 100} min={0} max={1} step={0.01} unit={percentUnit} track={`linear-gradient(90deg, #000, ${cssColor(fromHsb([h, s, 1]))})`} handle={cssColor(value)} disabled={disabled} onDrag={drag} onCommit={() => remember(latest.current)} onChange={(v) => setFromHsb([h, s, v])} />,
      ]
    : CHANNELS.map((channel, i) => {
        const at = (byte: number) => cssColor(value.map((v, j) => (j === i ? byte / 255 : v)) as Rgb);
        return (
          <GradientSlider key={channel} label={t(channel)} name={channel === "Red" ? t("{label} red", { label: shown }) : channel === "Green" ? t("{label} green", { label: shown }) : t("{label} blue", { label: shown })} value={Math.round(value[i] * 255)} min={0} max={255} step={1} unit={wholeUnit}
            track={`linear-gradient(90deg, ${at(0)}, ${at(255)})`} handle={cssColor(value)} disabled={disabled} onDrag={drag} onCommit={() => remember(latest.current)}
            onChange={(v) => set(value.map((x, j) => (j === i ? v / 255 : x)) as Rgb)} />
        );
      });
  return (
    <div className={`color-picker ${disabled ? "disabled" : ""}`}>
      <div className="color-picker-swatch" role="img" aria-label={t("{original} {originalHex}, {yours} {hex}", { original: originalLabel, originalHex: toHex(original), yours: (yoursLabel ?? t("yours")).toLowerCase(), hex })}>
        <span style={{ background: cssColor(original), color: textOn(original) }}><b style={pill(original)}>{originalLabel}</b></span>
        <span style={{ background: cssColor(value), color: textOn(value) }}><b style={pill(value)}>{yoursLabel ?? (changed ? t("Yours") : t("Yours (unchanged)"))}</b></span>
      </div>
      <div className="color-picker-body">
        <div className="color-picker-row">
          <input
            className="color-hex"
            aria-label={t("{label} hex color", { label: shown })}
            spellCheck={false}
            maxLength={7}
            disabled={disabled}
            value={hexDraft ?? hex}
            onFocus={(event) => { setHexDraft(hex); const box = event.currentTarget; setTimeout(() => box.select()); }}
            onChange={(event) => setHexDraft(event.target.value)}
            onBlur={() => { if (hexDraft !== null) commitHex(hexDraft); }}
            onKeyDown={(event) => {
              if (event.key === "Enter") { event.preventDefault(); if (hexDraft !== null) commitHex(hexDraft); }
              else if (event.key === "Escape") { if (hexDraft !== null && hexDraft !== hex) event.preventDefault(); setHexDraft(null); }
            }}
          />
          <div className="color-segmented" role="radiogroup" aria-label={t("{label} sliders", { label: shown })}>
            {(["hsb", "rgb"] as const).map((key) => (
              <button key={key} type="button" role="radio" aria-checked={mode === key} onClick={() => setMode(key)}>{key.toUpperCase()}</button>
            ))}
          </div>
          <button type="button" className="color-icon-button" aria-label={t("Copy {hex}", { hex })} title={copied ? t("Copied") : t("Copy hex")} onClick={() => void copy()}>
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><rect x="5" y="5" width="8.5" height="8.5" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.5" /><path d="M3 10.5V3.5A1 1 0 0 1 4 2.5h6.5" fill="none" stroke="currentColor" strokeWidth="1.5" /></svg>
          </button>
          {copied && <span className="color-copied" role="status">{t("Copied")}</span>}
          <span className="color-picker-spacer" />
          {quick && <button type="button" className="extra-text-button" disabled={disabled} onClick={() => set([0, 0, 0], true)}>{t("Black")}</button>}
          {quick && <button type="button" className="extra-text-button" disabled={disabled || !changed} onClick={onReset}>{t("{label} color", { label: originalLabel })}</button>}
        </div>
        <div className="color-picker-sliders">{sliders}</div>
        {recent.length > 0 && (
          <div className="color-recent">
            <span className="color-section-label" id={recentId}>{t("Recent colors")}</span>
            <div role="list" aria-labelledby={recentId}>
              {recent.map((color) => {
                const rgb = parseHex(color)!;
                return (
                  <span role="listitem" key={color}>
                    <button type="button" className="color-recent-swatch" aria-label={t("Use {color}", { color })} title={color} disabled={disabled} style={{ background: cssColor(rgb) }} onClick={() => set(rgb, true)} />
                  </span>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
