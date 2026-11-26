/** The Colors editor for the selected material, as three numbered steps in the order the build
 * applies them: 1 Recolor parts (color ranges), 2 Whole material (hue shift, saturation and
 * brightness, as the material has them), 3 Tint. Each step folds to one line that says what it
 * does; opening one closes the others (Shift+click keeps them open). Below: Include in build,
 * shared material, the build note and File details. A step's controls are shown only when the
 * material has that setting; otherwise the step says why. */
import { useState, type MouseEvent, type ReactNode } from "react";
import type { HeroColorRange } from "../models";
import { ExtraIcon } from "./Icons";
import { RANGE_DEFAULTS } from "./heroColorRanges";
import { ColorPicker } from "./ColorPicker";
import { ColorRangeCard, ScopeChoice } from "./ColorRangeCard";
import { GradientSlider, percentUnit, shiftUnit } from "./GradientSlider";
import { cssColor, fromHsb, sameColor, toHex, toHsb } from "./colorMath";
import { RANGE_LIMIT, type ColorEdit, type ColorKey, type ColorMaterial, type ColorMode, type Rgb } from "./colorAdapter";
import { STATE_PILLS } from "./ColorMaterialList";
import { shownColorName, tintHidesRanges, type Bytes, type PickResult } from "./colorPick";
import { OTHER_KEYS } from "./colorAdapter";
import { NOTHING, percentText, type Coverage } from "./colorCoverage";
import { changingRanges } from "./colorPixels";
import { m, mc, t, tc, tp, useT } from "../i18n";
import { T } from "../i18n/T";
// i18n-area: colors

/** Recolor by picking: first a click on a color ("point"), then its new color ("choose"). A
 * point's `note` says why a click could not be used: a grey or black spot, or a material whose
 * image is one flat color ("flat"). */
export type PickState =
  | { stage: "point"; index: number | null; keep: false; note: "" | "grey" | "black" | "flat" }
  | {
      stage: "choose"; index: number | null; keep: boolean; from: Bytes; to: Rgb; spot: { x: number; y: number; w: number } | null;
      /** Switch part_only: the texture coordinates of the clicked part (null: no part there), and
       * whether the range changes that part only or the color everywhere. */
      part?: { u: number; v: number } | null; scope?: "part" | "everywhere";
    };

export type StepNumber = 1 | 2 | 3;
export interface EditorProps {
  material: ColorMaterial;
  mode: ColorMode;
  disabled: boolean;
  /** The material's typical color (the average of its base color), for the slider tracks. */
  average: Rgb;
  /** The base-color thumbnail (empty when there is none). */
  thumbnail: string;
  edit: (changes: ColorEdit, removed?: ColorKey[]) => void;
  remove: () => void;
  /** How much each saved range selects (null while unknown), and the picked color's candidate. */
  coverages: (Coverage | null)[];
  pickCoverage: Coverage | null;
  pick: PickState | null;
  /** The range a pick would make, with what it does to the picked color. */
  pickResult: PickResult | null;
  startPick: (index: number | null) => void;
  changeColor: (index: number) => void;
  setPickColor: (color: Rgb) => void;
  finishPick: () => void;
  cancelPick: () => void;
  /** Where to click while picking. */
  pickHint: string;
  /** Why color ranges cannot be used with this image (from the game's texture). */
  rangesUnsupported: string;
  onDrag: (active: boolean) => void;
  /** The range the view lights now (pointed at, dragged), if any. */
  litRange: number | null;
  /** A range card was pointed at or left. */
  pointRange: (index: number, active: boolean) => void;
  /** A range's "selects" slider was dragged (true) or let go (false), or changed otherwise (null). */
  selectRange: (index: number, active: boolean | null) => void;
  /** Ranges for one part: how much each would select everywhere in the material. */
  everywhereCoverages?: (Coverage | null)[];
  /** While choosing with switch part_only: the share of each choice, and the choice. */
  pickShares?: { part: Coverage | null; everywhere: Coverage | null } | null;
  setPickScope?: (scope: "part" | "everywhere") => void;
}

const SELECTION_FIELDS: (keyof HeroColorRange)[] = ["hue", "hue_width", "saturation_range", "brightness_range", "softness"];
/** "2 colors changed", in the language shown. */
const colorsChanged = (n: number) => tp(n, { one: "{count} color changed", other: "{count} colors changed" });
/** The other surface changes a row can hold, by their saved key (shown in the language shown). */
const OTHER_NAMES: Record<string, string> = { roughness: mc("surface change", "roughness"), metalness: mc("surface change", "metalness"), opacity: mc("surface change", "opacity"), emission: mc("surface change", "emission") };
const otherName = (key: string) => ((OTHER_KEYS as readonly string[]).includes(key) && OTHER_NAMES[key] ? tc("surface change", OTHER_NAMES[key]) : key);
/** The three step titles (English, shown in the language shown). */
const STEP_TITLES = { 1: m("Recolor parts"), 2: m("Whole material"), 3: m("Tint") } as const;
const pct = (value: number) => `${Math.round(value * 100)}%`;

function Step({ n, title, summary, open, toggle, warn = false, children }: {
  n: StepNumber; title: string; summary: string; open: boolean; toggle: (n: StepNumber, keep: boolean) => void; warn?: boolean; children: ReactNode;
}) {
  const t = useT();
  return (
    <section className={`color-step ${open ? "open" : ""}`} aria-label={title}>
      <h4 className="color-step-heading">
        <button
          type="button"
          className="color-step-head"
          aria-expanded={open}
          aria-label={t("{n}. {title}: {summary}", { n, title, summary })}
          title={t("Shift+click keeps the other steps open")}
          onClick={(event: MouseEvent) => toggle(n, event.shiftKey || event.ctrlKey || event.metaKey)}
        >
          <span className="color-step-n" aria-hidden="true">{n}</span>
          <span className="color-step-title">{title}</span>
          <span className={`color-step-sum ${warn ? "warn" : ""}`}>{summary}</span>
          <span className="color-step-chevron" aria-hidden="true">▾</span>
        </button>
      </h4>
      {open && <div className="color-step-body">{children}</div>}
    </section>
  );
}

/** The step a material opens with: its color ranges, else its tint, else the whole material. */
export function firstStep(material: ColorMaterial, rangesOk: boolean): StepNumber {
  if (rangesOk) return 1;
  if (material.capabilities.tint) return 3;
  return 2;
}
/** The one-line summaries of the three steps, in the language shown. */
export function stepSummaries(material: ColorMaterial, rangesOk: boolean, pick: PickState | null) {
  const { capabilities: can, values } = material;
  const ranges = values?.ranges ?? [];
  const used = ranges.filter((r) => r.enabled !== false).length;
  const paused = ranges.length - used;
  const one = pick ? t("choosing a color…")
    : !rangesOk ? t("not available")
    : ranges.length ? [used ? colorsChanged(used) : "", paused ? tp(paused, { other: "{count} not in use" }) : ""].filter(Boolean).join(" · ")
    : t("not changed");
  const whole: string[] = [];
  if (can.hue && (values?.hue ?? 0) !== 0) whole.push(t("Hue {value}", { value: `${values!.hue! > 0 ? "+" : ""}${Math.round(values!.hue!)}°` }));
  if (can.saturation && (values?.saturation ?? 1) !== 1) whole.push(t("Saturation {value}", { value: pct(values!.saturation!) }));
  if (can.brightness && (values?.brightness ?? 1) !== 1) whole.push(t("Brightness {value}", { value: pct(values!.brightness!) }));
  const two = !(can.hue || can.saturation || can.brightness) ? t("not available") : whole.join(" · ") || t("not changed");
  const darkens = tintHidesRanges(values?.color, ranges);
  const three = !can.tint ? t("not available") : values?.color ? (darkens ? t("{hex} · darkens other colors", { hex: toHex(values.color) }) : toHex(values.color)) : t("original");
  return { one, two, three, darkens };
}

/** The out-of-reach sentence (English, one whole message per case; `<T>` translates it and fills in `{swatch}` and `{hex}`). */
function limitSentence(limits: ("saturation" | "brightness")[], tint: boolean): string {
  const bright = limits.includes("brightness"), saturated = limits.includes("saturation");
  if (saturated && bright) return tint
    ? limits[0] === "brightness" ? m("A color range can make a color at most twice as bright and saturated, so it gets as close as {swatch} {hex}. For a much lighter color, use Whole material or the Tint as well.") : m("A color range can make a color at most twice as saturated and bright, so it gets as close as {swatch} {hex}. For a much lighter color, use Whole material or the Tint as well.")
    : limits[0] === "brightness" ? m("A color range can make a color at most twice as bright and saturated, so it gets as close as {swatch} {hex}. For a much lighter color, use Whole material as well.") : m("A color range can make a color at most twice as saturated and bright, so it gets as close as {swatch} {hex}. For a much lighter color, use Whole material as well.");
  if (bright) return tint
    ? m("A color range can make a color at most twice as bright, so it gets as close as {swatch} {hex}. For a much lighter color, use Whole material or the Tint as well.")
    : m("A color range can make a color at most twice as bright, so it gets as close as {swatch} {hex}. For a much lighter color, use Whole material as well.");
  return tint
    ? m("A color range can make a color at most twice as saturated, so it gets as close as {swatch} {hex}. For a much stronger color, use Whole material or the Tint as well.")
    : m("A color range can make a color at most twice as saturated, so it gets as close as {swatch} {hex}. For a much stronger color, use Whole material as well.");
}

export function ColorEditor(props: EditorProps) {
  const { material, mode, disabled, average, thumbnail, edit, remove, coverages, pickCoverage, pick, pickResult, startPick, changeColor, setPickColor, finishPick, cancelPick, pickHint, rangesUnsupported, onDrag, litRange, pointRange, selectRange, everywhereCoverages = [], pickShares = null, setPickScope } = props;
  const t = useT();
  const { capabilities: can, values } = material;
  const editable = material.editable && !disabled;
  const originalLabel = mode === "hero" ? t("Game") : tc("own colors", "Original");
  const ranges = values?.ranges ?? [];
  const setRanges = (next: HeroColorRange[]) => edit(next.length ? { ranges: next } : {}, next.length ? [] : ["ranges"]);
  const pill = STATE_PILLS[material.state] ? t(STATE_PILLS[material.state]) : mode === "hero" ? (material.entry?.supported ? t("Game colors") : t("View only")) : t("Original colors");
  const [h, s, v] = toHsb(average);
  const hueShift = values?.hue ?? 0;
  const shifted = fromHsb([h + hueShift, s, v]);
  const rangesOk = can.ranges && !rangesUnsupported;
  const canPick = rangesOk && editable;
  const [open, setOpen] = useState<Set<StepNumber>>(() => new Set([firstStep(material, rangesOk)]));
  const [added, setAdded] = useState<number | null>(null);
  const toggle = (n: StepNumber, keep: boolean) => setOpen((current) => {
    if (current.has(n)) { const next = new Set(current); next.delete(n); return next; }
    return keep ? new Set(current).add(n) : new Set([n]);
  });
  const only = (n: StepNumber) => setOpen(new Set([n]));
  const summary = stepSummaries(material, rangesOk, pick);
  const usedRanges = ranges.filter((r) => r.enabled !== false).length;
  // A recolored copy of the texture is packaged only for ranges that change something (as the build decides).
  const recoloring = changingRanges(values).length > 0;
  const sub = [
    usedRanges ? colorsChanged(usedRanges) : "",
    can.tint ? (values?.color ? t("tint {hex}", { hex: toHex(values.color) }) : t("tint original")) : "",
  ].filter(Boolean).join(" · ");
  const begin = (index: number | null) => { only(1); startPick(index); };
  // A hero build stops when the material's ranges, together, select none of its base color.
  const inUse = ranges.flatMap((r, i) => (r.enabled !== false ? [coverages[i]] : []));
  const noneSelect = mode === "hero" && inUse.length > 0 && inUse.every((c) => c && c.fraction < NOTHING);

  function chooser() {
    if (!pick) return null;
    if (pick.stage === "point") {
      return (
        <div className={`color-pick-box ${pick.note ? "warn" : ""}`} role="status">
          {pick.note === "flat" ? (
            <>
              <strong>{t("This material's image is one flat color, so there is no color in it to pick out.")}</strong>
              <p>{can.tint ? (can.hue || can.saturation || can.brightness ? t("To change it, use the Tint or Whole material, which change the whole material.") : t("To change it, use the Tint, which change the whole material.")) : t("To change it, use Whole material, which change the whole material.")}</p>
              <div className="row-actions">
                {can.tint && <button type="button" className="extra-secondary small" onClick={() => { cancelPick(); only(3); }}>{t("Open Tint")}</button>}
                {(can.hue || can.saturation || can.brightness) && <button type="button" className="extra-secondary small" onClick={() => { cancelPick(); only(2); }}>{t("Open Whole material")}</button>}
                <button type="button" className="extra-text-button" onClick={cancelPick}>{t("Cancel")}</button>
              </div>
            </>
          ) : pick.note ? (
            <>
              <strong>{pick.note === "black" ? t("That spot is black: it has no hue to select or shift.") : t("That spot is grey or white: it has no hue to select or shift.")}</strong>
              <p>{pick.note === "black"
                ? can.tint ? t("A color range recolors one color by its hue, so it cannot pick out black areas. To change them, use Whole material or the Tint, which change the whole material; or click a colored spot.") : t("A color range recolors one color by its hue, so it cannot pick out black areas. To change them, use Whole material, which change the whole material; or click a colored spot.")
                : can.tint ? t("A color range recolors one color by its hue, so it cannot pick out grey or white areas. To change them, use Whole material or the Tint, which change the whole material; or click a colored spot.") : t("A color range recolors one color by its hue, so it cannot pick out grey or white areas. To change them, use Whole material, which change the whole material; or click a colored spot.")}</p>
              <div className="row-actions">
                {(can.hue || can.saturation || can.brightness) && <button type="button" className="extra-secondary small" onClick={() => { cancelPick(); only(2); }}>{t("Open Whole material")}</button>}
                {can.tint && <button type="button" className="extra-secondary small" onClick={() => { cancelPick(); only(3); }}>{t("Open Tint")}</button>}
                <button type="button" className="extra-text-button" onClick={cancelPick}>{t("Cancel")}</button>
              </div>
            </>
          ) : (
            <>
              <strong>{pick.index === null ? t("Click a color to recolor.") : t("Click the new color for Range {number}.", { number: pick.index + 1 })}</strong>
              <p>{t("{hint} Everything that color covers lights up before you choose. Press Esc to cancel.", { hint: pickHint })}</p>
              <div className="row-actions"><button type="button" className="extra-text-button" onClick={cancelPick}>{t("Cancel")}</button></div>
            </>
          )}
        </div>
      );
    }
    const from: Rgb = [pick.from[0] / 255, pick.from[1] / 255, pick.from[2] / 255];
    const same = sameColor(from, pick.to) && !pick.keep;
    const result = pickResult?.result ?? pick.to;
    // A grey, white or black pick (switch set_colour) is given the new color instead of shifted.
    const given = pickResult?.range.set_hue != null;
    const start = pick.keep ? t("From what this range selects") : t("This {color}", { color: shownColorName(from) });
    const share = pickCoverage ? percentText(pickCoverage.fraction) : "";
    return (
      <div className="color-pick-box choose" role="group" aria-label={t("Choose the new color")}>
        <strong>{pick.keep ? t("Choose the new color.") : t("Now choose the new color.")}</strong>
        <p className="hint">
          {pickCoverage
            ? given
              ? <T text="{start} · selects <b>{share}</b> of {material}." values={{ start, share, material: material.label }} tags={{ b: (chunk) => <b>{chunk}</b> }} />
              : <T text="{start} · selects <b>{share}</b> of {material}. The page works out the hue shift, saturation and brightness for you." values={{ start, share, material: material.label }} tags={{ b: (chunk) => <b>{chunk}</b> }} />
            : given ? t("{start}.", { start }) : t("{start}. The page works out the hue shift, saturation and brightness for you.", { start })}
        </p>
        {given && (
          <p className="hint">
            {pick.scope === "part"
              ? <T text="<b>{color} has no hue of its own</b>, so the part is given the new color: its hue and strength are set, and its light and shade are kept." values={{ color: shownColorName(from).replace(/^./, (c) => c.toUpperCase()) }} tags={{ b: (chunk) => <b>{chunk}</b> }} />
              : <T text="<b>{color} has no hue of its own</b>, so it is given the new color: its hue and strength are set, and its light and shade are kept." values={{ color: shownColorName(from).replace(/^./, (c) => c.toUpperCase()) }} tags={{ b: (chunk) => <b>{chunk}</b> }} />}
          </p>
        )}
        {pick.part !== undefined && setPickScope && (
          pick.part ? (
            <ScopeChoice label={t("The new color")} scope={pick.scope ?? "part"} setScope={setPickScope} part={pickShares?.part ?? null} everywhere={pickShares?.everywhere ?? null} disabled={!editable} />
          ) : (
            <p className="hint">{t("That spot is on no part of the model, so this range recolors the color everywhere in {material}.", { material: material.label })}</p>
          )
        )}
        {!pick.keep && <p className="hint">{t("Picked is the image's own color around the spot; the character is drawn darker by its lighting.")}</p>}
        <ColorPicker
          label={m("New color")}
          value={pick.to}
          original={from}
          originalLabel={pick.keep ? t("Selected") : t("Picked")}
          yoursLabel={tc("new color", "New")}
          changed
          quick={false}
          disabled={!editable}
          onChange={setPickColor}
          onReset={() => setPickColor(from)}
          onDrag={onDrag}
        />
        {pickResult && pickResult.limits.length > 0 && (
          <p className="color-pick-limit" role="note">
            <T text={limitSentence(pickResult.limits, can.tint)} values={{ swatch: <span className="color-inline-swatch" style={{ background: cssColor(result) }} aria-hidden="true" />, hex: toHex(result) }} />
          </p>
        )}
        {pickResult?.otherRange && !pickResult.limits.length && (
          <p className="color-pick-limit" role="note">{t("Another color range also changes this color, so it ends up as {hex}.", { hex: toHex(result) })}</p>
        )}
        <div className="row-actions">
          <button type="button" className="extra-primary small" disabled={!editable || same} onClick={finishPick}>{t("Done")}</button>
          <button type="button" className="extra-text-button" onClick={cancelPick}>{t("Cancel")}</button>
          {same && <span className="hint">{t("Choose a color different from the picked one.")}</span>}
        </div>
      </div>
    );
  }

  return (
    <div className="color-editor">
      <header className="color-editor-head">
        <span className="color-editor-thumb" aria-hidden="true" style={{ background: cssColor(material.baseColor ?? material.originalTint) }}>
          {thumbnail && <img src={thumbnail} alt="" />}
        </span>
        <div className="color-editor-title">
          <h3 title={material.id}>{material.label}</h3>
          <p>{values ? sub || t("No change yet") : mode === "hero" ? t("Game colors") : t("Original colors")}</p>
        </div>
        <span className={`color-pill ${material.state}`}>{pill}</span>
        <span className="color-picker-spacer" />
        {values && <button type="button" className="extra-text-button" disabled={disabled} onClick={remove}>{t("Reset {material}", { material: material.label })}</button>}
      </header>
      {material.missing ? (
        <div className="extra-editor-notice"><ExtraIcon name="warning" /><strong>{material.blocked}</strong><p>{mode === "hero" ? t("Remove this saved color. Reload character assets after a game update.") : t("Remove this saved color, or prepare the model again if the material should be there.")}</p></div>
      ) : material.state === "view-only" ? (
        <p className="extra-original-kept"><strong>{t("Game colors kept")}</strong>{material.blocked}</p>
      ) : !material.editable && material.blocked ? (
        <p className="extra-error">{material.blocked}</p>
      ) : null}
      {!material.missing && material.state !== "view-only" && (
        <div className="color-steps">
          <Step n={1} title={t(STEP_TITLES[1])} summary={summary.one} open={open.has(1)} toggle={toggle}>
            {!can.ranges ? (
              <p className="hint">{material.notes.ranges}</p>
            ) : rangesUnsupported ? (
              <p className="hint">{rangesUnsupported}</p>
            ) : (
              <>
                {chooser()}
                {ranges.map((range, index) => (
                  <ColorRangeCard
                    key={index}
                    index={index}
                    range={range}
                    disabled={!editable}
                    canPick={canPick && !(pick && pick.index === index)}
                    picking={pick?.index === index}
                    coverage={coverages[index] ?? null}
                    pick={() => begin(index)}
                    changeColor={() => changeColor(index)}
                    update={(changes) => {
                      setRanges(ranges.map((r, i) => (i === index ? { ...r, ...changes } : r)));
                      if (SELECTION_FIELDS.some((key) => key in changes)) selectRange(index, null);
                    }}
                    remove={() => {
                      if (pick?.index === index) cancelPick();
                      setRanges(ranges.filter((_, i) => i !== index));
                    }}
                    onDrag={onDrag}
                    onSelectDrag={(active) => selectRange(index, active)}
                    point={(active) => pointRange(index, active)}
                    fineTune={added === index}
                    lit={litRange === index}
                    buildNote={noneSelect ? t("While no color range of this material selects anything, the build stops with an error.") : ""}
                    everywhere={everywhereCoverages[index] ?? null}
                  />
                ))}
                <div className="hero-color-range-actions">
                  <button type="button" className="extra-primary small" disabled={!canPick || ranges.length >= RANGE_LIMIT || Boolean(pick)} onClick={() => begin(null)}>
                    <span aria-hidden="true">＋ </span>{t("Recolor another color")}
                  </button>
                  <span className="hint">
                    {ranges.length >= RANGE_LIMIT ? tp(RANGE_LIMIT, { other: "A material holds at most {count} color ranges." }) : tp(RANGE_LIMIT, { other: "Click a color on the character, then choose its new color. Up to {count}." })}
                  </span>
                  <button
                    type="button"
                    className="extra-text-button"
                    disabled={!editable || ranges.length >= RANGE_LIMIT}
                    onClick={() => { setAdded(ranges.length); setRanges([...ranges, { enabled: true, ...RANGE_DEFAULTS }]); }}
                  >
                    {t("Add color range")}
                  </button>
                </div>
                {/* Only while a color range changes something is a recolored copy packaged (the note under the steps says the same). */}
                {recoloring && <p className="hint">{mode === "hero" ? t("The mod gets a recolored copy of this material's base-color texture.") : t("The build recolors a copy of this material's base-color image.")}</p>}
              </>
            )}
          </Step>
          <Step n={2} title={t(STEP_TITLES[2])} summary={summary.two} open={open.has(2)} toggle={toggle}>
            {can.saturation || can.brightness || can.hue ? (
              <div className="color-sliders-block">
                {can.hue && (
                  <GradientSlider label={t("Hue shift")} value={hueShift} min={-180} max={180} step={1} unit={shiftUnit} disabled={!editable} onDrag={onDrag}
                    track={`linear-gradient(90deg, ${[0, 60, 120, 180, 240, 300, 360].map((d, i) => `${cssColor(fromHsb([h - 180 + d, Math.max(s, 0.5), Math.max(v, 0.6)]))} ${(i * 100) / 6}%`).join(", ")})`}
                    handle={cssColor(fromHsb([h + hueShift, Math.max(s, 0.5), Math.max(v, 0.6)]))} onChange={(hue) => edit({ hue })} />
                )}
                {can.saturation && (
                  <GradientSlider label={t("Saturation")} value={values?.saturation ?? 1} min={0} max={2} step={0.05} fine={0.01} unit={percentUnit} disabled={!editable} onDrag={onDrag}
                    track={`linear-gradient(90deg, ${cssColor(fromHsb([h + hueShift, 0, v]))}, ${cssColor(shifted)} 50%, ${cssColor(fromHsb([h + hueShift, Math.min(1, s * 2 + 0.2), v]))})`}
                    handle={cssColor(fromHsb([h + hueShift, Math.min(1, s * (values?.saturation ?? 1)), v]))} onChange={(saturation) => edit({ saturation })} />
                )}
                {can.brightness && (
                  <GradientSlider label={t("Brightness")} value={values?.brightness ?? 1} min={0} max={2} step={0.05} fine={0.01} unit={percentUnit} disabled={!editable} onDrag={onDrag}
                    track={`linear-gradient(90deg, #000, ${cssColor(shifted)} 50%, ${cssColor(fromHsb([h + hueShift, s, Math.min(1, v * 2 + 0.1)]))})`}
                    handle={cssColor(fromHsb([h + hueShift, s, Math.min(1, v * (values?.brightness ?? 1))]))} onChange={(brightness) => edit({ brightness })} />
                )}
                {material.notes.whole && <p className="hint">{material.notes.whole}</p>}
              </div>
            ) : <p className="hint">{material.notes.whole}</p>}
            {mode === "hero" && <p className="hint">{t("For black clothes, set the tint to black or the brightness to 0%.")}</p>}
          </Step>
          <Step n={3} title={t(STEP_TITLES[3])} summary={summary.three} open={open.has(3)} toggle={toggle} warn={summary.darkens}>
            <p className="hint">{mode === "hero" ? t("The color the game multiplies this material by.") : t("The color this material's base color is multiplied by.")}</p>
            {can.tint ? (
              <>
                <ColorPicker
                  label={m("Tint")}
                  value={values?.color ?? material.originalTint}
                  original={material.originalTint}
                  originalLabel={originalLabel}
                  changed={Boolean(values?.color)}
                  disabled={!editable}
                  onChange={(color) => edit({ color })}
                  onReset={() => edit({}, ["color"])}
                  onDrag={onDrag}
                />
                {material.notes.tint && <p className="hint">{material.notes.tint}</p>}
                {summary.darkens && (
                  <div className="color-tint-warning" role="note">
                    <p><T text="<strong>This tint darkens every other color of the material.</strong> A tint multiplies the whole material, so your recolored parts show darker and close to the tint." /></p>
                    <button type="button" className="extra-secondary small" disabled={!editable} onClick={() => edit({}, ["color"])}>{t("Back to the original tint")}</button>
                  </div>
                )}
              </>
            ) : <p className="hint">{material.notes.tint}</p>}
          </Step>
          <p className="hint color-steps-order">{t("Applied in this order: recolored parts first, then the whole material, then the tint on top.")}</p>
        </div>
      )}
      {material.sharedWith.length > 0 && !material.missing && (
        <div className="extra-shared">
          <ExtraIcon name="warning" size={18} />
          <div>
            <strong>{t("Shared material")}</strong>
            <p>{t("Also used by {models}. Recoloring it changes those models too.", { models: material.sharedWith.join(tc("list separator", ", ")) })}</p>
            {values && (
              <label><input type="checkbox" checked={material.allowShared} disabled={disabled} onChange={(e) => edit({ allow_shared: e.target.checked })} />{t("Recolor everywhere it is used")}</label>
            )}
          </div>
        </div>
      )}
      {values && (
        <div className="extra-replacement-actions color-editor-foot">
          <label className="extra-toggle">
            <input type="checkbox" checked={material.enabled} disabled={disabled} onChange={(e) => edit({ enabled: e.target.checked })} />
            <span aria-hidden="true" />
            <strong>{t("Include in build")}</strong>
          </label>
          <button className="extra-remove" disabled={disabled} onClick={remove}><ExtraIcon name="close" size={15} />{t("Remove color")}</button>
        </div>
      )}
      {material.otherChanges.length > 0 && (
        <p className="hint">{values
          ? t("This material's row also holds {changes} from Appearance → Surface changes; they stay as they are, and Include in build switches them too.", { changes: material.otherChanges.map(otherName).join(tc("list separator", ", ")) })
          : t("This material's row also holds {changes} from Appearance → Surface changes; they stay as they are.", { changes: material.otherChanges.map(otherName).join(tc("list separator", ", ")) })}</p>
      )}
      <p className="extra-inline-note">
        {mode === "hero"
          ? recoloring
            ? t("The build changes this material's color settings and adds a recolored copy of its base-color texture; the game's own files stay unchanged. The preview is approximate, so check the result in game.")
            : t("The build changes only this material's color settings; its textures stay the game's own. The preview is approximate, so check the result in game.")
          : t("The build recolors a copy of this material's base-color image (or its plain color) and sets its tint; your files stay unchanged. The game's lighting and shaders differ from the preview.")}
      </p>
      <details className="extra-file-details">
        <summary>{t("File details")}<ExtraIcon name="folder" size={15} /></summary>
        <dl>{material.details.map(([term, value]) => <div key={term}><dt>{term}</dt><dd>{value}</dd></div>)}</dl>
      </details>
    </div>
  );
}
