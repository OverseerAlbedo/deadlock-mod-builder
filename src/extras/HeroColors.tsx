import { useEffect, useRef, useState, type MouseEvent } from "react";
import type { HeroColorRange, HeroMaterial, Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import { ColorField } from "../components/MaterialEditor";
import { changeCustomization } from "../components/customization";
import { modelEnabled } from "../build/readiness";
import { ExtraIcon } from "./Icons";
import { useExtraMedia } from "./media";
import { RANGE_DEFAULTS, rangeAround, recolorPixels, recolorTexel, rgb, withDefaults, type ColorRange } from "./heroColorRanges";

type Rgb = [number, number, number];
/** A hero material from the catalog with its saved color, if any. */
export interface HeroColorItem {
  entry: HeroMaterialEntry;
  index: number;
  row?: HeroMaterial;
  missing: boolean;
}
const fold = (value: string) => value.toLowerCase();
function stem(path: string) {
  return (path.split("/").pop() ?? path).replace(/_c$/, "").replace(/\.vmat$/, "");
}
export function heroColorItems(entries: HeroMaterialEntry[], rows: HeroMaterial[]): HeroColorItem[] {
  const indexes = new Map<string, number>();
  rows.forEach((row, index) => {
    const key = fold(row.target ?? "");
    if (!indexes.has(key) || row.enabled !== false) indexes.set(key, index);
  });
  const known = new Set(entries.map((entry) => fold(entry.target)));
  return [
    ...entries.map((entry) => {
      const index = indexes.get(fold(entry.target)) ?? -1;
      return { entry, index, row: rows[index], missing: false };
    }),
    // Saved colors for materials the game no longer lists, so they can be removed.
    ...rows.flatMap((row, index) =>
      row.target && !known.has(fold(row.target))
        ? [{
            entry: { path: row.target.replace(/_c$/, ""), target: row.target, label: stem(row.target), shader: "", supported: false, reason: "This material is not in the loaded game files.", tint: null, albedo: null, tint_masked: false, color_texture: null, used_by: [], shared: false, original_sha256: "" },
            index, row, missing: true,
          }]
        : [],
    ),
  ];
}
const css = (rgb: Rgb) => `rgb(${rgb.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255)).join(" ")})`;
/** Approximates the build: the tint multiplies the base color; saturation and
 * brightness scale the material's own albedo adjustment. */
export function heroColorFilter(entry: HeroMaterialEntry, row?: HeroMaterial) {
  const albedo = entry.albedo ?? [1, 1, 1];
  return `saturate(${albedo[1] * (row?.saturation ?? 1)}) brightness(${albedo[2] * (row?.brightness ?? 1)})`;
}
function Slider({ label, value, update, disabled }: { label: string; value: number; update: (value: number) => void; disabled: boolean }) {
  return (
    <label className="color-slider">
      <span>{label}</span>
      <input type="range" aria-label={label} min={0} max={2} step={0.05} value={value} disabled={disabled} onChange={(event) => update(Number(event.target.value))} />
      <output>{Math.round(value * 100)}%</output>
    </label>
  );
}
/** The enabled ranges of a saved color, with defaults for unset values. */
export function activeRanges(row?: HeroMaterial): ColorRange[] {
  return (row?.ranges ?? []).filter((range) => range.enabled !== false).map(withDefaults);
}
/** The image pixel under a click on an `object-fit: cover` element, or null outside the image. */
export function coverPixel(box: { left: number; top: number; width: number; height: number }, x: number, y: number, width: number, height: number) {
  const scale = Math.max(box.width / width, box.height / height);
  const px = Math.floor((x - box.left - (box.width - width * scale) / 2) / scale);
  const py = Math.floor((y - box.top - (box.height - height * scale) / 2) / scale);
  return px >= 0 && py >= 0 && px < width && py < height ? { x: px, y: py } : null;
}
/** The preview image's pixels for the range preview and the color picker; null
 * until loaded or without a 2D canvas. */
function usePixels(url: string) {
  const [pixels, setPixels] = useState<ImageData | null>(null);
  useEffect(() => {
    setPixels(null);
    if (!url || typeof Image === "undefined") return;
    let live = true;
    const image = new Image();
    image.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = image.naturalWidth;
      canvas.height = image.naturalHeight;
      const context = canvas.width ? canvas.getContext("2d", { willReadFrequently: true }) : null;
      if (!live || !context) return;
      context.drawImage(image, 0, 0);
      setPixels(context.getImageData(0, 0, canvas.width, canvas.height));
    };
    image.src = url;
    return () => {
      live = false;
    };
  }, [url]);
  return pixels;
}
/** The base color with the ranges applied (or their selection), drawn with the build's own function. */
function RecoloredCanvas({ pixels, ranges, mask, label }: { pixels: ImageData; ranges: ColorRange[]; mask: boolean; label: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const key = JSON.stringify(ranges);
  useEffect(() => {
    const context = canvas.current?.getContext("2d");
    if (!canvas.current || !context) return;
    canvas.current.width = pixels.width;
    canvas.current.height = pixels.height;
    const copy = new ImageData(new Uint8ClampedArray(pixels.data), pixels.width, pixels.height);
    recolorPixels(ranges, copy.data, mask);
    context.putImageData(copy, 0, 0);
  }, [pixels, key, mask]);
  return <canvas ref={canvas} role="img" aria-label={label} />;
}
function MaterialPreview({ item, url, loading, error, mask, picking, pick }: {
  item: HeroColorItem; url: string; loading: boolean; error: string; mask: boolean; picking: boolean;
  pick: (color: [number, number, number]) => void;
}) {
  const { entry, row } = item;
  const ranges = activeRanges(row);
  const pixels = usePixels(url);
  const tint = row?.color ?? entry.tint ?? [1, 1, 1];
  function choose(event: MouseEvent<HTMLElement>) {
    if (!picking || !pixels) return;
    const spot = coverPixel(event.currentTarget.getBoundingClientRect(), event.clientX, event.clientY, pixels.width, pixels.height);
    if (!spot) return;
    const at = (spot.y * pixels.width + spot.x) * 4;
    pick([pixels.data[at], pixels.data[at + 1], pixels.data[at + 2]]);
  }
  return (
    <div className="extra-comparison">
      {[["Game", undefined], ["Your colors", row]].map(([label, shown]) => {
        const game = label === "Game";
        // The selection shows the ranges alone; colors add the tint and albedo approximation.
        const recolored = !game && pixels && (mask || ranges.length > 0);
        return (
          <section key={label as string}>
            <div className="extra-preview-label">
              <span>{label as string}</span>
              {!game && mask ? <small>Selection</small> : shown ? <small>Approximate</small> : null}
            </div>
            <div className="extra-media hero-color-media">
              <div
                className={`hero-color-stage ${game && picking ? "picking" : ""}`}
                style={{ filter: mask && !game ? undefined : heroColorFilter(entry, shown as HeroMaterial | undefined) }}
                onClick={game ? choose : undefined}
              >
                {recolored ? (
                  <RecoloredCanvas pixels={pixels} ranges={ranges} mask={mask} label={`${entry.label} ${mask ? "color range selection" : "base color with your colors"}`} />
                ) : url ? (
                  <img src={url} alt={`${entry.label} base color`} />
                ) : (
                  <div className="hero-color-blank" />
                )}
                {!(mask && !game) && <span className="hero-color-tint" style={{ background: css((shown ? tint : entry.tint ?? [1, 1, 1]) as Rgb) }} />}
              </div>
              {loading && <span className="extra-spinner" role="status" aria-label="Loading preview" />}
              {error && <small className="extra-thumbnail-error">Preview unavailable</small>}
            </div>
          </section>
        );
      })}
    </div>
  );
}

export function HeroColors({
  project,
  update,
  disabled,
  catalog,
  discovering,
  canDiscover,
  reload,
  openSetup,
  revision,
}: {
  project: Project;
  update: (project: Project) => void;
  disabled: boolean;
  catalog: ComponentCatalog | null;
  discovering: boolean;
  canDiscover: boolean;
  reload: () => void;
  openSetup?: () => void;
  revision: number;
}) {
  const [selected, setSelected] = useState(""), [query, setQuery] = useState("");
  const colors = project.customization.hero_materials ?? {};
  const rows = colors.materials ?? [];
  const items = heroColorItems(catalog?.materials ?? [], rows);
  const visible = items.filter((item) => `${item.entry.label} ${item.entry.path}`.toLowerCase().includes(query.toLowerCase()));
  const selection = items.find((item) => fold(item.entry.target) === fold(selected)) ?? null;
  const model = modelEnabled(project);
  const active = Boolean(project.customization.enabled) && colors.enabled !== false;
  const setRows = (materials: HeroMaterial[], activate = true) =>
    update(changeCustomization(project, { hero_materials: { ...colors, enabled: colors.enabled ?? true, materials } }, activate));
  function edit(item: HeroColorItem, changes: Partial<HeroMaterial>, removed: (keyof HeroMaterial)[] = []) {
    const base: HeroMaterial = item.row ?? { enabled: true, target: item.entry.target, original_sha256: item.entry.original_sha256, allow_shared: false };
    const next: HeroMaterial = { ...base, ...changes };
    removed.forEach((key) => delete next[key]);
    // A color that changes nothing is not kept (the build would refuse it);
    // color ranges keep it while they are being set up.
    const empty = !next.color && (next.saturation ?? 1) === 1 && (next.brightness ?? 1) === 1 && !next.ranges?.length;
    if (item.index >= 0)
      setRows(empty ? rows.filter((_, index) => index !== item.index) : rows.map((row, index) => (index === item.index ? next : row)), !empty);
    else if (!empty && rows.length < 64 && item.entry.supported) setRows([...rows, next]);
  }
  return (
    <>
      {model && (
        <div className="extra-banner" role="note">
          <ExtraIcon name="warning" />
          <div>
            <strong>Colors recolor the game's own model</strong>
            <p>They build without a replacement model: only the changed materials go into the mod, and the hero's meshes and textures stay the game's own.</p>
          </div>
          <button disabled={disabled} onClick={() => update(changeCustomization(project, { model_enabled: false }))}>
            Build without a replacement model
          </button>
        </div>
      )}
      <div id="extras-panel" role="tabpanel" aria-labelledby="extras-tab-colors" className="extras-layout">
        <section className="extras-library" aria-label="Hero materials">
          <div className="extras-library-heading">
            <div>
              <h2>Colors library</h2>
              <p>Recolor the hero's clothes and body without a replacement model.</p>
            </div>
            <span className="extra-library-count">{catalog ? `${items.length} materials` : "From your game"}</span>
          </div>
          <div className="extras-search">
            <ExtraIcon name="search" size={18} />
            <input aria-label="Search materials" placeholder="Search materials…" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
          {discovering ? (
            <div className="extra-empty"><span className="extra-spinner" /><h3>Finding the hero's materials</h3><p>Reading the hero's model and the game models that share its materials.</p></div>
          ) : !catalog ? (
            <div className="extra-empty">
              <div className="extra-empty-symbol"><ExtraIcon name="palette" size={32} /></div>
              <h3>{canDiscover ? "Load your character's assets" : "Connect your game to get started"}</h3>
              <p>{canDiscover ? "The hero's materials come from your game files." : "Choose the original game VPK in Setup, then return here."}</p>
              {canDiscover ? (
                <button className="extra-primary" onClick={reload} disabled={disabled}>Load character assets<ExtraIcon name="arrow" size={16} /></button>
              ) : openSetup && <button className="extra-primary" onClick={openSetup}>Open Setup<ExtraIcon name="arrow" size={16} /></button>}
            </div>
          ) : visible.length === 0 ? (
            <div className="extra-empty"><ExtraIcon name="search" size={32} /><h3>{query ? "No matching materials" : "No materials found"}</h3><p>{query ? "Try another name." : "The hero's model lists no materials in this game version."}</p></div>
          ) : (
            <div className="extras-assets extras-asset-list" role="list" aria-label="Materials">
              {visible.map((item) => {
                const status = item.row ? (item.row.enabled === false || !active ? "Paused" : "Changed") : !item.entry.supported ? "View only" : item.entry.shared ? "Shared" : "";
                const shown = item.row?.color ?? item.entry.tint ?? [1, 1, 1];
                return (
                  <div role="listitem" key={item.entry.target}>
                    <button className={`extras-asset ${fold(selected) === fold(item.entry.target) ? "selected" : ""} ${item.row ? "has-replacement" : ""}`} aria-pressed={fold(selected) === fold(item.entry.target)} onClick={() => setSelected(item.entry.target)}>
                      <span className="hero-color-swatch" aria-hidden="true" style={{ background: css(shown as Rgb), filter: heroColorFilter(item.entry, item.row) }} />
                      <div className="extra-asset-name">
                        <strong>{item.entry.label}</strong>
                        <span>{item.missing ? "Saved material unavailable" : item.entry.path.split("/").slice(-3, -2)[0] ?? ""}</span>
                      </div>
                      <span className={`extra-asset-state ${status === "Changed" ? "changed" : ""}`}>{status}</span>
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </section>
        <aside className="extras-inspector" aria-label="Color editor">
          {selection ? (
            <HeroColorEditor
              key={selection.entry.target}
              item={selection}
              hero={project.original_model}
              revision={revision}
              disabled={disabled}
              full={rows.length >= 64}
              edit={(changes, removed) => edit(selection, changes, removed)}
              remove={() => setRows(rows.filter((_, i) => i !== selection.index), false)}
            />
          ) : (
            <div className="extra-editor-empty">
              <span className="extras-eyebrow">HERO COLORS</span>
              <h2>Choose a material</h2>
              <p>Pick one of the hero's materials to change its tint, saturation or brightness. For black clothes, set the tint to black or the brightness to 0%. To change only some of its colors, add a color range.</p>
              <ul>
                <li><ExtraIcon name="check" size={16} />The game's textures and model stay unchanged</li>
                <li><ExtraIcon name="check" size={16} />Only the changed materials go into the mod</li>
                <li><ExtraIcon name="check" size={16} />Shared materials are marked before you change them</li>
              </ul>
            </div>
          )}
        </aside>
      </div>
    </>
  );
}

function HeroColorEditor({ item, hero, revision, disabled, full, edit, remove }: {
  item: HeroColorItem; hero: string; revision: number; disabled: boolean; full: boolean;
  edit: (changes: Partial<HeroMaterial>, removed?: (keyof HeroMaterial)[]) => void; remove: () => void;
}) {
  const { entry, row } = item;
  const editable = entry.supported && !item.missing && (Boolean(row) || !full);
  const { url, loading, error, media } = useExtraMedia(
    entry.color_texture && !item.missing ? { hero_path: hero, target: entry.target, kind: "materials", max_dimension: 512 } : null,
    `${entry.original_sha256}:${revision}`,
  );
  const [mask, setMask] = useState(false), [picking, setPicking] = useState<number | null>(null);
  const ranges = row?.ranges ?? [];
  const unsupported = typeof media?.details.ranges_unsupported === "string" ? media.details.ranges_unsupported : "";
  const setRanges = (next: HeroColorRange[]) => edit(next.length ? { ranges: next } : {}, next.length ? [] : ["ranges"]);
  function pick(color: [number, number, number]) {
    if (picking === null) return;
    const current = ranges[picking];
    if (current) setRanges(ranges.map((range, i) => (i === picking ? (rangeAround(color, range) as HeroColorRange) : range)));
    setPicking(null);
  }
  return (
    <>
      <header className="extra-inspector-heading">
        <div><span className="extras-eyebrow">SELECTED MATERIAL</span><h2>{entry.label}</h2></div>
        <span className={`extra-pill ${row ? "ready" : ""}`}>{row ? (row.enabled === false ? "Paused" : "Changed") : entry.supported ? "Game colors" : "View only"}</span>
      </header>
      {item.missing ? (
        <div className="extra-editor-notice"><ExtraIcon name="warning" /><strong>This material is no longer in the game files</strong><p>Remove this saved color. Reload character assets after a game update.</p></div>
      ) : (
        <MaterialPreview item={item} url={url} loading={loading} error={error} mask={mask} picking={picking !== null} pick={pick} />
      )}
      {!entry.supported && !item.missing && (
        <p className="extra-original-kept"><strong>Game colors kept</strong>{entry.reason.replace(/^Integrity check failed:\s*/, "")}</p>
      )}
      {!row && full && entry.supported && <p className="extra-error">Hero colors hold at most 64 materials. Remove one before adding another.</p>}
      <div className="extra-edit-controls">
        {entry.supported && !item.missing && (
          <div className="hero-color-controls">
            {entry.tint ? (
              <>
                <label className="hero-color-check">
                  <input type="checkbox" checked={Boolean(row?.color)} disabled={disabled || !editable} onChange={(e) => (e.target.checked ? edit({ color: entry.tint ?? [1, 1, 1] }) : edit({}, ["color"]))} />
                  Change tint
                </label>
                {row?.color && <ColorField label="Tint" value={row.color} update={(color) => edit({ color })} disabled={disabled} />}
                {row?.color && <button className="extra-text-button" disabled={disabled} onClick={() => edit({ color: [0, 0, 0] })}>Black</button>}
                <p className="hint">{entry.tint_masked ? "The game limits this material's tint to some areas (its tint mask)." : "The tint covers this whole material, including any skin it holds; check the preview."}</p>
              </>
            ) : <p className="hint">This material has no tint setting.</p>}
            {entry.albedo ? (
              <div className="color-sliders">
                <Slider label="Saturation" value={row?.saturation ?? 1} update={(saturation) => edit({ saturation })} disabled={disabled || !editable} />
                <Slider label="Brightness" value={row?.brightness ?? 1} update={(brightness) => edit({ brightness })} disabled={disabled || !editable} />
              </div>
            ) : <p className="hint">This material has no saturation or brightness setting.</p>}
            {entry.color_texture && (
              <section className="hero-color-ranges" aria-label="Color ranges">
                <div className="hero-color-ranges-heading">
                  <strong>Color ranges</strong>
                  <p className="hint">Recolor only the colors you pick, for example the clothes but not the skin, with a real hue shift. The mod gets a recolored copy of this material's base-color texture.</p>
                </div>
                {unsupported ? (
                  <p className="hint">{unsupported}</p>
                ) : (
                  <>
                    {ranges.map((range, index) => (
                      <RangeEditor
                        key={index}
                        index={index}
                        range={range}
                        disabled={disabled || !editable}
                        picking={picking === index}
                        pick={() => setPicking(picking === index ? null : index)}
                        update={(changes) => setRanges(ranges.map((r, i) => (i === index ? { ...r, ...changes } : r)))}
                        remove={() => {
                          setPicking(null);
                          setRanges(ranges.filter((_, i) => i !== index));
                        }}
                      />
                    ))}
                    {picking !== null && <p className="hint" role="status">Click the Game preview on the color this range should select.</p>}
                    <div className="hero-color-range-actions">
                      <button
                        disabled={disabled || !editable || ranges.length >= 8}
                        onClick={() => {
                          setRanges([...ranges, { enabled: true, ...RANGE_DEFAULTS }]);
                          setPicking(ranges.length);
                          setMask(true);
                        }}
                      >
                        Add color range
                      </button>
                      <label className="hero-color-check">
                        <input type="checkbox" checked={mask} onChange={(e) => setMask(e.target.checked)} />
                        Show selection
                      </label>
                    </div>
                  </>
                )}
              </section>
            )}
          </div>
        )}
        {entry.shared && !item.missing && (
          <div className="extra-shared">
            <ExtraIcon name="warning" size={18} />
            <div>
              <strong>Shared material</strong>
              <p>Also used by {entry.used_by.map((path) => stem(path.replace(/\.vmdl_c$/, ""))).join(", ")}. Recoloring it changes those models too.</p>
              {row && (
                <label><input type="checkbox" checked={row.allow_shared ?? false} disabled={disabled} onChange={(e) => edit({ allow_shared: e.target.checked })} />Recolor everywhere it is used</label>
              )}
            </div>
          </div>
        )}
        {row && (
          <div className="extra-replacement-actions">
            <label className="extra-toggle">
              <input type="checkbox" checked={row.enabled !== false} disabled={disabled} onChange={(e) => edit({ enabled: e.target.checked })} />
              <span aria-hidden="true" />
              <strong>Include in build</strong>
            </label>
            <button className="extra-remove" disabled={disabled} onClick={remove}><ExtraIcon name="close" size={15} />Remove color</button>
          </div>
        )}
        <p className="extra-inline-note">
          {ranges.length
            ? "The build changes this material's color settings and adds a recolored copy of its base-color texture; the game's own files stay unchanged. The preview is approximate, so check the result in game."
            : "The build changes only this material's color settings; its textures stay the game's own. The preview is approximate, so check the result in game."}
        </p>
        <details className="extra-file-details">
          <summary>File details<ExtraIcon name="folder" size={15} /></summary>
          <dl>
            <dt>Game material</dt><dd>{entry.target}</dd>
            {entry.color_texture && <><dt>Base-color texture</dt><dd>{entry.color_texture}</dd></>}
            {entry.shader && <><dt>Shader</dt><dd>{entry.shader}</dd></>}
            {entry.albedo && <><dt>Game saturation · brightness</dt><dd>{Math.round(entry.albedo[1] * 100)}% · {Math.round(entry.albedo[2] * 100)}%</dd></>}
          </dl>
        </details>
      </div>
    </>
  );
}

const percent = (value: number) => `${Math.round(value * 100)}%`;
function RangeSlider({ label, value, min, max, step, format, update, disabled }: {
  label: string; value: number; min: number; max: number; step: number;
  format: (value: number) => string; update: (value: number) => void; disabled: boolean;
}) {
  return (
    <label className="color-slider">
      <span>{label.replace(/^Range \d+ /, "")}</span>
      <input type="range" aria-label={label} min={min} max={max} step={step} value={value} disabled={disabled} onChange={(event) => update(Number(event.target.value))} />
      <output>{format(value)}</output>
    </label>
  );
}
/** One color range: what it selects and how it changes it. */
function RangeEditor({ index, range, disabled, picking, pick, update, remove }: {
  index: number; range: HeroColorRange; disabled: boolean; picking: boolean;
  pick: () => void; update: (changes: Partial<HeroColorRange>) => void; remove: () => void;
}) {
  const values = withDefaults(range);
  const name = `Range ${index + 1}`;
  const middle = ([low, high]: [number, number]) => (low + high) / 2;
  const centre = rgb(values.hue, middle(values.saturation_range), middle(values.brightness_range)).map((v) => Math.round(v * 255)) as [number, number, number];
  const result = recolorTexel([values], [...centre, 255]);
  // Keep each range's start at or below its end.
  const bound = (key: "saturation_range" | "brightness_range", side: 0 | 1, value: number) => {
    const next: [number, number] = [...values[key]];
    next[side] = value;
    if (next[0] > next[1]) next[1 - side] = value;
    update({ [key]: next });
  };
  const slider = (label: string, value: number, min: number, max: number, step: number, format: (v: number) => string, set: (v: number) => void) => (
    <RangeSlider label={`${name} ${label}`} value={value} min={min} max={max} step={step} format={format} update={set} disabled={disabled} />
  );
  return (
    <fieldset className="hero-color-range" aria-label={name}>
      <div className="hero-color-range-head">
        <span className="hero-color-swatch small" aria-hidden="true" style={{ background: css(centre.map((v) => v / 255) as Rgb) }} />
        <ExtraIcon name="arrow" size={14} />
        <span className="hero-color-swatch small" aria-hidden="true" style={{ background: css(result.slice(0, 3).map((v) => v / 255) as Rgb) }} />
        <strong>{name}</strong>
        <button className="extra-text-button" aria-pressed={picking} disabled={disabled} onClick={pick}>{picking ? "Picking…" : "Pick color"}</button>
        <label className="hero-color-check">
          <input type="checkbox" checked={range.enabled !== false} disabled={disabled} onChange={(e) => update({ enabled: e.target.checked })} />
          Use
        </label>
        <button className="extra-text-button" disabled={disabled} onClick={remove}>Remove</button>
      </div>
      <div className="color-sliders">
        <span className="hero-color-range-group">Selects</span>
        {slider("hue", values.hue, 0, 360, 1, (v) => `${Math.round(v)}°`, (hue) => update({ hue }))}
        {slider("hue width", values.hue_width, 0, 180, 1, (v) => (v >= 180 ? "All" : `±${Math.round(v)}°`), (hue_width) => update({ hue_width }))}
        {slider("saturation from", values.saturation_range[0], 0, 1, 0.01, percent, (v) => bound("saturation_range", 0, v))}
        {slider("saturation to", values.saturation_range[1], 0, 1, 0.01, percent, (v) => bound("saturation_range", 1, v))}
        {slider("brightness from", values.brightness_range[0], 0, 1, 0.01, percent, (v) => bound("brightness_range", 0, v))}
        {slider("brightness to", values.brightness_range[1], 0, 1, 0.01, percent, (v) => bound("brightness_range", 1, v))}
        {slider("softness", values.softness, 0, 1, 0.05, percent, (softness) => update({ softness }))}
        <span className="hero-color-range-group">Changes</span>
        {slider("hue shift", values.hue_shift, -180, 180, 1, (v) => `${Math.round(v)}°`, (hue_shift) => update({ hue_shift }))}
        {slider("saturation", values.saturation, 0, 2, 0.05, percent, (saturation) => update({ saturation }))}
        {slider("brightness", values.brightness, 0, 2, 0.05, percent, (brightness) => update({ brightness }))}
      </div>
    </fieldset>
  );
}
