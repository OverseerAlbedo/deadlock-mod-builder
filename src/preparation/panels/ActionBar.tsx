import type { Dispatch, SetStateAction } from 'react';
import type { PreparationResult, ReviewIssue } from '../models';
import type { PreviewScene } from '../../preview/packet';
import { StudioActionBar } from '../../studio/Studio';
import { PrepIcon, type PrepPanel } from '../PreparationChrome';
import { BUTTON_NAMES, TAB_NAMES } from '../pageNames';
import type { PrepView } from '../preparationShared';
/** The bottom bar of the frame, in three states: the prepared model is in use, a prepared model
 * is available, or ready to prepare (something needs a choice after a check that found issues). */
export function ActionBar({reviewed,result,usedResult,issues,setView,jump,preparedPreview,previewPrepared,next,busy,accept,setPanel,review,prepareModel}:{reviewed:boolean;result:PreparationResult|null;usedResult:boolean;issues:ReviewIssue[]|null;setView:Dispatch<SetStateAction<PrepView>>;jump:(id:string)=>void;preparedPreview:PreviewScene|null;previewPrepared:()=>Promise<void>;next?:()=>void;busy:boolean;accept:()=>Promise<void>;setPanel:Dispatch<SetStateAction<PrepPanel|null>>;review:()=>Promise<void>;prepareModel:()=>Promise<void>}){
  const tone=usedResult||result||reviewed?'ok':issues?.length?'attention':'quiet';
  const title=usedResult?'Your prepared model is in use':result?'Prepared model available':issues?.length?'Something needs your choice':'Ready to prepare';
  const text=usedResult?'Continue with materials and weapons, or preview the result below.':result?'Preview it, then use it in your project.':issues?.length?'Fix the items listed above, then click Prepare model again.':'Prepare model checks your choices, converts the model and uses it in one click.';
  return (
    <StudioActionBar tone={tone} title={title} text={text}>{usedResult?<><button onClick={()=>{setView('result');jump('prep-review-workspace');if(!preparedPreview)void previewPrepared();}}>Preview the result</button>{next&&<button className="prep-primary" disabled={busy} onClick={next}>Continue to Appearance & weapons<PrepIcon name="arrow" size={18}/></button>}</>:result?<><button onClick={()=>jump('prep-output')}>View prepared output</button><button className="prep-primary" disabled={busy} onClick={()=>void accept()}>Use this model<PrepIcon name="arrow" size={18}/></button></>:<><button className="prep-quiet" onClick={()=>{setPanel('review');jump('prep-review-workspace');}}>{TAB_NAMES.review}</button><button className="prep-secondary" disabled={busy} onClick={()=>void review()}><PrepIcon name="review" size={17}/>{BUTTON_NAMES.checkForProblems}</button><button className="prep-primary" disabled={busy} onClick={()=>void prepareModel()}>{BUTTON_NAMES.prepareModel}<PrepIcon name="arrow" size={18}/></button></>}</StudioActionBar>
  );
}
