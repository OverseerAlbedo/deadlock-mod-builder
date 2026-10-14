import type { JsonValue, Project, ProjectDocument } from '../models';
import type { ProjectInspection } from '../build/models';
import { projectReadiness, type ReviewDestination } from '../build/readiness';
import { HeroPicker, heroLabel } from './HeroPicker';
import { Disclosure, Field, PageTitle, StateLabel } from './Primitives';
import { PathValue } from './Paths';
import { FilePathInput } from './FilePathInput';
import { revealSection } from './workflowNavigation';
import { useEffect, useState } from 'react';
import { api, basename, type HeroProportions } from '../api';

/** Shiv, Wraith and other heroes far outside ordinary proportions (same rule as preparation). */
export function ProportionNotice({hero, value}: {hero: string; value: HeroProportions}) {
  const what = value.extremes.map(e => e === 'arms' ? (Math.abs(value.arm_left - value.arm_right) > 0.05 ? `a long ${value.arm_right > value.arm_left ? 'right' : 'left'} arm` : 'long arms') : 'short legs').join(' and ');
  return <p className="field-check field-check-warning hero-proportions" role="note">{hero} has unusual proportions: {what}. With <strong>Keep source proportions</strong>, preparation fits your model's body to {hero} and spreads the difference over the whole arm; with <strong>Match hero proportions</strong> your model takes {hero}'s proportions. Check the arms in game.</p>;
}
export function ProjectOverview({ document, report, fresh, disabled, update, selectHero, fail, go, loadOutfit, saveOutfit, chooseModel, prepareAgain }: {
  document: ProjectDocument; report: ProjectInspection | null; fresh: boolean; disabled: boolean;
  update: (project: Project) => void; selectHero: React.ComponentProps<typeof HeroPicker>['select'];
  fail: (title: string, error: unknown) => void; go: (page: ReviewDestination | 'prepare', section?: string) => void;
  loadOutfit: () => void; saveOutfit: () => void;
  /** Choose the character model file; Blender, FBX and glTF files are prepared automatically. */
  chooseModel?: () => void;
  /** Prepare the chosen model again (after the gun file changed). */
  prepareAgain?: () => void;
}) {
  const { project } = document, state = projectReadiness(project, report, fresh);
  const rows = state.rows.filter(row => !['processing', 'output'].includes(row.key) && row.status !== 'Not applicable');
  const complete = rows.filter(row => row.tone === 'ready').length;
  const hero = project.original_model ? heroLabel({name:project.hero_name,path:project.original_model,likely_hero:true}) : 'No character selected';
  const options = project.preparation_options as Record<string, JsonValue | undefined>;
  const source = project.preparation_source, prepared = Boolean(project.dmx);
  const gunFile = typeof options.weapon_source === 'string' ? options.weapon_source : '';
  const next = !project.original_model ? {text:'Choose the hero your mod replaces.',action:'Choose character',page:'project' as const,section:'character-selection'}
    : !source && !project.dmx ? {text:'Choose your character model file.',action:'Choose model file…',page:'project' as const,section:'your-model'}
    : !prepared ? {text:'Prepare your model: it is fitted to the hero automatically.',action:'Go to Prepare model →',page:'prepare' as const,section:undefined}
    : {text:state.ready?'Your project is ready to build.':'Check materials and weapons, then build your mod.',action:state.ready?'Go to Build & install →':'Go to Appearance & weapons →',page:state.ready?'build' as const:'appearance' as const,section:undefined};
  const [proportions, setProportions] = useState<HeroProportions | null>(null);
  useEffect(() => {
    let current = true; setProportions(null);
    if (project.original_model) Promise.resolve().then(() => api.heroProportions(project.original_model)).then(value => { if (current) setProportions(value ?? null); }).catch(() => undefined);
    return () => { current = false; };
  }, [project.original_model]);
  function navigate(page: ReviewDestination | 'prepare', section?: string) { if (page === 'project' && section) revealSection(section); else go(page, section); }
  return <>
    <PageTitle eyebrow="Step 1 of 5" title="Character & model" subtitle="Choose the hero your mod replaces and the model you want to use. Everything else is set up for you." />
    <div className="overview-start">
    <section className="overview-identity" aria-label="Project summary">
      <div className="overview-character" id="character-selection"><span className="eyebrow">Hero to replace</span><strong className="hero-name">{hero}</strong><HeroPicker current={project.original_model} disabled={disabled} select={selectHero} fail={fail}/>{proportions&&proportions.extremes.length>0&&<ProportionNotice hero={hero} value={proportions}/>}</div>
      <div className="overview-model" id="your-model"><span className="eyebrow">Your model</span>
        <strong className="hero-name">{source ? basename(source) : project.dmx ? basename(project.dmx) : 'No model chosen'}</strong>
        <span className="muted">{prepared ? project.preparation_report ? 'Prepared and ready' : 'Game-ready model (DMX)' : source ? 'Not prepared yet' : 'Blender, FBX, GLB, glTF or DMX'}</span>
        {chooseModel && <button className={source || project.dmx ? '' : 'primary'} disabled={disabled || !project.original_model} onClick={chooseModel}>{source || project.dmx ? 'Choose another model…' : 'Choose model file…'}</button>}
        {!project.original_model && <span className="hint">Choose the hero first.</span>}
        {(source || !project.dmx) && <><FilePathInput label="Gun from another file (optional)" kind="weapon" value={gunFile} disabled={disabled} fail={fail} update={path => {
          update({...project, preparation_options:JSON.parse(JSON.stringify({...options, weapon_source:path || undefined, weapon:undefined, reviewed:false})) as Project['preparation_options']});
          // A model that was chosen already is prepared again with the new gun.
          if (source && prepareAgain) prepareAgain();
        }}/>
        <span className="hint">Only if your gun is in a separate file. A file with two guns gives one for each hand.</span></>}
      </div>
      <div className="overview-name"><Field label="Project name" value={project.name} disabled={disabled} onChange={name => update({...project,name})} hint="Start with a letter. Up to 41 letters, numbers, spaces or underscores."/><span className="muted">{document.path?'Project saved on this computer':'New project · not saved yet'}</span>
        <details className="preset-menu"><summary>Customization presets</summary><p className="hint">Reuse saved customization choices.</p><div className="row"><button disabled={disabled} onClick={loadOutfit}>Load preset…</button><button disabled={disabled} onClick={saveOutfit}>Save preset…</button></div></details></div>
    </section>
    <section className="next-step" aria-label="Next step"><div><span className="eyebrow">Next step</span><p>{next.text}</p></div><button className="primary" disabled={disabled || (next.section === 'your-model' && !chooseModel)} onClick={()=>next.section === 'your-model' && chooseModel ? chooseModel() : navigate(next.page,next.section)}>{next.action}</button></section>
    </div>
    <section className="setup-progress" aria-label="Configuration progress">
      <div className="section-heading"><h2>Setup progress</h2><span className="muted">{complete} of {rows.length} areas ready</span></div>
      <progress value={complete} max={rows.length} aria-label="Areas ready"/>
      <div className="readiness-rows">{rows.map(row=><div className="readiness-row" key={row.key}><span className="row-label">{row.label}</span><StateLabel tone={row.tone}>{row.value}</StateLabel>{row.page&&<button className="tertiary" disabled={disabled} onClick={()=>navigate(row.page!,row.section)} aria-label={`${row.action} ${row.label.toLowerCase()}`}>{row.action}</button>}</div>)}</div>
    </section>
    <Disclosure title="Advanced project details" hint="Saved location, original model and project identity">
      <dl className="summary-list"><dt>Saved project</dt><dd><PathValue path={document.path} empty="Not saved yet"/></dd><dt>Original model</dt><dd><PathValue path={project.original_model}/></dd><dt>Replacement model</dt><dd><PathValue path={project.dmx}/></dd><dt>Source folder</dt><dd><PathValue path={project.source_root}/></dd><dt>Project ID</dt><dd className="mono path">{project.id}</dd><dt>Schema</dt><dd>{project.schema}</dd></dl>
    </Disclosure>
  </>;
}
