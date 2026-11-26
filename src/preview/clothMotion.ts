/** The movement preview's motions on the cloth preview: how far the character
 * travels in the world during each clip of the movement packet, at the hero's
 * own speeds when the packet carries them (`MotionHeader.speeds`, from the
 * game's hero definitions), and which loops are cuts. The view stays centred on
 * the character; the simulation sees the travel. */
import { RUN_SPEED, type MotionClip, type MotionHeader, type MotionPacket, type Vec3 } from './motion';
import { mc } from '../i18n';
// i18n-area: preview

/** Inches per second: forward is the hero's most move speed plus sprint, back and
 * sideways the most move speed (the game's definitions name no other). */
export interface MotionSpeeds {forward: number; back: number; strafe: number}
/** Speeds when the game's hero definitions could not be read (the earlier constant). */
export const FALLBACK_SPEEDS: MotionSpeeds = {forward: RUN_SPEED, back: RUN_SPEED, strafe: RUN_SPEED};
/** Gravity of a jump and a fall (in/s²). */
export const JUMP_GRAVITY = 1200;
/** How long before the end of the take-off clip the feet leave the ground (seconds). */
export const TAKE_OFF_LEAD = .08;
/** The long fall's top speed (in/s): an estimate, nothing in the game's data gives one. */
export const FALL_TOP_SPEED = 1800;
/** Seconds over which the speed (and the lean with the pose) changes when a motion
 * starts, stops or changes. */
export const EASE_SECONDS = .3;

/** The toolbar's motions, in order, with their labels (English; shown with `tc('motion', label)`; the clip names are identifiers). */
export const MOTION_LABELS: [string, string][] = [['idle', mc('motion', 'Stand')], ['run', mc('motion', 'Run')], ['back', mc('motion', 'Back')], ['left', mc('motion', 'Left')], ['right', mc('motion', 'Right')], ['stop', mc('motion', 'Stop')], ['jump', mc('motion', 'Jump')], ['fall', mc('motion', 'Fall')]];

export function speedsOf(header: MotionHeader): {speeds: MotionSpeeds; fromGame: boolean} {
  const s = header.speeds;
  const ok = s && [s.forward, s.back, s.strafe].every(v => typeof v === 'number' && Number.isFinite(v) && v > 0);
  return ok ? {speeds: {forward: s.forward, back: s.back, strafe: s.strafe}, fromGame: true} : {speeds: FALLBACK_SPEEDS, fromGame: false};
}

/** Smoothstep 0..1. */
export const ease = (x: number) => { const s = Math.min(1, Math.max(0, x)); return s * s * (3 - 2 * s); };

/** Seconds into the jump clip where the feet leave the ground and where it lands: from
 * shortly before the end of the take-off part to the start of the landing part. */
export function clothJumpArc(clip: MotionClip): [number, number] | null {
  if (clip.name !== 'jump' || clip.segments.length < 2) return null;
  const start = clip.segments[0] / clip.fps, air = clip.segments[1] / clip.fps;
  return [Math.max(0, start - TAKE_OFF_LEAD), start + air];
}

/** The character's travel from the start of one pass of a clip to `u` seconds into it. */
export function travelInLoop(clip: MotionClip, u: number, speeds: MotionSpeeds): Vec3 {
  switch (clip.name) {
    case 'run': return [speeds.forward * u, 0, 0];
    case 'back': return [-speeds.back * u, 0, 0];
    case 'left': return [0, speeds.strafe * u, 0];
    case 'right': return [0, -speeds.strafe * u, 0];
    case 'stop': {
      // Running, then the speed eases out over EASE_SECONDS as the stand begins.
      const end = (clip.segments[0] ?? clip.frames) / clip.fps;
      if (u <= end) return [speeds.forward * u, 0, 0];
      const d = u - end, x = Math.min(1, d / EASE_SECONDS);
      const after = x < 1 ? d - EASE_SECONDS * (x ** 3 - x ** 4 / 2) : EASE_SECONDS / 2;
      return [speeds.forward * (end + after), 0, 0];
    }
    case 'fall': {
      const reach = FALL_TOP_SPEED / JUMP_GRAVITY;
      const z = u < reach ? .5 * JUMP_GRAVITY * u * u : .5 * JUMP_GRAVITY * reach * reach + FALL_TOP_SPEED * (u - reach);
      return [0, 0, -z];
    }
    case 'jump': {
      const arc = clothJumpArc(clip);
      if (!arc) return [0, 0, 0];
      const span = arc[1] - arc[0], v = u - arc[0];
      return [0, 0, v > 0 && v < span ? .5 * JUMP_GRAVITY * v * (span - v) : 0];
    }
    default: return [0, 0, 0];
  }
}

/** Whether a looping clip jumps at its wrap (last frame to first) by far more than any of its
 * frame steps: then the wrap is a cut that re-seats the simulation, never a motion fed to it. */
export function loopCuts(packet: MotionPacket, clip: MotionClip): boolean {
  const bones = packet.header.bones, stride = bones.length * 12, d = packet.data;
  if (clip.frames < 2) return false;
  const step = (f0: number, f1: number) => {
    let worst = 0;
    const a = clip.offset + f0 * stride, b = clip.offset + f1 * stride;
    bones.forEach((bone, i) => {
      const [x, y, z] = bone.position, o = i * 12;
      let s = 0;
      for (let r = 0; r < 3; r++) { const k = r * 4 + o, p = d[a + k] * x + d[a + k + 1] * y + d[a + k + 2] * z + d[a + k + 3], q = d[b + k] * x + d[b + k + 1] * y + d[b + k + 2] * z + d[b + k + 3]; s += (p - q) * (p - q); }
      worst = Math.max(worst, Math.sqrt(s));
    });
    return worst;
  };
  let frame = 0;
  for (let f = 1; f < clip.frames; f++) frame = Math.max(frame, step(f - 1, f));
  return step(clip.frames - 1, 0) > Math.max(3 * frame, .5);
}

/** One motion as the cloth playback plays it, in its own time `time`: the clip's pose, its
 * travel along the ground and down a fall from `t0` to `t1` (continuous over loops; a pass
 * that restarts as a cut only adds its own travel), a jump's height above the ground (shown
 * in the view and felt by the cloth), and whether a step crosses a cut. */
export interface ClothMotion {
  name: string;
  sample: (time: number, out: Float32Array) => Float32Array;
  travel: (t0: number, t1: number) => Vec3;
  height: (time: number) => number;
  cut: (t0: number, t1: number) => boolean;
  /** Whether the view shows the floor (a fall keeps the character in view without one). */
  floor: boolean;
}
export function clothMotion(packet: MotionPacket, clip: MotionClip, speeds: MotionSpeeds, sample: (clip: MotionClip, time: number, out: Float32Array) => Float32Array): ClothMotion {
  const length = clip.frames / clip.fps, loop = (time: number) => ((time % length) + length) % length;
  const restarts = clip.name === 'jump' || clip.name === 'fall' || clip.name === 'stop' || loopCuts(packet, clip);
  const pass = (time: number) => Math.floor(time / length);
  // A jump's rise and fall is its height, not travel.
  const ground = (u: number): Vec3 => { const v = travelInLoop(clip, u, speeds); return clip.name === 'jump' ? [v[0], v[1], 0] : v; };
  const within = (time: number) => ground(loop(time));
  return {
    name: clip.name,
    sample: (time, out) => sample(clip, loop(time), out),
    travel: (t0, t1) => {
      if (pass(t0) === pass(t1)) { const a = within(t0), b = within(t1); return [b[0] - a[0], b[1] - a[1], b[2] - a[2]]; }
      // Over the wrap: the rest of this pass, then the start of the next.
      const end = ground(length), a = within(t0), b = within(t1);
      if (restarts) return [b[0], b[1], b[2]];
      return [end[0] - a[0] + b[0], end[1] - a[1] + b[1], end[2] - a[2] + b[2]];
    },
    height: time => clip.name === 'jump' ? travelInLoop(clip, loop(time), speeds)[2] : 0,
    cut: (t0, t1) => restarts && pass(t0) !== pass(t1),
    floor: clip.name !== 'fall',
  };
}
