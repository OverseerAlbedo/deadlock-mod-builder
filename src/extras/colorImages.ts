/** The Colors page's images: each material's original base-color pixels (loaded once, at
 * preview size) and the textures the 3D view draws, recolored here from those pixels.
 * - Game hero: images through `useExtraMedia` (512 px, the Extras preview queue and cache);
 *   one loader per material, shared by the list thumbnail, the 3D view and the editor.
 * - Replacement: `api.previewImage(path, 1024)`, kept per path until the project changes.
 * These textures belong to the page (it disposes them); the shared preview store of the
 * Physics page is not used, so a color edit here never reloads or evicts its images.
 * While a control is dragged only the selected material is recolored, from its cached
 * original pixels, at most once per animation frame and at no more than 512 px; when the
 * drag ends it is drawn again at full size. The selection glow's map (what one color range
 * selects) is drawn at half the live size (at most 256 px) from the same original pixels, in the
 * same pass as a live recolor when both change. */
/** The selection map takes every second pixel of the live size in each direction. */
export const SELECTION_STEP = 2;
import { useEffect, useRef, useState } from "react";
import { CanvasTexture, RepeatWrapping, SRGBColorSpace, type Texture } from "three";
import { api } from "../api";
import { previewError, useExtraMedia } from "./media";
import type { ExtraMedia, ExtraPreviewRequest } from "./models";
import { applyRecipe, isPlain, recipeKey, selectionPixels, selectionSize, type PixelRecipe } from "./colorPixels";
import { selectionKey } from "./colorCoverage";
import type { ColorRange } from "./heroColorRanges";
import { rangeMasks, type MaterialParts } from "./colorParts";

export const LIVE_SIZE = 512;
export interface ImageState { url: string; loading: boolean; error: string; media: ExtraMedia | null }
const EMPTY: ImageState = { url: "", loading: false, error: "", media: null };

/** One hero material's image through the Extras preview queue; reports its state upward. */
export function HeroImageLoader({ id, request, identity, report }: { id: string; request: ExtraPreviewRequest; identity: string; report: (id: string, state: ImageState) => void }) {
  const { url, loading, error, media } = useExtraMedia(request, identity);
  useEffect(() => { report(id, { url, loading, error, media }); }, [id, url, loading, error, media, report]);
  useEffect(() => () => report(id, EMPTY), [id, report]);
  return null;
}

/** Replacement images: one load per path (1024 px, as the character preview reads them). */
const files = new Map<string, Promise<string>>();
export function clearFileImages() {
  for (const promise of files.values()) void promise.then((url) => URL.revokeObjectURL(url), () => undefined);
  files.clear();
}
export function fileImageUrl(path: string): Promise<string> {
  let known = files.get(path);
  if (!known) {
    known = Promise.resolve().then(() => api.previewImage(path, 1024)).then((bytes) => URL.createObjectURL(new Blob([bytes], { type: "image/png" })));
    known.catch(() => files.delete(path));
    files.set(path, known);
  }
  return known;
}
/** The image of each of these paths (loading and failures included), for the paths wanted. */
export function useFileImages(paths: string[], project: string): Record<string, ImageState> {
  const [state, setState] = useState<Record<string, ImageState>>({});
  const key = [...new Set(paths)].sort().join("\n");
  useEffect(() => { clearFileImages(); setState({}); }, [project]);
  useEffect(() => {
    let live = true;
    for (const path of key ? key.split("\n") : []) {
      setState((current) => (current[path] && (current[path].url || current[path].loading || current[path].error) ? current : { ...current, [path]: { ...EMPTY, loading: true } }));
      fileImageUrl(path).then(
        (url) => { if (live) setState((current) => (current[path]?.url === url ? current : { ...current, [path]: { url, loading: false, error: "", media: null } })); },
        (error) => { if (live) setState((current) => ({ ...current, [path]: { url: "", loading: false, error: previewError(error), media: null } })); },
      );
    }
    return () => { live = false; };
  }, [key]);
  return state;
}

/** Original pixels of one image: the decoded image in a canvas, and its RGBA bytes at full size
 * and (for live edits) at most 512 px, read only when a recipe needs them. */
export class Pixels {
  private fullData?: ImageData;
  private smallData?: ImageData;
  constructor(readonly source: HTMLCanvasElement) {}
  get width() { return this.source.width; }
  get height() { return this.source.height; }
  /** The live size is smaller than the full one. */
  get scaled() { return Math.max(this.width, this.height) > LIVE_SIZE; }
  get full(): ImageData {
    this.fullData ??= this.source.getContext("2d", { willReadFrequently: true })!.getImageData(0, 0, this.width, this.height);
    return this.fullData;
  }
  get small(): ImageData {
    if (!this.scaled) return this.full;
    if (!this.smallData) {
      const scale = LIVE_SIZE / Math.max(this.width, this.height);
      const w = Math.max(1, Math.round(this.width * scale)), h = Math.max(1, Math.round(this.height * scale));
      const ctx = context(w, h)!;
      ctx.imageSmoothingQuality = "high";
      ctx.drawImage(this.source, 0, 0, w, h);
      this.smallData = ctx.getImageData(0, 0, w, h);
    }
    return this.smallData;
  }
}
const pixelCache = new Map<string, Promise<Pixels | null>>();
const canDraw = () => typeof document !== "undefined" && typeof Image !== "undefined" && !(globalThis as { process?: { env?: { VITEST?: string } } }).process?.env?.VITEST;
function context(width: number, height: number) {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  return canvas.getContext("2d", { willReadFrequently: true });
}
/** The pixels behind an image URL (null without a 2D canvas, as in tests). */
export function loadPixels(url: string): Promise<Pixels | null> {
  if (!url || !canDraw()) return Promise.resolve(null);
  let known = pixelCache.get(url);
  if (!known) {
    known = (async () => {
      const image = new Image();
      image.src = url;
      await image.decode();
      const width = image.naturalWidth, height = image.naturalHeight;
      const ctx = width && height ? context(width, height) : null;
      if (!ctx) return null;
      ctx.drawImage(image, 0, 0);
      return new Pixels(ctx.canvas);
    })().catch(() => null);
    pixelCache.set(url, known);
    // Keep the cache bounded (the page holds at most a few dozen images).
    while (pixelCache.size > 160) pixelCache.delete(pixelCache.keys().next().value!);
  }
  return known;
}
/** Pixels for each id's URL, as they finish loading. */
export function usePixels(urls: Record<string, string>): Record<string, Pixels> {
  const [pixels, setPixels] = useState<Record<string, { url: string; pixels: Pixels }>>({});
  const key = JSON.stringify(Object.entries(urls).sort());
  useEffect(() => {
    let live = true;
    for (const [id, url] of Object.entries(urls)) {
      if (!url) continue;
      void loadPixels(url).then((result) => {
        if (live && result) setPixels((current) => (current[id]?.url === url ? current : { ...current, [id]: { url, pixels: result } }));
      });
    }
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key stands for the URLs
  }, [key]);
  return Object.fromEntries(Object.entries(pixels).filter(([id, value]) => urls[id] === value.url).map(([id, value]) => [id, value.pixels]));
}

/** One material's textures: full size and live size, each a canvas the recipe is drawn into. */
type Slot = "plain" | "full" | "small";
function canvasTexture(canvas: HTMLCanvasElement) {
  const texture = new CanvasTexture(canvas);
  texture.colorSpace = SRGBColorSpace;
  texture.flipY = false;
  // Some hero meshes use texture coordinates beyond 0–1.
  texture.wrapS = texture.wrapT = RepeatWrapping;
  return texture;
}
class MaterialTexture {
  private canvases: Partial<Record<Slot, HTMLCanvasElement>> = {};
  private textures: Partial<Record<Slot, CanvasTexture>> = {};
  private drawn: Partial<Record<Slot, string>> = {};
  private selection?: { canvas: HTMLCanvasElement; image: ImageData; texture: CanvasTexture; key: string };
  shown: Slot = "plain";
  constructor(readonly pixels: Pixels) {}
  /** Draw `recipe` (nothing when already drawn) and, with `select`, the map of what that range
   * selects; the time it took in ms. A plain recipe shows the original image itself; `small`
   * draws at the live size. */
  draw(size: "full" | "small", recipe: PixelRecipe, select: ColorRange | null = null, parts: MaterialParts | null = null): number {
    const start = performance.now();
    // Ranges for one part are drawn with their part's mask once the material's parts are known.
    const selectKey = select ? selectionKey(select) + (parts && select.part ? "|parts" : "") : "";
    let overlay = select && this.selection?.key !== selectKey ? this.selectionBuffer() : null;
    const selectMask = (source: ImageData) => (select ? rangeMasks([select], parts, source.width, source.height)?.[0] ?? null : null);
    let drew = false;
    if (isPlain(recipe)) {
      this.shown = "plain";
      if (!this.textures.plain) { this.textures.plain = canvasTexture(this.pixels.source); this.textures.plain.needsUpdate = true; drew = true; }
    } else {
      const slot: Slot = size === "small" && this.pixels.scaled ? "small" : "full";
      this.shown = slot;
      const key = recipeKey(recipe) + (parts && recipe.ranges.some((range) => range.part) ? "|parts" : "");
      const source = slot === "small" ? this.pixels.small : this.pixels.full;
      const ctx = this.drawn[slot] === key ? null : this.canvas(slot, source).getContext("2d");
      if (ctx) {
        const copy = new ImageData(new Uint8ClampedArray(source.data), source.width, source.height);
        // One pass for both when the selection map has the size being drawn (a live drag).
        const together = overlay && source === this.pixels.small;
        const reach = rangeMasks(recipe.ranges, parts, source.width, source.height);
        applyRecipe(copy.data, recipe, together ? { range: select!, out: overlay!.image.data, width: source.width, step: SELECTION_STEP, mask: selectMask(source) } : undefined, reach);
        if (together) { this.finishSelection(selectKey); overlay = null; }
        ctx.putImageData(copy, 0, 0);
        const texture = (this.textures[slot] ??= canvasTexture(this.canvases[slot]!));
        texture.needsUpdate = true;
        this.drawn[slot] = key;
        drew = true;
      }
    }
    if (overlay) {
      selectionPixels({ range: select!, out: overlay.image.data, width: this.pixels.small.width, step: SELECTION_STEP, mask: selectMask(this.pixels.small) }, this.pixels.small.data);
      this.finishSelection(selectKey);
      drew = true;
    }
    return drew ? Math.max(0.001, performance.now() - start) : 0;
  }
  private canvas(slot: Slot, source: ImageData) {
    let canvas = this.canvases[slot];
    if (!canvas) {
      canvas = document.createElement("canvas");
      canvas.width = source.width;
      canvas.height = source.height;
      this.canvases[slot] = canvas;
    }
    return canvas;
  }
  private selectionBuffer() {
    if (!this.selection) {
      const small = this.pixels.small;
      const size = selectionSize(small.width, small.height, SELECTION_STEP);
      const canvas = document.createElement("canvas");
      canvas.width = size.width;
      canvas.height = size.height;
      const texture = new CanvasTexture(canvas);
      texture.flipY = false;
      texture.wrapS = texture.wrapT = RepeatWrapping;
      this.selection = { canvas, image: new ImageData(size.width, size.height), texture, key: "" };
    }
    return this.selection;
  }
  private finishSelection(key: string) {
    const selection = this.selection!;
    selection.canvas.getContext("2d")?.putImageData(selection.image, 0, 0);
    selection.texture.needsUpdate = true;
    selection.key = key;
  }
  get texture(): Texture | undefined {
    return this.textures[this.shown];
  }
  /** The selection map of `select`, once drawn. */
  selectionTexture(select: ColorRange, parts: MaterialParts | null = null): Texture | undefined {
    return this.selection && this.selection.key === selectionKey(select) + (parts && select.part ? "|parts" : "") ? this.selection.texture : undefined;
  }
  dispose() {
    Object.values(this.textures).forEach((texture) => texture?.dispose());
    this.selection?.texture.dispose();
  }
}

export interface LiveStats { lastMs: number; updates: number; size: string }
/** The 3D view's textures: every material with pixels drawn with its recipe. `live` is the
 * material being dragged (drawn small, once per frame); the rest are drawn at full size,
 * a few per frame so a view switch never stalls the page. */
export function useColorTextures(pixels: Record<string, Pixels>, recipes: Record<string, PixelRecipe>, live: string | null, select: { id: string; range: ColorRange } | null = null, parts: Record<string, MaterialParts | null> = {}): { textures: Record<string, Texture>; selection: Texture | undefined; version: number; stats: LiveStats } {
  const store = useRef(new Map<string, MaterialTexture>());
  const latest = useRef({ pixels, recipes, live, select, parts });
  latest.current = { pixels, recipes, live, select, parts };
  const partsSignature = Object.keys(parts).filter((id) => parts[id]).sort().join("|");
  const frame = useRef(0);
  const [version, setVersion] = useState(0);
  const stats = useRef<LiveStats>({ lastMs: 0, updates: 0, size: "" });
  useEffect(() => () => { cancelAnimationFrame(frame.current); store.current.forEach((t) => t.dispose()); store.current.clear(); }, []);
  const recipeSignature = JSON.stringify(recipes);
  const selectSignature = select ? `${select.id}:${selectionKey(select.range)}` : "";
  useEffect(() => {
    // Drop textures of images no longer used.
    for (const [id, texture] of store.current) if (pixels[id] !== texture.pixels) { texture.dispose(); store.current.delete(id); }
    for (const [id, value] of Object.entries(pixels)) if (!store.current.has(id)) store.current.set(id, new MaterialTexture(value));
    cancelAnimationFrame(frame.current);
    const step = () => {
      const { recipes: wanted, live: dragged, select: lit, parts: known } = latest.current;
      let changed = false;
      const started = performance.now();
      const order = [...store.current.keys()].sort((a, b) => Number(b === dragged) - Number(a === dragged));
      for (const id of order) {
        const texture = store.current.get(id)!;
        const recipe = wanted[id];
        if (!recipe) continue;
        const ms = texture.draw(id === dragged ? "small" : "full", recipe, lit && lit.id === id ? lit.range : null, known[id] ?? null);
        if (ms > 0) {
          changed = true;
          stats.current = { lastMs: ms, updates: stats.current.updates + 1, size: `${id === dragged ? "live" : "full"} ${texture.shown === "small" ? `${texture.pixels.small.width}×${texture.pixels.small.height}` : `${texture.pixels.width}×${texture.pixels.height}`}` };
          // Development builds only, read by a layout harness to report the time per update.
          if (import.meta.env.DEV) {
            const log = ((globalThis as { __dmbColorStats?: LiveStats[] }).__dmbColorStats ??= []);
            log.push(stats.current);
            if (log.length > 200) log.shift();
          }
        }
        // Keep each frame short; the rest is drawn on the next ones.
        if (performance.now() - started > 24) { frame.current = requestAnimationFrame(step); break; }
      }
      if (changed) setVersion((v) => v + 1);
    };
    frame.current = requestAnimationFrame(step);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the signatures stand for the recipes and the selection
  }, [pixels, recipeSignature, live, selectSignature, partsSignature]);
  const textures: Record<string, Texture> = {};
  for (const [id, texture] of store.current) if (texture.texture) textures[id] = texture.texture;
  const selection = select ? store.current.get(select.id)?.selectionTexture(select.range, parts[select.id] ?? null) : undefined;
  return { textures, selection, version, stats: stats.current };
}
