import type { Dispatch, SetStateAction } from 'react';
import type { Project } from '../../models';
import type { PreparationAnalysis, PreparationOptions, Role } from '../models';
import { roleLabel } from '../mapping';
import { MissingSkeletonNotice } from './MissingSkeletonNotice';
import { PrepCard } from './PrepCard';
/** The body-region filter of Body roles (also the labels the Joint check view shows). */
export const ROLE_REGIONS=[['all','All joints'],['torso','Torso & head'],['arms','Arms & hands'],['legs','Legs & feet']] as const;
/** The row of a role (a label click scrolls to it). */
export const roleRowId=(role:Role)=>`prep-role-${role}`;
/** The Body roles tab: the region filter as chips, one row per joint with the bone in your model
 * (or its X / Y / Z point for an unrigged model) and the bone on the hero. Every change goes
 * through `setRole` / `changeOptions` as before. */
export function RolesPanel({warnings,roleGroup,setRoleGroup,shownRoles,selectedRole,selectRole,project,options,busy,changeOptions,setRole,analysis,boneLabel}:{warnings:string[];roleGroup:string;setRoleGroup:Dispatch<SetStateAction<string>>;shownRoles:Role[];selectedRole:Role;selectRole:(role:Role)=>void;project:Project;options:PreparationOptions;busy:boolean;changeOptions:(values:Partial<PreparationOptions>)=>void;setRole:(side:'source'|'target',role:Role,name:string)=>void;analysis:PreparationAnalysis;boneLabel:(name:string)=>string}){
  const unrigged=project.model_input_mode==='unrigged_humanoid';
  return (
    <PrepCard title="Body roles" className="prep-roles-card" sub={unrigged?'Where each body joint is in your model (X, Y and Z). Select a joint to see it in the Joint check view.':'Which bone of your model is each body joint. Select a joint to see it in the Joint check view; changing either bone also updates that joint’s match.'}>
      {unrigged&&<MissingSkeletonNotice inspection={analysis.analysis}/>}
      {warnings.length>0&&<ul className="prep-alert" role="alert">{warnings.map(w=><li key={w}>{w}</li>)}</ul>}
      <div className="prep-chip-row" role="group" aria-label="Show body region">{ROLE_REGIONS.map(([value,label])=><button key={value} type="button" className="prep-chip" aria-pressed={roleGroup===value} onClick={()=>setRoleGroup(value)}>{label}</button>)}</div>
      <table className="prep-role-table"><thead><tr><th scope="col">Joint</th><th scope="col">{unrigged?'Point in your model (X, Y, Z)':'Bone in your model'}</th><th scope="col">Bone on the hero</th></tr></thead><tbody>{shownRoles.map(role=><tr key={role} id={roleRowId(role)} className={selectedRole===role?'selected-role':''} onClick={()=>{if(selectedRole!==role)selectRole(role);}}>
        <td><button type="button" className="prep-role-name" aria-pressed={selectedRole===role} onClick={event=>{event.stopPropagation();selectRole(role);}}>{roleLabel(role)}</button></td>
        <td>{unrigged?<div className="vector-fields">{[0,1,2].map(index=><input key={index} type="number" step="0.01" aria-label={`${role} landmark ${'XYZ'[index]}`} value={options.landmarks?.[role]?.[index]??''} disabled={busy} onChange={event=>{if(!Number.isFinite(event.target.valueAsNumber))return;const point=[...(options.landmarks?.[role]??[0,0,0])];point[index]=event.target.valueAsNumber;changeOptions({landmarks:{...options.landmarks,[role]:point},reviewed:false});}}/>)}</div>:<select aria-label={`Source ${role}`} title={options.source_roles?.[role]?boneLabel(options.source_roles[role]):undefined} disabled={busy} value={options.source_roles?.[role]??''} onChange={event=>setRole('source',role,event.target.value)}><option value="">Choose a source bone…</option>{analysis.analysis.bones.map(bone=><option key={bone.name} value={bone.name}>{boneLabel(bone.name)}</option>)}</select>}</td>
        <td><select aria-label={`Target ${role}`} title={options.target_roles?.[role]||undefined} disabled={busy} value={options.target_roles?.[role]??''} onChange={event=>setRole('target',role,event.target.value)}><option value="">Choose a target bone…</option>{analysis.target.map(bone=><option key={bone.name} value={bone.name}>{bone.name}</option>)}</select></td>
      </tr>)}</tbody></table>
    </PrepCard>
  );
}
