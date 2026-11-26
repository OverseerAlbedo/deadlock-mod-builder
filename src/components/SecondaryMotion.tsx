// i18n-area: physics
import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import type { JsonValue, Project, SecondaryMotionChoices } from '../models';
import type { ProjectInspection } from '../build/models';
import { StateLabel } from './Primitives';
import { SelectField } from './CustomizationPages';
import { charmNamed, namedBreast, namedButt, rigidNamed } from '../preparation/secondary-bones';
import { STOPPED_BEFORE_MOVING_PARTS, stoppedBeforeMovingParts, type PreviewActions, type PreviewState } from './previewLink';
import { changedCount, kindName, KIND_LABELS, listOf, PhysicsPresets, shownPresetMessage, TuningSliders, tunedCount, withTuning, type PresetMessage, type TuningKind } from './MotionTuning';
import { m, mc, t, tc, tp, useT } from '../i18n';
import { sentences } from '../build/words';
/** Part types most models have, offered before an Analyze names this model's own. */
const COMMON_KINDS:TuningKind[]=['hair','streamer','tail','ear','cloth','long_skirt','accessory'];
/** A part kind as one word (Analyze's kinds and the part type choice), shown in lower case in English. */
const PART_KIND_WORDS:string[]=[mc('part kind','hair'),mc('part kind','streamer'),mc('part kind','tail'),mc('part kind','ear'),mc('part kind','cloth'),mc('part kind','long skirt'),mc('part kind','accessory'),mc('part kind','breast'),mc('part kind','butt'),mc('part kind','charm'),mc('part kind','rigid')];
/** A part kind's shown word (the saved kind stays the identifier; an unknown kind shows as it is). */
export const partKindWord=(kind:string)=>{const word=kind.replaceAll('_',' ');return PART_KIND_WORDS.includes(word)?tc('part kind',word):word;};
// `sentences`: two whole sentences one after the other (a space in English, none between two Chinese ones).

/** Part types that can be fine-tuned: from the last Analyze, the chosen part types, breast, butt and
 * charm groups, and any type already tuned. Before an Analyze every common type is offered (the
 * model's own types are not known yet). In the order of the labels. */
export function tunableKinds(project:Project,report:ProjectInspection|null):TuningKind[]{
  const cfg=project.customization.secondary_motion??{},selected=cfg.roots??preparedMotionRoots(project);
  const prepared=(project.preparation_options.physics_kinds??{}) as Record<string,unknown>;
  const found=new Set<string>(Object.keys(cfg.tuning??{}));
  const analyzed=report&&'dmx' in report?report.secondary_motion?.parts??[]:[];
  for(const part of analyzed)found.add(part.kind);
  if(!analyzed.length)for(const kind of COMMON_KINDS)found.add(kind);
  for(const root of selected){const kind=cfg.part_kinds?.[root]??prepared[root];if(typeof kind==='string')found.add(kind);}
  if(breastGroups(project,report).length)found.add('breast');
  if(buttGroups(project,report).some(root=>cfg.part_kinds?.[root]==='butt'))found.add('butt');
  if(charmGroups(project,report).some(root=>cfg.part_kinds?.[root]!=='rigid'))found.add('charm');
  return (Object.keys(KIND_LABELS) as TuningKind[]).filter(kind=>found.has(kind));
}
export function preparedMotionRoots(project:Project):string[]{
  const roots=project.preparation_options.physics_roots;
  return project.preparation_options.preserve_physics_bones===true&&Array.isArray(roots)?roots.filter((r):r is string=>typeof r==='string'):[];
}
type Movement='automatic'|'balanced'|'stiff'|'soft';
type Update=(choice:Partial<SecondaryMotionChoices>)=>void;
/** Moving-part groups that are breasts: by name, by the preparation choice, or by the last Analyze. */
export function breastGroups(project:Project,report:ProjectInspection|null):string[]{
  const kinds=(project.preparation_options.physics_kinds??{}) as Record<string,unknown>;
  const analyzed=report&&'dmx' in report?report.secondary_motion?.parts??[]:[];
  return preparedMotionRoots(project).filter(root=>(namedBreast(root)||kinds[root]==='breast'||analyzed.some(p=>p.root===root&&p.kind==='breast'))&&!movesAsOther(analyzed,root,'breast'));
}
/** Analyze typed this breast- or butt-named group as another part: a longer part (a cape, a garment) hangs from the bone
 * and the group moves as that part (an MMD rig's 胸上2.L/R). Its switch would set Breast or Rigid and override that. */
const movesAsOther=(parts:{root:string;kind:string}[],root:string,kind:string)=>parts.some(p=>p.root===root&&p.kind!==kind&&p.kind!=='rigid');
/** Breast- or butt-named groups that move as the longer part hanging from them (see movesAsOther). */
function carryingGroups(project:Project,report:ProjectInspection|null,named:(root:string)=>boolean,kind:string):string[]{
  const analyzed=report&&'dmx' in report?report.secondary_motion?.parts??[]:[];
  return preparedMotionRoots(project).filter(root=>named(root)&&movesAsOther(analyzed,root,kind));
}
const carryingNote=(roots:string[],part:'breast'|'butt')=><div className="breast-motion-control"><p className="hint">{part==='breast'
  ?t('Breast motion: {roots} carry a longer part (a cape or garment) and move with it, so they do not bounce as breasts. To make them bounce instead, choose Breast as their part type on the Moving parts tab.',{roots:listOf(roots)})
  :t('Butt motion: {roots} carry a longer part (a cape or garment) and move with it, so they do not bounce as butts. To make them bounce instead, choose Butt as their part type on the Moving parts tab.',{roots:listOf(roots)})}</p></div>;
/** The on/off switch of a part type (a checkbox with the switch role, so it reads as on/off). */
function MotionSwitch({label,checked,disabled,onChange}:{label:string;checked:boolean;disabled:boolean;onChange:(on:boolean)=>void}){
  const t=useT();
  return <div className="motion-switch-row"><label className="motion-switch"><input type="checkbox" role="switch" checked={checked} disabled={disabled} onChange={event=>onChange(event.target.checked)}/><span className="motion-switch-track" aria-hidden="true"/><span className="motion-switch-label">{label}</span></label><span className="motion-switch-state" aria-hidden="true">{checked?t('On'):t('Off')}</span></div>;
}
/** Whether a part type's switch is on, shared by the switch and its chip so the two always agree. */
function selectedRoots(project:Project){const cfg=project.customization.secondary_motion??{};return cfg.roots??preparedMotionRoots(project);}
const allMoving=(project:Project,groups:string[])=>{const kinds=project.customization.secondary_motion?.part_kinds??{},selected=selectedRoots(project);return groups.length>0&&groups.every(root=>kinds[root]!=='rigid'&&selected.includes(root));};
function breastState(project:Project,report:ProjectInspection|null){
  const groups=breastGroups(project,report);
  return {groups,on:allMoving(project,groups),wanted:project.preparation_options.breast_motion===true,carrying:carryingGroups(project,report,namedBreast,'breast')};
}
function buttState(project:Project,report:ProjectInspection|null){
  const groups=buttGroups(project,report);
  return {groups,on:allMoving(project,groups),wanted:project.preparation_options.butt_motion===true,carrying:carryingGroups(project,report,namedButt,'butt')};
}
/** One place to turn breast motion on or off and set its amount. It applies at
 * the next Analyze and Build when the prepared model already has breast bones;
 * otherwise it says what Model preparation needs to do. */
export function BreastMotion({project,update,preparation,openPreparation,disabled,report}:{project:Project;update:Update;preparation?:(values:Record<string,JsonValue>)=>void;openPreparation?:()=>void;disabled:boolean;report:ProjectInspection|null}){
  const t=useT();
  const cfg=project.customization.secondary_motion??{};
  const {groups:breasts,on,wanted,carrying}=breastState(project,report),kinds={...cfg.part_kinds};
  if(!breasts.length&&carrying.length)return carryingNote(carrying,'breast');
  const amount=(cfg.profiles?.[breasts[0]]??'automatic') as Movement;
  function toggle(enabled:boolean){
    if(!breasts.length){preparation?.({breast_motion:enabled});return;}
    // Off holds the breast bones still (they stay in the model); on bounces them.
    for(const root of breasts){if(enabled)kinds[root]='breast';else kinds[root]='rigid';}
    update({part_kinds:kinds,...(cfg.roots&&enabled?{roots:[...new Set([...cfg.roots,...breasts])]}:{})});
    if(enabled&&!wanted)preparation?.({breast_motion:true});
  }
  return <div className="breast-motion-control">
    <MotionSwitch label={t('Breast motion')} checked={on||(!breasts.length&&wanted)} disabled={disabled} onChange={toggle}/>
    {breasts.length>0?<>
      <p className="hint">{on?t('On for {roots}. Applies at the next Analyze and Build; no conversion needed.',{roots:listOf(breasts)}):t('Off: {roots} follow the chest without bouncing.',{roots:listOf(breasts)})}</p>
      {on&&<SelectField label={t('Breast movement')} value={amount} values={[["automatic",t('Automatic bounce')],["soft",t('More movement')],["stiff",t('Stiffer')]]} onChange={value=>update({profiles:{...cfg.profiles,...Object.fromEntries(breasts.map(root=>[root,value as Movement]))}})} disabled={disabled}/>}
    </>:<p className="hint">{wanted?t('The prepared model has no breast bones that can move yet. In Model preparation, inspect again (breast bones are added where the chest has breasts) and convert, then click Use prepared model.'):t('Turn on to give the breasts a soft bounce. Model preparation then keeps or adds breast bones when you convert.')}{wanted&&openPreparation&&<> <button type="button" className="tertiary" disabled={disabled} onClick={openPreparation}>{t('Open Model preparation')}</button></>}</p>}
  </div>;
}
/** Moving-part groups that are butts: by name, by the preparation choice, or by the last Analyze. */
export function buttGroups(project:Project,report:ProjectInspection|null):string[]{
  const kinds=(project.preparation_options.physics_kinds??{}) as Record<string,unknown>;
  const analyzed=report&&'dmx' in report?report.secondary_motion?.parts??[]:[];
  return preparedMotionRoots(project).filter(root=>(namedButt(root)||kinds[root]==='butt'||analyzed.some(p=>p.root===root&&p.kind==='butt'))&&!movesAsOther(analyzed,root,'butt'));
}
/** Butt motion, separate from breast motion and off by default: its own switch and amount (a smaller, stiffer bounce).
 * With butt bones in the prepared model it applies at the next Analyze and Build; otherwise Model preparation must keep
 * them first (convert again). */
export function ButtMotion({project,update,preparation,openPreparation,disabled,report}:{project:Project;update:Update;preparation?:(values:Record<string,JsonValue>)=>void;openPreparation?:()=>void;disabled:boolean;report:ProjectInspection|null}){
  const t=useT();
  const cfg=project.customization.secondary_motion??{};
  const {groups:butts,on,wanted,carrying}=buttState(project,report),kinds={...cfg.part_kinds};
  if(!butts.length&&carrying.length)return carryingNote(carrying,'butt');
  if(!butts.length&&!wanted)return null;
  const amount=(cfg.profiles?.[butts[0]]??'automatic') as Movement;
  function toggle(enabled:boolean){
    if(!butts.length){preparation?.({butt_motion:enabled});return;}
    for(const root of butts)kinds[root]=enabled?'butt':'rigid';
    update({part_kinds:kinds,...(cfg.roots&&enabled?{roots:[...new Set([...cfg.roots,...butts])]}:{})});
  }
  return <div className="breast-motion-control">
    <MotionSwitch label={t('Butt motion')} checked={on||(!butts.length&&wanted)} disabled={disabled} onChange={toggle}/>
    {butts.length>0?<>
      <p className="hint">{on?t('On for {roots}: a small bounce, stiffer than breasts. Applies at the next Analyze and Build.',{roots:listOf(butts)}):t('Off: {roots} follow the hips without bouncing.',{roots:listOf(butts)})}</p>
      {on&&<SelectField label={t('Butt movement')} value={amount} values={[["automatic",t('Automatic small bounce')],["soft",t('More movement')],["stiff",t('Stiffer')]]} onChange={value=>update({profiles:{...cfg.profiles,...Object.fromEntries(butts.map(root=>[root,value as Movement]))}})} disabled={disabled}/>}
    </>:<p className="hint">{t('The prepared model has no butt bones that can move yet. In Model preparation, turn on Keep butt bones, convert again, then click Use prepared model.')}{openPreparation&&<> <button type="button" className="tertiary" disabled={disabled} onClick={openPreparation}>{t('Open Model preparation')}</button></>}</p>}
  </div>;
}
/** Moving-part groups that are charms (gun charms, keychains, pendants): by name, by the preparation's part type for
 * parts hanging from the gun, or by the last Analyze (a part on a fitted gun's weapon bone). */
export function charmGroups(project:Project,report:ProjectInspection|null):string[]{
  const kinds=(project.preparation_options.physics_kinds??{}) as Record<string,unknown>;
  const analyzed=report&&'dmx' in report?report.secondary_motion?.parts??[]:[];
  return preparedMotionRoots(project).filter(root=>charmNamed(root)||kinds[root]==='charm'||analyzed.some(p=>p.root===root&&p.kind==='charm'));
}
/** Gun charm motion on or off, and how much. Off holds the charms still on the gun (part type Rigid); no conversion needed. */
export function CharmMotion({project,update,disabled,report}:{project:Project;update:Update;disabled:boolean;report:ProjectInspection|null}){
  const t=useT();
  const cfg=project.customization.secondary_motion??{};
  const charms=charmGroups(project,report),kinds={...cfg.part_kinds};
  if(!charms.length)return null;
  const on=allMoving(project,charms);
  const amount=(cfg.profiles?.[charms[0]]??'automatic') as Movement;
  function toggle(enabled:boolean){
    for(const root of charms)kinds[root]=enabled?'charm':'rigid';
    update({part_kinds:kinds,...(cfg.roots&&enabled?{roots:[...new Set([...cfg.roots,...charms])]}:{})});
  }
  return <div className="breast-motion-control">
    <MotionSwitch label={t('Gun charm motion')} checked={on} disabled={disabled} onChange={toggle}/>
    <p className="hint">{on?t('On for {roots}: a light swing that settles quickly and does not sag into the gun. Applies at the next Analyze and Build.',{roots:listOf(charms)}):t('Off: {roots} stay still on the gun.',{roots:listOf(charms)})}</p>
    {on&&<SelectField label={t('Charm movement')} value={amount} values={[["automatic",t('Automatic light swing')],["soft",t('More movement')],["stiff",t('Stiffer')]]} onChange={value=>update({profiles:{...cfg.profiles,...Object.fromEntries(charms.map(root=>[root,value as Movement]))}})} disabled={disabled}/>}
  </div>;
}
/** Ear names, as the native part rule reads them (parts.rs semantic): ear, ears or mimi as a word, or 耳. */
export const earNamed=(name:string)=>name.toLowerCase().split(/[^a-z0-9]+/).some(s=>['ear','ears','mimi'].includes(s))||name.includes('耳');
/** Moving-part groups that are ears: by name, by the preparation's part type, or by the last Analyze. */
export function earGroups(project:Project,report:ProjectInspection|null):string[]{
  const kinds=(project.preparation_options.physics_kinds??{}) as Record<string,unknown>;
  const analyzed=report&&'dmx' in report?report.secondary_motion?.parts??[]:[];
  return preparedMotionRoots(project).filter(root=>(earNamed(root)&&!rigidNamed(root))||kinds[root]==='ear'||analyzed.some(p=>p.root===root&&p.kind==='ear'));
}
const earsOn=(project:Project,ears:string[])=>ears.some(root=>project.customization.secondary_motion?.part_kinds?.[root]!=='rigid');
/** Ear motion on (the default) or off. Off holds the ears still on the head (part type Rigid); on returns them to automatic ear motion. */
export function EarMotion({project,update,disabled,report}:{project:Project;update:Update;disabled:boolean;report:ProjectInspection|null}){
  const t=useT();
  const cfg=project.customization.secondary_motion??{};
  const ears=earGroups(project,report);
  if(!ears.length)return null;
  const on=earsOn(project,ears);
  function toggle(enabled:boolean){
    const kinds={...cfg.part_kinds};
    for(const root of ears){if(enabled)delete kinds[root];else kinds[root]='rigid';}
    update({part_kinds:kinds});
  }
  return <div className="breast-motion-control">
    <MotionSwitch label={t('Ear motion')} checked={on} disabled={disabled} onChange={toggle}/>
    <p className="hint">{on?t('On for {roots}: the ears sway lightly with the head. Applies at the next Analyze and Build.',{roots:listOf(ears)}):t('Off: {roots} follow the head without moving; anything hanging from them, such as earrings in the same group, stays still too.',{roots:listOf(ears)})}</p>
  </div>;
}
/** A part type with its own on/off switch (breasts, butt, gun charms, ears), as its chip and panel show it:
 * `off` when the switch is off, `bones` when the prepared model has groups of that type to move.
 * Null when the switch is not offered (no such groups and nothing asked for). The label is English. */
type SwitchInfo={label:string;off:boolean;bones:boolean};
function switchInfo(kind:TuningKind,project:Project,report:ProjectInspection|null):SwitchInfo|null{
  if(kind==='breast'){const s=breastState(project,report);return {label:m('Breast motion'),bones:s.groups.length>0,off:s.groups.length?!s.on:s.carrying.length?false:!s.wanted};}
  if(kind==='butt'){const s=buttState(project,report);if(!s.groups.length&&!s.wanted&&!s.carrying.length)return null;return {label:m('Butt motion'),bones:s.groups.length>0,off:s.groups.length?!s.on:s.carrying.length?false:!s.wanted};}
  if(kind==='charm'){const charms=charmGroups(project,report);return charms.length?{label:m('Gun charm motion'),bones:true,off:!allMoving(project,charms)}:null;}
  if(kind==='ear'){const ears=earGroups(project,report);return ears.length?{label:m('Ear motion'),bones:true,off:!earsOn(project,ears)}:null;}
  return null;
}
/** The chips: every type that can be fine-tuned, plus every type whose switch is offered (so a type
 * switched off keeps its chip and can be switched on again). In the order of the labels. */
function partChips(project:Project,report:ProjectInspection|null){
  const kinds=new Set<TuningKind>(tunableKinds(project,report));
  const switches=new Map<TuningKind,SwitchInfo>();
  for(const kind of ['breast','butt','charm','ear'] as TuningKind[]){const info=switchInfo(kind,project,report);if(info){switches.set(kind,info);kinds.add(kind);}}
  return {kinds:(Object.keys(KIND_LABELS) as TuningKind[]).filter(kind=>kinds.has(kind)),switches};
}


/** The chip Fine-tune shows: the one chosen, or the first changed type, or the first type. */
export function fineTuneChip(project:Project,report:ProjectInspection|null,chosen:TuningKind|null):TuningKind|null{
  const {kinds}=partChips(project,report),tuning=project.customization.secondary_motion?.tuning;
  return chosen&&kinds.includes(chosen)?chosen:kinds.find(kind=>changedCount(tuning?.[kind])>0)??kinds[0]??null;
}

/** Part-type chips (a tab list: arrow keys, Home and End move between them) and the panel of the chosen type.
 * `chosen`/`choose`: the chip, held by the page so labels on the character can pick it and it survives a tab switch. */
function FineTune({project,update,disabled,report,preparation,openPreparation,chosen:held,choose}:{project:Project;update:Update;disabled:boolean;report:ProjectInspection|null;preparation?:(values:Record<string,JsonValue>)=>void;openPreparation?:()=>void;chosen?:TuningKind|null;choose?:(kind:TuningKind)=>void}){
  const t=useT();
  const cfg=project.customization.secondary_motion??{};
  const {kinds,switches}=partChips(project,report);
  const [own,setOwn]=useState<TuningKind|null>(null);
  const chosen=choose?held??null:own,setChosen=choose??setOwn;
  const tabs=useRef(new Map<TuningKind,HTMLButtonElement>());
  const id=useId();
  const changed=(kind:TuningKind)=>changedCount(cfg.tuning?.[kind]);
  const active=fineTuneChip(project,report,chosen);
  if(!active)return null;
  function move(event:KeyboardEvent<HTMLDivElement>){
    const at=kinds.indexOf(active!);
    const next=({ArrowRight:at+1,ArrowDown:at+1,ArrowLeft:at-1,ArrowUp:at-1,Home:0,End:kinds.length-1} as Record<string,number>)[event.key];
    if(next===undefined)return;
    event.preventDefault();
    event.stopPropagation();
    const kind=kinds[(next+kinds.length)%kinds.length];
    setChosen(kind);tabs.current.get(kind)?.focus();
  }
  const info=switches.get(active),count=changed(active),label=kindName(active);
  const props={project,update,disabled,report};
  const control:ReactNode=active==='breast'?<BreastMotion {...props} preparation={preparation} openPreparation={openPreparation}/>:active==='butt'?<ButtMotion {...props} preparation={preparation} openPreparation={openPreparation}/>:active==='charm'?<CharmMotion {...props}/>:active==='ear'?<EarMotion {...props}/>:null;
  return <>
    <div className="tune-chips" role="tablist" aria-label={t('Part types to fine-tune')} onKeyDown={move}>
      {kinds.map(kind=>{const n=changed(kind),off=switches.get(kind)?.off;
        // The name in one piece: a browser puts a space before the hidden count's comma otherwise.
        const name=`${kindName(kind)}${off?` ${t('Off')}`:''}${n>0?tp(n,{other:', {count} changed'}):''}`;
        return <button key={kind} type="button" role="tab" aria-label={name} id={`${id}-${kind}`} aria-selected={kind===active} aria-controls={`${id}-panel`} tabIndex={kind===active?0:-1}
        ref={element=>{if(element)tabs.current.set(kind,element);else tabs.current.delete(kind);}} className={`tune-chip ${off?'off':''} ${n?'changed':''}`} onClick={()=>setChosen(kind)}>
        {n>0&&<span className="tune-dot" aria-hidden="true"/>}<span>{kindName(kind)}</span>{off&&<>{' '}<small>{t('Off')}</small></>}{n>0&&<span className="sr-only">{tp(n,{other:', {count} changed'})}</span>}
      </button>;})}
    </div>
    <div className="tune-panel" role="tabpanel" id={`${id}-panel`} aria-labelledby={`${id}-${active}`}>
      <div className="tune-panel-head"><h3>{label}</h3>{count>0&&<span className="tune-count">{tp(count,{one:'{count} changed',other:'{count} changed'})}</span>}{count>0&&<button type="button" className="tertiary compact" disabled={disabled} onClick={()=>update({tuning:withTuning(cfg.tuning,active,null)})}>{t('Reset {kind}',{kind:label.toLowerCase()})}</button>}</div>
      {control}
      {(!info||info.bones)&&<TuningSliders kind={active} cfg={cfg} update={update} disabled={disabled||Boolean(info?.off)}/>}
      {info?.bones&&info.off&&<p className="hint tune-off-reason">{t('{name} is off, so these sliders do nothing. Turn it on above to fine-tune {kind}.',{name:t(info.label),kind:label.toLowerCase()})}</p>}
    </div>
  </>;
}

/** What every piece of the Automatic motion controls takes. */
export type MotionProps={expert?:boolean;/** Whether this build carries the TangTang presets (private data); otherwise only Automatic is offered. */tangtangPresets?:boolean;project:Project;update:Update;disabled:boolean;report:ProjectInspection|null;fresh:boolean;preparation?:(values:Record<string,JsonValue>)=>void;openPreparation?:()=>void};

/** The state the pieces share, from the project and the last Analyze. */
function motionState({project,report,fresh}:MotionProps){
  const cfg=project.customization.secondary_motion??{},roots=preparedMotionRoots(project);
  const selected=cfg.roots??roots,reference=cfg.preset==='tangtang_reference',tang=reference||cfg.preset==='tangtang_r2';
  const checked=fresh&&report&&'dmx' in report?report.secondary_motion:null;
  const waitingForMapping=fresh&&report&&'dmx' in report&&!checked&&report.unresolved_meshes.length>0;
  // Kept moving-part bones only in meshes left out of the build (issue #5).
  const leftOut=fresh&&report&&'dmx' in report&&!checked?report.secondary_meshes_left_out??[]:[];
  const waiting=leftOut.length?t('add {meshes} to the main body node in Build & install → Mesh mapping, then Analyze again',{meshes:listOf(leftOut)}):waitingForMapping?t('complete Mesh mapping, then Analyze again'):t('analyze to check');
  // The preparation showed its detected groups and there were none: nothing in the rig can move.
  const offered=project.preparation_options.physics_offered;
  const nothingDetected=Array.isArray(offered)&&offered.length===0;
  const presetHint=reference?t('Accepted back streamers: flexible along their length, with anti-twist, damping and travel limits. Hair, ears, white cloth and tail keep their accepted tuning. Analyze fits the preset to compatible TangTang conversions, including Preserve source proportions.'):tang?t('Uses the exact R2 setup: accepted hair and tail, stiffer top ears, more movement in the back strips. Analyze fits the preset to compatible TangTang conversions.'):t('Build creates and compiles motion for the physics groups kept during model preparation. Body joints are excluded. Hair, streamers, tails, ears, long skirts, cloth and optional breast motion receive separate defaults. Halos, hats and hairpins stay rigid. Hair, tail and streamer chains receive automatic twist protection.');
  return {cfg,roots,selected,tang,checked,waiting,nothingDetected,presetHint,fineTune:!tang&&roots.length>0,tuned:tunedCount(cfg),rep:fresh?report:null};
}
const emptyText=(nothingDetected:boolean)=>nothingDetected?t('No moving parts were found in your model’s rig. Automatic secondary motion moves the hair, cloth and tail bones a model already has; clothes weighted only to body bones cannot move.'):t('No prepared physics groups are saved. Use Model preparation → Keep existing physics bones, convert, then use the prepared model.');
const groupCount=(n:number)=>tp(n,{one:'{count} group',other:'{count} groups'});

/** The top bar's status line in Automatic mode: what the last Analyze recorded, or what is missing. */
export function motionStatusLine(props:MotionProps):string{
  const {roots,selected,tang,checked,waiting,nothingDetected}=motionState(props);
  if(!roots.length)return nothingDetected?t('No moving parts found in your model’s rig'):t('No prepared physics groups saved');
  if(!checked)return `${tp(tang?roots.length:selected.length,{one:'{count} group selected',other:'{count} groups selected'})} · ${waiting}`;
  const bones=checked.preview_nodes?.length??0,kinds=new Set((checked.parts??checked.preview_nodes??[]).map(part=>part.kind).filter(kind=>kind!=='rigid')).size;
  return [groupCount(checked.groups.length),bones?tp(bones,{one:'{count} moving bone',other:'{count} moving bones'}):'',kinds?tp(kinds,{one:'{count} part type',other:'{count} part types'}):'',t('recorded by the last Analyze')].filter(Boolean).join(' · ');
}
/** The status words of the Fine-tune and Moving parts tabs in Automatic mode. */
export function motionTabWords(props:MotionProps){
  const {roots,tang,fineTune,tuned}=motionState(props);
  return {
    finetune:fineTune?(tuned?{status:tp(tuned,{one:'{count} changed',other:'{count} changed'}),tone:'changed' as const}:{status:t('all automatic'),tone:'quiet' as const}):tang?{status:t('preset'),tone:'quiet' as const}:{status:t('no parts'),tone:'attention' as const},
    parts:fineTune?{status:groupCount(roots.length),tone:'quiet' as const}:tang?{status:t('preset'),tone:'quiet' as const}:{status:t('none kept'),tone:'attention' as const},
  };
}

/** The preset choice, saved presets, the changed count and Reset all (the page's top bar). */
export function MotionTools(props:MotionProps){
  const t=useT();
  const {update,disabled,expert=true,tangtangPresets=true}=props;
  const {cfg,tang,fineTune,tuned}=motionState(props);
  const tangOptions:[string,string][]=[["tangtang_reference",m('TangTang — reference hair and cloth')],["tangtang_r2",m('TangTang R2 — accepted Haze conversion')]];
  // A build without the private TangTang data lists only Automatic; a project saved with a preset still shows its choice so the note below can explain it.
  const presetOptions:[string,string][]=[["automatic",t('Automatic — match movement to each part')],...(tangtangPresets?tangOptions.map(([key,text]):[string,string]=>[key,t(text)]):tangOptions.filter(([key])=>key===cfg.preset).map(([key,text]):[string,string]=>[key,t('{name} (not included in this build)',{name:t(text)})]))];
  const [confirming,setConfirming]=useState(false);
  // Kept as English with its values (or the backend's words) and drawn translated, so it follows a change of language.
  const [presetMessage,setPresetMessage]=useState<PresetMessage|null>(null);
  const presetId=useId();
  return <div className="motion-tools">
    {(expert||tang)&&<span className="motion-preset-select" title={t('Secondary motion preset')}><label htmlFor={presetId}>{t('Preset')}</label><select id={presetId} aria-label={t('Secondary motion preset')} className="compact" value={cfg.preset??'automatic'} disabled={disabled} onChange={event=>update({preset:event.target.value as SecondaryMotionChoices['preset']})}>{presetOptions.map(([key,text])=><option value={key} key={key}>{text}</option>)}</select></span>}
    {fineTune&&<PhysicsPresets cfg={cfg} update={update} disabled={disabled} report={setPresetMessage}/>}
    {fineTune&&<span className={`tune-count ${tuned?'':'quiet'}`} title={tuned?tp(tuned,{one:'{count} setting changed from automatic',other:'{count} settings changed from automatic'}):t('Every slider is at automatic')}>{tuned?tp(tuned,{one:'{count} changed',other:'{count} changed'}):t('All automatic')}</span>}
    {fineTune&&tuned>0&&!confirming&&<button type="button" className="tertiary compact" disabled={disabled} onClick={()=>setConfirming(true)}>{t('Reset all…')}</button>}
    {fineTune&&presetMessage&&<p className="hint motion-studio-message" role="status">{shownPresetMessage(presetMessage)}</p>}
    {fineTune&&confirming&&<div className="capability-notice tuning-reset-warning" role="alertdialog" aria-label={t('Reset all fine-tuning')}><strong>{t('Put every slider back to automatic?')}</strong><p>{tp(tuned,{one:'{count} setting across all part types will be cleared. Saved presets are kept; the next Analyze and Build use the automatic values.',other:'{count} settings across all part types will be cleared. Saved presets are kept; the next Analyze and Build use the automatic values.'})}</p><div className="row"><button type="button" className="primary" disabled={disabled} onClick={()=>{update({tuning:{}});setConfirming(false);}}>{t('Reset all')}</button><button type="button" onClick={()=>setConfirming(false)}>{t('Keep my values')}</button></div></div>}
  </div>;
}

/** The line above the sliders that says why they are not seen working in the preview, with the
 * one button that fixes it: load the character, go to Build & install for an Analyze, or switch
 * to Movement (exactly the preview's own Movement button). Quiet when the sliders are seen working.
 * `flash`: a counter the page raises when a slider changed while the line shows; each new value
 * highlights the line once. It never switches the view by itself. */
export function PreviewHint({state,actions,next,flash=0}:{state:PreviewState|null;actions:{current:PreviewActions};next?:()=>void;flash?:number}){
  const t=useT();
  if(!state)return null;
  const line=(text:string,button?:ReactNode)=><div key={flash} className={`preview-hint ${flash?'flash':''}`} role="status"><span>{text}</span>{button}</div>;
  if(!state.loaded)return line(t('Your character is not shown yet, so the sliders cannot be seen working.'),<button type="button" className="primary compact" disabled={!state.canMove} onClick={()=>actions.current.load?.()}>{t('Show my character')}</button>);
  if(state.analyzing&&!state.nodes)return line(t('Analyzing your moving parts… they swing here when it finishes.'));
  // The Movement button analyzes by itself when nothing is analyzed yet, so one click is enough here.
  if(!state.nodes&&!state.analyzed)return line(t('Nothing is analyzed yet in this session. See it move analyzes your moving parts first, then shows them moving.'),<button type="button" className="primary compact" disabled={!state.canMove} onClick={()=>actions.current.showMovement?.()}>{t('See it move')}</button>);
  if(!state.nodes&&state.stopped)return line(t(STOPPED_BEFORE_MOVING_PARTS),next&&<button type="button" className="primary compact" onClick={next}>{t('Go to Build & install')}</button>);
  if(!state.nodes)return line(t('The last Analyze recorded nothing that swings: check the Moving parts tab, then Analyze again on Build & install.'),next&&<button type="button" className="primary compact" onClick={next}>{t('Go to Build & install')}</button>);
  if(state.mode==='still')return line(t('The sliders show in Movement.'),<button type="button" className="primary compact" disabled={!state.canMove} onClick={()=>actions.current.showMovement?.()}>{t('See it move')}</button>);
  return <p className="hint preview-hint-quiet" role="status">{tp(state.nodes,{one:'{count} moving bone follows your sliders.',other:'{count} moving bones follow your sliders.'})}</p>;
}
/** Whether the line asks for something (any state other than the sliders working in Movement). */
export const previewHintShown=(state:PreviewState|null)=>Boolean(state&&(!state.loaded||!state.nodes||state.mode==='still'));

/** The Fine-tune tab: part-type chips, the chosen type's switch and sliders, and their notes. */
export function MotionFineTune({chosen,choose,previewHint,...props}:MotionProps&{chosen?:TuningKind|null;choose?:(kind:TuningKind)=>void;/** The line about the preview (PreviewHint), right above the sliders. */previewHint?:ReactNode}){
  const t=useT();
  const {project,update,disabled,preparation,openPreparation,tangtangPresets=true}=props;
  const {roots,tang,checked,nothingDetected,presetHint,fineTune,rep}=motionState(props);
  return <div className="motion-studio-controls">
    <h3 className="motion-studio-label">{t('Fine-tune movement')}</h3>
    {tang&&<>{!tangtangPresets&&<p className="hint">{t('This copy of the app was built without the TangTang presets. Choose Automatic; Analyze stops with that message otherwise.')}</p>}<p className="hint">{presetHint}</p><p className="hint">{t('The fine-tune sliders, the motion switches and the Moving parts tab apply to the Automatic preset only.')}</p></>}
    {roots.length===0&&<p role="status" className="motion-empty">{emptyText(nothingDetected)}</p>}
    {!tang&&roots.length===0&&<><BreastMotion project={project} update={update} preparation={preparation} openPreparation={openPreparation} disabled={disabled} report={rep}/><ButtMotion project={project} update={update} preparation={preparation} openPreparation={openPreparation} disabled={disabled} report={rep}/></>}
    {fineTune&&<>
      {!checked?.parts?.length&&<p className="hint tune-common">{t('Showing every common part type. After Analyze on Build & install, only the types found in this model are listed.')}</p>}
      {previewHint}
      <FineTune project={project} update={update} disabled={disabled} report={rep} preparation={preparation} openPreparation={openPreparation} chosen={chosen} choose={choose}/>
      <p className="hint">{t('Each slider starts at the automatic value. The change applies to every part of that type at the next Analyze and Build; a part\'s own Movement choice on the Moving parts tab is applied first.')}</p>
    </>}
  </div>;
}

type Filter='all'|'changed';
/** The part type choices: the saved value and its shown name. */
const PART_TYPE_CHOICES:[string,string][]=[["hair",m('Hair')],["streamer",m('Streamer / ribbon')],["tail",m('Tail')],["ear",m('Ear')],["cloth",m('Cloth')],["long_skirt",m('Long skirt')],["accessory",m('Accessory')],["breast",m('Breast (gentle spring)')],["butt",m('Butt (small bounce)')],["charm",m('Charm (light swing)')],["rigid",m('Rigid — follows its bone')]];
/** The Moving parts tab: a part type and a movement for every kept moving part (with the include
 * tick), a filter and a search, automatic hair movement, the source springs and the notes.
 * `highlight`: a part type picked on the character (its first part is scrolled to and marked). */
export function MovingParts({highlight,previewHint,...props}:MotionProps&{highlight?:{kind:string;at:number}|null;/** The line about the preview (PreviewHint), above the list. */previewHint?:ReactNode}){
  const t=useT();
  const {project,update,disabled}=props;
  const {cfg,roots,selected,tang,checked,nothingDetected,fineTune}=motionState(props);
  const [filter,setFilter]=useState<Filter>('all');
  const [query,setQuery]=useState('');
  const [marked,setMarked]=useState<string|null>(null);
  const list=useRef<HTMLDivElement>(null);
  const prepared=(project.preparation_options.physics_kinds??{}) as Record<string,unknown>;
  const partOf=(root:string)=>checked?.parts?.find(p=>p.root===root);
  const kindOf=(root:string)=>cfg.part_kinds?.[root]??partOf(root)?.kind??(typeof prepared[root]==='string'?prepared[root] as string:null);
  const changed=(root:string)=>!selected.includes(root)||cfg.part_kinds?.[root]!==undefined||(cfg.profiles?.[root]!==undefined&&cfg.profiles[root]!=='automatic');
  const changedRoots=roots.filter(changed);
  const words=query.trim().toLowerCase();
  // A search finds a part by its name or its part type (as saved, and as shown).
  const shown=roots.filter(root=>{const kind=kindOf(root)??'';return (filter==='all'||changed(root))&&(!words||root.toLowerCase().includes(words)||kind.replaceAll('_',' ').includes(words)||(kind!==''&&partKindWord(kind).toLowerCase().includes(words)));});
  // A label on the character picked a part type: show and mark its first part.
  useEffect(()=>{
    if(!highlight)return;
    const root=roots.find(r=>kindOf(r)===highlight.kind);
    setMarked(root??null);
    if(!root)return;
    setFilter('all');setQuery('');
    requestAnimationFrame(()=>list.current?.querySelector<HTMLElement>(`[data-root="${CSS.escape(root)}"]`)?.scrollIntoView?.({block:'nearest',behavior:'smooth'}));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only a new pick moves the mark
  },[highlight]);
  const springs=Object.values((project.preparation_options.physics_springs??{}) as Record<string,{source?:string}>);
  const springSources=[...new Set(springs.map(s=>s.source==='vrm'?'VRM':'MMD'))];
  const springSource=springSources.length>1?t('{first} and {next}',{first:springSources[0],next:springSources[1]}):springSources[0]??'';
  const hairMotion=checked?.hair_motion,longSkirt=checked?.long_skirt;
  if(!fineTune)return <div className="moving-parts"><p className="hint">{tang?t('The Moving parts tab applies to the Automatic preset only.'):nothingDetected?t('Nothing to list: your model’s rig has no hair, cloth or tail bones that can move.'):t('Nothing to list yet: the prepared model keeps no moving parts. See the Fine-tune tab for what to do.')}</p></div>;
  return <div className="moving-parts">
    <p className="studio-card-sub">{t('Choose which parts move and how. The app picks a type and a movement for each part; change one when it guessed wrong.')}</p>
    {previewHint}
    <div className="moving-parts-filter">
      <div className="segmented" role="radiogroup" aria-label={t('Show moving parts')}>
        <button type="button" role="radio" aria-checked={filter==='all'} onClick={()=>setFilter('all')}>{tp(roots.length,{other:'All {count}'})}</button>
        <button type="button" role="radio" aria-checked={filter==='changed'} onClick={()=>setFilter('changed')}>{tp(changedRoots.length,{other:'Changed {count}'})}</button>
      </div>
      <input type="search" className="compact" aria-label={t('Find a part')} placeholder={t('Find a part…')} value={query} onChange={event=>setQuery(event.target.value)}/>
    </div>
    <div className="moving-parts-list" ref={list}>
      <div className="moving-parts-row moving-parts-head" aria-hidden="true"><span>{t('Moving part')}</span><span>{t('Part type')}</span><span>{tc('moving parts','Movement')}</span></div>
      {shown.map(root=>{const part=partOf(root),group=checked?.groups.find(g=>g.root===root||g.source_root===root),on=selected.includes(root);return <div key={root} data-root={root} className={`moving-parts-row ${marked===root?'marked':''} ${on?'':'excluded'}`}>
        <div className="moving-parts-name"><label><input type="checkbox" checked={on} disabled={disabled||(selected.length===1&&on)} onChange={e=>{const next=e.target.checked?[...selected,root]:selected.filter(r=>r!==root);const profiles=Object.fromEntries(Object.entries(cfg.profiles??{}).filter(([r])=>next.includes(r)));const part_kinds=Object.fromEntries(Object.entries(cfg.part_kinds??{}).filter(([r])=>next.includes(r)));update({roots:next,profiles,part_kinds});}}/> {root}</label>
          {group&&<small>{tp(group.bones.length,{one:'{count} bone · hangs from {anchor}',other:'{count} bones · hangs from {anchor}'},{anchor:group.anchor})}</small>}</div>
        <div><SelectField label={t('Part type for {root}',{root})} value={cfg.part_kinds?.[root]??'automatic'} values={[["automatic",part?t('Automatic — {kind}',{kind:partKindWord(part.kind)}):t('Automatic')],...PART_TYPE_CHOICES.map(([value,text]):[string,string]=>[value,t(text)])]} onChange={value=>{const part_kinds={...cfg.part_kinds};if(value==='automatic')delete part_kinds[root];else part_kinds[root]=value as NonNullable<SecondaryMotionChoices['part_kinds']>[string];update({part_kinds});}} disabled={disabled||!on}/>
          {part&&<p className="hint">{part.reason}. {part.roll_locked?t('Twist protection planned.'):part.review_recommended?t('Review this part.'):''}</p>}</div>
        <div><SelectField label={t('Movement for {root}',{root})} value={cfg.profiles?.[root]??'automatic'} values={[["automatic",t('Automatic for this part')],["balanced",t('Balanced')],["stiff",t('Stiffer')],["soft",t('More movement')]]} onChange={value=>update({profiles:{...cfg.profiles,[root]:value as 'automatic'|'balanced'|'stiff'|'soft'}})} disabled={disabled||!on}/></div>
      </div>;})}
      {!shown.length&&<p className="hint">{filter==='changed'&&!words?t('Every part has its automatic type and movement.'):t('No part matches.')}</p>}
    </div>
    {hairMotion&&hairMotion.custom_profile_groups.length>0&&<div className="moving-parts-note"><p role="status">{tp(hairMotion.custom_profile_groups.length,{one:'Custom movement overrides: {count} hair group. These retain your chosen settings instead of automatic length adjustment.',other:'Custom movement overrides: {count} hair groups. These retain your chosen settings instead of automatic length adjustment.'})}</p><button type="button" disabled={disabled} onClick={()=>update({profiles:Object.fromEntries(Object.entries(cfg.profiles??{}).filter(([root])=>!hairMotion.custom_profile_groups.includes(root)))})}>{t('Use automatic hair movement')}</button></div>}
    {hairMotion&&hairMotion.review_groups.length>0&&<p role="status" className="moving-parts-note">{tp(hairMotion.review_groups.length,{one:'Shared hair movement could not be verified for {count} group. Review those hair bones on this tab.',other:'Shared hair movement could not be verified for {count} groups. Review those hair bones on this tab.'})}</p>}
    {springs.length>0&&<div className="source-springs moving-parts-note"><label className="checkbox checkbox-line"><input type="checkbox" checked={cfg.use_source_springs===true} disabled={disabled} onChange={event=>update({use_source_springs:event.target.checked})}/>{tp(springs.length,{one:'Start from your model\'s own {source} spring settings ({count} part)',other:'Start from your model\'s own {source} spring settings ({count} parts)'},{source:springSource})}</label>
      <p className="hint">{t('Off: the builder\'s own values (used for every model so far). On: those parts start from the stiffness, damping and gravity your model came with; the sliders then adjust from there. Applies at the next Analyze and Build.')}</p></div>}
    {longSkirt?.stability?.seams?.length?<p role="status" className="moving-parts-note">{t('Connected coat and skirt panels receive separation limits. Intentional openings are kept.')}</p>:null}
  </div>;
}

/** What the last Analyze recorded for the moving parts, and the mode's notes (the Physics mode tab). */
export function MotionStatus(props:MotionProps){
  const t=useT();
  const {expert=true}=props;
  const {roots,tang,checked,presetHint,rep}=motionState(props);
  const protectedCount=checked?.parts?.filter(p=>p.roll_locked).length??0;
  const hairMotion=checked?.hair_motion,longSkirt=checked?.long_skirt;
  const detected=checked?.parts?t('Detected: {parts}.',{parts:Object.entries(checked.parts.reduce<Record<string,number>>((counts,p)=>({...counts,[p.kind]:(counts[p.kind]??0)+1}),{})).map(([kind,n])=>tp(n,{other:'{count} {kind}'},{kind:partKindWord(kind)})).join(' · ')}):'';
  const reviewNote=checked?.parts?.some(p=>p.review_recommended)?t('Some parts need review; check their type and twist protection on the Moving parts tab.'):'';
  return <div className="motion-studio-foot">
    <h4>{t('What the last Analyze recorded')}</h4>
    {checked?<div className="studio-cards motion-status-grid">
      {roots.length>0&&<p><StateLabel tone="ready">{tp(checked.groups.length,{one:'{count} group checked · native fitting checked during Build',other:'{count} groups checked · native fitting checked during Build'})}</StateLabel></p>}
      {checked.parts&&<p role="status">{reviewNote?sentences(detected,reviewNote):`${detected} `}</p>}
      {checked.parts&&<p role="status">{tp(protectedCount,{one:'Twist protection planned for {count} group · checked during Build.',other:'Twist protection planned for {count} groups · checked during Build.'})}</p>}
      {hairMotion&&hairMotion.shared_profile_groups.length>0&&<p role="status">{tp(hairMotion.shared_profile_groups.length,{one:'Automatic hair movement: {count} group. Longer strands receive softer settings based on their length relative to the character. Short hair keeps its existing movement.',other:'Automatic hair movement: {count} groups. Longer strands receive softer settings based on their length relative to the character. Short hair keeps its existing movement.'})}</p>}
      {hairMotion&&(hairMotion.length_adjusted_groups?.length??0)>0&&<p role="status">{tp(hairMotion.length_adjusted_groups!.length,{one:'Long-hair softening applied to {count} group.',other:'Long-hair softening applied to {count} groups.'})}</p>}
      {hairMotion&&(hairMotion.jump_protected_groups?.length??0)>0&&<p role="status">{tp(hairMotion.jump_protected_groups!.length,{one:'Hair jump protection: stretch resistance and height limits for {count} group.',other:'Hair jump protection: stretch resistance and height limits for {count} groups.'})}</p>}
      {longSkirt&&<p role="status">{tp(longSkirt.groups.length,{one:'Long-skirt support: {count} group with mesh-verified panel connections, stretch resistance and bounded movement.',other:'Long-skirt support: {count} groups with mesh-verified panel connections, stretch resistance and bounded movement.'})}</p>}
    </div>:stoppedBeforeMovingParts(rep,props.project)?<p className="studio-note">{t(STOPPED_BEFORE_MOVING_PARTS)}</p>:<p className="studio-note">{t('Nothing recorded for this version of the project yet: Analyze on Build & install checks the moving parts.')}</p>}
    <p className="studio-note">{t('The movement preview swings what Analyze recorded; nothing is analyzed when this page opens.')}</p>
    {expert&&!tang&&<p className="hint">{presetHint}</p>}
    <p className="hint">{sentences(t('This mode replaces the original hero’s cloth simulation. Skeleton, animations and rigid-body physics are retained.'),tang?t('Body collision for the new moving parts is not added.'):t('Body colliders on the legs, arms, hips, neck and head (those your rig allows, measured by Analyze) keep the new moving parts out of the body.'))}</p>
  </div>;
}
