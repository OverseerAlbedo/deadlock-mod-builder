// Extras → Sounds editor: the saved sound edit (four optional fields on an
// audio replacement row) and the preview Rust renders for it. No audio maths
// here: the result, its waveform, loudness and the matched volume all come
// from `api.soundEditPreview`, the build's own code.
import { useEffect, useRef, useState } from "react";
import { api, errorText } from "../api";
import type { ResourceReplacement } from "../models";
import { parseExtraMedia } from "./media";
import type { AssetItem } from "./models";
import { replaceAll, type ReplaceAllResult } from "./soundTags";

export const EDIT_KEYS = ["start_seconds", "volume_db", "fade_in_seconds", "fade_out_seconds"] as const;
export type EditKey = (typeof EDIT_KEYS)[number];
export type SoundEditValues = Partial<Record<EditKey, number>>;
/** The saved format's limits (Rust `customization::SOUND_*`). */
export const VOLUME_DB_RANGE: [number, number] = [-24, 12];
export const START_MAX = 3600;
export const FADE_MAX = 60;

export interface SoundEditRequest {
  hero_path: string;
  row: ResourceReplacement;
  buckets?: number;
}
export interface WindowReport {
  policy: string;
  start_frame: number;
  file_frames: number;
  requested_gain_db: number | null;
  applied_gain_db: number | null;
  gain_limited: boolean;
  fade_in_frames: number;
  fade_out_frames: number;
  fade_in_shortened: boolean;
  fade_out_shortened: boolean;
  looping: boolean;
}
export interface SoundEditPreview {
  edited: boolean;
  source: { duration: number; rate: number; channels: number; peaks: number[] };
  game: {
    duration: number;
    rate: number;
    channels: number;
    frames: number;
    loop_start: number;
    loop_end: number;
    loudness: number | null;
    peaks: number[];
  };
  result: {
    peaks: number[];
    loudness: number | null;
    report: { padded_frames: number; clipped_samples: number; window?: WindowReport | null };
  } | null;
  /** Why the build would refuse the row as it is (nothing to play then). */
  refusal: string | null;
  matched: { volume_db: number; limited_by_clipping: boolean; limited_by_range: boolean } | null;
  percent: number;
  percent_db: number | null;
  /** The result as a WAV ("Yours"); empty with a refusal. */
  wave: ArrayBuffer;
}

export function parseSoundPreview(data: ArrayBuffer): SoundEditPreview {
  const media = parseExtraMedia(data);
  return { ...(media.details as unknown as Omit<SoundEditPreview, "wave">), wave: media.bytes };
}

/** Whether a row carries a sound edit (Rust `ResourceReplacement::sound_edit`). */
export function hasEdit(row: ResourceReplacement | undefined) {
  return Boolean(row) && EDIT_KEYS.some((key) => typeof row![key] === "number");
}
/** The row's edit values, missing ones at 0. */
export function editValues(row: ResourceReplacement | undefined): Record<EditKey, number> {
  return Object.fromEntries(EDIT_KEYS.map((key) => [key, typeof row?.[key] === "number" ? (row![key] as number) : 0])) as Record<EditKey, number>;
}
/** Rounded to what the page shows and inside the saved format's limits. */
export function cleanValue(key: EditKey, value: number) {
  if (!Number.isFinite(value)) return 0;
  if (key === "volume_db") return Math.round(Math.min(VOLUME_DB_RANGE[1], Math.max(VOLUME_DB_RANGE[0], value)) * 10) / 10;
  const high = key === "start_seconds" ? START_MAX : FADE_MAX;
  return Math.round(Math.min(high, Math.max(0, value)) * 1000) / 1000;
}
/** The row with edit values changed (other keys kept as they are). */
export function withEdit(row: ResourceReplacement, changes: SoundEditValues): ResourceReplacement {
  const next = { ...row };
  for (const key of EDIT_KEYS) {
    const value = changes[key];
    if (typeof value === "number") next[key] = cleanValue(key, value);
  }
  return next;
}
/** "Reset edits": the four fields removed and an older Volume percent back to 100. */
export function withoutEdit(row: ResourceReplacement): ResourceReplacement {
  const next = { ...row };
  for (const key of EDIT_KEYS) delete next[key];
  if (next.volume_percent !== undefined && next.volume_percent !== 100) next.volume_percent = 100;
  return next;
}
/** The four fields removed (a silenced row has no file to edit). */
export function stripEdit(row: ResourceReplacement): ResourceReplacement {
  const next = { ...row };
  for (const key of EDIT_KEYS) delete next[key];
  return next;
}

/**
 * "Replace all N variations with this part": the same file and the same edit
 * on every variation, with every rule of "Replace all" (view only, compiled
 * files, the game format, the shared choice, the 128-row limit). An edit
 * longer than a shorter variation is held to it by the build, never refused.
 */
export function replaceAllWithPart(
  saved: ResourceReplacement[],
  items: AssetItem[],
  source: string,
  edit: SoundEditValues,
  includeShared: boolean,
  volumePercent?: number,
): ReplaceAllResult {
  const result = replaceAll(saved, items, source, includeShared);
  const targets = new Set(result.set.map((item) => item.entry.target.toLowerCase()));
  const values = Object.fromEntries(EDIT_KEYS.filter((key) => typeof edit[key] === "number").map((key) => [key, edit[key]]));
  const rows = result.rows.map((row) =>
    targets.has((row.target ?? "").toLowerCase()) && row.enabled !== false && row.source === source
      ? { ...withEdit(stripEdit(row), values), ...(volumePercent === undefined ? {} : { volume_percent: volumePercent }) }
      : row,
  );
  return { ...result, rows };
}

/** The message of a superseded request (Rust `sound_edit::SUPERSEDED`). */
export const SUPERSEDED = "superseded";
function superseded(error: unknown) {
  return Boolean(error && typeof error === "object" && "message" in error && (error as { message: unknown }).message === SUPERSEDED);
}

/**
 * The preview of `request`, newest first: one request at a time, and while
 * one runs only the newest waiting request is kept. A result is shown until a
 * newer one arrives; errors of superseded requests are ignored.
 */
export function useSoundPreview(request: SoundEditRequest | null) {
  const [preview, setPreview] = useState<SoundEditPreview | null>(null),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(false);
  const state = useRef({ busy: false, pending: null as SoundEditRequest | null, alive: true, sent: 0 });
  const key = request ? JSON.stringify(request) : "";
  useEffect(() => {
    state.current.alive = true;
    return () => {
      state.current.alive = false;
    };
  }, []);
  useEffect(() => {
    if (!request) return;
    const s = state.current;
    s.pending = request;
    const pump = () => {
      if (s.busy || !s.pending || !s.alive) return;
      const next = s.pending;
      s.pending = null;
      s.busy = true;
      const id = ++s.sent;
      setLoading(true);
      void Promise.resolve()
        .then(() => api.soundEditPreview(next))
        .then(parseSoundPreview)
        .then(
          (answer) => {
            if (!s.alive || id !== s.sent) return;
            setPreview(answer);
            setError("");
          },
          (e) => {
            if (!s.alive || id !== s.sent || superseded(e)) return;
            setError(errorText(e));
          },
        )
        .finally(() => {
          s.busy = false;
          if (s.alive && !s.pending) setLoading(false);
          pump();
        });
    };
    pump();
    // `key` stands for the request's content.
  }, [key]);
  return { preview, error, loading };
}
