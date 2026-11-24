import { lazy, Suspense, useCallback, useId, useMemo, useState, type ReactNode } from 'react';
import type { PreviewScene } from '../preview/packet';
import type { ModelSummary, Project, ProjectDocument } from '../models';
import { basename } from '../api';
import { Check, SelectField } from './CustomizationPages';
import { FilePathInput } from './FilePathInput';
import { FaceDetails, faceLayerCounts, MaterialEditor, materialStatus, needsLook, NumberField, StaticExpression } from './MaterialEditor';
import { Equipment, equipmentChange, equipmentCount } from './Equipment';
import { WeaponModels } from './WeaponModels';
import { StateLabel } from './Primitives';
import { PathValue } from './Paths';
import type { ProjectInspection } from '../build/models';
import { projectReadiness } from '../build/readiness';
import { StudioActionBar, StudioCard, StudioFrame, StudioTabPanel, StudioTabs, StudioTopBar, StudioView, type StudioTab, type StudioTone } from '../studio/Studio';
import { StudioBarMenuButton, StudioBarMenuPanel, useStudioBarMenus } from '../studio/StudioBarMenu';
import type { ModelLabelItem, ModelLabelSet } from '../studio/ModelLabels';
import { textureRequests, textureStore, usePreviewTextures } from '../preview/textures';
import { faceFrame, materialCenters } from './appearanceView';
import { useAutoLoad } from '../preview/autoLoad';
import './appearance.css';
const Viewport = lazy(() => import('../preview/Viewport'));

type AppearanceTab = 'materials' | 'face' | 'gun' | 'accessories';
type Menu = 'model' | 'mesh' | 'textures' | 'saved';
/** At most this many material labels on the character; with more, only the selected material and
 * the ones Build & install still asks about are labelled. */
const ALL_LABELS = 12;

/** Step 3, in the studio frame: the top bar (the model and four menus: Model, Mesh options, Texture
 * folder, Saved inputs), four tabs (Materials, Face, Gun, Accessories & props) beside the one 3D
 * view, and the action bar. The view always sits in the same slot, so switching tabs never
 * remounts it; the Gun tab shows the gun's own preview in its place (the view stays mounted,
 * hidden). Inactive panels stay mounted (hidden). Nothing loads on page open: the 3D view loads on
 * a click, as before, unless Settings → Load 3D previews automatically is on (`autoLoad`). */
export function Appearance({ document, summary, choose, analyze, busy, drag, update, fail, preview, loadPreview, importPackage, analyzeMaterials, openPreparation, report = null, fresh = false, weapons, expert = true, next, openColors, autoLoad, previewCurrent }: {
  /** The Weapons section (the gun of a prepared model). */ weapons?: React.ReactNode;
  /** Settings → Load 3D previews automatically is on: loads the 3D view (resolves to false when it did not start). */ autoLoad?: () => Promise<boolean>;
  /** False when the shown preview belongs to an earlier prepared model of this project (it loads again by itself). */ previewCurrent?: boolean;
  report?: ProjectInspection | null; fresh?: boolean; openPreparation?: () => void; preview: PreviewScene | null; loadPreview: () => void; importPackage: () => void; analyzeMaterials: () => void;
  document: ProjectDocument | null; summary: ModelSummary | null; choose: () => void;
  update: (project: Project) => void; fail: (title: string, error: unknown) => void;
  analyze: (path: string) => void; busy: boolean; drag: string | null;
  /** Simple mode hides Saved inputs and the model details (nothing in them is a choice). */ expert?: boolean;
  /** Continue to Physics & preview. */ next?: () => void;
  /** Open Extras → Colors (the colour of a material's surface changes is edited there). */ openColors?: () => void;
}) {
  const project = document?.project;
  const current = summary?.path === project?.dmx ? summary : null;
  const readiness = project ? projectReadiness(project, report, fresh) : null;
  const inspection = readiness?.model ?? null;
  const compatibility = readiness?.rows.find(row => row.key === 'skeleton');
  const processingIssue = Boolean(inspection?.vertex_preflight.errors.length);
  const base = useId().replace(/:/g, '');
  const [tab, setTab] = useState<AppearanceTab>('materials');
  const menus = useStudioBarMenus<Menu>(`${base}-appearance`);
  /** The material chosen in the list or on the model; undefined: the first one Build & install
   * still asks about (else the first); null: none (the editor closed). Selecting writes nothing. */
  const [picked, setPicked] = useState<string | null | undefined>(undefined);
  const [shading, setShading] = useState('solid');
  const [only, setOnly] = useState(false);
  const [faceView, setFaceView] = useState(true);
  const [barSlot, setBarSlot] = useState<HTMLElement | null>(null);
  const [footSlot, setFootSlot] = useState<HTMLElement | null>(null);
  const materials = project?.materials ?? {};
  const names = Object.keys(materials);
  const shownMaterial = picked === null ? null : picked && materials[picked] ? picked : names.find(name => needsLook(materials[name])) ?? names[0] ?? null;
  const select = useCallback((name: string | null) => setPicked(name), []);
  const pick = useCallback((name: string) => { setPicked(name); setTab('materials'); }, []);
  // Load 3D previews automatically: the prepared model once per model file, after the page's own
  // start-up work (the saved gun) and never while something else runs.
  useAutoLoad({on: Boolean(autoLoad), key: project?.dmx && (!preview || previewCurrent === false) ? `dmx:${project.dmx}` : null, busy, load: () => autoLoad?.() ?? false});
  const viewMaterials = useMemo(() => preview ? [...new Set(preview.metadata.meshes.map(mesh => mesh.material))] : [], [preview]);
  const textures = usePreviewTextures(project ?? EMPTY, viewMaterials, Boolean(preview && project) && shading === 'textured', undefined, undefined, preview);
  // Pictures in the material list: base-color images the 3D view has loaded (here or on the Physics page); nothing is loaded for them.
  const thumbs = project ? textureStore.cached(project.id, textureRequests(project, names)) : {};
  const centers = useMemo(() => preview ? materialCenters(preview) : null, [preview]);
  const frame = useMemo(() => preview ? faceFrame(preview) : null, [preview]);
  const labels = useMemo<ModelLabelSet | null>(() => {
    if (!centers || !project || tab !== 'materials') return null;
    const all = names.filter(name => centers.has(name));
    const shown = all.length <= ALL_LABELS ? all : all.filter(name => name === shownMaterial || needsLook(materials[name]));
    const items: ModelLabelItem[] = shown.map(name => ({id: name, text: basename(name).replace(/\.vmat$/i, ''), detail: name === shownMaterial ? 'selected' : materialStatus(materials[name]).text.toLowerCase(), title: name, active: name === shownMaterial, point: centers.get(name)}));
    return items.length ? {items, select: pick, label: 'Materials on the character', legend: 'Click a part or a label to choose its material'} : null;
  }, [centers, project, tab, names.join('\u0000'), shownMaterial, materials, pick]); // eslint-disable-line react-hooks/exhaustive-deps
  const review = names.filter(name => needsLook(materials[name])).length;
  const withImage = names.filter(name => materials[name].textures?.color).length;
  const vertices = current ? current.meshes.reduce((count, mesh) => count + mesh.vertices, 0) : 0;
  const shownLayers = project ? faceLayerCounts(project).filter(({kind}) => project.customization.face_layers?.[kind] === true).length : 0;
  const layerKinds = project ? faceLayerCounts(project).length : 0;
  const expression = Boolean(project?.customization.face?.enabled);
  const equipment = project?.customization.equipment ?? {};
  const equipmentRows = project ? equipmentCount(project) : 0;
  const tabs: StudioTab<AppearanceTab>[] = [
    {id: 'materials', label: 'Materials', ...(review ? {status: `${review} to review`, tone: 'attention' as const} : names.length ? {status: 'reviewed', tone: 'ok' as const} : {status: 'none yet', tone: 'quiet' as const})},
    {id: 'face', label: 'Face', ...(shownLayers || expression ? {status: [shownLayers ? `${shownLayers} shown` : '', expression ? 'expression' : ''].filter(Boolean).join(' · '), tone: 'changed' as const} : {status: layerKinds ? 'all hidden' : 'nothing to set', tone: 'quiet' as const})},
    {id: 'gun', label: 'Gun', ...(project?.preparation_report ? {} : {status: 'as in the model', tone: 'quiet' as const})},
    {id: 'accessories', label: 'Accessories & props', ...(equipmentRows ? (equipment.enabled ? {status: `${equipmentRows} change${equipmentRows === 1 ? '' : 's'}`, tone: 'changed' as const} : {status: 'saved, off', tone: 'quiet' as const}) : {status: 'none', tone: 'quiet' as const})},
  ];
  const modelState = current ? (compatibility?.tone === 'ready' ? 'Source checks passed' : '✓ Model read') : project?.dmx ? 'Not analyzed yet' : undefined;
  const modelTone: StudioTone | undefined = current ? 'ok' : project?.dmx ? 'quiet' : undefined;
  const status = !project ? 'Open or create a project first.' : !project.dmx ? 'Choose a replacement model to start.' : current
    ? <>{`${current.meshes.length.toLocaleString()} mesh${current.meshes.length === 1 ? '' : 'es'} · ${vertices.toLocaleString()} vertices · ${names.length} material${names.length === 1 ? '' : 's'} · ${withImage} with a base color image`} · <span>{inspection ? compatibility?.tone === 'ready' ? 'Compiled output is checked during build.' : compatibility?.value : 'Compatibility validation still required'}</span></>
    : 'Selected. Analyze to read mesh and bone details.';
  const menu = (id: Menu, label: string, extra?: {detail?: ReactNode; tone?: StudioTone; reveal?: string}) => {
    const button = <StudioBarMenuButton base={menus.base} id={id} open={menus.open === id} onToggle={() => menus.toggle(id)} label={label} detail={extra?.detail} tone={extra?.tone}/>;
    // A link from Build & install (`revealSection`) opens the menu through this wrapper.
    return extra?.reveal ? <span key={id} id={extra.reveal} data-reveal-opens="" className="appearance-menu-target">{button}</span> : <span key={id} className="appearance-menu-target">{button}</span>;
  };
  const close = (id: Menu) => () => menus.close(id);
  const preparationLink = <p className="hint appearance-prepare-link">Need to prepare Blender, FBX, GLB, glTF or MMD?{openPreparation && <> <button className="tertiary" disabled={busy} onClick={openPreparation}>Open Model preparation →</button></>}</p>;
  const panel = project && menus.open ? {
    model: <StudioBarMenuPanel base={menus.base} id="model" title="Model" onClose={close('model')} className="appearance-menu-model">
      {project.dmx ? <>
        <div className="appearance-model-state">{current ? <StateLabel tone={compatibility?.tone === 'ready' ? 'ready' : 'neutral'}>{`${basename(project.dmx)} · ${compatibility?.tone === 'ready' ? 'source checks passed' : 'model read'}`}</StateLabel> : <StateLabel tone="neutral">{`${basename(project.dmx)} · not analyzed yet`}</StateLabel>}
          {current && <p className="model-counts">{current.bones.length.toLocaleString()} bones · {current.meshes.length.toLocaleString()} meshes · {current.encoding} / {current.format} {current.format_version}</p>}</div>
        <div className="row"><button disabled={busy} onClick={choose}>Replace…</button><button className={current ? '' : 'primary'} disabled={busy} onClick={() => analyze(project.dmx)}>Analyze model</button><button disabled={busy} onClick={importPackage}>Import source package…</button></div>
        <p className="hint">Drop another model here to replace it.</p>
        {preparationLink}
        {expert && <details className="technical"><summary>View model details</summary><dl><dt>Source</dt><dd><PathValue path={project.dmx}/></dd>{current && <><dt>Encoding</dt><dd>{current.encoding}</dd><dt>Format</dt><dd>{current.format} {current.format_version}</dd><dt>Elements</dt><dd>{current.elements.toLocaleString()}</dd><dt>Vertices</dt><dd>{vertices.toLocaleString()}</dd></>}</dl>{current && <div className="table-scroll"><table><thead><tr><th>Mesh</th><th>Vertices</th><th>Skin weights</th></tr></thead><tbody>{current.meshes.map(mesh => <tr key={mesh.element}><td>{mesh.name}</td><td className="numeric">{mesh.vertices.toLocaleString()}</td><td>{mesh.has_skin_weights ? 'Present' : 'Not present'}</td></tr>)}</tbody></table></div>}</details>}
      </> : <><p className="hint">No replacement model yet: choose one in the view below.</p>{preparationLink}</>}
    </StudioBarMenuPanel>,
    mesh: <StudioBarMenuPanel base={menus.base} id="mesh" title="Mesh options" onClose={close('mesh')}>
      <div className="setting-summary"><div><StateLabel tone={processingIssue ? 'attention' : project.auto_split ? 'ready' : 'neutral'}>{processingIssue ? 'Mesh processing needs attention' : project.auto_split ? 'Automatic splitting enabled' : 'Automatic splitting disabled'}</StateLabel><p>{processingIssue ? inspection?.vertex_preflight.errors[0] : project.auto_split ? 'Oversized meshes will be split automatically.' : 'Keeps meshes as supplied. Analyze the project to check the engine limits.'}</p></div></div>
      <div className="appearance-mesh-options">
        <Check label="Automatically split oversized meshes" value={project.auto_split} onChange={auto_split => update({ ...project, auto_split })} disabled={busy} />
        {project.auto_split && <NumberField label="Target vertices per mesh part" value={project.split_target} min={3} max={524287} step={1} update={split_target => update({ ...project, split_target })} disabled={busy} />}
        <SelectField label="Detail levels" value={project.lod_strategy} values={[["reuse", "Reuse replacement at original levels"], ["preserve", "Preserve original lower-detail meshes"], ["choose", "Choose per mapping"], ["single", "Single level"], ["disable", "Disable additional levels"], ["custom", "Custom mapping"]]} onChange={lod_strategy => update({ ...project, lod_strategy })} disabled={busy} />
        <Check label="Automatically map replacement meshes" value={project.auto_map} onChange={auto_map => update({ ...project, auto_map })} disabled={busy} />
        <Check label="Allow unskinned geometry" value={project.allow_unskinned} onChange={allow_unskinned => update({ ...project, allow_unskinned })} disabled={busy} />
      </div>
      <p className="hint">The engine vertex limit and all build validation still apply.</p>
    </StudioBarMenuPanel>,
    textures: <StudioBarMenuPanel base={menus.base} id="textures" title="Texture folder" onClose={close('textures')}>
      {project.textures && <p className="texture-count">{(() => { const count = new Set(Object.values(project.materials).flatMap(material => Object.values(material.textures ?? {}).filter(Boolean))).size; return count ? `${count} texture${count === 1 ? '' : 's'} assigned` : 'Folder selected · analyze materials to assign textures'; })()}</p>}
      <FilePathInput label="Texture folder" kind="texture_directory" value={project.textures} update={textures => update({ ...project, textures })} disabled={busy} fail={fail} />
      {project.textures && <button className="tertiary" disabled={busy} onClick={() => { menus.close('textures'); setTab('materials'); requestAnimationFrame(() => window.document.querySelector<HTMLElement>('.materials-studio .material-search')?.focus()); }}>Manage textures</button>}
    </StudioBarMenuPanel>,
    saved: <StudioBarMenuPanel base={menus.base} id="saved" title="Saved inputs" onClose={close('saved')}>
      <dl className="summary-list"><dt>Textures</dt><dd><PathValue path={project.textures} empty="No texture folder selected" /></dd><dt>Material assignments</dt><dd>{Object.keys(project.materials).length}</dd><dt>Model mappings</dt><dd>{Object.keys(project.meshes).length}</dd></dl>
      <p className="hint">These assignments are saved with the project and checked during preparation and build.</p>
    </StudioBarMenuPanel>,
  }[menus.open] : null;
  const chooseCard = <div id="replacement-model" className={`appearance-choose drop-zone ${drag ? 'drop-active' : ''}`}><div className="drop-symbol" aria-hidden="true">↓</div><h2>{drag ?? 'Add a replacement model'}</h2><p>Drop a game-ready DMX here or choose one manually.</p>
    <div className="row"><button className="primary" disabled={busy || !document} onClick={choose}>Choose model…</button><button disabled={busy || !document} onClick={importPackage}>Import source package…</button></div>{preparationLink}</div>;
  const viewChoice = tab === 'face'
    ? <div className="studio-view-group" role="group" aria-label="View"><button type="button" aria-pressed={faceView && Boolean(frame)} disabled={!frame} title={frame ? undefined : 'No head bone found: the whole model is shown'} onClick={() => setFaceView(true)}>Face</button><button type="button" aria-pressed={!faceView || !frame} onClick={() => setFaceView(false)}>Whole model</button></div>
    : tab === 'materials' ? <div className="studio-view-group" role="group" aria-label="Show only"><button type="button" aria-pressed={!only} onClick={() => setOnly(false)}>Whole model</button><button type="button" aria-pressed={only} disabled={!shownMaterial} onClick={() => setOnly(true)}>Selected material</button></div> : null;
  const view = <StudioView title={tab === 'face' ? 'Face' : '3D view'} className="appearance-view" bar={project?.dmx && preview ? viewChoice : undefined} barRef={setBarSlot} footRef={setFootSlot}
    barEnd={project?.dmx ? <button className={preview ? '' : 'primary'} disabled={busy} onClick={loadPreview}>{preview ? 'Reload preview' : 'Load preview'}</button> : undefined}
    footer={preview && tab === 'materials' ? <p className="hint">Click a part of the character to choose its material.</p> : null}>
    {!project?.dmx ? chooseCard
      : preview ? <Suspense fallback={<p className="hint studio-view-wait">Loading 3D renderer…</p>}><Viewport scene={preview} project={project} disabled={busy} shading={shading} onShading={setShading} shared={textures} labels={labels} toolbarSlot={barSlot} footSlot={footSlot}
          onPick={tab === 'materials' ? pick : undefined} only={tab === 'materials' && only ? shownMaterial : null} marked={tab === 'materials' ? shownMaterial : null} frame={tab === 'face' && faceView ? frame : null}/></Suspense>
      : <div className="preview-empty"><span aria-hidden="true">◇</span><h3>Inspect your replacement</h3><p>Load the model to check its shape, meshes and weights.</p><p className="hint">Rest-pose preview · cloth simulation runs in the game or ModelDoc.</p></div>}
  </StudioView>;
  const action = !project?.dmx ? {tone: 'attention' as const, title: 'Add a replacement model', text: 'Drop a game-ready DMX on the page or choose one. Prepare Blender, FBX, GLB, glTF or MMD files in Prepare model first.'}
    : !current ? {tone: 'quiet' as const, title: 'Model not analyzed yet', text: 'Analyze model reads its meshes and bones.'}
    : review ? {tone: 'attention' as const, title: `${review} material${review === 1 ? '' : 's'} still to review`, text: 'Check each one or use Accept all. Build & install asks for them.'}
    : names.length ? {tone: 'ok' as const, title: 'Everything reviewed', text: 'Your material choices are saved with the project.'}
    : {tone: 'quiet' as const, title: 'No materials yet', text: 'Analyze materials finds the images of your model’s materials.'};
  const weaponModels = project ? <WeaponModels project={project} variants={equipment.variants ?? []} change={variants => equipmentChange(project, update)({enabled: true, variants})} disabled={busy} fail={fail}/> : null;
  return <StudioFrame label="Appearance & weapons" className={`appearance-studio tab-${tab}${tab === 'gun' && !project?.preparation_report ? '-plain' : ''}`}>
    <h1 className="sr-only">Appearance &amp; weapons</h1>
    <StudioTopBar title={project?.dmx ? basename(project.dmx) : 'No replacement model yet'} badge={modelState} badgeTone={modelTone} status={status} below={panel}>
      {project && <>
        {menu('model', 'Model', project.dmx ? {reveal: 'replacement-model'} : {})}
        {menu('mesh', 'Mesh options', {reveal: 'model-processing', ...(processingIssue ? {tone: 'attention' as const} : {})})}
        {menu('textures', 'Texture folder', {reveal: 'textures'})}
        {expert && menu('saved', 'Saved inputs')}
      </>}
    </StudioTopBar>
    {drag && project?.dmx && <p className="appearance-drop-note" role="status">{drag}</p>}
    {project && <StudioTabs label="Appearance & weapons sections" base={base} tabs={tabs} active={tab} onSelect={setTab}/>}
    <div className={`appearance-grid ${project ? '' : 'no-project'}`}>
      <div className="appearance-view-slot">{view}</div>
      {project && <div className="appearance-panels">
        <StudioTabPanel base={base} id="materials" active={tab === 'materials'}>
          <MaterialEditor checked={Boolean(inspection)} analyze={analyzeMaterials} project={project} update={update} disabled={busy} fail={fail} selected={shownMaterial} select={select} thumbs={thumbs} openColors={openColors} labelHint={preview ? 'Or click a part of the character.' : undefined}/>
        </StudioTabPanel>
        <StudioTabPanel base={base} id="face" active={tab === 'face'}>
          {layerKinds ? <FaceDetails project={project} update={update} disabled={busy}/> : <StudioCard title="Face details"><p className="studio-note">Your model has no hidden face layers (such as blush or tears that an expression would show).</p></StudioCard>}
          <StaticExpression project={project} update={update} disabled={busy}/>
        </StudioTabPanel>
        <StudioTabPanel base={base} id="gun" active={tab === 'gun'} className="appearance-gun">
          {weapons}
          <StudioCard title="Heroes whose gun is its own model" className="appearance-weapon-models">{weaponModels}</StudioCard>
        </StudioTabPanel>
        <StudioTabPanel base={base} id="accessories" active={tab === 'accessories'}>
          <Equipment project={project} update={update} disabled={busy} fail={fail} summary={current}/>
        </StudioTabPanel>
      </div>}
    </div>
    <StudioActionBar tone={action.tone} title={action.title} text={action.text}>
      {project?.dmx && <button type="button" className={current ? '' : 'primary'} disabled={busy} onClick={() => analyze(project.dmx)}>Analyze model</button>}
      {next && project?.dmx && <button type="button" className={current ? 'primary' : ''} onClick={next}>Continue to Physics &amp; preview <span aria-hidden="true">→</span></button>}
    </StudioActionBar>
  </StudioFrame>;
}
const EMPTY = {id: '', materials: {}, customization: {}} as unknown as Project;
