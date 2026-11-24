import { Suspense, type Dispatch, type SetStateAction } from 'react';
import type { Project } from '../../models';
import type { PreparationAnalysis, PreparationOptions, Vec3 } from '../models';
import { NumberField } from '../../components/MaterialEditor';
import { Disclosure } from '../../components/Primitives';
import { CompactPathField } from '../../components/Paths';
import { FilePathInput } from '../../components/FilePathInput';
import { SOURCE_FORMATS_TEXT } from '../sourceFormats';
import { BUTTON_NAMES, TAB_NAMES } from '../pageNames';
import type { GltfPreview } from '../../preview/gltf-packet';
import { StudioView } from '../../studio/Studio';
import { PrepIcon, type PrepPanel } from '../PreparationChrome';
import { GltfViewport, type Operate, type PrepView } from '../preparationShared';
/** The file types the model file field takes, shown next to it before a check. */
export const SOURCE_TYPES='BLEND · GLB · glTF · FBX · PMX · PMD';
/** What was the "Your model" card: the model file, the gun file (before a check; afterwards it is
 * on the Gun tab), the orientation and the three start buttons. Shown openly before a check and in
 * the top bar's "Model file & orientation" menu after it. */
export function SourceFields({analysis,source,busy,sourceChanged,browse,options,operate,changeOptions,rotation,invalidate,setRotation,setAnalysis,setMappingPreview,project,setPanel,expert,inspect,setView,loadPreview}:{analysis:PreparationAnalysis|null;source:string;busy:boolean;sourceChanged:(path:string)=>void;browse:()=>Promise<void>;options:PreparationOptions;operate:Operate;changeOptions:(values:Partial<PreparationOptions>)=>void;rotation:Vec3;invalidate:()=>void;setRotation:Dispatch<SetStateAction<Vec3>>;setAnalysis:Dispatch<SetStateAction<PreparationAnalysis|null>>;setMappingPreview:Dispatch<SetStateAction<GltfPreview|null>>;project:Project;setPanel:Dispatch<SetStateAction<PrepPanel|null>>;expert:boolean;inspect:(auto?:boolean)=>Promise<void>;setView:Dispatch<SetStateAction<PrepView>>;loadPreview:(mapping?:boolean)=>Promise<void>}){
  return (
    <div className={`prep-source ${analysis?'is-inspected':''}`} id="prep-source">
      <CompactPathField label="Model file" value={source} disabled={busy} update={sourceChanged} browse={browse} placeholder={`Choose a ${SOURCE_FORMATS_TEXT} file…`}/>
      {!analysis&&<div className="prep-source-gun"><FilePathInput label="Gun from another file (optional)" kind="weapon" value={typeof options.weapon_source==='string'?options.weapon_source:''} disabled={busy} fail={(title,error)=>void operate(title,async()=>{throw error;})} update={path=>changeOptions({weapon_source:path||undefined,weapon:undefined,reviewed:false})}/><p className="hint">Only if your gun is in a separate file; it is prepared together with your character.</p></div>}
      {analysis&&<p className="hint">The gun file is on the {TAB_NAMES.weapon} tab.</p>}
      <Disclosure title="Model orientation" hint="Optional: turn the model before checking it (lying-down models are turned upright automatically)"><div className="override-grid">{(['X','Y','Z'] as const).map((axis,index)=><NumberField key={axis} label={`Source rotation ${axis} (degrees)`} value={rotation[index]} min={-360} max={360} step={1} disabled={busy} update={value=>{const next=[...rotation] as Vec3;next[index]=value;invalidate();setRotation(next);setAnalysis(null);setMappingPreview(null);}}/>)}</div></Disclosure>
      <div className="prep-source-actions"><button className="prep-primary" disabled={busy||!source||!project.original_model} onClick={()=>{setPanel(expert?'roles':null);void inspect(true);}}><PrepIcon name="review" size={18}/>Prepare automatically</button><button className="prep-secondary" disabled={busy||!source||!project.original_model} onClick={()=>{setPanel(expert?'roles':null);void inspect();}}>{BUTTON_NAMES.loadAndCheck}</button><button disabled={busy||!source} onClick={()=>{setView('model');void loadPreview();}}><PrepIcon name="eye" size={18}/>Preview the model and animation</button></div>
      <p className="hint">{project.original_model?'Checks, matches and prepares your model in one go, and stops only where a choice is needed. Your file is never changed.':'Choose the hero in Character & model first.'}</p>
    </div>
  );
}

/** The source preview shown before the model is checked, in the frame's view card. */
export function SourcePreview({preview,setPreview}:{preview:GltfPreview;setPreview:Dispatch<SetStateAction<GltfPreview|null>>}){
  return (
    <StudioView title="Source preview" className="prep-source-preview" bar={<strong className="prep-view-title">Source preview</strong>} barEnd={<button type="button" onClick={()=>setPreview(null)}>Close source preview</button>}><Suspense fallback={<p className="studio-view-wait">Loading source renderer…</p>}><GltfViewport preview={preview}/></Suspense></StudioView>
  );
}
