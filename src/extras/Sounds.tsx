// Extras → Sounds: a tagged, grouped library of the hero's sounds with
// variations folded into one row, and the shared replacement editor for the
// chosen variation. Tags, names, groups and families come from Rust discovery.
// i18n-area: sounds
import { useEffect, useMemo, useRef, useState } from "react";
import { tc, tp, useLanguage, useT } from "../i18n";
import type { Project, ResourceReplacement } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { changeCustomization } from "../components/customization";
import { basename } from "../api";
import { ExtraIcon } from "./Icons";
import { ExtraDropzone } from "./Dropzone";
import { ReplacementEditor } from "./ExtrasPage";
import { useExtraMedia } from "./media";
import { assetItems, detailNumber, isSilenced, type AssetItem } from "./models";
import { SoundEditor, decibels, seconds } from "./SoundEditor";
import { editValues, hasEdit, replaceAllWithPart } from "./soundEdit";
import {
  MAX_REPLACEMENTS,
  TAGS,
  VOICE_KINDS,
  changedStatus,
  filterRows,
  groupText,
  kindName,
  lengthText,
  reasonText,
  statusText,
  tagName,
  newSilencedRow,
  replaceAll,
  rowStatus,
  shownText,
  silenceAll,
  silenceRefusal,
  silencedRow,
  soundOf,
  soundRows,
  tagCounts,
  tagsOf,
  withSource,
  type ReplaceAllResult,
  type SoundFilter,
  type SoundRow,
  type TagKey,
} from "./soundTags";

export const SOUNDS_PAGE_SIZE = 40;
/** Refusals of a file too long for a replacement without an edit (Rust `prepare_audio`, `read_file`, `preview_file`). */
const LONG_FILE = /Trim longer audio|16-million-sample|smaller than 128 MiB|Converted audio is too large|longer than the game/i;
// Stable empties keep the memoised rows when a project has no sound replacements.
const NO_ROWS: ResourceReplacement[] = [];
const NO_ENTRIES: ComponentCatalog["audio"] = [];
const stopEvent = "dmb-stop-extra-audio";
let rowContext: AudioContext | null = null;

export interface SoundsProps {
  project: Project;
  update: (project: Project) => void;
  disabled: boolean;
  catalog: ComponentCatalog | null;
  discovering: boolean;
  canDiscover: boolean;
  canReload: boolean;
  reload: () => void;
  openSetup?: () => void;
  revision: number;
  hero: string;
}

/** "3 replaced", "2 silenced" or "5 changed" (both kinds) for the saved rows. */
function savedCountText(rows: ResourceReplacement[]) {
  const silent = rows.filter(isSilenced).length;
  if (silent === 0) return tp(rows.length, { one: "{count} replaced", other: "{count} replaced" });
  if (silent === rows.length) return tp(rows.length, { one: "{count} silenced", other: "{count} silenced" });
  return tp(rows.length, { one: "{count} changed", other: "{count} changed" });
}
/** Why a sound cannot be silenced, as shown ("" when it can). */
const refusalShown = (item: AssetItem) => {
  const why = silenceRefusal(item);
  return why ? reasonText(why) : "";
};
function fileName(item: AssetItem) {
  return basename(item.entry.path).replace(/\.vsnd(_c)?$/i, "");
}
function TagChips({ tags }: { tags: TagKey[] }) {
  useT();
  return (
    <span className="sounds-tags">
      {tags.map((tag) => (
        <span key={tag} className="sounds-tag">
          <i className={`sounds-dot ${tag}`} aria-hidden="true" />
          {tagName(tag)}
        </span>
      ))}
    </span>
  );
}

/** Plays one original once; the row stays in charge of which row plays. */
function RowAudio({
  item,
  hero,
  revision,
  done,
  failed,
}: {
  item: AssetItem;
  hero: string;
  revision: number;
  done: () => void;
  failed: (message: string) => void;
}) {
  const { media, error } = useExtraMedia(
    { hero_path: hero, target: item.entry.target, kind: "audio", max_dimension: 512 },
    `${item.entry.original_sha256 ?? ""}:${revision}`,
  );
  const latest = useRef({ done, failed });
  latest.current = { done, failed };
  useEffect(() => {
    if (error) latest.current.failed(error);
  }, [error]);
  useEffect(() => {
    if (!media || media.mime_type !== "audio/wav") return;
    if (typeof AudioContext === "undefined") return;
    let node: AudioBufferSourceNode | null = null,
      stopped = false;
    // Stop the inspector's players and any other row first.
    window.dispatchEvent(new Event(stopEvent));
    const onStop = () => {
      stopped = true;
      latest.current.done();
    };
    window.addEventListener(stopEvent, onStop);
    const context = (rowContext ??= new AudioContext());
    void context
      .resume()
      .then(() => context.decodeAudioData(media.bytes.slice(0)))
      .then((buffer) => {
        if (stopped) return;
        node = context.createBufferSource();
        node.buffer = buffer;
        node.connect(context.destination);
        node.onended = () => {
          if (!stopped) latest.current.done();
        };
        node.start();
      })
      .catch((e) => {
        if (!stopped) latest.current.failed(String(e?.message ?? e));
      });
    return () => {
      stopped = true;
      window.removeEventListener(stopEvent, onStop);
      if (node) {
        node.onended = null;
        try {
          node.stop();
        } catch {
          /* already ended */
        }
        node.disconnect();
      }
    };
  }, [media]);
  return null;
}

function SoundDetails({
  row,
  item,
  choose,
  disabled,
  saved,
  apply,
}: {
  row: SoundRow;
  item: AssetItem;
  choose: (key: string) => void;
  disabled: boolean;
  saved: ResourceReplacement[];
  /** `source` is null for "Silence all N variations". */
  apply: (result: ReplaceAllResult, source: string | null) => void;
}) {
  const t = useT();
  // Which "all variations" panel is open: one file for all, or silence for all.
  const [open, setOpenPanel] = useState<"replace" | "silence" | null>(null),
    [shared, setShared] = useState(false);
  const toggle = (panel: "replace" | "silence") => {
    setOpenPanel(open === panel ? null : panel);
    setShared(false);
  };
  const info = soundOf(item.entry);
  const count = row.items.length;
  const index = row.items.findIndex((i) => i.key === item.key);
  const sharedCount = row.items.filter((i) => i.entry.shared).length;
  const channels = detailNumber(item.entry, "channels"),
    rate = detailNumber(item.entry, "rate");
  const format = [
    channels === null ? "" : channels === 2 ? t("Stereo") : channels === 1 ? t("Mono") : tp(channels, { one: "{count} channel", other: "{count} channels" }),
    rate ? `${Math.round(rate / 100) / 10} kHz` : "",
  ]
    .filter(Boolean)
    .join(" · ");
  const voiceKinds = (info.voice ?? [])
    .filter((k) => VOICE_KINDS.some((v) => v.key === k))
    .map(kindName);
  return (
    <div className="sounds-inspector-details">
      {!item.missing && (
        <div className="sounds-inspector-tags">
          <TagChips tags={tagsOf(info)} />
          {voiceKinds.length > 0 && (
            <span className="sounds-inspector-kinds">{voiceKinds.join(" · ")}</span>
          )}
        </div>
      )}
      <p className="sounds-inspector-file">
        <span>{t("Game file")}</span> {fileName(item)}
        {format && <span className="sounds-inspector-format"> · {format}</span>}
      </p>
      {count > 1 && (
        <div className="sounds-variation">
          <label htmlFor="sound-variation">{t("Variation")}</label>
          <select
            id="sound-variation"
            value={item.key}
            onChange={(e) => choose(e.target.value)}
          >
            {row.items.map((v, i) => (
              <option key={v.key} value={v.key}>
                {v.row
                  ? isSilenced(v.row)
                    ? t("{number} of {count} · silenced", { number: i + 1, count })
                    : t("{number} of {count} · replaced", { number: i + 1, count })
                  : t("{number} of {count}", { number: i + 1, count })}
              </option>
            ))}
          </select>
          <span className="sounds-variation-actions">
            <button
              type="button"
              className="extra-text-button"
              aria-expanded={open === "replace"}
              disabled={disabled}
              onClick={() => toggle("replace")}
            >
              {tp(count, { one: "Replace all {count} variations with one file", other: "Replace all {count} variations with one file" })}
            </button>
            <button
              type="button"
              className="extra-text-button"
              aria-expanded={open === "silence"}
              disabled={disabled}
              onClick={() => toggle("silence")}
            >
              {tp(count, { one: "Silence all {count} variations", other: "Silence all {count} variations" })}
            </button>
          </span>
          <span className="sounds-variation-note">
            {row.fromEvent
              ? t("The game picks one of these at random. Each can be replaced on its own.")
              : t("Numbered copies of one sound (this is {number} of {count}). Each can be replaced on its own.", { number: index + 1, count })}
          </span>
        </div>
      )}
      {count > 1 && open === "replace" && (
        <div className="sounds-replace-all" role="group" aria-label={t("Replace all variations")}>
          <p>
            {t("Every variation gets the same file. Each one can still be changed on its own afterwards. Volume and timing you already set are kept.")}
          </p>
          {sharedCount > 0 && (
            <label className="sounds-replace-shared">
              <input
                type="checkbox"
                checked={shared}
                disabled={disabled}
                onChange={(e) => setShared(e.target.checked)}
              />
              {tp(sharedCount, { one: "Also replace the shared variations ({count}). This can also change other heroes.", other: "Also replace the shared variations ({count}). This can also change other heroes." })}
            </label>
          )}
          <ExtraDropzone
            kind="audio"
            source=""
            disabled={disabled}
            change={(source) => {
              apply(replaceAll(saved, row.items, source, shared), source);
              setOpenPanel(null);
            }}
          />
        </div>
      )}
      {count > 1 && open === "silence" && (
        <div className="sounds-replace-all" role="group" aria-label={t("Silence all variations")}>
          <p>
            {t("The game plays nothing for any of them. Each one can still be changed on its own afterwards.")}
          </p>
          {sharedCount > 0 && (
            <label className="sounds-replace-shared">
              <input
                type="checkbox"
                checked={shared}
                disabled={disabled}
                onChange={(e) => setShared(e.target.checked)}
              />
              {tp(sharedCount, { one: "Also silence the shared variations ({count}). This can also change other heroes.", other: "Also silence the shared variations ({count}). This can also change other heroes." })}
            </label>
          )}
          <button
            type="button"
            className="extra-secondary"
            disabled={disabled}
            onClick={() => {
              apply(silenceAll(saved, row.items, shared), null);
              setOpenPanel(null);
            }}
          >
            {tp(count, { one: "Silence {count} variations", other: "Silence {count} variations" })}
          </button>
        </div>
      )}
    </div>
  );
}

export function Sounds({
  project,
  update,
  disabled,
  catalog,
  discovering,
  canDiscover,
  canReload,
  reload,
  openSetup,
  revision,
  hero,
}: SoundsProps) {
  const t = useT();
  const [query, setQuery] = useState(""),
    [tags, setTags] = useState<TagKey[]>([]),
    [voice, setVoice] = useState<string[]>([]),
    [replacedOnly, setReplacedOnly] = useState(false),
    [replaceableOnly, setReplaceableOnly] = useState(false),
    [selected, setSelected] = useState(""),
    [binding, setBinding] = useState<number | null>(null),
    [page, setPage] = useState(0),
    [sourceRevision, setSourceRevision] = useState(0),
    [playing, setPlaying] = useState(""),
    [playError, setPlayError] = useState<{ key: string; message: string } | null>(null),
    [report, setReport] = useState<{ row: string; source: string | null; result: ReplaceAllResult } | null>(null),
    /** The sound whose editor is open (its item key); opening writes nothing. */
    [editing, setEditing] = useState<string | null>(null);
  const projectRef = useRef(project);
  projectRef.current = project;
  const updateRef = useRef(update);
  updateRef.current = update;
  const active = Boolean(project.customization.enabled);
  const saved = project.customization.audio ?? NO_ROWS;
  const entries = catalog?.audio ?? NO_ENTRIES;
  const items = useMemo(() => assetItems(entries, saved), [entries, saved]);
  const all = useMemo(() => soundRows(items, binding === null), [items, binding]);
  const filter: SoundFilter = { query, tags, voice, replacedOnly, replaceableOnly };
  // The search also matches the tag and voice-kind names shown in the current language.
  const language = useLanguage();
  const visible = useMemo(() => {
    const rows = filterRows(all, filter);
    return binding === null
      ? rows
      : rows.filter((r) => r.items.every((i) => i.entry.supported && !i.row && !i.missing));
    // The filter's fields and the language are the dependencies.
  }, [all, query, tags, voice, replacedOnly, replaceableOnly, binding, language]); // eslint-disable-line react-hooks/exhaustive-deps
  const counts = useMemo(() => tagCounts(all, filter), [all, query, replacedOnly, replaceableOnly, tags, language]); // eslint-disable-line react-hooks/exhaustive-deps
  const viewOnly = useMemo(() => items.filter((i) => !i.missing && !i.entry.supported).length, [items]);
  const groupSizes = useMemo(() => {
    const sizes = new Map<string, number>();
    for (const row of visible) sizes.set(row.group, (sizes.get(row.group) ?? 0) + 1);
    return sizes;
  }, [visible]);
  const pageCount = Math.max(1, Math.ceil(visible.length / SOUNDS_PAGE_SIZE)),
    shownPage = Math.min(page, pageCount - 1),
    pageRows = visible.slice(shownPage * SOUNDS_PAGE_SIZE, (shownPage + 1) * SOUNDS_PAGE_SIZE);
  const files = visible.reduce((sum, r) => sum + r.items.length, 0);
  useEffect(() => setPage(0), [query, tags, voice, replacedOnly, replaceableOnly, binding]);
  const selectedRow = all.find((r) => r.items.some((i) => i.key === selected)) ?? null;
  const selection = selectedRow?.items.find((i) => i.key === selected) ?? null;

  function changeRows(next: ResourceReplacement[], activate = true) {
    updateRef.current(changeCustomization(projectRef.current, { audio: next }, activate));
  }
  function current() {
    return projectRef.current.customization.audio ?? [];
  }
  function edit(item: AssetItem, changes: Partial<ResourceReplacement>) {
    if (item.index >= 0)
      changeRows(current().map((row, i) => (i === item.index ? { ...row, ...changes } : row)));
  }
  /** The row replaced as a whole (a sound edit removes keys, which `edit` would keep). */
  function put(item: AssetItem, next: ResourceReplacement) {
    if (item.index >= 0) changeRows(current().map((row, i) => (i === item.index ? next : row)));
  }
  /** "Silence this sound": the row keeps its target, inclusion and shared choice. */
  function silence(item: AssetItem) {
    const rows = current();
    if (item.index >= 0)
      changeRows(rows.map((row, i) => (i === item.index ? silencedRow(row) : row)));
    else if (rows.length < MAX_REPLACEMENTS) changeRows([...rows, newSilencedRow(item)]);
  }
  function add(item: AssetItem, source: string) {
    const rows = current();
    // A file given to a silenced sound replaces the silence.
    if (item.index >= 0)
      changeRows(rows.map((row, i) => (i === item.index ? withSource(row, source) : row)));
    else if (rows.length < MAX_REPLACEMENTS)
      changeRows([
        ...rows,
        {
          enabled: true,
          source,
          target: item.entry.target,
          allow_shared: false,
          ...(item.entry.original_sha256 ? { original_sha256: item.entry.original_sha256 } : {}),
          volume_percent: 100,
          audio_fit: "pad" as const,
        },
      ]);
    setSourceRevision((v) => v + 1);
  }
  function choose(row: SoundRow) {
    const first = row.items[0];
    if (binding !== null && row.items.length === 1 && first.entry.supported && !first.row) {
      changeRows(
        current().map((saved, i) =>
          i === binding
            ? {
                ...saved,
                target: first.entry.target,
                original_sha256: first.entry.original_sha256 ?? undefined,
                allow_shared: false,
              }
            : saved,
        ),
      );
      setBinding(null);
    }
    setReport(null);
    setSelected((row.items.find((i) => i.row) ?? first).key);
  }
  function toggleTag(tag: TagKey) {
    setTags((list) => (list.includes(tag) ? list.filter((other) => other !== tag) : [...list, tag]));
    if (tag === "voice" && tags.includes("voice")) setVoice([]);
  }
  function clear() {
    setTags([]);
    setVoice([]);
    setQuery("");
    setReplacedOnly(false);
    setReplaceableOnly(false);
  }
  const filtered = Boolean(query || tags.length || replacedOnly || replaceableOnly);
  const nothing = !discovering && (catalog || replacedOnly) && visible.length === 0;
  // The editor: only for a sound in the catalog with a replacement file.
  const editable = (item: AssetItem | null) =>
    Boolean(item && !item.missing && item.entry.supported && item.row && !isSilenced(item.row) && (item.row.source ?? "").trim());
  const edited = editing !== null && selection?.key === editing && editable(selection) && selectedRow ? selection : null;
  if (edited && selectedRow) {
    const row = edited.row!;
    const others = selectedRow.items.length;
    return (
      <div id="extras-panel" role="tabpanel" aria-labelledby="extras-tab-audio" className="sounds-edit-layout">
        <SoundEditor
          key={edited.key}
          hero={project.original_model}
          heroName={hero}
          name={soundOf(edited.entry).name}
          target={edited.entry.target}
          row={row}
          disabled={disabled}
          commit={(next) => put(edited, next)}
          close={() => setEditing(null)}
          silence={() => silence(edited)}
          silenceRefusal={refusalShown(edited)}
          variations={others > 1 ? {
            count: others,
            sharedCount: selectedRow.items.filter((i) => i.entry.shared).length,
            apply: (values, includeShared) => {
              const result = replaceAllWithPart(current(), selectedRow.items, row.source!, values, includeShared, row.volume_percent);
              changeRows(result.rows);
              setSourceRevision((v) => v + 1);
              setReport({ row: selectedRow.key, source: row.source!, result });
              setEditing(null);
            },
          } : null}
        />
      </div>
    );
  }

  return (
    <div
      id="extras-panel"
      role="tabpanel"
      aria-labelledby="extras-tab-audio"
      className="extras-layout sounds-layout"
    >
      <section className="extras-library sounds-library" aria-label={t("Sounds assets")}>
        <div className="extras-library-heading">
          <div>
            <h2>{t("Sounds")}</h2>
            <p>
              {t("Replace or silence the hero’s voice, ability and weapon sounds")}
              {catalog ? ` · ${tp(catalog.audio.length, { one: "{count} sound from your game", other: "{count} sounds from your game" }, { count: catalog.audio.length.toLocaleString("en-US") })}` : ""}
            </p>
          </div>
          {saved.length > 0 ? (
            <span className="sounds-replaced-count">{savedCountText(saved)}</span>
          ) : (
            <span className="extra-library-count">{catalog ? "" : t("From your game")}</span>
          )}
        </div>
        <div className="extras-search">
          <ExtraIcon name="search" size={18} />
          <input
            aria-label={t("Search sounds")}
            placeholder={t("Search sounds, for example “reload”, “kill” or “ping”…")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {query && (
            <button aria-label={t("Clear search")} onClick={() => setQuery("")}>
              <ExtraIcon name="close" size={16} />
            </button>
          )}
        </div>
        <div className="sounds-filters">
          <div role="group" aria-labelledby="sounds-show" className="sounds-filter-group">
            <span className="sounds-filter-label" id="sounds-show">
              {t("Show")}
            </span>
            {TAGS.map((tag) => (
              <button
                key={tag.key}
                type="button"
                className="sounds-filter"
                aria-pressed={tags.includes(tag.key)}
                onClick={() => toggleTag(tag.key)}
              >
                <i className={`sounds-dot ${tag.key}`} aria-hidden="true" />
                {tagName(tag.key)}
                <small>{(counts.tags.get(tag.key) ?? 0).toLocaleString("en-US")}</small>
              </button>
            ))}
          </div>
          <span className="sounds-filter-gap" />
          <button
            type="button"
            className="sounds-filter"
            aria-pressed={replacedOnly}
            onClick={() => setReplacedOnly(!replacedOnly)}
          >
            {t("Replaced only")}<small>{saved.length}</small>
          </button>
          {viewOnly > 0 && (
            <button
              type="button"
              className="sounds-filter"
              aria-pressed={replaceableOnly}
              onClick={() => setReplaceableOnly(!replaceableOnly)}
            >
              {t("Replaceable only")}
            </button>
          )}
          <button type="button" className="extra-text-button sounds-clear" disabled={!filtered} onClick={clear}>
            {t("Clear")}
          </button>
        </div>
        {tags.includes("voice") && (
          <div className="sounds-subfilters" role="group" aria-label={t("Voice line kinds")}>
            {VOICE_KINDS.map((kind) => {
              const n = counts.voice.get(kind.key) ?? 0,
                on = voice.includes(kind.key);
              return (
                <button
                  key={kind.key}
                  type="button"
                  className="sounds-subfilter"
                  aria-pressed={on}
                  disabled={!n && !on}
                  onClick={() =>
                    setVoice((list) => (on ? list.filter((k) => k !== kind.key) : [...list, kind.key]))
                  }
                >
                  {kindName(kind.key)}
                  <small>{n.toLocaleString("en-US")}</small>
                </button>
              );
            })}
          </div>
        )}
        {binding !== null ? (
          <div className="sounds-binding">
            <p className="extra-inline-note">
              {t("Choose a replaceable sound without an existing replacement to assign your saved file.")}
            </p>
            <button className="extra-text-button" onClick={() => setBinding(null)}>
              {t("Cancel destination change")}
            </button>
          </div>
        ) : (
          (catalog || replacedOnly) &&
          !discovering && (
            <p className="sounds-shown" aria-live="polite">
              {shownText(visible.length, files, filter)}
            </p>
          )
        )}
        {discovering ? (
          <div className="extra-empty">
            <span className="extra-spinner" />
            <h3>{t("Finding {hero}’s sounds", { hero })}</h3>
            <p>{t("Reading sound files and sound events from your game archive.")}</p>
          </div>
        ) : !catalog && !replacedOnly ? (
          <div className="extra-empty">
            <div className="extra-empty-symbol">
              <ExtraIcon name="audio" size={32} />
            </div>
            <h3>{canDiscover ? t("Load your character’s assets") : t("Connect your game to get started")}</h3>
            <p>
              {canDiscover
                ? t("Browse the original sounds before choosing what to replace. Your saved replacements stay in Replaced only.")
                : t("Choose the original game VPK in Setup, then return here to load this character’s icons, sounds and effects.")}
            </p>
            {canDiscover ? (
              <button className="extra-primary" onClick={reload} disabled={disabled || !canReload}>
                {t("Load character assets")}
                <ExtraIcon name="arrow" size={16} />
              </button>
            ) : (
              openSetup && (
                <button className="extra-primary" onClick={openSetup}>
                  {t("Open Setup")}
                  <ExtraIcon name="arrow" size={16} />
                </button>
              )
            )}
          </div>
        ) : nothing ? (
          <div className="extra-empty">
            <ExtraIcon name={query ? "search" : "audio"} size={32} />
            <h3>
              {query
                ? t("No matching sounds")
                : replacedOnly && !tags.length
                  ? t("No replacements yet")
                  : filtered
                    ? t("No sounds with these tags")
                    : t("No assets found in this category")}
            </h3>
            <p>
              {query || tags.length
                ? t("Try another name or clear your filters.")
                : replacedOnly
                  ? t("Select an original sound, then drop in a replacement.")
                  : t("Discovery uses the selected model, known character aliases and linked game resources.")}
            </p>
            {filtered && <button onClick={clear}>{t("Show all sounds")}</button>}
          </div>
        ) : (
          <div className="sounds-list">
            {pageRows.map((row, i) => {
              const heading = i === 0 || pageRows[i - 1].group !== row.group;
              return (
                <SoundGroupRow
                  key={row.key}
                  row={row}
                  heading={heading ? { name: row.groupName, count: groupSizes.get(row.group) ?? 0 } : null}
                  selected={selectedRow?.key === row.key}
                  active={active}
                  playing={playing === row.key}
                  error={playError?.key === row.key ? playError.message : ""}
                  play={() => {
                    setPlayError(null);
                    setPlaying(playing === row.key ? "" : row.key);
                  }}
                  choose={() => choose(row)}
                />
              );
            })}
          </div>
        )}
        {visible.length > SOUNDS_PAGE_SIZE && !discovering && (
          <div className="extras-pagination">
            <span>
              {t("{from}–{to} of {total}", { from: shownPage * SOUNDS_PAGE_SIZE + 1, to: Math.min(visible.length, (shownPage + 1) * SOUNDS_PAGE_SIZE), total: visible.length.toLocaleString("en-US") })}
            </span>
            <button aria-label={t("Previous sounds page")} disabled={shownPage === 0} onClick={() => setPage(shownPage - 1)}>
              {tc("page", "Previous")}
            </button>
            <span>
              {t("Page {page} of {count}", { page: shownPage + 1, count: pageCount })}
            </span>
            <button
              aria-label={t("Next sounds page")}
              disabled={shownPage + 1 >= pageCount}
              onClick={() => setPage(shownPage + 1)}
            >
              {tc("page", "Next")}
            </button>
          </div>
        )}
        {catalog && (
          <div className="extras-library-footer">
            <ExtraIcon name="folder" size={15} />
            <span>{t("Sounds for {hero} · shared resources are labeled", { hero })}</span>
          </div>
        )}
        {playing &&
          (() => {
            const row = all.find((r) => r.key === playing);
            const item = row && (row.key === selectedRow?.key && selection ? selection : row.items[0]);
            return item && !item.missing ? (
              <RowAudio
                key={`${playing}:${item.key}`}
                item={item}
                hero={project.original_model}
                revision={revision}
                done={() => setPlaying((p) => (p === playing ? "" : p))}
                failed={(message) => {
                  setPlayError({ key: playing, message });
                  setPlaying("");
                }}
              />
            ) : null;
          })()}
      </section>
      <aside className="extras-inspector sounds-inspector" aria-label={t("Replacement editor")}>
        {report && selectedRow?.key === report.row && (
          <div className="sounds-report" role="status">
            <strong>
              {report.source === null
                ? tp(report.result.set.length + report.result.refused.length, { one: "{done} of {count} variations are now silenced", other: "{done} of {count} variations are now silenced" }, { done: report.result.set.length })
                : tp(report.result.set.length + report.result.refused.length, { one: "{done} of {count} variations now use {file}", other: "{done} of {count} variations now use {file}" }, { done: report.result.set.length, file: basename(report.source) })}
            </strong>
            {report.result.refused.length > 0 && (
              <>
                <p>{t("Not set:")}</p>
                <ul>
                  {report.result.refused.map(({ item, reason }) => (
                    <li key={item.key}>
                      <b>{fileName(item)}</b> {reasonText(reason)}
                    </li>
                  ))}
                </ul>
              </>
            )}
            <button className="extra-text-button" onClick={() => setReport(null)}>
              {t("Dismiss")}
            </button>
          </div>
        )}
        {selection && selectedRow ? (
          <ReplacementEditor
            key={`audio:${selection.key}`}
            item={selection.missing ? selection : { ...selection, entry: { ...selection.entry, label: soundOf(selection.entry).name } }}
            kind="audio"
            hero={project.original_model}
            revision={revision}
            sourceRevision={sourceRevision}
            disabled={disabled}
            active={active}
            full={saved.length >= MAX_REPLACEMENTS}
            edit={(changes) => edit(selection, changes)}
            add={(path) => add(selection, path)}
            silence={() => silence(selection)}
            silenceRefusal={refusalShown(selection)}
            previewError={(error) => editable(selection) && !hasEdit(selection.row) && LONG_FILE.test(error) ? (
              <div className="extra-preview-placeholder extra-sound-long" role="status">
                <strong>{t("This file is longer than the game’s sound: choose the part that plays.")}</strong>
                <button type="button" className="extra-primary" onClick={() => { setPlaying(""); setEditing(selection.key); }}>
                  {t("Choose the part that plays")}
                </button>
              </div>
            ) : null}
            soundEdit={editable(selection) ? (
              <div className="extra-sound-edit">
                <button type="button" className="extra-primary" onClick={() => { setPlaying(""); setEditing(selection.key); }}>
                  {t("Edit the part, volume and fades")}
                </button>
                <small>
                  {hasEdit(selection.row)
                    ? (() => {
                        const v = editValues(selection.row);
                        return t("From {start} · {volume} · fade in {fadeIn}, fade out {fadeOut}", { start: seconds(v.start_seconds), volume: decibels(v.volume_db), fadeIn: seconds(v.fade_in_seconds), fadeOut: seconds(v.fade_out_seconds) });
                      })()
                    : t("Choose which part of your file plays, how loud, and how it fades.")}
                </small>
              </div>
            ) : null}
            remove={() => changeRows(current().filter((_, i) => i !== selection.index), false)}
            chooseDestination={() => {
              setBinding(selection.index);
              clear();
            }}
          >
            <SoundDetails
              row={selectedRow}
              item={selection}
              choose={setSelected}
              disabled={disabled}
              saved={saved}
              apply={(result, source) => {
                changeRows(result.rows);
                setSourceRevision((v) => v + 1);
                setReport({ row: selectedRow.key, source, result });
              }}
            />
          </ReplacementEditor>
        ) : (
          <div className="extra-editor-empty">
            <div className="extra-comparison-illustration" aria-hidden="true">
              <div>
                <ExtraIcon name="audio" size={32} />
              </div>
              <ExtraIcon name="arrow" size={24} />
              <div>
                <ExtraIcon name="upload" size={30} />
              </div>
            </div>
            <span className="extras-eyebrow">{t("PREVIEW & REPLACE")}</span>
            <h2>{t("Start with the original")}</h2>
            <p>{t("Choose a sound from the library to compare it with your replacement.")}</p>
            <ul>
              <li>
                <ExtraIcon name="check" size={16} />
                {t("Preview before you change anything")}
              </li>
              <li>
                <ExtraIcon name="check" size={16} />
                {t("Drop a file or browse your computer")}
              </li>
              <li>
                <ExtraIcon name="check" size={16} />
                {t("Choose the part that plays, its volume and fades")}
              </li>
            </ul>
          </div>
        )}
      </aside>
    </div>
  );
}

function SoundGroupRow({
  row,
  heading,
  selected,
  active,
  playing,
  error,
  play,
  choose,
}: {
  row: SoundRow;
  heading: { name: string; count: number } | null;
  selected: boolean;
  active: boolean;
  playing: boolean;
  error: string;
  play: () => void;
  choose: () => void;
}) {
  const t = useT();
  const first = row.items[0],
    count = row.items.length,
    status = rowStatus(row, active),
    voiceOnly = row.tags.includes("voice");
  const sub = first.missing
    ? t("Saved destination unavailable")
    : count > 1
      ? voiceOnly
        ? row.fromEvent
          ? tp(count, { one: "{count} line · the game picks one at random", other: "{count} lines · the game picks one at random" })
          : tp(count, { one: "{count} line", other: "{count} lines" })
        : row.fromEvent
          ? tp(count, { one: "{count} variation · the game picks one at random", other: "{count} variations · the game picks one at random" })
          : tp(count, { one: "{count} variation", other: "{count} variations" })
      : fileName(first);
  const tone =
    status === "Needs attention"
      ? "attention"
      : changedStatus(status)
        ? "changed"
        : "";
  return (
    <>
      {heading && (
        <h3 className="sounds-group">
          <span>{groupText(heading.name)}</span>
          <em>
            · {tp(heading.count, { one: "{count} sound", other: "{count} sounds" })}
          </em>
          <i aria-hidden="true" />
        </h3>
      )}
      <div className={`sounds-row ${selected ? "selected" : ""}`}>
        <button
          type="button"
          className={`sounds-play ${playing ? "playing" : ""}`}
          aria-label={playing ? t("Stop original") : t("Play original")}
          aria-pressed={playing}
          title={playing ? t("Stop") : t("Play the original")}
          disabled={first.missing}
          onClick={play}
        >
          <ExtraIcon name={playing ? "pause" : "play"} size={14} />
        </button>
        <button type="button" className="sounds-row-main" aria-pressed={selected} onClick={choose}>
          <span className="sounds-row-name">
            <strong>{row.name}</strong>
            <small>{error ? t("Preview unavailable: {error}", { error }) : sub}</small>
          </span>
          <TagChips tags={row.tags} />
          <span className="sounds-row-length">{lengthText(row.items)}</span>
          <span className={`sounds-state ${tone}`}>{statusText(status)}</span>
        </button>
      </div>
    </>
  );
}
