/** Extras → Objects: what the page shows about the catalog (groups, what kind an object is),
 * the fit answer's decoding, and the one-at-a-time fit queue. No fit maths here: the fit is
 * applied in Rust and the answer is already in the game object's frame. */
// i18n-area: objects
import { t, tp } from "../i18n";
import {
  OBJECT_FIT_MIME,
  OBJECTS_NO_BLENDER,
  OBJECTS_NO_GAME,
  OBJECT_NEEDS_CURRENT_SDK,
  type AbilityObjectRow,
  type ObjectCatalog,
  type ObjectFitView,
  type ObjectRow,
} from "./objectTypes";

const fold = (value: string) => value.toLowerCase();

/** One listed object: a catalog row, a saved row, or both. A saved row the catalog does not
 * list (the game changed, or the catalog is not loaded yet) keeps a stand-in entry. */
export interface ObjectItem {
  key: string;
  target: string;
  entry: ObjectRow | null;
  saved: AbilityObjectRow | null;
}
export interface ObjectGroup { key: string; title: string; items: ObjectItem[] }

/** The ability's heading, in the Effects page's group order, in the language shown (the
 * ability's name as it comes). */
export function groupTitle(row: Pick<ObjectRow, "group" | "ability_name" | "ability">, hero: string): string {
  const name = row.ability_name || readable(row.ability);
  if (row.group >= 101 && row.group <= 104) return `${row.group - 100} · ${name}`;
  if (row.group === 200) return t("Gun");
  if (row.group === 201) return t("Gun · alternate fire");
  if (row.group === 210) return t("Melee");
  if (row.group === 300) return t("Innate · {name}", { name });
  if (row.group === 350) return hero || name;
  return name;
}
const readable = (id: string) => id.replace(/^(citadel_)?(ability_)?/, "").replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());

/** What kind of object it is, in one line: the engine's own words for an object beyond the first
 * kind (`kind.label`, as it comes); otherwise from its structure, in the order the backend checks
 * what cannot be replaced yet (in the language shown). */
export function kindText(row: ObjectRow): string {
  if (row.offered && row.kind) return row.kind.label;
  const s = row.structure;
  if (row.offered) return t("Simple object");
  if (!s) return t("Object");
  if (s.bones > 0) return s.animated ? t("Animated object with a skeleton") : t("Object with a skeleton");
  if (s.physics) return t("Object with collision");
  if (s.hitboxes > 0) return t("Object with hitboxes");
  if (s.animated) return t("Animated object");
  if (s.attachments > 0) return t("Object with attachment points");
  if (s.materials.length !== 1) return s.materials.length ? tp(s.materials.length, { other: "Object with {count} materials" }) : t("Object without a material");
  if (row.drawn !== "particle") return t("Object the ability places");
  return t("Object");
}
/** The readable name from a file stem ("haze_dagger_model" → "Haze dagger model"). */
export const nameFromTarget = (target: string) => {
  const stem = target.split("/").pop()!.replace(/\.vmdl(_c)?$/i, "");
  return { file: stem, name: stem.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase()) };
};

/** The list: catalog rows under their ability (catalog order kept), then saved rows the catalog
 * does not list under "Saved, not listed now". `replacedOnly` keeps saved rows only. */
export function objectGroups(catalog: ObjectCatalog | null, saved: AbilityObjectRow[], hero: string, replacedOnly: boolean): ObjectGroup[] {
  const savedBy = new Map(saved.map((r) => [fold(r.target), r]));
  const groups: ObjectGroup[] = [];
  const seen = new Set<string>();
  for (const entry of [...(catalog?.rows ?? [])].sort((a, b) => a.group - b.group)) {
    const key = fold(entry.target);
    if (seen.has(key)) continue;
    seen.add(key);
    const row = savedBy.get(key) ?? null;
    if (replacedOnly && !row) continue;
    const title = groupTitle(entry, hero);
    const groupKey = `${entry.group}:${entry.ability}`;
    let group = groups.find((g) => g.key === groupKey);
    if (!group) groups.push((group = { key: groupKey, title, items: [] }));
    group.items.push({ key, target: entry.target, entry, saved: row });
  }
  const extra = saved.filter((r) => !seen.has(fold(r.target)));
  if (extra.length)
    groups.push({
      key: "saved",
      title: catalog ? t("Saved, not listed now") : t("Saved replacements"),
      items: extra.map((r) => ({ key: fold(r.target), target: r.target, entry: null, saved: r })),
    });
  return groups;
}

/** Is the message (with or without the "Integrity check failed: " prefix) this one? */
export const isMessage = (text: string, message: string) => text.includes(message);
/** Engine messages that a setting fixes: the game archive, Blender or the SDK. */
export const needsSettings = (text: string) => [OBJECTS_NO_GAME, OBJECTS_NO_BLENDER, OBJECT_NEEDS_CURRENT_SDK].some((m) => isMessage(text, m));

/** The fit answer: `[u32 header length][header {mime_type, details}][data]`. Each packet is
 * a copy of its bytes (a real ArrayBuffer for `decodePreview`). */
export interface FitAnswer { view: ObjectFitView; game: ArrayBuffer[]; yours: ArrayBuffer | null }
export function parseFitAnswer(data: ArrayBuffer): FitAnswer {
  if (!(data instanceof ArrayBuffer) || data.byteLength < 4) throw new Error("Incomplete object view.");
  const size = new DataView(data).getUint32(0, true);
  if (size + 4 > data.byteLength) throw new Error("Invalid object view header.");
  const head = JSON.parse(new TextDecoder().decode(new Uint8Array(data, 4, size))) as { mime_type: string; details: ObjectFitView };
  if (head.mime_type !== OBJECT_FIT_MIME || !head.details || !Array.isArray(head.details.packets)) throw new Error("Unsupported object view.");
  const body = 4 + size;
  const game: ArrayBuffer[] = [];
  let yours: ArrayBuffer | null = null;
  for (const packet of head.details.packets) {
    const start = body + packet.offset, end = start + packet.length;
    if (!Number.isSafeInteger(packet.offset) || !Number.isSafeInteger(packet.length) || packet.offset < 0 || packet.length < 0 || end > data.byteLength) throw new Error("Invalid object view packet.");
    const bytes = data.slice(start, end);
    if (packet.role === "yours") yours = bytes;
    else game.push(bytes);
  }
  return { view: head.details, game, yours };
}

/** One fit request at a time: while one runs, only the newest waiting request is kept (older
 * waiting ones are dropped unanswered), and an answer that is not for the newest request asked
 * is dropped (`apply` is not called). */
export class FitQueue<T> {
  private running = false;
  private waiting: { id: number; run: () => Promise<T> } | null = null;
  private latest = 0;
  constructor(private apply: (value: T) => void, private failed: (error: unknown) => void) {}
  request(run: () => Promise<T>): number {
    const id = ++this.latest;
    this.waiting = { id, run };
    this.next();
    return id;
  }
  /** Forget what was asked (a different object was selected): answers still running are dropped. */
  drop() {
    this.latest++;
    this.waiting = null;
  }
  get busy() { return this.running || this.waiting !== null; }
  private next() {
    if (this.running || !this.waiting) return;
    const { id, run } = this.waiting;
    this.waiting = null;
    this.running = true;
    void Promise.resolve()
      .then(run)
      .then(
        (value) => { if (id === this.latest) this.apply(value); },
        (error) => { if (id === this.latest) this.failed(error); },
      )
      .finally(() => {
        this.running = false;
        this.next();
      });
  }
}
