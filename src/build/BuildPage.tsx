// i18n-area: build
import { useEffect, useState } from 'react';
import type { Project } from '../models';
import { basename, type VpkSlots } from '../api';
import type { BuildResult, ProjectInspection, OriginalValidation, PreparedModel } from './models';
import { projectReadiness, modelEnabled, type PreviousAnalysis, type ReviewDestination, type ReviewIssue } from './readiness';
import { Check, SelectField } from '../components/CustomizationPages';
import { NumberField } from '../components/MaterialEditor';
import { Disclosure, PageTitle, Section, StateLabel } from '../components/Primitives';
import { PathValue } from '../components/Paths';
import { revealSection } from '../components/workflowNavigation';
import { MeshMapping } from './MeshMapping';
import { changesSince, loadBuildRecord } from './changes';
import { InstalledMods } from './InstalledMods';
import { includedExtras } from './includedExtras';
import { changeCustomization } from '../components/customization';
import { m, tc, tp, tpc, useT } from '../i18n';
import { T } from '../i18n/T';

/** How often the slot status reads the folders again while this page is open (ms). */
export const SLOT_RESCAN_MS=3000;
/** Whether the chosen slot is free: the folders are read again each time the slot
 * changes, the window comes back, every few seconds while the page is open (a VPK
 * deleted in Explorer is seen without returning focus), and when the project is analyzed (`scan`). */
function SlotStatus({project,slots,scan}:{project:Project;slots:VpkSlots|null;scan?:()=>void}){
  const t=useT();
  useEffect(()=>{
    if(!scan)return;
    scan();
    const visible=()=>{if(window.document.visibilityState==='visible')scan();};
    window.addEventListener('focus',scan);
    window.document.addEventListener('visibilitychange',visible);
    const timer=window.setInterval(visible,SLOT_RESCAN_MS);
    return()=>{window.removeEventListener('focus',scan);window.document.removeEventListener('visibilitychange',visible);window.clearInterval(timer);};
  },[project.vpk_number,project.id,project.original_model]);// eslint-disable-line react-hooks/exhaustive-deps
  if(!slots||slots.configured_slot!==project.vpk_number)return null;
  const own=slots.previous_outputs.includes(project.vpk_number);
  return <StateLabel tone={slots.configured_slot_occupied?'attention':'ready'}>{slots.configured_slot_occupied?t('In use by another VPK in the game’s addons folder'):slots.configured_slot_own_install?t('Your installed build of this project · Install replaces it'):own?t('Free · your earlier build of this project uses it'):tc('vpk slot','Free')}</StateLabel>;
}

export function BuildPage({project,update,report,fresh,prepared,original,busy,inspect,prepare,validateOriginal,build,result,install,installed,openOutput,chooseFreeSlot,go,analyzing=false,expert=true,slots=null,scanSlots,previous=null,openProjectFile}: {
  /** Expert shows the developer tools and raw receipts. */expert?:boolean;
  /** Opens a saved project file (Installed mods → Open its project), as Open… does, unsaved-changes question included. */openProjectFile?:(path:string)=>void;
  /** The project as last analyzed (and whether a build checked the live project since): a
   * report that is not current stays the working list while it is for the same model, hero and mode. */previous?:PreviousAnalysis|null;
  /** The last VPK slot scan (refreshed at Analyze and by `scanSlots`), for the checklist and the slot status. */slots?:VpkSlots|null;scanSlots?:()=>void;
  project:Project;update:(project:Project)=>void;report:ProjectInspection|null;fresh:boolean;
  prepared:PreparedModel|null;original:OriginalValidation|null;busy:boolean;
  build:(overwriteOutput:boolean,overwriteInstall:boolean)=>void; result:BuildResult|null; install:(path:string|null,overwrite:boolean)=>void;installed:string|null;openOutput?:()=>void;chooseFreeSlot?:()=>void;
  inspect:()=>void;prepare:(offline:boolean)=>void;validateOriginal:(compile:boolean)=>void;go?:(page:ReviewDestination,section?:string)=>void;analyzing?:boolean;
}) {
  const t=useT();
  const [offline,setOffline]=useState(false),[overwriteInstall,setOverwriteInstall]=useState(false);
  // A build replaces this project's own earlier output (its private output folder; the old one is backed up),
  // and installing replaces this project's own installed build of that slot: neither asks again. Replacing
  // another mod's VPK in the addons folder still needs the box below.
  const ownInstall=Boolean(slots&&slots.configured_slot===project.vpk_number&&slots.configured_slot_own_install);
  const runBuild=()=>build(true,overwriteInstall||ownInstall);
  const state=projectReadiness(project,report,fresh,slots,overwriteInstall,previous);
  const modelReport=report&&'dmx' in report?report:null;
  const componentReport=report&&'model_enabled' in report?report:null;
  const blockers: (ReviewIssue & {analyze?:boolean})[] = [...state.issues];
  if (!state.current && !state.kept && state.hasInputs) blockers.push({text:report?t('Project changed. Analyze the current inputs again.'):t('Analyze the project to check compatibility, materials and mappings.'),action:m('Analyze project'),analyze:true});
  // Actions stay English (they are compared); they are translated where they are drawn.
  const actionText=(action:string|undefined)=>action?t(action):t('View details');
  const blocking=(count:number)=>tp(count,{one:'{count} issue blocking build',other:'{count} issues blocking build'});
  function act(page?:ReviewDestination,section?:string){if(page&&page!=='build')go?.(page,section);else if(section)revealSection(section);}
  function rowAction(key:string,page?:ReviewDestination,section?:string){if(key==='skeleton'||(key==='mapping'&&!modelReport))inspect();else act(page,section);}
  function issueAction(issue:ReviewIssue){if(issue.action==='Choose free slot'&&chooseFreeSlot)chooseFreeSlot();else act(issue.page,issue.section);}
  const pipelineSteps = state.refused ? t('Analyze to see what the last build found, then build again.') : state.hasInputs && !state.ready ? t('Resolve the items above before building.') : state.kept ? state.verified ? t('The last build checked this project. Build checks it again and validates outputs before publishing.') : t('The project changed since the last analysis: Build checks it again before it starts and validates outputs before publishing.') : t('Current files and compiled outputs are rechecked before publication.');
  // What the project's own rows ask the build to include; the model's analysis does not list them.
  const extras = includedExtras(project, modelEnabled(project));
  // The list of the last analysis, kept after project edits until Analyze runs again.
  const keptList = state.kept&&<div className="build-blockers kept-review">
    <h2 id="build-requirements">{state.refused?t('The last build found something that still needs a choice'):state.items.length?state.open?t('{open} of {total} left on the list from the last analysis',{open:state.open,total:state.items.length}):state.verified?t('The last build checked this project'):t('Everything on the list from the last analysis is handled'):state.open?blocking(state.open):t('Project changed since the last analysis')}</h2>
    <ul className="review-issues">{state.items.map(item=><li key={item.key} className={`review-item review-item-${item.state}`}>
      <span><span className="issue-symbol" aria-hidden="true">{item.state==='done'?'✓':item.state==='changed'?'↻':'!'}</span>{item.text}{item.state!=='open'&&<span className="review-state">{item.state==='done'?t('Done'):t('Changed · checked again by Build or Analyze')}</span>}</span>
      {item.state!=='done'&&<button className="tertiary" disabled={busy} onClick={()=>issueAction(item)}>{actionText(item.action)}</button>}
    </li>)}
    <li className="review-item review-item-analyze"><span><span className="issue-symbol" aria-hidden="true">{'—'}</span>{state.refused?t('Analyze to see what is left.'):state.open?t('Analyze again to refresh the whole list.'):state.verified?t('Analyze again to refresh the analysis details (optional).'):state.items.length?t('Everything on the list is handled. Build checks the project again before it starts; Analyze again is optional.'):t('Build checks the project again before it starts; Analyze again is optional.')}</span><button className={state.refused?'primary':undefined} disabled={busy} onClick={inspect}>{analyzing?t('Analyzing…'):t('Analyze project')}</button></li></ul>
  </div>;
  const output = state.rows.find(row=>row.key==='output')!;
  const record = loadBuildRecord(project.id), changed = record ? changesSince(record.project, project) : null;
  // Ctrl+B opens this page and builds when nothing blocks it.
  useEffect(()=>{const shortcut=()=>{if(!busy&&state.ready)runBuild();};window.addEventListener('dmb-build-shortcut',shortcut);return()=>window.removeEventListener('dmb-build-shortcut',shortcut);},[busy,state.ready,build,overwriteInstall,ownInstall]);
  return <>
    <PageTitle eyebrow={t('Step {step} of {total}',{step:5,total:5})} title={t('Build & install')} subtitle={t('Check your project, make the mod and add it to the game. Anything that needs a choice is listed here.')} action={<div className="title-actions">
      {result&&<div className="last-build" role="status" aria-label={t('Last completed build')}><span className="build-completed">{result.compiled?t('Build completed'):t('Build finished')}</span><span className="muted">{result.vpk?basename(result.vpk):t('No VPK')}{result.installed?` · ${t('installed')}`:result.vpk?` · ${t('not installed')}`:''}{record?` · ${new Date(record.time).toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'})}`:''}</span>{openOutput&&<button className="tertiary" disabled={busy} onClick={openOutput}>{t('Open output folder')}</button>}<button className="tertiary" onClick={()=>revealSection('last-build')}>{t('Check in game')}</button></div>}
      {report&&<button disabled={busy} onClick={inspect} aria-label={t('Re-analyze project')}>{analyzing?t('Analyzing…'):state.current?t('Re-analyze'):t('Analyze again')}</button>}
    </div>}/>
    <div className="review-layout">
    <section className="review-validation" aria-label={t('Build readiness')}>
      <div className="section-heading"><h2>{t('Project checklist')}</h2><span className="muted">{state.current?t('Current analysis'):state.kept?t('Last analysis · project changed since'):t('Analysis required')}</span></div>
      <div className="table-scroll"><table className="validation-table"><caption className="sr-only">{t('Current project validation')}</caption><thead><tr><th scope="col">{tc('column','Item')}</th><th scope="col">{tc('column','Status')}</th><th scope="col">{tc('column','Details')}</th><th scope="col">{tc('column','Action')}</th></tr></thead><tbody>{state.rows.map(row=>{
        const needsAnalysis=row.key==='skeleton'||(row.key==='mapping'&&!modelReport);
        const action=row.key==='mapping'&&!modelReport?m('Analyze'):row.action;
        const tone=analyzing&&row.key==='skeleton'?'working':row.tone;
        const label=analyzing&&row.key==='skeleton'?m('Processing'):row.status??(row.tone==='ready'?m('Ready'):row.tone==='attention'?m('Needs attention'):m('Not checked'));
        const shownAction=action?t(action):'';
        return <tr key={row.key}><th scope="row">{t(row.label)}</th><td><StateLabel tone={tone}>{t(label)}</StateLabel></td><td className="validation-detail">{row.value}</td><td><button className="tertiary" disabled={busy||(needsAnalysis&&!state.hasInputs)||(!needsAnalysis&&row.page!=='build'&&!go)} onClick={()=>rowAction(row.key,row.page,row.section)} aria-label={t('{action} {item}',{action:shownAction,item:t(row.label).toLowerCase()})}>{shownAction}</button></td></tr>;
      })}</tbody></table></div>
    </section>
    <div className="review-controls">
    <section className={`build-action-panel ${(state.kept?state.open||state.refused:blockers.length)?'has-blockers':''}`} aria-label={t('Build decision')}>
      {keptList||(blockers.length>0?<div className="build-blockers"><h2 id="build-requirements">{blocking(blockers.length)}</h2><ul className="review-issues">{blockers.map((issue,index)=><li key={index}><span><span className="issue-symbol" aria-hidden="true">{issue.analyze?'—':'!'}</span>{issue.text}</span><button className={issue.analyze?'primary':'tertiary'} disabled={busy} onClick={()=>issue.analyze?inspect():issueAction(issue)}>{issue.analyze&&analyzing?t('Analyzing…'):actionText(issue.action)}</button>{issue.withoutModel&&<button className="tertiary" disabled={busy} onClick={()=>update(changeCustomization(project,{model_enabled:false}))}>{t(issue.withoutModel)}</button>}</li>)}</ul></div>:<div className="build-ready"><StateLabel tone={state.ready?'ready':'neutral'}>{state.title}</StateLabel></div>)}
      <div className="build-decision"><p id="build-explanation">{!state.hasInputs?t('Choose the required inputs above to enable analysis.'):pipelineSteps}</p><button className="primary build-primary" disabled={busy||!state.ready} aria-describedby="build-explanation" onClick={runBuild}>{t('Build mod')}</button></div>
    </section>
    {changed&&<div className="build-changes" aria-label={t('Changes since the last build')}><span className="eyebrow">{t('Since the last build · {time}',{time:new Date(record!.time).toLocaleString()})}</span>{changed.length?<ul>{changed.map(item=><li key={item}>{t(item)}</li>)}</ul>:<p className="muted">{t('Nothing changed.')}</p>}</div>}
    <div className="output-summary"><span className="eyebrow">{t('Output')}</span><strong>{output.value}{result&&(result.compiled||result.vpk)&&<span className="build-completed" style={{marginLeft:10}}>{t('Completed')}</span>}</strong><span className="muted">{!project.compile_model&&modelEnabled(project)?t('Prepare source files only'):[modelEnabled(project)?t('Compile model'):t('Build extra assets'),project.build_vpk?t('Package after validation'):'',project.build_vpk&&project.install?t('Install after success'):''].filter(Boolean).join(' · ')}</span>{extras&&<span className="muted">{t('Extras included: {extras}',{extras})}</span>}</div>
    </div>
    </div>
    {result&&<Section title={t('Last completed build')} id="last-build" actions={openOutput&&<button disabled={busy} onClick={openOutput}>{t('Open output folder')}</button>}><dl className="summary-list"><dt>{t('Model compilation')}</dt><dd>{result.compiled?<span className="build-completed">{t('Completed')}</span>:t('Not performed')}</dd><dt>VPK</dt><dd><PathValue path={result.vpk} empty={t('Not packaged')}/></dd><dt>{t('Installation')}</dt><dd><PathValue path={result.installed} empty={t('Not installed')}/></dd><dt>{t('Receipt')}</dt><dd><PathValue path={result.manifest} empty={t('No package manifest')}/></dd>{result.prepared&&<><dt>{t('Prepared workspace')}</dt><dd><PathValue path={result.prepared}/></dd></>}</dl>{result.vpk&&!result.installed&&<button disabled={busy} onClick={()=>install(result.vpk,overwriteInstall)}>{t('Validate and install this VPK')}</button>}
      <div className="check-in-game"><strong>{t('Check in game')}</strong>{!modelEnabled(project)?<ul><li>{extras?t('What you replaced ({extras}) in the hero select screen and in a match',{extras}):t('Your extras in the hero select screen and in a match')}</li></ul>:<ul><li>{t('The character in the hero select screen and in a match')}</li><li>{t('Arms and elbows while running, aiming and using abilities')}</li><li>{t('The gun in the hand, and the fingers around it')}</li><li>{project.preparation_options.breast_motion===true||project.preparation_options.butt_motion===true?t('Hair, cloth, tails and other moving parts, breast and butt bounce'):t('Hair, cloth, tails and other moving parts')}</li><li>{t('Clipping through the body')}</li></ul>}</div></Section>}
    {report&&<Disclosure title={t('Analysis details')} hint={fresh?tp(report.warnings.length,{one:'{count} review note · full inspection available',other:'{count} review notes · full inspection available'}):t('Previous analysis · run Analyze project to refresh')} id="analysis-details">
      {modelReport?<>
        <p className={`status ${!fresh||modelReport.needs_review||modelReport.vertex_preflight.errors.length?'status-skipped':'status-succeeded'}`}>{!fresh?state.kept?t('Project changed since this analysis · Build checks it again'):t('Project changed · analyze again'):modelReport.vertex_preflight.errors.length?t('Blocking problems found'):modelReport.needs_review?t('Analysis needs review'):t('Analysis passed · compilation still required')}</p>
        <div className="inline-stats"><span><T text="<strong>{count}</strong> bones" values={{count:modelReport.dmx.bones.length.toLocaleString()}}/></span><span><T text="<strong>{count}</strong> source meshes" values={{count:modelReport.dmx.meshes.length.toLocaleString()}}/></span><span><T text="<strong>{count}</strong> source triangles" values={{count:modelReport.geometry.source_triangles.toLocaleString()}}/></span></div>
        {modelReport.vertex_preflight.errors.length>0&&<ul className="issue-list">{modelReport.vertex_preflight.errors.map((value,index)=><li key={index}>{value}</li>)}</ul>}
        {modelReport.warnings.length>0&&<ul className="issue-list">{modelReport.warnings.map((value,index)=><li key={index}>{value}</li>)}</ul>}
        <p className="hint">{modelReport.vertex_preflight.count_scope}</p>
      </>:<><p className="status">{fresh?t('Extra assets inspected · compilation still required'):state.kept?t('Project changed since this analysis · Build checks it again'):t('Project changed · analyze again')}</p><p>{[
        tpc('analysis counts',componentReport?.icons.length??0,{one:'{count} icons',other:'{count} icons'}),
        componentReport?.item_icons?.length?tp(componentReport.item_icons.length,{one:'{count} item icon',other:'{count} item icons'}):'',
        componentReport?.item_names?.rows?.length?tp(componentReport.item_names.rows.length,{one:'{count} item name',other:'{count} item names'}):'',
        tpc('analysis counts',componentReport?.animations.length??0,{one:'{count} animations',other:'{count} animations'}),
        tpc('analysis counts',componentReport?.effects.length??0,{one:'{count} effects',other:'{count} effects'}),
        componentReport?.effect_edits?.length?tp(componentReport.effect_edits.length,{one:'{count} effect edit',other:'{count} effect edits'}):'',
        tpc('analysis counts',componentReport?.audio.length??0,{one:'{count} audio replacements',other:'{count} audio replacements'}),
        componentReport?.ability_objects?.length?tp(componentReport.ability_objects.length,{one:'{count} ability object',other:'{count} ability objects'}):'',
        componentReport?.materials?.length?tp(componentReport.materials.length,{one:'{count} hero color',other:'{count} hero colors'}):'',
      ].filter(Boolean).join(' · ')}</p>{report.warnings.length>0&&<ul>{report.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}</>}
      {expert&&<details className="technical"><summary>{modelReport?t('Complete native inspection receipt'):t('Extra asset inspection receipt')}</summary><pre>{JSON.stringify(report,null,2)}</pre></details>}
    </Disclosure>}
    {modelReport&&<Disclosure title={t('Mesh mapping')} hint={tp(modelReport.model.meshes.length,{one:'{count} original render nodes · keep, replace or disable',other:'{count} original render nodes · keep, replace or disable'})} id="mesh-mapping">
      <MeshMapping report={modelReport} meshes={project.meshes} busy={busy} strategy={project.lod_strategy} update={meshes=>update({...project,meshes})}/>
    </Disclosure>}
    <Disclosure title={t('Customize build')} hint={t('Detail levels, VPK slot, compile, package and install')} id="customize-build">
    <Disclosure title={t('Build settings')} hint={t('Detail levels and LOD acknowledgement')} id="build-settings">
      <SelectField label={t('LOD strategy')} value={project.lod_strategy} values={[["choose",t("Choose a strategy…")],["reuse",t("Reuse replacement at original levels")],["preserve",t("Preserve original lower-detail meshes")],["single",t("Single level")],["disable",t("Disable additional levels")],["custom",t("Use custom mesh mappings")]]} onChange={lod_strategy=>update({...project,lod_strategy,review_lods:false})} disabled={busy}/>
      <Check label={t('I have reviewed the LOD appearance and performance tradeoffs')} value={project.review_lods} onChange={review_lods=>update({...project,review_lods})} disabled={busy}/><p className="hint">{t('No simplified geometry is generated by changing this strategy. Original lower-detail meshes can restore the original appearance at a distance.')}</p>
    </Disclosure>
    <Disclosure title={t('Output settings')} hint={t('Slot {slot} · compilation, packaging and installation',{slot:project.vpk_number})} id="output-settings">
      <NumberField label={t('VPK slot')} value={project.vpk_number} min={2} max={99} step={1} update={vpk_number=>update({...project,vpk_number})} disabled={busy}/><div className="row">{chooseFreeSlot&&<button disabled={busy} onClick={chooseFreeSlot}>{t('Choose free slot')}</button>}<p className="mono">pak{String(project.vpk_number).padStart(2,'0')}_dir.vpk</p><SlotStatus project={project} slots={slots} scan={scanSlots}/></div><p className="hint">{t('The game’s addons folder is checked again every time the slot changes, you come back to the app or the project is analyzed, so a VPK you deleted frees its slot at once.')}</p>
      <Check label={t('Compile replacement model')} value={project.compile_model} onChange={compile_model=>update({...project,compile_model})} disabled={busy}/><Check label={t('Package a VPK after validation')} value={project.build_vpk} onChange={build_vpk=>update({...project,build_vpk})} disabled={busy}/><Check label={t('Install after successful validation and packaging')} value={project.install} onChange={install=>update({...project,install})} disabled={busy}/>
      <details className="technical"><summary>{t('Replace existing output')}</summary><Check label={t('Replace an existing installed VPK at the selected slot')} value={overwriteInstall} onChange={setOverwriteInstall} disabled={busy}/></details>
    </Disclosure>
    </Disclosure>
    <InstalledMods busy={busy} onOpenProject={openProjectFile}/>
    {expert&&<Disclosure title={t('Developer / compiler tools')} hint={t('Source preparation, compiler tests and existing VPK installation')}>
      <Section title={t('Compiler preparation')} actions={<button disabled={busy||!modelReport||!fresh||modelReport.needs_review||Boolean(modelReport.vertex_preflight.errors.length)} onClick={()=>prepare(offline)}>{t('Prepare source files')}</button>}><Check label={t('Prepare offline source workspace')} value={offline} onChange={setOffline} disabled={busy}/><p className="hint">{t('Preparation writes compiler sources without compiling, packaging or installing.')}</p>{prepared&&<dl className="summary-list"><dt>{t('Content')}</dt><dd><PathValue path={prepared.content}/></dd><dt>{t('Compiled output directory')}</dt><dd><PathValue path={prepared.game}/></dd><dt>{tc('output','Model')}</dt><dd><PathValue path={prepared.model}/></dd><dt>{t('Generated materials')}</dt><dd>{prepared.generated_materials.length}</dd></dl>}</Section>
      <Section title={t('Original model test')} actions={<div className="row"><button disabled={busy||!project.original_model} onClick={()=>validateOriginal(false)}>{t('Inspect compiler plan')}</button><button disabled={busy||!project.original_model} onClick={()=>validateOriginal(true)}>{t('Compile original test')}</button></div>}><p className="hint">{t('Test the original character with the selected toolchain. The result is not packaged or installed.')}</p>{original&&<><p className="status">{original.compiled?t('Original test compiled'):t('Original compiler plan inspected')}</p>{original.compiled_model&&<PathValue path={original.compiled_model}/>}<details className="technical"><summary>{t('Original test receipt')}</summary><pre>{JSON.stringify(original,null,2)}</pre></details></>}</Section>
      <Section title={t('Install an existing VPK')} actions={<button disabled={busy||!project.original_model} onClick={()=>install(null,overwriteInstall)}>{t('Choose VPK to validate and install…')}</button>}><p className="hint">{t('The selected VPK is copied privately and checked against the current game before installation.')}</p>{installed&&<div className="row"><StateLabel tone="ready">{t('Installed')}</StateLabel><PathValue path={installed}/></div>}</Section>
    </Disclosure>}
  </>;
}
