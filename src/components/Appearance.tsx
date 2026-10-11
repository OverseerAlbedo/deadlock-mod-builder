import { lazy, Suspense } from 'react';
import type { PreviewScene } from '../preview/packet';
const Viewport = lazy(() => import('../preview/Viewport'));
import type { ModelSummary, Project, ProjectDocument } from '../models';
import { basename } from '../api';
import { Check, SelectField } from './CustomizationPages';
import { FilePathInput } from './FilePathInput';
import { AppearanceOverrides, MaterialEditor, NumberField } from './MaterialEditor';
import { Equipment } from './Equipment';
import { Disclosure, PageTitle, Section, StateLabel } from './Primitives';
import { PathValue } from './Paths';
import type { ProjectInspection } from '../build/models';
import { projectReadiness } from '../build/readiness';
import { revealSection } from './workflowNavigation';

export function Appearance({ document, summary, choose, analyze, busy, drag, update, fail, preview, loadPreview, importPackage, analyzeMaterials, openPreparation, report = null, fresh = false }: {
  report?: ProjectInspection | null; fresh?: boolean; openPreparation?: () => void; preview: PreviewScene | null; loadPreview: () => void; importPackage: () => void; analyzeMaterials: () => void;
  document: ProjectDocument | null; summary: ModelSummary | null; choose: () => void;
  update: (project: Project) => void; fail: (title: string, error: unknown) => void;
  analyze: (path: string) => void; busy: boolean; drag: string | null;
}) {
  const project = document?.project;
  const current = summary?.path === project?.dmx ? summary : null;
  const inspection = project ? projectReadiness(project, report, fresh).model : null;
  const compatibility = project ? projectReadiness(project, report, fresh).rows.find(row => row.key === 'skeleton') : null;
  const processingIssue = Boolean(inspection?.vertex_preflight.errors.length);
  const textureCount = project ? new Set(Object.values(project.materials).flatMap(material => Object.values(material.textures ?? {}).filter(Boolean))).size : 0;
  return <>
    <PageTitle eyebrow="Model & materials" title="Appearance" subtitle="Add your model, inspect it, then review textures and materials." action={<button disabled={busy || !document} onClick={importPackage}>Import source package…</button>} />
    <div className={project?.dmx ? 'appearance-model-grid' : ''}>
      <Section title="Replacement model" id="replacement-model" className="model-selection" actions={project?.dmx ? <button disabled={busy} onClick={choose}>Replace…</button> : undefined}>
        {project?.dmx ? <div className={`model-file-panel ${drag ? 'drop-active' : ''}`}>
          <div className="model-file-heading"><div className="file-emblem" aria-hidden="true">◇</div><div><strong title={project.dmx}>{basename(project.dmx)}</strong><span className="muted">Replacement model</span></div></div>
          {current ? <>{compatibility?.tone==='ready'?<StateLabel tone="ready">Source checks passed</StateLabel>:<span className="status status-succeeded">✓ Model read</span>}<p className="model-counts">{current.bones.length.toLocaleString()} bones · {current.meshes.length.toLocaleString()} meshes · {current.encoding} / {current.format} {current.format_version}</p><p className="hint">{inspection?compatibility?.tone==='ready'?'Compiled output is checked during build.':compatibility?.value:'Compatibility validation still required'}</p></> : <p className="hint">Selected. Analyze to read mesh and bone details.</p>}
          <div className="row"><button className={current ? '' : 'primary'} disabled={busy} onClick={() => analyze(project.dmx)}>Analyze model</button></div>
          <details className="technical"><summary>View model details</summary><dl><dt>Source</dt><dd><PathValue path={project.dmx}/></dd>{current && <><dt>Encoding</dt><dd>{current.encoding}</dd><dt>Format</dt><dd>{current.format} {current.format_version}</dd><dt>Elements</dt><dd>{current.elements.toLocaleString()}</dd><dt>Vertices</dt><dd>{current.meshes.reduce((count, mesh) => count + mesh.vertices, 0).toLocaleString()}</dd></>}</dl>{current && <div className="table-scroll"><table><thead><tr><th>Mesh</th><th>Vertices</th><th>Skin weights</th></tr></thead><tbody>{current.meshes.map(mesh => <tr key={mesh.element}><td>{mesh.name}</td><td className="numeric">{mesh.vertices.toLocaleString()}</td><td>{mesh.has_skin_weights ? 'Present' : 'Not present'}</td></tr>)}</tbody></table></div>}</details>
          <div className="compact-drop">{drag ?? 'Drop another model here to replace it'}</div>
        </div> : <div className={`drop-zone ${drag ? 'drop-active' : ''}`}><div className="drop-symbol" aria-hidden="true">↓</div><h2>{drag ?? 'Add a replacement model'}</h2><p>Drop a game-ready DMX here or choose one manually.</p><button className="primary" disabled={busy || !document} onClick={choose}>Choose model…</button></div>}
        <div className="model-preparation-link"><span>Need to prepare Blender, FBX, GLB or glTF?</span>{openPreparation&&<button className="tertiary" disabled={busy} onClick={openPreparation}>Open Model preparation →</button>}</div>
      </Section>
      {project?.dmx && <Section title="3D preview" className="preview-section" actions={<button className={preview?'':'primary'} disabled={busy} onClick={loadPreview}>{preview ? 'Reload preview' : 'Load preview'}</button>}>
        {preview ? <Suspense fallback={<p className="hint">Loading 3D renderer…</p>}><Viewport scene={preview} project={project} disabled={busy} /></Suspense> : <div className="preview-empty"><span aria-hidden="true">◇</span><h3>Inspect your replacement</h3><p>Load the model to check its shape, meshes and weights.</p><p className="hint">Rest-pose preview · cloth simulation runs in the game or ModelDoc.</p></div>}
      </Section>}
    </div>
    {project && <>
      <div className="appearance-options-grid">
      <Section title="Model processing">
        <div className="setting-summary"><div><StateLabel tone={processingIssue ? 'attention' : project.auto_split ? 'ready' : 'neutral'}>{processingIssue ? 'Mesh processing needs attention' : project.auto_split ? 'Automatic splitting enabled' : 'Automatic splitting disabled'}</StateLabel><p>{processingIssue ? inspection?.vertex_preflight.errors[0] : project.auto_split ? 'Oversized meshes will be split automatically.' : 'Keeps meshes as supplied. Analyze the project to check the engine limits.'}</p></div></div>
        <Disclosure title="Configure mesh processing" hint="Split target, detail levels and advanced options" id="model-processing">
          <Check label="Automatically split oversized meshes" value={project.auto_split} onChange={auto_split => update({ ...project, auto_split })} disabled={busy} />
          {project.auto_split && <NumberField label="Target vertices per mesh part" value={project.split_target} min={3} max={524287} step={1} update={split_target => update({ ...project, split_target })} disabled={busy} />}
          <SelectField label="Detail levels" value={project.lod_strategy} values={[["reuse", "Reuse replacement at original levels"], ["preserve", "Preserve original lower-detail meshes"], ["choose", "Choose per mapping"], ["single", "Single level"], ["disable", "Disable additional levels"], ["custom", "Custom mapping"]]} onChange={lod_strategy => update({ ...project, lod_strategy })} disabled={busy} />
          <Check label="Automatically map replacement meshes" value={project.auto_map} onChange={auto_map => update({ ...project, auto_map })} disabled={busy} />
          <Check label="Allow unskinned geometry" value={project.allow_unskinned} onChange={allow_unskinned => update({ ...project, allow_unskinned })} disabled={busy} />
          <p className="hint">The engine vertex limit and all build validation still apply.</p>
        </Disclosure>
      </Section>
      <Section title="Textures" id="textures" actions={project.textures&&<button className="tertiary" disabled={busy} onClick={()=>revealSection('materials')}>Manage textures</button>}>{project.textures&&<p className="texture-count">{textureCount ? `${textureCount} texture${textureCount===1?'':'s'} assigned` : 'Folder selected · analyze materials to assign textures'}</p>}<FilePathInput label="Texture folder" kind="texture_directory" value={project.textures} update={textures => update({ ...project, textures })} disabled={busy} fail={fail} /></Section>
      </div>
      <MaterialEditor checked={Boolean(inspection)} analyze={analyzeMaterials} project={project} update={update} disabled={busy} fail={fail} />
      <Disclosure title="Surface changes and expression" hint={`${project.customization.appearance?.enabled ? `${project.customization.appearance.materials?.length??0} surface changes` : 'Optional surface changes'} · ${project.customization.face?.enabled?'Static expression saved':'Expression off'}`}><AppearanceOverrides project={project} update={update} disabled={busy} fail={fail} /></Disclosure>
      <Disclosure title="Equipment" hint={project.customization.equipment?.enabled?'Equipment choices saved · open to manage':'Optional weapons, accessories, props and alternate forms'}><Equipment project={project} update={update} disabled={busy} fail={fail} summary={current} /></Disclosure>
      <Disclosure title="Saved inputs and mappings" hint="File locations and assignment counts"><dl className="summary-list"><dt>Textures</dt><dd><PathValue path={project.textures} empty="No texture folder selected" /></dd><dt>Material assignments</dt><dd>{Object.keys(project.materials).length}</dd><dt>Model mappings</dt><dd>{Object.keys(project.meshes).length}</dd></dl><p className="hint">These assignments are saved with the project and checked during preparation and build.</p></Disclosure>
    </>}
  </>;
}
