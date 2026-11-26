// Types of the effect player's commands and events (Rust `src-tauri/src/effect_player.rs`).
// The player plays the selected effect in Extras → Effects; "yours" is made from exactly the
// files a build writes. Frames come as raw bytes from `api.effectPlayerFrame`.
// i18n-area: effects
import { m } from "../i18n";

/** Why the player cannot run on this computer. */
export type PlayerUnavailable = "missing" | "opengl";
export type PlayerState =
  | { state: "idle" }
  | { state: "ready"; driver: string }
  | { state: "unavailable"; reason: PlayerUnavailable; message: string }
  | { state: "failed"; message: string };

/** The view choices (kept for the session, never in the project). */
export interface PlayerView {
  compare: boolean;
  loop: boolean;
  speed: number;
  paused: boolean;
  /** Six hex digits; only `PLAYER_BACKGROUNDS` are offered. */
  background: string;
  /** Stops, 0 to +4. */
  brightness: number;
  width: number;
  height: number;
  fps: number;
}

/** The parts of the helper's `loaded` report the page reads. */
export interface PlayerLoaded {
  effect?: string;
  compare?: boolean;
  bounds?: { empty?: boolean; size?: string };
  files?: { fromOverlay?: string[]; fromGame?: number; missing?: string[] };
  rules?: string[];
  notes?: {
    untintedModels?: { renderers: number; parts: string[]; note: string };
    decals?: { renderers: number; parts: string[]; surface: "floor" | "none"; note: string };
    screenSpace?: boolean;
    cannotShow?: string[];
    undrivenControlPoints?: { cp: number; readBy?: string[]; filled?: string[] }[];
  };
  seeded?: boolean;
  compareNote?: string;
  gameVersion?: boolean;
}

export interface PlayerAnswer {
  state: PlayerState;
  /** The first frame whose generation is at least this one shows the result. */
  generation?: number;
  loaded?: PlayerLoaded;
  /** The overlay's files (game paths); empty when the edit writes nothing. */
  overlay: string[];
  /** A newer edit came first; this one was not made. */
  superseded: boolean;
  /** The helper refused the command: one plain sentence (the previous picture stays). */
  error?: string;
  overlay_ms: number;
  reply_ms: number;
  /** A sentence about the view, for example a frame rate this helper cannot play (the saved choice stays). */
  notice?: string;
}

export type PlayerControl =
  | { command: "compare"; on: boolean }
  | { command: "pause" }
  | { command: "resume" }
  | { command: "restart" }
  | { command: "loop"; on: boolean }
  | { command: "speed"; value: number }
  | { command: "size"; width: number; height: number }
  | { command: "background"; color: string }
  | { command: "brightness"; stops: number }
  | { command: "fit" }
  | { command: "reset_view" }
  | { command: "orbit"; dx: number; dy: number }
  | { command: "zoom"; steps: number }
  | { command: "fps"; value: number };

/** `effect-player` events. */
export type PlayerEvent =
  | { type: "starting" }
  | { type: "ready"; driver: string; start_ms: number }
  | { type: "loaded"; target: string; generation: number; loaded: PlayerLoaded }
  | { type: "finished"; generation: number }
  | { type: "restarted"; message: string }
  | { type: "unavailable"; reason: PlayerUnavailable; message: string }
  | { type: "failed"; message: string }
  | { type: "stopped" };

/** A frame as the page receives it: a 16-byte bridge header, the helper's 32-byte frame header, a JPEG. */
export const FRAME_PREFIX = 16;
export const FRAME_HEADER = 32;
export interface PlayerFrame {
  /** The bridge's counter (ask for frames after it). */
  counter: number;
  /** When the bridge received it (Unix ms). */
  receivedAt: number;
  sequence: number;
  generation: number;
  time: number;
  width: number;
  height: number;
  coverage: number;
  compare: boolean;
  paused: boolean;
  finished: boolean;
  jpeg: Uint8Array;
}
export function parseFrame(buffer: ArrayBuffer): PlayerFrame | null {
  if (!buffer || buffer.byteLength < FRAME_PREFIX + FRAME_HEADER) return null;
  const v = new DataView(buffer);
  if (v.getUint8(0) !== 0x44 || v.getUint8(1) !== 0x4d || v.getUint8(2) !== 0x42 || v.getUint8(3) !== 0x46) return null;
  const h = FRAME_PREFIX;
  const flags = v.getUint8(h + 24);
  return {
    counter: v.getUint32(4, true),
    receivedAt: v.getFloat64(8, true),
    sequence: v.getUint32(h, true),
    generation: v.getUint32(h + 4, true),
    time: v.getFloat32(h + 8, true),
    width: v.getUint16(h + 16, true),
    height: v.getUint16(h + 18, true),
    coverage: v.getFloat32(h + 20, true),
    compare: (flags & 1) !== 0,
    paused: (flags & 2) !== 0,
    finished: (flags & 4) !== 0,
    jpeg: new Uint8Array(buffer, h + FRAME_HEADER),
  };
}

export const PLAYER_BACKGROUNDS = [
  { color: "0b0c10", name: m("Black") },
  { color: "2a2e3a", name: m("Slate") },
] as const;
export const PLAYER_SPEEDS = [
  { value: 1, label: "1×" },
  { value: 0.5, label: "½×" },
  { value: 0.25, label: "¼×" },
] as const;
export const DEFAULT_VIEW: PlayerView = { compare: false, loop: true, speed: 1, paused: false, background: "0b0c10", brightness: 0, width: 640, height: 480, fps: 30 };
