import type { Project, ProjectDocument } from '../models';
import type { ProjectInspection } from '../build/models';
import { projectReadiness, type ReviewDestination } from '../build/readiness';
import { HeroPicker, heroLabel } from './HeroPicker';
import { Disclosure, Field, PageTitle, StateLabel } from './Primitives';
import { PathValue } from './Paths';
import { revealSection } from './workflowNavigation';

export function ProjectOverview({ document, report, fresh, disabled, update, selectHero, fail, go, loadOutfit, saveOutfit }: {
  document: ProjectDocument; report: ProjectInspection | null; fresh: boolean; disabled: boolean;
  update: (project: Project) => void; selectHero: React.ComponentProps<typeof HeroPicker>['select'];
  fail: (title: string, error: unknown) => void; go: (page: ReviewDestination, section?: string) => void;
  loadOutfit: () => void; saveOutfit: () => void;
}) {
  const { project } = document, state = projectReadiness(project, report, fresh);
  const rows = state.rows.filter(row => !['processing', 'output'].includes(row.key) && row.status !== 'Not applicable');
  const complete = rows.filter(row => row.tone === 'ready').length;
  const hero = project.original_model ? heroLabel({name:project.hero_name,path:project.original_model,likely_hero:true}) : 'No character selected';
  const next = !project.original_model ? {text:'Choose the character you want to replace.',action:'Choose character',page:'project' as const,section:'character-selection'} : !state.hasInputs ? {text:'Add a replacement model to continue.',action:'Go to Appearance →',page:'appearance' as const,section:'replacement-model'} : {text:state.ready?'Your project is ready for the build review.':state.issues.length?'Open Review & build to resolve the outstanding choices.':'Analyze the project to check its compatibility and prepare for the build.',action:'Go to Review & build →',page:'build' as const,section:undefined};
  function navigate(page: ReviewDestination, section?: string) { if (page === 'project' && section) revealSection(section); else go(page, section); }
  return <>
    <PageTitle eyebrow="Your workspace" title="Project overview" subtitle="Your character, configuration progress, and next step." />
    <div className="overview-start">
    <section className="overview-identity" aria-label="Project summary">
      <div className="overview-character" id="character-selection"><span className="eyebrow">Character</span><strong className="hero-name">{hero}</strong><HeroPicker current={project.original_model} disabled={disabled} select={selectHero} fail={fail}/></div>
      <div className="overview-name"><Field label="Project name" value={project.name} disabled={disabled} onChange={name => update({...project,name})} hint="Start with a letter. Up to 41 letters, numbers, spaces or underscores."/><span className="muted">{document.path?'Project saved on this computer':'New project · not saved yet'}</span></div>
      <div className="overview-actions"><span className="eyebrow">Quick actions</span><details className="preset-menu"><summary>Customization presets</summary><p className="hint">Reuse saved customization choices.</p><div className="row"><button disabled={disabled} onClick={loadOutfit}>Load preset…</button><button disabled={disabled} onClick={saveOutfit}>Save preset…</button></div></details></div>
    </section>
    <section className="next-step" aria-label="Next step"><div><span className="eyebrow">Next step</span><p>{next.text}</p></div><button className="primary" disabled={disabled} onClick={()=>navigate(next.page,next.section)}>{next.action}</button></section>
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
