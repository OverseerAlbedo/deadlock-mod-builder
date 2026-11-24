// Extras → Items: the shop's item pictures, the same for every hero, grouped
// like the in-game shop (Weapon, Vitality, Spirit, then tiers with their price),
// plus Legendary (Street Brawl) and Misc. The catalog is read only when this
// tab opens, and pictures load only for the tiles of the open category.
import { useEffect, useMemo, useRef, useState } from "react";
import type { Project, ResourceReplacement } from "../models";
import { api } from "../api";
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
import { ItemHeading } from "./ItemName";
import "./items.css";

const NO_ROWS: ResourceReplacement[] = [];
const NO_NAMES: ItemNameRow[] = [];
export const MISSING_NAMES_TITLE = "Saved names for items the game no longer has";
export const UNLISTED_NAMES_TITLE = "Saved names for items this list does not show";
const fold = (value: string) => value.toLowerCase();
const words = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
/** The item record a tile's rename belongs to: its owner (the record whose name the tile shows). */
export function nameToken(item: AssetItem): string | null {
  return itemOf(item)?.items[0] ?? null;
}
export const ITEM_NOTE =
  "Item icons are the same for every hero, so this change shows for all heroes while the mod is installed.";
/** Under a picture the game's names file names no item for (Misc), once the names were read. */
export const NO_ITEM_NAME_NOTE = "This picture has no item name in the shop, so only its picture can be replaced.";

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
  const status = replacementStatus(item, active);
  const off = Boolean(entry?.switched_off);
  const state = `${off ? ", switched off in the game" : ""}${item.row ? `, ${status.toLowerCase()}` : ""}`;
  const also = entry && entry.names.length > 1 ? ` · also ${entry.names.slice(1).join(", ")}` : "";
  const name = renamed === null ? item.entry.label : `${renamed}, renamed from ${item.entry.label}`;
  const tip = renamed === null ? item.entry.label : `${renamed} · renamed · in the game: ${item.entry.label}`;
  return (
    <div role="listitem" ref={ref}>
      <button
        type="button"
        className={`items-tile ${selected ? "selected" : ""} ${item.row ? "replaced" : ""} ${off ? "off" : ""} ${renamed === null ? "" : "renamed"}`}
        data-category={entry?.category ?? "misc"}
        aria-pressed={selected}
        aria-label={`${name}${state}`}
        title={`${tip}${also}${off ? ` · ${SWITCHED_OFF_REASON}` : ""}`}
        onClick={choose}
      >
        <span className={`items-picture ${loading ? "loading" : ""}`}>
          {url ? <img src={url} alt="" /> : <ExtraIcon name={error || item.missing ? "warning" : "image"} size={22} />}
          {item.row && <i className={`items-dot ${status === "Replaced" ? "" : "attention"}`} aria-hidden="true" />}
        </span>
        {renamed === null ? (
          <span className="items-name">{item.entry.label}</span>
        ) : (
          <>
            <span className="items-name">
              <span className="items-name-mark" aria-hidden="true">
                <ExtraIcon name="pencil" size={11} />
              </span>
              {renamed}
            </span>
            <span className="items-was">{item.entry.label}</span>
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
  return (
    <section className={`items-tier ${search ? "search" : ""}`} data-category={group.category} aria-label={`${search ? `${categoryLabel(group.category)} · ` : ""}${group.title}`}>
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
            <b>{souls(group.price)}</b> souls
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
  const shown = useMemo(() => filterItems(items, query, replacedOnly, renamedOf), [items, query, replacedOnly, renamedOf]);
  const changedCount = useMemo(() => items.filter((i) => i.row || renamedOf(i) !== null).length, [items, renamedOf]);
  // Whether the game's names file names an item record (only then can it be renamed).
  const gameHas = (token: string) => Boolean(names?.found && Object.prototype.hasOwnProperty.call(names.names, fold(token)));
  // Saved names the game does not have now, and names the game has that no tile here carries.
  const shownTokens = useMemo(() => new Set(items.map(nameToken).filter((t): t is string => t !== null).map(fold)), [items]);
  const missingNames = names?.found ? nameRows.filter((r) => !gameHas(r.item)) : [];
  const unlistedNames = names?.found && catalog ? nameRows.filter((r) => gameHas(r.item) && !shownTokens.has(fold(r.item))) : [];
  const total = catalog ? catalog.items.filter((i) => i.category !== "misc" && !i.switched_off).length : 0;
  // Misc shows only when it has something (rows of the catalog or saved pictures).
  const shownCategories = ITEM_CATEGORIES.filter(({ key }) => key !== "misc" || !catalog || counts.get("misc")!.rows > 0);
  const openCategory: ItemCategory = shownCategories.some((c) => c.key === category) ? category : "weapon";
  const groups = useMemo(
    () => itemGroups(binding === null ? shown : shown.filter((i) => !i.row && !i.missing && i.entry.supported), searching ? null : openCategory),
    [shown, searching, openCategory, binding],
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
  /** Save a checked name for an item record: the game's own name removes the rename. */
  function rename(token: string, gameName: string, name: string): string | undefined {
    const rows = savedItemNames(projectRef.current);
    if (name === gameName) {
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
      <section className="extras-library items-library" aria-label="Item icons">
        <div className="extras-library-heading">
          <div>
            <h2>Items</h2>
            <p>
              Replace the icons of the shop’s items with your own pictures, or rename them
              {catalog ? ` · ${total.toLocaleString("en-US")} items from your game` : ""} · icons load when you open this page
            </p>
          </div>
          <div className="items-heading-actions">
            {saved.length > 0 && <span className="sounds-replaced-count">{words(saved.length, "new icon", "new icons")}</span>}
            {nameRows.length > 0 && <span className="sounds-replaced-count">{nameRows.length} renamed</span>}
            {(saved.length > 0 || nameRows.length > 0) &&
              (confirmReset ? (
                <span className="items-confirm" role="group" aria-label="Remove every item icon and name">
                  <span>
                    Remove{" "}
                    {[saved.length ? words(saved.length, "new icon", "new icons") : "", nameRows.length ? words(nameRows.length, "name", "names") : ""]
                      .filter(Boolean)
                      .join(" and ")}
                    ?
                  </span>
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
                    Remove
                  </button>
                  <button type="button" className="extra-text-button" onClick={() => setConfirmReset(false)}>
                    Keep
                  </button>
                </span>
              ) : (
                <button type="button" className="extra-text-button" disabled={disabled} onClick={() => setConfirmReset(true)}>
                  Remove all…
                </button>
              ))}
          </div>
        </div>
        <div className="items-toolbar">
          <div className="items-categories" role="tablist" aria-label="Item categories">
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
                  {label}
                  {/* What the game has now; Legendary adds its switched-off items apart. */}
                  <small>{catalog ? (key === "misc" ? count.all + count.off : count.all) : "—"}</small>
                  {catalog && key === "legendary" && count.off > 0 && (
                    <small className="items-tab-off" title={`${count.off} Street Brawl items the game has switched off`}>
                      · {count.off} off
                    </small>
                  )}
                  {catalog && count.replaced > 0 && <i className="items-tab-replaced" title={`${count.replaced} replaced`} aria-label={`${count.replaced} replaced`} />}
                </button>
              );
            })}
          </div>
          <div className="extras-search items-search">
            <ExtraIcon name="search" size={17} />
            <input aria-label="Search items" placeholder="Search items by new or game name…" value={query} onChange={(e) => setQuery(e.target.value)} />
            {query && (
              <button aria-label="Clear search" onClick={() => setQuery("")}>
                <ExtraIcon name="close" size={15} />
              </button>
            )}
          </div>
          <button type="button" className="sounds-filter" aria-pressed={replacedOnly} onClick={() => setReplacedOnly(!replacedOnly)}>
            Changed only<small>{changedCount}</small>
          </button>
        </div>
        {binding !== null && (
          <div className="sounds-binding">
            <p className="extra-inline-note">Choose an item without a replacement to give it your saved picture.</p>
            <button className="extra-text-button" onClick={() => setBinding(null)}>
              Cancel destination change
            </button>
          </div>
        )}
        <div id="items-groups" className="items-groups" aria-live="polite">
          {!canLoad && !catalog ? (
            <div className="extra-empty">
              <div className="extra-empty-symbol">
                <ExtraIcon name="image" size={32} />
              </div>
              <h3>Connect your game to get started</h3>
              <p>Choose the original game VPK in Setup, then return here to see the game’s items.</p>
              {openSetup && (
                <button className="extra-primary" onClick={openSetup}>
                  Open Setup
                  <ExtraIcon name="arrow" size={16} />
                </button>
              )}
            </div>
          ) : loading && !catalog ? (
            <div className="extra-empty">
              <span className="extra-spinner" />
              <h3>Reading the game’s items</h3>
              <p>Item names, tiers and prices come from your game’s own files.</p>
            </div>
          ) : error && !catalog ? (
            <div className="extra-empty">
              <ExtraIcon name="warning" size={32} />
              <h3>The game’s items could not be read</h3>
              <p>{error}</p>
              <button className="extra-primary" disabled={loading} onClick={() => load(true)}>
                Try again
              </button>
            </div>
          ) : groups.length === 0 ? (
            <div className="extra-empty">
              <ExtraIcon name={searching ? "search" : "image"} size={32} />
              <h3>
                {searching
                  ? "No matching items"
                  : replacedOnly
                    ? `No changed ${categoryLabel(category)} items`
                    : `No ${categoryLabel(category)} items in your game`}
              </h3>
              <p>
                {searching
                  ? "Try another name or clear the search."
                  : replacedOnly
                    ? "Turn off Changed only to see every item, then drop in a picture or rename one."
                    : "Your game lists none in this category."}
              </p>
              {(searching || replacedOnly) && (
                <button
                  onClick={() => {
                    setQuery("");
                    setReplacedOnly(false);
                  }}
                >
                  Show all items
                </button>
              )}
            </div>
          ) : (
            <>
              {searching && (
                <p className="sounds-shown">
                  {shown.length} {shown.length === 1 ? "item" : "items"} match “{query.trim()}” in every category
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
            <section key={title as string} className="items-saved-names" aria-label={title as string}>
              <h3>{title as string}</h3>
              <ul>
                {(rows as ItemNameRow[]).map((row) => (
                  <li key={row.item}>
                    <span>{row.name}</span>
                    <small>{row.item}</small>
                    <button
                      type="button"
                      className="extra-text-button"
                      aria-label={`Remove the name ${row.name}`}
                      disabled={disabled}
                      onClick={() => changeNames(withoutItemName(savedItemNames(projectRef.current), row.item), false)}
                    >
                      Remove
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
      <aside className="extras-inspector items-inspector" data-category={entry?.category} aria-label="Replacement editor">
        {selection ? (
          <ReplacementEditor
            key={`items:${selection.key}`}
            item={selection}
            kind="icons"
            previewKind="items"
            eyebrow="SELECTED ITEM"
            title={selection.entry.label}
            heading={
              <ItemHeading
                gameName={selectionToken ? (names?.names[fold(selectionToken)] ?? selection.entry.label) : selection.entry.label}
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
            association="Item icon · the same for every hero"
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
                {entry.switched_off && <span className="items-chip off">Switched off in the game</span>}
                {entry.category === "misc" && entry.slot && entry.tier && (
                  <span className="items-chip">
                    Was {categoryLabel(entry.slot)} · tier {entry.tier}
                  </span>
                )}
              </div>
            )}
            {entry?.switched_off && (
              <p className="items-off-note">
                <strong>Not in the game now.</strong> {SWITCHED_OFF_REASON} Your icon is still built, ready in case the item comes back.
              </p>
            )}
            {entry && entry.names.length > 1 && (
              <p className="items-used-by">
                <span>Also named by</span>{" "}
                {entry.names.slice(1).filter((n, i, all) => all.indexOf(n) === i).join(", ")}
              </p>
            )}
            {names?.found && !(selectionToken && gameHas(selectionToken)) && <p className="items-note">{NO_ITEM_NAME_NOTE}</p>}
            <p className="items-note">{ITEM_NOTE}</p>
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
            <span className="extras-eyebrow">PREVIEW & REPLACE</span>
            <h2>Pick an item</h2>
            <p>Choose an item to compare its game icon with your picture.</p>
            <ul>
              <li>
                <ExtraIcon name="check" size={16} />
                PNG or JPG · square works best
              </li>
              <li>
                <ExtraIcon name="check" size={16} />
                The same icon for every hero
              </li>
              <li>
                <ExtraIcon name="check" size={16} />
                Pause or remove changes at any time
              </li>
            </ul>
          </div>
        )}
      </aside>
    </div>
  );
}
