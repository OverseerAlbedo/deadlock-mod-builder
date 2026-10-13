import { useEffect, useRef, useState } from 'react';
import { api } from '../api';
import type { JsonValue, Project } from '../models';
import { Section } from '../components/Primitives';
import { samePath, WeaponPanel, weaponDefaults } from './WeaponPanel';
import type { SeparateGunProps } from './SeparateGun';
import type { PreparationAnalysis, PreparationOptions } from './models';
import { gunChoiceOptions } from './secondary-bones';
type Operate=(label:string,operation:()=>Promise<void>,cancel?:boolean)=>Promise<unknown>;
const asOptions=(options:PreparationOptions)=>JSON.parse(JSON.stringify(options)) as Record<string,JsonValue>;
const gunOf=(options:PreparationOptions)=>JSON.stringify([options.weapon??null,options.weapon_source??'']);
/** Reopened preparations by report path, so returning to this step does not read them again. */
const opened=new Map<string,PreparationAnalysis>();

/** Step 3: the gun of a prepared model, changed and applied here (the model is prepared again in one click). */
export function WeaponsSection({project,update,busy,operate,refreshPreview,openPreparation,separate}:{
  project:Project;update:(project:Project)=>void;busy:boolean;operate:Operate;
  /** Reload the 3D preview of the prepared model after the gun changed. */refreshPreview?:()=>void;
  /** Go to Prepare model and prepare again (a new gun file needs a new inspection). */openPreparation:()=>void;
  separate?:Omit<SeparateGunProps,'project'|'analysis'|'options'|'busy'|'operate'|'change'|'updateProject'>;
}){
  const updateRef=useRef(update);updateRef.current=update;
  const [analysis,setAnalysis]=useState<PreparationAnalysis|null>(null),[applied,setApplied]=useState(''),[failed,setFailed]=useState('');
  const options=project.preparation_options as PreparationOptions;
  const report=project.preparation_report;
  const loaded=useRef('');
  async function load(){
    if(!report)return;
    loaded.current=report;
    const known=opened.get(report);
    if(known){setAnalysis(known);setApplied(gunOf(options));setFailed('');return;}
    await operate('The saved preparation could not be opened',async()=>{
      try{
        const value=await api.preparationResume(project);
        opened.set(report,value.analysis);
        setAnalysis(value.analysis);setApplied(gunOf(options));setFailed('');
      }catch(error){setFailed(String(error instanceof Error?error.message:error));throw error;}
    },true);
  }
  useEffect(()=>{if(report&&loaded.current!==report&&!busy){setAnalysis(null);void load();}},[report,busy]);// eslint-disable-line react-hooks/exhaustive-deps
  if(!report)return <Section title="Weapons" id="weapons"><p className="hint">{project.dmx?'Your model was added as a ready-made DMX, so its gun is used as it is. To replace the hero\'s gun with yours, prepare your Blender, FBX or glTF model in Prepare model.':'Prepare your model first; then you can replace the hero\'s gun here.'}</p></Section>;
  if(!analysis)return <Section title="Weapons" id="weapons"><p className="hint">{failed?`The saved preparation could not be opened: ${failed}`:'Opening the prepared model\'s gun…'}</p>{failed&&<button disabled={busy} onClick={()=>void load()}>Try again</button>}</Section>;
  const change=(values:Partial<PreparationOptions>)=>{const next={...options,...gunChoiceOptions(analysis.analysis.bones,analysis.analysis.weighted_bones,options,values)};updateRef.current({...project,preparation_options:asOptions(next)} as Project);};
  const pending=gunOf(options)!==applied;
  // The same file may be spelled differently (\\?\ prefix, case, slashes).
  const needsInspection=Boolean(options.weapon_source||analysis.weapon_file)&&!samePath(options.weapon_source??'',analysis.weapon_file?.selection.source??'');
  /** Prepare the model again with the new gun choice (same inspection) and use it. */
  async function apply(){
    if(!analysis)return;
    await operate('The gun change could not be applied',async()=>{
      const chosen={...options,reviewed:true,...(Object.keys(options.mapping_anchors??{}).length?{follow_motion_reviewed:true}:{})};
      const ready={...project,preparation_options:asOptions(chosen)} as Project;
      const issues=await api.preparationReview(ready,analysis,chosen);
      if(issues.length)throw new Error(issues.map(i=>i.message).join('\n'));
      const converted=await api.preparationConvert(ready,analysis,chosen,false);
      const accepted=await api.preparationAccept(ready,converted);
      updateRef.current(accepted);
      setApplied(gunOf(accepted.preparation_options as PreparationOptions));
      // The same inspection made this preparation too.
      opened.set(accepted.preparation_report,analysis);
      loaded.current=accepted.preparation_report;
      refreshPreview?.();
    },true);
  }
  const shown:PreparationOptions=options.weapon?options:{...options,weapon:weaponDefaults(analysis,undefined)};
  return <section className="weapons-step" id="weapons" aria-label="Weapons">
    <div className="prep-workspace"><WeaponPanel project={project} analysis={analysis} options={shown} change={change} busy={busy} operate={operate} reinspect={openPreparation} separate={separate?{...separate,updateProject:value=>updateRef.current(value)}:undefined}/>
    {(pending||needsInspection)&&<div className="prep-card prep-gun-apply" role="status"><div><strong>{needsInspection?'Your gun file changed.':'Your gun changes are not in the model yet.'}</strong><p className="hint">{needsInspection?'The new gun file is loaded when the model is prepared again.':'Apply them to prepare the model again with your gun (about a minute); the preview then shows the result on your character.'}</p></div>
      {needsInspection?<button className="prep-primary" disabled={busy} onClick={openPreparation}>Prepare again with this gun file</button>:<button className="prep-primary" disabled={busy} onClick={()=>void apply()}>Apply gun changes</button>}</div>}
    </div>
  </section>;
}
