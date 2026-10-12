import { lazy, Suspense, useState } from 'react';
import { api } from '../api';
import type { Project } from '../models';
import { Check, SelectField } from '../components/CustomizationPages';
import { NumberField } from '../components/MaterialEditor';
import { Section } from '../components/Primitives';
import { FilePathInput } from '../components/FilePathInput';
import type { PreparationAnalysis, PreparationOptions, Vec3, WeaponCandidate, WeaponChoice, WeaponPreview } from './models';
const WeaponViewport=lazy(()=>import('./WeaponViewport'));
type Operate=(label:string,operation:()=>Promise<void>,cancel?:boolean)=>Promise<unknown>;

export const candidateLabel=(c:WeaponCandidate)=>`${c.objects.join(' + ')} · ${c.vertices.toLocaleString()} vertices${c.gun_file?' · from your gun file':''}${c.attach_bone?` · held by ${c.attach_bone}`:''}`;
export const sameObjects=(a:string[]|undefined,b:string[])=>Boolean(a)&&a!.length===b.length&&[...a!].sort().every((n,i)=>n===[...b].sort()[i]);
/** Windows paths compared as the native side resolves them (no \\?\ prefix, any case or slash). */
export const samePath=(a:string,b:string)=>{const plain=(p:string)=>p.trim().replace(/^\\\\\?\\/,'').replace(/\//g,'\\').toLowerCase();return plain(a)===plain(b);};

/** Default gun choice for a fresh inspection, keeping a still-valid earlier choice. */
export function weaponDefaults(analysis:PreparationAnalysis,current:WeaponChoice|undefined):WeaponChoice|undefined{
  const candidates=analysis.analysis.weapons??[];
  if(current?.objects?.length&&candidates.some(c=>sameObjects(current.objects,c.objects)))return current;
  return analysis.weapon_suggestion??undefined;
}

export function WeaponPanel({project,analysis,options,change,busy,operate,reinspect}:{project:Project;analysis:PreparationAnalysis;options:PreparationOptions;change:(values:Partial<PreparationOptions>)=>void;busy:boolean;operate:Operate;reinspect?:()=>void}){
  const [preview,setPreview]=useState<{key:string;value:WeaponPreview}|null>(null);
  const candidates=analysis.analysis.weapons??[],reference=analysis.weapon_reference,choice=options.weapon??{};
  const gunFile=options.weapon_source??'',inspectedFile=analysis.weapon_file?.selection.source??'',added=analysis.analysis.weapon_file;
  const gunFilePending=gunFile||inspectedFile?!samePath(gunFile,inspectedFile):false;
  const ownGuns=candidates.filter(c=>!c.gun_file);
  const fail=(title:string,error:unknown)=>{void operate(title,async()=>{throw error;});};
  const weaponModel=project.model_input_mode==='weapon';
  const heroGun=Boolean(reference&&reference.meshes.length>0),separate=Boolean(reference&&!reference.separate_model&&reference.meshes.length===0);
  const selected=candidates.find(c=>sameObjects(choice.objects,c.objects));
  const action=weaponModel?'replace':(choice.enabled?choice.action||'keep':'keep');
  const fitting=Boolean(selected)&&action==='replace';
  const key=JSON.stringify([choice,analysis.source_sha256]);
  const set=(values:Partial<WeaponChoice>)=>change({weapon:{action:weaponModel?'replace':action,objects:choice.objects??[],...choice,enabled:true,...values},reviewed:false});
  const offset=(choice.offset??[0,0,0]) as Vec3;
  const bones=[...new Set([...(reference?.bones??[]),...analysis.target.map(b=>b.name).filter(n=>/weapon|gun|muzzle|prop_/i.test(n))])].sort();
  async function load(){await operate('The gun fit could not be previewed',async()=>{const value=await api.preparationWeaponPreview(project,analysis,options);setPreview({key,value});});}
  const shown=preview&&preview.key===key?preview.value:null;
  return <Section title={weaponModel?'Weapon model':'Gun'} className="prep-card prep-weapon-card">
    <p className="prep-section-intro">{weaponModel
      ?<>This project replaces the separate weapon model <span className="mono">{project.original_model}</span>. Only the gun from your file is exported; it is fitted to the original gun's grip, barrel and size so the muzzle and hand positions line up.</>
      :heroGun?<>This hero's gun is part of the hero model ({reference!.meshes.join(', ')}). Your gun can replace it: it is fitted to the original gun's grip, barrel and size, and follows the same weapon bones.</>
      :separate?<>This hero's gun is a separate model. To replace it, create a weapon project in <strong>Appearance → Equipment → Weapon model</strong>. Here you can leave your gun out of the character or let it follow the hand.</>
      :<>The hero's original gun could not be read from the game files, so your gun can only be left out or kept in the hand.</>}</p>
    {!weaponModel&&<div className="prep-gun-file">
      <FilePathInput label="Gun file" kind="weapon" value={gunFile} disabled={busy} fail={fail} update={path=>change({weapon_source:path||undefined,weapon:undefined,reviewed:false})}/>
      <p className="hint">Optional: use the gun from another Blender, FBX or glTF file. Inspection adds it to your character, puts its grip in the right hand and offers it as the gun below. That file is never changed.</p>
      {gunFilePending?<div className="prep-gun-file-status" role="status"><p>{gunFile?'Inspect the source again to load the gun from this file.':'Inspect the source again to remove the gun file\'s gun.'}</p>{reinspect&&<button disabled={busy} onClick={reinspect}>Inspect again with this gun file</button>}</div>
        :added&&<p className="hint">From {added.file}: {added.objects.join(', ')}, {added.placed?`moved into the right hand (${added.bone})`:added.bone?`held by ${added.bone}`:'kept where it is in the file'}.</p>}
      {Boolean(gunFile)&&!gunFilePending&&ownGuns.length>0&&<p className="hint">Your character file also has a gun ({ownGuns.map(c=>c.objects.join(' + ')).join(', ')}). It stays on the character unless you set it aside in Scene objects.</p>}
    </div>}
    {!candidates.length?<p className="hint">{weaponModel?'No gun was found in the source. Check Scene objects: the gun must be a visible mesh. To use a gun from another file, choose that file as the source path and inspect again.':'No gun was found in the character file. Choose a gun file above to use a gun from another file.'}</p>:<>
      <SelectField label="Gun in your file" value={selected?selected.id:''} values={[["","No gun in this file"],...candidates.map(c=>[c.id,candidateLabel(c)] as const)]} disabled={busy} onChange={id=>{const c=candidates.find(x=>x.id===id);set({objects:c?c.objects:[],enabled:Boolean(c)});}}/>
      {selected&&!weaponModel&&<fieldset className="prep-weapon-actions" disabled={busy}><legend>What should happen to it?</legend>
        <label className="checkbox checkbox-line"><input type="radio" name="weapon-action" checked={action==='replace'} disabled={!heroGun} onChange={()=>set({action:'replace'})}/>Replace the hero's gun with it (fitted to the original){!heroGun&&<small className="hint"> — not available: {separate?'this hero\'s gun is a separate model':'the original gun was not found'}</small>}</label>
        <label className="checkbox checkbox-line"><input type="radio" name="weapon-action" checked={action==='remove'} onChange={()=>set({action:'remove'})}/>Leave it out of the character</label>
        <label className="checkbox checkbox-line"><input type="radio" name="weapon-action" checked={action==='keep'} onChange={()=>set({action:'keep'})}/>Keep it in the hand (it follows the hand like any other accessory)</label>
      </fieldset>}
    </>}
    {fitting&&<div className="prep-weapon-fit">
      <SelectField label="Gun size" value={choice.fit??'match'} values={[["match","Match the original gun's size (recommended)"],["keep_size","Keep my gun's size relative to my character"]]} disabled={busy} onChange={fit=>set({fit:fit as WeaponChoice['fit']})}/>
      <SelectField label="Gun skinning" value={choice.bone?'bone':choice.skin??'auto'} values={[["auto","Copy the original gun's bone weights (moving parts follow)"],["rigid","Rigid on the main weapon bone"],["bone","Rigid on a bone I choose"]]} disabled={busy} onChange={value=>set(value==='bone'?{skin:'rigid',bone:bones[0]??''}:{skin:value as WeaponChoice['skin'],bone:''})}/>
      {choice.bone!==undefined&&choice.bone!==''&&<SelectField label="Weapon bone" value={choice.bone} values={bones.map(b=>[b,b] as const)} disabled={busy} onChange={bone=>set({bone})}/>}
      <details className="prep-option-help"><summary>Adjust the fit</summary>
        <p className="hint">Automatic fitting finds the barrel, muzzle and grip on both guns. If the gun points backwards or is upside down, flip or roll it here and preview again.</p>
        <Check label="Flip barrel direction" value={choice.flip??false} disabled={busy} onChange={flip=>set({flip})}/>
        <div className="override-grid">
          <NumberField label="Roll around barrel (degrees)" value={choice.roll??0} min={-180} max={180} step={15} disabled={busy} update={roll=>set({roll})}/>
          <NumberField label="Scale multiplier" value={choice.scale??1} min={.1} max={10} step={.05} disabled={busy} update={scale=>set({scale})}/>
        </div>
        <div className="vector-fields">{(['Forward','Up','Right'] as const).map((axis,index)=><NumberField key={axis} label={`Move ${axis.toLowerCase()} (inches)`} value={offset[index]} min={-50} max={50} step={.25} disabled={busy} update={value=>{const next=[...offset] as Vec3;next[index]=value;set({offset:next});}}/>)}</div>
        <button className="tertiary" disabled={busy} onClick={()=>set({flip:false,roll:0,scale:1,offset:[0,0,0]})}>Reset adjustments</button>
      </details>
      <div className="row"><button disabled={busy} onClick={()=>void load()}>{shown?'Refresh gun preview':'Preview gun fit'}</button></div>
      {shown&&<>
        <Suspense fallback={<p>Loading gun preview…</p>}><WeaponViewport preview={shown}/></Suspense>
        <dl className="summary-list prep-weapon-stats"><dt>Scale</dt><dd>{shown.fit.scale.toFixed(3)}×</dd><dt>Muzzle distance</dt><dd>{shown.fit.muzzle_offset_mm.toFixed(0)} mm</dd><dt>Skinning</dt><dd>{shown.fit.skin==='transfer'?`copied from ${shown.fit.skin_bones.join(', ')}`:`rigid on ${shown.fit.skin_bones[0]}`}</dd><dt>Fit confidence</dt><dd>yours: {shown.fit.source.confidence} · original: {shown.fit.target.confidence}</dd></dl>
        {[...shown.fit.notes,...shown.fit.source.notes,...shown.fit.target.notes].length>0&&<ul className="hint">{[...shown.fit.notes,...shown.fit.source.notes.map(n=>`Your gun: ${n}`),...shown.fit.target.notes.map(n=>`Original: ${n}`)].map((n,i)=><li key={i}>{n}</li>)}</ul>}
      </>}
    </div>}
  </Section>;
}
