// i18n-area: prepare
import { useMemo, useState } from 'react';
import { m, t, tp, useT } from '../../i18n';
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
  const t=useT();
  const [open,setOpen]=useState(false);
  const text=useMemo(()=>open?JSON.stringify(value,null,2):'',[open,value]);
  return <details className="technical" onToggle={event=>setOpen((event.currentTarget as HTMLDetailsElement).open)}><summary>{t('Native source inspection')}</summary>{open&&<pre>{text}</pre>}</details>;
}
/** The preparation modes the menu offers (weapon only for a gun project); the names stay English (translated where drawn). */
export function modeChoices(project:Project):[string,string][]{
  return [["rigged_humanoid",m("Rigged humanoid")],["unrigged_humanoid",m("Unrigged humanoid")],["prepared",m("Prepared source")],...(project.model_input_mode==='weapon'||weaponLike(project.original_model)?[["weapon",m("Weapon model (gun only)")] as [string,string]]:[])];
}
/** The count of notes the Notes menu holds (import notes and suggestion checks). */
export const noteCount=(analysis:PreparationAnalysis)=>(analysis.analysis.warnings?.length??0)+(analysis.suggestions.assistance?.warnings.length??0);
/** What the check found, in one sentence (was the "Your model is checked" summary), in the interface language. */
export function foundSummary(analysis:PreparationAnalysis,unmapped:number,prepared=false){
  if(analysis.analysis.missing_skeleton)return t('This file has no skeleton. Read the note below before preparing.');
  if(unmapped)return tp(unmapped,{one:'{count} joint still need a match below. Everything else was matched automatically.',other:'{count} joints still need a match below. Everything else was matched automatically.'});
  return prepared?t('Every joint was matched automatically. To change something, use the tabs, then {check} or {again} on {tab}.',{check:t(BUTTON_NAMES.checkForProblems),again:t(BUTTON_NAMES.prepareAgain),tab:t(TAB_NAMES.review)})
    :t('Every joint was matched automatically. Click {button} below, or look through the tabs first.',{button:t(BUTTON_NAMES.prepareModel)});
}
/** The Preparation mode menu: the select, as it was in "What we found in your model". */
export function ModePanel({project,busy,invalidate,update,options}:{project:Project;busy:boolean;invalidate:()=>void;update:(project:Project)=>void;options:PreparationOptions}){
  const t=useT();
  return <div className="prep-mode-panel"><SelectField label={t('Preparation mode')} value={project.model_input_mode} values={modeChoices(project).map(([value,label])=>[value,t(label)] as const)} onChange={model_input_mode=>{invalidate();update({...project,model_input_mode,preparation_options:asOptions({...options,reviewed:false})});}} disabled={busy}/>
    <p className="hint">{t('Each check sets this from your model; change it only when the check chose the wrong kind of model.')}</p></div>;
}
/** What the check found that needs a look, shown openly under the top bar (never in a menu): the
 * summary sentence when joints are unmatched or the file has no skeleton, and the missing-skeleton
 * note. The sentence for a model where everything matched is in the Notes menu. */
export function FoundNotices({analysis,unmapped,choice}:{analysis:PreparationAnalysis;unmapped:number;
  /** Simple mode's automatic preparation stopped before converting a file without its skeleton. */choice?:SkeletonChoice}){
  const t=useT();
  const attention=Boolean(analysis.analysis.missing_skeleton)||unmapped>0;
  if(!attention)return null;
  return <section className="prep-found" aria-label={t('What we found in your model')}>
    <p className="prep-found-line"><PrepIcon name="info" size={16}/>{foundSummary(analysis,unmapped)}</p>
    <MissingSkeletonNotice inspection={analysis.analysis} choice={choice}/>
  </section>;
}
/** The Notes menu: the summary, the inspection details and notes, Re-detect body mapping and the
 * mapping-preset note. `details` (Expert, or matching shown) gates the details and Re-detect, as
 * in the card these came from. */
export function NotesPanel({details,busy,suggestions,unmapped,analysis,prepared=false}:{details:boolean;busy:boolean;suggestions:(resetRoles?:boolean)=>Promise<void>;unmapped:number;analysis:PreparationAnalysis;
  /** A preparation result is shown: its buttons are Check for problems and Prepare again. */prepared?:boolean}){
  const t=useT();
  const notes=noteCount(analysis);
  return <div className="prep-notes-panel">
    <p className="prep-notes-summary"><PrepIcon name={analysis.analysis.missing_skeleton||unmapped?'info':'check'} size={16}/>{foundSummary(analysis,unmapped,prepared)}</p>
    {analysis.suggestions.assistance?.preset&&<p className="prep-preset-status" role="status"><PrepIcon name="check" size={15}/>{analysis.suggestions.assistance.preset}</p>}
    {details&&<><details className="prep-inspection-details"><summary><PrepIcon name="info" size={16}/>{t('Inspection details & notes')}{notes>0&&<span className="prep-count">{notes}</span>}</summary>
      <p>{t('Auto-map uses names, hierarchy, sides, joint positions and skin-region evidence. Existing direct choices are retained. Review the suggested assignments and any remaining groups.')}</p>
      {(analysis.analysis.warnings?.length??0)>0&&<ul aria-label={t('Source import notes')}>{analysis.analysis.warnings!.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}
      {(analysis.suggestions.assistance?.warnings.length??0)>0&&<ul aria-label={t('Suggestion checks')}>{analysis.suggestions.assistance!.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}
      <RawInspection value={analysis.analysis}/>
    </details>
    <div className="row"><button disabled={busy} onClick={()=>void suggestions()}><PrepIcon name="refresh" size={16}/>{t('Re-detect body mapping')}</button></div></>}
  </div>;
}
