// i18n-area: prepare
import { lazy, Suspense, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react';
import { m, t, tc, useT } from '../i18n';
import { shownHero } from '../i18n/gameNames';
import { api, basename, errorText } from '../api';
import { changeCustomization } from '../components/customization';
import { weaponLike } from '../components/WeaponModels';
import type { Project } from '../models';
import { Check, SelectField } from '../components/CustomizationPages';
import { NumberField } from '../components/MaterialEditor';
import { FilePathInput } from '../components/FilePathInput';
import { type Hand, type PreparationAnalysis, type PreparationOptions, type Vec3, type WeaponCandidate, type WeaponChoice, type WeaponFit, type WeaponPreview } from './models';
import { adjusted, adjustments, fittedHands, gunName, handGun, heroGun, leftMode, nudge, previewShapes, replaceable, sameObjects as same, withAdjustments, yourGuns, type HeroGun, type Nudge } from './gun';
import { SeparateGun, type SeparateGunProps } from './SeparateGun';
import '../studio/studio.css';
import '../studio/studioChoices.css';
import './prepParts.css';
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

/** The hero's name for the sentences of the panel (shown only). */
const heroLabel=(project:Project)=>shownHero(project.hero_name)||(project.original_model.split('/').pop()??'').replace(/\.vmdl(_c)?$/,'')||t('the hero');
/** Clauses shown one after the other in one sentence (a comma in English). */
const nextClause=(first:string,next:string)=>tc('clauses','{first}, {next}',{first,next});

/** Plain-language explanation of the hero's gun, your gun(s) and what will happen. */
function GunSummary({kind,hero,project,analysis,choice,gunFile}:{kind:HeroGun;hero:string;project:Project;analysis:PreparationAnalysis;choice:WeaponChoice;gunFile:string}){
  const t=useT();
  const {file,own}=yourGuns(analysis),reference=analysis.weapon_reference;
  const heroLine={
    dual:()=>t("{hero} holds two guns, one in each hand. They are part of {hero}'s model, so your guns can replace them directly.",{hero}),
    single:()=>t("{hero}'s gun is part of {hero}'s model, so your gun can replace it directly.",{hero}),
    separate:()=>t("{hero}'s gun is a separate model. The app can make it from your gun and build it into the same mod.",{hero}),
    unreadable:()=>t("{hero}'s gun could not be read from the game files ({files}), so it cannot be replaced. You can leave your gun out or keep it in the hand.",{hero,files:reference?.unreadable?.join(', ')??''}),
    weapon_model:()=>t("This project replaces the separate weapon model {model}. Only your gun is used; it is fitted to the original gun's grip, barrel and size.",{model:project.original_model}),
    unknown:()=>t("{hero}'s original gun has not been read yet. Inspect the source again to replace it.",{hero}),
  }[kind]();
  const received=analysis.analysis.weapon_file,fileName=received?.file??gunFile.split(/[\\/]/).pop()??'';
  // Where each gun of the gun file went: one whole clause per gun.
  const gunClause=(side:Hand|null|undefined,placed:boolean,bone:string)=>side==='left'?(placed?t('the left gun was put in {bone}',{bone}):t('the left gun is held by {bone}',{bone})):(placed?t('the right gun was put in {bone}',{bone}):t('the right gun is held by {bone}',{bone}));
  const placed=received?.guns?.length?received.guns.filter(g=>g.bone).map(g=>gunClause(g.side,g.placed,g.bone!)).reduce((a,b)=>a?nextClause(a,b):b,'')
    :received?.bone?(received.placed?t('it was put in the right hand ({bone})',{bone:received.bone}):t('it is held by ({bone})',{bone:received.bone})):'';
  const yours=file.length>1?(placed?t('Your gun file {file} has two guns, one for each hand: {placed}.',{file:fileName,placed}):t('Your gun file {file} has two guns, one for each hand.',{file:fileName}))
    :file.length===1?(placed?t('Your gun file {file} has one gun; {placed}.',{file:fileName,placed}):t('Your gun file {file} has one gun.',{file:fileName}))
    :own.length?(own.length===1?t('Your character file has a gun: {names}.',{names:own.map(c=>gunName(c)).join(tc('list separator','; '))}):t('Your character file has {count} possible guns: {names}.',{count:own.length,names:own.map(c=>gunName(c)).join(tc('list separator','; '))}))
    :t('No gun was found in your files yet.');
  const alsoOwn=file.length>0&&own.length>0?t('Your character file also has a gun ({parts}). It stays on the character unless you set it aside in Scene objects.',{parts:own.map(c=>c.objects.join(' + ')).join(', ')}):'';
  const action=choice.enabled?choice.action:undefined;
  const mode=leftMode(choice);
  const result=action==='replace'&&kind==='dual'?(mode==='own'?t("Your right gun replaces {hero}'s right gun, and your left gun replaces the left one.",{hero}):mode==='copy'?t("Your gun replaces both of {hero}'s guns: a copy goes in each hand.",{hero}):t("Your gun replaces {hero}'s right gun; the left hand keeps {hero}'s own gun.",{hero}))
    :action==='replace'&&kind==='single'?t("Your gun replaces {hero}'s gun. It is fitted to the original gun's grip, barrel and size, so recoil and reload animations still work.",{hero})
    :action==='remove'?(kind==='dual'?t('{hero} keeps both of their own guns; your gun is left out of the character.',{hero}):t('{hero} keeps their own gun; your gun is left out of the character.',{hero}))
    :action==='keep'?t("Your gun stays in the hand as an accessory, and {hero}'s gun stays too.",{hero}):'';
  const charm=analysis.analysis.weapon_file?.charm_bones?.length;
  return <div className="prep-gun-summary" role="status">
    <p className="prep-gun-hero">{heroLine}</p>
    {kind!=='weapon_model'&&<p>{yours}</p>}
    {alsoOwn&&<p className="prep-gun-also">{alsoOwn}</p>}
    {result&&<p className="prep-gun-result">{result}</p>}
    {Boolean(charm)&&action==='replace'&&<p className="prep-gun-charm">{t('The charm on your gun keeps its own bones, so it will swing. Turn it on or off under Physics → Gun charm motion.')}</p>}
  </div>;
}

/** One "what should your character hold" card: a radio with its title, an optional badge and a line of help. */
function HoldCard({checked,disabled,onChange,title,badge,help}:{checked:boolean;disabled?:boolean;onChange:()=>void;title:string;badge?:string;help:ReactNode}){
  return <label className={`studio-option ${checked?'selected':''} ${disabled?'disabled':''}`}>
    <input type="radio" name="weapon-action" checked={checked} disabled={disabled} onChange={onChange}/>
    <span className="studio-option-text"><span className="studio-option-title"><strong>{title}</strong>{badge&&<small className="studio-option-badge ok">{badge}</small>}</span><span className="studio-option-help">{help}</span></span>
  </label>;
}

/** Beginner controls for one hand: flip, turn, tilt, aim, size and small moves. */
/** The beginner controls for one hand, or for both guns at once (each moves in its own hand's frame). */
export function AdjustControls({choice,hand,set,busy}:{choice:WeaponChoice;hand:Hand|'both';set:(next:WeaponChoice)=>void;busy:boolean}){
  const t=useT();
  const apply=(change:Nudge)=>set((hand==='both'?['right','left'] as Hand[]:[hand]).reduce((next,h)=>withAdjustments(next,h,nudge(adjustments(next,h),change)),choice));
  const button=(label:string,change:Nudge,title?:string)=><button type="button" className="prep-secondary" disabled={busy} title={title} onClick={()=>apply(change)}>{label}</button>;
  const roll=t('Roll around the barrel');
  return <div className="prep-gun-adjust" role="group" aria-label={hand==='both'?t('Adjust both guns'):hand==='left'?t('Adjust the left-hand gun'):t('Adjust the right-hand gun')}>
    <div className="prep-gun-adjust-row">{button(t('Flip direction'),{kind:'flip'},t('Point the barrel the other way'))}{button(t('Turn ⟲ 90°'),{kind:'turn',degrees:-90},roll)}{button(t('Turn ⟳ 90°'),{kind:'turn',degrees:90},roll)}{button(t('Smaller'),{kind:'size',factor:1/1.1})}{button(t('Bigger'),{kind:'size',factor:1.1})}<span className="prep-gun-adjust-end">{button(t('Reset'),{kind:'reset'},hand==='both'?t('Undo all adjustments of both guns'):t('Undo all adjustments of this hand'))}</span></div>
    <div className="prep-gun-adjust-row">{button(t('Tilt up 5°'),{kind:'tilt',degrees:5},t('Pitch: raise the muzzle around the grip'))}{button(t('Tilt down 5°'),{kind:'tilt',degrees:-5},t('Pitch: lower the muzzle around the grip'))}{button(t('Aim left 5°'),{kind:'aim',degrees:-5},t('Yaw: turn the muzzle left around the grip'))}{button(t('Aim right 5°'),{kind:'aim',degrees:5},t('Yaw: turn the muzzle right around the grip'))}{button(t('Roll ⟲ 5°'),{kind:'turn',degrees:-5},roll)}{button(t('Roll ⟳ 5°'),{kind:'turn',degrees:5},roll)}</div>
    <div className="prep-gun-adjust-row">{button(t('◀ Back'),{kind:'move',axis:0,inches:-.25})}{button(t('Forward ▶'),{kind:'move',axis:0,inches:.25})}{button(t('▲ Up'),{kind:'move',axis:1,inches:.25})}{button(t('▼ Down'),{kind:'move',axis:1,inches:-.25})}{button(tc('gun move','Left'),{kind:'move',axis:2,inches:-.25})}{button(tc('gun move','Right'),{kind:'move',axis:2,inches:.25})}</div>
  </div>;
}

/** The three moves of Exact numbers, by axis. */
const MOVES=[m('Move forward (inches)'),m('Move up (inches)'),m('Move right (inches)')] as const;
/** Exact numbers for one hand (Advanced). */
function ExactNumbers({choice,hand,set,busy}:{choice:WeaponChoice;hand:Hand;set:(next:WeaponChoice)=>void;busy:boolean}){
  const t=useT();
  const current=adjustments(choice,hand),put=(values:Partial<typeof current>)=>set(withAdjustments(choice,hand,{...current,...values}));
  // The left gun's fields carry its name in front.
  const label=(text:string)=>hand==='left'?t('Left gun: {field}',{field:t(text)}):t(text);
  return <div className="prep-gun-exact" role="group" aria-label={hand==='left'?t('Exact numbers, left gun'):t('Exact numbers')}>
    <Check label={label(m('Flip barrel direction'))} value={current.flip} disabled={busy} onChange={flip=>put({flip})}/>
    <div className="prep-gun-numbers">
      <NumberField label={label(m('Roll around barrel (degrees)'))} value={current.roll} min={-180} max={180} step={15} disabled={busy} update={roll=>put({roll})}/>
      <NumberField label={label(m('Pitch, muzzle up (degrees)'))} value={current.pitch} min={-180} max={180} step={1} disabled={busy} update={pitch=>put({pitch})}/>
      <NumberField label={label(m('Yaw, muzzle right (degrees)'))} value={current.yaw} min={-180} max={180} step={1} disabled={busy} update={yaw=>put({yaw})}/>
      <NumberField label={label(m('Scale multiplier'))} value={current.scale} min={.1} max={10} step={.05} disabled={busy} update={scale=>put({scale})}/>
    </div>
    <div className="prep-gun-numbers">{MOVES.map((move,index)=><NumberField key={move} label={label(move)} value={current.offset[index]} min={-50} max={50} step={.25} disabled={busy} update={value=>{const offset=[...current.offset] as Vec3;offset[index]=value;put({offset});}}/>)}</div>
  </div>;
}

/** How the barrel direction was found (`WeaponFrame.direction`), for the fit line. */
const FRONT:Record<string,string>={names:m('its front was found from the part names'),grip:m('its front is the end away from the grip'),shape:m('its front was guessed from the shape: check the arrows'),flipped:m('direction flipped by you'),flipped_names:m('direction flipped by you, but the part names already gave the front: press Flip direction again unless the preview shows the gun backwards')};
/** The fit in words: size, where the muzzle ends and how the gun's front was found. */
function FitLine({fit,choice,hand,hero}:{fit:WeaponFit;choice:WeaponChoice;hand:Hand;hero:string}){
  const t=useT();
  const factor=adjustments(choice,hand).scale,scaled=Math.abs(factor-1)>.005,percent=Math.round(factor*100);
  const size=choice.fit==='keep_size'?(scaled?t('Size: as in your file next to your character × {percent}%',{percent}):t('Size: as in your file next to your character'))
    :(scaled?t("Size: the length of {hero}'s gun × {percent}%",{hero,percent}):t("Size: the length of {hero}'s gun",{hero}));
  const muzzle=fit.muzzle_offset_mm<=50?t('the muzzle lines up with the original'):t('the muzzle ends {mm} mm from the original (shots still come from the original muzzle)',{mm:fit.muzzle_offset_mm.toFixed(0)});
  const found=FRONT[fit.source.direction??''];
  const direction=found?t(found):fit.source.confidence==='low'||fit.target.confidence==='low'?t('check the direction in the preview'):'';
  return <p className="hint prep-gun-fit-line">{size} · {muzzle}{direction?` · ${direction}`:''}</p>;
}

/** The gun panel, shared by Prepare model's Gun tab and the Gun tab of Appearance & weapons: the
 * fit preview with the hand switch and every adjust button on the left (only while your gun replaces
 * the hero's), and on the right the summary, the gun file, what the character holds, the hands and
 * the advanced settings. `actions` (optional) sit under the summary (Appearance's Apply gun changes).
 * The gun file field shows its label translated and passes its English `name` for the failure titles. */
export function WeaponPanel({project,analysis,options,change,busy,operate,reinspect,separate,actions}:{project:Project;analysis:PreparationAnalysis;options:PreparationOptions;change:(values:Partial<PreparationOptions>)=>void;busy:boolean;operate:Operate;reinspect?:()=>void;separate?:Omit<SeparateGunProps,'project'|'analysis'|'options'|'busy'|'operate'|'change'>;actions?:ReactNode}){
  const t=useT();
  const headingId=useId();
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
  const [hand,setHand]=useState<Hand|'both'>('right');
  // "Both guns" adjusts both hands together; the view and fit line follow the right gun.
  const both=hand==='both'&&hands.length>1;
  const shownHand:Hand=hand!=='both'&&hands.includes(hand)?hand:'right';
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
  const dual=kind==='dual';
  const hold=(next:WeaponChoice['action'])=>()=>set({...choice,objects:choice.objects?.length?choice.objects:candidates[0].objects,action:next});
  const fitView=fitting&&<div className="prep-gun-fitcol">
    <section className="studio-view prep-gun-view" aria-label={t('Gun fit')}>
      <div className="studio-view-bar"><strong className="prep-view-title">{t('Gun fit')}</strong>
        {hands.length>1&&<span className="studio-view-bar-end"><span className="studio-view-group" role="group" aria-label={t('Hand to adjust')}>{hands.map(h=><button key={h} type="button" aria-pressed={!both&&shownHand===h} onClick={()=>setHand(h)}>{h==='right'?t('Right-hand gun'):t('Left-hand gun')}</button>)}<button type="button" aria-pressed={both} onClick={()=>setHand('both')}>{t('Both guns')}</button></span></span>}
      </div>
      <div className="studio-view-stage">
        {shown?.error?<p className="hint prep-gun-error" role="alert">{t('The fit could not be previewed: {error}',{error:shown.error})}</p>
          :shownValue?<Suspense fallback={<p className="studio-view-wait">{t('Loading the 3D preview…')}</p>}><WeaponViewport preview={shownValue} shapes={shapes} focus={both?'both':shownHand}/></Suspense>
          :<div className="prep-weapon-viewport prep-weapon-loading"><p>{t('Fitting your gun…')}</p></div>}
      </div>
      <div className="prep-gun-adjust-area">
        <AdjustControls choice={choice} hand={both?'both':shownHand} set={set} busy={busy}/>
        {fit&&<FitLine fit={fit} choice={choice} hand={shownHand} hero={hero}/>}
        {adjusted(choice)&&<p className="hint">{t('Adjustments are kept for the next conversion.')}</p>}
      </div>
    </section>
  </div>;
  // The fit's notes come from the preview (Rust); only their prefixes are the app's.
  const notes=[...(shown?.value?.fit.notes??[]),...(shown?.value?.fit.source.notes??[]).map(n=>t('Your gun: {note}',{note:n})),...(shown?.value?.fit.target.notes??[]).map(n=>t('Original: {note}',{note:n}))];
  const unavailable=kind==='unreadable'?t("Not available: {hero}'s gun could not be read.",{hero}):kind==='separate'?t("Not available: {hero}'s gun is a separate model.",{hero}):t('Not available: the original gun was not found.');
  return <section className={`prep-gun ${fitting?'with-fit':''}`} aria-labelledby={headingId}><div className="prep-gun-grid">
    {fitView}
    <div className="studio-card prep-gun-settings">
      <div className="studio-card-head"><h2 id={headingId} className="prep-tab-card-title">{weaponModel?t('Weapon model'):t('Gun')}</h2></div>
      <GunSummary kind={kind} hero={hero} project={project} analysis={analysis} choice={choice} gunFile={inspectedFile||gunFile}/>
      {actions}
      {(kind==='single'||kind==='dual')&&stale.length>0&&<div className="prep-gun-file-status prep-callout warn" role="alert"><p>{t("This project also builds a separate weapon model ({models}). {hero}'s gun is part of {hero}'s model, so that weapon project is not needed and would replace something else in the game.",{models:stale.map(v=>basename(v.model??'')).join(', '),hero})}</p>{separate&&<button className="prep-primary" disabled={busy} onClick={()=>{const equipment=project.customization.equipment??{};separate.updateProject(changeCustomization(project,{equipment:{...equipment,variants:(equipment.variants??[]).filter(v=>!stale.includes(v))}}));}}>{t('Stop building it')}</button>}</div>}
      {!weaponModel&&<div className="prep-gun-file">
        <FilePathInput label={t('Gun from another file (optional)')} name="Gun from another file (optional)" kind="weapon" value={gunFile} disabled={busy} fail={fail} update={path=>change({weapon_source:path||undefined,weapon:undefined,reviewed:false})}/>
        <p className="studio-note">{t("Use a gun saved in another Blender, FBX, glTF or MMD (.pmx, .pmd) file. A file with two guns gives one for each hand. It is put in your character's hand automatically; the file itself is never changed.")}</p>
        {gunFilePending&&<div className="prep-gun-file-status prep-callout" role="status"><p>{gunFile?t('Loading the gun from this file needs a new inspection.'):t("Removing the gun file's gun needs a new inspection.")}</p>{reinspect&&<button className="prep-primary" disabled={busy} onClick={reinspect}>{gunFile?t('Load the gun from this file'):t('Inspect again without it')}</button>}</div>}
      </div>}
      {kind==='separate'&&!weaponModel&&separate&&(candidates.length>0||project.customization.equipment?.variants?.some(v=>v.project))?<SeparateGun project={project} analysis={analysis} options={options} busy={busy} operate={operate} change={change} {...separate}/>
      :!candidates.length?<p className="studio-note">{weaponModel?t('No gun was found in the source. Check Scene objects: the gun must be a visible mesh. To use a gun from another file, choose that file as the source path and inspect again.'):t('Choose a gun file above to use a gun from another file.')}</p>
      :<>
        {!weaponModel&&<fieldset className="prep-weapon-actions" disabled={busy}><legend>{t('What should your character hold?')}</legend>
          <div className="studio-options">
            <HoldCard checked={action==='replace'} disabled={!replaceable(kind)} onChange={hold('replace')} title={dual?t("Use my guns instead of {hero}'s",{hero}):t("Use my gun instead of {hero}'s",{hero})} badge={t('recommended')}
              help={replaceable(kind)?(dual?t("Your guns are fitted to {hero}'s grip, barrel and size.",{hero}):t("Your gun is fitted to {hero}'s grip, barrel and size.",{hero})):unavailable}/>
            <HoldCard checked={action==='remove'} onChange={hold('remove')} title={dual?t("Keep {hero}'s guns",{hero}):t("Keep {hero}'s gun",{hero})} help={t('Your gun is left out of the character.')}/>
            <HoldCard checked={action==='keep'} onChange={hold('keep')} title={t('Keep my gun in the hand as an accessory')} help={t("{hero}'s gun stays too, so the character holds both.",{hero})}/>
          </div>
        </fieldset>}
        {(action==='replace'||weaponModel)&&(kind==='dual'&&!weaponModel||candidates.length>1||fileGuns.length>1)&&<div className="prep-gun-section">
          <h3 className="prep-section-heading">{t('Hands')}</h3>
          <div className="prep-gun-hands">
            {kind==='dual'&&!weaponModel?<>
              <SelectField label={t('Right hand')} value={right?.id??''} values={options_(candidates)} disabled={busy} onChange={id=>set({...choice,objects:pick(id)})}/>
              <SelectField label={t('Left hand')} value={left==='own'?handGun(choice,candidates,'left')?.id??'':left==='copy'?'copy':'keep'} values={[...options_(candidates),['copy',t('A copy of the right-hand gun')],['keep',t("{hero}'s own left gun",{hero})]]} disabled={busy}
                onChange={id=>set({...choice,left:{...choice.left,keep:id==='keep',objects:id==='copy'||id==='keep'?[]:pick(id)}})}/>
              {left==='own'&&<button type="button" className="prep-secondary prep-swap" disabled={busy} onClick={swap}>{t('Swap hands')}</button>}
            </>:<SelectField label={t('Your gun')} value={right?.id??''} values={options_(candidates)} disabled={busy} onChange={id=>set({...choice,objects:pick(id)})}/>}
          </div>
          {kind==='dual'&&!weaponModel&&<p className="studio-note">{t("The left hand can have its own gun, a copy of the right-hand gun, or {hero}'s own left gun.",{hero})}</p>}
        </div>}
        <div className="prep-gun-section prep-advanced">
          <h3 className="prep-section-heading">{t('Advanced gun settings')}</h3>
          {fitting&&<>
            <div className="prep-gun-selects">
              <SelectField label={t('Gun size')} value={choice.fit??'match'} values={[["match",t("Match the original gun's size (recommended)")],["keep_size",t("Keep my gun's size relative to my character")]]} disabled={busy} onChange={value=>set({...choice,fit:value as WeaponChoice['fit']})}/>
              <SelectField label={t('Gun skinning')} value={choice.bone?'bone':choice.skin??'auto'} values={[["auto",t("Copy the original gun's bone weights (moving parts follow)")],["rigid",t('Rigid on the main weapon bone')],["bone",t('Rigid on a bone I choose')]]} disabled={busy} onChange={value=>set(value==='bone'?{...choice,skin:'rigid',bone:bones[0]??''}:{...choice,skin:value as WeaponChoice['skin'],bone:''})}/>
              {Boolean(choice.bone)&&<SelectField label={t('Weapon bone')} value={choice.bone!} values={bones.map(b=>[b,b] as const)} disabled={busy} onChange={bone=>set({...choice,bone})}/>}
            </div>
            <label className={`prep-switch-row ${choice.merge??true?'on':''}`}><input type="checkbox" className="prep-switch" checked={choice.merge??true} disabled={busy} onChange={event=>set({...choice,merge:event.target.checked})}/><span className="prep-switch-text">{t("Join each gun's parts into one mesh (recommended: faster builds and better in-game performance; materials and bone weights stay)")}</span></label>
            {hands.map(h=><ExactNumbers key={h} choice={choice} hand={h} set={set} busy={busy}/>)}
          </>}
          {candidates.map(c=><details key={c.id} className="technical prep-gun-parts"><summary>{t('Parts of {gun}',{gun:gunName(c,gunFile)})}</summary><p className="hint mono">{c.objects.join(', ')}</p>{c.attach_bone&&<p className="hint">{t('Held by {bone}.',{bone:c.attach_bone})}</p>}</details>)}
          {notes.map((note,i)=><p key={i} className="studio-note">{note}</p>)}
        </div>
      </>}
    </div>
  </div></section>;
}
