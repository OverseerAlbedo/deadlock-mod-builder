import type { JsonValue, Project, ProjectDocument } from '../models';
import type { ProjectInspection } from '../build/models';
import { projectReadiness, type ReviewDestination } from '../build/readiness';
import { HeroPicker, ShownHeroLabel, heroLabel, shownPickerLabel } from './HeroPicker';
import { useGameNames } from '../i18n/gameNames';
import { Disclosure, Field, PageTitle, StateLabel } from './Primitives';
import { PathValue } from './Paths';
import { FilePathInput } from './FilePathInput';
import { revealSection } from './workflowNavigation';
import { useEffect, useState } from 'react';
import { api, basename, type HeroProportions } from '../api';
import { m, mc, tc, tp, useT } from '../i18n';
import { T } from '../i18n/T';
// i18n-area: shell

/** What is unusual about a hero's proportions, in English (one whole phrase per case). */
const PROPORTION_WORDS: string[] = [
  m('a long right arm'), m('a long left arm'), m('long arms'), m('short legs'),
  m('a long right arm and short legs'), m('a long left arm and short legs'), m('long arms and short legs'),
  m('short legs and a long right arm'), m('short legs and a long left arm'), m('short legs and long arms'),
];
/** The readiness rows' names as they sit inside a row button's accessible name ("Change character"):
 * the English row label in lower case, translated as one word with this context. */
const READINESS_AREAS: string[] = [
  mc('readiness area', 'character'), mc('readiness area', 'replacement model'), mc('readiness area', 'compatibility'),
  mc('readiness area', 'materials'), mc('readiness area', 'model mapping'), mc('readiness area', 'mesh processing'),
  mc('readiness area', 'physics'), mc('readiness area', 'motion'), mc('readiness area', 'output'),
];
const shownArea = (label: string) => { const area = label.toLowerCase(); return READINESS_AREAS.includes(area) ? tc('readiness area', area) : area; };

/** Shiv, Wraith and other heroes far outside ordinary proportions (same rule as preparation). */
export function ProportionNotice({hero, value}: {hero: string; value: HeroProportions}) {
  const t = useT();
  const what = value.extremes.map(e => e === 'arms' ? (Math.abs(value.arm_left - value.arm_right) > 0.05 ? `a long ${value.arm_right > value.arm_left ? 'right' : 'left'} arm` : 'long arms') : 'short legs').join(' and ');
  // Every phrase `what` can be is marked above (PROPORTION_WORDS), so it is translated as one piece.
  const shown = PROPORTION_WORDS.includes(what) ? t(what) : what;
  return <p className="field-check field-check-warning hero-proportions" role="note"><T text="{hero} has unusual proportions: {what}. With <strong>Preserve source proportions</strong>, preparation fits your model's body to {hero} and spreads the difference over the whole arm; with <strong>Match hero proportions</strong> your model takes {hero}'s proportions. Check the arms in game." values={{hero, what: shown}}/></p>;
}
export function ProjectOverview({ document, report, fresh, disabled, update, selectHero, copyForHero, fail, go, loadOutfit, saveOutfit, chooseModel, prepareAgain }: {
  document: ProjectDocument; report: ProjectInspection | null; fresh: boolean; disabled: boolean;
  update: (project: Project) => void; selectHero: React.ComponentProps<typeof HeroPicker>['select'];
  /** Make a copy for another hero (a new project; this one is not changed). */ copyForHero?: React.ComponentProps<typeof HeroPicker>['select'];
  fail: (title: string, error: unknown) => void; go: (page: ReviewDestination | 'prepare', section?: string) => void;
  loadOutfit: () => void; saveOutfit: () => void;
  /** Choose the character model file; Blender, FBX, glTF and MMD files are prepared automatically. */
  chooseModel?: () => void;
  /** Prepare the chosen model again (after the gun file changed). */
  prepareAgain?: () => void;
}) {
  const t = useT();
  useGameNames();
  const { project } = document, state = projectReadiness(project, report, fresh);
  const rows = state.rows.filter(row => !['processing', 'output'].includes(row.key) && row.status !== 'Not applicable');
  const complete = rows.filter(row => row.tone === 'ready').length;
  const hero = project.original_model ? heroLabel({name:project.hero_name,path:project.original_model,likely_hero:true}) : t('No character selected');
  const options = project.preparation_options as Record<string, JsonValue | undefined>;
  const source = project.preparation_source, prepared = Boolean(project.dmx);
  const gunFile = typeof options.weapon_source === 'string' ? options.weapon_source : '';
  // English, drawn translated.
  const next = !project.original_model ? {text:m('Choose the hero your mod replaces.'),action:m('Choose character'),page:'project' as const,section:'character-selection'}
    : !source && !project.dmx ? {text:m('Choose your character model file.'),action:m('Choose model file…'),page:'project' as const,section:'your-model'}
    : !prepared ? {text:m('Prepare your model: it is fitted to the hero automatically.'),action:m('Go to Prepare model →'),page:'prepare' as const,section:undefined}
    : {text:state.ready?m('Your project is ready to build.'):m('Check materials and weapons, then build your mod.'),action:state.ready?m('Go to Build & install →'):m('Go to Appearance & weapons →'),page:state.ready?'build' as const:'appearance' as const,section:undefined};
  const [proportions, setProportions] = useState<HeroProportions | null>(null);
  useEffect(() => {
    let current = true; setProportions(null);
    if (project.original_model) Promise.resolve().then(() => api.heroProportions(project.original_model)).then(value => { if (current) setProportions(value ?? null); }).catch(() => undefined);
    return () => { current = false; };
  }, [project.original_model]);
  function navigate(page: ReviewDestination | 'prepare', section?: string) { if (page === 'project' && section) revealSection(section); else go(page, section); }
  return <>
    <PageTitle eyebrow={t('Step 1 of 5')} title={t('Character & model')} subtitle={t('Choose the hero your mod replaces and the model you want to use. Everything else is set up for you.')} />
    <div className="overview-start">
    <section className="overview-identity" aria-label={t('Project summary')}>
      <div className="overview-character" id="character-selection"><span className="eyebrow">{t('Hero to replace')}</span><strong className="hero-name">{project.original_model?<ShownHeroLabel label={hero}/>:hero}</strong><HeroPicker current={project.original_model} disabled={disabled} select={selectHero} fail={fail}/>{proportions&&proportions.extremes.length>0&&<ProportionNotice hero={shownPickerLabel(hero)} value={proportions}/>}{copyForHero&&project.original_model&&<HeroPicker copy current={project.original_model} disabled={disabled} select={copyForHero} fail={fail}/>}</div>
      <div className="overview-model" id="your-model"><span className="eyebrow">{t('Your model')}</span>
        <strong className="hero-name">{source ? basename(source) : project.dmx ? basename(project.dmx) : t('No model chosen')}</strong>
        <span className="muted">{prepared ? project.preparation_report ? t('Prepared and ready') : t('Game-ready model (DMX)') : source ? t('Not prepared yet') : t('Blender, FBX, GLB, glTF, MMD or DMX')}</span>
        {chooseModel && <button className={source || project.dmx ? '' : 'primary'} disabled={disabled || !project.original_model} onClick={chooseModel}>{source || project.dmx ? t('Choose another model…') : t('Choose model file…')}</button>}
        {!project.original_model && <span className="hint">{t('Choose the hero first.')}</span>}
        {(source || !project.dmx) && <><FilePathInput label={t('Gun from another file (optional)')} name="Gun from another file (optional)" kind="weapon" value={gunFile} disabled={disabled} fail={fail} update={path => {
          update({...project, preparation_options:JSON.parse(JSON.stringify({...options, weapon_source:path || undefined, weapon:undefined, reviewed:false})) as Project['preparation_options']});
          // A model that was chosen already is prepared again with the new gun.
          if (source && prepareAgain) prepareAgain();
        }}/>
        <span className="hint">{t('Only if your gun is in a separate file. A file with two guns gives one for each hand.')}</span></>}
      </div>
      <div className="overview-name"><Field label={t('Project name')} value={project.name} disabled={disabled} onChange={name => update({...project,name})} hint={t('Start with a letter. Up to 41 letters, numbers, spaces or underscores.')}/><span className="muted">{document.path?t('Project saved on this computer'):t('New project · not saved yet')}</span>
        <details className="preset-menu"><summary>{t('Customization presets')}</summary><p className="hint">{t('Reuse saved customization choices.')}</p><div className="row"><button disabled={disabled} onClick={loadOutfit}>{t('Load preset…')}</button><button disabled={disabled} onClick={saveOutfit}>{t('Save preset…')}</button></div></details></div>
    </section>
    <section className="next-step" aria-label={t('Next step')}><div><span className="eyebrow">{t('Next step')}</span><p>{t(next.text)}</p></div><button className="primary" disabled={disabled || (next.section === 'your-model' && !chooseModel)} onClick={()=>next.section === 'your-model' && chooseModel ? chooseModel() : navigate(next.page,next.section)}>{t(next.action)}</button></section>
    </div>
    <section className="setup-progress" aria-label={t('Configuration progress')}>
      <div className="section-heading"><h2>{t('Setup progress')}</h2><span className="muted">{tp(rows.length, {other: '{ready} of {count} areas ready'}, {ready: complete})}</span></div>
      <progress value={complete} max={rows.length} aria-label={t('Areas ready')}/>
      <div className="readiness-rows">{rows.map(row=><div className="readiness-row" key={row.key}><span className="row-label">{t(row.label)}</span><StateLabel tone={row.tone}>{row.value}</StateLabel>{row.page&&<button className="tertiary" disabled={disabled} onClick={()=>navigate(row.page!,row.section)} aria-label={tc('readiness row', '{action} {area}', {action: t(row.action ?? ''), area: shownArea(row.label)})}>{t(row.action ?? '')}</button>}</div>)}</div>
    </section>
    <Disclosure title={t('Advanced project details')} hint={t('Saved location, original model and project identity')}>
      <dl className="summary-list"><dt>{t('Saved project')}</dt><dd><PathValue path={document.path} empty={t('Not saved yet')}/></dd><dt>{t('Original model')}</dt><dd><PathValue path={project.original_model}/></dd><dt>{t('Replacement model')}</dt><dd><PathValue path={project.dmx}/></dd><dt>{t('Source folder')}</dt><dd><PathValue path={project.source_root}/></dd><dt>{t('Project ID')}</dt><dd className="mono path">{project.id}</dd><dt>{t('Schema')}</dt><dd>{project.schema}</dd></dl>
    </Disclosure>
  </>;
}
