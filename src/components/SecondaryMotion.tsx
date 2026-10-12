import type { JsonValue, Project, SecondaryMotionChoices } from '../models';
import type { ProjectInspection } from '../build/models';
import { Disclosure, StateLabel } from './Primitives';
import { SelectField } from './CustomizationPages';
import { namedBreast } from '../preparation/secondary-bones';
export function preparedMotionRoots(project:Project):string[]{
  const roots=project.preparation_options.physics_roots;
  return project.preparation_options.preserve_physics_bones===true&&Array.isArray(roots)?roots.filter((r):r is string=>typeof r==='string'):[];
}
type Movement='automatic'|'balanced'|'stiff'|'soft';
/** Moving-part groups that are breasts: by name, by the preparation choice, or by the last Analyze. */
export function breastGroups(project:Project,report:ProjectInspection|null):string[]{
  const kinds=(project.preparation_options.physics_kinds??{}) as Record<string,unknown>;
  const analyzed=report&&'dmx' in report?report.secondary_motion?.parts??[]:[];
  return preparedMotionRoots(project).filter(root=>namedBreast(root)||kinds[root]==='breast'||analyzed.some(p=>p.root===root&&p.kind==='breast'));
}
/** One place to turn breast motion on or off and set its amount. It applies at
 * the next Analyze and Build when the prepared model already has breast bones;
 * otherwise it says what Model preparation needs to do. */
export function BreastMotion({project,update,preparation,openPreparation,disabled,report}:{project:Project;update:(choice:Partial<SecondaryMotionChoices>)=>void;preparation?:(values:Record<string,JsonValue>)=>void;openPreparation?:()=>void;disabled:boolean;report:ProjectInspection|null}){
  const cfg=project.customization.secondary_motion??{},roots=preparedMotionRoots(project),selected=cfg.roots??roots;
  const breasts=breastGroups(project,report),kinds={...cfg.part_kinds};
  const on=breasts.length>0&&breasts.every(root=>kinds[root]!=='rigid'&&selected.includes(root));
  const wanted=project.preparation_options.breast_motion===true;
  const amount=(cfg.profiles?.[breasts[0]]??'automatic') as Movement;
  function toggle(enabled:boolean){
    if(!breasts.length){preparation?.({breast_motion:enabled});return;}
    // Off holds the breast bones still (they stay in the model); on bounces them.
    for(const root of breasts){if(enabled)kinds[root]='breast';else kinds[root]='rigid';}
    update({part_kinds:kinds,...(cfg.roots&&enabled?{roots:[...new Set([...cfg.roots,...breasts])]}:{})});
    if(enabled&&!wanted)preparation?.({breast_motion:true});
  }
  return <div className="breast-motion-control">
    <label className="checkbox checkbox-line"><input type="checkbox" checked={on||(!breasts.length&&wanted)} disabled={disabled} onChange={event=>toggle(event.target.checked)}/>Breast motion</label>
    {breasts.length>0?<>
      <p className="hint">{on?`On for ${breasts.join(', ')}. Applies at the next Analyze and Build; no conversion needed.`:`Off: ${breasts.join(', ')} follow the chest without bouncing.`}</p>
      {on&&<SelectField label="Breast movement" value={amount} values={[["automatic","Automatic bounce"],["soft","More movement"],["stiff","Stiffer"]]} onChange={value=>update({profiles:{...cfg.profiles,...Object.fromEntries(breasts.map(root=>[root,value as Movement]))}})} disabled={disabled}/>}
    </>:<p className="hint">{wanted?'The prepared model has no breast bones that can move yet. In Model preparation, inspect again (breast bones are added where the chest has breasts) and convert, then click Use prepared model.':'Turn on to give the breasts a soft bounce. Model preparation then keeps or adds breast bones when you convert.'}{wanted&&openPreparation&&<> <button type="button" className="tertiary" disabled={disabled} onClick={openPreparation}>Open Model preparation</button></>}</p>}
  </div>;
}
export function SecondaryMotion({project,update,disabled,report,fresh,preparation,openPreparation}:{project:Project;update:(choice:Partial<SecondaryMotionChoices>)=>void;disabled:boolean;report:ProjectInspection|null;fresh:boolean;preparation?:(values:Record<string,JsonValue>)=>void;openPreparation?:()=>void}){
  const cfg=project.customization.secondary_motion??{},roots=preparedMotionRoots(project);
  const selected=cfg.roots??roots,reference=cfg.preset==='tangtang_reference',tang=reference||cfg.preset==='tangtang_r2';
  const checked=fresh&&report&&'dmx' in report?report.secondary_motion:null;
  const protectedCount=checked?.parts?.filter(p=>p.roll_locked).length??0;
  const hairMotion=checked?.hair_motion;
  const longSkirt=checked?.long_skirt;
  const waitingForMapping=fresh&&report&&'dmx' in report&&!checked&&report.unresolved_meshes.length>0;
  return <div className="editor-fields secondary-motion-controls">
    <div className="motion-preset">
    <SelectField label="Secondary motion preset" value={cfg.preset??'automatic'} values={[["automatic","Automatic — match movement to each part"],["tangtang_reference","TangTang — reference hair and cloth"],["tangtang_r2","TangTang R2 — accepted Haze conversion"]]} onChange={preset=>update({preset:preset as SecondaryMotionChoices['preset']})} disabled={disabled}/>
    <p className="hint">{reference?'Accepted back streamers: flexible along their length, with anti-twist, damping and travel limits. Hair, ears, white cloth and tail keep their accepted tuning. Analyze fits the preset to compatible TangTang conversions, including Preserve source proportions.':tang?'Uses the exact R2 setup: accepted hair and tail, stiffer top ears, more movement in the back strips. Analyze fits the preset to compatible TangTang conversions.':'Build creates and compiles motion for the physics groups kept during model preparation. Body joints are excluded. Hair, streamers, tails, ears, long skirts, cloth and optional breast motion receive separate defaults. Halos, hats and hairpins stay rigid. Hair, tail and streamer chains receive automatic twist protection.'}</p>
    </div>
    {roots.length===0?<p role="status">No prepared physics groups are saved. Use Model preparation → Keep existing physics bones, convert, then use the prepared model.</p>:<p><StateLabel tone={checked?'ready':'neutral'}>{checked?`${checked.groups.length} groups checked · native fitting checked during Build`:`${tang?roots.length:selected.length} groups selected · ${waitingForMapping?'complete Mesh mapping, then Analyze again':'analyze to check'}`}</StateLabel></p>}
    {!tang&&<BreastMotion project={project} update={update} preparation={preparation} openPreparation={openPreparation} disabled={disabled} report={fresh?report:null}/>}
    <div className="motion-status-grid">
    {checked?.parts&&<p role="status">Detected: {Object.entries(checked.parts.reduce<Record<string,number>>((counts,p)=>({...counts,[p.kind]:(counts[p.kind]??0)+1}),{})).map(([kind,n])=>`${n} ${kind.replaceAll('_',' ')}`).join(' · ')}. {checked.parts.filter(p=>p.review_recommended).length>0?'Some parts need review; check their type and twist protection under Adjust moving parts.':''}</p>}
    {checked?.parts&&<p role="status">Twist protection planned for {protectedCount} {protectedCount===1?'group':'groups'} · checked during Build.</p>}
    {hairMotion&&hairMotion.shared_profile_groups.length>0&&<p role="status">Automatic hair movement: {hairMotion.shared_profile_groups.length} {hairMotion.shared_profile_groups.length===1?'group':'groups'}. Longer strands receive softer settings based on their length relative to the character. Short hair keeps its existing movement.</p>}
    {hairMotion&&(hairMotion.length_adjusted_groups?.length??0)>0&&<p role="status">Long-hair softening applied to {hairMotion.length_adjusted_groups!.length} {hairMotion.length_adjusted_groups!.length===1?'group':'groups'}.</p>}
    {hairMotion&&(hairMotion.jump_protected_groups?.length??0)>0&&<p role="status">Long-hair jump protection: stretch resistance and height limits for {hairMotion.jump_protected_groups!.length} {hairMotion.jump_protected_groups!.length===1?'group':'groups'}.</p>}
    {longSkirt&&<p role="status">Long-skirt support: {longSkirt.groups.length} {longSkirt.groups.length===1?'group':'groups'} with mesh-verified panel connections, stretch resistance and bounded movement.</p>}
    {longSkirt?.stability?.seams?.length? <p role="status">Connected coat and skirt panels receive separation limits. Intentional openings are kept.</p>:null}
    {hairMotion&&hairMotion.custom_profile_groups.length>0&&<div><p role="status">Custom movement overrides: {hairMotion.custom_profile_groups.length} hair {hairMotion.custom_profile_groups.length===1?'group':'groups'}. These retain your chosen settings instead of automatic length adjustment.</p><button type="button" disabled={disabled} onClick={()=>update({profiles:Object.fromEntries(Object.entries(cfg.profiles??{}).filter(([root])=>!hairMotion.custom_profile_groups.includes(root)))})}>Use automatic hair movement</button></div>}
    {hairMotion&&hairMotion.review_groups.length>0&&<p role="status">Shared hair movement could not be verified for {hairMotion.review_groups.length} groups. Review those hair bones under Adjust moving parts.</p>}
    </div>
    <p className="hint">This mode replaces the original hero’s cloth simulation. Skeleton, animations and rigid-body physics are retained. Body collision for the new moving parts is not added.</p>
    {!tang&&roots.length>0&&<Disclosure title="Adjust moving parts" hint="Optional · choose groups and how freely each moves">
      <div className="table-scroll"><table><thead><tr><th scope="col">Moving part</th><th scope="col">Part type</th><th scope="col">Movement</th></tr></thead><tbody>{roots.map(root=><tr key={root}><td><label><input type="checkbox" checked={selected.includes(root)} disabled={disabled||(selected.length===1&&selected.includes(root))} onChange={e=>{const next=e.target.checked?[...selected,root]:selected.filter(r=>r!==root);const profiles=Object.fromEntries(Object.entries(cfg.profiles??{}).filter(([r])=>next.includes(r)));const part_kinds=Object.fromEntries(Object.entries(cfg.part_kinds??{}).filter(([r])=>next.includes(r)));update({roots:next,profiles,part_kinds});}}/> {root}</label></td><td><SelectField label={`Part type for ${root}`} value={cfg.part_kinds?.[root]??'automatic'} values={[["automatic",`Automatic${checked?.parts?.find(p=>p.root===root)?' — '+checked.parts.find(p=>p.root===root)!.kind.replaceAll('_',' '):''}`],["hair","Hair"],["streamer","Streamer / ribbon"],["tail","Tail"],["ear","Ear"],["cloth","Cloth"],["long_skirt","Long skirt"],["accessory","Accessory"],["breast","Breast (gentle spring)"],["rigid","Rigid — follows its bone"]]} onChange={value=>{const part_kinds={...cfg.part_kinds};if(value==='automatic')delete part_kinds[root];else part_kinds[root]=value as NonNullable<SecondaryMotionChoices['part_kinds']>[string];update({part_kinds});}} disabled={disabled||!selected.includes(root)}/>{checked?.parts?.find(p=>p.root===root)&&<p className="hint">{checked.parts.find(p=>p.root===root)!.reason}. {checked.parts.find(p=>p.root===root)!.roll_locked?'Twist protection planned.':checked.parts.find(p=>p.root===root)!.review_recommended?'Review this part.':''}</p>}</td><td><SelectField label={`Movement for ${root}`} value={cfg.profiles?.[root]??'automatic'} values={[["automatic","Automatic for this part"],["balanced","Balanced"],["stiff","Stiffer"],["soft","More movement"]]} onChange={value=>update({profiles:{...cfg.profiles,[root]:value as 'automatic'|'balanced'|'stiff'|'soft'}})} disabled={disabled||!selected.includes(root)}/></td></tr>)}</tbody></table></div>
    </Disclosure>}
    <p className="hint">Run Analyze, then Build. No proxy export or manual file copying is needed. Check the compiled character’s movement in game.</p>
  </div>;
}
