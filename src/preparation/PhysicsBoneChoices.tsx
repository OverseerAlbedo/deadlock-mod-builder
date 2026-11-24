import { useId, type ReactNode } from 'react';
import type { PreparationOptions } from './models';
import type { PhysicsGroup } from './secondary-bones';
import { BUTTON_NAMES } from './pageNames';
import { undetectedRoots, keepDetectedGroups } from './savedChoices';
import { PrepCard, PrepCardHeading } from './panels/PrepCard';

/** Breast motion for the current inspection: detected breast bones, whether
 * they were added by the builder, and the result when none could be added. */
export interface BreastChoice {count:number;toggle:(enabled:boolean)=>void;added?:boolean;result?:{status:string;reason?:string};reinspect?:()=>void}
/** Butt bones found in the current inspection (named, or a low mirrored pair on the hips). */
export interface ButtChoice {count:number;toggle:(enabled:boolean)=>void}
const amount='Turn the bounce on or off and choose the amount on the Physics & preview page.';
function breastHint(breasts:BreastChoice,on:boolean):string{
  const bones=`${breasts.count} breast ${breasts.count===1?'bone':'bones'}`;
  if(on&&breasts.count>0)return breasts.added?`Breast bones were added where the chest has breasts. ${amount}`:`Kept: ${bones}. ${amount}`;
  if(on&&breasts.result)return `No breast bones were added: ${breasts.result.reason??'the chest shape could not be measured.'}`;
  if(on)return 'This model has no breast bones. Inspect again to add them where the chest has breasts.';
  return breasts.count>0?`Off. ${bones} ${breasts.count===1?'was':'were'} found; they follow the chest.`:"Off. This model has no breast bones; turning this on adds them where the chest has breasts (inspect again afterwards). Breast bones that come with the rig's own physics are kept either way; the Physics & preview page decides whether they bounce.";
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
  // Groups you were shown are remembered, so a group you leave off is not re-added on the next inspection.
  const change=(values:Partial<PreparationOptions>)=>apply({...values,physics_offered:groups.map(g=>g.root)});
  const enabled=options.preserve_physics_bones??false,roots=options.physics_roots??[],selected=groups.filter(g=>roots.includes(g.root));
  // Charms on a replaced gun hang from the fitted gun's weapon bone.
  const fitted=options.weapon?.enabled===true&&options.weapon.action==='replace';
  const anchor=(g:PhysicsGroup)=>fitted&&gunParts.includes(g.root)?'the fitted gun':g.target_anchor;
  const charms=groups.filter(g=>gunParts.includes(g.root)),missing=undetectedRoots(groups,options);
  const stats=`${selected.length} ${selected.length===1?'group':'groups'} selected · ${selected.reduce((n,g)=>n+g.source_bones.length,0)} bones · ${selected.reduce((n,g)=>n+g.weighted_bones.length,0)} weighted bones`;
  return <PrepCard title="Hair, cloth and other moving parts" className="prep-physics-card" sub="Hair, cloth, tails and other moving parts in your rig are found and kept automatically. Changes here apply the next time you prepare the model.">
    <div className="prep-switches">
      <SwitchRow label="Keep existing physics bones (rig only)" value={enabled} disabled={busy||(!groups.length&&!enabled)} onChange={value=>change({preserve_physics_bones:value,physics_roots:roots.length?roots:groups.map(g=>g.root),reviewed:false})}>
        {enabled&&<p className="prep-physics-stats" role="status">{stats}</p>}
      </SwitchRow>
      {enabled&&<SwitchRow label="Automatically add hair, cloth and tail motion when using the prepared model" value={options.automatic_secondary_motion===true} disabled={busy} onChange={value=>change({automatic_secondary_motion:value,reviewed:false})}>
        <p>Build picks the motion for each part; set the amounts on the Physics &amp; preview page.</p>
      </SwitchRow>}
      {breasts&&<SwitchRow label="Keep or add breast bones (for breast motion)" value={options.breast_motion===true} disabled={busy} onChange={breasts.toggle}>
        <p>{breastHint(breasts,options.breast_motion===true)}</p>
        {options.breast_motion===true&&breasts.count===0&&!breasts.result&&breasts.reinspect&&<button type="button" className="prep-inline-action" disabled={busy} onClick={breasts.reinspect}>Inspect again to add breast bones</button>}
      </SwitchRow>}
      {butts&&butts.count>0&&<SwitchRow label="Keep butt bones (for butt motion)" value={options.butt_motion===true} disabled={busy} onChange={butts.toggle}>
        <p>{options.butt_motion===true?`Kept: ${butts.count} butt ${butts.count===1?'bone':'bones'}, with a small bounce of their own (separate from breasts). Set the amount on the Physics & preview page.`:`Off (the default). ${butts.count} butt ${butts.count===1?'bone was':'bones were'} found; they follow the hips without moving.`}</p>
      </SwitchRow>}
    </div>
    {!groups.length&&<p className="studio-note">No supported physics groups were detected. For a saved preparation from an older version, press {BUTTON_NAMES.loadAndCheck} again to read its rigid-body bindings.</p>}
    {enabled&&<>
      {missing.length>0&&<div role="alert" className="prep-physics-missing prep-callout warn"><p>{missing.length===1?`A saved group is no longer detected: ${missing[0]}.`:`${missing.length} saved groups are no longer detected: ${missing.join(', ')}.`} Press {BUTTON_NAMES.useDetectedGroups} to keep only the groups found now, or {BUTTON_NAMES.loadAndCheck} if you changed the model file.</p><button type="button" disabled={busy} onClick={()=>apply(keepDetectedGroups(groups,options))}>{BUTTON_NAMES.useDetectedGroups}</button></div>}
      {groups.length>0&&<div className="prep-physics-groups">
        <PrepCardHeading actions={<><button type="button" disabled={busy||!groups.length} onClick={()=>change({physics_roots:groups.map(g=>g.root),reviewed:false})}>Keep all detected groups</button><button type="button" disabled={busy||!roots.length} onClick={()=>change({physics_roots:[],reviewed:false})}>Clear group selection</button></>}>Detected groups ({groups.length})</PrepCardHeading>
        <div className="prep-group-list">{groups.map(g=>{const id=`${groupRowId(g.root)}-check`;return <div key={g.root} id={groupRowId(g.root)} className={`prep-group-row ${roots.includes(g.root)?'kept':''} ${marked===g.root?'marked':''}`}>
          <input id={id} type="checkbox" checked={roots.includes(g.root)} disabled={busy} onChange={event=>change({physics_roots:event.target.checked?[...roots,g.root]:roots.filter(n=>n!==g.root),reviewed:false})}/>
          <label htmlFor={id}><strong>{g.root}</strong><span className="sr-only"> · </span><small>{g.source_bones.length} bones → {anchor(g)}</small></label>
        </div>;})}</div>
      </div>}
      {charms.length>0&&<p className="studio-note">{`Gun charms: ${charms.map(g=>g.root).join(', ')} ${charms.length===1?'hangs':'hang'} from the gun. `}{fitted?'They keep their own bones on the fitted gun and get a light swing. ':'They follow the hand with the gun and get a light swing. '}Turn it on or off on the Physics &amp; preview page.</p>}
    </>}
  </PrepCard>;
}
