import { useEffect, useRef, useState, type ReactNode } from "react";
import type { Project, ResourceReplacement } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { changeCustomization } from "../components/customization";
import { modelEnabled } from "../build/readiness";
import { basename } from "../api";
import { ExtraIcon } from "./Icons";
import { AssetPreview, AssetThumbnail } from "./AssetPreview";
import { ExtraDropzone } from "./Dropzone";
import { HeroColors } from "./HeroColors";
import { colorAdapter } from "./colorAdapter";
import { Sounds } from "./Sounds";
import { Effects, EffectsReplaceBar, type EffectsView } from "./Effects";
import { effectEditCounts, pausedRows, savedRows } from "./effectEdits";
import { Items, keptItemCount } from "./Items";
import { Objects, keptObjectCount, type Operate } from "./Objects";
import { objectCounts, pausedObjectRows, savedObjectRows } from "./objectRows";
import { itemNameCounts, pausedItemNames, savedItemNames } from "./itemNames";
import { clearExtraMediaCache } from "./media";
import { NOT_A_PICTURE, NOT_A_PICTURE_STATUS, isNotAPicture, usePictureChecks } from "./pictureChecks";
import { openTabs } from "./extrasTabs";
import { hasEdit } from "./soundEdit";
import {
  assetItems,
  assetLabel,
  detailNumber,
  duration,
  extraKinds,
  isSilenced,
  replacementStatus,
  specs,
  type AssetItem,
  type ExtraKind,
} from "./models";
import "./extras.css";

export interface ExtrasProps {
  project: Project;
  update: (project: Project) => void;
  disabled: boolean;
  fail: (title: string, error: unknown) => void;
  catalog?: ComponentCatalog | null;
  discover?: () => void | Promise<void>;
  discovering?: boolean;
  canDiscover?: boolean;
  openSetup?: () => void;
  reviewBuild?: () => void;
  /** Opens Prepare model (Colors with a replacement model that is not prepared yet). */
  openPreparation?: () => void;
  /** The app's operation runner (Objects prepares a file with it). */
  operate?: Operate;
  /** Settings → Appearance → Load 3D previews automatically (the Objects view). */
  autoPreviews?: boolean;
}

export function ExtrasWorkspace({
  project,
  update,
  disabled,
  catalog: incoming,
  discover,
  discovering = false,
  canDiscover = true,
  openSetup,
  reviewBuild,
  openPreparation,
  operate,
  autoPreviews = false,
  fail,
}: ExtrasProps) {
  // The tab last open for this project in this session (leaving the page and coming back keeps it).
  const remembered = openTabs.get(project.id);
  const [kind, setKind] = useState<ExtraKind>(() => extraKinds.find((k) => k === remembered) ?? "icons"),
    [colors, setColors] = useState(remembered === "colors"),
    // Extras → Items (Items.tsx): the game's item icons, read only when opened.
    [itemsTab, setItemsTab] = useState(remembered === "items"),
    [, setItemsLoaded] = useState(0),
    // Extras → Objects (Objects.tsx): mounted when first opened, then kept (hidden) so its view never remounts.
    [objectsTab, setObjectsTab] = useState(remembered === "objects"),
    [objectsOpened, setObjectsOpened] = useState(remembered === "objects"),
    [, setObjectsLoaded] = useState(0),
    [selected, setSelected] = useState(""),
    [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all"),
    [page, setPage] = useState(0),
    [revision, setRevision] = useState(0),
    [sourceRevision, setSourceRevision] = useState(0),
    [binding, setBinding] = useState<number | null>(null);
  const attempted = useRef("");
  const projectRef = useRef(project);
  projectRef.current = project;
  // A replacement picture found unreadable shows on its row (replacementStatus).
  usePictureChecks();
  const catalog =
    incoming?.hero_path === project.original_model ? incoming : null;
  const rows = project.customization[kind] ?? [],
    items = assetItems(catalog?.[kind] ?? [], rows),
    spec = specs[kind];
  const heroColors = project.customization.hero_materials?.materials ?? [];
  const itemRows = project.customization.item_icons ?? [];
  const changed =
    extraKinds.reduce(
      (sum, key) => sum + (project.customization[key] ?? []).length,
      0,
    ) +
    heroColors.length +
    itemRows.length +
    itemNameCounts(project).saved +
    effectEditCounts(project).saved +
    objectCounts(project).saved;
  // Colors as the Colors page counts them: hero colors, or with a replacement model its
  // color rows (Surface changes). Shown, and enough to open Build & install; pausing uses `changed`.
  const colorChanges = colorAdapter(project, catalog).changed;
  const shownChanges = changed - heroColors.length + colorChanges;
  const active = Boolean(project.customization.enabled);
  const enabled =
    extraKinds.reduce(
      (sum, key) =>
        sum +
        (project.customization[key] ?? []).filter((r) => r.enabled !== false)
          .length,
      0,
    ) +
    (project.customization.hero_materials?.enabled === false
      ? 0
      : heroColors.filter((r) => r.enabled !== false).length) +
    itemRows.filter((r) => r.enabled !== false).length +
    itemNameCounts(project).enabled +
    effectEditCounts(project).enabled +
    objectCounts(project).enabled;
  // Effects: Edit colors (Effects.tsx) or Replace a file (the shared layout below).
  const [effectsView, setEffectsView] = useState<EffectsView>("colors");
  const effectsColors: boolean = kind === "effects" && effectsView === "colors";
  const tabs = [...extraKinds, "colors", "items", "objects"] as const;
  const shownTab: (typeof tabs)[number] = objectsTab ? "objects" : itemsTab ? "items" : colors ? "colors" : kind;
  // Sounds has its own library (Sounds.tsx); annotated so the shared layout
  // below keeps its sound branches type-checked.
  const soundsTab: boolean = kind === "audio";
  const visible = items.filter(
    (item) =>
      `${assetLabel(item.entry)} ${item.entry.path} ${item.row?.source ?? ""}`
        .toLowerCase()
        .includes(query.toLowerCase()) &&
      (filter !== "changed" || item.row) &&
      (filter !== "available" || item.entry.supported),
  );
  const pageSize = 48,
    pageCount = Math.max(1, Math.ceil(visible.length / pageSize)),
    shownPage = Math.min(page, pageCount - 1);
  const selection = items.find((item) => item.key === selected) ?? null;
  const hero =
    project.hero_name ||
    basename(project.original_model).replace(/\.vmdl(_c)?$/, "");
  // Every asset read needs the hero: without one the page asks for it and loads nothing.
  const noHero = !project.original_model;
  useEffect(() => {
    setSelected("");
    setQuery("");
    setPage(0);
    setBinding(null);
    attempted.current = "";
  }, [project.id, project.original_model]);
  useEffect(() => {
    const key = `${project.id}:${project.original_model}`;
    if (
      !noHero &&
      !catalog &&
      discover &&
      canDiscover &&
      !disabled &&
      !discovering &&
      attempted.current !== key
    ) {
      attempted.current = key;
      void discover();
    }
  }, [
    noHero,
    catalog,
    discover,
    canDiscover,
    disabled,
    discovering,
    project.id,
    project.original_model,
  ]);
  useEffect(() => setPage(0), [query, filter, kind]);
  const current = useRef({ update, kind });
  current.current = { update, kind };
  function changeRows(next: ResourceReplacement[], activate = true) {
    current.current.update(
      changeCustomization(
        projectRef.current,
        { [current.current.kind]: next },
        activate,
      ),
    );
  }
  function edit(item: AssetItem, changes: Partial<ResourceReplacement>) {
    const saved = projectRef.current.customization[kind] ?? [];
    if (item.index >= 0)
      changeRows(
        saved.map((row, index) =>
          index === item.index ? { ...row, ...changes } : row,
        ),
      );
  }
  function addSource(item: AssetItem, source: string) {
    const saved = projectRef.current.customization[kind] ?? [];
    if (item.index >= 0) edit(item, { source });
    else if (saved.length < 128)
      changeRows([
        ...saved,
        {
          enabled: true,
          source,
          target: item.entry.target,
          allow_shared: false,
          ...(item.entry.original_sha256
            ? { original_sha256: item.entry.original_sha256 }
            : {}),
          ...(kind === "icons" ? { fit: "contain" as const } : {}),
          ...(kind === "audio"
            ? { volume_percent: 100, audio_fit: "pad" as const }
            : {}),
        },
      ]);
    setSourceRevision((v) => v + 1);
  }
  function select(item: AssetItem) {
    if (binding !== null && item.entry.supported && !item.row) {
      const saved = projectRef.current.customization[kind] ?? [];
      changeRows(
        saved.map((row, index) =>
          index === binding
            ? {
                ...row,
                target: item.entry.target,
                original_sha256: item.entry.original_sha256 ?? undefined,
                allow_shared: false,
              }
            : row,
        ),
      );
      setBinding(null);
    }
    setSelected(item.key);
  }
  function reload() {
    clearExtraMediaCache();
    setRevision((v) => v + 1);
    void discover?.();
  }
  function tab(next: (typeof tabs)[number]) {
    openTabs.set(project.id, next);
    setColors(next === "colors");
    setItemsTab(next === "items");
    setObjectsTab(next === "objects");
    if (next === "objects") setObjectsOpened(true);
    if (next !== "colors" && next !== "items" && next !== "objects") setKind(next);
    setSelected("");
    setQuery("");
    setPage(0);
    setBinding(null);
  }
  function pauseAll() {
    const value = !enabled;
    const changes: Record<string, unknown> = Object.fromEntries(
      extraKinds.map((key) => [
        key,
        (project.customization[key] ?? []).map((row) => ({
          ...row,
          enabled: value,
        })),
      ]),
    );
    if (project.customization.hero_materials)
      changes.hero_materials = {
        ...project.customization.hero_materials,
        enabled: true,
        materials: heroColors.map((row) => ({ ...row, enabled: value })),
      };
    if (project.customization.item_icons)
      changes.item_icons = itemRows.map((row) => ({ ...row, enabled: value }));
    if (savedItemNames(project).length)
      changes.item_names = pausedItemNames(savedItemNames(project), value);
    if (savedRows(project).length)
      changes.effect_edits = pausedRows(savedRows(project), value);
    if (savedObjectRows(project).length)
      changes.ability_objects = pausedObjectRows(savedObjectRows(project), value);
    update(changeCustomization(project, changes, value));
  }
  return (
    <div className="extras-shell">
      <header className="extras-heading">
        <div>
          <span className="extras-eyebrow">CHARACTER CUSTOMIZATION</span>
          <h1>
            Extras{hero && <span className="extra-hero-chip">{hero}</span>}
          </h1>
          <p>Replace the hero's icons, sounds and effects with your own files, or change its colors.</p>
        </div>
        <div className="extras-heading-actions">
          {shownChanges > 0 && (
            <span className="extra-change-count">
              <span />
              {shownChanges} change{shownChanges === 1 ? "" : "s"}
            </span>
          )}
          <button
            className="extra-secondary"
            onClick={reload}
            disabled={disabled || discovering || !discover || !canDiscover || noHero}
          >
            <ExtraIcon name="refresh" size={17} />
            {discovering
              ? "Loading assets…"
              : catalog
                ? "Reload assets"
                : "Load character assets"}
          </button>
          {reviewBuild && (
            <button
              className="extra-primary"
              disabled={disabled || !(changed || shownChanges)}
              onClick={reviewBuild}
            >
              Build & install
              <ExtraIcon name="arrow" size={17} />
            </button>
          )}
        </div>
      </header>
      <div className="extras-guide">
        <span>
          <b>1</b>Choose an asset
        </span>
        <i />
        <span>
          <b>2</b>Preview & replace
        </span>
        <i />
        <span>
          <b>3</b>Build & install
        </span>
        <small>Save your project to keep your changes</small>
      </div>
      {!active && changed > 0 && (
        <div className="extra-banner">
          <ExtraIcon name="pause" />
          <div>
            <strong>Your saved extras are paused</strong>
            <p>
              Project customization is off. Enable it to include your saved
              changes in the next build.
            </p>
          </div>
          <button
            disabled={disabled}
            onClick={() => update(changeCustomization(project, {}))}
          >
            Enable customization
          </button>
        </div>
      )}
      {noHero ? (
        <section className="extras-library" aria-label="Choose the hero first">
          <div className="extra-empty">
            <div className="extra-empty-symbol">
              <ExtraIcon name="image" size={32} />
            </div>
            <h3>Choose the hero first</h3>
            <p>
              Choose the hero your mod replaces in Character & model, then
              return here to load its icons, sounds, effects and colors.
            </p>
          </div>
        </section>
      ) : (
      <>
      <div
        className="extras-tabs"
        role="tablist"
        aria-label="Extras categories"
      >
        {tabs.map((key, index) => (
          <button
            key={key}
            id={`extras-tab-${key}`}
            role="tab"
            aria-selected={shownTab === key}
            aria-controls={key === "objects" ? "extras-panel-objects" : "extras-panel"}
            tabIndex={shownTab === key ? 0 : -1}
            onClick={() => tab(key)}
            onKeyDown={(event) => {
              const move =
                event.key === "ArrowRight"
                  ? 1
                  : event.key === "ArrowLeft"
                    ? -1
                    : 0;
              if (move || event.key === "Home" || event.key === "End") {
                event.preventDefault();
                const next =
                  event.key === "Home"
                    ? 0
                    : event.key === "End"
                      ? tabs.length - 1
                      : (index + move + tabs.length) % tabs.length;
                tab(tabs[next]);
                document
                  .getElementById(`extras-tab-${tabs[next]}`)
                  ?.focus();
              }
            }}
          >
            {key === "objects" ? (
              <>
                <ExtraIcon name="cube" />
                Objects
                <span>{keptObjectCount(project.original_model) ?? "—"}</span>
                {savedObjectRows(project).length > 0 && <i title="Contains replacements" />}
              </>
            ) : key === "items" ? (
              <>
                <ExtraIcon name="image" />
                Items
                <span>{keptItemCount() ?? "—"}</span>
                {(itemRows.length > 0 || itemNameCounts(project).saved > 0) && <i title="Contains replacements" />}
              </>
            ) : key === "colors" ? (
              <>
                <ExtraIcon name="palette" />
                Colors
                <span>{modelEnabled(project) ? Object.keys(project.materials).length : catalog?.materials?.length ?? "—"}</span>
                {colorChanges > 0 && <i title="Contains color changes" />}
              </>
            ) : (
              <>
                <ExtraIcon name={specs[key].icon} />
                {specs[key].title}
                <span>{catalog?.[key].length ?? "—"}</span>
                {((project.customization[key]?.length ?? 0) > 0 ||
                  (key === "effects" && savedRows(project).length > 0)) && (
                  <i title="Contains replacements" />
                )}
              </>
            )}
          </button>
        ))}
      </div>
      {objectsOpened && (
        <Objects
          project={project}
          update={update}
          disabled={disabled}
          canLoad={canDiscover}
          hero={hero}
          revision={revision}
          shown={objectsTab}
          operate={operate}
          autoPreviews={autoPreviews}
          openSetup={openSetup}
          reviewBuild={reviewBuild}
          fail={fail}
          loaded={() => setObjectsLoaded((v) => v + 1)}
        />
      )}
      {objectsTab ? null : itemsTab ? (
        <Items
          project={project}
          update={update}
          disabled={disabled}
          canLoad={canDiscover}
          openSetup={openSetup}
          revision={revision}
          loaded={() => setItemsLoaded((v) => v + 1)}
        />
      ) : colors ? (
        <HeroColors
          project={project}
          update={update}
          disabled={disabled}
          catalog={catalog}
          discovering={discovering}
          canDiscover={canDiscover && Boolean(discover)}
          reload={reload}
          openSetup={openSetup}
          openPreparation={openPreparation}
          revision={revision}
          heroName={hero}
        />
      ) : soundsTab ? (
        <Sounds
          key={`${project.id}:${project.original_model}`}
          project={project}
          update={update}
          disabled={disabled}
          catalog={catalog}
          discovering={discovering}
          canDiscover={canDiscover}
          canReload={Boolean(discover)}
          reload={reload}
          openSetup={openSetup}
          revision={revision}
          hero={hero}
        />
      ) : effectsColors ? (
        <Effects
          key={`${project.id}:${project.original_model}`}
          project={project}
          update={update}
          disabled={disabled}
          catalog={catalog}
          discovering={discovering}
          canDiscover={canDiscover}
          canReload={Boolean(discover)}
          reload={reload}
          openSetup={openSetup}
          revision={revision}
          hero={hero}
          view={effectsView}
          setView={setEffectsView}
        />
      ) : (
      <>
      {kind === "effects" && (
        <EffectsReplaceBar project={project} revision={revision} view={effectsView} setView={setEffectsView} />
      )}
      <div
        id="extras-panel"
        role="tabpanel"
        aria-labelledby={`extras-tab-${kind}`}
        className="extras-layout"
      >
        <section className="extras-library" aria-label={`${spec.title} assets`}>
          <div className="extras-library-heading">
            <div>
              <h2>{spec.title} library</h2>
              <p>{spec.description}</p>
            </div>
            <span className="extra-library-count">
              {catalog ? `${items.length} assets` : "From your game"}
            </span>
          </div>
          <div className="extras-search">
            <ExtraIcon name="search" size={18} />
            <input
              aria-label={`Search ${spec.title.toLowerCase()}`}
              placeholder={`Search ${spec.title.toLowerCase()}…`}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            {query && (
              <button aria-label="Clear search" onClick={() => setQuery("")}>
                <ExtraIcon name="close" size={16} />
              </button>
            )}
          </div>
          <div className="extras-filters">
            <div role="group" aria-label="Asset filter">
              {[
                ["all", "All assets"],
                ["changed", `Changed (${rows.length})`],
                ["available", "Replaceable"],
              ].map(([key, label]) => (
                <button
                  key={key}
                  aria-pressed={filter === key}
                  onClick={() => setFilter(key)}
                >
                  {label}
                </button>
              ))}
            </div>
            {binding !== null && (
              <button
                className="extra-text-button"
                onClick={() => setBinding(null)}
              >
                Cancel destination change
              </button>
            )}
          </div>
          {binding !== null && (
            <p className="extra-inline-note">
              Choose a replaceable asset without an existing replacement to
              assign your saved file.
            </p>
          )}
          {discovering ? (
            <div className="extra-empty">
              <span className="extra-spinner" />
              <h3>Finding {hero}’s assets</h3>
              <p>
                Reading icons, sound files and effects from your game archive.
              </p>
            </div>
          ) : !catalog && filter !== "changed" ? (
            <div className="extra-empty">
              <div className="extra-empty-symbol">
                <ExtraIcon name={spec.icon} size={32} />
              </div>
              <h3>
                {canDiscover
                  ? "Load your character’s assets"
                  : "Connect your game to get started"}
              </h3>
              <p>
                {canDiscover
                  ? "Browse the original assets before choosing what to replace. Your saved replacements stay in Changed."
                  : "Choose the original game VPK in Setup, then return here to load this character’s icons, sounds and effects."}
              </p>
              {canDiscover ? (
                <button
                  className="extra-primary"
                  onClick={reload}
                  disabled={disabled || !discover}
                >
                  Load character assets
                  <ExtraIcon name="arrow" size={16} />
                </button>
              ) : (
                openSetup && (
                  <button className="extra-primary" onClick={openSetup}>
                    Open Setup
                    <ExtraIcon name="arrow" size={16} />
                  </button>
                )
              )}
            </div>
          ) : visible.length === 0 ? (
            <div className="extra-empty">
              <ExtraIcon name={query ? "search" : spec.icon} size={32} />
              <h3>
                {query
                  ? "No matching assets"
                  : filter === "changed"
                    ? "No replacements yet"
                    : "No assets found in this category"}
              </h3>
              <p>
                {query
                  ? "Try another name or clear your filters."
                  : filter === "changed"
                    ? "Select an original asset in All assets, then drop in a replacement."
                    : "Discovery uses the selected model, known character aliases and linked game resources."}
              </p>
              {(query || filter !== "all") && (
                <button
                  onClick={() => {
                    setQuery("");
                    setFilter("all");
                  }}
                >
                  Show all assets
                </button>
              )}
            </div>
          ) : (
            <div
              className={`extras-assets ${kind === "icons" ? "extras-icon-grid" : "extras-asset-list"}`}
              role="list"
            >
              {visible
                .slice(shownPage * pageSize, (shownPage + 1) * pageSize)
                .map((item) => {
                  const status = replacementStatus(item, active),
                    seconds = detailNumber(item.entry, "duration");
                  return (
                    <div role="listitem" key={item.key}>
                      <button
                        className={`extras-asset ${selected === item.key ? "selected" : ""} ${item.row ? "has-replacement" : ""}`}
                        aria-pressed={selected === item.key}
                        onClick={() => select(item)}
                      >
                        {kind === "icons" && !item.missing ? (
                          <AssetThumbnail
                            hero={project.original_model}
                            entry={item.entry}
                            revision={revision}
                          />
                        ) : (
                          <div className={`extra-asset-symbol ${kind}`}>
                            <ExtraIcon
                              name={spec.icon}
                              size={kind === "icons" ? 30 : 21}
                            />
                          </div>
                        )}
                        <div className="extra-asset-name">
                          <strong>{assetLabel(item.entry)}</strong>
                          <span>
                            {item.missing
                              ? "Saved destination unavailable"
                              : kind === "icons"
                                ? `${item.entry.width ?? "—"} × ${item.entry.height ?? "—"}`
                                : kind === "audio" && seconds !== null
                                  ? `${duration(seconds)} · ${detailNumber(item.entry, "channels") === 2 ? "Stereo" : "Mono"}`
                                  : item.entry.path.split("/").slice(-2, -1)[0]}
                          </span>
                        </div>
                        <span
                          className={`extra-asset-state ${status === "Replaced" ? "changed" : status === "Needs attention" || status === NOT_A_PICTURE_STATUS ? "attention" : ""}`}
                        >
                          {item.row ? (
                            <>
                              <ExtraIcon
                                name={
                                  status === "Replaced"
                                    ? "check"
                                    : status === "Paused"
                                      ? "pause"
                                      : "warning"
                                }
                                size={13}
                              />
                              {status}
                            </>
                          ) : !item.entry.supported ? (
                            "View only"
                          ) : item.entry.shared ? (
                            "Shared"
                          ) : (
                            ""
                          )}
                        </span>
                      </button>
                    </div>
                  );
                })}
            </div>
          )}
          {visible.length > pageSize && (
            <div className="extras-pagination">
              <button
                aria-label="Previous assets page"
                disabled={shownPage === 0}
                onClick={() => setPage(shownPage - 1)}
              >
                Previous
              </button>
              <span>
                Page {shownPage + 1} of {pageCount}
              </span>
              <button
                aria-label="Next assets page"
                disabled={shownPage + 1 >= pageCount}
                onClick={() => setPage(shownPage + 1)}
              >
                Next
              </button>
            </div>
          )}
          {catalog && (
            <div className="extras-library-footer">
              <ExtraIcon name="folder" size={15} />
              <span>Assets for {hero} · shared resources are labeled</span>
            </div>
          )}
        </section>
        <aside className="extras-inspector" aria-label="Replacement editor">
          {selection ? (
            <ReplacementEditor
              key={`${kind}:${selection.key}`}
              item={selection}
              kind={kind}
              hero={project.original_model}
              revision={revision}
              sourceRevision={sourceRevision}
              disabled={disabled}
              active={active}
              full={rows.length >= 128}
              edit={(changes) => edit(selection, changes)}
              add={(path) => addSource(selection, path)}
              remove={() =>
                changeRows(
                  rows.filter((_, i) => i !== selection.index),
                  false,
                )
              }
              chooseDestination={() => {
                setBinding(selection.index);
                setFilter("all");
                setQuery("");
              }}
            />
          ) : (
            <div className="extra-editor-empty">
              <div className="extra-comparison-illustration" aria-hidden="true">
                <div>
                  <ExtraIcon name={spec.icon} size={32} />
                </div>
                <ExtraIcon name="arrow" size={24} />
                <div>
                  <ExtraIcon name="upload" size={30} />
                </div>
              </div>
              <span className="extras-eyebrow">PREVIEW & REPLACE</span>
              <h2>Start with the original</h2>
              <p>
                Choose{" "}
                {kind === "icons"
                  ? "an icon"
                  : kind === "audio"
                    ? "a sound"
                    : "an effect"}{" "}
                from the library to compare it with your replacement.
              </p>
              <ul>
                <li>
                  <ExtraIcon name="check" size={16} />
                  Preview before you change anything
                </li>
                <li>
                  <ExtraIcon name="check" size={16} />
                  Drop a file or browse your computer
                </li>
                <li>
                  <ExtraIcon name="check" size={16} />
                  {kind === "audio"
                    ? "Listen and adjust volume from 0–200%"
                    : "Pause or remove changes at any time"}
                </li>
              </ul>
            </div>
          )}
        </aside>
      </div>
      </>
      )}
      </>
      )}
      {catalog?.warnings.length ? (
        <details className="extras-discovery-notes">
          <summary>
            <ExtraIcon name="warning" size={16} />
            Discovery notes ({catalog.warnings.length})
          </summary>
          {catalog.warnings.map((warning, index) => (
            <p key={index}>{warning}</p>
          ))}
        </details>
      ) : null}
      <footer className="extras-bottom-bar">
        <div>
          <ExtraIcon name="check" size={17} />
          <span>
            <strong>
              {active ? enabled : 0} active change
              {(active ? enabled : 0) === 1 ? "" : "s"}
            </strong>
            <small>
              {modelEnabled(project) && !project.dmx
                ? "No replacement model is chosen yet: add one, or untick Include replacement model and textures in Build options to build only the extras"
                : modelEnabled(project)
                ? colorChanges > 0
                  // The header counts these (the replacement's color rows); Pause all never touches them.
                  ? `Your replacement model stays included, with its ${colorChanges} color change${colorChanges === 1 ? "" : "s"}, which Pause all extras does not pause`
                  : "Your replacement model stays included"
                : "Extras-only project · original model stays included"}
            </small>
          </span>
        </div>
        <div>
          {changed > 0 && (
            <button
              className="extra-text-button"
              disabled={disabled}
              onClick={pauseAll}
            >
              {enabled ? "Pause all extras" : "Resume all extras"}
            </button>
          )}
          <details className="extras-settings">
            <summary>
              <ExtraIcon name="settings" size={17} />
              Build options
            </summary>
            <div>
              <label>
                <input
                  type="checkbox"
                  checked={active}
                  disabled={disabled}
                  onChange={(e) =>
                    update(
                      changeCustomization(
                        project,
                        {
                          enabled: e.target.checked,
                          ...(!e.target.checked ? { model_enabled: true } : {}),
                        },
                        false,
                      ),
                    )
                  }
                />
                Enable project customization
              </label>
              <p>
                This shared setting also controls saved physics, surface and
                equipment changes.
              </p>
              <label>
                <input
                  type="checkbox"
                  checked={project.customization.model_enabled ?? true}
                  disabled={disabled}
                  onChange={(e) =>
                    update(
                      changeCustomization(
                        project,
                        { model_enabled: e.target.checked },
                        !e.target.checked || active,
                      ),
                    )
                  }
                />
                Include replacement model and textures
              </label>
            </div>
          </details>
        </div>
      </footer>
    </div>
  );
}

/** A replacement picture whose file does not read as a picture: said plainly, the cause under it.
 * Other preview failures keep the usual "Preview unavailable". */
function pictureError(error: string) {
  if (!isNotAPicture(error)) return null;
  return (
    <div className="extra-preview-placeholder extra-error" role="status">
      <ExtraIcon name="warning" size={26} />
      <strong>{NOT_A_PICTURE}</strong>
      <p>{error}</p>
    </div>
  );
}
export function ReplacementEditor({
  item,
  kind,
  hero,
  revision,
  sourceRevision,
  disabled,
  active,
  full,
  edit,
  add,
  remove,
  chooseDestination,
  children,
  previewKind,
  eyebrow,
  title,
  association,
  silence,
  silenceRefusal = "",
  soundEdit,
  previewError,
  heading,
}: {
  /** Items only: its own heading (the item's name with Rename) in place of the eyebrow, title and status. */
  heading?: ReactNode;
  /** Sounds only: the way into the sound editor; an edited sound shows only it, not the old volume and timing. */
  soundEdit?: ReactNode;
  /** Sounds only: shown instead of a replacement preview error when it returns something. */
  previewError?: (error: string) => ReactNode;
  /** Sounds only: "Silence this sound" (a row without a file). Others leave it out. */
  silence?: () => void;
  /** Why this sound cannot be silenced ("" when it can). */
  silenceRefusal?: string;
  /** The heading as given (item names keep their hyphens). */
  title?: string;
  /** Shown under the heading (Sounds: tags, variation picker, Replace all). */
  children?: ReactNode;
  /** Previews of another catalog than the hero's (Items: the game's item pictures). */
  previewKind?: "items";
  /** Replaces "SELECTED ICON" and the like. */
  eyebrow?: string;
  /** Replaces the Association line of File details. */
  association?: string;
  item: AssetItem;
  kind: ExtraKind;
  hero: string;
  revision: number;
  sourceRevision: number;
  disabled: boolean;
  active: boolean;
  full: boolean;
  edit: (changes: Partial<ResourceReplacement>) => void;
  add: (path: string) => void;
  remove: () => void;
  chooseDestination: () => void;
}) {
  usePictureChecks();
  const { entry, row } = item,
    spec = specs[kind],
    volume = row?.volume_percent ?? 100,
    source = row?.source ?? "",
    [volumeDraft, setVolumeDraft] = useState(String(volume));
  useEffect(() => setVolumeDraft(String(volume)), [volume]);
  const request = {
    hero_path: hero,
    target: entry.target,
    kind: previewKind ?? kind,
    max_dimension: 512,
  };
  const identity = `${entry.original_sha256 ?? ""}:${revision}`;
  const status = replacementStatus(item, active),
    length = detailNumber(entry, "duration"),
    previewOnly = !entry.supported && !item.missing,
    silenced = kind === "audio" && isSilenced(row),
    soundEdited = kind === "audio" && !silenced && hasEdit(row);
  const setVolume = (value: number) => {
    if (Number.isFinite(value))
      edit({ volume_percent: Math.min(200, Math.max(0, Math.round(value))) });
  };
  return (
    <>
      {heading ?? (
      <header className="extra-inspector-heading">
        <div>
          <span className="extras-eyebrow">
            {eyebrow ?? `SELECTED ${spec.singular.toUpperCase()}`}
          </span>
          <h2>{title ?? assetLabel(entry)}</h2>
        </div>
        <span className={`extra-pill ${status === "Replaced" || status === "Silenced" ? "ready" : ""}`}>
          {status}
        </span>
      </header>
      )}
      {children}
      {item.missing ? (
        <div className="extra-editor-notice">
          <ExtraIcon name="warning" />
          <strong>Choose an available game destination</strong>
          <p>
            {silenced
              ? "This silenced sound is not in the loaded catalog. It stays silenced once you choose where it goes."
              : "This saved replacement is not in the loaded catalog. Your source file is retained."}
          </p>
          <button disabled={disabled} onClick={chooseDestination}>
            Choose destination
          </button>
        </div>
      ) : (
        <>
          <div
            className={`extra-comparison ${kind === "audio" ? "audio" : ""} ${previewOnly && !row ? "original-only" : ""}`}
          >
            <section>
              <div className="extra-preview-label">
                <span>Original</span>
                <small>
                  {kind === "icons"
                    ? `${entry.width ?? "—"} × ${entry.height ?? "—"}`
                    : length !== null
                      ? `${length.toFixed(2)}s`
                      : ""}
                </small>
              </div>
              <AssetPreview
                request={request}
                identity={identity}
                kind={kind}
                label="Original"
                disabled={disabled}
              />
            </section>
            {(!previewOnly || row) && <section>
              <div className="extra-preview-label">
                <span>Your replacement</span>
                {row && (
                  <small>
                    {silenced ? "Silenced" : soundEdited ? "Edited" : kind === "audio" ? `${volume}% volume` : "Preview"}
                  </small>
                )}
              </div>
              {silenced ? (
                <div className="extra-media extra-media-audio">
                  <div className="extra-preview-placeholder extra-silenced" role="status">
                    <ExtraIcon name="volume" size={34} />
                    <span>Silenced: the game plays nothing for this sound.</span>
                  </div>
                </div>
              ) : soundEdited ? (
                <div className="extra-media extra-media-audio">
                  <div className="extra-preview-placeholder" role="status">
                    <ExtraIcon name="volume" size={34} />
                    <span>Edited: listen to exactly what the game will play in the editor.</span>
                  </div>
                </div>
              ) : <AssetPreview
                request={
                  source
                    ? {
                        ...request,
                        source,
                        fit: row?.fit ?? "contain",
                        audio_fit: row?.audio_fit ?? "exact",
                      }
                    : null
                }
                identity={`${identity}:${sourceRevision}`}
                kind={kind}
                label="Replacement"
                errorView={previewError ?? (kind === "icons" ? pictureError : undefined)}
                volume={volume}
                disabled={disabled}
              />}
            </section>}
          </div>
          {kind === "effects" && (
            <p className="extra-inline-note">
              This viewer shows particle structure. Check the animated effect in
              game after building.
            </p>
          )}
          {previewOnly && !row && (
            <p className="extra-original-kept">
              <strong>Original kept</strong>
              No action needed. Replacement is not available for this {spec.singular}.
            </p>
          )}
          {previewOnly && row && (
            <div className="extra-editor-notice">
              <ExtraIcon name="warning" />
              <strong>This saved replacement is unavailable</strong>
              <p>
                Remove it or turn off “Include in build” below to keep using
                the original {spec.singular}.
              </p>
            </div>
          )}
        </>
      )}
      <div className="extra-edit-controls">
        {entry.supported && <ExtraDropzone
          kind={kind}
          source={source}
          disabled={
            disabled || item.missing || !entry.supported || (!row && full)
          }
          change={add}
        />}
        {entry.supported && !row && full && (
          <p className="extra-error">
            This category already has 128 replacements. Remove one before adding
            another.
          </p>
        )}
        {silence && entry.supported && !item.missing && !silenced && (
          <div className="extra-silence">
            <button
              type="button"
              className="extra-secondary"
              disabled={disabled || (!row && full) || Boolean(silenceRefusal)}
              onClick={silence}
            >
              <ExtraIcon name="volume" size={16} />
              Silence this sound
            </button>
            <small>
              {silenceRefusal ||
                (row
                  ? "Instead of your file: the game plays nothing for this sound."
                  : "No file needed: the game plays nothing for this sound.")}
            </small>
          </div>
        )}
        {entry.supported && row && kind === "icons" && (
          <fieldset className="extra-image-fit">
            <legend>Image fit</legend>
            <div>
              {[
                ["contain", "Fit inside", "Keep the full image"],
                ["cover", "Fill & crop", "Fill the original frame"],
                ["stretch", "Stretch", "Match its dimensions"],
              ].map(([value, label, hint]) => (
                <button
                  key={value}
                  type="button"
                  aria-pressed={(row.fit ?? "contain") === value}
                  disabled={disabled}
                  onClick={() =>
                    edit({ fit: value as ResourceReplacement["fit"] })
                  }
                >
                  <strong>{label}</strong>
                  <small>{hint}</small>
                </button>
              ))}
            </div>
          </fieldset>
        )}
        {entry.supported && row && kind === "audio" && !silenced && soundEdit}
        {entry.supported && row && kind === "audio" && !silenced && !(soundEdit && soundEdited) && (
          <div className="extra-volume">
            <div className="extra-volume-heading">
              <label htmlFor="replacement-volume">
                <ExtraIcon name="volume" size={18} />
                Replacement volume
              </label>
              <div>
                <input
                  aria-label="Replacement volume percent"
                  type="number"
                  min="0"
                  max="200"
                  step="1"
                  value={volumeDraft}
                  disabled={disabled}
                  onChange={(e) => {
                    setVolumeDraft(e.target.value);
                    if (
                      e.target.value !== "" &&
                      Number(e.target.value) >= 0 &&
                      Number(e.target.value) <= 200
                    )
                      setVolume(Number(e.target.value));
                  }}
                  onBlur={() => {
                    const value =
                      volumeDraft === "" ? 100 : Number(volumeDraft);
                    setVolume(value);
                    setVolumeDraft(
                      String(
                        Math.max(
                          0,
                          Math.min(
                            200,
                            Math.round(Number.isFinite(value) ? value : 100),
                          ),
                        ),
                      ),
                    );
                  }}
                />
                <span>%</span>
                <button
                  className="extra-text-button"
                  disabled={disabled || volume === 100}
                  onClick={() => setVolume(100)}
                >
                  Reset
                </button>
              </div>
            </div>
            <input
              id="replacement-volume"
              type="range"
              min="0"
              max="200"
              step="1"
              value={volume}
              disabled={disabled}
              onChange={(e) => setVolume(Number(e.target.value))}
            />
            <div className="extra-volume-scale">
              <span>0% · Silent</span>
              <span>100% · Original level</span>
              <span>200% · 2×</span>
            </div>
            <p>Applied to your replacement preview and the built mod.</p>
            <div className="extra-audio-timing">
              <label htmlFor="extra-audio-fit">Sound timing</label>
              <select
                id="extra-audio-fit"
                value={row.audio_fit ?? "exact"}
                disabled={disabled}
                onChange={(e) =>
                  edit({
                    audio_fit: e.target
                      .value as ResourceReplacement["audio_fit"],
                  })
                }
              >
                <option value="pad">
                  Match original timing · pad shorter audio
                </option>
                <option value="trim">
                  Match original timing · allow trimming longer audio
                </option>
                <option value="exact">Exact source format and length</option>
              </select>
              <small>
                Preserves the original game timing and loop points.{" "}
                {length !== null && `Target: ${length.toFixed(2)} seconds.`}
              </small>
            </div>
          </div>
        )}
        {entry.shared && (entry.supported || row) && (
          <div className="extra-shared">
            <ExtraIcon name="warning" size={18} />
            <div>
              <strong>Shared game resource</strong>
              <p>
                Replacing this asset can also change other characters that use
                it.
              </p>
              {row && (
                <label>
                  <input
                    type="checkbox"
                    checked={row.allow_shared ?? false}
                    disabled={disabled}
                    onChange={(e) => edit({ allow_shared: e.target.checked })}
                  />
                  Include this shared replacement
                </label>
              )}
            </div>
          </div>
        )}
        {row && (
          <div className="extra-replacement-actions">
            <label className="extra-toggle">
              <input
                type="checkbox"
                checked={row.enabled !== false}
                disabled={disabled}
                onChange={(e) => edit({ enabled: e.target.checked })}
              />
              <span aria-hidden="true" />
              <strong>Include in build</strong>
            </label>
            <button
              className="extra-remove"
              disabled={disabled}
              onClick={remove}
            >
              <ExtraIcon name="close" size={15} />
              {silenced ? "Remove silence" : "Remove replacement"}
            </button>
          </div>
        )}
        <details className="extra-file-details">
          <summary>
            File details
            <ExtraIcon name="folder" size={15} />
          </summary>
          <dl>
            {previewOnly && <>
              <dt>Why replacement is unavailable</dt>
              <dd>
                {(entry.reason || "This format is not supported for replacement yet.")
                  .replace(/^Integrity check failed:\s*/, "")}
              </dd>
            </>}
            <dt>Game destination</dt>
            <dd>{entry.target || "Not assigned"}</dd>
            {source && (
              <>
                <dt>Replacement source</dt>
                <dd>{source}</dd>
              </>
            )}
            {silenced && (
              <>
                <dt>Replacement</dt>
                <dd>Silence (no file)</dd>
              </>
            )}
            <dt>Association</dt>
            <dd>
              {association ?? (entry.association === "other_hero_voice"
                ? "Another hero’s voice line about this hero · shared"
                : entry.shared
                  ? "Linked resource · potentially shared"
                  : "Selected character’s assets")}
            </dd>
          </dl>
        </details>
      </div>
    </>
  );
}
