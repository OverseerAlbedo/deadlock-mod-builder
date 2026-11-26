// i18n-area: extras
import type { CatalogEntry } from "../components/AssetCatalog";
import type { ResourceReplacement } from "../models";
import { m } from "../i18n";
import { NOT_A_PICTURE_STATUS, isUnreadablePicture } from "./pictureChecks";
export type ExtraKind = "icons" | "audio" | "effects";
export interface ExtraPreviewRequest {
  hero_path: string;
  target: string;
  /** `items`: the game's item pictures (Extras → Items), not the hero's catalog. */
  kind: ExtraKind | "materials" | "items";
  source?: string;
  fit?: string;
  audio_fit?: string;
  max_dimension?: number;
}
export interface ExtraMedia {
  mime_type: string;
  details: Record<string, unknown>;
  bytes: ArrayBuffer;
}
export interface AssetItem {
  entry: CatalogEntry;
  index: number;
  row?: ResourceReplacement;
  missing: boolean;
  key: string;
}
export const extraKinds: ExtraKind[] = ["icons", "audio", "effects"];
export const specs = {
  icons: {
    title: m("Icons"),
    singular: "icon",
    icon: "image",
    sourceKind: "icon",
    formats: "PNG · JPG",
    extensions: ["png", "jpg", "jpeg"],
    description: m("Portraits, abilities and character artwork."),
  },
  audio: {
    title: m("Sounds"),
    singular: "sound",
    icon: "audio",
    sourceKind: "audio",
    formats: "WAV · MP3 · FLAC · OGG · VSND",
    extensions: ["wav", "mp3", "flac", "ogg", "vsnd_c"],
    description: m("Voices, abilities and other character sounds."),
  },
  effects: {
    title: m("Effects"),
    singular: "effect",
    icon: "sparkles",
    sourceKind: "effect",
    formats: "Compiled VPCF",
    extensions: ["vpcf_c"],
    description: m("Particle effects and their game dependencies."),
  },
} as const;
export function assetItems(
  entries: CatalogEntry[],
  rows: ResourceReplacement[],
): AssetItem[] {
  const indexes = new Map<string, number>();
  rows.forEach((row, index) => {
    const key = (row.target ?? "").toLowerCase();
    if (!indexes.has(key) || row.enabled !== false) indexes.set(key, index);
  });
  const represented = new Set(
    entries.map((entry) => indexes.get(entry.target.toLowerCase())),
  );
  return [
    ...entries.map((entry) => {
      const index = indexes.get(entry.target.toLowerCase()) ?? -1;
      return {
        entry,
        index,
        row: rows[index],
        missing: false,
        key: entry.target,
      };
    }),
    ...rows.flatMap((row, index) =>
      represented.has(index)
        ? []
        : [
            {
              entry: {
                path: row.target ?? "",
                target: row.target ?? "",
                label: row.target?.split("/").pop() || "Unassigned replacement",
                supported: false,
              },
              index,
              row,
              missing: true,
              key: `saved:${index}`,
            },
          ],
    ),
  ];
}
export function assetLabel(entry: CatalogEntry) {
  return (
    entry.label
      .replace(/ \(potentially shared\)$/, "")
      .replace(/\.(vtex|vsnd|vpcf)(_c)?$/, "")
      .replace(/[_-]+/g, " ")
      .trim() || "Untitled asset"
  );
}
export function duration(seconds: number) {
  return `${Math.floor(seconds / 60)}:${Math.floor(seconds % 60)
    .toString()
    .padStart(2, "0")}`;
}
export function detailNumber(entry: CatalogEntry, key: string): number | null {
  const meta = entry.metadata as Record<string, unknown> | undefined;
  const value = meta?.[key];
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}
/**
 * A sound row that silences its game file (Rust `ResourceReplacement::silenced`):
 * `silence` without a file. A file wins, as an older app may have added one.
 */
export function isSilenced(row: ResourceReplacement | undefined): boolean {
  return row?.silence === true && !(row.source ?? "").trim();
}
/** The status words `replacementStatus` and the Sounds rows give (identifiers, compared in
 * English); they are translated where they are drawn, with `t(status)`. */
export const STATUS_WORDS = [m("Original"), m("Paused"), m("Needs attention"), m("Silenced"), m("Replaced"), m("Changed"), m("View only"), m("Shared")] as const;
export function replacementStatus(item: AssetItem, active: boolean) {
  if (!item.row) return "Original";
  if (!active || item.row.enabled === false) return "Paused";
  // A silenced sound (`silence`) has no source file.
  const silenced = isSilenced(item.row);
  if (
    item.missing ||
    !item.entry.supported ||
    (!item.row.source && !silenced && !item.row.taken) ||
    (item.entry.shared && !item.row.allow_shared)
  )
    return "Needs attention";
  // A picture whose file its preview could not read (a text file named .png).
  if (isUnreadablePicture(item.row.source)) return NOT_A_PICTURE_STATUS;
  return silenced ? "Silenced" : "Replaced";
}
