import { useState } from 'react';
import type { Project } from '../models';
import type { BuildResult, ProjectInspection, OriginalValidation, PreparedModel } from './models';
import { projectReadiness, modelEnabled, type ReviewDestination, type ReviewIssue } from './readiness';
import { Check, SelectField } from '../components/CustomizationPages';
import { NumberField } from '../components/MaterialEditor';
import { Disclosure, PageTitle, Section, StateLabel } from '../components/Primitives';
import { PathValue } from '../components/Paths';
import { revealSection } from '../components/workflowNavigation';
import { MeshMapping } from './MeshMapping';

export function BuildPage({project,update,report,fresh,prepared,original,busy,inspect,prepare,validateOriginal,build,result,install,installed,openOutput,chooseFreeSlot,go,analyzing=false}: {
  project:Project;update:(project:Project)=>void;report:ProjectInspection|null;fresh:boolean;
  prepared:PreparedModel|null;original:OriginalValidation|null;busy:boolean;
  build:(overwriteOutput:boolean,overwriteInstall:boolean)=>void; result:BuildResult|null; install:(path:string|null,overwrite:boolean)=>void;installed:string|null;openOutput?:()=>void;chooseFreeSlot?:()=>void;
  inspect:()=>void;prepare:(offline:boolean)=>void;validateOriginal:(compile:boolean)=>void;go?:(page:ReviewDestination,section?:string)=>void;analyzing?:boolean;
}) {
  const [offline,setOffline]=useState(false),[overwriteOutput,setOverwriteOutput]=useState(false),[overwriteInstall,setOverwriteInstall]=useState(false);
  const state=projectReadiness(project,report,fresh);
  const modelReport=report&&'dmx' in report?report:null;
  const componentReport=report&&'model_enabled' in report?report:null;
  const blockers: (ReviewIssue & {analyze?:boolean})[] = [...state.issues];
  if (!state.current && state.hasInputs) blockers.push({text:report?'Project changed. Analyze the current inputs again.':'Analyze the project to check compatibility, materials and mappings.',action:'Analyze project',analyze:true});
  function act(page?:ReviewDestination,section?:string){if(page&&page!=='build')go?.(page,section);else if(section)revealSection(section);}
  function rowAction(key:string,page?:ReviewDestination,section?:string){if(key==='skeleton'||(key==='mapping'&&!modelReport))inspect();else act(page,section);}
  const pipelineSteps = state.hasInputs && !state.ready ? 'Resolve the items above before building.' : 'Current files and compiled outputs are rechecked before publication.';
  const output = state.rows.find(row=>row.key==='output')!;
  return <>
    <PageTitle eyebrow="Check & export" title="Review & build" subtitle="Validate your current project, resolve blockers, then build." action={state.current&&<button disabled={busy} onClick={inspect} aria-label="Re-analyze project">{analyzing?'Analyzing…':'Re-analyze'}</button>}/>
    <div className="review-layout">
    <section className="review-validation" aria-label="Build readiness">
      <div className="section-heading"><h2>Project checklist</h2><span className="muted">{state.current?'Current analysis':'Analysis required'}</span></div>
      <div className="table-scroll"><table className="validation-table"><caption className="sr-only">Current project validation</caption><thead><tr><th scope="col">Item</th><th scope="col">Status</th><th scope="col">Details</th><th scope="col">Action</th></tr></thead><tbody>{state.rows.map(row=>{
        const needsAnalysis=row.key==='skeleton'||(row.key==='mapping'&&!modelReport);
        const action=row.key==='mapping'&&!modelReport?'Analyze':row.action;
        const tone=analyzing&&row.key==='skeleton'?'working':row.tone;
        const label=analyzing&&row.key==='skeleton'?'Processing':row.status??(row.tone==='ready'?'Ready':row.tone==='attention'?'Needs attention':'Not checked');
        return <tr key={row.key}><th scope="row">{row.label}</th><td><StateLabel tone={tone}>{label}</StateLabel></td><td className="validation-detail">{row.value}</td><td><button className="tertiary" disabled={busy||(needsAnalysis&&!state.hasInputs)||(!needsAnalysis&&row.page!=='build'&&!go)} onClick={()=>rowAction(row.key,row.page,row.section)} aria-label={`${action} ${row.label.toLowerCase()}`}>{action}</button></td></tr>;
      })}</tbody></table></div>
    </section>
    <div className="review-controls">
    <section className={`build-action-panel ${blockers.length?'has-blockers':''}`} aria-label="Build decision">
      {blockers.length>0?<div className="build-blockers"><h2 id="build-requirements">{blockers.length} {blockers.length===1?'issue':'issues'} blocking build</h2><ul className="review-issues">{blockers.map((issue,index)=><li key={index}><span><span className="issue-symbol" aria-hidden="true">{issue.analyze?'—':'!'}</span>{issue.text}</span><button className={issue.analyze?'primary':'tertiary'} disabled={busy} onClick={()=>issue.analyze?inspect():act(issue.page,issue.section)}>{issue.analyze&&analyzing?'Analyzing…':issue.action??'View details'}</button></li>)}</ul></div>:<div className="build-ready"><StateLabel tone={state.ready?'ready':'neutral'}>{state.title}</StateLabel></div>}
      <div className="build-decision"><p id="build-explanation">{!state.hasInputs?'Choose the required inputs above to enable analysis.':pipelineSteps}</p><button className="primary build-primary" disabled={busy||!state.ready} aria-describedby="build-explanation" onClick={()=>build(overwriteOutput,overwriteInstall)}>Build mod</button></div>
    </section>
    <div className="output-summary"><span className="eyebrow">Output</span><strong>{output.value}</strong><span className="muted">{!project.compile_model&&modelEnabled(project)?'Prepare source files only':`${modelEnabled(project)?'Compile model':'Build extra assets'}${project.build_vpk?' · Package after validation':''}${project.build_vpk&&project.install?' · Install after success':''}`}</span></div>
    </div>
    </div>
    {result&&<Section title="Last completed build" actions={openOutput&&<button disabled={busy} onClick={openOutput}>Open output folder</button>}><dl className="summary-list"><dt>Model compilation</dt><dd>{result.compiled?'Completed':'Not performed'}</dd><dt>VPK</dt><dd><PathValue path={result.vpk} empty="Not packaged"/></dd><dt>Installation</dt><dd><PathValue path={result.installed} empty="Not installed"/></dd><dt>Receipt</dt><dd><PathValue path={result.manifest} empty="No package manifest"/></dd>{result.prepared&&<><dt>Prepared workspace</dt><dd><PathValue path={result.prepared}/></dd></>}</dl>{result.vpk&&!result.installed&&<button disabled={busy} onClick={()=>install(result.vpk,overwriteInstall)}>Validate and install this VPK</button>}</Section>}
    {report&&<Disclosure title="Analysis details" hint={fresh?`${report.warnings.length} review notes · full inspection available`:'Previous analysis · run Analyze project to refresh'} id="analysis-details">
      {modelReport?<>
        <p className={`status ${!fresh||modelReport.needs_review||modelReport.vertex_preflight.errors.length?'status-skipped':'status-succeeded'}`}>{!fresh?'Project changed · analyze again':modelReport.vertex_preflight.errors.length?'Blocking problems found':modelReport.needs_review?'Analysis needs review':'Analysis passed · compilation still required'}</p>
        <div className="inline-stats"><span><strong>{modelReport.dmx.bones.length.toLocaleString()}</strong> bones</span><span><strong>{modelReport.dmx.meshes.length.toLocaleString()}</strong> source meshes</span><span><strong>{modelReport.geometry.source_triangles.toLocaleString()}</strong> source triangles</span></div>
        {modelReport.vertex_preflight.errors.length>0&&<ul className="issue-list">{modelReport.vertex_preflight.errors.map((value,index)=><li key={index}>{value}</li>)}</ul>}
        {modelReport.warnings.length>0&&<ul className="issue-list">{modelReport.warnings.map((value,index)=><li key={index}>{value}</li>)}</ul>}
        <p className="hint">{modelReport.vertex_preflight.count_scope}</p>
      </>:<><p className="status">{fresh?'Extra assets inspected · compilation still required':'Project changed · analyze again'}</p><p>{componentReport?.icons.length??0} icons · {componentReport?.animations.length??0} animations · {componentReport?.effects.length??0} effects · {componentReport?.audio.length??0} audio replacements</p>{report.warnings.length>0&&<ul>{report.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}</>}
      <details className="technical"><summary>{modelReport?'Complete native inspection receipt':'Extra asset inspection receipt'}</summary><pre>{JSON.stringify(report,null,2)}</pre></details>
    </Disclosure>}
    {modelReport&&<Disclosure title="Mesh mapping" hint={`${modelReport.model.meshes.length} original render nodes · keep, replace or disable`} id="mesh-mapping">
      <MeshMapping report={modelReport} meshes={project.meshes} busy={busy} strategy={project.lod_strategy} update={meshes=>update({...project,meshes})}/>
    </Disclosure>}
    <Disclosure title="Build settings" hint="Detail levels and LOD acknowledgement" id="build-settings">
      <SelectField label="LOD strategy" value={project.lod_strategy} values={[["choose","Choose a strategy…"],["reuse","Reuse replacement at original levels"],["preserve","Preserve original lower-detail meshes"],["single","Single level"],["disable","Disable additional levels"],["custom","Use custom mesh mappings"]]} onChange={lod_strategy=>update({...project,lod_strategy,review_lods:false})} disabled={busy}/>
      <Check label="I have reviewed the LOD appearance and performance tradeoffs" value={project.review_lods} onChange={review_lods=>update({...project,review_lods})} disabled={busy}/><p className="hint">No simplified geometry is generated by changing this strategy. Original lower-detail meshes can restore the original appearance at a distance.</p>
    </Disclosure>
    <Disclosure title="Output settings" hint={`Slot ${project.vpk_number} · compilation, packaging and installation`} id="output-settings">
      <NumberField label="VPK slot" value={project.vpk_number} min={2} max={99} step={1} update={vpk_number=>update({...project,vpk_number})} disabled={busy}/><div className="row">{chooseFreeSlot&&<button disabled={busy} onClick={chooseFreeSlot}>Choose free slot</button>}<p className="mono">pak{String(project.vpk_number).padStart(2,'0')}_dir.vpk</p></div>
      <Check label="Compile replacement model" value={project.compile_model} onChange={compile_model=>update({...project,compile_model})} disabled={busy}/><Check label="Package a VPK after validation" value={project.build_vpk} onChange={build_vpk=>update({...project,build_vpk})} disabled={busy}/><Check label="Install after successful validation and packaging" value={project.install} onChange={install=>update({...project,install})} disabled={busy}/>
      <details className="technical"><summary>Replace existing output</summary><Check label="Replace an existing output VPK for this project and slot" value={overwriteOutput} onChange={setOverwriteOutput} disabled={busy}/><Check label="Replace an existing installed VPK at the selected slot" value={overwriteInstall} onChange={setOverwriteInstall} disabled={busy}/></details>
    </Disclosure>
    <Disclosure title="Developer / compiler tools" hint="Source preparation, compiler tests and existing VPK installation">
      <Section title="Compiler preparation" actions={<button disabled={busy||!modelReport||!fresh||modelReport.needs_review||Boolean(modelReport.vertex_preflight.errors.length)} onClick={()=>prepare(offline)}>Prepare source files</button>}><Check label="Prepare offline source workspace" value={offline} onChange={setOffline} disabled={busy}/><p className="hint">Preparation writes compiler sources without compiling, packaging or installing.</p>{prepared&&<dl className="summary-list"><dt>Content</dt><dd><PathValue path={prepared.content}/></dd><dt>Compiled output directory</dt><dd><PathValue path={prepared.game}/></dd><dt>Model</dt><dd><PathValue path={prepared.model}/></dd><dt>Generated materials</dt><dd>{prepared.generated_materials.length}</dd></dl>}</Section>
      <Section title="Original model test" actions={<div className="row"><button disabled={busy||!project.original_model} onClick={()=>validateOriginal(false)}>Inspect compiler plan</button><button disabled={busy||!project.original_model} onClick={()=>validateOriginal(true)}>Compile original test</button></div>}><p className="hint">Test the original character with the selected toolchain. The result is not packaged or installed.</p>{original&&<><p className="status">{original.compiled?'Original test compiled':'Original compiler plan inspected'}</p>{original.compiled_model&&<PathValue path={original.compiled_model}/>}<details className="technical"><summary>Original test receipt</summary><pre>{JSON.stringify(original,null,2)}</pre></details></>}</Section>
      <Section title="Install an existing VPK" actions={<button disabled={busy||!project.original_model} onClick={()=>install(null,overwriteInstall)}>Choose VPK to validate and install…</button>}><p className="hint">The selected VPK is copied privately and checked against the current game before installation.</p>{installed&&<div className="row"><StateLabel tone="ready">Installed</StateLabel><PathValue path={installed}/></div>}</Section>
    </Disclosure>
  </>;
}
