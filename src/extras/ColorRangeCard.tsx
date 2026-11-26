/** One color range: From → To swatches with a plain name ("Blue → gold"), how much of the
 * material it selects, Use and Remove, picking another color on the character, changing the new
 * color, and Fine-tune this range: what it selects (a hue band, saturation and brightness
 * ranges, softness) and what it changes it to (hue shift, saturation, brightness), with the
 * limits the build checks. Pointing at the card, or dragging a "selects" slider, lights what the
 * range selects on the character. */
import { useState } from "react";
import type { HeroColorRange } from "../models";
import { colorSwitches, recolorTexel, rgb, switchRefusal, withDefaults } from "./heroColorRanges";
import { GradientRange, GradientSlider, degreeUnit, percentUnit, shiftUnit, type Unit } from "./GradientSlider";
import { HUE_TRACK } from "./ColorPicker";
import { cssColor, fromHsb } from "./colorMath";
import { nothingReason, percentText, type Coverage } from "./colorCoverage";
import { rangeChanges, shownRangeName } from "./colorPick";
import type { Rgb } from "./colorAdapter";
import { m, t, useT } from "../i18n";
import { T } from "../i18n/T";
// i18n-area: colors

const widthUnit: Unit = {
  format: (value) => (value >= 180 ? t("All") : `±${Math.round(value)}°`),
  // "All" typed in English or in the language shown.
  parse: (text) => {
    const all = t("All");
    return /^\s*all\s*$/i.test(text) || text.trim() === all ? 180 : degreeUnit.parse(text.replace(/^\s*±/, ""));
  },
};
/** The middle color a range selects and what it turns into. */
export function rangeSwatches(range: HeroColorRange): { from: Rgb; to: Rgb } {
  const values = withDefaults(range);
  const middle = ([low, high]: [number, number]) => (low + high) / 2;
  const centre = rgb(values.hue, middle(values.saturation_range), middle(values.brightness_range)).map((v) => Math.round(v * 255)) as [number, number, number];
  const result = recolorTexel([values], [...centre, 255]);
  return { from: centre.map((v) => v / 255) as Rgb, to: result.slice(0, 3).map((v) => v / 255) as Rgb };
}
/** The From → To of a range: what it selects in this image (the weighted mean of its original
 * pixels) when known, else the middle of its selection; To is what the range makes of From. */
export function rangeColors(range: HeroColorRange, coverage: Coverage | null | undefined): { from: Rgb; to: Rgb } {
  if (!coverage?.mean) return rangeSwatches(range);
  const from = coverage.mean;
  const out = recolorTexel([{ ...withDefaults(range), softness: 0, hue_width: 180, saturation_range: [0, 1], brightness_range: [0, 1] }], [...(from.map((v) => Math.round(v * 255)) as [number, number, number]), 255]);
  return { from, to: out.slice(0, 3).map((v) => v / 255) as Rgb };
}
/** A rainbow starting at `start` degrees (the colors a hue shift turns the range into). */
const shiftedHues = (start: number) =>
  `linear-gradient(90deg, ${[0, 60, 120, 180, 240, 300, 360].map((d, i) => `${cssColor(fromHsb([start + d, 1, 1]))} ${(i * 100) / 6}%`).join(", ")})`;

export interface RangeCardProps {
  index: number;
  range: HeroColorRange;
  disabled: boolean;
  /** Picking on the character or the image is possible. */
  canPick: boolean;
  /** This range is being picked again. */
  picking: boolean;
  /** How much of the material it selects (null while unknown). */
  coverage: Coverage | null;
  /** Pick another color on the character for this range. */
  pick: () => void;
  /** Change the new color (the picker, from what it selects to what it makes of it). */
  changeColor: () => void;
  update: (changes: Partial<HeroColorRange>) => void;
  remove: () => void;
  onDrag?: (active: boolean) => void;
  /** A "selects" slider is being dragged (true) or let go (false): the view lights the selection. */
  onSelectDrag?: (active: boolean) => void;
  /** The pointer or the focus is on the card (true) or left it (false). */
  point?: (active: boolean) => void;
  /** Fine-tune starts open (a range added by hand). */
  fineTune?: boolean;
  /** The view lights this range now. */
  lit?: boolean;
  /** Added to the "selects nothing" warning (the build stops when no range selects anything). */
  buildNote?: string;
  /** For a range for one part: how much the same range would select everywhere in the material. */
  everywhere?: Coverage | null;
}

/** "This part only" / "Everywhere this color": the choice of a range picked on one part (switch
 * part_only), with the share of the material each selects. */
export function ScopeChoice({ scope, setScope, part, everywhere, disabled, label }: {
  scope: "part" | "everywhere"; setScope: (scope: "part" | "everywhere") => void;
  part: Coverage | null; everywhere: Coverage | null; disabled: boolean; label: string;
}) {
  const t = useT();
  return (
    <div className="color-scope" role="radiogroup" aria-label={t("{label}: what it changes", { label })}>
      <div className="color-segmented">
        <button type="button" role="radio" aria-checked={scope === "part"} disabled={disabled} onClick={() => setScope("part")}>{part ? t("This part only · {share}", { share: percentText(part.fraction) }) : t("This part only")}</button>
        <button type="button" role="radio" aria-checked={scope === "everywhere"} disabled={disabled} onClick={() => setScope("everywhere")}>{everywhere ? t("Everywhere this color · {share}", { share: percentText(everywhere.fraction) }) : t("Everywhere this color")}</button>
      </div>
      <p className="hint">
        {scope === "part"
          ? t("Only the part of the model you clicked, and any part that shares its place in the image (a mirrored sleeve or leg changes with it).")
          : t("Every area of this color in the material, on every part.")}
      </p>
    </div>
  );
}
/** Why the app refuses a saved range whose switch is off (shown on its card). */
export const SWITCH_NOTES = {
  set_colour: m("This version of the app keeps new colors for grey, white and black switched off: the build stops on this range until you remove it."),
  part_only: m("This version of the app keeps This part only switched off: the build stops on this range until it is set to Everywhere this color."),
} as const;

export function ColorRangeCard({ index, range, disabled, canPick, picking, coverage, pick, changeColor, update, remove, onDrag, onSelectDrag, point, fineTune = false, lit = false, buildNote = "", everywhere = null }: RangeCardProps) {
  const t = useT();
  const [open, setOpen] = useState(fineTune);
  // The spot of a range set to Everywhere, so This part only can come back while the page is open.
  const [spot, setSpot] = useState(range.part ?? null);
  const values = withDefaults(range);
  const refusal = switchRefusal(range);
  const scoped = colorSwitches().part_only && Boolean(range.part ?? spot);
  const name = t("Range {number}", { number: index + 1 });
  const { from, to } = rangeColors(range, coverage);
  const changes = rangeChanges(range);
  const hue = values.hue;
  const pure = cssColor(fromHsb([hue, 1, 1]));
  const centreS = (values.saturation_range[0] + values.saturation_range[1]) / 2;
  const centreV = (values.brightness_range[0] + values.brightness_range[1]) / 2;
  const width = values.hue_width;
  const band = width >= 180 ? { from: 0, to: 1 } : { from: (((hue - width) % 360) + 360) % 360 / 360, to: ((hue + width) % 360) / 360 };
  const common = { disabled, onDrag };
  const selects = { disabled, onDrag: (active: boolean) => { onDrag?.(active); onSelectDrag?.(active); } };
  const reason = coverage ? nothingReason(coverage) : null;
  const paused = range.enabled === false;
  return (
    <fieldset
      className={`color-range-card ${lit ? "lit" : ""} ${paused ? "paused" : ""}`}
      aria-label={name}
      onPointerEnter={() => point?.(true)}
      onPointerLeave={() => point?.(false)}
      onFocus={() => point?.(true)}
      onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) point?.(false); }}
    >
      <div className="color-range-head">
        <span className="color-range-swatch large" aria-hidden="true" style={{ background: cssColor(from) }} />
        <span aria-hidden="true" className="color-range-arrow">→</span>
        <span className="color-range-swatch large" aria-hidden="true" style={{ background: cssColor(to) }} />
        <div className="color-range-title">
          <strong>{shownRangeName(from, to, changes)}</strong>
          <small>
            {name}
            {paused ? t(" · not in use") : ""}
            {coverage ? (reason ? <span className="color-range-none">{t(" · selects nothing in this material")}</span> : <T text=" · selects <b>{share}</b> of this material" values={{ share: percentText(coverage.fraction) }} tags={{ b: (chunk) => <b className="color-range-pct">{chunk}</b> }} />) : null}
          </small>
        </div>
        <span className="color-range-tools">
          <label className="hero-color-check">
            <input type="checkbox" checked={range.enabled !== false} disabled={disabled} onChange={(e) => update({ enabled: e.target.checked })} />
            {t("Use")}
          </label>
          <button type="button" className="extra-text-button" disabled={disabled} onClick={remove}>{t("Remove")}</button>
        </span>
      </div>
      {reason && (
        <p className="color-range-warning" role="note">
          {buildNote
            ? t("This range covers no part of the image: {reason} Pick a color on the character instead, or widen it under Fine-tune. {note}", { reason, note: buildNote })
            : t("This range covers no part of the image: {reason} Pick a color on the character instead, or widen it under Fine-tune.", { reason })}
        </p>
      )}
      {refusal && <p className="color-range-warning" role="note">{t(SWITCH_NOTES[refusal])}</p>}
      {scoped && (
        <ScopeChoice
          label={name}
          scope={range.part ? "part" : "everywhere"}
          setScope={(scope) => {
            if (scope === "part" && !range.part && spot) update({ part: spot });
            if (scope === "everywhere" && range.part) { setSpot(range.part); update({ part: undefined }); }
          }}
          part={range.part ? coverage : null}
          everywhere={range.part ? everywhere : coverage}
          disabled={disabled}
        />
      )}
      <div className="color-range-actions">
        <button type="button" className="extra-secondary small" aria-pressed={picking} disabled={disabled || !canPick} onClick={pick}>
          {picking ? t("Picking… click a color") : t("Pick another color on the character")}
        </button>
        <button type="button" className="extra-secondary small" disabled={disabled} onClick={changeColor}>{t("Change the new color…")}</button>
        <span className="color-picker-spacer" />
        <button type="button" className="extra-text-button" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? t("Hide fine-tuning") : t("Fine-tune this range")}<span aria-hidden="true">{open ? " ▴" : " ▾"}</span>
        </button>
      </div>
      {open && (
        <div className="color-range-fine">
          <span className="color-section-label">{t("What it selects")}</span>
          <GradientSlider label={t("Selects hue")} name={t("{range} hue", { range: name })} value={hue} min={0} max={360} step={1} unit={degreeUnit} track={HUE_TRACK} handle={pure} band={band} {...selects} onChange={(v) => update({ hue: v })} />
          <GradientSlider label={t("Hue width")} name={t("{range} hue width", { range: name })} value={width} min={0} max={180} step={1} unit={widthUnit} track={`linear-gradient(90deg, ${pure}, ${cssColor(fromHsb([hue + 90, 1, 1]))} 50%, ${cssColor(fromHsb([hue + 180, 1, 1]))})`} handle={pure} {...selects} onChange={(v) => update({ hue_width: v })} />
          <GradientRange label={t("Saturation")} name={t("{range} saturation", { range: name })} values={values.saturation_range} min={0} max={1} step={0.01} unit={percentUnit} rangeLabel={m("range")}
            track={`linear-gradient(90deg, ${cssColor(fromHsb([hue, 0, Math.max(centreV, 0.35)]))}, ${cssColor(fromHsb([hue, 1, Math.max(centreV, 0.35)]))})`} {...selects} onChange={(v) => update({ saturation_range: v })} />
          <GradientRange label={t("Brightness")} name={t("{range} brightness", { range: name })} values={values.brightness_range} min={0} max={1} step={0.01} unit={percentUnit} rangeLabel={m("range")}
            track={`linear-gradient(90deg, #000, ${cssColor(fromHsb([hue, Math.max(centreS, 0.35), 1]))})`} {...selects} onChange={(v) => update({ brightness_range: v })} />
          <GradientSlider label={t("Softness")} name={t("{range} softness", { range: name })} value={values.softness} min={0} max={1} step={0.05} fine={0.01} unit={percentUnit} track={`linear-gradient(90deg, ${pure}, ${cssColor(fromHsb([hue, 0.3, 0.6]))})`} handle={pure} {...selects} onChange={(v) => update({ softness: v })} />
          <span className="color-section-label">{t("What it changes it to")}</span>
          <GradientSlider label={t("Shift hue")} name={t("{range} hue shift", { range: name })} value={values.hue_shift} min={-180} max={180} step={1} unit={shiftUnit} track={shiftedHues(hue - 180)} handle={cssColor(fromHsb([hue + values.hue_shift, 1, 1]))} {...common} onChange={(v) => update({ hue_shift: v })} />
          <GradientSlider label={t("Saturation")} name={t("{range} saturation", { range: name })} value={values.saturation} min={0} max={2} step={0.05} fine={0.01} unit={percentUnit}
            track={`linear-gradient(90deg, ${cssColor(fromHsb([hue + values.hue_shift, 0, 0.8]))}, ${cssColor(fromHsb([hue + values.hue_shift, 0.5, 0.8]))} 50%, ${cssColor(fromHsb([hue + values.hue_shift, 1, 0.8]))})`} handle={cssColor(to)} {...common} onChange={(v) => update({ saturation: v })} />
          <GradientSlider label={t("Brightness")} name={t("{range} brightness", { range: name })} value={values.brightness} min={0} max={2} step={0.05} fine={0.01} unit={percentUnit}
            track={`linear-gradient(90deg, #000, ${cssColor(fromHsb([hue + values.hue_shift, 0.8, 0.5]))} 50%, ${cssColor(fromHsb([hue + values.hue_shift, 0.8, 1]))})`} handle={cssColor(to)} {...common} onChange={(v) => update({ brightness: v })} />
        </div>
      )}
    </fieldset>
  );
}
