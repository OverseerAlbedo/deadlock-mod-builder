import { lazy, Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { api, basename, errorText, type WeaponModel } from '../api';
import type { JsonValue, Project, VariantChoices } from '../models';
import { changeCustomization } from '../components/customization';
import { weaponLike } from '../components/WeaponModels';
import { SelectField } from '../components/CustomizationPages';
import type { PreparationAnalysis, PreparationOptions, WeaponChoice, WeaponPreview } from './models';
import { adjustments, nudge, previewShapes, withAdjustments, type Nudge } from './gun';
const WeaponViewport=lazy(()=>import('./WeaponViewport'));
type Operate=(label:string,operation:()=>Promise<void>,cancel?:boolean)=>Promise<unknown>;
const asOptions=(options:PreparationOptions)=>JSON.parse(JSON.stringify(options)) as Record<string,JsonValue>;

export interface SeparateGunProps {
  project:Project;analysis:PreparationAnalysis;options:PreparationOptions;busy:boolean;operate:Operate;
  change:(values:Partial<PreparationOptions>)=>void;
  /** Where this project is saved; the gun project is saved next to it. */
  projectPath?:string|null;
  /** Saves this project first (when it was never saved); resolves to its path. */
  saveProject?:()=>Promise<string|undefined>;
  /** Replaces the whole project (to link the gun project into the mod). */
  updateProject:(project:Project)=>void;
}

/** The gun project file beside this project: Name.dmb.json -> Name.gun-<model>.dmb.json. */
export function gunProjectPath(parent:string,model:string){
  const slash=Math.max(parent.lastIndexOf('/'),parent.lastIndexOf('\\'));
  const folder=parent.slice(0,slash+1),stem=parent.slice(slash+1).replace(/\.dmb\.json$/i,'').replace(/\.json$/i,'');
  const gun=basename(model).replace(/\.vmdl(_c)?$/i,'').replace(/[^A-Za-z0-9_-]+/g,'_');
  return `${folder}${stem}.gun-${gun}.dmb.json`;
}

/** The opened gun project; applied: its gun choice when last converted. */
type Session={path:string;project:Project;analysis:PreparationAnalysis;applied:string};

/** Heroes whose gun is a separate model: "use my gun" makes that model from your gun in a linked gun project. */
export function SeparateGun({project,options,busy,operate,projectPath,saveProject,updateProject}:SeparateGunProps){
  const hero=project.hero_name||basename(project.original_model).replace(/\.vmdl(_c)?$/,'');
  const variants=project.customization.equipment?.variants??[];
  const linked=variants.find(v=>weaponLike(v.model??'')&&v.project);
  const [models,setModels]=useState<WeaponModel[]|null>(null),[model,setModel]=useState(linked?.model??'');
  const [session,setSession]=useState<Session|null>(null);
  const [preview,setPreview]=useState<{key:string;value?:WeaponPreview;error?:string}|null>(null);
  const asked=useRef(false);
  const gunFile=options.weapon_source??'';
  async function discover(){await operate(`Could not find ${hero}'s gun model`,async()=>{const list=await api.discoverWeaponModels(project);setModels(list);if(list.length&&!model)setModel(list[0].path.replace(/_c$/i,''));});}
  useEffect(()=>{if(!asked.current&&!busy&&!linked&&models===null){asked.current=true;void discover();}},[busy]);// eslint-disable-line react-hooks/exhaustive-deps
  /** Prepare (or prepare again) the gun project and link it; your gun is left off the character's body. */
  async function prepare(adjusted?:WeaponChoice){
    if(!model)return;
    await operate(`${hero}'s gun model could not be made from your gun`,async()=>{
      let parentPath=projectPath??undefined;
      if(!parentPath)parentPath=await saveProject?.();
      if(!parentPath)throw new Error('Save your project first, so the gun project can be saved next to it.');
      const path=linked?.model===model&&linked.project?linked.project:gunProjectPath(parentPath,model);
      const existing=linked?.model===model&&linked.project?(await api.openProject(linked.project).catch(()=>null))?.project:null;
      let child=existing??await api.selectHero({...(await api.newProject()).project,name:`${project.name||'character'}_gun`.slice(0,41)},model);
      const source=gunFile||project.preparation_source;
      child={...child,model_input_mode:'weapon',preparation_source:source,preparation_options:gunFile?{}:asOptions({scene:options.scene}),vpk_number:project.vpk_number,install:false,compile_model:true,build_vpk:true};
      const childAnalysis=await api.preparationInspect(child,source,[0,0,0]);
      const candidates=childAnalysis.analysis.weapons??[];
      // The gun the character uses (its own gun) or the gun file's (right-hand) gun.
      const own=options.weapon?.objects;
      const suggested=childAnalysis.weapon_suggestion??undefined;
      const objects=!gunFile&&own?.length&&candidates.some(c=>c.objects.length===own.length&&c.objects.every(n=>own.includes(n)))?own:suggested?.objects;
      if(!objects?.length)throw new Error('No gun was found to make the gun model from. Check that your gun is a visible mesh.');
      const previous=(existing?.preparation_options as PreparationOptions|undefined)?.weapon;
      const base:WeaponChoice={...(suggested??{}),...(adjusted??previous??{}),objects,enabled:true,action:'replace'};
      const childOptions:PreparationOptions={...(child.preparation_options as PreparationOptions),weapon:base,reviewed:true};
      child={...child,preparation_options:asOptions(childOptions)};
      const issues=await api.preparationReview(child,childAnalysis,childOptions);
      if(issues.length)throw new Error(issues.map(i=>i.message).join('\n'));
      const result=await api.preparationConvert(child,childAnalysis,childOptions,false);
      const accepted=await api.preparationAccept(child,result);
      const saved=await api.saveProject(path,accepted);
      setSession({path:saved.path??path,project:saved.project,analysis:childAnalysis,applied:JSON.stringify((saved.project.preparation_options as PreparationOptions).weapon)});
      // Link it into this mod, and leave your gun off the character's body.
      const equipment=project.customization.equipment??{};
      const others=(equipment.variants??[]).filter(v=>v.model!==model);
      const variant:VariantChoices={enabled:true,model,project:saved.path??path};
      const weapon=options.weapon?.objects?.length?{...options.weapon,enabled:true,action:'remove' as const}:options.weapon;
      updateProject(changeCustomization({...project,preparation_options:asOptions({...options,weapon,reviewed:false})},{equipment:{...equipment,enabled:true,variants:[...others,variant]}}));
    },true);
  }
  async function resume(){
    if(!linked?.project)return;
    await operate('The gun project could not be opened',async()=>{
      const document=await api.openProject(linked.project!);
      const resumed=await api.preparationResume(document.project);
      setSession({path:linked.project!,project:document.project,analysis:resumed.analysis,applied:JSON.stringify((document.project.preparation_options as PreparationOptions).weapon)});
    },true);
  }
  function unlink(){
    const equipment=project.customization.equipment??{};
    updateProject(changeCustomization(project,{equipment:{...equipment,variants:(equipment.variants??[]).filter(v=>v!==linked)}}));
    setSession(null);
  }
  // Fit preview of the gun project, live while adjusting (applied with "Apply changes").
  const choice=(session?.project.preparation_options as PreparationOptions|undefined)?.weapon;
  const key=JSON.stringify([choice,session?.path]);
  useEffect(()=>{
    if(!session||!choice)return;
    let current=true;
    const timer=window.setTimeout(()=>{api.preparationWeaponPreview(session.project,session.analysis,session.project.preparation_options as PreparationOptions).then(value=>{if(current)setPreview({key,value});}).catch(error=>{if(current)setPreview(last=>({key,error:errorText(error),value:last?.value}));});},300);
    return ()=>{current=false;window.clearTimeout(timer);};
  },[key]);// eslint-disable-line react-hooks/exhaustive-deps
  const shown=preview?.key===key?preview:null;
  // The last result stays on screen while the next one is computed.
  const shownValue=session&&choice?preview?.value:undefined;
  const shapes=useMemo(()=>session&&choice?previewShapes(session.analysis,choice,'weapon_model'):undefined,[session?.analysis,JSON.stringify(choice?.objects)]);// eslint-disable-line react-hooks/exhaustive-deps
  const adjust=(change:Nudge)=>{if(!session||!choice)return;const next=withAdjustments(choice,'right',nudge(adjustments(choice,'right'),change));setSession({...session,project:{...session.project,preparation_options:asOptions({...(session.project.preparation_options as PreparationOptions),weapon:next})}});};
  const pending=Boolean(session&&choice&&session.applied!==JSON.stringify(choice));
  const button=(label:string,change:Nudge)=><button type="button" className="prep-secondary" disabled={busy} onClick={()=>adjust(change)}>{label}</button>;
  return <div className="prep-separate-gun">
    {linked?<div className="prep-gun-linked" role="status">
      <p><strong>Your gun replaces {hero}'s gun model ({basename(linked.model??'')}).</strong> It is made in a linked gun project ({basename(linked.project??'')}) and built into this mod together with the character. Your gun is left off the character's body.</p>
      <div className="row"><button className="prep-secondary" disabled={busy} onClick={()=>void (session?prepare(choice):resume())}>{session?'Make it again':'Adjust the fit'}</button><button className="prep-quiet" disabled={busy} onClick={unlink}>Stop replacing {hero}'s gun</button></div>
    </div>:<>
      {models&&models.length>1&&<SelectField label={`${hero}'s gun model`} value={model} values={models.map(m=>[m.path.replace(/_c$/i,''),`${m.label} — ${m.path}`] as const)} disabled={busy} onChange={setModel}/>}
      {models&&!models.length&&<p className="hint">No separate gun model was found for {hero} in the game files, so the gun cannot be replaced. You can leave your gun out or keep it in the hand in Advanced.</p>}
      <div className="row"><button className="prep-primary" disabled={busy||!model} onClick={()=>void prepare()}>Use my gun instead of {hero}'s <em className="prep-recommended">recommended</em></button>{models===null&&<button className="prep-secondary" disabled={busy} onClick={()=>void discover()}>Find {hero}'s gun model</button>}</div>
      <p className="hint">The app makes {hero}'s gun model from {gunFile?`the gun in ${basename(gunFile)}`:'the gun in your character file'}, fits it to the original gun's grip, barrel and size, and builds it into the same mod.{projectPath?'':' Your project is saved first, so the gun project can be saved next to it.'}</p>
    </>}
    {session&&choice&&<div className="prep-weapon-fit">
      {shown?.error?<p className="hint" role="alert">The fit could not be previewed: {shown.error}</p>:shownValue?<Suspense fallback={<p>Loading the 3D preview…</p>}><WeaponViewport preview={shownValue} shapes={shapes}/></Suspense>:<div className="prep-weapon-viewport prep-weapon-loading"><p>Fitting your gun…</p></div>}
      <div className="prep-gun-adjust" role="group" aria-label="Adjust the gun model">
        <div className="prep-gun-adjust-row">{button('Flip direction',{kind:'flip'})}{button('Turn ⟲ 90°',{kind:'turn',degrees:-90})}{button('Turn ⟳ 90°',{kind:'turn',degrees:90})}{button('Smaller',{kind:'size',factor:1/1.1})}{button('Bigger',{kind:'size',factor:1.1})}{button('Reset',{kind:'reset'})}</div>
      </div>
      <div className="row"><button className="prep-primary" disabled={busy||!pending} onClick={()=>void prepare(choice)}>Apply changes to {hero}'s gun model</button>{pending&&<span className="hint">Your adjustments are shown in the preview; Apply makes the gun model again with them.</span>}</div>
    </div>}
  </div>;
}
