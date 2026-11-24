/** The Colors page's material list: All / Changed (and Weapon when the hero's weapon is known), search, groups with small headings, and rows
 * with a thumbnail of the base color, a dot of your color, what changed and a state pill. A
 * listbox: arrow keys, Home and End move the selection; thumbnails load when a row is seen.
 * Pointing at a row (or moving to it with the keys) reports it with `point`, so the 3D view can
 * light that material; `pointed` marks the row of the part the pointer is on in the 3D view. */
import { useEffect, useId, useMemo, useRef, type KeyboardEvent, type ReactNode } from "react";
import { ExtraIcon } from "./Icons";
import { useVisible } from "./media";
import { changeSummary, type ColorMaterial, type ColorMode, type Rgb } from "./colorAdapter";
import { cssColor } from "./colorMath";
import { rangeSwatches } from "./ColorRangeCard";

export type ListFilter = "all" | "changed" | "weapon";
/** Which materials the weapon is drawn with, and which of them the body uses as well. The page
 * takes both from the 3D view's meshes, whose weapon mark comes from Rust. */
export interface WeaponMaterials { weapon: ReadonlySet<string>; body: ReadonlySet<string> }
/** The line a material used by both the weapon and the body shows. */
export const WEAPON_AND_BODY = "Used by the weapon and the body";
export const onWeaponAndBody = (id: string, weapon?: WeaponMaterials) => Boolean(weapon?.weapon.has(id) && weapon.body.has(id));
export const STATE_PILLS: Record<ColorMaterial["state"], string> = {
  changed: "Changed", paused: "Paused", shared: "Shared", "view-only": "View only", missing: "Unavailable", original: "",
};
/** The second line of a row: what changed, or the material's state. */
export function stateLine(material: ColorMaterial, mode: ColorMode): string {
  if (material.values) return changeSummary(material.values);
  if (material.missing) return "Saved material unavailable";
  if (material.state === "view-only") return "Cannot be recolored";
  if (material.sharedWith.length) return `Also used by ${material.sharedWith.length} other model${material.sharedWith.length === 1 ? "" : "s"}`;
  return mode === "hero" ? "Game colors" : "Original colors";
}
/** The color the small dot shows: your tint, else what your first color range turns its color into. */
export function yourColor(material: ColorMaterial): Rgb | null {
  if (!material.values) return null;
  if (material.values.color) return material.values.color;
  const range = material.values.ranges?.find((r) => r.enabled !== false);
  return range ? rangeSwatches(range).to : null;
}
/** The rows shown for a filter and a search, in their groups (each group once, in list order). */
export function visibleGroups(materials: ColorMaterial[], filter: ListFilter, query: string, weapon?: WeaponMaterials) {
  const text = query.trim().toLowerCase();
  const kept = (m: ColorMaterial) => (filter === "changed" ? Boolean(m.values) : filter === "weapon" ? Boolean(weapon?.weapon.has(m.id)) : true);
  const shown = materials.filter((m) => kept(m) && (!text || `${m.label} ${m.id}`.toLowerCase().includes(text)));
  const groups = new Map<string, ColorMaterial[]>();
  for (const material of shown) groups.set(material.group, [...(groups.get(material.group) ?? []), material]);
  return [...groups.entries()].map(([name, items]) => ({ name, items }));
}

function Thumbnail({ material, url, seen }: { material: ColorMaterial; url: string; seen: (id: string) => void }) {
  const { ref, visible } = useVisible();
  useEffect(() => { if (visible) seen(material.id); }, [visible, material.id, seen]);
  const dot = yourColor(material);
  return (
    <div ref={ref} className="color-thumb" style={{ background: cssColor(material.baseColor ?? material.originalTint) }} aria-hidden="true">
      {url && <img src={url} alt="" loading="lazy" decoding="async" />}
      {dot && <u style={{ background: cssColor(dot) }} />}
    </div>
  );
}

export function ColorMaterialList({ materials, mode, selected, select, filter: chosen, setFilter, query, setQuery, thumbnails, seen, empty, point, pointed, weapon }: {
  materials: ColorMaterial[]; mode: ColorMode; selected: string; select: (id: string) => void;
  /** The weapon's materials; the Weapon filter shows only when one of the listed materials is one. */
  weapon?: WeaponMaterials;
  /** A row is pointed at or focused (null: none). */
  point?: (id: string | null) => void;
  /** The material under the pointer in the 3D view. */
  pointed?: string | null;
  filter: ListFilter; setFilter: (filter: ListFilter) => void; query: string; setQuery: (query: string) => void;
  thumbnails: Record<string, string>; seen: (id: string) => void;
  /** Shown instead of the rows (loading, no catalog …). */
  empty?: ReactNode;
}) {
  const weaponCount = weapon ? materials.filter((m) => weapon.weapon.has(m.id)).length : 0;
  const filter: ListFilter = chosen === "weapon" && !weaponCount ? "all" : chosen;
  const groups = useMemo(() => visibleGroups(materials, filter, query, weapon), [materials, filter, query, weapon]);
  const flat = groups.flatMap((group) => group.items);
  const changed = materials.filter((m) => m.values).length;
  const prefix = useId();
  const box = useRef<HTMLDivElement>(null);
  const optionId = (id: string) => `${prefix}-${flat.findIndex((m) => m.id === id)}`;
  const active = flat.some((m) => m.id === selected) ? selected : "";
  useEffect(() => {
    if (!active) return;
    // Scroll only the list (scrollIntoView would scroll the page too).
    const list = box.current;
    const row = [...(list?.querySelectorAll<HTMLElement>("[data-id]") ?? [])].find((item) => item.dataset.id === active);
    if (!list || !row) return;
    const top = row.offsetTop - list.offsetTop, bottom = top + row.offsetHeight;
    if (top < list.scrollTop) list.scrollTop = top - 28;
    else if (bottom > list.scrollTop + list.clientHeight) list.scrollTop = bottom - list.clientHeight + 4;
  }, [active]);
  function keys(event: KeyboardEvent<HTMLDivElement>) {
    if (!flat.length) return;
    const at = flat.findIndex((m) => m.id === active);
    const next = ({ ArrowDown: at + 1, ArrowUp: at < 0 ? 0 : at - 1, Home: 0, End: flat.length - 1, PageDown: at + 8, PageUp: at - 8 } as Record<string, number>)[event.key];
    if (next === undefined) return;
    event.preventDefault();
    const id = flat[Math.max(0, Math.min(flat.length - 1, next))].id;
    select(id);
    point?.(id);
  }
  return (
    <section className="color-list" aria-label="Materials">
      <div className="color-list-head">
        <h3>Materials</h3>
        <div className="color-segmented small" role="radiogroup" aria-label="Show materials">
          <button type="button" role="radio" aria-checked={filter === "all"} onClick={() => setFilter("all")}>All {materials.length}</button>
          <button type="button" role="radio" aria-checked={filter === "changed"} onClick={() => setFilter("changed")}>Changed {changed}</button>
          {weaponCount > 0 && <button type="button" role="radio" aria-checked={filter === "weapon"} onClick={() => setFilter("weapon")}>Weapon {weaponCount}</button>}
        </div>
      </div>
      <div className="extras-search color-search">
        <ExtraIcon name="search" size={16} />
        <input aria-label="Search materials" placeholder="Search materials…" value={query} onChange={(e) => setQuery(e.target.value)} />
        {query && <button aria-label="Clear search" onClick={() => setQuery("")}><ExtraIcon name="close" size={14} /></button>}
      </div>
      {empty ?? (flat.length === 0 ? (
        <div className="color-list-empty">
          <ExtraIcon name="search" size={24} />
          <strong>{query ? "No matching materials" : filter === "changed" ? "No changed materials yet" : "No materials found"}</strong>
          <p>{query ? "Try another name." : filter === "changed" ? "Choose a material and change its colors." : mode === "hero" ? "The hero's model lists no materials in this game version." : "Prepare your model and analyze its materials on Appearance & weapons."}</p>
        </div>
      ) : (
        <div
          ref={box}
          className="color-list-scroll"
          role="listbox"
          aria-label="Materials"
          tabIndex={0}
          aria-activedescendant={active ? optionId(active) : undefined}
          onKeyDown={keys}
          onFocus={(event) => { if (event.target === event.currentTarget && active) point?.(active); }}
          onBlur={() => point?.(null)}
          onPointerLeave={() => point?.(null)}
        >
          {groups.map((group) => (
            <div role="group" aria-label={group.name} key={group.name} className="color-list-group">
              <div className="color-list-group-name" aria-hidden="true">{group.name}</div>
              {group.items.map((material) => {
                const pill = STATE_PILLS[material.state];
                return (
                  <div
                    key={material.id}
                    id={optionId(material.id)}
                    data-id={material.id}
                    role="option"
                    aria-selected={material.id === active}
                    className={`color-row ${material.id === active ? "selected" : ""} ${material.id === pointed ? "pointed" : ""} ${material.state === "view-only" || material.missing ? "dim" : ""}`}
                    onClick={() => select(material.id)}
                    onPointerEnter={() => point?.(material.id)}
                  >
                    <Thumbnail material={material} url={thumbnails[material.id] ?? ""} seen={seen} />
                    <div className="color-row-text" title={`${material.label}\n${stateLine(material, mode)}`}>
                      {/* A name the column cuts: its full name, and its file to tell two of one name apart. */}
                      <strong title={material.id && material.id !== material.label ? `${material.label}\n${material.id}` : material.label}>{material.label}</strong>
                      <small>{stateLine(material, mode)}</small>
                      {onWeaponAndBody(material.id, weapon) && <small>{WEAPON_AND_BODY}</small>}
                    </div>
                    {pill && <span className={`color-pill ${material.state}`}>{pill}</span>}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      ))}
      {point && !empty && flat.length > 0 && (
        <p className="color-list-note">Point at a material to see it light up on the character. Pointing at a part of the character marks its material here.</p>
      )}
    </section>
  );
}
