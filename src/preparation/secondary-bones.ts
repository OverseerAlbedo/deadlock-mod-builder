import type { Bone, PreparationOptions, Vec3 } from './models';
export const PHYSICS_BINDING_POLICY='verified-rigid-body-bone-binding-v1';
export const SECONDARY_PREFIX='dmb_secondary_';
export interface PhysicsGroup {root:string;source_anchor:string;target_anchor:string;source_bones:string[];weighted_bones:string[]}
/** Keep in sync with secondary_bones.rs (HINTS) and secondary_bones.py. */
export const SECONDARY_HINTS=['hair','ponytail','braid','ribbon','streamer','skirt','cloth','cape','tail','robe','sash','scarf','shawl','mantle','cloak','tassel','veil','髪','发','髮','尻尾','しっぽ','耳','リボン','裙','マント','スカート','スカーフ'];
const namedSecondary=(name:string)=>SECONDARY_HINTS.some(s=>name.toLowerCase().includes(s))||name.toLowerCase().split(/[^a-z0-9]+/).some(s=>['ear','ears'].includes(s));
/** Rigid head and body accessories. Hair and ear names do not make these move (hairpin, hair ornament, 髪飾り, ear ornament);
 * other motion names such as ribbon or tassel still win. Keep in sync with secondary_bones.rs (RIGID_HINTS) and secondary_bones.py. */
export const RIGID_HINTS=['halo','hat','cap','pin','pins','clip','clips','crown','tiara','circlet','horn','horns','helmet','helm','glasses','goggles','goggle','spectacles','visor','mask','earring','earrings','piercing','hairpin','hairclip','barrette','hairband','headband','ornament','accessory','acc','飾','冠','帽','角','眼鏡','メガネ','輪'];
/** Breast bones move only when breast motion is turned on. */
export const BREAST_HINTS=['breast','bust','boob','pectoral','oppai','おっぱい','乳'];
const isAscii=(text:string)=>/^[\x00-\x7f]*$/.test(text);
/** Split on separators and camel case: HatBrim_L -> hat, brim, l. */
function tokens(name:string):string[]{
  const out:string[]=[];let word='',previousLower=false;
  for(const c of name){
    if(!/[\p{L}\p{N}]/u.test(c)){if(word)out.push(word.toLowerCase());word='';previousLower=false;continue;}
    if(/\p{Lu}/u.test(c)&&previousLower&&word){out.push(word.toLowerCase());word='';}
    previousLower=/\p{Ll}/u.test(c)||/[0-9]/.test(c);word+=c;
  }
  if(word)out.push(word.toLowerCase());
  return out.map(t=>t.replace(/[0-9]+$/,'')).filter(Boolean);
}
function rigidNamed(name:string):boolean{
  const words=tokens(name),n=name.toLowerCase();
  const rigid=RIGID_HINTS.some(h=>isAscii(h)?words.includes(h)||(h.length>=6&&n.includes(h)):n.includes(h));
  const moving=SECONDARY_HINTS.filter(h=>!['hair','髪','发','髮','耳'].includes(h)).some(h=>n.includes(h));
  return rigid&&!moving;
}
const namedBreast=(name:string)=>{const n=name.toLowerCase();return BREAST_HINTS.some(h=>n.includes(h))||(n.includes('胸')&&(n.includes('左')||n.includes('右')||tokens(name).some(t=>['l','r','left','right'].includes(t))));};
const distance=(a:Vec3,b:Vec3)=>Math.sqrt(a.reduce((s,v,k)=>s+(v-b[k])**2,0));
/** A closed loop (halo, crown band, hat brim): the single chain that contains the bone, from below its body anchor, ends close to where it starts. */
function ringChain(bone:Bone,names:Map<string,Bone>,children:Map<string,string[]>,sourceRoles:Partial<Record<string,string>>):boolean{
  const roles=new Set(Object.values(sourceRoles));let steps=0;
  while(bone.parent&&names.has(bone.parent)&&!roles.has(bone.parent)&&steps<512){bone=names.get(bone.parent)!;steps++;}
  const chain=[bone];let cursor=bone.name;
  while(children.get(cursor)?.length){const next=children.get(cursor)!;if(next.length!==1||chain.length>=512||!names.has(next[0]))return false;cursor=next[0];chain.push(names.get(cursor)!);}
  const path=chain.slice(1).reduce((s,b,i)=>s+distance(chain[i].position,b.position),0);
  return chain.length>=4&&path>1e-6&&distance(chain[0].position,chain[chain.length-1].position)<0.3*path;
}
/** Facing direction in the ground plane, from the feet to the toes (default -Y). */
function facing(names:Map<string,Bone>,children:Map<string,string[]>,sourceRoles:Partial<Record<string,string>>):[number,number]{
  const f:[number,number]=[0,0];
  for(const side of ['foot_L','foot_R']){const foot=names.get(sourceRoles[side]??'');if(!foot)continue;
    for(const child of children.get(foot.name)??[]){const c=names.get(child);if(c){f[0]+=c.position[0]-foot.position[0];f[1]+=c.position[1]-foot.position[1];}}}
  const n=Math.sqrt(f[0]**2+f[1]**2);return n<1e-6?[0,-1]:[f[0]/n,f[1]/n];
}
/** Torso bones between the pelvis and the neck (or head), where breast bones hang. */
function torsoChain(names:Map<string,Bone>,sourceRoles:Partial<Record<string,string>>):string[]{
  const chain:string[]=[];const start=names.get(sourceRoles.neck??sourceRoles.head??'');
  let cursor=start?.parent?names.get(start.parent):undefined;
  while(cursor&&chain.length<64){chain.push(cursor.name);if(cursor.name===sourceRoles.pelvis)break;cursor=cursor.parent?names.get(cursor.parent):undefined;}
  return chain;
}
/** Breast bones: weighted, short, non-role children of a torso bone that are named as breasts,
 * or form a mirrored pair in front of the chest between the spine and the neck. */
function breastRoots(bones:Bone[],names:Map<string,Bone>,children:Map<string,string[]>,weighted:Set<string>,sourceRoles:Partial<Record<string,string>>):Set<string>{
  const roleNames=new Set(Object.values(sourceRoles).filter((n):n is string=>Boolean(n)));
  const pelvis=names.get(sourceRoles.pelvis??''),head=names.get(sourceRoles.head??'');if(!pelvis||!head)return new Set();
  const torso=distance(pelvis.position,head.position);if(torso<=1e-6)return new Set();
  const parents=torsoChain(names,sourceRoles);
  const low=(names.get(sourceRoles.spine??'')??pelvis).position[2],high=(names.get(sourceRoles.neck??'')??head).position[2];
  const short=(bone:Bone)=>{const branch=[bone.name];for(let at=0;at<branch.length;at++){if(branch.length>3)return false;branch.push(...(children.get(branch[at])??[]));}
    return branch.length<=3&&branch.every(n=>!roleNames.has(n)&&names.has(n)&&distance(names.get(n)!.position,bone.position)<=0.15*torso);};
  const options=bones.filter(b=>b.parent!==null&&parents.includes(b.parent)&&!roleNames.has(b.name)&&weighted.has(b.name)&&short(b));
  const output=new Set(options.filter(b=>namedBreast(b.name)).map(b=>b.name));
  const forward=facing(names,children,sourceRoles),lateral=[-forward[1],forward[0]];
  const place=(b:Bone)=>{const p=names.get(b.parent!)!,d=[b.position[0]-p.position[0],b.position[1]-p.position[1]];return [d[0]*forward[0]+d[1]*forward[1],d[0]*lateral[0]+d[1]*lateral[1],b.position[2]];};
  options.forEach((a,i)=>{
    const [fa,la,za]=place(a);
    const partners=options.filter((b,j)=>{if(j===i||b.parent!==a.parent)return false;const [fb,lb,zb]=place(b);
      return fa>=0.02*torso&&fb>=0.02*torso&&Math.abs(fa-fb)<=0.03*torso&&Math.abs(za-zb)<=0.03*torso&&la*lb<0&&Math.abs(la+lb)<=0.04*torso&&Math.abs(la-lb)>=0.06*torso&&Math.abs(la-lb)<=0.5*torso&&za>low&&za<high;});
    if(partners.length===1&&!rigidNamed(a.name))output.add(a.name);
  });
  return output;
}
/** Detected breast bones (named, or a mirrored pair in front of the chest). */
export function breastBones(bones:Bone[],weighted:string[],sourceRoles:Partial<Record<string,string>>):string[]{
  const names=new Map(bones.map(b=>[b.name,b])),children=new Map<string,string[]>();
  for(const bone of bones){if(bone.parent)children.set(bone.parent,[...(children.get(bone.parent)??[]),bone.name]);}
  return [...breastRoots(bones,names,children,new Set(weighted),sourceRoles)].sort();
}
/** Unnamed hanging part: a non-role chain of at least three weighted bones on the pelvis/spine/chest/neck/head spanning a quarter of the torso. */
function hangingChain(bone:Bone,names:Map<string,Bone>,children:Map<string,string[]>,weighted:Set<string>,sourceRoles:Partial<Record<string,string>>):boolean{
  const roles=new Set(Object.values(sourceRoles).filter((n):n is string=>Boolean(n)));
  if(roles.has(bone.name)||!bone.parent||!['pelvis','spine','chest','neck','head'].some(r=>sourceRoles[r]===bone.parent))return false;
  const pelvis=names.get(sourceRoles.pelvis??''),head=names.get(sourceRoles.head??'');if(!pelvis||!head)return false;
  const distance=(a:Vec3,b:Vec3)=>Math.sqrt(a.reduce((s,v,k)=>s+(v-b[k])**2,0)),torso=distance(pelvis.position,head.position);
  if(torso<=1e-6)return false;
  const chain=[bone];let cursor=bone.name;
  while(children.get(cursor)?.length){const next=children.get(cursor)!;if(next.length!==1||chain.length>=512||!names.has(next[0]))return false;cursor=next[0];chain.push(names.get(cursor)!);}
  return chain.length>=3&&chain.filter(b=>weighted.has(b.name)).length>=3&&!chain.some(b=>roles.has(b.name))&&distance(chain[0].position,chain[chain.length-1].position)>=0.25*torso;
}
/** Verified physics first; otherwise named, weighted chains with distinct pivots. Breast bones join only with breast motion on
 * (`breast_motion: false` also leaves source-physics breasts on the body; unset keeps the earlier behavior). */
export function physicsGroups(bones:Bone[],weighted:string[],options:PreparationOptions):PhysicsGroup[]{
  const names=new Map(bones.map(b=>[b.name,b])),sourceRoles=options.source_roles??{},targetRoles=options.target_roles??{};
  if(names.size!==bones.length)return [];
  const children=new Map<string,string[]>();for(const bone of bones){if(bone.parent)children.set(bone.parent,[...(children.get(bone.parent)??[]),bone.name]);}
  const physical=new Set(bones.filter(b=>b.physics?.policy===PHYSICS_BINDING_POLICY&&Array.isArray(b.physics.dynamic_bodies)&&b.physics.dynamic_bodies.length&&b.physics.dynamic_bodies.every(n=>typeof n==='string'&&n.length)).map(b=>b.name));
  const weightedSet=new Set(weighted),breasts=options.breast_motion,breastSet=breastRoots(bones,names,children,weightedSet,sourceRoles);
  if(breasts===false)for(const n of breastSet)physical.delete(n);
  const verified=new Set(physical);
  for(const bone of bones.filter(b=>(namedSecondary(b.name)||hangingChain(b,names,children,weightedSet,sourceRoles))&&!rigidNamed(b.name)&&!ringChain(b,names,children,sourceRoles)&&!breastSet.has(b.name))){
    const branch=new Set<string>(),pending=[bone.name];let invalid=false;
    while(pending.length){const n=pending.pop()!;if(branch.has(n)||branch.size>=512){invalid=true;break;}branch.add(n);pending.push(...(children.get(n)??[]));}
    if(invalid||[...branch].some(n=>verified.has(n)||Object.values(sourceRoles).includes(n)||Boolean(names.get(n)?.physics?.dynamic_bodies?.length||names.get(n)?.physics?.kinematic_bodies?.length)))continue;
    const used=weighted.filter(n=>branch.has(n)).map(n=>names.get(n)!);
    if(used.length>=2&&used.slice(1).some(b=>b.position.reduce((s,v,k)=>s+(v-used[0].position[k])**2,0)>1e-10))physical.add(bone.name);
  }
  // A breast is usually one weighted bone; it moves as a whole.
  if(breasts===true)for(const root of breastSet){const p=names.get(root)?.physics;if(!(p?.dynamic_bodies?.length||p?.kinematic_bodies?.length))physical.add(root);}
  const roles=new Set(Object.values(sourceRoles)),weights=new Set(weighted),groups:PhysicsGroup[]=[];
  for(const root of [...physical].sort()){
    let cursor=names.get(root)?.parent,anchor:string|undefined,dynamicParent=false,invalid=false;const seen=new Set([root]);
    while(cursor){if(seen.has(cursor)||!names.has(cursor)){invalid=true;break;}seen.add(cursor);dynamicParent||=physical.has(cursor);if(!anchor&&roles.has(cursor))anchor=cursor;cursor=names.get(cursor)?.parent;}
    if(invalid||dynamicParent||!anchor)continue;
    const branch:string[]=[],pending=[root];
    while(pending.length){const name=pending.shift()!;if(branch.includes(name)||branch.length>=512){invalid=true;break;}branch.push(name);pending.push(...bones.filter(b=>b.parent===name).map(b=>b.name).sort());}
    if(invalid||branch.some(n=>roles.has(n)))continue;
    const used=branch.filter(n=>weights.has(n)).sort(),role=Object.keys(sourceRoles).find(r=>sourceRoles[r as keyof typeof sourceRoles]===anchor) as keyof typeof targetRoles|undefined,destination=role?targetRoles[role]:undefined;
    if(used.length&&destination)groups.push({root,source_anchor:anchor,target_anchor:destination,source_bones:branch,weighted_bones:used});
  }
  return groups;
}
/** New models keep every detected group. When automatic motion is already on, groups detected for the
 * first time (for example after detection improves) are added; groups you were shown and left off stay off. */
export function automaticPhysicsOptions(bones:Bone[],weighted:string[],options:PreparationOptions):PreparationOptions{
  if(options.preserve_physics_bones===false)return options;
  const groups=physicsGroups(bones,weighted,options);
  if(options.preserve_physics_bones===undefined)return groups.length?{...options,preserve_physics_bones:true,physics_roots:groups.map(g=>g.root),automatic_secondary_motion:true}:options;
  if(!options.automatic_secondary_motion)return options;
  const roots=options.physics_roots??[],offered=new Set(Array.isArray(options.physics_offered)?options.physics_offered.filter((n):n is string=>typeof n==='string'):roots);
  const fresh=groups.map(g=>g.root).filter(root=>!offered.has(root)&&!roots.includes(root));
  return fresh.length?{...options,physics_roots:[...roots,...fresh]}:options;
}
export function keptPhysicsBones(groups:PhysicsGroup[],options:PreparationOptions):Set<string>{
  return new Set(options.preserve_physics_bones?groups.filter(g=>options.physics_roots?.includes(g.root)).flatMap(g=>g.source_bones):[]);
}
/** Optional breast motion, off by default. Turning it on keeps the detected breast bones as moving parts with the gentle
 * breast part type (other groups are unchanged); turning it off returns them to the body. Convert again afterwards. */
/** Names of breast bones the builder adds in Blender (resources/blender/breast_bones.py). */
export const GENERATED_BREAST_PREFIX='DMB_Breast_';
export function breastMotionOptions(bones:Bone[],weighted:string[],options:PreparationOptions,enabled:boolean):PreparationOptions{
  const breasts=breastBones(bones,weighted,options.source_roles??{});
  const roots=(options.physics_roots??[]).filter(r=>!breasts.includes(r));
  const kinds=Object.fromEntries(Object.entries(options.physics_kinds??{}).filter(([root])=>!breasts.includes(root)));
  const next:PreparationOptions={...options,breast_motion:enabled,physics_roots:roots,physics_kinds:kinds,reviewed:false};
  if(!enabled){
    // Breast bones the builder added go back to the chest until the next inspection removes them.
    const chest=options.target_roles?.chest??options.target_roles?.spine,added=bones.map(b=>b.name).filter(n=>n.startsWith(GENERATED_BREAST_PREFIX));
    return chest&&added.length?{...next,mapping:{...options.mapping,...Object.fromEntries(added.map(n=>[n,chest]))}}:next;
  }
  const added=physicsGroups(bones,weighted,next).map(g=>g.root).filter(root=>breasts.includes(root));
  const kept=options.preserve_physics_bones===undefined?physicsGroups(bones,weighted,next).map(g=>g.root):[...roots,...added];
  return {...next,preserve_physics_bones:true,automatic_secondary_motion:true,physics_roots:kept,physics_kinds:{...kinds,...Object.fromEntries(added.map(root=>[root,'breast']))}};
}
