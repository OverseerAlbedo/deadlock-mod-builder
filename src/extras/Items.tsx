// Extras → Items: the shop's item pictures, the same for every hero, grouped
// like the in-game shop (Weapon, Vitality, Spirit, then tiers with their price),
// plus Legendary (Street Brawl) and Misc. The catalog is read only when this
// tab opens, and pictures load only for the tiles of the open category.
// i18n-area: items
import { useEffect, useMemo, useRef, useState } from "react";
import type { Project, ResourceReplacement } from "../models";
import { api } from "../api";
import { m, t, tp, useLanguage, useT } from "../i18n";
import { T } from "../i18n/T";
import { shownItem, useGameNames } from "../i18n/gameNames";
import { changeCustomization } from "../components/customization";
import { ExtraIcon } from "./Icons";
import { ReplacementEditor } from "./ExtrasPage";
import { previewError, useExtraMedia, useVisible } from "./media";
import { replacementStatus, type AssetItem } from "./models";
import { usePictureChecks } from "./pictureChecks";
import {
  ITEM_CATEGORIES,
  MAX_ITEM_ICONS,
  categoryCounts,
  categoryLabel,
  filterItems,
  itemAssets,
  itemGroups,
  itemOf,
  placeText,
  souls,
  SWITCHED_OFF_REASON,
  type ItemCatalog,
  type ItemCategory,
  type ItemIcon,
  type ItemGroup,
} from "./itemCatalog";
import {
  itemNameOf,
  savedItemNames,
  withItemName,
  withItemNameRows,
  withoutItemName,
  type ItemNameRow,
  type ItemNamesState,
} from "./itemNames";
import { ItemHeading, statusWord } from "./ItemName";
import "./items.css";

const NO_ROWS: ResourceReplacement[] = [];
const NO_NAMES: ItemNameRow[] = [];
export const MISSING_NAMES_TITLE = m("Saved names for items the game no longer has");
export const UNLISTED_NAMES_TITLE = m("Saved names for items this list does not show");
const fold = (value: string) => value.toLowerCase();
const newIcons = (n: number) => tp(n, { one: "{count} new icon", other: "{count} new icons" });
const nameCount = (n: number) => tp(n, { one: "{count} name", other: "{count} names" });
/** "Remove 2 new icons and 1 name?" as one sentence per case. */
function removeQuestion(icons: number, names: number) {
  if (icons && names) return t("Remove {icons} and {names}?", { icons: newIcons(icons), names: nameCount(names) });
  return t("Remove {what}?", { what: icons ? newIcons(icons) : nameCount(names) });
}
/** A tile's accessible name: the shown name, then whether it is switched off and its icon's state. */
function tileLabel(name: string, off: boolean, status: string | null) {
  const state = status === null ? "" : statusWord(status).toLowerCase();
  if (off && state) return t("{name}, switched off in the game, {state}", { name, state });
  if (off) return t("{name}, switched off in the game", { name });
  if (state) return t("{name}, {state}", { name, state });
  return name;
}
/** The item record a tile's rename belongs to: its owner (the record whose name the tile shows). */
export function nameToken(item: AssetItem): string | null {
  return itemOf(item)?.items[0] ?? null;
}
/** A tile's game name as the game shows it in the interface language (English without one). */
export function shownItemLabel(item: AssetItem): string {
  return shownItem(nameToken(item), item.entry.label);
}
/** The other item records' names of a picture, as the game shows them in the interface language. */
function otherNames(entry: ItemIcon): string[] {
  return entry.names.slice(1).map((name, i) => shownItem(entry.items[i + 1], name));
}
export const ITEM_NOTE = m(
  "Item icons are the same for every hero, so this change shows for all heroes while the mod is installed.");
/** Under a picture the game's names file names no item for (Misc), once the names were read. */
export const NO_ITEM_NAME_NOTE = m("This picture has no item name in the shop, so only its picture can be replaced.");

// The catalog of this session, kept across tab switches (the app keeps it per
// game archive too, so a new read after Reload assets is cheap).
let kept: ItemCatalog | null = null;
// The game's item names of this session (asked with the catalog).
let keptNames: ItemNamesState | null = null;
/** Tests: forget the kept catalog. */
export function forgetItemCatalog() {
  kept = null;
  keptNames = null;
}
/** Items the game has now (shop and Legendary, not switched off) of the kept
 * catalog, or null before the tab was opened. */
export function keptItemCount() {
  return kept ? kept.items.filter((i) => i.category !== "misc" && !i.switched_off).length : null;
}

export interface ItemsProps {
  project: Project;
  update: (project: Project) => void;
  disabled: boolean;
  /** Whether a game archive is set (Setup). */
  canLoad: boolean;
  openSetup?: () => void;
  /** Bumped by Reload assets: read the game's items again. */
  revision: number;
  /** Called after the catalog was read (the Items tab shows its count). */
  loaded?: () => void;
}

function ItemTile({
  item,
  selected,
  active,
  revision,
  choose,
  renamed,
}: {
  item: AssetItem;
  selected: boolean;
  active: boolean;
  revision: number;
  choose: () => void;
  /** The item's saved new name, if any. */
  renamed: string | null;
}) {
  const entry = itemOf(item);
  const { ref, visible } = useVisible();
  const { url, loading, error } = useExtraMedia(
    visible && entry ? { hero_path: "", target: entry.target, kind: "items", max_dimension: 128 } : null,
    `${entry?.original_sha256 ?? ""}:${revision}`,
  );
  usePictureChecks();
  const t = useT();
  useGameNames();
  const status = replacementStatus(item, active);
  const off = Boolean(entry?.switched_off);
  const others = entry && entry.names.length > 1 ? otherNames(entry).join(", ") : "";
  // The game's name in the interface language (English when the game has none there).
  const game = shownItemLabel(item);
  const name = renamed === null ? game : t("{name}, renamed from {game}", { name: renamed, game });
  const tip = renamed === null ? game : t("{name} · renamed · in the game: {game}", { name: renamed, game });
  const withAlso = others ? t("{text} · also {names}", { text: tip, names: others }) : tip;
  return (
    <div role="listitem" ref={ref}>
      <button
        type="button"
        className={`items-tile ${selected ? "selected" : ""} ${item.row ? "replaced" : ""} ${off ? "off" : ""} ${renamed === null ? "" : "renamed"}`}
        data-category={entry?.category ?? "misc"}
        aria-pressed={selected}
        aria-label={tileLabel(name, off, item.row ? status : null)}
        title={off ? t("{text} · {reason}", { text: withAlso, reason: t(SWITCHED_OFF_REASON) }) : withAlso}
        onClick={choose}
      >
        <span className={`items-picture ${loading ? "loading" : ""}`}>
          {url ? <img src={url} alt="" /> : <ExtraIcon name={error || item.missing ? "warning" : "image"} size={22} />}
          {item.row && <i className={`items-dot ${status === "Replaced" ? "" : "attention"}`} aria-hidden="true" />}
        </span>
        {renamed === null ? (
          <span className="items-name">{game}</span>
        ) : (
          <>
            <span className="items-name">
              <span className="items-name-mark" aria-hidden="true">
                <ExtraIcon name="pencil" size={11} />
              </span>
              {renamed}
            </span>
            <span className="items-was">{game}</span>
          </>
        )}
      </button>
    </div>
  );
}

function GroupRow({
  group,
  search,
  ...tile
}: {
  group: ItemGroup;
  search: boolean;
  selected: string;
  active: boolean;
  revision: number;
  choose: (item: AssetItem) => void;
  renamed: (item: AssetItem) => string | null;
}) {
  const t = useT();
  return (
    <section className={`items-tier ${search ? "search" : ""}`} data-category={group.category} aria-label={search ? t("{category} · {group}", { category: categoryLabel(group.category), group: group.title }) : group.title}>
      <header className="items-tier-label">
        {search && (
          <span className="items-tier-category">
            <i className="items-category-dot" data-category={group.category} aria-hidden="true" />
            {categoryLabel(group.category)}
          </span>
        )}
        <strong>{group.title}</strong>
        {group.price !== null ? (
          <span className="items-price">
            <T text="<b>{price}</b> souls" values={{ price: souls(group.price) }} tags={{ b: (chunk) => <b>{chunk}</b> }} />
          </span>
        ) : (
          group.note && <span className="items-tier-note">{group.note}</span>
        )}
      </header>
      <div className="items-tiles" role="list">
        {group.items.map((item) => (
          <ItemTile
            key={item.key}
            item={item}
            selected={tile.selected === item.key}
            active={tile.active}
            revision={tile.revision}
            choose={() => tile.choose(item)}
            renamed={tile.renamed(item)}
          />
        ))}
      </div>
    </section>
  );
}

export function Items({ project, update, disabled, canLoad, openSetup, revision, loaded }: ItemsProps) {
  const [catalog, setCatalog] = useState<ItemCatalog | null>(kept),
    [loading, setLoading] = useState(false),
    [error, setError] = useState(""),
    [category, setCategory] = useState<ItemCategory>("weapon"),
    [query, setQuery] = useState(""),
    [replacedOnly, setReplacedOnly] = useState(false),
    [selected, setSelected] = useState(""),
    [binding, setBinding] = useState<number | null>(null),
    [sourceRevision, setSourceRevision] = useState(0),
    [confirmReset, setConfirmReset] = useState(false);
  const language = useLanguage();
  const t = useT();
  const gameNames = useGameNames();
  const projectRef = useRef(project);
  projectRef.current = project;
  const updateRef = useRef(update);
  updateRef.current = update;
  const seenRevision = useRef(revision);
  const loadedRef = useRef(loaded);
  loadedRef.current = loaded;
  const [names, setNames] = useState<ItemNamesState | null>(keptNames);
  const active = Boolean(project.customization.enabled);
  const saved = project.customization.item_icons ?? NO_ROWS;
  const savedNames = project.customization.item_names;
  const nameRows = useMemo(() => (Array.isArray(savedNames) ? savedNames : NO_NAMES), [savedNames]);
  const items = useMemo(() => itemAssets(catalog, saved), [catalog, saved]);
  const counts = useMemo(() => categoryCounts(items), [items]);
  const searching = query.trim().length > 0;
  // A tile's saved new name (by its owner record), from the project alone.
  const renamedOf = useMemo(() => {
    const byItem = new Map(nameRows.map((r) => [fold(r.item), r.name]));
    return (item: AssetItem) => {
      const token = nameToken(item);
      return token === null ? null : (byItem.get(fold(token)) ?? null);
    };
  }, [nameRows]);
  // Searched by the new name, the English name and the game's name in the interface language.
  const shown = useMemo(() => filterItems(items, query, replacedOnly, renamedOf), [items, query, replacedOnly, renamedOf, gameNames]);
  const changedCount = useMemo(() => items.filter((i) => i.row || renamedOf(i) !== null).length, [items, renamedOf]);
  // Whether the game's names file names an item record (only then can it be renamed).
  const gameHas = (token: string) => Boolean(names?.found && Object.prototype.hasOwnProperty.call(names.names, fold(token)));
  // Saved names the game does not have now, and names the game has that no tile here carries.
  const shownTokens = useMemo(() => new Set(items.map(nameToken).filter((token): token is string => token !== null).map(fold)), [items]);
  const missingNames = names?.found ? nameRows.filter((r) => !gameHas(r.item)) : [];
  const unlistedNames = names?.found && catalog ? nameRows.filter((r) => gameHas(r.item) && !shownTokens.has(fold(r.item))) : [];
  const total = catalog ? catalog.items.filter((i) => i.category !== "misc" && !i.switched_off).length : 0;
  // Misc shows only when it has something (rows of the catalog or saved pictures).
  const shownCategories = ITEM_CATEGORIES.filter(({ key }) => key !== "misc" || !catalog || counts.get("misc")!.rows > 0);
  const openCategory: ItemCategory = shownCategories.some((c) => c.key === category) ? category : "weapon";
  // The group titles are drawn in the language shown: built again when it changes.
  const groups = useMemo(
    () => itemGroups(binding === null ? shown : shown.filter((i) => !i.row && !i.missing && i.entry.supported), searching ? null : openCategory),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the language is read inside
    [shown, searching, openCategory, binding, language],
  );
  const selection = items.find((i) => i.key === selected) ?? null;

  function load(reload: boolean) {
    setLoading(true);
    setError("");
    void api
      .itemCatalog(reload)
      .then((value) => {
        kept = value;
        setCatalog(value);
        loadedRef.current?.();
      })
      .catch((e) => setError(previewError(e)))
      .finally(() => setLoading(false));
    // The game's item names, asked only here (the tab is open): renaming and the list of
    // saved names the game no longer has. Without them nothing can be renamed.
    void Promise.resolve()
      .then(() => api.itemNamesState(savedItemNames(projectRef.current)))
      .then((value) => {
        keptNames = value ?? null;
        setNames(keptNames);
      })
      .catch(() => {
        keptNames = null;
        setNames(null);
      });
  }
  // Ask for the game's items whenever the tab opens: the app keeps them per
  // game archive, so this is instant unless the game or Settings changed. The
  // list kept here only avoids a loading flash meanwhile ...
  useEffect(() => {
    if (canLoad) load(false);
    // Only on opening the tab and when a game becomes available.
  }, [canLoad]);
  // ... and again after Reload assets.
  useEffect(() => {
    if (revision !== seenRevision.current) {
      seenRevision.current = revision;
      if (canLoad) load(true);
    }
  }, [revision, canLoad]);

  function current() {
    return projectRef.current.customization.item_icons ?? [];
  }
  function changeRows(next: ResourceReplacement[], activate = true) {
    updateRef.current(changeCustomization(projectRef.current, { item_icons: next }, activate));
  }
  function edit(item: AssetItem, changes: Partial<ResourceReplacement>) {
    if (item.index >= 0) changeRows(current().map((row, i) => (i === item.index ? { ...row, ...changes } : row)));
  }
  function add(item: AssetItem, source: string) {
    const rows = current();
    if (item.index >= 0) edit(item, { source });
    else if (rows.length < MAX_ITEM_ICONS)
      changeRows([
        ...rows,
        {
          enabled: true,
          source,
          target: item.entry.target,
          allow_shared: false,
          ...(item.entry.original_sha256 ? { original_sha256: item.entry.original_sha256 } : {}),
          fit: "contain" as const,
        },
      ]);
    setSourceRevision((v) => v + 1);
  }
  function choose(item: AssetItem) {
    if (binding !== null && !item.row && !item.missing && item.entry.supported) {
      changeRows(
        current().map((row, i) =>
          i === binding ? { ...row, target: item.entry.target, original_sha256: item.entry.original_sha256 ?? undefined, allow_shared: false } : row,
        ),
      );
      setBinding(null);
    }
    setSelected(item.key);
  }
  /** One step: the project with these item name rows. */
  function changeNames(next: ItemNameRow[], activate: boolean) {
    updateRef.current(withItemNameRows(projectRef.current, next, activate));
  }
  /** Save a checked name for an item record: the game's own name (in English, or as the game
   * shows it in the interface language) removes the rename; any other name is saved as typed. */
  function rename(token: string, gameName: string, name: string): string | undefined {
    const rows = savedItemNames(projectRef.current);
    if (name === gameName || name === shownItem(token, gameName)) {
      if (itemNameOf(rows, token)) changeNames(withoutItemName(rows, token), false);
      return undefined;
    }
    const next = withItemName(rows, token, name);
    if (next.error) return next.error;
    changeNames(next.rows, true);
    return undefined;
  }
  /** Remove every new icon and every name in one step. */
  function removeAll() {
    let next = projectRef.current;
    if ((next.customization.item_icons ?? []).length) next = changeCustomization(next, { item_icons: [] }, false);
    if (savedItemNames(next).length) next = withItemNameRows(next, [], false);
    updateRef.current(next);
  }
  function chooseCategory(next: ItemCategory) {
    setCategory(next);
    setQuery("");
  }

  const entry = selection ? itemOf(selection) : null;
  const selectionToken = selection ? nameToken(selection) : null;
  return (
    <div id="extras-panel" role="tabpanel" aria-labelledby="extras-tab-items" className="extras-layout items-layout">
      <section className="extras-library items-library" aria-label={t("Item icons")}>
        <div className="extras-library-heading">
          <div>
            <h2>{t("Items")}</h2>
            <p>
              {catalog
                ? tp(total, { other: "Replace the icons of the shop’s items with your own pictures, or rename them · {total} items from your game · icons load when you open this page" }, { total: total.toLocaleString("en-US") })
                : t("Replace the icons of the shop’s items with your own pictures, or rename them · icons load when you open this page")}
            </p>
          </div>
          <div className="items-heading-actions">
            {saved.length > 0 && <span className="sounds-replaced-count">{newIcons(saved.length)}</span>}
            {nameRows.length > 0 && <span className="sounds-replaced-count">{tp(nameRows.length, { other: "{count} renamed" })}</span>}
            {(saved.length > 0 || nameRows.length > 0) &&
              (confirmReset ? (
                <span className="items-confirm" role="group" aria-label={t("Remove every item icon and name")}>
                  <span>{removeQuestion(saved.length, nameRows.length)}</span>
                  <button
                    type="button"
                    className="extra-text-button items-confirm-yes"
                    disabled={disabled}
                    onClick={() => {
                      removeAll();
                      setConfirmReset(false);
                      setSelected("");
                    }}
                  >
                    {t("Remove")}
                  </button>
                  <button type="button" className="extra-text-button" onClick={() => setConfirmReset(false)}>
                    {t("Keep")}
                  </button>
                </span>
              ) : (
                <button type="button" className="extra-text-button" disabled={disabled} onClick={() => setConfirmReset(true)}>
                  {t("Remove all…")}
                </button>
              ))}
          </div>
        </div>
        <div className="items-toolbar">
          <div className="items-categories" role="tablist" aria-label={t("Item categories")}>
            {shownCategories.map(({ key, label }, index) => {
              const count = counts.get(key)!;
              const on = !searching && openCategory === key;
              return (
                <button
                  key={key}
                  id={`items-category-${key}`}
                  type="button"
                  role="tab"
                  aria-selected={on}
                  aria-controls="items-groups"
                  tabIndex={on || (searching && index === 0) ? 0 : -1}
                  className="items-category"
                  data-category={key}
                  onClick={() => chooseCategory(key)}
                  onKeyDown={(event) => {
                    const move = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
                    if (move || event.key === "Home" || event.key === "End") {
                      event.preventDefault();
                      const next =
                        event.key === "Home"
                          ? 0
                          : event.key === "End"
                            ? shownCategories.length - 1
                            : (index + move + shownCategories.length) % shownCategories.length;
                      chooseCategory(shownCategories[next].key);
                      document.getElementById(`items-category-${shownCategories[next].key}`)?.focus();
                    }
                  }}
                >
                  <i className="items-category-dot" data-category={key} aria-hidden="true" />
                  {t(label)}
                  {/* What the game has now; Legendary adds its switched-off items apart. */}
                  <small>{catalog ? (key === "misc" ? count.all + count.off : count.all) : "—"}</small>
                  {catalog && key === "legendary" && count.off > 0 && (
                    <small className="items-tab-off" title={tp(count.off, { other: "{count} Street Brawl items the game has switched off" })}>
                      {tp(count.off, { other: "· {count} off" })}
                    </small>
                  )}
                  {catalog && count.replaced > 0 && <i className="items-tab-replaced" title={tp(count.replaced, { one: "{count} replaced", other: "{count} replaced" })} aria-label={tp(count.replaced, { one: "{count} replaced", other: "{count} replaced" })} />}
                </button>
              );
            })}
          </div>
          <div className="extras-search items-search">
            <ExtraIcon name="search" size={17} />
            <input aria-label={t("Search items")} placeholder={t("Search items by new or game name…")} value={query} onChange={(e) => setQuery(e.target.value)} />
            {query && (
              <button aria-label={t("Clear search")} onClick={() => setQuery("")}>
                <ExtraIcon name="close" size={15} />
              </button>
            )}
          </div>
          <button type="button" className="sounds-filter" aria-pressed={replacedOnly} onClick={() => setReplacedOnly(!replacedOnly)}>
            {t("Changed only")}<small>{changedCount}</small>
          </button>
        </div>
        {binding !== null && (
          <div className="sounds-binding">
            <p className="extra-inline-note">{t("Choose an item without a replacement to give it your saved picture.")}</p>
            <button className="extra-text-button" onClick={() => setBinding(null)}>
              {t("Cancel destination change")}
            </button>
          </div>
        )}
        <div id="items-groups" className="items-groups" aria-live="polite">
          {!canLoad && !catalog ? (
            <div className="extra-empty">
              <div className="extra-empty-symbol">
                <ExtraIcon name="image" size={32} />
              </div>
              <h3>{t("Connect your game to get started")}</h3>
              <p>{t("Choose the original game VPK in Setup, then return here to see the game’s items.")}</p>
              {openSetup && (
                <button className="extra-primary" onClick={openSetup}>
                  {t("Open Setup")}
                  <ExtraIcon name="arrow" size={16} />
                </button>
              )}
            </div>
          ) : loading && !catalog ? (
            <div className="extra-empty">
              <span className="extra-spinner" />
              <h3>{t("Reading the game’s items")}</h3>
              <p>{t("Item names, tiers and prices come from your game’s own files.")}</p>
            </div>
          ) : error && !catalog ? (
            <div className="extra-empty">
              <ExtraIcon name="warning" size={32} />
              <h3>{t("The game’s items could not be read")}</h3>
              <p>{error}</p>
              <button className="extra-primary" disabled={loading} onClick={() => load(true)}>
                {t("Try again")}
              </button>
            </div>
          ) : groups.length === 0 ? (
            <div className="extra-empty">
              <ExtraIcon name={searching ? "search" : "image"} size={32} />
              <h3>
                {searching
                  ? t("No matching items")
                  : replacedOnly
                    ? t("No changed {category} items", { category: categoryLabel(category) })
                    : t("No {category} items in your game", { category: categoryLabel(category) })}
              </h3>
              <p>
                {searching
                  ? t("Try another name or clear the search.")
                  : replacedOnly
                    ? t("Turn off Changed only to see every item, then drop in a picture or rename one.")
                    : t("Your game lists none in this category.")}
              </p>
              {(searching || replacedOnly) && (
                <button
                  onClick={() => {
                    setQuery("");
                    setReplacedOnly(false);
                  }}
                >
                  {t("Show all items")}
                </button>
              )}
            </div>
          ) : (
            <>
              {searching && (
                <p className="sounds-shown">
                  {tp(shown.length, { one: "{count} item match “{query}” in every category", other: "{count} items match “{query}” in every category" }, { query: query.trim() })}
                </p>
              )}
              {groups.map((group) => (
                <GroupRow
                  key={group.key}
                  group={group}
                  search={searching}
                  selected={selected}
                  active={active}
                  revision={revision}
                  choose={choose}
                  renamed={renamedOf}
                />
              ))}
            </>
          )}
          {error && catalog && (
            <p className="extra-error" role="alert">
              {error}
            </p>
          )}
        </div>
        {[
          [MISSING_NAMES_TITLE, missingNames],
          [UNLISTED_NAMES_TITLE, unlistedNames],
        ].map(([title, rows]) =>
          (rows as ItemNameRow[]).length ? (
            <section key={title as string} className="items-saved-names" aria-label={t(title as string)}>
              <h3>{t(title as string)}</h3>
              <ul>
                {(rows as ItemNameRow[]).map((row) => (
                  <li key={row.item}>
                    <span>{row.name}</span>
                    <small>{row.item}</small>
                    <button
                      type="button"
                      className="extra-text-button"
                      aria-label={t("Remove the name {name}", { name: row.name })}
                      disabled={disabled}
                      onClick={() => changeNames(withoutItemName(savedItemNames(projectRef.current), row.item), false)}
                    >
                      {t("Remove")}
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ) : null,
        )}
        {catalog?.warnings.length ? (
          <div className="extras-library-footer">
            <ExtraIcon name="warning" size={15} />
            <span>{catalog.warnings.join(" ")}</span>
          </div>
        ) : null}
      </section>
      <aside className="extras-inspector items-inspector" data-category={entry?.category} aria-label={t("Replacement editor")}>
        {selection ? (
          <ReplacementEditor
            key={`items:${selection.key}`}
            item={selection}
            kind="icons"
            previewKind="items"
            eyebrow={t("SELECTED ITEM")}
            title={shownItemLabel(selection)}
            heading={
              <ItemHeading
                gameName={selectionToken ? (names?.names[fold(selectionToken)] ?? selection.entry.label) : selection.entry.label}
                shownName={shownItem(selectionToken, selectionToken ? (names?.names[fold(selectionToken)] ?? selection.entry.label) : selection.entry.label)}
                row={selectionToken ? itemNameOf(nameRows, selectionToken) : null}
                renamable={Boolean(selectionToken && gameHas(selectionToken))}
                status={replacementStatus(selection, active)}
                active={active}
                disabled={disabled}
                save={(name) =>
                  selectionToken ? rename(selectionToken, names?.names[fold(selectionToken)] ?? selection.entry.label, name) : undefined
                }
                useGame={() => selectionToken && changeNames(withoutItemName(savedItemNames(projectRef.current), selectionToken), false)}
              />
            }
            association={t("Item icon · the same for every hero")}
            hero=""
            revision={revision}
            sourceRevision={sourceRevision}
            disabled={disabled}
            active={active}
            full={saved.length >= MAX_ITEM_ICONS}
            edit={(changes) => edit(selection, changes)}
            add={(path) => add(selection, path)}
            remove={() => changeRows(current().filter((_, i) => i !== selection.index), false)}
            chooseDestination={() => {
              setBinding(selection.index);
              setQuery("");
              setReplacedOnly(false);
            }}
          >
            {entry && (
              <div className="items-selected-place">
                <span className="items-chip">
                  <i className="items-category-dot" data-category={entry.category} aria-hidden="true" />
                  {categoryLabel(entry.category)}
                </span>
                <span className="items-chip">{placeText(entry)}</span>
                {entry.switched_off && <span className="items-chip off">{t("Switched off in the game")}</span>}
                {entry.category === "misc" && entry.slot && entry.tier && (
                  <span className="items-chip">
                    {t("Was {category} · tier {tier}", { category: categoryLabel(entry.slot), tier: entry.tier })}
                  </span>
                )}
              </div>
            )}
            {entry?.switched_off && (
              <p className="items-off-note">
                <T text="<strong>Not in the game now.</strong> {reason} Your icon is still built, ready in case the item comes back." values={{ reason: t(SWITCHED_OFF_REASON) }} />
              </p>
            )}
            {entry && entry.names.length > 1 && (
              <p className="items-used-by">
                <span>{t("Also named by")}</span>{" "}
                {otherNames(entry).filter((n, i, all) => all.indexOf(n) === i).join(", ")}
              </p>
            )}
            {names?.found && !(selectionToken && gameHas(selectionToken)) && <p className="items-note">{t(NO_ITEM_NAME_NOTE)}</p>}
            <p className="items-note">{t(ITEM_NOTE)}</p>
          </ReplacementEditor>
        ) : (
          <div className="extra-editor-empty">
            <div className="extra-comparison-illustration" aria-hidden="true">
              <div>
                <ExtraIcon name="image" size={32} />
              </div>
              <ExtraIcon name="arrow" size={24} />
              <div>
                <ExtraIcon name="upload" size={30} />
              </div>
            </div>
            <span className="extras-eyebrow">{t("PREVIEW & REPLACE")}</span>
            <h2>{t("Pick an item")}</h2>
            <p>{t("Choose an item to compare its game icon with your picture.")}</p>
            <ul>
              <li>
                <ExtraIcon name="check" size={16} />
                {t("PNG or JPG · square works best")}
              </li>
              <li>
                <ExtraIcon name="check" size={16} />
                {t("The same icon for every hero")}
              </li>
              <li>
                <ExtraIcon name="check" size={16} />
                {t("Pause or remove changes at any time")}
              </li>
            </ul>
          </div>
        )}
      </aside>
    </div>
  );
}
