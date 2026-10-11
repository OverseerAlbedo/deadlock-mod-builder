import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import type {Project} from '../models';
import {Section} from '../components/Primitives';
import type {ClothAnalysis,ClothAnalysisResult,ClothAuthoringReceipt} from './models';
type Props={project:Project;update:(project:Project)=>void;busy:boolean;operate:(label:string,fn:()=>Promise<void>,cancel?:boolean)=>Promise<unknown>;onPreview?:(path:string)=>void|Promise<void>};
export function ClothAuthoringPanel({project,busy,operate,onPreview}:Props){
 const [analysis,setAnalysis]=useState<ClothAnalysis|null>(null),[receipt,setReceipt]=useState<ClothAuthoringReceipt|null>(null),[reportPath,setReportPath]=useState('');
 const projectKey=JSON.stringify(project);
 useEffect(()=>{setAnalysis(null);setReceipt(null);setReportPath('');},[projectKey]);
 const selection=project.customization.cloth_setup?.meshes??[];
 const enabled=Boolean(project.dmx&&project.original_model&&(!project.customization.enabled||project.customization.model_enabled!==false));
 const analyze=()=>operate('Analyze skirt bones',async()=>{setAnalysis(null);setReceipt(null);setReportPath('');const value=await invoke<ClothAnalysisResult>('skirt_analyze',{project});setAnalysis(value.analysis);setReportPath(value.report_path);},true);
 const author=(automatic:boolean)=>operate(automatic?'Create automatic cloth preview':'Create custom-bone skirt preview',async()=>{setAnalysis(null);setReceipt(null);setReportPath('');const value=await invoke<ClothAuthoringReceipt>(automatic?'cloth_setup':'skirt_author',{project});setAnalysis(value.analysis);setReceipt(value);setReportPath(value.report_path);},true);
 return <Section title="Create a cloth test workspace"><p className="hint">Automatic setup uses the selected open cloth mesh and its existing skin weights. It creates an isolated render mask, simulation proxy, motion maps, and proposed leg capsules for review. These files require native ModelDoc and in-game testing before any new solver can be used in a build.</p>
  <p>Selected mesh: <strong>{selection[0]||'Choose a cloth mesh above.'}</strong></p>
  <div className="row"><button className="primary" disabled={busy||!enabled||selection.length!==1} onClick={()=>void author(true)}>Create automatic cloth preview</button></div>
  <details className="technical"><summary>Existing custom skirt bones</summary><p className="hint">For a game-ready DMX retaining weighted skirt bone chains, analyze the original hero and selected branches before creating a separate preview. This route requires an original hero with audited existing cloth.</p><div className="row"><button disabled={busy||!enabled} onClick={()=>void analyze()}>Analyze skirt bones</button><button disabled={busy||!enabled} onClick={()=>void author(false)}>Create skirt bone preview</button></div></details>
  {analysis&&<div className="editor-fields"><p role="status">{receipt?'Cloth workspace created for review':'Cloth analysis saved'}. Runtime compatibility has not been established.</p><dl className="summary-list">{analysis.kind==='automatic_cloth'?<><dt>Dynamic positions</dt><dd>{analysis.summary.dynamic_position_count.toLocaleString()}</dd><dt>Pinned positions</dt><dd>{analysis.summary.pinned_position_count.toLocaleString()}</dd></>:<><dt>Skirt chains</dt><dd>{analysis.summary.chain_count.toLocaleString()}</dd><dt>Custom bones</dt><dd>{analysis.summary.custom_bone_count.toLocaleString()}</dd></>}<dt>Proposed leg colliders</dt><dd>{analysis.summary.leg_collider_count.toLocaleString()}</dd><dt>Overlapping positions at rest</dt><dd>{analysis.summary.overlapping_position_count.toLocaleString()}</dd></dl>
   {analysis.blockers.length>0&&<div role="alert"><strong>Review blockers</strong><ul>{analysis.blockers.map((message,i)=><li key={i}>{message}</li>)}</ul></div>}
   {analysis.warnings.length>0&&<ul>{analysis.warnings.map((message,i)=><li key={i}>{message}</li>)}</ul>}
   {analysis.leg_colliders.length>0&&<details><summary>Proposed collision fit</summary><table><thead><tr><th>Bone</th><th>World radius</th><th>Source</th></tr></thead><tbody>{analysis.leg_colliders.map((collider,index)=><tr key={index}><td>{collider.bone}</td><td>{collider.radius_world.toPrecision(5)} in</td><td>{collider.origin.replaceAll('_',' ')}</td></tr>)}</tbody></table><p className="hint">These are unapproved proposals. Inspect their position and contact during animation.</p></details>}
   {analysis.kind==='skirt_analysis'&&<details><summary>Original solver inventory</summary><p>{analysis.original_solver.status==='inventoried'?`${analysis.original_solver.nodes} nodes; ${analysis.original_solver.control_count} controls.`:analysis.original_solver.reason??'No embedded solver was inventoried.'}</p><p className="hint">{analysis.original_solver.authored_merge_reason}</p></details>}
  </div>}
  {receipt&&<div className="editor-fields"><h3>Review files</h3><dl className="summary-list"><dt>Workspace</dt><dd className="path">{receipt.workspace_path}</dd><dt>Before-cloth ModelDoc</dt><dd className="path">{receipt.preview_before_model_path}</dd><dt>Cloth ModelDoc</dt><dd className="path">{receipt.preview_model_path}</dd><dt>Full authored ModelDoc</dt><dd className="path">{receipt.model_path}</dd></dl>
   {onPreview&&<div className="row"><button disabled={busy} onClick={()=>void onPreview(receipt.surface.render_path)}>Inspect staged render mesh</button><button disabled={busy} onClick={()=>void onPreview(receipt.surface.proxy_path)}>Inspect simulation proxy</button></div>}
   <p className="hint">The viewport shows geometry. Test cloth simulation and compare both ModelDocs in the native toolset. The preview copies omit external AG2 graphs; the full authored source retains them.</p>
   {receipt.geometry_previews.missing_staged_references.length>0&&<details open><summary>Unstaged dependencies</summary><ul>{receipt.geometry_previews.missing_staged_references.map(path=><li key={path}><code>{path}</code></li>)}</ul></details>}
   {receipt.pending_gates.length>0&&<details><summary>Remaining native checks</summary><ul>{receipt.pending_gates.map((gate,index)=><li key={index}>{gate}</li>)}</ul></details>}
  </div>}
  {reportPath&&<p className="hint">Report: <code className="path">{reportPath}</code></p>}
 </Section>;
}
