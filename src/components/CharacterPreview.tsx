// i18n-area: physics
import { lazy, Suspense, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { t, tp, useLanguage, useT } from '../i18n';
import { sentences } from '../build/words';
import type { Project } from '../models';
import type { ProjectInspection } from '../build/models';
import type { PreviewScene } from '../preview/packet';
import { decodeMotion, type MotionPacket, type PreviewNode, type Tuning } from '../preview/motion';
import { validPacket, type ClothPacket } from '../preview/clothPacket';
import { api } from '../api';
import { StudioView } from '../studio/Studio';
import type { ModelLabelItem, ModelLabelSet } from '../studio/ModelLabels';
import { PhysicsLabelsContext } from './physicsLabels';
import { PreviewLinkContext, STOPPED_BEFORE_MOVING_PARTS, stoppedBeforeMovingParts } from './previewLink';
import { KIND_LABELS, kindName } from './MotionTuning';
import { hasColorImages, shownTextureMessage, usePreviewTextures } from '../preview/textures';
import { modelIdentity, viewChoices } from '../preview/viewChoices';
import { idOf, useAutoLoad } from '../preview/autoLoad';
const Viewport = lazy(() => import('../preview/Viewport'));
/** The Physics page's preview: its choices are remembered apart from any other preview. */
const VIEW = 'physics';
const MovementViewport = lazy(() => import('../preview/MovementViewport'));

/** The prepared character in 3D (with its gun), shown where its look and motion are
 * chosen: still, or moving through the hero's stand, run and jump clips with its
 * moving parts swinging (from the last Analyze and the Fine-tune sliders). */
export function CharacterPreview({ project, preview, load, busy, report = null, fail, analyze, analyzing = false, autoLoad, previewCurrent }: { project: Project; preview: PreviewScene | null; /** Shows the character; a returned promise ends when that load has finished. */load: () => void | Promise<unknown>; busy: boolean; report?: ProjectInspection | null; fail?: (title: string, error: unknown) => void;
  /** Analyze project (records the moving parts the movement preview swings); runs by itself when Movement opens without them. A returned promise ends when the analysis has finished. */analyze?: () => void | Promise<unknown>; analyzing?: boolean;
  /** Settings → Load 3D previews automatically is on: loads the character (resolves to false when it did not start). */autoLoad?: () => Promise<boolean>;
  /** False when the shown preview belongs to an earlier prepared model of this project (it loads again by itself). */previewCurrent?: boolean }) {
  const t = useT();
  const language = useLanguage();
  const model = modelIdentity(project);
  // Load 3D previews automatically: Movement was the last choice for this prepared model, but its
  // clips cannot be reused (the preview was loaded again): they load by themselves once the
  // character is shown. Read before the effect below forgets the choice; a click on Still or
  // Movement ends it. No Analyze is started for it (Analyze writes the mesh and material lists).
  const [movementWanted, setMovementWanted] = useState(() => viewChoices.get(project, VIEW)?.mode === 'movement');
  // What was shown when the page was last left (this project and prepared model only). A held
  // packet is reused only with the same preview scene; Movement comes back only when its packet
  // is held, so opening the page never loads clips or starts an Analyze.
  const [remembered] = useState(() => {
    const choice = viewChoices.get(project, VIEW);
    const packet = choice?.packet && preview && choice.scene === preview ? choice.packet : null;
    return {shading: choice?.shading ?? null, packet, mode: choice?.mode === 'movement' && packet ? 'movement' as const : 'still' as const};
  });
  const [mode, setMode] = useState<'still' | 'movement'>(remembered.mode);
  const [packet, setPacket] = useState<MotionPacket | null>(remembered.packet);
  const [loading, setLoading] = useState(false);
  /** Solid or textured (or the still view's skin weights), kept across Still and Movement and
   * across page changes for the session. Unset: textured when a colour image is assigned to the
   * shown character's materials. */
  const [shading, setShading] = useState<string | null>(remembered.shading);
  // The prepared model the shown packet belongs to (a late packet for another one is dropped).
  const current = useRef(model);
  const packetModel = useRef(model);
  useEffect(() => {
    if (current.current === model) return;
    current.current = model;
    setPacket(null); setMode('still'); setMovementWanted(false);
  }, [model]);
  useEffect(() => {
    const held = packet && packetModel.current === model ? packet : null;
    viewChoices.set(project, VIEW, {shading, mode: held ? mode : 'still', packet: held, scene: held ? preview : null});
  }, [project.id, model, shading, mode, packet, preview]); // the project matters only through its id and prepared model
  const materials = useMemo(() => preview ? [...new Set(preview.metadata.meshes.map(mesh => mesh.material))] : [], [preview]);
  const images = hasColorImages(project, materials);
  const [failed, setFailed] = useState(false);
  const shown = shading ?? (images && !failed ? 'textured' : 'solid');
  const textures = usePreviewTextures(project, materials, Boolean(preview) && shown === 'textured', undefined, undefined, preview);
  // Nothing loaded: fall back to Solid unless Textures was chosen (the message says why).
  useEffect(() => { setFailed(textures.status === 'failed'); }, [textures.status]);
  /** `auto`: opened by Load 3D previews automatically, with the character already shown; it
   * loads the clips only (the notice still offers Analyze). */
  async function showMovement(auto = false) {
    setMode('movement'); setMovementWanted(false);
    // One backend operation at a time: the character, then Analyze, then the clips. Started
    // together, whichever came second waited two seconds for the first and then failed as
    // "Another operation is running".
    const shown = !preview && !auto ? load() : undefined;
    if (shown instanceof Promise) await shown;
    // The moving parts come from Analyze; run it now rather than show a still character.
    const analysis = !auto && !analyzed && analyze && !busy ? analyze() : undefined;
    if (analysis instanceof Promise) await analysis;
    if (packet) return;
    setLoading(true);
    const asked = model;
    try {
      const loaded = decodeMotion(await api.motionPreview(project));
      if (current.current === asked) { packetModel.current = asked; setPacket(loaded); }
    }
    catch (error) { if (current.current === asked) setMode('still'); fail?.('The movement preview could not be loaded', error); }
    finally { setLoading(false); }
  }
  // Load 3D previews automatically: the character once per prepared model, then Movement when it
  // was the last choice (one clips load, never in a loop: the choice is gone once it starts).
  useAutoLoad({on: Boolean(autoLoad), key: project.dmx && (!preview || previewCurrent === false) ? `dmx:${project.dmx}` : null, busy, load: () => autoLoad?.() ?? false});
  // The clips wait for the textures (each image load and the clips take the backend's operation lock).
  useAutoLoad({on: Boolean(autoLoad) && movementWanted, key: preview && !packet && mode === 'still' && previewCurrent !== false && textures.status !== 'loading' ? `movement:${model}:${idOf(preview)}` : null, busy, load: () => { void showMovement(true); return true; }});
  const analyzed = Boolean(report && 'dmx' in report);
  const nodes: PreviewNode[] = report && 'dmx' in report ? report.secondary_motion?.preview_nodes ?? report.motion_preview_nodes ?? [] : [];
  // The game's cloth model from the last Analyze (Automatic secondary motion only), asked for
  // whenever Movement is shown; none (an older Analyze, another mode) keeps the simple swing.
  const proxy = report && 'dmx' in report && report.secondary_motion?.preset === 'automatic' ? report.secondary_motion.proxy_sha256 || null : null;
  // A new Analyze can write the packet for the same proxy (one analyzed before the packet existed).
  const analysis = useMemo(() => ++analysisCount, [report]);
  const cloth = useClothPacket(project, mode === 'movement' ? proxy : null, analysis);
  const tuning = (project.customization.secondary_motion?.tuning ?? {}) as Record<string, Tuning | undefined>;
  // Tell the page what is shown, so its tabs can say why the sliders are not seen working.
  const link = useContext(PreviewLinkContext);
  const canMove = !busy && Boolean(project.dmx);
  const stopped = stoppedBeforeMovingParts(report, project);
  useEffect(() => { link?.report({mode, loaded: Boolean(preview), nodes: nodes.length, analyzed, analyzing, canMove, ...(stopped ? {stopped} : {})}); }, [link, mode, preview, nodes.length, analyzed, analyzing, canMove, stopped]);
  useEffect(() => () => link?.report(null), [link]);
  useEffect(() => { if (link) link.actions.current = {showMovement: () => void showMovement(), load}; });
  const switcher = <div className="segmented" role="radiogroup" aria-label={t('Character view')}>
    <button type="button" role="radio" aria-checked={mode === 'still'} onClick={() => { setMode('still'); setMovementWanted(false); }}>{t('Still')}</button>
    <button type="button" role="radio" aria-checked={mode === 'movement'} disabled={busy || !project.dmx} onClick={() => void showMovement()}>{t('Movement')}</button>
  </div>;
  // Labels on the character: one per part type the last Analyze recorded, only where the page asks
  // for them (Automatic mode) and only on a loaded preview. Nothing is loaded or analyzed for them.
  const physicsLabels = useContext(PhysicsLabelsContext);
  const labels = useMemo<ModelLabelSet | null>(() => physicsLabels && nodes.length ? partLabels(nodes, physicsLabels.active, physicsLabels.select) : null, [physicsLabels, nodes, language]); // eslint-disable-line react-hooks/exhaustive-deps
  // The movement view puts its toolbar into the view's bar and its notes into the footer.
  const [barSlot, setBarSlot] = useState<HTMLElement | null>(null);
  const [footSlot, setFootSlot] = useState<HTMLElement | null>(null);
  return <StudioView title={t('Your character')} className={`character-preview mode-${mode}`} bar={switcher} barRef={setBarSlot} footRef={setFootSlot}
    barEnd={<button className={preview ? '' : 'primary'} disabled={busy || !project.dmx} onClick={load}>{preview ? t('Reload preview') : t('Show my character')}</button>}
    footer={preview && shading === null && images && failed && textures.message ? <p className="hint">{sentences(t('Showing Solid: the textures could not be loaded.'), shownTextureMessage(textures.message))}</p> : null}>
    {mode === 'movement' && !nodes.length && <div className="capability-notice" role="status"><strong>{analyzing ? t('Analyzing your moving parts…') : stopped ? t('Mesh mappings needed a choice') : analyzed ? t('No moving parts to swing') : t('Moving parts not analyzed yet')}</strong><p>{analyzing ? t('Hair, cloth and tails start swinging here as soon as the analysis finishes.') : stopped ? t(STOPPED_BEFORE_MOVING_PARTS) : analyzed ? t('The analysis found no moving parts for this model: choose Automatic secondary motion under Physics mode and keep hair, cloth and tails as moving parts in Prepare model, then Analyze again.') : t('The movement preview swings the parts recorded by Analyze project; until then the character plays the clips with everything still.')}</p>{analyze && !analyzing && <button className="primary" disabled={busy} onClick={analyze}>{analyzed ? t('Analyze again') : t('Analyze now')}</button>}</div>}
    {mode === 'movement'
      ? (preview && packet ? <Suspense fallback={<p className="hint">{t('Loading the 3D view…')}</p>}><MovementViewport scene={preview} packet={packet} nodes={nodes} tuning={tuning} project={project} shading={shown === 'weights' ? 'solid' : shown} onShading={setShading} shared={textures} toolbarSlot={barSlot} footSlot={footSlot} labels={labels} cloth={cloth.packet} clothNote={nodes.length ? clothNote(proxy, cloth.status) : null} onAnalyze={analyze && !analyzing && !busy ? analyze : undefined}/></Suspense>
        : <p className="hint studio-view-wait">{loading || !preview ? t('Loading your character and the hero’s stand, run and jump clips…') : t('Loading…')}</p>)
      : preview ? <Suspense fallback={<p className="hint">{t('Loading the 3D view…')}</p>}><Viewport scene={preview} project={project} disabled={busy} shading={shown} onShading={setShading} shared={textures} labels={labels} toolbarSlot={barSlot} footSlot={footSlot}/></Suspense>
        : <p className="hint studio-view-wait">{project.dmx ? t('See your prepared character in 3D, with its gun, its texture colours and moving parts. Movement plays the hero’s stand, run and jump clips with hair, cloth and other moving parts swinging.') : t('Prepare your model first (step 2).')}</p>}
  </StudioView>;
}

type ClothStatus = 'idle' | 'loading' | 'ready' | 'none' | 'failed';
/** The cloth packet of `proxy` (a proxy hash from the last Analyze), read once per project and
 * hash; null while there is none. A failure is a state, never an error message. */
let analysisCount = 0;
function useClothPacket(project: Project, proxy: string | null, analysis: number): {packet: ClothPacket | null; status: ClothStatus} {
  type Held = {key: string; proxy: string; text: string | null; packet: ClothPacket | null; status: ClothStatus};
  const [held, setHeld] = useState<Held | null>(null);
  const latest = useRef(project);
  latest.current = project;
  const key = proxy ? `${project.id}:${proxy}:${analysis}` : null;
  const asked = useRef<string | null>(null);
  useEffect(() => {
    if (!key || !proxy || asked.current === key) return;
    asked.current = key;
    // While a new Analyze's packet is read, the one of the same proxy keeps playing.
    setHeld(before => ({key, proxy, text: before?.proxy === proxy ? before.text : null, packet: before?.proxy === proxy ? before.packet : null, status: 'loading'}));
    void Promise.resolve().then(() => api.previewCloth(latest.current, proxy)).then(text => {
      if (asked.current !== key) return;
      setHeld(before => {
        // The same packet again keeps its object, so the running simulation is not started again.
        if (text && before?.text === text && before.packet) return {...before, key, status: 'ready'};
        let parsed: unknown = null;
        try { parsed = text ? JSON.parse(text) : null; } catch { parsed = null; }
        const packet = validPacket(parsed) ? parsed : null;
        return {key, proxy, text: packet ? text : null, packet, status: !text ? 'none' : packet ? 'ready' : 'failed'};
      });
    }).catch(() => { if (asked.current === key) setHeld({key, proxy, text: null, packet: null, status: 'failed'}); });
  }, [key, proxy]);
  if (!key) return {packet: null, status: 'idle'};
  if (held?.key === key) return {packet: held.packet, status: held.status};
  return {packet: held?.proxy === proxy ? held.packet : null, status: 'loading'};
}

/** The quiet line under the movement view when it shows the simple swing instead of the game's cloth model. */
export function clothNote(proxy: string | null, status: ClothStatus): {text: string; analyze: boolean} | null {
  if (!proxy) return {text: t('Moving parts swing by a simple approximation here; the game’s cloth simulation is shown for Automatic secondary motion.'), analyze: false};
  if (status === 'none') return {text: t('Moving parts swing by a simple approximation: Analyze again to see the game’s cloth simulation.'), analyze: true};
  if (status === 'failed') return {text: t('Moving parts swing by a simple approximation: the cloth data of the last Analyze could not be read. Analyze again to see the game’s cloth simulation.'), analyze: true};
  return null;
}

/** One label per recorded part type (in the order of the Fine-tune chips), with how many parts it
 * has, placed at the middle of that type's moving bones. */
export function partLabels(nodes: PreviewNode[], active: string | null, select: (kind: string) => void): ModelLabelSet | null {
  const kinds = new Map<string, {roots: Set<string>; bones: string[]}>();
  for (const node of nodes) {
    if (node.kind === 'rigid') continue;
    const entry = kinds.get(node.kind) ?? {roots: new Set<string>(), bones: []};
    entry.roots.add(node.root); entry.bones.push(node.bone);
    kinds.set(node.kind, entry);
  }
  const order = Object.keys(KIND_LABELS);
  const items: ModelLabelItem[] = [...kinds].sort(([a], [b]) => (order.indexOf(a) + 1 || 99) - (order.indexOf(b) + 1 || 99)).map(([kind, {roots, bones}]) => {
    const text = kind in KIND_LABELS ? kindName(kind) : kind.replaceAll('_', ' ');
    return {id: kind, text, detail: tp(roots.size, {one: '{count} part', other: '{count} parts'}), title: t('Fine-tune {kind}', {kind: text.toLowerCase()}), active: kind === active, bones};
  });
  return items.length ? {items, select, legend: t('Moving parts · click a label to fine-tune that type'), label: t('Moving part types')} : null;
}
