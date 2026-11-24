import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Texture } from 'three';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { AppearanceMaterial, Project } from '../models';

const mocks = vi.hoisted(() => ({previewImage: vi.fn(), fileStamps: vi.fn()}));
vi.mock('../api', () => ({api: mocks, basename: (s: string) => s}));
vi.mock('@tauri-apps/plugin-dialog', () => ({open: vi.fn()}));
import { applyTextureEdit, colorEdits, hasColorImages, NO_TEXTURES, refreshTextures, requestKey, TextureStore, textureRequests, usePreviewTextures, type DecodeTexture, type TextureEdit } from './textures';
import { adjustPixels } from './colorAdjust';
import { recolorPixels, withDefaults } from '../extras/heroColorRanges';

const base = structuredClone(reference.cases[0].expected) as unknown as Project;
const project = (extra: Partial<Project['customization']> = {}): Project => ({...base,
  materials: {
    body: {textures: {color: 'body.png'}},
    hair: {textures: {color: 'hair.png', normal: 'hair_n.png'}},
    plain: {textures: {}},
    blush: {textures: {color: 'blush.png'}, face_layer: 'blush'},
  },
  customization: {enabled: true, ...extra},
} as unknown as Project);
const edited = (rows: AppearanceMaterial[], enabled = true) => project({appearance: {enabled, materials: rows}});
const decode = (): DecodeTexture => vi.fn(async (_bytes: ArrayBuffer, edit: TextureEdit | null) => Object.assign(new Texture(), {name: edit ? JSON.stringify(edit) : 'plain'}));

describe('Preview textures', () => {
  it('asks for each material’s base-colour image once, skipping materials without one and hidden face layers', () => {
    const requests = textureRequests(project(), ['body', 'hair', 'hair', 'plain', 'blush', 'unknown']);
    expect(requests).toEqual([{material: 'body', path: 'body.png', edit: null}, {material: 'hair', path: 'hair.png', edit: null}]);
    expect(hasColorImages(project(), ['plain', 'blush'])).toBe(false);
    expect(hasColorImages(project(), ['plain', 'hair'])).toBe(true);
  });
  it('draws a face layer only when it is shown', () => {
    const shown = project({face_layers: {blush: true}});
    expect(textureRequests(shown, ['blush'])).toEqual([{material: 'blush', path: 'blush.png', edit: null}]);
    expect(textureRequests(project({face_layers: {tears: true}}), ['blush'])).toEqual([]);
  });
  it('applies the colour edits of the Surface changes rows: hue, saturation, brightness and colour ranges', () => {
    const rows = [{enabled: true, target: 'body', hue: 30, saturation: 1.5}, {enabled: true, target: 'hair', hue: 0, saturation: 1, brightness: 1}, {enabled: false, target: 'plain', hue: 90}];
    const body: TextureEdit = {ranges: [], hue: 30, saturation: 1.5, brightness: 1};
    expect([...colorEdits(edited(rows))]).toEqual([['body', body]]);
    expect(textureRequests(edited(rows), ['body', 'hair'])).toEqual([{material: 'body', path: 'body.png', edit: body}, {material: 'hair', path: 'hair.png', edit: null}]);
    // Surface changes switched off: the build keeps the original colours, so does the preview.
    expect(colorEdits(edited(rows, false)).size).toBe(0);
    // Colour ranges alone are an edit; paused ranges are not.
    const ranged = edited([{enabled: true, target: 'hair', ranges: [{hue: 20, hue_width: 15, hue_shift: 90}, {enabled: false, hue_shift: 40}]}]);
    expect(colorEdits(ranged).get('hair')).toEqual({ranges: [withDefaults({hue: 20, hue_width: 15, hue_shift: 90})], hue: 0, saturation: 1, brightness: 1});
    expect(colorEdits(edited([{enabled: true, target: 'hair', ranges: [{enabled: false, hue_shift: 40}]}])).size).toBe(0);
  });
  it('recolours exactly as the build: ranges first, then hue, saturation and brightness, alpha untouched', () => {
    const edit: TextureEdit = {ranges: [withDefaults({hue: 0, hue_width: 30, hue_shift: 120})], hue: 30, saturation: 0.5, brightness: 0.8};
    const pixels = () => new Uint8ClampedArray([200, 40, 30, 255, 30, 60, 220, 7, 128, 128, 128, 90]);
    const expected = pixels();
    recolorPixels(edit.ranges, expected);
    adjustPixels(expected, edit);
    const data = pixels();
    applyTextureEdit(data, edit);
    expect([...data]).toEqual([...expected]);
    expect([data[3], data[7], data[11]]).toEqual([255, 7, 90]);
    // The red pixel was turned by the range (to green) before the hue shift.
    expect(data[1]).toBeGreaterThan(data[0]);
  });
  it('keys the store by image and every value of the edit, ranges included', () => {
    const base = {material: 'hair', path: 'hair.png'};
    const plain = requestKey({...base, edit: null});
    const shifted = requestKey({...base, edit: {ranges: [], hue: 10, saturation: 1, brightness: 1}});
    const ranged = requestKey({...base, edit: {ranges: [withDefaults({hue_shift: 10})], hue: 10, saturation: 1, brightness: 1}});
    const moved = requestKey({...base, edit: {ranges: [withDefaults({hue_shift: 11})], hue: 10, saturation: 1, brightness: 1}});
    expect(new Set([plain, shifted, ranged, moved]).size).toBe(4);
  });
});

/** Stamps per path (size:time); a missing entry means the backend could not stat the file. */
let stamps: Record<string, {len: number; modified_ms: number} | null> = {};
const stampOf = (path: string) => stamps[path] === undefined ? {len: 10, modified_ms: 1} : stamps[path];
beforeEach(() => {
  stamps = {};
  mocks.previewImage.mockReset();
  mocks.previewImage.mockResolvedValue(new ArrayBuffer(4));
  mocks.fileStamps.mockReset();
  mocks.fileStamps.mockImplementation(async (paths: string[]) => paths.map(stampOf));
});
const requested = () => mocks.previewImage.mock.calls.map(call => call[0]);

describe('The texture store', () => {
  it('loads at preview size, then reuses every unchanged image with no further reads', async () => {
    const store = new TextureStore(), fake = decode();
    const requests = textureRequests(edited([{enabled: true, target: 'body', brightness: .5}]), ['body', 'hair']);
    const first = await refreshTextures('p1', requests, fake, undefined, store);
    expect(requested()).toEqual(['body.png', 'hair.png']);
    expect(mocks.previewImage).toHaveBeenCalledWith('body.png', 1024);
    expect(fake).toHaveBeenCalledWith(expect.any(ArrayBuffer), {ranges: [], hue: 0, saturation: 1, brightness: 0.5});
    expect(mocks.fileStamps).toHaveBeenCalledWith(['body.png', 'hair.png']);
    mocks.previewImage.mockClear();
    const again = await refreshTextures('p1', requests, fake, undefined, store);
    expect(mocks.previewImage).not.toHaveBeenCalled();
    expect(again!.textures.body).toBe(first!.textures.body);
    expect(again!.textures.hair).toBe(first!.textures.hair);
    expect(mocks.fileStamps).toHaveBeenCalledTimes(2);
  });
  it('reloads only the image whose file, path or colour edit changed', async () => {
    const store = new TextureStore(), fake = decode();
    const shown = project();
    const first = await refreshTextures('p1', textureRequests(shown, ['body', 'hair']), fake, undefined, store);
    mocks.previewImage.mockClear();
    // The file was repainted: its stamp changed.
    stamps['hair.png'] = {len: 11, modified_ms: 2};
    const repainted = await refreshTextures('p1', textureRequests(shown, ['body', 'hair']), fake, undefined, store);
    expect(requested()).toEqual(['hair.png']);
    expect(repainted!.textures.body).toBe(first!.textures.body);
    expect(repainted!.textures.hair).not.toBe(first!.textures.hair);
    // Another image assigned to a material.
    mocks.previewImage.mockClear();
    const moved = {...shown, materials: {...shown.materials, body: {textures: {color: 'body_v2.png'}}}} as Project;
    await refreshTextures('p1', textureRequests(moved, ['body', 'hair']), fake, undefined, store);
    expect(requested()).toEqual(['body_v2.png']);
    // A colour edit on one material.
    mocks.previewImage.mockClear();
    const tinted = {...moved, customization: {enabled: true, appearance: {enabled: true, materials: [{enabled: true, target: 'hair', hue: 40}]}}} as Project;
    await refreshTextures('p1', textureRequests(tinted, ['body', 'hair']), fake, undefined, store);
    expect(requested()).toEqual(['hair.png']);
    expect(fake).toHaveBeenLastCalledWith(expect.any(ArrayBuffer), {ranges: [], hue: 40, saturation: 1, brightness: 1});
    // Only what the latest requests use is kept.
    expect(store.size).toBe(2);
  });
  it('tries a file it cannot stat again and shows its error', async () => {
    const store = new TextureStore(), fake = decode();
    await refreshTextures('p1', textureRequests(project(), ['body', 'hair']), fake, undefined, store);
    mocks.previewImage.mockClear();
    stamps['hair.png'] = null;
    mocks.previewImage.mockImplementation(async (path: string) => { if (path === 'hair.png') throw {message: 'The file is missing'}; return new ArrayBuffer(4); });
    const result = await refreshTextures('p1', textureRequests(project(), ['body', 'hair']), fake, undefined, store);
    expect(requested()).toEqual(['hair.png']);
    expect(result!.failures).toEqual(['hair: {"message":"The file is missing"}']);
    expect(Object.keys(result!.textures)).toEqual(['body']);
  });
  it('keeps the shown textures when the stamp check fails', async () => {
    const store = new TextureStore(), fake = decode();
    const first = await refreshTextures('p1', textureRequests(project(), ['body', 'hair']), fake, undefined, store);
    mocks.previewImage.mockClear();
    mocks.fileStamps.mockRejectedValue(new Error('backend unavailable'));
    const result = await refreshTextures('p1', textureRequests(project(), ['body', 'hair']), fake, undefined, store);
    expect(mocks.previewImage).not.toHaveBeenCalled();
    expect(result!.textures).toEqual(first!.textures);
    expect(result!.failures).toEqual([]);
  });
  it('clears and frees everything when another project is opened', async () => {
    const store = new TextureStore(), fake = decode();
    const first = await refreshTextures('p1', textureRequests(project(), ['body']), fake, undefined, store);
    const freed = vi.spyOn(first!.textures.body, 'dispose');
    mocks.previewImage.mockClear();
    await refreshTextures('p2', textureRequests(project(), ['body']), fake, undefined, store);
    expect(freed).toHaveBeenCalled();
    expect(requested()).toEqual(['body.png']);
    expect(store.cached('p1', textureRequests(project(), ['body']))).toEqual({});
  });
  it('shares one load between two views asking at once', async () => {
    const store = new TextureStore(), fake = decode();
    const requests = textureRequests(project(), ['body']);
    const [a, b] = await Promise.all([refreshTextures('p1', requests, fake, undefined, store), refreshTextures('p1', requests, fake, undefined, store)]);
    expect(requested()).toEqual(['body.png']);
    expect(a!.textures.body).toBe(b!.textures.body);
  });
});

describe('Preview textures in a view', () => {
  it('loads once, and a view opened again shows the stored textures on its first render with no reads', async () => {
    const store = new TextureStore(), fake = decode();
    const shown = project(), materials = ['body', 'hair'];
    const {result, rerender, unmount} = renderHook(({enabled}) => usePreviewTextures(shown, materials, enabled, fake, store), {initialProps: {enabled: false}});
    expect(result.current.status).toBe('idle');
    expect(mocks.previewImage).not.toHaveBeenCalled();
    rerender({enabled: true});
    await waitFor(() => expect(result.current.status).toBe('ready'));
    const textures = result.current.textures;
    expect(Object.keys(textures)).toEqual(['body', 'hair']);
    unmount();
    const again = renderHook(() => usePreviewTextures(shown, materials, true, fake, store));
    expect(again.result.current.status).toBe('ready');
    expect(again.result.current.textures).toEqual(textures);
    await waitFor(() => expect(mocks.fileStamps).toHaveBeenCalledTimes(2));
    expect(mocks.previewImage).toHaveBeenCalledTimes(2);
    expect(again.result.current.textures.body).toBe(textures.body);
  });
  it('falls back with the message when nothing loads', async () => {
    const store = new TextureStore(), fake = decode();
    mocks.previewImage.mockRejectedValue(new Error('missing'));
    const {result: none} = renderHook(() => usePreviewTextures(project(), ['body'], true, fake, store));
    await waitFor(() => expect(none.current.status).toBe('failed'));
    expect(none.current.message).toBe('body: missing');
    const {result: empty} = renderHook(() => usePreviewTextures(project(), ['plain'], true, fake, store));
    await waitFor(() => expect(empty.current.status).toBe('failed'));
    expect(empty.current.message).toBe(NO_TEXTURES);
  });
});
