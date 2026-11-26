// i18n-area: prepare
import { useId, type ReactNode } from 'react';
import { t, tp, useT } from '../i18n';
import { nextSentence, type PreparationOptions } from './models';
import { MAX_KEPT_BONES, keptPhysicsBones, type PhysicsGroup } from './secondary-bones';
import { BUTTON_NAMES } from './pageNames';
import { undetectedRoots, keepDetectedGroups } from './savedChoices';
import { PrepCard, PrepCardHeading } from './panels/PrepCard';

/** Breast motion for the current inspection: detected breast bones, whether
 * they were added by the builder, and the result when none could be added. */
export interface BreastChoice {count:number;toggle:(enabled:boolean)=>void;added?:boolean;result?:{status:string;reason?:string};reinspect?:()=>void}
/** Butt bones found in the current inspection (named, or a low mirrored pair on the hips). */
export interface ButtChoice {count:number;toggle:(enabled:boolean)=>void}
function breastHint(breasts:BreastChoice,on:boolean):string{
  const amount=t('Turn the bounce on or off and choose the amount on the Physics & preview page.');
  if(on&&breasts.count>0)return breasts.added?nextSentence(t('Breast bones were added where the chest has breasts.'),amount):nextSentence(tp(breasts.count,{one:'Kept: {count} breast bone.',other:'Kept: {count} breast bones.'}),amount);
  // The reason comes from the inspection (Blender); only the fallback is the app's.
  if(on&&breasts.result)return breasts.result.reason?t('No breast bones were added: {reason}',{reason:breasts.result.reason}):t('No breast bones were added: the chest shape could not be measured.');
  if(on)return t('This model has no breast bones. Inspect again to add them where the chest has breasts.');
  return breasts.count>0?tp(breasts.count,{one:'Off. {count} breast bone was found; they follow the chest.',other:'Off. {count} breast bones were found; they follow the chest.'}):t("Off. This model has no breast bones; turning this on adds them where the chest has breasts (inspect again afterwards). Breast bones that come with the rig's own physics are kept either way; the Physics & preview page decides whether they bounce.");
}
/** The row of a detected group (a label on the character scrolls to it). */
export const groupRowId=(root:string)=>`prep-group-${Array.from(root).map(c=>/[A-Za-z0-9_-]/.test(c)?c:`-${c.codePointAt(0)!.toString(16)}`).join('')}`;

/** A switch: a checkbox drawn as a toggle, its name as the label and the explanation under it
 * (described by, not part of the name, so the name stays what messages quote). */
function SwitchRow({label,value,disabled,onChange,children}:{label:string;value:boolean;disabled?:boolean;onChange:(value:boolean)=>void;children?:ReactNode}){
  const id=useId(),hint=useId();
  return <div className={`prep-switch-row ${value?'on':''}`}>
    <input id={id} type="checkbox" className="prep-switch" checked={value} disabled={disabled} aria-describedby={children?hint:undefined} onChange={event=>onChange(event.target.checked)}/>
    <div className="prep-switch-text"><label htmlFor={id}>{label}</label>{children&&<div id={hint} className="prep-switch-hint">{children}</div>}</div>
  </div>;
}

export function PhysicsBoneChoices({groups,options,change:apply,busy,breasts,butts,gunParts=[],marked}:{groups:PhysicsGroup[];options:PreparationOptions;change:(values:Partial<PreparationOptions>)=>void;busy:boolean;breasts?:BreastChoice;butts?:ButtChoice;gunParts?:string[];
  /** Optional: the group a label on the character pointed at (shown marked in the list). */marked?:string}){
  const t=useT();
  // Groups you were shown are remembered, so a group you leave off is not re-added on the next inspection.
  const change=(values:Partial<PreparationOptions>)=>apply({...values,physics_offered:groups.map(g=>g.root)});
  const enabled=options.preserve_physics_bones??false,roots=options.physics_roots??[],selected=groups.filter(g=>roots.includes(g.root));
  // Charms on a replaced gun hang from the fitted gun's weapon bone.
  const fitted=options.weapon?.enabled===true&&options.weapon.action==='replace';
  const anchor=(g:PhysicsGroup)=>fitted&&gunParts.includes(g.root)?t('the fitted gun'):g.target_anchor;
  const charms=groups.filter(g=>gunParts.includes(g.root)),missing=undetectedRoots(groups,options);
  const stats=tp(selected.length,{one:'{count} group selected · {bones} bones · {weighted} weighted bones',other:'{count} groups selected · {bones} bones · {weighted} weighted bones'},{bones:selected.reduce((n,g)=>n+g.source_bones.length,0),weighted:selected.reduce((n,g)=>n+g.weighted_bones.length,0)});
  const buttons={use:t(BUTTON_NAMES.useDetectedGroups),load:t(BUTTON_NAMES.loadAndCheck)};
  const charmNames=charms.map(g=>g.root).join(', ');
  // The kept bones against the limit Prepare model's own check uses (it decides; nothing here is disabled by it).
  const kept=keptPhysicsBones(groups,options).size,over=kept-MAX_KEPT_BONES;
  return <PrepCard title={t('Hair, cloth and other moving parts')} className="prep-physics-card" sub={t('Hair, cloth, tails and other moving parts in your rig are found and kept automatically. Changes here apply the next time you prepare the model.')}>
    <div className="prep-switches">
      <SwitchRow label={t('Keep existing physics bones (rig only)')} value={enabled} disabled={busy||(!groups.length&&!enabled)} onChange={value=>change({preserve_physics_bones:value,physics_roots:roots.length?roots:groups.map(g=>g.root),reviewed:false})}>
        {enabled&&<p className="prep-physics-stats" role="status">{stats}</p>}
      </SwitchRow>
      {enabled&&<SwitchRow label={t('Automatically add hair, cloth and tail motion when using the prepared model')} value={options.automatic_secondary_motion===true} disabled={busy} onChange={value=>change({automatic_secondary_motion:value,reviewed:false})}>
        <p>{t('Build picks the motion for each part; set the amounts on the Physics & preview page.')}</p>
      </SwitchRow>}
      {breasts&&<SwitchRow label={t('Keep or add breast bones (for breast motion)')} value={options.breast_motion===true} disabled={busy} onChange={breasts.toggle}>
        <p>{breastHint(breasts,options.breast_motion===true)}</p>
        {options.breast_motion===true&&breasts.count===0&&!breasts.result&&breasts.reinspect&&<button type="button" className="prep-inline-action" disabled={busy} onClick={breasts.reinspect}>{t('Inspect again to add breast bones')}</button>}
      </SwitchRow>}
      {butts&&butts.count>0&&<SwitchRow label={t('Keep butt bones (for butt motion)')} value={options.butt_motion===true} disabled={busy} onChange={butts.toggle}>
        <p>{options.butt_motion===true?tp(butts.count,{one:'Kept: {count} butt bone, with a small bounce of their own (separate from breasts). Set the amount on the Physics & preview page.',other:'Kept: {count} butt bones, with a small bounce of their own (separate from breasts). Set the amount on the Physics & preview page.'}):tp(butts.count,{one:'Off (the default). {count} butt bone was found; they follow the hips without moving.',other:'Off (the default). {count} butt bones were found; they follow the hips without moving.'})}</p>
      </SwitchRow>}
    </div>
    {!groups.length&&<p className="studio-note">{t('No supported physics groups were detected. For a saved preparation from an older version, press {button} again to read its rigid-body bindings.',{button:buttons.load})}</p>}
    {enabled&&<>
      {missing.length>0&&<div role="alert" className="prep-physics-missing prep-callout warn"><p>{missing.length===1?t('A saved group is no longer detected: {name}. Press {use} to keep only the groups found now, or {load} if you changed the model file.',{name:missing[0],...buttons}):tp(missing.length,{other:'{count} saved groups are no longer detected: {names}. Press {use} to keep only the groups found now, or {load} if you changed the model file.'},{names:missing.join(', '),...buttons})}</p><button type="button" disabled={busy} onClick={()=>apply(keepDetectedGroups(groups,options))}>{t(BUTTON_NAMES.useDetectedGroups)}</button></div>}
      {groups.length>0&&<div className="prep-physics-groups">
        <PrepCardHeading actions={<><button type="button" disabled={busy||!groups.length} onClick={()=>change({physics_roots:groups.map(g=>g.root),reviewed:false})}>{t('Keep all detected groups')}</button><button type="button" disabled={busy||!roots.length} onClick={()=>change({physics_roots:[],reviewed:false})}>{t('Clear group selection')}</button></>}>{t('Detected groups ({count})',{count:groups.length})}</PrepCardHeading>
        {selected.length>0&&<p className={`prep-kept-count ${over>0?'over':''}`} aria-live="polite" aria-atomic="true">
          <span>{t('Kept bones: {count} of {limit}',{count:kept,limit:MAX_KEPT_BONES})}</span>
          {over>0&&<>{' '}<span>{tp(over,{one:'{count} bone over the limit. Leaving groups out brings it down.',other:'{count} bones over the limit. Leaving groups out brings it down.'})}</span></>}
        </p>}
        <div className="prep-group-list">{groups.map(g=>{const id=`${groupRowId(g.root)}-check`;return <div key={g.root} id={groupRowId(g.root)} className={`prep-group-row ${roots.includes(g.root)?'kept':''} ${marked===g.root?'marked':''}`}>
          <input id={id} type="checkbox" checked={roots.includes(g.root)} disabled={busy} onChange={event=>change({physics_roots:event.target.checked?[...roots,g.root]:roots.filter(n=>n!==g.root),reviewed:false})}/>
          <label htmlFor={id}><strong>{g.root}</strong><span className="sr-only"> · </span><small>{tp(g.source_bones.length,{other:'{count} bones → {anchor}'},{anchor:anchor(g)})}</small></label>
        </div>;})}</div>
      </div>}
      {charms.length>0&&<p className="studio-note">{fitted
        ?charms.length===1?t('Gun charms: {names} hangs from the gun. They keep their own bones on the fitted gun and get a light swing. Turn it on or off on the Physics & preview page.',{names:charmNames}):t('Gun charms: {names} hang from the gun. They keep their own bones on the fitted gun and get a light swing. Turn it on or off on the Physics & preview page.',{names:charmNames})
        :charms.length===1?t('Gun charms: {names} hangs from the gun. They follow the hand with the gun and get a light swing. Turn it on or off on the Physics & preview page.',{names:charmNames}):t('Gun charms: {names} hang from the gun. They follow the hand with the gun and get a light swing. Turn it on or off on the Physics & preview page.',{names:charmNames})}</p>}
    </>}
  </PrepCard>;
}
