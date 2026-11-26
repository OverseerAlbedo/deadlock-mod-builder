// i18n-area: prepare
import type { Dispatch, SetStateAction } from 'react';
import { m, useT } from '../../i18n';
import type { Project } from '../../models';
import type { PreparationAnalysis, PreparationOptions, Role } from '../models';
import { doubleRoles, roleIdText, roleLabel, roleWarningText } from '../mapping';
import { MissingSkeletonNotice } from './MissingSkeletonNotice';
import { PrepCard } from './PrepCard';
/** The body-region filter of Body roles (also the labels the Joint check view shows). */
export const ROLE_REGIONS=[['all',m('All joints')],['torso',m('Torso & head')],['arms',m('Arms & hands')],['legs',m('Legs & feet')]] as const;
/** The row of a role (a label click scrolls to it). */
export const roleRowId=(role:Role)=>`prep-role-${role}`;
/** The Body roles tab: the region filter as chips, one row per joint with the bone in your model
 * (or its X / Y / Z point for an unrigged model) and the bone on the hero. Every change goes
 * through `setRole` / `changeOptions` as before. `warnings` (English) key the rows of the
 * double-role list; the rows show the same warnings in the interface language. */
export function RolesPanel({warnings,roleGroup,setRoleGroup,shownRoles,selectedRole,selectRole,project,options,busy,changeOptions,setRole,analysis,boneLabel}:{warnings:string[];roleGroup:string;setRoleGroup:Dispatch<SetStateAction<string>>;shownRoles:Role[];selectedRole:Role;selectRole:(role:Role)=>void;project:Project;options:PreparationOptions;busy:boolean;changeOptions:(values:Partial<PreparationOptions>)=>void;setRole:(side:'source'|'target',role:Role,name:string)=>void;analysis:PreparationAnalysis;boneLabel:(name:string)=>string}){
  const t=useT();
  const unrigged=project.model_input_mode==='unrigged_humanoid';
  const doubles=doubleRoles(options);
  return (
    <PrepCard title={t('Body roles')} className="prep-roles-card" sub={unrigged?t('Where each body joint is in your model (X, Y and Z). Select a joint to see it in the Joint check view.'):t('Which bone of your model is each body joint. Select a joint to see it in the Joint check view; changing either bone also updates that joint’s match.')}>
      {unrigged&&<MissingSkeletonNotice inspection={analysis.analysis}/>}
      {warnings.length>0&&<ul className="prep-alert" role="alert">{warnings.map((w,index)=><li key={w}>{doubles[index]?roleWarningText(doubles[index]):w}</li>)}</ul>}
      <div className="prep-chip-row" role="group" aria-label={t('Show body region')}>{ROLE_REGIONS.map(([value,label])=><button key={value} type="button" className="prep-chip" aria-pressed={roleGroup===value} onClick={()=>setRoleGroup(value)}>{t(label)}</button>)}</div>
      <table className="prep-role-table"><thead><tr><th scope="col">{t('Joint')}</th><th scope="col">{unrigged?t('Point in your model (X, Y, Z)'):t('Bone in your model')}</th><th scope="col">{t('Bone on the hero')}</th></tr></thead><tbody>{shownRoles.map(role=><tr key={role} id={roleRowId(role)} className={selectedRole===role?'selected-role':''} onClick={()=>{if(selectedRole!==role)selectRole(role);}}>
        <td><button type="button" className="prep-role-name" aria-pressed={selectedRole===role} onClick={event=>{event.stopPropagation();selectRole(role);}}>{roleLabel(role)}</button></td>
        <td>{unrigged?<div className="vector-fields">{[0,1,2].map(index=><input key={index} type="number" step="0.01" aria-label={t('{role} landmark {axis}',{role:roleIdText(role),axis:'XYZ'[index]})} value={options.landmarks?.[role]?.[index]??''} disabled={busy} onChange={event=>{if(!Number.isFinite(event.target.valueAsNumber))return;const point=[...(options.landmarks?.[role]??[0,0,0])];point[index]=event.target.valueAsNumber;changeOptions({landmarks:{...options.landmarks,[role]:point},reviewed:false});}}/>)}</div>:<select aria-label={t('Source {role}',{role:roleIdText(role)})} title={options.source_roles?.[role]?boneLabel(options.source_roles[role]):undefined} disabled={busy} value={options.source_roles?.[role]??''} onChange={event=>setRole('source',role,event.target.value)}><option value="">{t('Choose a source bone…')}</option>{analysis.analysis.bones.map(bone=><option key={bone.name} value={bone.name}>{boneLabel(bone.name)}</option>)}</select>}</td>
        <td><select aria-label={t('Target {role}',{role:roleIdText(role)})} title={options.target_roles?.[role]||undefined} disabled={busy} value={options.target_roles?.[role]??''} onChange={event=>setRole('target',role,event.target.value)}><option value="">{t('Choose a target bone…')}</option>{analysis.target.map(bone=><option key={bone.name} value={bone.name}>{bone.name}</option>)}</select></td>
      </tr>)}</tbody></table>
    </PrepCard>
  );
}
