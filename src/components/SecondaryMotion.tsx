import type { JsonValue, Project, SecondaryMotionChoices } from '../models';
import type { ProjectInspection } from '../build/models';
import { Disclosure, StateLabel } from './Primitives';
import { SelectField } from './CustomizationPages';
import { charmNamed, namedBreast, namedButt } from '../preparation/secondary-bones';
import { KIND_LABELS, PhysicsPresets, TuningSliders, type TuningKind } from './MotionTuning';
/** Part types most models have, offered before an Analyze names this model's own. */
const COMMON_KINDS:TuningKind[]=['hair','streamer','tail','ear','cloth','long_skirt','accessory'];
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
/** Moving-part groups that are butts: by name, by the preparation choice, or by the last Analyze. */
export function buttGroups(project:Project,report:ProjectInspection|null):string[]{
  const kinds=(project.preparation_options.physics_kinds??{}) as Record<string,unknown>;
  const analyzed=report&&'dmx' in report?report.secondary_motion?.parts??[]:[];
  return preparedMotionRoots(project).filter(root=>namedButt(root)||kinds[root]==='butt'||analyzed.some(p=>p.root===root&&p.kind==='butt'));
}
/** Butt motion, separate from breast motion and off by default: its own switch and amount (a smaller, stiffer bounce).
 * With butt bones in the prepared model it applies at the next Analyze and Build; otherwise Model preparation must keep
 * them first (convert again). */
export function ButtMotion({project,update,preparation,openPreparation,disabled,report}:{project:Project;update:(choice:Partial<SecondaryMotionChoices>)=>void;preparation?:(values:Record<string,JsonValue>)=>void;openPreparation?:()=>void;disabled:boolean;report:ProjectInspection|null}){
  const cfg=project.customization.secondary_motion??{},roots=preparedMotionRoots(project),selected=cfg.roots??roots;
  const butts=buttGroups(project,report),kinds={...cfg.part_kinds};
  const wanted=project.preparation_options.butt_motion===true;
  if(!butts.length&&!wanted)return null;
  const on=butts.length>0&&butts.every(root=>kinds[root]!=='rigid'&&selected.includes(root));
  const amount=(cfg.profiles?.[butts[0]]??'automatic') as Movement;
  function toggle(enabled:boolean){
    if(!butts.length){preparation?.({butt_motion:enabled});return;}
    for(const root of butts)kinds[root]=enabled?'butt':'rigid';
    update({part_kinds:kinds,...(cfg.roots&&enabled?{roots:[...new Set([...cfg.roots,...butts])]}:{})});
  }
  return <div className="breast-motion-control">
    <label className="checkbox checkbox-line"><input type="checkbox" checked={on||(!butts.length&&wanted)} disabled={disabled} onChange={event=>toggle(event.target.checked)}/>Butt motion</label>
    {butts.length>0?<>
      <p className="hint">{on?`On for ${butts.join(', ')}: a small bounce, stiffer than breasts. Applies at the next Analyze and Build.`:`Off: ${butts.join(', ')} follow the hips without bouncing.`}</p>
      {on&&<SelectField label="Butt movement" value={amount} values={[["automatic","Automatic small bounce"],["soft","More movement"],["stiff","Stiffer"]]} onChange={value=>update({profiles:{...cfg.profiles,...Object.fromEntries(butts.map(root=>[root,value as Movement]))}})} disabled={disabled}/>}
    </>:<p className="hint">The prepared model has no butt bones that can move yet. In Model preparation, turn on Keep butt bones, convert again, then click Use prepared model.{openPreparation&&<> <button type="button" className="tertiary" disabled={disabled} onClick={openPreparation}>Open Model preparation</button></>}</p>}
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
export function CharmMotion({project,update,disabled,report}:{project:Project;update:(choice:Partial<SecondaryMotionChoices>)=>void;disabled:boolean;report:ProjectInspection|null}){
  const cfg=project.customization.secondary_motion??{},roots=preparedMotionRoots(project),selected=cfg.roots??roots;
  const charms=charmGroups(project,report),kinds={...cfg.part_kinds};
  if(!charms.length)return null;
  const on=charms.every(root=>kinds[root]!=='rigid'&&selected.includes(root));
  const amount=(cfg.profiles?.[charms[0]]??'automatic') as Movement;
  function toggle(enabled:boolean){
    for(const root of charms)kinds[root]=enabled?'charm':'rigid';
    update({part_kinds:kinds,...(cfg.roots&&enabled?{roots:[...new Set([...cfg.roots,...charms])]}:{})});
  }
  return <div className="breast-motion-control">
    <label className="checkbox checkbox-line"><input type="checkbox" checked={on} disabled={disabled} onChange={event=>toggle(event.target.checked)}/>Gun charm motion</label>
    <p className="hint">{on?`On for ${charms.join(', ')}: a light swing that settles quickly and does not sag into the gun. Applies at the next Analyze and Build.`:`Off: ${charms.join(', ')} stay still on the gun.`}</p>
    {on&&<SelectField label="Charm movement" value={amount} values={[["automatic","Automatic light swing"],["soft","More movement"],["stiff","Stiffer"]]} onChange={value=>update({profiles:{...cfg.profiles,...Object.fromEntries(charms.map(root=>[root,value as Movement]))}})} disabled={disabled}/>}
  </div>;
}
export function SecondaryMotion({project,update,disabled,report,fresh,preparation,openPreparation,expert=true}:{expert?:boolean;project:Project;update:(choice:Partial<SecondaryMotionChoices>)=>void;disabled:boolean;report:ProjectInspection|null;fresh:boolean;preparation?:(values:Record<string,JsonValue>)=>void;openPreparation?:()=>void}){
  const cfg=project.customization.secondary_motion??{},roots=preparedMotionRoots(project);
  const selected=cfg.roots??roots,reference=cfg.preset==='tangtang_reference',tang=reference||cfg.preset==='tangtang_r2';
  const checked=fresh&&report&&'dmx' in report?report.secondary_motion:null;
  const protectedCount=checked?.parts?.filter(p=>p.roll_locked).length??0;
  const hairMotion=checked?.hair_motion;
  const longSkirt=checked?.long_skirt;
  const waitingForMapping=fresh&&report&&'dmx' in report&&!checked&&report.unresolved_meshes.length>0;
  return <div className="editor-fields secondary-motion-controls">
    {(expert||tang)&&<div className="motion-preset">
    <SelectField label="Secondary motion preset" value={cfg.preset??'automatic'} values={[["automatic","Automatic — match movement to each part"],["tangtang_reference","TangTang — reference hair and cloth"],["tangtang_r2","TangTang R2 — accepted Haze conversion"]]} onChange={preset=>update({preset:preset as SecondaryMotionChoices['preset']})} disabled={disabled}/>
    <p className="hint">{reference?'Accepted back streamers: flexible along their length, with anti-twist, damping and travel limits. Hair, ears, white cloth and tail keep their accepted tuning. Analyze fits the preset to compatible TangTang conversions, including Preserve source proportions.':tang?'Uses the exact R2 setup: accepted hair and tail, stiffer top ears, more movement in the back strips. Analyze fits the preset to compatible TangTang conversions.':'Build creates and compiles motion for the physics groups kept during model preparation. Body joints are excluded. Hair, streamers, tails, ears, long skirts, cloth and optional breast motion receive separate defaults. Halos, hats and hairpins stay rigid. Hair, tail and streamer chains receive automatic twist protection.'}</p>
    </div>}
    {roots.length===0?<p role="status">No prepared physics groups are saved. Use Model preparation → Keep existing physics bones, convert, then use the prepared model.</p>:<p><StateLabel tone={checked?'ready':'neutral'}>{checked?`${checked.groups.length} groups checked · native fitting checked during Build`:`${tang?roots.length:selected.length} groups selected · ${waitingForMapping?'complete Mesh mapping, then Analyze again':'analyze to check'}`}</StateLabel></p>}
    {!tang&&<BreastMotion project={project} update={update} preparation={preparation} openPreparation={openPreparation} disabled={disabled} report={fresh?report:null}/>}
    {!tang&&<ButtMotion project={project} update={update} preparation={preparation} openPreparation={openPreparation} disabled={disabled} report={fresh?report:null}/>}
    {!tang&&<CharmMotion project={project} update={update} disabled={disabled} report={fresh?report:null}/>}
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
    {!tang&&roots.length>0&&<Disclosure title="Fine-tune movement" hint="Optional · stiffness, damping, gravity and swing per part type, and saved presets">
      {(()=>{const springs=Object.values((project.preparation_options.physics_springs??{}) as Record<string,{source?:string}>);if(!springs.length)return null;const source=[...new Set(springs.map(s=>s.source==='vrm'?'VRM':'MMD'))].join(' and ');
        return <div className="source-springs"><label className="checkbox checkbox-line"><input type="checkbox" checked={cfg.use_source_springs===true} disabled={disabled} onChange={event=>update({use_source_springs:event.target.checked})}/>Start from your model's own {source} spring settings ({springs.length} {springs.length===1?'part':'parts'})</label>
          <p className="hint">Off: the builder's own values (used for every model so far). On: those parts start from the stiffness, damping and gravity your model came with; the sliders then adjust from there. Applies at the next Analyze and Build.</p></div>;})()}
      {!checked?.parts?.length&&<p className="hint">Showing every common part type. After Analyze on Build &amp; install, only the types found in this model are listed.</p>}
      <div className="motion-tuning-grid">{tunableKinds(project,fresh?report:null).map(kind=><TuningSliders key={kind} kind={kind} cfg={cfg} update={update} disabled={disabled}/>)}</div>
      <p className="hint">Each slider starts at the automatic value. The change applies to every part of that type at the next Analyze and Build; a part's own Movement choice under Adjust moving parts is applied first.</p>
      <PhysicsPresets cfg={cfg} update={update} disabled={disabled}/>
    </Disclosure>}
    {!tang&&roots.length>0&&<Disclosure title="Adjust moving parts" hint="Optional · choose groups and how freely each moves">
      <div className="table-scroll"><table><thead><tr><th scope="col">Moving part</th><th scope="col">Part type</th><th scope="col">Movement</th></tr></thead><tbody>{roots.map(root=><tr key={root}><td><label><input type="checkbox" checked={selected.includes(root)} disabled={disabled||(selected.length===1&&selected.includes(root))} onChange={e=>{const next=e.target.checked?[...selected,root]:selected.filter(r=>r!==root);const profiles=Object.fromEntries(Object.entries(cfg.profiles??{}).filter(([r])=>next.includes(r)));const part_kinds=Object.fromEntries(Object.entries(cfg.part_kinds??{}).filter(([r])=>next.includes(r)));update({roots:next,profiles,part_kinds});}}/> {root}</label></td><td><SelectField label={`Part type for ${root}`} value={cfg.part_kinds?.[root]??'automatic'} values={[["automatic",`Automatic${checked?.parts?.find(p=>p.root===root)?' — '+checked.parts.find(p=>p.root===root)!.kind.replaceAll('_',' '):''}`],["hair","Hair"],["streamer","Streamer / ribbon"],["tail","Tail"],["ear","Ear"],["cloth","Cloth"],["long_skirt","Long skirt"],["accessory","Accessory"],["breast","Breast (gentle spring)"],["butt","Butt (small bounce)"],["charm","Charm (light swing)"],["rigid","Rigid — follows its bone"]]} onChange={value=>{const part_kinds={...cfg.part_kinds};if(value==='automatic')delete part_kinds[root];else part_kinds[root]=value as NonNullable<SecondaryMotionChoices['part_kinds']>[string];update({part_kinds});}} disabled={disabled||!selected.includes(root)}/>{checked?.parts?.find(p=>p.root===root)&&<p className="hint">{checked.parts.find(p=>p.root===root)!.reason}. {checked.parts.find(p=>p.root===root)!.roll_locked?'Twist protection planned.':checked.parts.find(p=>p.root===root)!.review_recommended?'Review this part.':''}</p>}</td><td><SelectField label={`Movement for ${root}`} value={cfg.profiles?.[root]??'automatic'} values={[["automatic","Automatic for this part"],["balanced","Balanced"],["stiff","Stiffer"],["soft","More movement"]]} onChange={value=>update({profiles:{...cfg.profiles,[root]:value as 'automatic'|'balanced'|'stiff'|'soft'}})} disabled={disabled||!selected.includes(root)}/></td></tr>)}</tbody></table></div>
    </Disclosure>}
    <p className="hint">Run Analyze, then Build. No proxy export or manual file copying is needed.</p>
  </div>;
}
