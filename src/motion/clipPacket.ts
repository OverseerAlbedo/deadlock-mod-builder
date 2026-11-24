/** The Motion page's clip packet: the movement preview's binary form (a little-endian u32 header
 * length, the JSON header, padding to 4 bytes, then a row-major 3x4 rest-to-pose matrix per frame and
 * bone as f32), holding one clip at its own frame rate. Decoded and sampled here (not through
 * `preview/motion.ts`), so the page does not depend on the movement preview's player. */
import type { ClipPacket, PlayedClip } from './motionTypes';

export function decodeClipPacket(buffer: ArrayBuffer): ClipPacket {
  if (!(buffer instanceof ArrayBuffer) || buffer.byteLength < 4) throw new Error('Invalid clip packet.');
  const length = new DataView(buffer).getUint32(0, true);
  if (length > 16 * 1024 * 1024 || 4 + length > buffer.byteLength) throw new Error('Truncated clip packet header.');
  const header = JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(new Uint8Array(buffer, 4, length))) as ClipPacket['header'];
  if (!Array.isArray(header.bones) || !Array.isArray(header.clips) || header.clips.length !== 1) throw new Error('Invalid clip packet header.');
  const start = Math.ceil((4 + length) / 4) * 4;
  const data = new Float32Array(buffer, start, (buffer.byteLength - start) / 4);
  const clip = header.clips[0];
  if (!(clip.frames >= 1) || !(clip.fps > 0) || clip.offset + clip.frames * header.bones.length * 12 > data.length) throw new Error('Truncated clip packet frames.');
  return {header, data};
}

/** Seconds the packet's frames span (frame 0 at 0 s, the last at this time). */
export function packetLength(clip: PlayedClip): number {
  return clip.frames > 1 ? (clip.frames - 1) / clip.fps : 0;
}

/** The time shown for a clip: its own length, or what the packet holds when it was cut. */
export function shownLength(clip: PlayedClip): number {
  return clip.cut ? packetLength(clip) : clip.duration;
}

/** The 0-based stored frame nearest `time` (seconds from the start). */
export function frameAt(clip: PlayedClip, time: number): number {
  return Math.min(clip.frames - 1, Math.max(0, Math.round(time * clip.fps)));
}

/** The rest-to-pose skins at `time` (held at the ends), blended between the two nearest stored frames. */
export function sampleClip(packet: ClipPacket, time: number, out: Float32Array): Float32Array {
  const clip = packet.header.clips[0], bones = packet.header.bones.length, stride = bones * 12;
  const f = Math.min(clip.frames - 1, Math.max(0, time * clip.fps));
  const i0 = Math.floor(f), a = f - i0, i1 = i0 + 1 < clip.frames ? i0 + 1 : i0;
  const s0 = clip.offset + i0 * stride, s1 = clip.offset + i1 * stride, d = packet.data;
  for (let k = 0; k < stride; k++) out[k] = d[s0 + k] * (1 - a) + d[s1 + k] * a;
  return out;
}

/** "0.34 / 0.80 s · frame 10 of 24", from the clip's own frame count and length. */
export function readout(clip: PlayedClip, time: number): string {
  const length = shownLength(clip);
  const at = Math.min(Math.max(time, 0), length);
  return `${at.toFixed(2)} / ${length.toFixed(2)} s · frame ${frameAt(clip, at) + 1} of ${clip.clip_frames}`;
}
