import { lazy, Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { api, basename, errorText } from '../api';
import { changeCustomization } from '../components/customization';
import { weaponLike } from '../components/WeaponModels';
import type { Project } from '../models';
import { Check, SelectField } from '../components/CustomizationPages';
import { NumberField } from '../components/MaterialEditor';
import { Section } from '../components/Primitives';
import { FilePathInput } from '../components/FilePathInput';
import type { Hand, PreparationAnalysis, PreparationOptions, Vec3, WeaponCandidate, WeaponChoice, WeaponFit, WeaponPreview } from './models';
import { adjusted, adjustments, fittedHands, gunName, handGun, heroGun, leftMode, nudge, previewShapes, replaceable, sameObjects as same, withAdjustments, yourGuns, type HeroGun, type Nudge } from './gun';
import { SeparateGun, type SeparateGunProps } from './SeparateGun';
const WeaponViewport=lazy(()=>import('./WeaponViewport'));
type Operate=(label:string,operation:()=>Promise<void>,cancel?:boolean)=>Promise<unknown>;

export const candidateLabel=(c:WeaponCandidate,gunFile?:string)=>gunName(c,gunFile);
export const sameObjects=(a:string[]|undefined,b:string[])=>same(a,b);
/** Windows paths compared as the native side resolves them (no \\?\ prefix, any case or slash). */
export const samePath=(a:string,b:string)=>{const plain=(p:string)=>p.trim().replace(/^\\\\\?\\/,'').replace(/\//g,'\\').toLowerCase();return plain(a)===plain(b);};

/** Default gun choice for a fresh inspection, keeping a still-valid earlier choice. */
export function weaponDefaults(analysis:PreparationAnalysis,current:WeaponChoice|undefined):WeaponChoice|undefined{
  const candidates=analysis.analysis.weapons??[];
  const valid=(objects?:string[])=>!objects?.length||candidates.some(c=>same(c.objects,objects));
  if(current?.objects?.length&&valid(current.objects)&&valid(current.left?.objects))return current;
  return analysis.weapon_suggestion??undefined;
}

const heroLabel=(project:Project)=>project.hero_name||(project.original_model.split('/').pop()??'the hero').replace(/\.vmdl(_c)?$/,'')||'the hero';

/** Plain-language explanation of the hero's gun, your gun(s) and what will happen. */
function GunSummary({kind,hero,project,analysis,choice,gunFile}:{kind:HeroGun;hero:string;project:Project;analysis:PreparationAnalysis;choice:WeaponChoice;gunFile:string}){
  const {file,own}=yourGuns(analysis),reference=analysis.weapon_reference;
  const heroLine={
    dual:`${hero} holds two guns, one in each hand. They are part of ${hero}'s model, so your guns can replace them directly.`,
    single:`${hero}'s gun is part of ${hero}'s model, so your gun can replace it directly.`,
    separate:`${hero}'s gun is a separate model. The app can make it from your gun and build it into the same mod.`,
    unreadable:`${hero}'s gun could not be read from the game files (${reference?.unreadable?.join(', ')}), so it cannot be replaced. You can leave your gun out or keep it in the hand.`,
    weapon_model:`This project replaces the separate weapon model ${project.original_model}. Only your gun is used; it is fitted to the original gun's grip, barrel and size.`,
    unknown:`${hero}'s original gun has not been read yet. Inspect the source again to replace it.`,
  }[kind];
  const received=analysis.analysis.weapon_file,fileName=received?.file??gunFile.split(/[\\/]/).pop();
  const placed=received?.guns?.length?received.guns.filter(g=>g.bone).map(g=>`${g.side==='left'?'the left gun':'the right gun'} ${g.placed?'was put in':'is held by'} ${g.bone}`).join(', ')
    :received?.bone?`it ${received.placed?'was put in the right hand':'is held by'} (${received.bone})`:'';
  const yours=file.length>1?`Your gun file ${fileName} has two guns, one for each hand${placed?`: ${placed}`:''}.`
    :file.length===1?`Your gun file ${fileName} has one gun${placed?`; ${placed}`:''}.`
    :own.length?`Your character file has ${own.length===1?'a gun':`${own.length} possible guns`}: ${own.map(c=>gunName(c)).join('; ')}.`
    :'No gun was found in your files yet.';
  const alsoOwn=file.length>0&&own.length>0?`Your character file also has a gun (${own.map(c=>c.objects.join(' + ')).join(', ')}). It stays on the character unless you set it aside in Scene objects.`:'';
  const action=choice.enabled?choice.action:undefined;
  const mode=leftMode(choice);
  const result=action==='replace'&&kind==='dual'?(mode==='own'?`Your right gun replaces ${hero}'s right gun, and your left gun replaces the left one.`:mode==='copy'?`Your gun replaces both of ${hero}'s guns: a copy goes in each hand.`:`Your gun replaces ${hero}'s right gun; the left hand keeps ${hero}'s own gun.`)
    :action==='replace'&&kind==='single'?`Your gun replaces ${hero}'s gun. It is fitted to the original gun's grip, barrel and size, so recoil and reload animations still work.`
    :action==='remove'?`${hero} keeps ${kind==='dual'?'both of their own guns':'their own gun'}; your gun is left out of the character.`
    :action==='keep'?`Your gun stays in the hand as an accessory, and ${hero}'s gun stays too.`:'';
  const charm=analysis.analysis.weapon_file?.charm_bones?.length;
  return <div className="prep-gun-summary" role="status">
    <p><strong>{heroLine}</strong></p>
    {kind!=='weapon_model'&&<p>{yours}</p>}
    {alsoOwn&&<p className="hint">{alsoOwn}</p>}
    {result&&<p className="prep-gun-result">{result}</p>}
    {Boolean(charm)&&action==='replace'&&<p className="hint">The charm on your gun keeps its own bones, so it will swing. Turn it on or off under Physics → Gun charm motion.</p>}
  </div>;
}

/** Beginner controls for one hand: flip, turn, size and small moves. */
function AdjustControls({choice,hand,set,busy}:{choice:WeaponChoice;hand:Hand;set:(next:WeaponChoice)=>void;busy:boolean}){
  const apply=(change:Nudge)=>set(withAdjustments(choice,hand,nudge(adjustments(choice,hand),change)));
  const button=(label:string,change:Nudge,title?:string)=><button type="button" className="prep-secondary" disabled={busy} title={title} onClick={()=>apply(change)}>{label}</button>;
  return <div className="prep-gun-adjust" role="group" aria-label={`Adjust the ${hand}-hand gun`}>
    <div className="prep-gun-adjust-row">{button('Flip direction',{kind:'flip'},'Point the barrel the other way')}{button('Turn ⟲ 90°',{kind:'turn',degrees:-90},'Roll around the barrel')}{button('Turn ⟳ 90°',{kind:'turn',degrees:90},'Roll around the barrel')}{button('Smaller',{kind:'size',factor:1/1.1})}{button('Bigger',{kind:'size',factor:1.1})}</div>
    <div className="prep-gun-adjust-row">{button('◀ Back',{kind:'move',axis:0,inches:-.25})}{button('Forward ▶',{kind:'move',axis:0,inches:.25})}{button('▲ Up',{kind:'move',axis:1,inches:.25})}{button('▼ Down',{kind:'move',axis:1,inches:-.25})}{button('Left',{kind:'move',axis:2,inches:-.25})}{button('Right',{kind:'move',axis:2,inches:.25})}{button('Reset',{kind:'reset'},'Undo all adjustments of this hand')}</div>
  </div>;
}

/** Exact numbers for one hand (Advanced). */
function ExactNumbers({choice,hand,set,busy}:{choice:WeaponChoice;hand:Hand;set:(next:WeaponChoice)=>void;busy:boolean}){
  const current=adjustments(choice,hand),put=(values:Partial<typeof current>)=>set(withAdjustments(choice,hand,{...current,...values}));
  const prefix=hand==='left'?'Left gun: ':'';
  return <div className="prep-gun-exact">
    <Check label={`${prefix}Flip barrel direction`} value={current.flip} disabled={busy} onChange={flip=>put({flip})}/>
    <div className="override-grid">
      <NumberField label={`${prefix}Roll around barrel (degrees)`} value={current.roll} min={-180} max={180} step={15} disabled={busy} update={roll=>put({roll})}/>
      <NumberField label={`${prefix}Scale multiplier`} value={current.scale} min={.1} max={10} step={.05} disabled={busy} update={scale=>put({scale})}/>
    </div>
    <div className="vector-fields">{(['Forward','Up','Right'] as const).map((axis,index)=><NumberField key={axis} label={`${prefix}Move ${axis.toLowerCase()} (inches)`} value={current.offset[index]} min={-50} max={50} step={.25} disabled={busy} update={value=>{const offset=[...current.offset] as Vec3;offset[index]=value;put({offset});}}/>)}</div>
  </div>;
}

/** The fit in words: size, where the muzzle ends and how the gun's front was found. */
function FitLine({fit,choice,hand,hero}:{fit:WeaponFit;choice:WeaponChoice;hand:Hand;hero:string}){
  const factor=adjustments(choice,hand).scale;
  const size=(choice.fit==='keep_size'?'as in your file next to your character':`the length of ${hero}'s gun`)+(Math.abs(factor-1)>.005?` × ${Math.round(factor*100)}%`:'');
  const muzzle=fit.muzzle_offset_mm<=50?'the muzzle lines up with the original':`the muzzle ends ${fit.muzzle_offset_mm.toFixed(0)} mm from the original (shots still come from the original muzzle)`;
  const front:Record<string,string>={names:'its front was found from the part names',grip:'its front is the end away from the grip',shape:'its front was guessed from the shape: check the arrows',flipped:'direction flipped by you',flipped_names:'direction flipped by you, but the part names already gave the front: press Flip direction again unless the preview shows the gun backwards'};
  const direction=front[fit.source.direction??'']??(fit.source.confidence==='low'||fit.target.confidence==='low'?'check the direction in the preview':'');
  return <p className="hint prep-gun-fit-line">Size: {size} · {muzzle}{direction?` · ${direction}`:''}</p>;
}

export function WeaponPanel({project,analysis,options,change,busy,operate,reinspect,separate}:{project:Project;analysis:PreparationAnalysis;options:PreparationOptions;change:(values:Partial<PreparationOptions>)=>void;busy:boolean;operate:Operate;reinspect?:()=>void;separate?:Omit<SeparateGunProps,'project'|'analysis'|'options'|'busy'|'operate'|'change'>}){
  const weaponModel=project.model_input_mode==='weapon';
  const kind=heroGun(analysis.weapon_reference,weaponModel),hero=heroLabel(project);
  const {all:candidates,file:fileGuns}=yourGuns(analysis);
  const choice:WeaponChoice=options.weapon??{};
  const gunFile=options.weapon_source??'',inspectedFile=analysis.weapon_file?.selection.source??'';
  const gunFilePending=gunFile||inspectedFile?!samePath(gunFile,inspectedFile):false;
  const action=weaponModel?'replace':(choice.enabled?choice.action||'keep':'keep');
  const set=(next:WeaponChoice)=>change({weapon:{...next,enabled:true,...(weaponModel?{action:'replace' as const}:{})},reviewed:false});
  const right=handGun(choice,candidates,'right');
  const hands=fittedHands(kind,choice);
  const fitting=Boolean(right)&&action==='replace'&&replaceable(kind);
  const [hand,setHand]=useState<Hand>('right');
  const shownHand:Hand=hands.includes(hand)?hand:'right';
  const fail=(title:string,error:unknown)=>{void operate(title,async()=>{throw error;});};
  // Live preview: recomputed shortly after every change (no Blender run, no busy state). The
  // last result stays on screen meanwhile, so the 3D view keeps its camera while you adjust.
  const [preview,setPreview]=useState<{key:string;value?:WeaponPreview;choice?:WeaponChoice;error?:string}|null>(null);
  const key=JSON.stringify([choice,analysis.source_sha256,options.source_roles]);
  const latest=useRef(key);latest.current=key;
  useEffect(()=>{
    if(!fitting)return;
    const timer=window.setTimeout(()=>{
      api.preparationWeaponPreview(project,analysis,options)
        .then(value=>{if(latest.current===key)setPreview({key,value,choice});})
        .catch(error=>{if(latest.current===key)setPreview(last=>({key,error:errorText(error),value:last?.value,choice:last?.choice}));});
    },300);
    return ()=>window.clearTimeout(timer);
  },[key,fitting]);// eslint-disable-line react-hooks/exhaustive-deps
  const shown=preview?.key===key?preview:null;
  const bones=[...new Set([...(analysis.weapon_reference?.bones??[]),...analysis.target.map(b=>b.name).filter(n=>/weapon|gun|muzzle|prop_/i.test(n))])].sort();
  const options_=(list:WeaponCandidate[])=>list.map(c=>[c.id,gunName(c,gunFile)] as const);
  const pick=(id:string)=>candidates.find(c=>c.id===id)?.objects??[];
  const left=leftMode(choice);
  const swap=()=>{const r=adjustments(choice,'right'),l=adjustments(choice,'left');set({...choice,...l,objects:choice.left?.objects??[],left:{...choice.left,...r,objects:choice.objects??[]}});};
  const shownValue=preview?.value,shownChoice=preview?.choice??choice;
  const fit=shownValue?(shownHand==='left'?shownValue.hands?.find(h=>h.side==='left')?.fit:shownValue.fit):null;
  // The shapes of the guns the shown preview was computed for.
  const shapesKey=JSON.stringify([shownChoice.objects,shownChoice.left?.objects,shownChoice.left?.keep]);
  const shapes=useMemo(()=>previewShapes(analysis,shownChoice,kind),[analysis,kind,shapesKey]);// eslint-disable-line react-hooks/exhaustive-deps
  // Separate weapon models this project builds (only wanted for heroes whose gun is its own model).
  const stale=(project.customization.equipment?.variants??[]).filter(v=>v.enabled!==false&&weaponLike(v.model??''));
  return <Section title={weaponModel?'Weapon model':'Gun'} className="prep-card prep-weapon-card">
    <GunSummary kind={kind} hero={hero} project={project} analysis={analysis} choice={choice} gunFile={inspectedFile||gunFile}/>
    {(kind==='single'||kind==='dual')&&stale.length>0&&<div className="prep-gun-file-status" role="alert"><p>This project also builds a separate weapon model ({stale.map(v=>basename(v.model??'')).join(', ')}). {hero}'s gun is part of {hero}'s model, so that weapon project is not needed and would replace something else in the game.</p>{separate&&<button className="prep-primary" disabled={busy} onClick={()=>{const equipment=project.customization.equipment??{};separate.updateProject(changeCustomization(project,{equipment:{...equipment,variants:(equipment.variants??[]).filter(v=>!stale.includes(v))}}));}}>Stop building it</button>}</div>}
    {!weaponModel&&<div className="prep-gun-file">
      <FilePathInput label="Gun from another file (optional)" kind="weapon" value={gunFile} disabled={busy} fail={fail} update={path=>change({weapon_source:path||undefined,weapon:undefined,reviewed:false})}/>
      <p className="hint">Use a gun saved in another Blender, FBX or glTF file. A file with two guns gives one for each hand. It is put in your character's hand automatically; the file itself is never changed.</p>
      {gunFilePending&&<div className="prep-gun-file-status" role="status"><p>{gunFile?'Loading the gun from this file needs a new inspection.':'Removing the gun file\'s gun needs a new inspection.'}</p>{reinspect&&<button className="prep-primary" disabled={busy} onClick={reinspect}>{gunFile?'Load the gun from this file':'Inspect again without it'}</button>}</div>}
    </div>}
    {kind==='separate'&&!weaponModel&&separate&&(candidates.length>0||project.customization.equipment?.variants?.some(v=>v.project))?<SeparateGun project={project} analysis={analysis} options={options} busy={busy} operate={operate} change={change} {...separate}/>
    :!candidates.length?<p className="hint">{weaponModel?'No gun was found in the source. Check Scene objects: the gun must be a visible mesh. To use a gun from another file, choose that file as the source path and inspect again.':'Choose a gun file above to use a gun from another file.'}</p>
    :<>
      {!weaponModel&&<fieldset className="prep-weapon-actions" disabled={busy}><legend>What should your character hold?</legend>
        <label className="checkbox checkbox-line"><input type="radio" name="weapon-action" checked={action==='replace'} disabled={!replaceable(kind)} onChange={()=>set({...choice,objects:choice.objects?.length?choice.objects:candidates[0].objects,action:'replace'})}/>
          <span>Use my gun{kind==='dual'?'s':''} instead of {hero}'s <em className="prep-recommended">recommended</em>{!replaceable(kind)&&<small className="hint"> Not available: {kind==='unreadable'?`${hero}'s gun could not be read`:kind==='separate'?`${hero}'s gun is a separate model`:'the original gun was not found'}.</small>}</span></label>
        <label className="checkbox checkbox-line"><input type="radio" name="weapon-action" checked={action==='remove'} onChange={()=>set({...choice,objects:choice.objects?.length?choice.objects:candidates[0].objects,action:'remove'})}/><span>Keep {hero}'s gun{kind==='dual'?'s':''}<small className="hint"> Your gun is left out of the character.</small></span></label>
        <details className="prep-option-help" open={action==='keep'}><summary>More options</summary>
          <label className="checkbox checkbox-line"><input type="radio" name="weapon-action" checked={action==='keep'} onChange={()=>set({...choice,objects:choice.objects?.length?choice.objects:candidates[0].objects,action:'keep'})}/><span>Keep my gun in the hand as an accessory<small className="hint"> {hero}'s gun stays too, so the character holds both.</small></span></label>
        </details>
      </fieldset>}
      {(action==='replace'||weaponModel)&&<div className="prep-gun-hands">
        {kind==='dual'&&!weaponModel?<>
          <SelectField label="Right hand" value={right?.id??''} values={options_(candidates)} disabled={busy} onChange={id=>set({...choice,objects:pick(id)})}/>
          <SelectField label="Left hand" value={left==='own'?handGun(choice,candidates,'left')?.id??'':left==='copy'?'copy':'keep'} values={[...options_(candidates),['copy','A copy of the right-hand gun'],['keep',`${hero}'s own left gun`]]} disabled={busy}
            onChange={id=>set({...choice,left:{...choice.left,keep:id==='keep',objects:id==='copy'||id==='keep'?[]:pick(id)}})}/>
          {left==='own'&&<button type="button" className="prep-secondary" disabled={busy} onClick={swap}>Swap hands</button>}
        </>:candidates.length>1||fileGuns.length>1?<SelectField label="Your gun" value={right?.id??''} values={options_(candidates)} disabled={busy} onChange={id=>set({...choice,objects:pick(id)})}/>:null}
      </div>}
      {fitting&&<div className="prep-weapon-fit">
        {hands.length>1&&<div className="prep-role-filter" role="group" aria-label="Hand to adjust">{hands.map(h=><button key={h} type="button" aria-pressed={shownHand===h} onClick={()=>setHand(h)}>{h==='right'?'Right-hand gun':'Left-hand gun'}</button>)}</div>}
        {shown?.error?<p className="hint" role="alert">The fit could not be previewed: {shown.error}</p>
          :shownValue?<Suspense fallback={<p>Loading the 3D preview…</p>}><WeaponViewport preview={shownValue} shapes={shapes} focus={shownHand}/></Suspense>
          :<div className="prep-weapon-viewport prep-weapon-loading"><p>Fitting your gun…</p></div>}
        <AdjustControls choice={choice} hand={shownHand} set={set} busy={busy}/>
        {fit&&<FitLine fit={fit} choice={choice} hand={shownHand} hero={hero}/>}
        {adjusted(choice)&&<p className="hint">Adjustments are kept for the next conversion.</p>}
      </div>}
      <details className="prep-advanced"><summary>Advanced gun settings</summary>
        {fitting&&<>
          <SelectField label="Gun size" value={choice.fit??'match'} values={[["match","Match the original gun's size (recommended)"],["keep_size","Keep my gun's size relative to my character"]]} disabled={busy} onChange={value=>set({...choice,fit:value as WeaponChoice['fit']})}/>
          <SelectField label="Gun skinning" value={choice.bone?'bone':choice.skin??'auto'} values={[["auto","Copy the original gun's bone weights (moving parts follow)"],["rigid","Rigid on the main weapon bone"],["bone","Rigid on a bone I choose"]]} disabled={busy} onChange={value=>set(value==='bone'?{...choice,skin:'rigid',bone:bones[0]??''}:{...choice,skin:value as WeaponChoice['skin'],bone:''})}/>
          {Boolean(choice.bone)&&<SelectField label="Weapon bone" value={choice.bone!} values={bones.map(b=>[b,b] as const)} disabled={busy} onChange={bone=>set({...choice,bone})}/>}
          <Check label="Join each gun's parts into one mesh (recommended: faster builds and better in-game performance; materials and bone weights stay)" value={choice.merge??true} disabled={busy} onChange={merge=>set({...choice,merge})}/>
          {hands.map(h=><ExactNumbers key={h} choice={choice} hand={h} set={set} busy={busy}/>)}
        </>}
        {candidates.map(c=><details key={c.id} className="technical"><summary>Parts of {gunName(c,gunFile)}</summary><p className="hint mono">{c.objects.join(', ')}</p>{c.attach_bone&&<p className="hint">Held by {c.attach_bone}.</p>}</details>)}
        {[...(shown?.value?.fit.notes??[]),...(shown?.value?.fit.source.notes??[]).map(n=>`Your gun: ${n}`),...(shown?.value?.fit.target.notes??[]).map(n=>`Original: ${n}`)].map((note,i)=><p key={i} className="hint">{note}</p>)}
      </details>
    </>}
  </Section>;
}
