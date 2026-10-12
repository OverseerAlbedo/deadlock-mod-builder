import { Check } from '../components/CustomizationPages';
import { Section } from '../components/Primitives';
import type { PreparationOptions } from './models';
import type { PhysicsGroup } from './secondary-bones';

/** Breast motion for the current inspection: detected breast bones, whether
 * they were added by the builder, and the result when none could be added. */
export interface BreastChoice {count:number;toggle:(enabled:boolean)=>void;added?:boolean;result?:{status:string;reason?:string};reinspect?:()=>void}
const amount='Change the amount under Physics → Automatic motion → Adjust moving parts and check it in game.';
function breastHint(breasts:BreastChoice,on:boolean):string{
  const bones=`${breasts.count} breast ${breasts.count===1?'bone':'bones'}`;
  if(on&&breasts.count>0)return breasts.added?`Breast bones were added where the chest has breasts. They bounce with a soft spring. ${amount}`:`A soft bounce for ${bones}. ${amount}`;
  if(on&&breasts.result)return `No breast bones were added: ${breasts.result.reason??'the chest shape could not be measured.'}`;
  if(on)return 'This model has no breast bones. Inspect again to add them where the chest has breasts.';
  return breasts.count>0?`Off. ${bones} ${breasts.count===1?'was':'were'} found; they follow the chest.`:'Off. This model has no breast bones; turning this on adds them where the chest has breasts (inspect again afterwards).';
}

export function PhysicsBoneChoices({groups,options,change:apply,busy,breasts}:{groups:PhysicsGroup[];options:PreparationOptions;change:(values:Partial<PreparationOptions>)=>void;busy:boolean;breasts?:BreastChoice}){
  // Groups you were shown are remembered, so a group you leave off is not re-added on the next inspection.
  const change=(values:Partial<PreparationOptions>)=>apply({...values,physics_offered:groups.map(g=>g.root)});
  const enabled=options.preserve_physics_bones??false,roots=options.physics_roots??[],selected=groups.filter(g=>roots.includes(g.root));
  return <Section title="Hair, cloth and other moving parts" className="prep-card prep-physics-card">
    <p className="hint">Uses existing physics bindings or recognizable secondary bone chains with skin weights. Supported groups are selected automatically when you inspect a new model.</p>
    <Check label="Keep existing physics bones (rig only)" value={enabled} disabled={busy||(!groups.length&&!enabled)} onChange={value=>change({preserve_physics_bones:value,physics_roots:roots.length?roots:groups.map(g=>g.root),reviewed:false})}/>
    {breasts&&<div className="prep-breast-motion">
      <Check label="Breast motion (optional)" value={options.breast_motion===true} disabled={busy} onChange={breasts.toggle}/>
      <p className="hint">{breastHint(breasts,options.breast_motion===true)} After changing this, convert again and click Use prepared model.</p>
      {options.breast_motion===true&&breasts.count===0&&!breasts.result&&breasts.reinspect&&<button disabled={busy} onClick={breasts.reinspect}>Inspect again to add breast bones</button>}
    </div>}
    {!groups.length&&<p className="hint">No supported physics groups were detected. For a saved preparation from an older version, press Inspect source again to read its rigid-body bindings.</p>}
    {enabled&&<>
      <p className="prep-physics-stats" role="status">{selected.length} groups selected · {selected.reduce((n,g)=>n+g.source_bones.length,0)} bones · {selected.reduce((n,g)=>n+g.weighted_bones.length,0)} weighted bones</p>
      <Check label="Automatically add hair, cloth and tail motion when using the prepared model" value={options.automatic_secondary_motion===true} disabled={busy} onChange={value=>change({automatic_secondary_motion:value,reviewed:false})}/>
      <p className="hint">Preparation keeps the bones and weights. Automatic motion chooses settings for each part during Build, including anti-twist for supported streamers. Rig preservation alone does not create in-game swaying.</p>
      <details className="prep-physics-groups"><summary>Choose detected groups ({groups.length})</summary>
        <div className="row"><button disabled={busy||!groups.length} onClick={()=>change({physics_roots:groups.map(g=>g.root),reviewed:false})}>Keep all detected groups</button><button disabled={busy||!roots.length} onClick={()=>change({physics_roots:[],reviewed:false})}>Clear group selection</button></div>
        <div className="prep-physics-list">{groups.map(g=><div key={g.root}><Check label={`${g.root} · ${g.source_bones.length} bones → ${g.target_anchor}`} value={roots.includes(g.root)} disabled={busy} onChange={checked=>change({physics_roots:checked?[...roots,g.root]:roots.filter(n=>n!==g.root),reviewed:false})}/></div>)}</div>
      </details>
      {roots.some(n=>!groups.some(g=>g.root===n))&&<p role="alert">Some saved groups are no longer detected. Inspect the source and choose the groups again.</p>}
    </>}
  </Section>;
}
