import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import type {Project} from '../models';
import {Section} from '../components/Primitives';
import type {ClothAnalysis,ClothAnalysisResult,ClothAuthoringReceipt} from './models';
import {m,useT} from '../i18n';
// i18n-area: physics
type Props={project:Project;update:(project:Project)=>void;busy:boolean;operate:(label:string,fn:()=>Promise<void>,cancel?:boolean)=>Promise<unknown>;onPreview?:(path:string)=>void|Promise<void>};
export function ClothAuthoringPanel({project,busy,operate,onPreview}:Props){
 const t=useT();
 const [analysis,setAnalysis]=useState<ClothAnalysis|null>(null),[receipt,setReceipt]=useState<ClothAuthoringReceipt|null>(null),[reportPath,setReportPath]=useState('');
 const projectKey=JSON.stringify(project);
 useEffect(()=>{setAnalysis(null);setReceipt(null);setReportPath('');},[projectKey]);
 const selection=project.customization.cloth_setup?.meshes??[];
 const enabled=Boolean(project.dmx&&project.original_model&&(!project.customization.enabled||project.customization.model_enabled!==false));
 const analyze=()=>operate(m('Analyze skirt bones'),async()=>{setAnalysis(null);setReceipt(null);setReportPath('');const value=await invoke<ClothAnalysisResult>('skirt_analyze',{project});setAnalysis(value.analysis);setReportPath(value.report_path);},true);
 const author=(automatic:boolean)=>operate(automatic?'Create automatic cloth preview':'Create custom-bone skirt preview',async()=>{setAnalysis(null);setReceipt(null);setReportPath('');const value=await invoke<ClothAuthoringReceipt>(automatic?'cloth_setup':'skirt_author',{project});setAnalysis(value.analysis);setReceipt(value);setReportPath(value.report_path);},true);
 return <Section title={t('Create a cloth test workspace')}><p className="hint">{t('Automatic setup uses the selected open cloth mesh and its existing skin weights. It creates an isolated render mask, simulation proxy, motion maps, and proposed leg capsules for review. These files require native ModelDoc and in-game testing before any new solver can be used in a build.')}</p>
  <p>{t('Selected mesh:')} <strong>{selection[0]||t('Choose a cloth mesh above.')}</strong></p>
  <div className="row"><button className="primary" disabled={busy||!enabled||selection.length!==1} onClick={()=>void author(true)}>{t('Create automatic cloth preview')}</button></div>
  <details className="technical"><summary>{t('Existing custom skirt bones')}</summary><p className="hint">{t('For a game-ready DMX retaining weighted skirt bone chains, analyze the original hero and selected branches before creating a separate preview. This route requires an original hero with audited existing cloth.')}</p><div className="row"><button disabled={busy||!enabled} onClick={()=>void analyze()}>{t('Analyze skirt bones')}</button><button disabled={busy||!enabled} onClick={()=>void author(false)}>{t('Create skirt bone preview')}</button></div></details>
  {analysis&&<div className="editor-fields"><p role="status">{receipt?t('Cloth workspace created for review. Runtime compatibility has not been established.'):t('Cloth analysis saved. Runtime compatibility has not been established.')}</p><dl className="summary-list">{analysis.kind==='automatic_cloth'?<><dt>{t('Dynamic positions')}</dt><dd>{analysis.summary.dynamic_position_count.toLocaleString()}</dd><dt>{t('Pinned positions')}</dt><dd>{analysis.summary.pinned_position_count.toLocaleString()}</dd></>:<><dt>{t('Skirt chains')}</dt><dd>{analysis.summary.chain_count.toLocaleString()}</dd><dt>{t('Custom bones')}</dt><dd>{analysis.summary.custom_bone_count.toLocaleString()}</dd></>}<dt>{t('Proposed leg colliders')}</dt><dd>{analysis.summary.leg_collider_count.toLocaleString()}</dd><dt>{t('Overlapping positions at rest')}</dt><dd>{analysis.summary.overlapping_position_count.toLocaleString()}</dd></dl>
   {analysis.blockers.length>0&&<div role="alert"><strong>{t('Review blockers')}</strong><ul>{analysis.blockers.map((message,i)=><li key={i}>{message}</li>)}</ul></div>}
   {analysis.warnings.length>0&&<ul>{analysis.warnings.map((message,i)=><li key={i}>{message}</li>)}</ul>}
   {analysis.leg_colliders.length>0&&<details><summary>{t('Proposed collision fit')}</summary><table><thead><tr><th>{t('Bone')}</th><th>{t('World radius')}</th><th>{t('Source')}</th></tr></thead><tbody>{analysis.leg_colliders.map((collider,index)=><tr key={index}><td>{collider.bone}</td><td>{collider.radius_world.toPrecision(5)} in</td><td>{collider.origin.replaceAll('_',' ')}</td></tr>)}</tbody></table><p className="hint">{t('These are unapproved proposals. Inspect their position and contact during animation.')}</p></details>}
   {analysis.kind==='skirt_analysis'&&<details><summary>{t('Original solver inventory')}</summary><p>{analysis.original_solver.status==='inventoried'?t('{nodes} nodes; {controls} controls.',{nodes:String(analysis.original_solver.nodes),controls:String(analysis.original_solver.control_count)}):analysis.original_solver.reason??t('No embedded solver was inventoried.')}</p><p className="hint">{analysis.original_solver.authored_merge_reason}</p></details>}
  </div>}
  {receipt&&<div className="editor-fields"><h3>{t('Review files')}</h3><dl className="summary-list"><dt>{t('Workspace')}</dt><dd className="path">{receipt.workspace_path}</dd><dt>{t('Before-cloth ModelDoc')}</dt><dd className="path">{receipt.preview_before_model_path}</dd><dt>{t('Cloth ModelDoc')}</dt><dd className="path">{receipt.preview_model_path}</dd><dt>{t('Full authored ModelDoc')}</dt><dd className="path">{receipt.model_path}</dd></dl>
   {onPreview&&<div className="row"><button disabled={busy} onClick={()=>void onPreview(receipt.surface.render_path)}>{t('Inspect staged render mesh')}</button><button disabled={busy} onClick={()=>void onPreview(receipt.surface.proxy_path)}>{t('Inspect simulation proxy')}</button></div>}
   <p className="hint">{t('The viewport shows geometry. Test cloth simulation and compare both ModelDocs in the native toolset. The preview copies omit external AG2 graphs; the full authored source retains them.')}</p>
   {receipt.geometry_previews.missing_staged_references.length>0&&<details open><summary>{t('Unstaged dependencies')}</summary><ul>{receipt.geometry_previews.missing_staged_references.map(path=><li key={path}><code>{path}</code></li>)}</ul></details>}
   {receipt.pending_gates.length>0&&<details><summary>{t('Remaining native checks')}</summary><ul>{receipt.pending_gates.map((gate,index)=><li key={index}>{gate}</li>)}</ul></details>}
  </div>}
  {reportPath&&<p className="hint">{t('Report:')} <code className="path">{reportPath}</code></p>}
 </Section>;
}
