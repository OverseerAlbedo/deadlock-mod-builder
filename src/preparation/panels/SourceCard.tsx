// i18n-area: prepare
import { Suspense, type Dispatch, type SetStateAction } from 'react';
import { useT } from '../../i18n';
import type { Project } from '../../models';
import type { PreparationAnalysis, PreparationOptions, Vec3 } from '../models';
import { NumberField } from '../../components/MaterialEditor';
import { Disclosure } from '../../components/Primitives';
import { CompactPathField } from '../../components/Paths';
import { FilePathInput } from '../../components/FilePathInput';
import { BUTTON_NAMES, TAB_NAMES } from '../pageNames';
import type { GltfPreview } from '../../preview/gltf-packet';
import { StudioView } from '../../studio/Studio';
import { PrepIcon, type PrepPanel } from '../PreparationChrome';
import { GltfViewport, type Operate, type PrepView } from '../preparationShared';
/** The file types the model file field takes, shown next to it before a check. */
export const SOURCE_TYPES='BLEND · GLB · glTF · FBX · PMX · PMD';
// The model file field's placeholder names the formats of `sourceFormats.ts SOURCE_FORMATS_TEXT` in one whole sentence.
/** What was the "Your model" card: the model file, the gun file (before a check; afterwards it is
 * on the Gun tab), the orientation and the three start buttons. Shown openly before a check and in
 * the top bar's "Model file & orientation" menu after it. The gun file field shows its
 * label translated and passes its English `name` for the failure titles. */
export function SourceFields({analysis,source,busy,sourceChanged,browse,options,operate,changeOptions,rotation,invalidate,setRotation,setAnalysis,setMappingPreview,project,setPanel,expert,inspect,setView,loadPreview}:{analysis:PreparationAnalysis|null;source:string;busy:boolean;sourceChanged:(path:string)=>void;browse:()=>Promise<void>;options:PreparationOptions;operate:Operate;changeOptions:(values:Partial<PreparationOptions>)=>void;rotation:Vec3;invalidate:()=>void;setRotation:Dispatch<SetStateAction<Vec3>>;setAnalysis:Dispatch<SetStateAction<PreparationAnalysis|null>>;setMappingPreview:Dispatch<SetStateAction<GltfPreview|null>>;project:Project;setPanel:Dispatch<SetStateAction<PrepPanel|null>>;expert:boolean;inspect:(auto?:boolean)=>Promise<void>;setView:Dispatch<SetStateAction<PrepView>>;loadPreview:(mapping?:boolean)=>Promise<void>}){
  const t=useT();
  return (
    <div className={`prep-source ${analysis?'is-inspected':''}`} id="prep-source">
      <CompactPathField label={t('Model file')} value={source} disabled={busy} update={sourceChanged} browse={browse} placeholder={t('Choose a Blender, GLB, glTF, FBX or MMD (.pmx, .pmd) file…')}/>
      {!analysis&&<div className="prep-source-gun"><FilePathInput label={t('Gun from another file (optional)')} name="Gun from another file (optional)" kind="weapon" value={typeof options.weapon_source==='string'?options.weapon_source:''} disabled={busy} fail={(title,error)=>void operate(title,async()=>{throw error;})} update={path=>changeOptions({weapon_source:path||undefined,weapon:undefined,reviewed:false})}/><p className="hint">{t('Only if your gun is in a separate file; it is prepared together with your character.')}</p></div>}
      {analysis&&<p className="hint">{t('The gun file is on the {tab} tab.',{tab:t(TAB_NAMES.weapon)})}</p>}
      <Disclosure title={t('Model orientation')} hint={t('Optional: turn the model before checking it (lying-down models are turned upright automatically)')}><div className="override-grid">{(['X','Y','Z'] as const).map((axis,index)=><NumberField key={axis} label={t('Source rotation {axis} (degrees)',{axis})} value={rotation[index]} min={-360} max={360} step={1} disabled={busy} update={value=>{const next=[...rotation] as Vec3;next[index]=value;invalidate();setRotation(next);setAnalysis(null);setMappingPreview(null);}}/>)}</div></Disclosure>
      <div className="prep-source-actions"><button className="prep-primary" disabled={busy||!source||!project.original_model} onClick={()=>{setPanel(expert?'roles':null);void inspect(true);}}><PrepIcon name="review" size={18}/>{t('Prepare automatically')}</button><button className="prep-secondary" disabled={busy||!source||!project.original_model} onClick={()=>{setPanel(expert?'roles':null);void inspect();}}>{t(BUTTON_NAMES.loadAndCheck)}</button><button disabled={busy||!source} onClick={()=>{setView('model');void loadPreview();}}><PrepIcon name="eye" size={18}/>{t('Preview the model and animation')}</button></div>
      <p className="hint">{project.original_model?t('Checks, matches and prepares your model in one go, and stops only where a choice is needed. Your file is never changed.'):t('Choose the hero in Character & model first.')}</p>
    </div>
  );
}

/** The source preview shown before the model is checked, in the frame's view card. */
export function SourcePreview({preview,setPreview}:{preview:GltfPreview;setPreview:Dispatch<SetStateAction<GltfPreview|null>>}){
  const t=useT();
  return (
    <StudioView title={t('Source preview')} className="prep-source-preview" bar={<strong className="prep-view-title">{t('Source preview')}</strong>} barEnd={<button type="button" onClick={()=>setPreview(null)}>{t('Close source preview')}</button>}><Suspense fallback={<p className="studio-view-wait">{t('Loading source renderer…')}</p>}><GltfViewport preview={preview}/></Suspense></StudioView>
  );
}
