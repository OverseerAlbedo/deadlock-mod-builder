/** The character preview's colour textures, shared by the still and the movement view so
 * switching between them never loads the images again. Each material's assigned base-colour
 * image (Materials → Base color) is read at preview size, with the colour edits applied as the
 * build applies them (the same functions on the image's pixels): colour ranges first
 * (`recolorPixels`), then hue, saturation and brightness (`adjustPixels`). Face layers (blush,
 * tears) that are switched off are not drawn; shown ones are alpha-blended over the face. */
import { useEffect, useRef, useState } from 'react';
import { CanvasTexture, SRGBColorSpace, Texture } from 'three';
import { api } from '../api';
import type { Project } from '../models';
import { adjustPixels } from './colorAdjust';
import { recolorPixels, withDefaults, type ColorRange } from '../extras/heroColorRanges';
import { m, t } from '../i18n';
// i18n-area: preview

/** A material's colour edit (Appearance → Surface changes, Extras → Colors): its enabled colour
 * ranges (defaults filled in) and the hue shift, saturation and brightness after them. */
export interface TextureEdit { ranges: ColorRange[]; hue: number; saturation: number; brightness: number }
/** One image to load: the material it colours, the image, and the colour edit applied to it. */
export interface TextureRequest { material: string; path: string; edit: TextureEdit | null }
/** Previewed image size, as the still view has always loaded them. */
export const PREVIEW_TEXTURE_SIZE = 1024;
export const NO_TEXTURES = m('No assigned color images could be loaded. Assign a PNG or JPEG color texture first.');
/** A texture state's message as shown: this file's own sentence translated, the loaders' error
 * messages (one line per failed image) as they come. */
export const shownTextureMessage = (message: string) => message === NO_TEXTURES ? t(NO_TEXTURES) : message;

/** The colour edit for each material (Surface changes rows: colour ranges, hue, saturation,
 * brightness); only rows that are on and that change something. The last row for a material wins. */
export function colorEdits(project: Project): Map<string, TextureEdit> {
  const appearance = project.customization.appearance;
  const edits = new Map<string, TextureEdit>();
  if (!project.customization.enabled || !appearance?.enabled) return edits;
  for (const row of appearance.materials ?? []) {
    if (row.enabled === false || !row.target) continue;
    // A range for one part needs the part's mask, which only the Colors page draws: this
    // preview leaves it out rather than recolor the whole image.
    const ranges = (row.ranges ?? []).filter(range => range.enabled !== false && !range.part).map(withDefaults);
    if (row.hue === undefined && row.saturation === undefined && row.brightness === undefined && !row.ranges?.length) continue;
    const edit: TextureEdit = {ranges, hue: row.hue ?? 0, saturation: row.saturation ?? 1, brightness: row.brightness ?? 1};
    const neutral = !ranges.length && edit.hue === 0 && edit.saturation === 1 && edit.brightness === 1;
    if (neutral) edits.delete(row.target); else edits.set(row.target, edit);
  }
  return edits;
}
/** A colour edit applied to RGBA pixels in place, as the build changes the image. */
export function applyTextureEdit(data: Uint8ClampedArray, edit: TextureEdit) {
  if (edit.ranges.length) recolorPixels(edit.ranges, data);
  adjustPixels(data, edit);
}

/** A face layer (source alpha 0: blush, tears, other) the build leaves hidden: not drawn in the preview. */
export function hiddenFaceLayer(project: Project, material: string): boolean {
  const kind = project.materials[material]?.face_layer;
  return Boolean(kind) && project.customization.face_layers?.[kind!] !== true;
}
/** A face layer the build shows: drawn with its own alpha over the face. */
export function shownFaceLayer(project: Project, material: string): boolean {
  return Boolean(project.materials[material]?.face_layer) && !hiddenFaceLayer(project, material);
}

/** The images the preview needs for these materials (each once), skipping hidden face layers. */
export function textureRequests(project: Project, materials: Iterable<string>): TextureRequest[] {
  const edits = colorEdits(project), requests: TextureRequest[] = [];
  for (const material of new Set(materials)) {
    const path = project.materials[material]?.textures?.color;
    if (!path || hiddenFaceLayer(project, material)) continue;
    requests.push({material, path, edit: edits.get(material) ?? null});
  }
  return requests;
}
/** Whether any of these materials has a colour image assigned (the preview then starts textured). */
export const hasColorImages = (project: Project, materials: Iterable<string>) => textureRequests(project, materials).length > 0;
/** The store key: the image and the colour edit (every value of it, ranges included). */
export const requestKey = (request: TextureRequest) => `${request.path}\u0000${request.edit ? JSON.stringify(request.edit) : ''}`;

/** Image bytes to a three.js texture with the colour edit applied (replaced in tests: jsdom has no image decoding). */
export type DecodeTexture = (bytes: ArrayBuffer, edit: TextureEdit | null) => Promise<Texture>;
export const decodeTexture: DecodeTexture = async (bytes, edit) => {
  const url = URL.createObjectURL(new Blob([bytes], {type: 'image/png'}));
  try {
    const image = new Image();
    image.src = url;
    await image.decode();
    let texture: Texture;
    if (edit) {
      const canvas = document.createElement('canvas');
      canvas.width = image.naturalWidth; canvas.height = image.naturalHeight;
      const context = canvas.getContext('2d', {willReadFrequently: true});
      if (!context) throw new Error('The colour edit could not be drawn.');
      context.drawImage(image, 0, 0);
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
      applyTextureEdit(pixels.data, edit);
      context.putImageData(pixels, 0, 0);
      texture = new CanvasTexture(canvas);
    } else {
      texture = new Texture(image);
    }
    texture.colorSpace = SRGBColorSpace;
    texture.flipY = false;
    texture.needsUpdate = true;
    return texture;
  } finally { URL.revokeObjectURL(url); }
};

export interface LoadedTextures { textures: Record<string, Texture>; failures: string[] }
type Stamp = string | null;
const stampText = (stamp: {len: number; modified_ms: number} | null | undefined): Stamp => stamp ? `${stamp.len}:${stamp.modified_ms}` : null;

/** The textures of the open project, kept while the preview is closed (leaving the Physics page
 * unmounts it) so coming back reloads nothing. Keyed by image and colour edit; each entry keeps the
 * file's stamp (size and modification time) so a repainted image is noticed. It holds only what the
 * latest request set uses, is cleared when another project is opened, and is the only place that
 * disposes these textures: a canvas that unmounts never does (the next canvas uploads them again). */
export class TextureStore {
  private project: string | null = null;
  private entries = new Map<string, {texture: Texture; stamp: Stamp}>();
  private pending = new Map<string, Promise<Texture>>();
  /** Bumped by `clear`, so a load started for an earlier project is dropped. */
  epoch = 0;
  get size() { return this.entries.size; }
  /** Switch to a project, clearing what another project left. */
  use(project: string) { if (project !== this.project) { this.clear(); this.project = project; } }
  clear() { this.entries.forEach(entry => entry.texture.dispose()); this.entries.clear(); this.pending.clear(); this.project = null; this.epoch++; }
  entry(key: string) { return this.entries.get(key); }
  /** The stored textures for these requests (nothing for another project), without checking stamps. */
  cached(project: string, requests: TextureRequest[]): Record<string, Texture> {
    if (project !== this.project) return {};
    const out: Record<string, Texture> = {};
    for (const request of requests) { const entry = this.entries.get(requestKey(request)); if (entry) out[request.material] = entry.texture; }
    return out;
  }
  /** One load per image, colour edit and stamp at a time, shared by every view asking for it. */
  load(key: string, stamp: Stamp, run: () => Promise<Texture>): Promise<Texture> {
    const id = `${key}\u0000${stamp ?? ''}`;
    const known = this.pending.get(id);
    if (known) return known;
    const epoch = this.epoch;
    const promise = run().then(texture => {
      if (epoch !== this.epoch) { texture.dispose(); throw new Error('The project changed while the textures loaded.'); }
      const old = this.entries.get(key);
      if (old && old.texture !== texture) old.texture.dispose();
      this.entries.set(key, {texture, stamp});
      return texture;
    }).finally(() => { this.pending.delete(id); });
    this.pending.set(id, promise);
    return promise;
  }
  drop(key: string) { const entry = this.entries.get(key); if (entry) { entry.texture.dispose(); this.entries.delete(key); } }
  /** Free every texture these keys do not use. */
  retain(keys: Set<string>) { for (const key of [...this.entries.keys()]) if (!keys.has(key)) this.drop(key); }
}
export const textureStore = new TextureStore();

/** Bring the store up to date for these requests: stored textures whose file stamp is unchanged are
 * kept; new images, changed colour edits and changed files load again (only those). One stamp call
 * covers every path; when it fails, stored textures are kept and only missing ones load. A path
 * with no stamp (missing or unreadable) loads again so its error is shown. Null when superseded. */
export async function refreshTextures(project: string, requests: TextureRequest[], decode: DecodeTexture = decodeTexture, stale: () => boolean = () => false, store: TextureStore = textureStore): Promise<LoadedTextures | null> {
  store.use(project);
  const epoch = store.epoch;
  let stamps: Stamp[] | null = null;
  try { stamps = (await api.fileStamps(requests.map(request => request.path))).map(stampText); } catch { stamps = null; }
  const textures: Record<string, Texture> = {}, failures: string[] = [];
  for (const [i, request] of requests.entries()) {
    if (stale() || epoch !== store.epoch) return null;
    const key = requestKey(request), entry = store.entry(key);
    const stamp = stamps ? stamps[i] ?? null : null;
    if (entry && (stamps === null || (stamp !== null && entry.stamp === stamp))) { textures[request.material] = entry.texture; continue; }
    try {
      textures[request.material] = await store.load(key, stamps ? stamp : null, async () => decode(await api.previewImage(request.path, PREVIEW_TEXTURE_SIZE), request.edit));
    } catch (error) {
      if (epoch !== store.epoch) return null;
      store.drop(key);
      failures.push(`${request.material}: ${error instanceof Error ? error.message : JSON.stringify(error)}`);
    }
  }
  if (stale() || epoch !== store.epoch) return null;
  store.retain(new Set(requests.map(requestKey)));
  return {textures, failures};
}

export type TextureState = {status: 'idle' | 'loading' | 'ready' | 'failed'; textures: Record<string, Texture>; message: string};
const IDLE: TextureState = {status: 'idle', textures: {}, message: ''};
const sameTextures = (a: Record<string, Texture>, b: Record<string, Texture>) => { const keys = Object.keys(a); return keys.length === Object.keys(b).length && keys.every(key => a[key] === b[key]); };
/** The textures for the shown character from the shared store: stored ones at once (first frame),
 * then the file stamps are checked in the background and only changed images load again. Runs
 * while `enabled`, and again when the requests change or `revision` does (a reloaded scene re-checks the stamps). */
export function usePreviewTextures(project: Project, materials: string[], enabled: boolean, decode: DecodeTexture = decodeTexture, store: TextureStore = textureStore, revision?: unknown): TextureState {
  const requests = textureRequests(project, materials);
  const signature = JSON.stringify(requests);
  const [state, setState] = useState<TextureState>(() => {
    const cached = store.cached(project.id, requests);
    return Object.keys(cached).length ? {status: 'ready', textures: cached, message: ''} : IDLE;
  });
  const generation = useRef(0);
  useEffect(() => {
    if (!enabled) return;
    const version = ++generation.current;
    const stale = () => version !== generation.current;
    if (!requests.length) { setState({status: 'failed', textures: {}, message: NO_TEXTURES}); return; }
    const cached = store.cached(project.id, requests);
    const shown = Object.keys(cached).length > 0;
    setState(current => shown ? (current.status === 'ready' && sameTextures(current.textures, cached) ? current : {status: 'ready', textures: cached, message: current.message}) : current.status === 'loading' ? current : {...current, status: 'loading'});
    void refreshTextures(project.id, requests, decode, stale, store).then(result => {
      if (!result || stale()) return;
      const loaded = Object.keys(result.textures).length > 0;
      const message = result.failures.join('\n') || (loaded ? '' : NO_TEXTURES);
      setState(current => current.status === (loaded ? 'ready' : 'failed') && current.message === message && sameTextures(current.textures, result.textures) ? current : {status: loaded ? 'ready' : 'failed', textures: result.textures, message});
    });
    return () => { generation.current++; };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the signature stands for the requests
  }, [signature, enabled, decode, project.id, store, revision]);
  return state;
}
