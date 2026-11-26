/** Extras → Colors: the material list (left), the character in 3D or the flat texture (middle)
 * and the editor as three steps (right), under a top bar with the mode, the counts, Undo, Redo
 * and Reset all colors. Without a replacement model it recolors the game hero's own materials
 * (Hero colors); with one it edits the replacement's materials (the Surface changes rows of the
 * Appearance page). Both through one `ColorAdapter` (colorAdapter.ts). Undo and Redo are the
 * app's project history (history/useProjectHistory.tsx): a drag is one step there.
 * Pointing at a material in the list makes it pulse on the character; pointing at a part of the
 * character marks its row. "Recolor another color" picks a color on the character (or the
 * image), lights everything it covers, then asks for the new color (colorPick.ts). */
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import type { HeroColorRange, HeroMaterial, Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import { changeCustomization } from "../components/customization";
import { floatBuffer, indexBuffer } from "../preview/packet";
import { ExtraIcon } from "./Icons";
import { colorSwitches, withDefaults } from "./heroColorRanges";
import { MaterialParts, type PartMesh } from "./colorParts";
import { colorAdapter, savedHeroColors, type ColorEdit, type ColorKey, type ColorMaterial, type Rgb } from "./colorAdapter";
import { ColorMaterialList, type ListFilter, type WeaponMaterials } from "./ColorMaterialList";
import { ColorEditor, type PickState } from "./ColorEditor";
import { ColorTextureView, type TexturePick } from "./ColorTextureView";
import { HeroImageLoader, useColorTextures, useFileImages, usePixels, type ImageState, type Pixels } from "./colorImages";
import { flatImage, recipeColor, recipeFor, uvAverage, zeroUvs } from "./colorPixels";
import { forgetScene, GUN_AWAY_REASON, heroSceneKey, loadHeroScene, loadReplacementScene, replacementSceneKey, useScene, type HeroSceneMesh } from "./colorScenes";
import type { Highlight, ScenePick } from "./ColorScene";
import { huelessPick, pickRange, shownColorName, type Bytes } from "./colorPick";
import { percentText, useCoverages } from "./colorCoverage";
import { glowColorFor } from "./colorGlow";
import { rangeColors } from "./ColorRangeCard";
import { cssColor, rememberColor } from "./colorMath";
import { useHistory } from "../history/useProjectHistory";
import { m, tc, tp, useLanguage, useT } from "../i18n";
import { T } from "../i18n/T";
import "./colors.css";
// i18n-area: colors

export { heroColorItems, type HeroColorItem } from "./colorAdapter";
export { activeRanges, coverPixel } from "./colorPixels";
const ColorScene = lazy(() => import("./ColorScene"));

/** Approximates the build: the tint multiplies the base color; saturation and
 * brightness scale the material's own albedo adjustment. */
export function heroColorFilter(entry: HeroMaterialEntry, row?: HeroMaterial) {
  const albedo = entry.albedo ?? [1, 1, 1];
  return `saturate(${albedo[1] * (row?.saturation ?? 1)}) brightness(${albedo[2] * (row?.brightness ?? 1)})`;
}
const fold = (value: string) => value.toLowerCase();
const NO_IMAGE: ImageState = { url: "", loading: false, error: "", media: null };
/** A material's typical color: the mean of a sample of its pixels. */
function averageColor(pixels: Pixels | undefined): Rgb | null {
  if (!pixels) return null;
  const data = pixels.small.data;
  let r = 0, g = 0, b = 0, n = 0;
  const stride = Math.max(4, Math.floor(data.length / 4 / 4096) * 4);
  for (let i = 0; i < data.length; i += stride) { if (data[i + 3] < 8) continue; r += data[i]; g += data[i + 1]; b += data[i + 2]; n++; }
  return n ? [r / n / 255, g / n / 255, b / n / 255] : null;
}
const multiply = (a: Rgb, b: Rgb): Rgb => [a[0] * b[0], a[1] * b[1], a[2] * b[2]];
/** The spot saved for a range for one part: its texture coordinates to six decimals (far below
 * `colorParts.SPOT_REACH`, so the saved spot is on the same part). */
const partSpot = ({ u, v }: { u: number; v: number }) => ({ u: Math.round(u * 1e6) / 1e6, v: Math.round(v * 1e6) / 1e6 });
const toBytes = (color: Rgb): Bytes => color.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255)) as Bytes;
const fromBytes = (bytes: Bytes): Rgb => [bytes[0] / 255, bytes[1] / 255, bytes[2] / 255];

/** The reason shown for a hidden part when the scene gives none (English; shown in the language shown). */
const NOT_SHOWN = m("Not shown in game by default");
/** A line under the view, kept as what it says (built in the language shown when drawn). */
type Notice = { kind: "other"; part: string | null; material: string } | { kind: "no-uvs" } | { kind: "loading" } | null;
type Tab = "character" | "texture";
type View = "original" | "yours";
/** The line under the view while the Weapon filter draws weapon parts the view hides at first. */
export const weaponShownNote = (count: number) =>
  tp(count, { one: "Weapon: {count} part the game places by animation is shown where the model keeps it.", other: "Weapon: {count} parts the game places by animation are shown where the model keeps them." });

export function HeroColors({
  project,
  update,
  disabled,
  catalog,
  discovering,
  canDiscover,
  reload,
  openSetup,
  openPreparation,
  revision,
  heroName = "",
}: {
  project: Project;
  update: (project: Project) => void;
  disabled: boolean;
  catalog: ComponentCatalog | null;
  discovering: boolean;
  canDiscover: boolean;
  reload: () => void;
  openSetup?: () => void;
  /** Opens Prepare model (replacement mode without a prepared model). */
  openPreparation?: () => void;
  revision: number;
  /** The hero's name as the Extras page shows it. */
  heroName?: string;
}) {
  const language = useLanguage();
  const t = useT();
  // Built again when the language changes: the adapter's texts are in the language shown.
  // eslint-disable-next-line react-hooks/exhaustive-deps -- the language is read inside
  const adapter = useMemo(() => colorAdapter(project, catalog, revision), [project, catalog, revision, language]);
  const { mode, materials } = adapter;
  const hero = mode === "hero";
  const [selected, setSelected] = useState("");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<ListFilter>("all");
  const [tab, setTab] = useState<Tab>("character");
  const [view, setView] = useState<View>("yours");
  const [pick, setPick] = useState<PickState | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [resetView, setResetView] = useState(0);
  const [dragging, setDragging] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);
  const [attempt, setAttempt] = useState(0);
  // Pointing: a row of the list (lights the material), a part of the character (marks the row),
  // a range card or a dragged "selects" slider (lights what the range selects).
  const [pointedRow, setPointedRow] = useState<string | null>(null);
  const [pointedPart, setPointedPart] = useState<string | null>(null);
  const [litRange, setLitRange] = useState<number | null>(null);
  const litTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const selectDragging = useRef(false);
  useEffect(() => { setSelected(""); setPick(null); setNotice(null); setLitRange(null); }, [mode, project.id, project.original_model]);
  useEffect(() => () => clearTimeout(litTimer.current), []);
  const material = materials.find((m) => fold(m.id) === fold(selected)) ?? null;
  const select = (id: string) => { if (fold(id) !== fold(selected)) { setPick(null); setNotice(null); setLitRange(null); } setSelected(id); };

  // Undo and Redo: the app's one project history (Ctrl+Z / Ctrl+Y are handled there too).
  const history = useHistory();
  function apply(next: Project | null, _ids: string[]) {
    if (next) update(next);
  }
  function travel(direction: "undo" | "redo") {
    setPick(null);
    if (direction === "undo") history?.undo(); else history?.redo();
  }
  // A drag of a slider is one step of the history; a drag still open when the page goes ends there.
  const dragOpen = useRef(false);
  const drag = (active: boolean) => { setDragging(active); dragOpen.current = active; history?.gesture(active); };
  const historyRef = useRef(history);
  historyRef.current = history;
  useEffect(() => () => { if (dragOpen.current) historyRef.current?.gesture(false); }, []);

  // The character: the hero's own model (after the catalog is loaded) or the prepared replacement.
  const sceneKey = hero ? (catalog ? heroSceneKey(project, revision) : null) : replacementSceneKey(project);
  // A prepared model may have been prepared again under the same path: read it again each time the page opens.
  useEffect(() => { const key = replacementSceneKey(project); if (key) forgetScene(key); // eslint-disable-next-line react-hooks/exhaustive-deps -- once per opening
  }, []);
  const scene = useScene(sceneKey, hero ? () => loadHeroScene(project) : loadReplacementScene(project), true, attempt);
  useEffect(() => { if (scene.status === "failed") setTab("texture"); }, [scene.status]);
  const ids = useMemo(() => new Map(materials.map((m) => [fold(m.id), m.id])), [materials]);
  const meshes = (scene.scene?.metadata.meshes ?? []) as HeroSceneMesh[];
  const meshMaterials = useMemo(() => {
    const paths = new Map(materials.flatMap((m) => (m.entry ? [[fold(m.entry.path), m.id] as const] : [])));
    return meshes.map((mesh) => (hero ? ids.get(fold(mesh.target ?? "")) ?? paths.get(fold(mesh.material)) ?? null : ids.get(fold(mesh.material)) ?? null));
  }, [meshes, ids, materials, hero]);
  const hiddenMeshes = useMemo(() => meshes.flatMap((mesh, index) => (mesh.default_hidden ? [index] : [])), [meshes]);
  // The weapon's materials, from the meshes Rust marks as weapon parts (none: no Weapon filter).
  const weaponMaterials = useMemo<WeaponMaterials | undefined>(() => {
    const weapon = new Set<string>(), body = new Set<string>();
    meshes.forEach((mesh, index) => { const id = meshMaterials[index]; if (id) (mesh.weapon ? weapon : body).add(id); });
    return weapon.size ? { weapon, body } : undefined;
  }, [meshes, meshMaterials]);
  const weaponFilter = filter === "weapon" && Boolean(weaponMaterials);
  // With the Weapon filter on, weapon parts the view hides at first (a gun the game moves into
  // the hand only while playing) are drawn too, where the model stores them, and can be clicked.
  // Weapon effect layers (glass, glow) stay hidden: drawn solid they would cover the gun.
  const weaponShown = useMemo(() => (weaponFilter && !showHidden ? hiddenMeshes.filter((index) => meshes[index].weapon && meshes[index].hidden_reason === GUN_AWAY_REASON) : []), [weaponFilter, showHidden, hiddenMeshes, meshes]);
  const hidden = useMemo(() => new Set(showHidden ? [] : hiddenMeshes.filter((index) => !weaponShown.includes(index))), [showHidden, hiddenMeshes, weaponShown]);
  // Those parts can lie outside what the view frames (a gun stored at the model's origin): frame
  // the drawn parts with them while they are drawn.
  const weaponFrame = useMemo(() => {
    const packet = scene.scene;
    if (!packet || !weaponShown.length) return undefined;
    const { min, max } = packet.metadata.bounds;
    const lo: [number, number, number] = [...min], hi: [number, number, number] = [...max];
    for (const index of weaponShown) {
      const positions = floatBuffer(packet, meshes[index].positions);
      for (let i = 0; i + 2 < positions.length; i += 3) for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], positions[i + k]); hi[k] = Math.max(hi[k], positions[i + k]); }
    }
    return { min: lo, max: hi };
  }, [scene.scene, meshes, weaponShown]);
  const hiddenReasons = useMemo(() => {
    const counts = new Map<string, number>();
    for (const index of hiddenMeshes) { const reason = meshes[index].hidden_reason || NOT_SHOWN; counts.set(reason, (counts.get(reason) ?? 0) + 1); }
    return [...counts];
  }, [hiddenMeshes, meshes]);
  const noUvs = useMemo(() => scene.scene ? meshes.map((mesh) => zeroUvs(mesh.uvs ? floatBuffer(scene.scene!, mesh.uvs) : null)) : [], [scene.scene, meshes]);
  const shownIds = useMemo(() => new Set(meshMaterials.filter((id, index): id is string => Boolean(id) && !hidden.has(index))), [meshMaterials, hidden]);
  const character = tab === "character" && scene.status === "ready";
  // Hero materials no mesh of the shown model uses (smoke, effect-only materials).
  const notOnModel = useMemo(() => {
    const rows = (scene.scene?.metadata as { rows_without_mesh?: string[] } | undefined)?.rows_without_mesh ?? [];
    return rows.map((target) => materials.find((m) => fold(m.id) === fold(target))?.label ?? target.split("/").pop()!.replace(/\.vmat_c$/, ""));
  }, [scene.scene, materials]);

  // Images: thumbnails when a row is seen, every shown material for the 3D view, the selected one.
  const [seen, setSeen] = useState<Set<string>>(() => new Set());
  const markSeen = useCallback((id: string) => setSeen((current) => (current.has(id) ? current : new Set(current).add(id))), []);
  const wanted = useMemo(() => new Set([...seen, ...(material ? [material.id] : []), ...(character ? shownIds : [])]), [seen, material, character, shownIds]);
  const [heroImages, setHeroImages] = useState<Record<string, ImageState>>({});
  const report = useCallback((id: string, state: ImageState) => setHeroImages((current) => {
    const old = current[id];
    return old && old.url === state.url && old.loading === state.loading && old.error === state.error && old.media === state.media ? current : { ...current, [id]: state };
  }), []);
  const filePaths = hero ? [] : materials.flatMap((m) => (wanted.has(m.id) && m.texture?.kind === "file" ? [m.texture.path] : []));
  const files = useFileImages(filePaths, project.id);
  const imageOf = (m: ColorMaterial): ImageState => (!m.texture ? NO_IMAGE : m.texture.kind === "hero" ? heroImages[m.id] ?? NO_IMAGE : files[m.texture.path] ?? NO_IMAGE);
  const thumbnails = Object.fromEntries(materials.map((m) => [m.id, imageOf(m).url]));
  const pixelUrls: Record<string, string> = {};
  for (const m of materials) if ((character && shownIds.has(m.id)) || m.id === material?.id) { const url = imageOf(m).url; if (url) pixelUrls[m.id] = url; }
  const pixels = usePixels(pixelUrls);
  const own = material ? pixels[material.id] ?? null : null;

  // Picking: the range a pick would make, and how much each range (and the pick) selects.
  const ranges = useMemo(() => material?.values?.ranges ?? [], [material]);
  // The parts of a material (colorParts.ts), for ranges that change one part: worked out from the
  // shown model's meshes once per material and scene, only for materials that need them.
  const partsCache = useMemo(() => new Map<string, MaterialParts | null>(), [scene.scene, meshMaterials]);
  const partsOf = useCallback((id: string | undefined): MaterialParts | null => {
    if (!id || !scene.scene) return null;
    if (!partsCache.has(id)) {
      const found: PartMesh[] = [];
      meshes.forEach((mesh, index) => {
        if (meshMaterials[index] !== id || !mesh.uvs || !mesh.indices) return;
        found.push({ positions: floatBuffer(scene.scene!, mesh.positions), uvs: floatBuffer(scene.scene!, mesh.uvs), indices: indexBuffer(scene.scene!, mesh.indices) });
      });
      partsCache.set(id, found.length ? new MaterialParts(found) : null);
    }
    return partsCache.get(id) ?? null;
  }, [partsCache, scene.scene, meshes, meshMaterials]);
  const switches = colorSwitches();
  const ownParts = material && (switches.part_only || ranges.some((r) => r.part)) ? partsOf(material.id) : null;
  const pickResult = useMemo(() => {
    if (pick?.stage !== "choose") return null;
    const result = pickRange(pick.from, pick.to, ranges, pick.index, pick.keep);
    // This part only (switch part_only): the range keeps the clicked spot; Everywhere drops it.
    if (pick.scope === "part" && pick.part) result.range = { ...result.range, part: pick.part };
    else if (pick.scope === "everywhere") { const { part: _part, ...rest } = result.range; result.range = rest; }
    return result;
  }, [pick, ranges]);
  const coverageRanges = useMemo(() => {
    const all = pickResult ? [...ranges, pickResult.range] : [...ranges];
    // Ranges for one part (and a pick on a part) also measured everywhere, for the choice's shares.
    const everywhere = all.map((range) => { const { part: _part, ...rest } = range; return rest; });
    const choosePart = pick?.stage === "choose" && pick.part && pickResult ? [{ ...pickResult.range, part: pick.part }] : [];
    return [...all, ...everywhere, ...choosePart];
  }, [ranges, pickResult, pick]);
  const coverageAll = useCoverages(own ? own.small : null, coverageRanges, ownParts);
  const coverages = coverageAll.slice(0, ranges.length);
  const pickCoverage = pickResult ? coverageAll[ranges.length] ?? null : null;
  const counted = ranges.length + (pickResult ? 1 : 0);
  const everywhereCoverages = coverageAll.slice(counted, counted + ranges.length);
  const pickShares = pick?.stage === "choose" && pick.part && pickResult
    ? { part: coverageAll[2 * counted] ?? null, everywhere: coverageAll[counted + ranges.length] ?? null }
    : null;
  // While choosing, your view shows the range the pick would make.
  const shownValues = (m: ColorMaterial) => {
    if (!pickResult || m.id !== material?.id) return m.values;
    const next = [...ranges];
    next[pick!.index ?? ranges.length] = pickResult.range;
    return { ...(m.values ?? {}), ranges: next };
  };

  // What lights up: the pick's selection, a pointed or dragged range, or a pointed material.
  const litValues: HeroColorRange | null = pickResult ? pickResult.range : litRange !== null ? ranges[litRange] ?? null : null;
  const litCoverage = pickResult ? pickCoverage : litRange !== null ? coverages[litRange] ?? null : null;
  const litFrom = litValues ? (pick?.stage === "choose" ? fromBytes(pick.from) : rangeColors(litValues, litCoverage).from) : null;
  const selectionGlow = useMemo(() => (material && litValues ? { id: material.id, range: withDefaults(litValues), color: glowColorFor(litFrom) } : null),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed by the values
    [material?.id, JSON.stringify(litValues), litFrom?.join()]);
  const glowCache = useRef(new WeakMap<Pixels, Rgb | null>());
  const pointedMaterial = pointedRow ? materials.find((m) => m.id === pointedRow) ?? null : null;
  const pointedColor = useMemo(() => {
    if (!pointedMaterial) return null;
    const source = pixels[pointedMaterial.id];
    let average: Rgb | null = null;
    if (source) {
      if (!glowCache.current.has(source)) glowCache.current.set(source, averageColor(source));
      average = glowCache.current.get(source) ?? null;
    }
    const tint = view === "yours" && pointedMaterial.values?.color ? pointedMaterial.values.color : pointedMaterial.originalTint;
    return glowColorFor(multiply(average ?? pointedMaterial.baseColor ?? [1, 1, 1], tint));
  }, [pointedMaterial, pixels, view]);
  const highlight: Highlight | null = useMemo(() => {
    if (selectionGlow) return { kind: "selection", id: selectionGlow.id, color: selectionGlow.color };
    if (pointedMaterial && pointedColor) return { kind: "material", id: pointedMaterial.id, color: pointedColor };
    return null;
  }, [selectionGlow, pointedMaterial, pointedColor]);
  /** Light range `index` now; with `ms`, let it fade that long after (null index: let go). */
  const holdRange = (index: number | null, ms: number | null) => {
    clearTimeout(litTimer.current);
    if (index !== null) setLitRange(index);
    if (ms !== null) litTimer.current = setTimeout(() => setLitRange(null), ms);
  };

  // What the 3D view draws: each material's recipe and tint, in the chosen view.
  const recipes = Object.fromEntries(Object.keys(pixels).map((id) => {
    const m = materials.find((item) => item.id === id)!;
    return [id, recipeFor(mode, view, m ? shownValues(m) : null, m?.entry)];
  }));
  const live = dragging && material ? material.id : null;
  // The parts of every material drawn with a range for one part (and of the one being picked on).
  const drawnParts: Record<string, MaterialParts | null> = {};
  for (const id of Object.keys(pixels)) {
    const m = materials.find((item) => item.id === id);
    const values = m ? shownValues(m) : null;
    if (values?.ranges?.some((range) => range.part) || (selectionGlow?.id === id && selectionGlow.range.part)) drawnParts[id] = partsOf(id);
  }
  const { textures, selection, version } = useColorTextures(character ? pixels : {}, recipes, live, character && selectionGlow ? { id: selectionGlow.id, range: selectionGlow.range } : null, drawnParts);
  const colors: Record<string, Rgb> = {};
  for (const m of materials) {
    const tint = view === "yours" && m.capabilities.tint ? m.values?.color ?? m.originalTint : m.originalTint;
    // A plain base color (no image) changes as the build changes it.
    colors[m.id] = m.baseColor ? multiply(recipeColor(m.baseColor, recipeFor(mode, view, m.values, m.entry)), tint) : tint;
  }

  function edit(changes: ColorEdit, removed: ColorKey[] = []) {
    if (!material) return;
    apply(adapter.edit(material.id, changes, removed), [material.id]);
  }
  /** The selected material's image is one flat color: nothing in it to pick out by color. */
  const flat = (id: string | undefined) => Boolean(id && pixels[id] && flatImage(pixels[id].small));
  function picked(bytes: Bytes, spot: { x: number; y: number; w: number } | null, uv?: { u: number; v: number }) {
    if (!pick) return;
    if (flat(material?.id)) { setPick({ stage: "point", index: pick.index, keep: false, note: "flat" }); return; }
    const grey = huelessPick(bytes);
    // Grey, white and black are given a new color only with the switch set_colour.
    if (grey && !switches.set_colour) { setPick({ stage: "point", index: pick.index, keep: false, note: grey }); return; }
    // This part only (switch part_only): the spot's part, if the model has one there.
    const spotPart = switches.part_only && uv ? (partsOf(material?.id)?.at(uv.u, uv.v) ? partSpot(uv) : null) : undefined;
    setPick({
      stage: "choose", index: pick.index, keep: false, from: bytes, to: pick.stage === "choose" && !pick.keep ? pick.to : fromBytes(bytes), spot,
      ...(spotPart !== undefined ? { part: spotPart, scope: spotPart ? "part" : "everywhere" } : {}),
    });
    setNotice(null);
  }
  function pickOnModel(hit: ScenePick) {
    if (!pick || !material) return;
    if (hit.id !== material.id) { setNotice({ kind: "other", part: materials.find((m) => m.id === hit.id)?.label ?? null, material: material.label }); return; }
    const source = pixels[hit.id];
    if (noUvs[hit.mesh]) { setNotice({ kind: "no-uvs" }); setTab("texture"); return; }
    if (!source) { setNotice({ kind: "loading" }); return; }
    // The average over the small area read around the click on the clicked part.
    picked(uvAverage(source.full, hit.samples?.length ? hit.samples : [{ u: hit.u, v: hit.v }]), { x: hit.x, y: hit.y, w: hit.w }, { u: hit.u, v: hit.v });
  }
  function changeColor(index: number) {
    const range = ranges[index];
    if (!range) return;
    const { from, to } = rangeColors(range, coverages[index]);
    // A range for one part keeps its part (its card switches it to Everywhere).
    setPick({ stage: "choose", index, keep: true, from: toBytes(from), to, spot: null, ...(range.part ? { part: range.part, scope: "part" as const } : {}) });
  }
  function finishPick() {
    if (!pickResult || !pick || pick.stage !== "choose") return;
    const next = [...ranges];
    next[pick.index ?? ranges.length] = pickResult.range;
    edit({ ranges: next });
    rememberColor(pick.to);
    setPick(null);
  }
  const cancelPick = () => { setPick(null); setNotice(null); };
  // Esc cancels picking at any point (a field that uses Esc itself keeps it).
  useEffect(() => {
    if (!pick) return;
    const key = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape" && !event.defaultPrevented) { setPick(null); setNotice(null); } };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [pick]);
  const root = useRef<HTMLDivElement>(null);

  const image = material ? imageOf(material) : NO_IMAGE;
  const rangesUnsupported = hero && typeof image.media?.details.ranges_unsupported === "string" ? (image.media.details.ranges_unsupported as string) : "";
  const average = (material && (averageColor(pixels[material.id]) ?? material.baseColor ?? material.originalTint)) || ([0.6, 0.6, 0.6] as Rgb);
  const originalLabel = hero ? t("Game") : tc("own colors", "Original");
  const pickHint = character
    ? material ? t("Click a color on {material} on the character (or on the {original} image on the Texture tab).", { material: material.label, original: originalLabel }) : t("Click a color on the material on the character (or on the {original} image on the Texture tab).", { original: originalLabel })
    : t("Click a color on the {original} or Yours image.", { original: originalLabel });
  const noticeText = !notice ? "" : notice.kind === "no-uvs" ? t("This part has no texture coordinates to pick from: pick on the Texture tab instead.")
    : notice.kind === "loading" ? t("The texture is still loading.")
    : notice.part !== null ? t("That part is {part}: click {material} to pick its color.", { part: notice.part, material: notice.material })
    : t("That part is another material: click {material} to pick its color.", { material: notice.material });
  const heroRows = savedHeroColors(project);
  const changed = adapter.changed;
  const loaders = hero
    ? materials.filter((m) => m.texture?.kind === "hero" && wanted.has(m.id)).map((m) => {
        const texture = m.texture as Extract<ColorMaterial["texture"], { kind: "hero" }>;
        return <HeroImageLoader key={m.id} id={m.id} request={texture.request} identity={texture.identity} report={report} />;
      })
    : null;

  const listEmpty = hero && discovering ? (
    <div className="color-list-empty"><span className="extra-spinner" /><strong>{t("Finding the hero's materials")}</strong><p>{t("Reading the hero's model and the game models that share its materials.")}</p></div>
  ) : hero && !catalog ? (
    <div className="color-list-empty">
      <ExtraIcon name="palette" size={28} />
      <strong>{canDiscover ? t("Load your character's assets") : t("Connect your game to get started")}</strong>
      <p>{canDiscover ? t("The hero's materials come from your game files.") : t("Choose the original game VPK in Setup, then return here.")}</p>
      {canDiscover ? (
        <button className="extra-primary" onClick={reload} disabled={disabled}>{t("Load character assets")}<ExtraIcon name="arrow" size={16} /></button>
      ) : openSetup && <button className="extra-primary" onClick={openSetup}>{t("Open Setup")}<ExtraIcon name="arrow" size={16} /></button>}
    </div>
  ) : undefined;

  // A material whose image is one flat color is said to be so as soon as picking starts (or
  // its image arrives), before any click.
  const shownPick: PickState | null = pick?.stage === "point" && !pick.note && flat(material?.id) ? { ...pick, note: "flat" } : pick;
  // The view's chip and legend: what the view is showing now.
  const litName = pickResult ? "this color" : litRange !== null ? `Range ${litRange + 1}` : "";
  const chip = shownPick?.stage === "point" && !shownPick.note ? (material ? t("Click a color on {material}", { material: material.label }) : t("Click a color on the character"))
    : litName ? (pickResult ? t("Showing what this color covers") : t("Showing what Range {number} covers", { number: (litRange ?? 0) + 1 }))
    : material ? t("{material} selected", { material: material.label }) : "";
  const legend = selectionGlow && litName && material
    ? litCoverage
      ? pickResult ? t("what this color covers · {share} of {material}", { share: percentText(litCoverage.fraction), material: material.label }) : t("what Range {number} covers · {share} of {material}", { number: (litRange ?? 0) + 1, share: percentText(litCoverage.fraction), material: material.label })
      : pickResult ? t("what this color covers") : t("what Range {number} covers", { number: (litRange ?? 0) + 1 })
    : highlight?.kind === "material" && pointedMaterial ? t("{material} · what you are pointing at", { material: pointedMaterial.label }) : "";
  const glowCss = highlight ? cssColor(highlight.color) : undefined;
  const popover = pick?.stage === "choose" && pick.spot && !pick.keep ? (
    <span className="color-pick-pop" style={{ left: Math.max(8, Math.min(pick.spot.x - 140, pick.spot.w - 288)), top: pick.spot.y > 70 ? pick.spot.y - 54 : pick.spot.y + 24 }}>
      <span className="color-inline-swatch" style={{ background: cssColor(fromBytes(pick.from)) }} aria-hidden="true" />
      <b>{t("This {color}", { color: shownColorName(fromBytes(pick.from)) })}</b>
      {pickCoverage && <span><T text=" selects <b>{share}</b> of {material}" values={{ share: percentText(pickCoverage.fraction), material: material?.label ?? "" }} tags={{ b: (chunk) => <b>{chunk}</b> }} /></span>}
    </span>
  ) : null;

  function stage() {
    if (tab === "texture") {
      if (!material) return <div className="color-stage-message"><strong>{t("Choose a material")}</strong><p>{hero ? t("Its base-color image is shown here, game and yours side by side.") : t("Its base-color image is shown here, original and yours side by side.")}</p></div>;
      if (material.missing) return <div className="color-stage-message"><strong>{material.blocked}</strong></div>;
      if (!material.texture) return <div className="color-stage-message"><strong>{t("No base-color image")}</strong><p>{hero ? t("This material has no base-color texture to show.") : t("This material has a plain base color; the 3D view shows it.")}</p></div>;
      return (
        <>
          <ColorTextureView
            label={material.label}
            originalLabel={originalLabel}
            url={image.url}
            pixels={own}
            original={recipeFor(mode, "original", material.values, material.entry)}
            yours={recipeFor(mode, "yours", shownValues(material), material.entry)}
            tint={material.values?.color ?? material.originalTint}
            originalTint={material.originalTint}
            picking={Boolean(pick)}
            pick={(hit: TexturePick) => picked(hit.color, { x: hit.x, y: hit.y, w: hit.w }, hit.u !== undefined && hit.v !== undefined ? { u: hit.u, v: hit.v } : undefined)}
            parts={ownParts}
            loading={image.loading}
            error={image.error}
            live={dragging}
            approximate={material.approximate}
            cssFilter={(shown) => (material.entry ? heroColorFilter(material.entry, shown === "yours" ? (material.values as HeroMaterial | null) ?? undefined : undefined) : undefined)}
            selection={selectionGlow ? { range: selectionGlow.range, color: selectionGlow.color } : null}
          />
          {popover}
        </>
      );
    }
    if (hero && !catalog) return <div className="color-stage-message"><strong>{t("The hero appears here")}</strong><p>{t("Load the character's assets to see the hero in 3D.")}</p></div>;
    if (!hero && !project.dmx) return (
      <div className="color-stage-message">
        <strong>{t("No prepared model yet")}</strong>
        <p>{t("Prepare your model first: the 3D view shows the prepared replacement with your colors.")}</p>
        <div className="row-actions">
          {openPreparation && <button className="extra-primary" onClick={openPreparation}>{t("Open Prepare model")}<ExtraIcon name="arrow" size={16} /></button>}
          <button className="extra-secondary" disabled={disabled} onClick={() => update(changeCustomization(project, { model_enabled: false }))}>
            {heroName ? t("Recolor the game's own {hero} instead", { hero: heroName }) : t("Recolor the game's own hero instead")}
          </button>
        </div>
        <p>{t("The build then changes the colors of the game's own model and includes no replacement model.")}</p>
      </div>
    );
    if (scene.status === "failed") return (
      <div className="color-stage-message" role="alert">
        <strong>{hero ? t("The hero's model could not be shown") : t("Your model could not be shown")}</strong>
        <p>{scene.message}</p>
        <p>{t("The Texture tab still shows each material's base color, and every setting can be changed.")}</p>
        <div className="row-actions">
          <button className="extra-secondary" onClick={() => setAttempt((v) => v + 1)}>{t("Try again")}</button>
          <button className="extra-secondary" onClick={() => setTab("texture")}>{t("Show the Texture tab")}</button>
        </div>
      </div>
    );
    if (scene.status !== "ready" || !scene.scene) return (
      <div className="color-stage-message" role="status">
        <span className="extra-spinner" />
        <strong>{hero ? t("Loading the hero's model…") : t("Loading your model…")}</strong>
        <p>{hero ? t("The first time, the hero's model is read from your game files; this can take a minute. You can change colors meanwhile.") : t("Reading the prepared model.")}</p>
      </div>
    );
    return (
      <>
        <Suspense fallback={<div className="color-stage-message" role="status"><span className="extra-spinner" /><strong>{t("Loading the 3D view…")}</strong></div>}>
          <ColorScene
            scene={scene.scene}
            materialOf={(index) => meshMaterials[index] ?? null}
            textures={textures}
            colors={colors}
            hidden={hidden}
            selected={material?.id ?? null}
            select={select}
            picking={Boolean(pick)}
            pick={pickOnModel}
            hover={setPointedPart}
            highlight={highlight}
            selection={selection}
            reset={resetView}
            frame={weaponFrame}
            version={version}
          />
        </Suspense>
        {popover}
      </>
    );
  }
  const foot = tab === "texture"
    ? pick ? t("Click a color in the image to pick it, exactly as on the character. Press Esc to cancel.") : ""
    : pick ? t("Click a color: everything that color covers lights up before you choose. Press Esc to cancel.")
    : hero ? t("Click a part of the character to choose its material · approximate: the game's lighting and shaders differ") : t("Click a part of the character to choose its material · the game's lighting differs");

  return (
    <div id="extras-panel" role="tabpanel" aria-labelledby="extras-tab-colors" className="colors-studio" ref={root}>
      {loaders}
      <header className="colors-head colors-topbar">
        <h2>{t("Colors")}</h2>
        <span className="color-mode-badge">{hero ? t("Game hero") : t("Your model")}</span>
        {(!hero || catalog) && <span className="colors-stat">{hero ? tp(materials.length, { one: "{count} material from your game", other: "{count} materials from your game" }) : tp(materials.length, { one: "{count} material", other: "{count} materials" })}</span>}
        {changed > 0 && <span className="color-count">{tp(changed, { one: "{count} changed", other: "{count} changed" })}</span>}
        <span className="color-picker-spacer" />
        <button type="button" className="extra-secondary small color-history" disabled={disabled || !history?.canUndo} aria-keyshortcuts="Control+Z" title={t("Undo ({keys})", { keys: "Ctrl+Z" })} onClick={() => travel("undo")}>
          <span aria-hidden="true">↶ </span>{t("Undo")}
        </button>
        <button type="button" className="extra-secondary small color-history" disabled={disabled || !history?.canRedo} aria-keyshortcuts="Control+Y Control+Shift+Z" title={t("Redo ({keys})", { keys: "Ctrl+Y" })} onClick={() => travel("redo")}>
          <span aria-hidden="true">↷ </span>{t("Redo")}
        </button>
        <span className="color-reset-all">
          <button type="button" className="extra-text-button" disabled={disabled || !changed} aria-expanded={confirmReset} onClick={() => setConfirmReset(!confirmReset)}>{t("Reset all colors…")}</button>
          {confirmReset && (
            <div className="color-confirm" role="dialog" aria-label={t("Reset all colors")}>
              <p>{hero
                ? tp(changed, { one: "Remove your colors from all {count} material? The hero keeps the game's colors. Undo brings them back.", other: "Remove your colors from all {count} materials? The hero keeps the game's colors. Undo brings them back." })
                : tp(changed, { one: "Remove your colors from all {count} material? Other surface changes stay. Undo brings them back.", other: "Remove your colors from all {count} materials? Other surface changes stay. Undo brings them back." })}</p>
              <div className="row-actions">
                <button type="button" className="extra-remove" onClick={() => { apply(adapter.resetAll(), materials.filter((m) => m.values).map((m) => m.id)); setConfirmReset(false); setPick(null); }}>{t("Reset all")}</button>
                <button type="button" className="extra-secondary" autoFocus onClick={() => setConfirmReset(false)}>{t("Cancel")}</button>
              </div>
            </div>
          )}
        </span>
      </header>
      {!hero && heroRows > 0 && (
        <div className="extra-banner" role="note">
          <ExtraIcon name="warning" />
          <div>
            <strong>{tp(heroRows, { one: "{count} saved hero color not used with a replacement model", other: "{count} saved hero colors not used with a replacement model" })}</strong>
            <p>{t("This page now edits your replacement model's colors. Hero colors recolor the game's own model and apply only when you build without a replacement model.")}</p>
          </div>
          <button disabled={disabled} onClick={() => update(changeCustomization(project, { model_enabled: false }))}>
            {t("Build without a replacement model")}
          </button>
        </div>
      )}
      <div className="colors-grid">
        <ColorMaterialList
          materials={materials}
          mode={mode}
          selected={material?.id ?? ""}
          select={select}
          filter={filter}
          setFilter={setFilter}
          weapon={weaponMaterials}
          query={query}
          setQuery={setQuery}
          thumbnails={thumbnails}
          seen={markSeen}
          empty={listEmpty}
          point={setPointedRow}
          pointed={character ? pointedPart ?? (highlight?.kind === "material" ? pointedRow : null) : null}
        />
        <section className="color-view" aria-label={t("Preview")}>
          <div className="color-view-tools">
            <div className="color-segmented" role="radiogroup" aria-label={t("Preview")}>
              {(["character", "texture"] as const).map((key) => (
                <button key={key} type="button" role="radio" aria-checked={tab === key} onClick={() => setTab(key)}>{key === "character" ? tc("color view", "Character") : t("Texture")}</button>
              ))}
            </div>
            {tab === "character" && (
              <div className="color-segmented" role="radiogroup" aria-label={t("Colors shown")}>
                {(["original", "yours"] as const).map((key) => (
                  <button key={key} type="button" role="radio" aria-checked={view === key} onClick={() => setView(key)}>{key === "original" ? originalLabel : t("Yours")}</button>
                ))}
              </div>
            )}
            <span className="color-picker-spacer" />
            {chip && <span className={`color-view-chip ${litName || pick ? "lit" : ""}`} role="status" style={glowCss && litName ? ({ "--glow": glowCss } as CSSProperties) : undefined}>{chip}</span>}
            {tab === "character" && hiddenMeshes.length > 0 && (
              <label className="hero-color-check" title={hiddenReasons.map(([reason, count]) => `${reason === NOT_SHOWN ? t(NOT_SHOWN) : reason} (${count})`).join("\n")}>
                <input type="checkbox" checked={showHidden} onChange={(e) => setShowHidden(e.target.checked)} />
                {t("Show hidden parts ({count})", { count: hiddenMeshes.length })}
              </label>
            )}
            {character && <button type="button" className="extra-secondary small" onClick={() => setResetView((v) => v + 1)}>{t("Reset view")}</button>}
          </div>
          {tab === "character" && showHidden && hiddenReasons.length > 0 && (
            <ul className="color-hidden-reasons" aria-label={t("Why these parts are hidden")}>
              {hiddenReasons.map(([reason, count]) => <li key={reason}>{count > 1 ? tp(count, { other: "{reason} ({count} parts)" }, { reason: reason === NOT_SHOWN ? t(NOT_SHOWN) : reason }) : reason === NOT_SHOWN ? t(NOT_SHOWN) : reason}</li>)}
            </ul>
          )}
          {character && weaponShown.length > 0 && (
            <p className="hint color-view-notice" role="status">{weaponShownNote(weaponShown.length)}</p>
          )}
          {character && notOnModel.length > 0 && (
            <p className="hint color-view-notice">{t("Not on the model in 3D (they still change in game): {materials}. Use the Texture tab for them.", { materials: notOnModel.join(", ") })}</p>
          )}
          {tab === "texture" && scene.status === "failed" && (
            <p className="hint color-view-notice" role="status">
              {hero ? t("The hero's model could not be shown in 3D: {message}", { message: scene.message ?? "" }) : t("Your model could not be shown in 3D: {message}", { message: scene.message ?? "" })}{" "}
              <button type="button" className="extra-text-button" onClick={() => { setAttempt((v) => v + 1); setTab("character"); }}>{t("Try again")}</button>
            </p>
          )}
          <div className={`color-stage ${tab}`} style={glowCss ? ({ "--glow": glowCss } as CSSProperties) : undefined}>
            {stage()}
            {legend && (character || tab === "texture") && <span className={`color-stage-legend ${tab === "texture" ? "right" : ""}`}><i aria-hidden="true" />{legend}</span>}
          </div>
          {foot && <p className="color-view-foot">{foot}</p>}
          {noticeText && <p className="hint color-view-notice" role="status">{noticeText}</p>}
        </section>
        <aside className="color-inspector" aria-label={t("Color editor")}>
          {material ? (
            <ColorEditor
              key={material.id}
              material={material}
              mode={mode}
              disabled={disabled}
              average={average}
              thumbnail={thumbnails[material.id] ?? ""}
              edit={edit}
              remove={() => { apply(adapter.remove(material.id), [material.id]); setPick(null); }}
              coverages={coverages}
              pickCoverage={pickCoverage}
              pick={shownPick}
              pickResult={pickResult}
              startPick={(index) => { setPick({ stage: "point", index, keep: false, note: "" }); setNotice(null); }}
              changeColor={changeColor}
              setPickColor={(to) => setPick((current) => (current?.stage === "choose" ? { ...current, to } : current))}
              finishPick={finishPick}
              cancelPick={cancelPick}
              pickHint={pickHint}
              rangesUnsupported={rangesUnsupported}
              onDrag={drag}
              litRange={litValues && !pickResult ? litRange : null}
              pointRange={(index, active) => {
                if (active) holdRange(index, null);
                else if (!selectDragging.current) holdRange(null, 300);
              }}
              selectRange={(index, active) => {
                if (active === true) { selectDragging.current = true; holdRange(index, null); }
                else if (active === false) { selectDragging.current = false; holdRange(index, 700); }
                else if (!selectDragging.current) holdRange(index, 900);
              }}
              everywhereCoverages={everywhereCoverages}
              pickShares={pickShares}
              setPickScope={(scope) => setPick((current) => (current?.stage === "choose" ? { ...current, scope } : current))}
            />
          ) : (
            <div className="extra-editor-empty">
              <span className="extras-eyebrow">{hero ? t("HERO COLORS") : t("YOUR MODEL'S COLORS")}</span>
              <h2>{t("Choose a material")}</h2>
              <p>
                {hero
                  ? t("Pick one of the hero's materials, in the list or on the character. Recolor some of its colors by clicking them on the character, or change the whole material or its tint. For black clothes, set the tint to black or the brightness to 0%.")
                  : t("Pick one of your model's materials, in the list or on the character. Recolor some of its colors by clicking them on the character, or change the whole material's hue, saturation, brightness or tint.")}
              </p>
              <ul>
                {hero ? (
                  <>
                    <li><ExtraIcon name="check" size={16} />{t("The game's textures and model stay unchanged")}</li>
                    <li><ExtraIcon name="check" size={16} />{t("Only the changed materials go into the mod")}</li>
                    <li><ExtraIcon name="check" size={16} />{t("Shared materials are marked before you change them")}</li>
                  </>
                ) : (
                  <>
                    <li><ExtraIcon name="check" size={16} />{t("The same changes as Appearance → Surface changes")}</li>
                    <li><ExtraIcon name="check" size={16} />{t("Your image files stay unchanged; the build recolors copies")}</li>
                    <li><ExtraIcon name="check" size={16} />{t("The preview uses the build's own color math")}</li>
                  </>
                )}
              </ul>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
