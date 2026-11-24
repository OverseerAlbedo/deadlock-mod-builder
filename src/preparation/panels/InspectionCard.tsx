import { useMemo, useState } from 'react';
import type { Project } from '../../models';
import type { PreparationAnalysis, PreparationOptions } from '../models';
import { weaponLike } from '../../components/WeaponModels';
import { SelectField } from '../../components/CustomizationPages';
import { PrepIcon } from '../PreparationChrome';
import { asOptions } from '../preparationShared';
import { MissingSkeletonNotice, type SkeletonChoice } from './MissingSkeletonNotice';
import { BUTTON_NAMES, TAB_NAMES } from '../pageNames';
/** The raw inspection, serialized only once the panel is opened (it can be several MB). */
function RawInspection({value}:{value:unknown}){
  const [open,setOpen]=useState(false);
  const text=useMemo(()=>open?JSON.stringify(value,null,2):'',[open,value]);
  return <details className="technical" onToggle={event=>setOpen((event.currentTarget as HTMLDetailsElement).open)}><summary>Native source inspection</summary>{open&&<pre>{text}</pre>}</details>;
}
/** The preparation modes the menu offers (weapon only for a gun project). */
export function modeChoices(project:Project):[string,string][]{
  return [["rigged_humanoid","Rigged humanoid"],["unrigged_humanoid","Unrigged humanoid"],["prepared","Prepared source"],...(project.model_input_mode==='weapon'||weaponLike(project.original_model)?[["weapon","Weapon model (gun only)"] as [string,string]]:[])];
}
/** The count of notes the Notes menu holds (import notes and suggestion checks). */
export const noteCount=(analysis:PreparationAnalysis)=>(analysis.analysis.warnings?.length??0)+(analysis.suggestions.assistance?.warnings.length??0);
/** What the check found, in one sentence (was the "Your model is checked" summary). */
export function foundSummary(analysis:PreparationAnalysis,unmapped:number,prepared=false){
  return analysis.analysis.missing_skeleton?'This file has no skeleton. Read the note below before preparing.':unmapped?`${unmapped} joint${unmapped===1?'':'s'} still need a match below. Everything else was matched automatically.`:prepared?`Every joint was matched automatically. To change something, use the tabs, then ${BUTTON_NAMES.checkForProblems} or ${BUTTON_NAMES.prepareAgain} on ${TAB_NAMES.review}.`:`Every joint was matched automatically. Click ${BUTTON_NAMES.prepareModel} below, or look through the tabs first.`;
}
/** The Preparation mode menu: the select, as it was in "What we found in your model". */
export function ModePanel({project,busy,invalidate,update,options}:{project:Project;busy:boolean;invalidate:()=>void;update:(project:Project)=>void;options:PreparationOptions}){
  return <div className="prep-mode-panel"><SelectField label="Preparation mode" value={project.model_input_mode} values={modeChoices(project)} onChange={model_input_mode=>{invalidate();update({...project,model_input_mode,preparation_options:asOptions({...options,reviewed:false})});}} disabled={busy}/>
    <p className="hint">Each check sets this from your model; change it only when the check chose the wrong kind of model.</p></div>;
}
/** What the check found that needs a look, shown openly under the top bar (never in a menu): the
 * summary sentence when joints are unmatched or the file has no skeleton, and the missing-skeleton
 * note. The sentence for a model where everything matched is in the Notes menu. */
export function FoundNotices({analysis,unmapped,choice}:{analysis:PreparationAnalysis;unmapped:number;
  /** Simple mode's automatic preparation stopped before converting a file without its skeleton. */choice?:SkeletonChoice}){
  const attention=Boolean(analysis.analysis.missing_skeleton)||unmapped>0;
  if(!attention)return null;
  return <section className="prep-found" aria-label="What we found in your model">
    <p className="prep-found-line"><PrepIcon name="info" size={16}/>{foundSummary(analysis,unmapped)}</p>
    <MissingSkeletonNotice inspection={analysis.analysis} choice={choice}/>
  </section>;
}
/** The Notes menu: the summary, the inspection details and notes, Re-detect body mapping and the
 * mapping-preset note. `details` (Expert, or matching shown) gates the details and Re-detect, as
 * in the card these came from. */
export function NotesPanel({details,busy,suggestions,unmapped,analysis,prepared=false}:{details:boolean;busy:boolean;suggestions:(resetRoles?:boolean)=>Promise<void>;unmapped:number;analysis:PreparationAnalysis;
  /** A preparation result is shown: its buttons are Check for problems and Prepare again. */prepared?:boolean}){
  const notes=noteCount(analysis);
  return <div className="prep-notes-panel">
    <p className="prep-notes-summary"><PrepIcon name={analysis.analysis.missing_skeleton||unmapped?'info':'check'} size={16}/>{foundSummary(analysis,unmapped,prepared)}</p>
    {analysis.suggestions.assistance?.preset&&<p className="prep-preset-status" role="status"><PrepIcon name="check" size={15}/>{analysis.suggestions.assistance.preset}</p>}
    {details&&<><details className="prep-inspection-details"><summary><PrepIcon name="info" size={16}/>Inspection details & notes{notes>0&&<span className="prep-count">{notes}</span>}</summary>
      <p>Auto-map uses names, hierarchy, sides, joint positions and skin-region evidence. Existing direct choices are retained. Review the suggested assignments and any remaining groups.</p>
      {(analysis.analysis.warnings?.length??0)>0&&<ul aria-label="Source import notes">{analysis.analysis.warnings!.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}
      {(analysis.suggestions.assistance?.warnings.length??0)>0&&<ul aria-label="Suggestion checks">{analysis.suggestions.assistance!.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}
      <RawInspection value={analysis.analysis}/>
    </details>
    <div className="row"><button disabled={busy} onClick={()=>void suggestions()}><PrepIcon name="refresh" size={16}/>Re-detect body mapping</button></div></>}
  </div>;
}
