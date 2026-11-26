/** The Texture tab: the selected material's base-color image flat, original | yours side by
 * side, drawn with the same recipe as the 3D view. What a color range selects is outlined over
 * the real image (a glow would hide the pixels). While a color is being picked, a click on
 * either image reads the original color under it. */
import { useEffect, useRef, type CSSProperties, type MouseEvent } from "react";
import { applyRecipe, areaAverage, containBox, imagePoint, isPlain, PICK_AREA_PX, recipeKey, type PixelRecipe } from "./colorPixels";
import type { Pixels } from "./colorImages";
import { cssColor } from "./colorMath";
import { rangeWeight, hsv, type ColorRange } from "./heroColorRanges";
import { selectionKey } from "./colorCoverage";
import { rangeMasks, type MaterialParts } from "./colorParts";
import type { Rgb } from "./colorAdapter";
import { useT } from "../i18n";
// i18n-area: colors

/** Draws `recipe` over the pixels into a canvas, at most once per animation frame. */
function RecipeCanvas({ pixels, recipe, label, live, parts }: { pixels: Pixels; recipe: PixelRecipe; label: string; live: boolean; parts: MaterialParts | null }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const key = recipeKey(recipe) + (parts && recipe.ranges.some((range) => range.part) ? "|parts" : "");
  const frame = useRef(0);
  useEffect(() => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      const element = canvas.current, ctx = element?.getContext("2d");
      if (!element || !ctx) return;
      const source = live ? pixels.small : pixels.full;
      element.width = source.width;
      element.height = source.height;
      if (isPlain(recipe)) { ctx.drawImage(pixels.source, 0, 0, source.width, source.height); return; }
      const copy = new ImageData(new Uint8ClampedArray(source.data), source.width, source.height);
      applyRecipe(copy.data, recipe, undefined, rangeMasks(recipe.ranges, parts, source.width, source.height));
      ctx.putImageData(copy, 0, 0);
    });
    return () => cancelAnimationFrame(frame.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key stands for the recipe
  }, [pixels, key, live]);
  return <canvas ref={canvas} role="img" aria-label={label} />;
}

/** The outline of what a range selects (pixels at least half selected; a band EDGE pixels wide,
 * so it stays visible when the image is shown smaller), with a light fill, drawn at the live
 * size (at most 512 px) in `color` into an RGBA buffer. */
const EDGE = 2;
export function selectionOutline(source: { data: Uint8ClampedArray; width: number; height: number }, range: ColorRange, color: Rgb, out: Uint8ClampedArray, mask: Uint8Array | null = null) {
  const { data, width, height } = source;
  const inside = new Uint8Array(width * height);
  // A range for one part: only its part's pixels (`mask`, colorParts.ts).
  for (let p = 0, i = 0; p < inside.length; p++, i += 4) inside[p] = (!mask || mask[p] === 1) && rangeWeight(range, hsv(data[i] / 255, data[i + 1] / 255, data[i + 2] / 255)) >= 0.5 ? 1 : 0;
  const [r, g, b] = color.map((c) => Math.round(c * 255));
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const p = y * width + x, i = p * 4;
      let alpha = 0;
      if (inside[p]) {
        let edge = x < EDGE || y < EDGE || x >= width - EDGE || y >= height - EDGE;
        for (let dy = -EDGE; dy <= EDGE && !edge; dy++) for (let dx = -EDGE; dx <= EDGE && !edge; dx++) edge = !inside[(y + dy) * width + x + dx];
        alpha = edge ? 255 : 72;
      }
      out[i] = r; out[i + 1] = g; out[i + 2] = b; out[i + 3] = alpha;
    }
  }
}
/** The selection outline over one image (same size and fit as the image under it). */
function SelectionCanvas({ pixels, range, color, parts }: { pixels: Pixels; range: ColorRange; color: Rgb; parts: MaterialParts | null }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const key = selectionKey(range) + color.join() + (parts && range.part ? "|parts" : "");
  const frame = useRef(0);
  useEffect(() => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      const element = canvas.current, ctx = element?.getContext("2d");
      if (!element || !ctx) return;
      const source = pixels.small;
      element.width = source.width;
      element.height = source.height;
      const image = ctx.createImageData(source.width, source.height);
      selectionOutline(source, range, color, image.data, rangeMasks([range], parts, source.width, source.height)?.[0] ?? null);
      ctx.putImageData(image, 0, 0);
    });
    return () => cancelAnimationFrame(frame.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key stands for the range and the color
  }, [pixels, key]);
  return <canvas ref={canvas} className="color-texture-selection" aria-hidden="true" />;
}

/** A click on the image: its color (the area under the pointer), where on the view it was, and
 * the texture coordinates of the point (u = x / width, v = y / height: v = 0 is the first row). */
export interface TexturePick { color: [number, number, number]; x: number; y: number; w: number; u?: number; v?: number }
export function ColorTextureView({ label, originalLabel, url, pixels, original, yours, tint, originalTint, picking, pick, loading, error, live, cssFilter, approximate, selection, parts = null }: {
  label: string; originalLabel: string; url: string; pixels: Pixels | null;
  original: PixelRecipe; yours: PixelRecipe; tint: Rgb; originalTint: Rgb;
  picking: boolean; pick: (hit: TexturePick) => void;
  loading: boolean; error: string; live: boolean;
  /** The preview only approximates the build for this material. */
  approximate: boolean;
  /** Without a 2D canvas the plain image is shown with this approximation. */
  cssFilter: (view: "original" | "yours") => string | undefined;
  /** What one color range selects, outlined in this color over both images. */
  selection?: { range: ColorRange; color: Rgb } | null;
  /** The material's parts, for ranges that change one part (colorParts.ts). */
  parts?: MaterialParts | null;
}) {
  const t = useT();
  function choose(event: MouseEvent<HTMLElement>) {
    if (!picking || !pixels) return;
    // The image is fitted whole inside its element (object-fit: contain): read the click against
    // the box it is drawn in, over the small area under the pointer.
    const element = event.currentTarget.querySelector("canvas:not(.color-texture-selection), img") ?? event.currentTarget;
    const drawn = containBox(element.getBoundingClientRect(), pixels.width, pixels.height);
    const point = imagePoint(drawn, event.clientX, event.clientY, pixels.width, pixels.height);
    if (!point) return;
    const radius = ((PICK_AREA_PX / 2) * pixels.width) / drawn.width;
    const view = event.currentTarget.closest(".color-texture-view")?.getBoundingClientRect() ?? event.currentTarget.getBoundingClientRect();
    pick({ color: areaAverage(pixels.full, point.x, point.y, radius), x: event.clientX - view.left, y: event.clientY - view.top, w: view.width, u: point.x / pixels.width, v: point.y / pixels.height });
  }
  return (
    <div className="color-texture-view" style={selection ? ({ "--glow": cssColor(selection.color) } as CSSProperties) : undefined}>
      {(["original", "yours"] as const).map((view) => {
        const first = view === "original";
        const shownTint = first ? originalTint : tint;
        return (
          <section key={view}>
            <div className="color-texture-label">
              <span>{first ? originalLabel : t("Yours")}</span>
              {!first && <small>{approximate ? t("Approximate") : t("As built")}</small>}
            </div>
            <div className={`color-texture-stage ${picking ? "picking" : ""}`} onClick={choose}>
              {pixels ? (
                <RecipeCanvas pixels={pixels} recipe={first ? original : yours} live={live && !first} parts={parts} label={first ? t("{material} base color", { material: label }) : t("{material} base color with your colors", { material: label })} />
              ) : url ? (
                <img src={url} alt={t("{material} base color", { material: label })} style={{ filter: cssFilter(view) }} />
              ) : (
                <div className="color-texture-blank" />
              )}
              {pixels && selection && <SelectionCanvas pixels={pixels} range={selection.range} color={selection.color} parts={parts} />}
              <span className="color-texture-tint" title={t("Tint")} style={{ background: cssColor(shownTint) }} />
              {loading && <span className="extra-spinner" role="status" aria-label={t("Loading preview")} />}
              {error && <small className="extra-thumbnail-error">{t("Preview unavailable")}</small>}
            </div>
          </section>
        );
      })}
    </div>
  );
}
