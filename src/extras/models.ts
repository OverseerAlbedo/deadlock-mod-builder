import type { CatalogEntry } from "../components/AssetCatalog";
import type { ResourceReplacement } from "../models";
export type ExtraKind = "icons" | "audio" | "effects";
export interface ExtraPreviewRequest {
  hero_path: string;
  target: string;
  kind: ExtraKind | "materials";
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
    title: "Icons",
    singular: "icon",
    icon: "image",
    sourceKind: "icon",
    formats: "PNG · JPG",
    extensions: ["png", "jpg", "jpeg"],
    description: "Portraits, abilities and character artwork.",
  },
  audio: {
    title: "Sounds",
    singular: "sound",
    icon: "audio",
    sourceKind: "audio",
    formats: "WAV · MP3 · FLAC · OGG · VSND",
    extensions: ["wav", "mp3", "flac", "ogg", "vsnd_c"],
    description: "Voices, abilities and other character sounds.",
  },
  effects: {
    title: "Effects",
    singular: "effect",
    icon: "sparkles",
    sourceKind: "effect",
    formats: "Compiled VPCF",
    extensions: ["vpcf_c"],
    description: "Particle effects and their game dependencies.",
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
export function replacementStatus(item: AssetItem, active: boolean) {
  if (!item.row) return "Original";
  if (!active || item.row.enabled === false) return "Paused";
  if (
    item.missing ||
    !item.entry.supported ||
    !item.row.source ||
    (item.entry.shared && !item.row.allow_shared)
  )
    return "Needs attention";
  return "Replaced";
}
