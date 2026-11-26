// i18n-area: prepare
import type { Dispatch, SetStateAction } from 'react';
import { useT } from '../../i18n';
import type { PreparationResult, ReviewIssue } from '../models';
import type { PreviewScene } from '../../preview/packet';
import { StudioActionBar } from '../../studio/Studio';
import { PrepIcon, type PrepPanel } from '../PreparationChrome';
import { BUTTON_NAMES, TAB_NAMES } from '../pageNames';
import type { PrepView } from '../preparationShared';
/** The bottom bar of the frame, in three states: the prepared model is in use, a prepared model
 * is available, or ready to prepare (something needs a choice after a check that found issues). */
export function ActionBar({reviewed,result,usedResult,issues,setView,jump,preparedPreview,previewPrepared,next,busy,accept,setPanel,review,prepareModel}:{reviewed:boolean;result:PreparationResult|null;usedResult:boolean;issues:ReviewIssue[]|null;setView:Dispatch<SetStateAction<PrepView>>;jump:(id:string)=>void;preparedPreview:PreviewScene|null;previewPrepared:()=>Promise<void>;next?:()=>void;busy:boolean;accept:()=>Promise<void>;setPanel:Dispatch<SetStateAction<PrepPanel|null>>;review:()=>Promise<void>;prepareModel:()=>Promise<void>}){
  const t=useT();
  const tone=usedResult||result||reviewed?'ok':issues?.length?'attention':'quiet';
  const title=usedResult?t('Your prepared model is in use'):result?t('Prepared model available'):issues?.length?t('Something needs your choice'):t('Ready to prepare');
  const text=usedResult?t('Continue with materials and weapons, or preview the result below.'):result?t('Preview it, then use it in your project.'):issues?.length?t('Fix the items listed above, then click Prepare model again.'):t('Prepare model checks your choices, converts the model and uses it in one click.');
  return (
    <StudioActionBar tone={tone} title={title} text={text}>{usedResult?<><button onClick={()=>{setView('result');jump('prep-review-workspace');if(!preparedPreview)void previewPrepared();}}>{t('Preview the result')}</button>{next&&<button className="prep-primary" disabled={busy} onClick={next}>{t('Continue to Appearance & weapons')}<PrepIcon name="arrow" size={18}/></button>}</>:result?<><button onClick={()=>jump('prep-output')}>{t('View prepared output')}</button><button className="prep-primary" disabled={busy} onClick={()=>void accept()}>{t('Use this model')}<PrepIcon name="arrow" size={18}/></button></>:<><button className="prep-quiet" onClick={()=>{setPanel('review');jump('prep-review-workspace');}}>{t(TAB_NAMES.review)}</button><button className="prep-secondary" disabled={busy} onClick={()=>void review()}><PrepIcon name="review" size={17}/>{t(BUTTON_NAMES.checkForProblems)}</button><button className="prep-primary" disabled={busy} onClick={()=>void prepareModel()}>{t(BUTTON_NAMES.prepareModel)}<PrepIcon name="arrow" size={18}/></button></>}</StudioActionBar>
  );
}
