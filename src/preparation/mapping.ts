import { ROLES, type MappingEntry, type PreparationOptions, type Role, type Suggestions } from './models';

export const roleLabel=(role:Role)=>({pelvis:'Pelvis / hips',spine:'Lower spine',chest:'Chest',neck:'Neck',head:'Head',upperarm_L:'Left upper arm',forearm_L:'Left forearm',hand_L:'Left hand',thigh_L:'Left thigh',shin_L:'Left shin',foot_L:'Left foot',upperarm_R:'Right upper arm',forearm_R:'Right forearm',hand_R:'Right hand',thigh_R:'Right thigh',shin_R:'Right shin',foot_R:'Right foot'}[role]);

/** A landmark pair and its skin mapping must agree, including unweighted controls. */
export function changeRole(options:PreparationOptions,side:'source'|'target',role:Role,name:string):PreparationOptions {
  const source_roles={...options.source_roles},target_roles={...options.target_roles},mapping={...options.mapping};
  const previousSource=source_roles[role],previousTarget=target_roles[role];
  if(side==='source')source_roles[role]=name;else target_roles[role]=name;
  if(previousSource&&mapping[previousSource]===previousTarget)delete mapping[previousSource];
  for(const r of ROLES){const s=source_roles[r],t=target_roles[r];if(s&&t)mapping[s]=t;}
  return {...options,source_roles,target_roles,mapping,reviewed:false,follow_motion_reviewed:false};
}
export function applySuggestions(options:PreparationOptions,suggestions:Suggestions):PreparationOptions {
  return {...options,mapping:suggestions.mapping,mapping_anchors:suggestions.assistance?.anchors??{},follow_motion_reviewed:false,source_roles:suggestions.source?.roles??{},target_roles:suggestions.target.roles,reviewed:false,influence_limit:options.influence_limit??4};
}
export function assignTargets(options:PreparationOptions,sources:string[],target:string):PreparationOptions {
  const mapping={...options.mapping},target_roles={...options.target_roles},mapping_anchors={...options.mapping_anchors};
  for(const source of sources){
    mapping[source]=target;
    delete mapping_anchors[source];
    for(const role of ROLES)if(options.source_roles?.[role]===source)target_roles[role]=target;
  }
  return {...options,mapping,mapping_anchors,target_roles,reviewed:false,follow_motion_reviewed:false};
}
/** Only apply the displayed proposals to unresolved bones. Existing choices win. */
export function applyFollowing(options:PreparationOptions,entries:MappingEntry[],preserveExistingTarget=false):PreparationOptions {
  const mapping={...options.mapping},mapping_anchors={...options.mapping_anchors};
  for(const entry of entries){const p=entry.follow;if(!p||mapping[p.anchor]!==p.target||(mapping[entry.source]&&!(preserveExistingTarget&&mapping[entry.source]===p.target)))continue;mapping[entry.source]=p.target;mapping_anchors[entry.source]=p.anchor;}
  return {...options,mapping,mapping_anchors,reviewed:false,follow_motion_reviewed:false};
}
/** Bulk actions span every page and filter, using the same proposals as each row. */
export function pendingFollowing(options:PreparationOptions,entries:MappingEntry[],target?:string):MappingEntry[] {
  return entries.filter(entry=>entry.follow&&!options.mapping?.[entry.source]&&options.mapping?.[entry.follow.anchor]===entry.follow.target&&(target===undefined||entry.follow.target===target));
}
export function roleWarnings(options:PreparationOptions):string[] {
  const result:string[]=[];
  for(const side of ['source','target'] as const){
    const roles=options[`${side}_roles`]??{},seen=new Map<string,Role>();
    for(const role of ROLES){const name=roles[role];if(!name)continue;const previous=seen.get(name);if(previous)result.push(`${side==='source'?'Source':'Target'} bone ${name} is assigned to both ${roleLabel(previous)} and ${roleLabel(role)}.`);seen.set(name,role);}
  }
  return result;
}
