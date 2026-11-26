// i18n-area: prepare
import { ROLES, type MappingEntry, type PreparationOptions, type Role, type Suggestions } from './models';
import { m, mc, t, tc } from '../i18n';

/** Each body role's name in English (the role id is the identifier; the name is only shown). */
const ROLE_NAMES:Record<Role,string>={pelvis:m('Pelvis / hips'),spine:m('Lower spine'),chest:m('Chest'),neck:m('Neck'),head:m('Head'),upperarm_L:m('Left upper arm'),forearm_L:m('Left forearm'),hand_L:m('Left hand'),thigh_L:m('Left thigh'),shin_L:m('Left shin'),foot_L:m('Left foot'),upperarm_R:m('Right upper arm'),forearm_R:m('Right forearm'),hand_R:m('Right hand'),thigh_R:m('Right thigh'),shin_R:m('Right shin'),foot_R:m('Right foot')};
/** A body role's shown name, in the interface language. */
export const roleLabel=(role:Role)=>t(ROLE_NAMES[role]);
/** A role named by its identifier (accessible names such as "Source upperarm_L"): English shows the
 * identifier itself; another language shows the role's name with the identifier after it, because
 * messages from the backend name roles by their identifier. */
const ROLE_IDS:Record<Role,string>={pelvis:mc('role id','pelvis'),spine:mc('role id','spine'),chest:mc('role id','chest'),neck:mc('role id','neck'),head:mc('role id','head'),upperarm_L:mc('role id','upperarm_L'),forearm_L:mc('role id','forearm_L'),hand_L:mc('role id','hand_L'),thigh_L:mc('role id','thigh_L'),shin_L:mc('role id','shin_L'),foot_L:mc('role id','foot_L'),upperarm_R:mc('role id','upperarm_R'),forearm_R:mc('role id','forearm_R'),hand_R:mc('role id','hand_R'),thigh_R:mc('role id','thigh_R'),shin_R:mc('role id','shin_R'),foot_R:mc('role id','foot_R')};
export const roleIdText=(role:Role)=>tc('role id',ROLE_IDS[role]);
/** The labels the backend gives a model's bones in the mapping assistance ("pelvis", "upperarm L",
 * "Left thumb"), shown the same way; any other label stays as it comes. */
const BONE_LABELS:Record<string,string>=Object.fromEntries([
  mc('bone label','pelvis'),mc('bone label','spine'),mc('bone label','chest'),mc('bone label','neck'),mc('bone label','head'),
  mc('bone label','upperarm L'),mc('bone label','forearm L'),mc('bone label','hand L'),mc('bone label','thigh L'),mc('bone label','shin L'),mc('bone label','foot L'),
  mc('bone label','upperarm R'),mc('bone label','forearm R'),mc('bone label','hand R'),mc('bone label','thigh R'),mc('bone label','shin R'),mc('bone label','foot R'),
  mc('bone label','Left thumb'),mc('bone label','Left index'),mc('bone label','Left middle'),mc('bone label','Left ring'),mc('bone label','Left pinky'),
  mc('bone label','Right thumb'),mc('bone label','Right index'),mc('bone label','Right middle'),mc('bone label','Right ring'),mc('bone label','Right pinky'),
].map(label=>[label,label]));
export const boneLabelText=(label:string)=>Object.prototype.hasOwnProperty.call(BONE_LABELS,label)?tc('bone label',BONE_LABELS[label]):label;

/** A landmark pair and its skin mapping must agree, including unweighted controls. */
export function changeRole(options:PreparationOptions,side:'source'|'target',role:Role,name:string):PreparationOptions {
  const source_roles={...options.source_roles},target_roles={...options.target_roles},mapping={...options.mapping};
  const previousSource=source_roles[role],previousTarget=target_roles[role];
  if(side==='source')source_roles[role]=name;else target_roles[role]=name;
  if(previousSource&&mapping[previousSource]===previousTarget)delete mapping[previousSource];
  for(const r of ROLES){const s=source_roles[r],target=target_roles[r];if(s&&target)mapping[s]=target;}
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
/** One bone given to two body roles on one side. */
export interface DoubleRole {side:'source'|'target';name:string;first:Role;second:Role}
export function doubleRoles(options:PreparationOptions):DoubleRole[] {
  const result:DoubleRole[]=[];
  for(const side of ['source','target'] as const){
    const roles=options[`${side}_roles`]??{},seen=new Map<string,Role>();
    for(const role of ROLES){const name=roles[role];if(!name)continue;const previous=seen.get(name);if(previous)result.push({side,name,first:previous,second:role});seen.set(name,role);}
  }
  return result;
}
/** The double roles as English sentences (counted and used as keys; `roleWarningText` shows them). */
export function roleWarnings(options:PreparationOptions):string[] {
  return doubleRoles(options).map(d=>`${d.side==='source'?'Source':'Target'} bone ${d.name} is assigned to both ${ROLE_NAMES[d.first]} and ${ROLE_NAMES[d.second]}.`);
}
/** A double role as shown, in the interface language. */
export function roleWarningText(d:DoubleRole):string {
  const values={name:d.name,first:roleLabel(d.first),second:roleLabel(d.second)};
  return d.side==='source'?t('Source bone {name} is assigned to both {first} and {second}.',values):t('Target bone {name} is assigned to both {first} and {second}.',values);
}
