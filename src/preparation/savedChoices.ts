import { ROLES, type Bone, type PreparationOptions, type Suggestions } from './models';
import { applySuggestions } from './mapping';
import { automaticPhysicsOptions, physicsGroups, type PhysicsGroup } from './secondary-bones';

/** Saved choices that the body roles decide, kept consistent with them. Which groups exist is
 * decided by the moving-part detectors (secondary-bones.ts); this module only changes what is
 * saved. */

/** Saved moving-part roots that the current source roles make part of the body: the root is a
 * role bone, or a role bone hangs below it (it lies on a chain between role bones). Such a root
 * can never be a detected group. Roots that are not bones of this rig (another source file) are
 * not touched. */
export function bodyRoots(bones:Bone[],options:PreparationOptions):string[]{
  const roles=new Set(Object.values(options.source_roles??{}).filter((n):n is string=>Boolean(n)));
  const names=new Set(bones.map(b=>b.name)),parent=new Map(bones.map(b=>[b.name,b.parent??null]));
  // Every role bone and every bone above one.
  const body=new Set<string>();
  for(const role of roles){let cursor:string|null|undefined=role;const seen=new Set<string>();while(cursor&&!seen.has(cursor)){seen.add(cursor);body.add(cursor);cursor=parent.get(cursor);}}
  return (options.physics_roots??[]).filter(root=>names.has(root)&&body.has(root));
}

/** Saved roots inside another detected group that is kept (for example a part that hung from a
 * chain the roles took for a leg, once that chain is a moving part of its own): their bones move
 * with that group, and the root can never be a group of its own. */
export function absorbedRoots(bones:Bone[],weighted:string[],options:PreparationOptions):string[]{
  const saved=options.physics_roots??[],kept=options.preserve_physics_bones?physicsGroups(bones,weighted,options).filter(g=>saved.includes(g.root)):[];
  return saved.filter(root=>kept.some(g=>g.root!==root&&g.source_bones.includes(root)));
}
/** Saved roots the current roles rule out: part of the body, or inside another kept group. */
export const rootsRuledOut=(bones:Bone[],weighted:string[],options:PreparationOptions)=>[...new Set([...bodyRoots(bones,options),...absorbedRoots(bones,weighted,options)])];

function without(options:PreparationOptions,dropped:Set<string>):PreparationOptions{
  if(!dropped.size)return options;
  const keep=(root:string)=>!dropped.has(root);
  const next={...options,physics_roots:(options.physics_roots??[]).filter(keep)};
  if(options.physics_kinds)next.physics_kinds=Object.fromEntries(Object.entries(options.physics_kinds).filter(([root])=>keep(root)));
  if(Array.isArray(options.physics_offered))next.physics_offered=options.physics_offered.filter(root=>typeof root!=='string'||keep(root));
  return next;
}
/** Drops saved roots that became part of the body (with their part types and the record that
 * they were offered, so they count as new if a later role change frees them again), adds newly
 * detected groups under the rule every inspection uses (`automaticPhysicsOptions`: groups you
 * were shown and left off stay off), then drops saved roots now inside another kept group.
 * Every role change goes through this. */
export function physicsForRoles(bones:Bone[],weighted:string[],options:PreparationOptions):PreparationOptions{
  const next=automaticPhysicsOptions(bones,weighted,without(options,new Set(bodyRoots(bones,options))));
  return without(next,new Set(absorbedRoots(bones,weighted,next)));
}

const sameRoles=(a:PreparationOptions,b:PreparationOptions)=>ROLES.every(role=>(a.source_roles?.[role]??'')===(b.source_roles?.[role]??'')&&(a.target_roles?.[role]??'')===(b.target_roles?.[role]??''));
/** An option change: when it changed a body role, the saved moving parts follow the new roles. */
export function withRoleChange(bones:Bone[],weighted:string[],previous:PreparationOptions,next:PreparationOptions):PreparationOptions{
  return sameRoles(previous,next)?next:physicsForRoles(bones,weighted,next);
}

/** The roles in the options differ from the ones the shown suggestions were made with. */
export function rolesDiffer(options:PreparationOptions,suggestions:Suggestions):boolean{
  return ROLES.some(role=>options.source_roles?.[role]!==suggestions.source?.roles[role]||options.target_roles?.[role]!==suggestions.target.roles[role]);
}

/** Refresh suggestions using my roles: the backend's new mapping and body-following matches,
 * with the user's own roles kept as they are (never the reply's) and every role bone mapped to
 * its role's target, as `changeRole` keeps them. The returned suggestions carry the same roles,
 * so the page no longer reports that the roles changed. */
export function refreshWithRoles(options:PreparationOptions,reply:Suggestions):{options:PreparationOptions;suggestions:Suggestions}{
  const source_roles={...options.source_roles},target_roles={...options.target_roles};
  const next=applySuggestions(options,reply);
  const mapping={...next.mapping},mapping_anchors={...next.mapping_anchors};
  for(const role of ROLES){const s=source_roles[role],t=target_roles[role];if(s&&t){mapping[s]=t;delete mapping_anchors[s];}}
  const suggestions:Suggestions={...reply,mapping,target:{...reply.target,roles:target_roles},...(reply.source?{source:{...reply.source,roles:source_roles}}:{}),...(reply.assistance?{assistance:{...reply.assistance,anchors:mapping_anchors}}:{})};
  return {options:{...next,source_roles,target_roles,mapping,mapping_anchors},suggestions};
}

/** The same saved choices, ignoring key order and the two review confirmations. */
export function sameChoices(a:PreparationOptions,b:PreparationOptions):boolean{
  const canonical=(value:unknown):unknown=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).filter(([,v])=>v!==undefined).sort(([x],[y])=>x<y?-1:x>y?1:0).map(([k,v])=>[k,canonical(v)])):value;
  const plain=(o:PreparationOptions)=>JSON.stringify(canonical({...o,reviewed:undefined,follow_motion_reviewed:undefined}));
  return plain(a)===plain(b);
}
/** Use the detected groups: the saved groups that are still detected stay chosen, the others
 * are dropped; when none is left, every detected group is kept. */
export function keepDetectedGroups(groups:PhysicsGroup[],options:PreparationOptions):Partial<PreparationOptions>{
  const detected=groups.map(g=>g.root),kept=(options.physics_roots??[]).filter(root=>detected.includes(root));
  return {physics_roots:kept.length?kept:detected,physics_offered:detected,reviewed:false};
}
/** Saved roots that are not detected groups now. */
export const undetectedRoots=(groups:PhysicsGroup[],options:PreparationOptions)=>(options.physics_roots??[]).filter(root=>!groups.some(g=>g.root===root));