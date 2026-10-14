import { SecondaryMotion } from './SecondaryMotion';
import { ImagePreview } from './ImagePreview';
import { DestinationPicker, type ComponentCatalog } from './AssetCatalog';
import { useId, useState, type ReactNode } from 'react';
import type { AssetKind, ClothingChoices, ClothSetupChoices, ModelSummary, PathKind, PhysicsChoices, SecondaryMotionChoices, Project, ResourceReplacement } from '../models';
import { api } from '../api';
import { FilePathInput } from './FilePathInput';
import { Disclosure, PageTitle, Section, StateLabel } from './Primitives';
import { PathValue } from './Paths';
import { modelEnabled, testedPhysicsEnabled } from '../build/readiness';
import type { ProjectInspection } from '../build/models';
import { includedClothHero, qualifiedIncludedCloth } from '../cloth/includedSetup';
import { ExtrasWorkspace, type ExtrasProps } from '../extras/ExtrasPage';
import { changeCustomization } from './customization';
export { changeCustomization } from './customization';

type Props={catalog?:ComponentCatalog|null;discover?:()=>void;project:Project;update:(project:Project)=>void;disabled:boolean;fail:(title:string,error:unknown)=>void};
export function Check({label,value,onChange,disabled}:{label:string;value:boolean;onChange:(value:boolean)=>void;disabled:boolean}){
  return <label className="checkbox checkbox-line"><input type="checkbox" checked={value} disabled={disabled} onChange={event=>onChange(event.target.checked)}/>{label}</label>;
}
export function SelectField({label,value,values,onChange,disabled}:{label:string;value:string;values:readonly(readonly[string,string])[];onChange:(value:string)=>void;disabled:boolean}){
  const id=useId();return <div className="field"><label htmlFor={id}>{label}</label><select id={id} value={value} disabled={disabled} onChange={event=>onChange(event.target.value)}>{values.map(([key,text])=><option value={key} key={key}>{text}</option>)}</select></div>;
}
export function PhysicsPage({project,update,disabled,fail,summary,children,report=null,fresh=false,openPreparation,preview,expert=true}:{expert?:boolean;report?:ProjectInspection|null;fresh?:boolean;summary:ModelSummary|null;children?:ReactNode;openPreparation?:()=>void;/** The 3D view of the prepared character. */preview?:ReactNode}&Props){
  const physics=project.customization.physics??{},cloth=project.customization.cloth_setup??{},clothing=project.customization.clothing??{};
  const physicsUpdate=(values:Partial<PhysicsChoices>)=>update(changeCustomization(project,{physics:{...physics,...values,hero_path:project.original_model},...(values.enabled?{secondary_motion:{...project.customization.secondary_motion,enabled:false}}:{})}));
  const secondaryUpdate=(values:Partial<SecondaryMotionChoices>)=>update(changeCustomization(project,{secondary_motion:{...project.customization.secondary_motion,...values},physics:{...physics,enabled:false}}));
  const preserve=()=>update(changeCustomization(project,{physics:{...physics,enabled:false},secondary_motion:{...project.customization.secondary_motion,enabled:false}}));
  const clothUpdate=(values:Partial<ClothSetupChoices>)=>update(changeCustomization(project,{cloth_setup:{...cloth,...values}}));
  const clothingUpdate=(values:Partial<ClothingChoices>)=>update(changeCustomization(project,{clothing:{...clothing,...values}}));
  async function included(){try{const choice=await api.includedPhysics(project.original_model);update(changeCustomization(project,{physics:{...physics,...choice},secondary_motion:{...project.customization.secondary_motion,enabled:false}}));}catch(error){fail('Could not select the included cloth setup',error);}}
  const meshes=summary?.path===project.dmx?summary.meshes.map(mesh=>mesh.name):[];
  const selected=cloth.meshes?.[0]??'', tested=testedPhysicsEnabled(project);
  const wanted=Boolean(project.customization.enabled&&physics.enabled),generated=Boolean(project.customization.enabled&&project.customization.secondary_motion?.enabled);
  const includedAvailable=includedClothHero(project), qualified=qualifiedIncludedCloth(project,report,fresh);
  return <><PageTitle eyebrow="Step 4 of 5" title="Physics & preview" subtitle="Choose how hair, cloth, breasts and gun charms move, then look at your character."/>
    {preview}
    <Section title="Physics mode" className="physics-mode-section">
      <fieldset className="mode-options"><legend className="sr-only">Physics mode</legend>
        <label className={`mode-option ${!wanted&&!generated?'selected':''}`}><input type="radio" name="physics-mode" checked={!wanted&&!generated} disabled={disabled} onChange={preserve}/><span><strong>Preserve original physics <small>Default</small></strong><span>Keep the original character’s supported hair and cloth movement.</span></span></label>
        <label className={`mode-option ${wanted?'selected':''}`}><input type="radio" name="physics-mode" checked={wanted} disabled={disabled} onChange={()=>physicsUpdate({enabled:true})}/><span><strong>Replacement cloth setup</strong><span>Use a tested hair and cloth setup made for your replacement model.</span></span></label>
        <label className={`mode-option ${generated?'selected':''}`}><input type="radio" name="physics-mode" checked={generated} disabled={disabled} onChange={()=>secondaryUpdate({enabled:true})}/><span><strong>Automatic secondary motion</strong><span>Generate movement for the replacement’s preserved hair, ears, tail and clothing bones.</span></span></label>
      </fieldset>
      {!modelEnabled(project)&&<p className="hint">This project keeps the original model. Enable the replacement model in Extras to use tested cloth physics.</p>}
      {!project.customization.enabled&&physics.enabled&&<p className="hint">A tested setup is saved but inactive while project customization is off.</p>}
    </Section>
    {generated&&<Section title="Automatic motion" className="automatic-motion-section"><SecondaryMotion expert={expert} project={project} update={secondaryUpdate} disabled={disabled} report={report} fresh={fresh} preparation={values=>update({...project,preparation_options:{...project.preparation_options,...values}})} openPreparation={openPreparation}/></Section>}
    {qualified&&<div className="included-cloth-summary"><StateLabel tone="ready">Vindicta balanced cloth</StateLabel><p>Included hair and skirt reference · source binding checked. Compiled output is validated during build.</p></div>}
    {wanted&&<div className="cloth-selection-summary"><StateLabel tone={tested&&physics.source?'neutral':'attention'}>{tested&&physics.source?'Replacement reference selected':!modelEnabled(project)?'Replacement model required':'Choose a tested reference'}</StateLabel>{physics.source&&<PathValue path={physics.source}/>}</div>}
    <Disclosure title="Configure replacement cloth" hint="Tested compiled reference and activation setting" open={wanted&&!physics.source?true:undefined}>
      {includedAvailable&&<div className="included-cloth-choice"><div><strong>Vindicta balanced cloth</strong><p className="hint">For the matching Vindicta R12 replacement. Choosing this reference does not establish compatibility; Analyze project checks the source and build checks compiled output.</p></div><button disabled={disabled||!project.dmx} onClick={()=>void included()}>Use included Vindicta cloth setup</button>{!project.dmx&&<p className="hint">Choose a replacement model first.</p>}</div>}
      <Check label="Use tested hair and cloth physics" value={physics.enabled??false} onChange={enabled=>physicsUpdate({enabled})} disabled={disabled}/>
      <FilePathInput label="Tested compiled model" value={physics.source??''} kind="physics" update={source=>physicsUpdate({source})} disabled={disabled} fail={fail}/>
      <p className="hint">The build verifies matching bones, rest poses, rigid bodies and cloth bindings before installing.</p>
    </Disclosure>
    <Disclosure title="Advanced cloth authoring" hint="Create and inspect a new cloth candidate in a separate test workspace">
      <p className="section-description">Automatic authoring prepares a candidate for ModelDoc testing. It does not enable new in-game cloth by itself.</p>
      <Section title="Cloth preview choices"><SelectField label="Cloth mesh" value={selected} values={[["","Choose a model mesh…"],...[...new Set([...meshes,...(selected?[selected]:[])])].map(name=>[name,meshes.includes(name)?name:`${name} (saved, not checked)`] as const)]} onChange={name=>clothUpdate({meshes:name?[name]:[]})} disabled={disabled}/><div className="field-grid"><SelectField label="Attachment edge" value={cloth.attachment??'top'} values={[["top","Keep the top edge attached"],["bottom","Keep the bottom edge attached"]]} onChange={attachment=>clothUpdate({attachment:attachment as ClothSetupChoices['attachment']})} disabled={disabled}/><SelectField label="Proposed collision" value={cloth.collision??'legs'} values={[["legs","Leg collision"],["none","No collision — preview only"]]} onChange={collision=>clothUpdate({collision:collision as ClothSetupChoices['collision']})} disabled={disabled}/></div></Section>
      {children}
    </Disclosure>
    <Disclosure title="Additional garment" hint={clothing.enabled?'Garment included in saved choices':'Optional · add a separate garment'} open={clothing.enabled&&!clothing.source?true:undefined}>
      <Check label="Include an added garment" value={clothing.enabled??false} onChange={enabled=>clothingUpdate({enabled})} disabled={disabled}/>{clothing.enabled&&<div className="editor-fields"><FilePathInput label="Garment model" kind="garment" value={clothing.source??''} update={source=>clothingUpdate({source})} disabled={disabled} fail={fail}/><SelectField label="Garment movement" value={clothing.mode??'weighted'} values={[["weighted","Follow the character’s bones"],["simulated","Independent cloth simulation"]]} onChange={mode=>clothingUpdate({mode:mode as ClothingChoices['mode']})} disabled={disabled}/><p className="hint">{clothing.mode==='simulated'?'Author and test simulation in ModelDoc. Once the garment is part of the matching replacement DMX, select its tested compiled reference above.':'Fit the garment to the replacement before preparation. Binding, fitting, and build qualification remain separate operations.'}</p></div>}
    </Disclosure>
  </>;
}
const assetSpec:Record<AssetKind,{title:string;singular:string;pathKind:PathKind;hint:string}>={
  icons:{title:'Icons',singular:'icon',pathKind:'icon',hint:'Images replace existing game artwork after format and size checks.'},
  animations:{title:'Animation replacements',singular:'animation',pathKind:'animation',hint:'Compatible VNM clips must match the original skeleton, timing, and events. Blender and FBX conversion is not provided by this editor.'},
  effects:{title:'Effects',singular:'effect',pathKind:'effect',hint:'Compiled particle resources and their dependencies require validation before packaging.'},
  audio:{title:'Audio',singular:'sound',pathKind:'audio',hint:'Audio format, timing, and channels must match the original resource. Shared sounds can affect other characters.'},
};
export function AssetRows({kind,...props}:{kind:AssetKind}&Props){
  const {project,update,disabled,fail,catalog,discover}=props, spec=assetSpec[kind],rows=project.customization[kind]??[];
  const change=(next:ResourceReplacement[])=>update(changeCustomization(project,{[kind]:next}));
  const edit=(index:number,value:Partial<ResourceReplacement>)=>change(rows.map((row,i)=>i===index?{...row,...value}:row));
  if(!rows.length)return <div className="optional-asset-row"><div><h2>{spec.title}</h2><p>{kind==='icons'?'No replacement icon':kind==='audio'?'No custom sounds':kind==='effects'?'No custom effects':'No replacement animations'} <span className="optional-tag">Optional</span></p></div><button disabled={disabled} onClick={()=>change([{enabled:true,source:'',target:''}])}>Add {spec.singular}</button></div>;
  return <Section title={spec.title} actions={<button disabled={disabled||rows.length>=128} onClick={()=>change([...rows,{enabled:true,source:'',target:''}])}>Add {spec.singular}</button>}><p className="hint">{spec.hint}</p>{rows.map((row,index)=><div className="asset-editor" key={index}><div className="section-heading"><Check label={`Include ${spec.singular} ${index+1}`} value={row.enabled??true} onChange={enabled=>edit(index,{enabled})} disabled={disabled}/><button className="tertiary" disabled={disabled} onClick={()=>change(rows.filter((_,i)=>i!==index))}>Remove</button></div><FilePathInput label={`New ${spec.singular} ${index+1}`} kind={spec.pathKind} value={row.source??''} update={source=>edit(index,{source})} disabled={disabled} fail={fail}/><div className="destination"><span className="muted">Game destination</span><code className="path">{row.target||'Not selected'}</code><span className="hint">{row.target?'Saved destination — ownership and compatibility must be verified before building.':'Select a discovered game destination before building. This row can be saved as a draft.'}</span></div>{discover&&<DestinationPicker kind={kind} catalog={catalog?.hero_path===project.original_model?catalog:null} current={row.target??''} disabled={disabled} discover={discover} choose={entry=>{const next={...row,target:entry.target,allow_shared:false};if(entry.original_sha256)next.original_sha256=entry.original_sha256;else delete next.original_sha256;change(rows.map((item,i)=>i===index?next:item));}}/>}{catalog?.[kind].find(entry=>entry.target===row.target)?.shared&&<Check label="Allow this replacement to affect other characters sharing the resource" value={row.allow_shared??false} onChange={allow_shared=>edit(index,{allow_shared})} disabled={disabled}/>} {kind==='icons'&&row.source&&<ImagePreview path={row.source} disabled={disabled} fail={fail}/>} {kind==='icons'&&<SelectField label={`Image fit ${index+1}`} value={row.fit??'contain'} values={[["contain","Contain — retain the full image"],["cover","Cover — crop to fill"],["stretch","Stretch to the target size"]]} onChange={fit=>edit(index,{fit:fit as ResourceReplacement['fit']})} disabled={disabled}/>}<details className="technical"><summary>Advanced destination</summary><label className="field">Exact existing game asset<input value={row.target??''} disabled={disabled} onChange={event=>{const copy={...row,target:event.target.value};delete copy.original_sha256;delete copy.allow_shared;change(rows.map((item,i)=>i===index?copy:item));}}/></label><p className="hint">Only an existing resource belonging to the selected character can pass build validation. Editing this path invalidates its saved fingerprint and shared-resource permission.</p>{(kind==='audio'||kind==='effects')&&<Check label="Allow changes to other characters that share this asset" value={row.allow_shared??false} onChange={allow_shared=>edit(index,{allow_shared})} disabled={disabled}/>}</details></div>)}</Section>;
}
export function MotionPage(props:Props){
  const {project,update,disabled}=props, rows=project.customization.animations??[];
  const active=Boolean(project.customization.enabled&&rows.some(row=>row.enabled!==false));
  const [editing,setEditing]=useState(active);
  function preserve(){setEditing(false);update({...changeCustomization(project,{animations:rows.map(row=>({...row,enabled:false}))},false),animation_policy:'preserve'});}
  return <><PageTitle eyebrow="Animation library" title="Motion" subtitle="Keep the original movement or configure compatible replacement clips."/>
    <Section title="Animations"><fieldset className="mode-options"><legend className="sr-only">Animation choices</legend>
      <label className={`mode-option ${!editing&&!active?'selected':''}`}><input type="radio" name="animation-choice" checked={!editing&&!active} disabled={disabled} onChange={preserve}/><span><strong>Preserve original animations <small>Recommended</small></strong><span>Use the character’s original movement and sequences.</span></span></label>
      <label className={`mode-option ${editing||active?'selected':''}`}><input type="radio" name="animation-choice" checked={editing||active} disabled={disabled} onChange={()=>setEditing(true)}/><span><strong>Replace animation clips</strong><span>Choose existing animations and add compatible replacements.</span></span></label>
    </fieldset>{project.animation_policy!=='preserve'&&<p className="hint">Advanced policy active: legacy animation references are disabled. Review animation graph handling below.</p>}</Section>
    {!editing&&rows.length===0&&<div className="motion-preserved"><span className="studio-symbol" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="m9 5 11 7-11 7zM4 5v14"/></svg></span><div><h2>Original movement, ready to keep</h2><p>No animation replacements are configured. Choose Replace animation clips above when you want to add one.</p></div></div>}
    {(editing||rows.length>0)&&<><AssetRows kind="animations" {...props}/>{!project.customization.enabled&&rows.length>0&&<p className="hint">Saved replacements are inactive while project customization is off. Enable it in Extras to include them.</p>}</>}
    <Disclosure title="Animation graph handling" hint="Advanced policy and compiler settings">
      <SelectField label="Animation policy" value={project.animation_policy} values={[["preserve","Preserve original animations"],["disable_legacy","Disable legacy animation references"]]} onChange={animation_policy=>update({...project,animation_policy})} disabled={disabled}/>
      <SelectField label="Animation graph mode" value={project.ag2_mode} values={[["auto","Automatic"],["preserve","Preserve"],["cswin","Legacy compiler"],["adapter","Configured adapter"]]} onChange={ag2_mode=>update({...project,ag2_mode})} disabled={disabled}/>
    </Disclosure>
  </>;
}
export function ExtrasPage(props:ExtrasProps){return <ExtrasWorkspace {...props}/>;}
