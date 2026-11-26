// i18n-area: appearance
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
import { mc, tc, tp, tpc, useLanguage, useT } from '../i18n';
import './appearance.css';
const Viewport = lazy(() => import('../preview/Viewport'));

type AppearanceTab = 'materials' | 'face' | 'gun' | 'accessories';
type Menu = 'model' | 'mesh' | 'textures' | 'saved';
/** At most this many material labels on the character; with more, only the selected material and
 * the ones Build & install still asks about are labelled. */
const ALL_LABELS = 12;
/** A material status word as the label on the character shows it (lower case in English). */
const LABEL_WORDS: Record<string, string> = {
  'Reviewed': mc('material label', 'reviewed'), 'Keep source VMAT': mc('material label', 'keep source vmat'), 'Saved': mc('material label', 'saved'),
  'Choose an image': mc('material label', 'choose an image'), 'No image found': mc('material label', 'no image found'), 'Needs review': mc('material label', 'needs review'),
};
const labelWord = (status: string) => tc('material label', LABEL_WORDS[status] ?? status.toLowerCase());
/** Detail-level choices: the saved value and its shown name. */
const LOD_CHOICES: [string, string][] = [["reuse", mc('detail levels', 'Reuse replacement at original levels')], ["preserve", mc('detail levels', 'Preserve original lower-detail meshes')], ["choose", mc('detail levels', 'Choose per mapping')], ["single", mc('detail levels', 'Single level')], ["disable", mc('detail levels', 'Disable additional levels')], ["custom", mc('detail levels', 'Custom mapping')]];

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
  const t = useT();
  const language = useLanguage();
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
    const items: ModelLabelItem[] = shown.map(name => ({id: name, text: basename(name).replace(/\.vmat$/i, ''), detail: name === shownMaterial ? t('selected') : labelWord(materialStatus(materials[name]).text), title: name, active: name === shownMaterial, point: centers.get(name)}));
    return items.length ? {items, select: pick, label: t('Materials on the character'), legend: t('Click a part or a label to choose its material')} : null;
  }, [centers, project, tab, names.join('\u0000'), shownMaterial, materials, pick, language]); // eslint-disable-line react-hooks/exhaustive-deps
  const review = names.filter(name => needsLook(materials[name])).length;
  const withImage = names.filter(name => materials[name].textures?.color).length;
  const vertices = current ? current.meshes.reduce((count, mesh) => count + mesh.vertices, 0) : 0;
  const shownLayers = project ? faceLayerCounts(project).filter(({kind}) => project.customization.face_layers?.[kind] === true).length : 0;
  const layerKinds = project ? faceLayerCounts(project).length : 0;
  const expression = Boolean(project?.customization.face?.enabled);
  const equipment = project?.customization.equipment ?? {};
  const equipmentRows = project ? equipmentCount(project) : 0;
  const tabs: StudioTab<AppearanceTab>[] = [
    {id: 'materials', label: t('Materials'), ...(review ? {status: tp(review, {other: '{count} to review'}), tone: 'attention' as const} : names.length ? {status: t('reviewed'), tone: 'ok' as const} : {status: t('none yet'), tone: 'quiet' as const})},
    {id: 'face', label: t('Face'), ...(shownLayers || expression ? {status: [shownLayers ? tp(shownLayers, {other: '{count} shown'}) : '', expression ? t('expression') : ''].filter(Boolean).join(' · '), tone: 'changed' as const} : {status: layerKinds ? t('all hidden') : t('nothing to set'), tone: 'quiet' as const})},
    {id: 'gun', label: t('Gun'), ...(project?.preparation_report ? {} : {status: t('as in the model'), tone: 'quiet' as const})},
    {id: 'accessories', label: t('Accessories & props'), ...(equipmentRows ? (equipment.enabled ? {status: tp(equipmentRows, {one: '{count} change', other: '{count} changes'}), tone: 'changed' as const} : {status: t('saved, off'), tone: 'quiet' as const}) : {status: tc('accessories', 'none'), tone: 'quiet' as const})},
  ];
  const modelState = current ? (compatibility?.tone === 'ready' ? t('Source checks passed') : t('✓ Model read')) : project?.dmx ? t('Not analyzed yet') : undefined;
  const modelTone: StudioTone | undefined = current ? 'ok' : project?.dmx ? 'quiet' : undefined;
  const status = !project ? t('Open or create a project first.') : !project.dmx ? t('Choose a replacement model to start.') : current
    ? <>{[tp(current.meshes.length, {one: '{count} mesh', other: '{count} meshes'}, {count: current.meshes.length.toLocaleString()}), tp(vertices, {other: '{count} vertices'}, {count: vertices.toLocaleString()}), tp(names.length, {one: '{count} material', other: '{count} materials'}), tp(withImage, {other: '{count} with a base color image'})].join(' · ')} · <span>{inspection ? compatibility?.tone === 'ready' ? t('Compiled output is checked during build.') : compatibility?.value : t('Compatibility validation still required')}</span></>
    : t('Selected. Analyze to read mesh and bone details.');
  const menu = (id: Menu, label: string, extra?: {detail?: ReactNode; tone?: StudioTone; reveal?: string}) => {
    const button = <StudioBarMenuButton base={menus.base} id={id} open={menus.open === id} onToggle={() => menus.toggle(id)} label={label} detail={extra?.detail} tone={extra?.tone}/>;
    // A link from Build & install (`revealSection`) opens the menu through this wrapper.
    return extra?.reveal ? <span key={id} id={extra.reveal} data-reveal-opens="" className="appearance-menu-target">{button}</span> : <span key={id} className="appearance-menu-target">{button}</span>;
  };
  const close = (id: Menu) => () => menus.close(id);
  const preparationLink = <p className="hint appearance-prepare-link">{t('Need to prepare Blender, FBX, GLB, glTF or MMD?')}{openPreparation && <> <button className="tertiary" disabled={busy} onClick={openPreparation}>{t('Open Model preparation →')}</button></>}</p>;
  const panel = project && menus.open ? {
    model: <StudioBarMenuPanel base={menus.base} id="model" title={t('Model')} onClose={close('model')} className="appearance-menu-model">
      {project.dmx ? <>
        <div className="appearance-model-state">{current ? <StateLabel tone={compatibility?.tone === 'ready' ? 'ready' : 'neutral'}>{compatibility?.tone === 'ready' ? t('{file} · source checks passed', {file: basename(project.dmx)}) : t('{file} · model read', {file: basename(project.dmx)})}</StateLabel> : <StateLabel tone="neutral">{t('{file} · not analyzed yet', {file: basename(project.dmx)})}</StateLabel>}
          {current && <p className="model-counts">{tpc('model counts', current.bones.length, {other: '{count} bones'}, {count: current.bones.length.toLocaleString()})} · {tpc('model counts', current.meshes.length, {other: '{count} meshes'}, {count: current.meshes.length.toLocaleString()})} ·{current.encoding} / {current.format} {current.format_version}</p>}</div>
        <div className="row"><button disabled={busy} onClick={choose}>{t('Replace…')}</button><button className={current ? '' : 'primary'} disabled={busy} onClick={() => analyze(project.dmx)}>{t('Analyze model')}</button><button disabled={busy} onClick={importPackage}>{t('Import source package…')}</button></div>
        <p className="hint">{t('Drop another model here to replace it.')}</p>
        {preparationLink}
        {expert && <details className="technical"><summary>{t('View model details')}</summary><dl><dt>{t('Source')}</dt><dd><PathValue path={project.dmx}/></dd>{current && <><dt>{t('Encoding')}</dt><dd>{current.encoding}</dd><dt>{t('Format')}</dt><dd>{current.format} {current.format_version}</dd><dt>{t('Elements')}</dt><dd>{current.elements.toLocaleString()}</dd><dt>{t('Vertices')}</dt><dd>{vertices.toLocaleString()}</dd></>}</dl>{current && <div className="table-scroll"><table><thead><tr><th>{t('Mesh')}</th><th>{t('Vertices')}</th><th>{t('Skin weights')}</th></tr></thead><tbody>{current.meshes.map(mesh => <tr key={mesh.element}><td>{mesh.name}</td><td className="numeric">{mesh.vertices.toLocaleString()}</td><td>{mesh.has_skin_weights ? t('Present') : t('Not present')}</td></tr>)}</tbody></table></div>}</details>}
      </> : <><p className="hint">{t('No replacement model yet: choose one in the view below.')}</p>{preparationLink}</>}
    </StudioBarMenuPanel>,
    mesh: <StudioBarMenuPanel base={menus.base} id="mesh" title={t('Mesh options')} onClose={close('mesh')}>
      <div className="setting-summary"><div><StateLabel tone={processingIssue ? 'attention' : project.auto_split ? 'ready' : 'neutral'}>{processingIssue ? t('Mesh processing needs attention') : project.auto_split ? t('Automatic splitting enabled') : t('Automatic splitting disabled')}</StateLabel><p>{processingIssue ? inspection?.vertex_preflight.errors[0] : project.auto_split ? t('Oversized meshes will be split automatically.') : t('Keeps meshes as supplied. Analyze the project to check the engine limits.')}</p></div></div>
      <div className="appearance-mesh-options">
        <Check label={t('Automatically split oversized meshes')} value={project.auto_split} onChange={auto_split => update({ ...project, auto_split })} disabled={busy} />
        {project.auto_split && <NumberField label={t('Target vertices per mesh part')} value={project.split_target} min={3} max={524287} step={1} update={split_target => update({ ...project, split_target })} disabled={busy} />}
        <SelectField label={t('Detail levels')} value={project.lod_strategy} values={LOD_CHOICES.map(([value, text]) => [value, tc('detail levels', text)] as const)} onChange={lod_strategy => update({ ...project, lod_strategy })} disabled={busy} />
        <Check label={t('Automatically map replacement meshes')} value={project.auto_map} onChange={auto_map => update({ ...project, auto_map })} disabled={busy} />
        <Check label={t('Allow unskinned geometry')} value={project.allow_unskinned} onChange={allow_unskinned => update({ ...project, allow_unskinned })} disabled={busy} />
      </div>
      <p className="hint">{t('The engine vertex limit and all build validation still apply.')}</p>
    </StudioBarMenuPanel>,
    textures: <StudioBarMenuPanel base={menus.base} id="textures" title={t('Texture folder')} onClose={close('textures')}>
      {project.textures && <p className="texture-count">{(() => { const count = new Set(Object.values(project.materials).flatMap(material => Object.values(material.textures ?? {}).filter(Boolean))).size; return count ? tp(count, {one: '{count} texture assigned', other: '{count} textures assigned'}) : t('Folder selected · analyze materials to assign textures'); })()}</p>}
      <FilePathInput label={t('Texture folder')} name="Texture folder" kind="texture_directory" value={project.textures} update={textures => update({ ...project, textures })} disabled={busy} fail={fail} />
      {project.textures && <button className="tertiary" disabled={busy} onClick={() => { menus.close('textures'); setTab('materials'); requestAnimationFrame(() => window.document.querySelector<HTMLElement>('.materials-studio .material-search')?.focus()); }}>{t('Manage textures')}</button>}
    </StudioBarMenuPanel>,
    saved: <StudioBarMenuPanel base={menus.base} id="saved" title={t('Saved inputs')} onClose={close('saved')}>
      <dl className="summary-list"><dt>{t('Textures')}</dt><dd><PathValue path={project.textures} empty={t('No texture folder selected')} /></dd><dt>{t('Material assignments')}</dt><dd>{Object.keys(project.materials).length}</dd><dt>{t('Model mappings')}</dt><dd>{Object.keys(project.meshes).length}</dd></dl>
      <p className="hint">{t('These assignments are saved with the project and checked during preparation and build.')}</p>
    </StudioBarMenuPanel>,
  }[menus.open] : null;
  const chooseCard = <div id="replacement-model" className={`appearance-choose drop-zone ${drag ? 'drop-active' : ''}`}><div className="drop-symbol" aria-hidden="true">↓</div><h2>{drag ?? t('Add a replacement model')}</h2><p>{t('Drop a game-ready DMX here or choose one manually.')}</p>
    <div className="row"><button className="primary" disabled={busy || !document} onClick={choose}>{t('Choose model…')}</button><button disabled={busy || !document} onClick={importPackage}>{t('Import source package…')}</button></div>{preparationLink}</div>;
  const viewChoice = tab === 'face'
    ? <div className="studio-view-group" role="group" aria-label={tc('camera', 'View')}><button type="button" aria-pressed={faceView && Boolean(frame)} disabled={!frame} title={frame ? undefined : t('No head bone found: the whole model is shown')} onClick={() => setFaceView(true)}>{t('Face')}</button><button type="button" aria-pressed={!faceView || !frame} onClick={() => setFaceView(false)}>{t('Whole model')}</button></div>
    : tab === 'materials' ? <div className="studio-view-group" role="group" aria-label={t('Show only')}><button type="button" aria-pressed={!only} onClick={() => setOnly(false)}>{t('Whole model')}</button><button type="button" aria-pressed={only} disabled={!shownMaterial} onClick={() => setOnly(true)}>{t('Selected material')}</button></div> : null;
  const view = <StudioView title={tab === 'face' ? t('Face') : t('3D view')} className="appearance-view" bar={project?.dmx && preview ? viewChoice : undefined} barRef={setBarSlot} footRef={setFootSlot}
    barEnd={project?.dmx ? <button className={preview ? '' : 'primary'} disabled={busy} onClick={loadPreview}>{preview ? t('Reload preview') : t('Load preview')}</button> : undefined}
    footer={preview && tab === 'materials' ? <p className="hint">{t('Click a part of the character to choose its material.')}</p> : null}>
    {!project?.dmx ? chooseCard
      : preview ? <Suspense fallback={<p className="hint studio-view-wait">{t('Loading 3D renderer…')}</p>}><Viewport scene={preview} project={project} disabled={busy} shading={shading} onShading={setShading} shared={textures} labels={labels} toolbarSlot={barSlot} footSlot={footSlot}
          onPick={tab === 'materials' ? pick : undefined} only={tab === 'materials' && only ? shownMaterial : null} marked={tab === 'materials' ? shownMaterial : null} frame={tab === 'face' && faceView ? frame : null}/></Suspense>
      : <div className="preview-empty"><span aria-hidden="true">◇</span><h3>{t('Inspect your replacement')}</h3><p>{t('Load the model to check its shape, meshes and weights.')}</p><p className="hint">{t('Rest-pose preview · cloth simulation runs in the game or ModelDoc.')}</p></div>}
  </StudioView>;
  const action = !project?.dmx ? {tone: 'attention' as const, title: t('Add a replacement model'), text: t('Drop a game-ready DMX on the page or choose one. Prepare Blender, FBX, GLB, glTF or MMD files in Prepare model first.')}
    : !current ? {tone: 'quiet' as const, title: t('Model not analyzed yet'), text: t('Analyze model reads its meshes and bones.')}
    : review ? {tone: 'attention' as const, title: tp(review, {one: '{count} material still to review', other: '{count} materials still to review'}), text: t('Check each one or use Accept all. Build & install asks for them.')}
    : names.length ? {tone: 'ok' as const, title: t('Everything reviewed'), text: t('Your material choices are saved with the project.')}
    : {tone: 'quiet' as const, title: t('No materials yet'), text: t('Analyze materials finds the images of your model’s materials.')};
  const weaponModels = project ? <WeaponModels project={project} variants={equipment.variants ?? []} change={variants => equipmentChange(project, update)({enabled: true, variants})} disabled={busy} fail={fail}/> : null;
  return <StudioFrame label={t('Appearance & weapons')} className={`appearance-studio tab-${tab}${tab === 'gun' && !project?.preparation_report ? '-plain' : ''}`}>
    <h1 className="sr-only">{t('Appearance & weapons')}</h1>
    <StudioTopBar title={project?.dmx ? basename(project.dmx) : t('No replacement model yet')} badge={modelState} badgeTone={modelTone} status={status} below={panel}>
      {project && <>
        {menu('model', t('Model'), project.dmx ? {reveal: 'replacement-model'} : {})}
        {menu('mesh', t('Mesh options'), {reveal: 'model-processing', ...(processingIssue ? {tone: 'attention' as const} : {})})}
        {menu('textures', t('Texture folder'), {reveal: 'textures'})}
        {expert && menu('saved', t('Saved inputs'))}
      </>}
    </StudioTopBar>
    {drag && project?.dmx && <p className="appearance-drop-note" role="status">{drag}</p>}
    {project && <StudioTabs label={t('Appearance & weapons sections')} base={base} tabs={tabs} active={tab} onSelect={setTab}/>}
    <div className={`appearance-grid ${project ? '' : 'no-project'}`}>
      <div className="appearance-view-slot">{view}</div>
      {project && <div className="appearance-panels">
        <StudioTabPanel base={base} id="materials" active={tab === 'materials'}>
          <MaterialEditor checked={Boolean(inspection)} analyze={analyzeMaterials} project={project} update={update} disabled={busy} fail={fail} selected={shownMaterial} select={select} thumbs={thumbs} openColors={openColors} labelHint={preview ? t('Or click a part of the character.') : undefined}/>
        </StudioTabPanel>
        <StudioTabPanel base={base} id="face" active={tab === 'face'}>
          {layerKinds ? <FaceDetails project={project} update={update} disabled={busy}/> : <StudioCard title={t('Face details')}><p className="studio-note">{t('Your model has no hidden face layers (such as blush or tears that an expression would show).')}</p></StudioCard>}
          <StaticExpression project={project} update={update} disabled={busy}/>
        </StudioTabPanel>
        <StudioTabPanel base={base} id="gun" active={tab === 'gun'} className="appearance-gun">
          {weapons}
          <StudioCard title={t('Heroes whose gun is its own model')} className="appearance-weapon-models">{weaponModels}</StudioCard>
        </StudioTabPanel>
        <StudioTabPanel base={base} id="accessories" active={tab === 'accessories'}>
          <Equipment project={project} update={update} disabled={busy} fail={fail} summary={current}/>
        </StudioTabPanel>
      </div>}
    </div>
    <StudioActionBar tone={action.tone} title={action.title} text={action.text}>
      {project?.dmx && <button type="button" className={current ? '' : 'primary'} disabled={busy} onClick={() => analyze(project.dmx)}>{t('Analyze model')}</button>}
      {next && project?.dmx && <button type="button" className={current ? 'primary' : ''} onClick={next}>{t('Continue to Physics & preview')} <span aria-hidden="true">→</span></button>}
    </StudioActionBar>
  </StudioFrame>;
}
const EMPTY = {id: '', materials: {}, customization: {}} as unknown as Project;
