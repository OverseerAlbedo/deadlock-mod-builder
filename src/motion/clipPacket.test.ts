import { describe, expect, it } from 'vitest';
import { decodeClipPacket, frameAt, readout, sampleClip, shownLength } from './clipPacket';
import { motionList, shortPath } from './motionCatalog';
import type { AnimationEntry, ComponentCatalog } from '../components/AssetCatalog';

function packet(frames: number, fps: number, values: number[], extra: Record<string, unknown> = {}) {
  const header = new TextEncoder().encode(JSON.stringify({bones: [{name: 'root', parent: -1, position: [0, 0, 0]}], clips: [{name: 'c', sources: ['c'], fps, frames, segments: [frames], offset: 0, duration: (frames - 1) / fps, clip_frames: frames, additive: false, blend_may_differ: false, cut: false, which: 'game', marks: [], ...extra}]}));
  const start = Math.ceil((4 + header.length) / 4) * 4, out = new ArrayBuffer(start + frames * 48);
  new DataView(out).setUint32(0, header.length, true); new Uint8Array(out, 4, header.length).set(header);
  const data = new Float32Array(out, start);
  values.forEach((v, f) => { data[f * 12 + 3] = v; });
  return out;
}

describe('clip packets', () => {
  it('keep the clip’s own frames: the time runs over its length, frames counted from 1 in the readout', () => {
    const decoded = decodeClipPacket(packet(25, 30, Array.from({length: 25}, (_, f) => f)));
    const clip = decoded.header.clips[0];
    expect(shownLength(clip)).toBeCloseTo(0.8);
    expect(readout(clip, 0.34)).toBe('0.34 / 0.80 s · frame 11 of 25');
    expect(readout(clip, 5)).toBe('0.80 / 0.80 s · frame 25 of 25');
    expect(frameAt(clip, 0.8)).toBe(24);
    const out = new Float32Array(12);
    expect(sampleClip(decoded, 0.5 / 30, out)[3]).toBeCloseTo(0.5);
    expect(sampleClip(decoded, 9, out)[3]).toBe(24);
  });
  it('state a cut clip by what the packet holds', () => {
    const clip = decodeClipPacket(packet(10, 30, [], {cut: true, clip_frames: 5000, duration: 200})).header.clips[0];
    expect(shownLength(clip)).toBeCloseTo(0.3);
    expect(readout(clip, 0)).toBe('0.00 / 0.30 s · frame 1 of 5000');
  });
  it('refuse a truncated packet', () => {
    expect(() => decodeClipPacket(packet(10, 30, []).slice(0, 200))).toThrow();
  });
});

describe('the Motion list', () => {
  const entry = (label: string, extra: Partial<AnimationEntry> = {}): AnimationEntry => ({path: label, target: `m/clips/${label}.vnmclip_c`, label, supported: true, skeleton: 's', frames: 2, duration: 1, tracks: 1, events: 0, additive: false, name: label, group: 'movement', graph_named: true, ...extra});
  const catalog = {hero_path: 'h', icons: [], effects: [], audio: [], variants: [], animation_unavailable: null, warnings: [], animations: [entry('a'), entry('a.vnmclip+non_additive', {derived_from: 'm/clips/a.vnmclip_c'}), entry('b', {graph_named: false})]} as ComponentCatalog;
  it('puts a derived copy on its parent, folds unnamed clips and keeps every unplaceable row', () => {
    const list = motionList(catalog, [{source: 'x', target: 'M/Clips/A.vnmclip_c'}, {source: 'y', target: 'm/clips/a.vnmclip_c'}, {source: '', target: 'm/clips/b.vnmclip_c'}]);
    expect(list.items.map(i => [i.entry.label, i.index, i.derived?.label])).toEqual([['a', 0, 'a.vnmclip+non_additive']]);
    expect(list.folded.map(i => i.entry.label)).toEqual(['b']);
    expect(list.others).toEqual([1, 2]);
    expect(motionList(null, [{}, {}]).others).toEqual([0, 1]);
  });
  it('shortens long clip paths in the middle', () => {
    expect(shortPath('models/heroes_staging/haze/clips/run_n.vnmclip_c')).toBe('models/heroes_staging/haze/…/run_n.vnmclip_c');
  });
});

describe('the replacement file name', () => {
  it('is cut in the middle, keeping the extension and the last letters', async () => {
    const {fileNameParts} = await import('./ClipCard');
    expect(fileNameParts('C:\\clips\\my_run_forward.vnmclip_c')).toEqual(['my_run_for', 'ward.vnmclip_c']);
    expect(fileNameParts('/clips/abc.vnmclip')).toEqual(['', 'abc.vnmclip']);
  });
});
