import type { Bone, PreparationOptions, Vec3 } from './models';
export const PHYSICS_BINDING_POLICY='verified-rigid-body-bone-binding-v1';
export const SECONDARY_PREFIX='dmb_secondary_';
export interface PhysicsGroup {root:string;source_anchor:string;target_anchor:string;source_bones:string[];weighted_bones:string[]}
/** Keep in sync with secondary_bones.rs (HINTS) and secondary_bones.py. */
export const SECONDARY_HINTS=['hair','ponytail','braid','ribbon','streamer','skirt','cloth','cape','tail','robe','sash','scarf','shawl','mantle','cloak','tassel','veil','髪','发','髮','尻尾','しっぽ','耳','リボン','裙','マント','スカート','スカーフ'];
const namedSecondary=(name:string)=>SECONDARY_HINTS.some(s=>name.toLowerCase().includes(s))||name.toLowerCase().split(/[^a-z0-9]+/).some(s=>['ear','ears'].includes(s));
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
/** Verified physics first; otherwise named, weighted chains with distinct pivots. */
export function physicsGroups(bones:Bone[],weighted:string[],options:PreparationOptions):PhysicsGroup[]{
  const names=new Map(bones.map(b=>[b.name,b])),sourceRoles=options.source_roles??{},targetRoles=options.target_roles??{};
  if(names.size!==bones.length)return [];
  const children=new Map<string,string[]>();for(const bone of bones){if(bone.parent)children.set(bone.parent,[...(children.get(bone.parent)??[]),bone.name]);}
  const physical=new Set(bones.filter(b=>b.physics?.policy===PHYSICS_BINDING_POLICY&&Array.isArray(b.physics.dynamic_bodies)&&b.physics.dynamic_bodies.length&&b.physics.dynamic_bodies.every(n=>typeof n==='string'&&n.length)).map(b=>b.name));
  const verified=new Set(physical),weightedSet=new Set(weighted);
  for(const bone of bones.filter(b=>namedSecondary(b.name)||hangingChain(b,names,children,weightedSet,sourceRoles))){
    const branch=new Set<string>(),pending=[bone.name];let invalid=false;
    while(pending.length){const n=pending.pop()!;if(branch.has(n)||branch.size>=512){invalid=true;break;}branch.add(n);pending.push(...(children.get(n)??[]));}
    if(invalid||[...branch].some(n=>verified.has(n)||Object.values(sourceRoles).includes(n)||Boolean(names.get(n)?.physics?.dynamic_bodies?.length||names.get(n)?.physics?.kinematic_bodies?.length)))continue;
    const used=weighted.filter(n=>branch.has(n)).map(n=>names.get(n)!);
    if(used.length>=2&&used.slice(1).some(b=>b.position.reduce((s,v,k)=>s+(v-used[0].position[k])**2,0)>1e-10))physical.add(bone.name);
  }
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
