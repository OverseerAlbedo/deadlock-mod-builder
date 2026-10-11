import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import type {Settings} from '../models';
import {Section} from '../components/Primitives';
type Candidate={backend:string;root:string;compiler:string;game:string;configuration_found:boolean;ag2_test:string};
type Report={path:string;report:unknown};
type Blender={path:string;version:string;launch_verified:boolean;conversion_certified:boolean};
type Props={settings:Settings;blenderSaved:(path:string)=>void;disabled:boolean;operate:(label:string,fn:()=>Promise<void>,cancel?:boolean)=>Promise<unknown>};
export function ToolDiagnostics({settings,blenderSaved,disabled,operate}:Props){
  const[candidates,setCandidates]=useState<Candidate[]|null>(null),[report,setReport]=useState<Report|null>(null),[blender,setBlender]=useState<Blender|null>(null);
  useEffect(()=>{if(blender&&blender.path!==settings.blender)setBlender(null);},[settings.blender,blender]);
  return <Section title="Tool diagnostics"><p className="hint">Inspect available compiler paths, dependencies, and class names. Static evidence does not replace the original-hero compatibility test.</p>
    <div className="row"><button disabled={disabled} onClick={()=>void operate('Find model compiler candidates',async()=>{setCandidates(await invoke<Candidate[]>('tool_candidates',{settings}));},true)}>Find compiler candidates</button>
      <button disabled={disabled} onClick={()=>void operate('Audit compiler files and mounts',async()=>{setReport(null);setReport(await invoke<Report>('tool_diagnostics',{settings,classes:false}));},true)}>Audit configuration</button>
      <button disabled={disabled} onClick={()=>void operate('Scan compiler class evidence',async()=>{setReport(null);setReport(await invoke<Report>('tool_diagnostics',{settings,classes:true}));},true)}>Scan class evidence</button>
      <button disabled={disabled} onClick={()=>void operate('Validate Blender',async()=>{setBlender(null);const result=await invoke<Blender>('validate_blender',{executable:settings.blender||null});blenderSaved(result.path);setBlender(result);},true)}>Validate Blender</button></div>
    <p className="hint">Successful Blender validation saves its executable path on this PC. Other edited settings still need Save settings.</p>
    {candidates&&<details open><summary>{candidates.length} compiler candidates</summary>{candidates.length===0?<p>No compiler executable found in the detected installations.</p>:<table><thead><tr><th>Compiler</th><th>Game configuration</th><th>Native test</th></tr></thead><tbody>{candidates.map(candidate=><tr key={candidate.compiler}><td className="path">{candidate.compiler}</td><td>{candidate.configuration_found?'Found':'Missing'}</td><td>{candidate.ag2_test}</td></tr>)}</tbody></table>}</details>}
    {blender&&<p role="status">Blender {blender.version} launched successfully and its path is saved on this PC. Conversion remains separately validated.</p>}
    {report&&<details open><summary>Diagnostic report</summary><p>Complete report: <code className="path">{report.path}</code></p><pre>{JSON.stringify(report.report,null,2).slice(0,60000)}</pre><p className="hint">The display is limited to the first 60,000 characters; the saved report contains the complete evidence.</p></details>}
  </Section>;
}
