// i18n-area: prepare
import { tp, useT } from '../../i18n';
import { nextSentence, type Inspection } from '../models';
import { PrepIcon } from '../PreparationChrome';
/** The choice Simple mode's automatic preparation stops for (FoundNotices only): continue with
 * automatic weights, which converts as before, or choose another model file. */
export interface SkeletonChoice {busy:boolean;proceed:()=>void;another:()=>void}
/** The file's meshes carry bone weights but the skeleton was not saved with them
 * (`missing_skeleton` of the inspection). Shown openly under the top bar (FoundNotices) and on Body roles. */
export function MissingSkeletonNotice({inspection,choice}:{inspection:Inspection;choice?:SkeletonChoice}){
  const t=useT();
  const gap=inspection.missing_skeleton;
  if(!gap)return null;
  const several=gap.meshes.length>1,count=gap.groups.toLocaleString();
  // Without a skeleton the joints are estimated from the body's shape (estimate_landmarks); preparing repeats that estimate.
  const humanoid=inspection.humanoid as {errors?:string[]};
  const noShape=Array.isArray(humanoid?.errors)&&humanoid.errors.length>0;
  const estimate=t('So the joints cannot be matched to the hero’s from your model’s own bones. The app treats the file as a model without a skeleton: it leaves these weights out and makes new ones from joint positions it estimates from the body’s shape.');
  return <div className="prep-missing-skeleton" role="note" aria-label={t('No skeleton in this file')}><PrepIcon name="info" size={18}/><div>
    <strong>{t('This file has no skeleton')}</strong>
    <p>{several?tp(gap.groups,{one:'This file’s meshes have weights for {count} bone but no skeleton; the skeleton (armature) was not saved with them.',other:'This file’s meshes have weights for {count} bones but no skeleton; the skeleton (armature) was not saved with them.'},{count}):tp(gap.groups,{one:'This file’s mesh has weights for {count} bone but no skeleton; the skeleton (armature) was not saved with it.',other:'This file’s mesh has weights for {count} bones but no skeleton; the skeleton (armature) was not saved with it.'},{count})}</p>
    <p>{noShape?nextSentence(estimate,t('For this model those positions could not be found, so it cannot be prepared this way.')):estimate}</p>
    <p>{t('Choose a file that includes the skeleton, for example an FBX or the original model this mesh came from. Or add the skeleton back in Blender and save the file again.')}</p>
    {choice&&<><p>{t('Preparation stopped here, before converting your model, so you can choose.')}</p>
      <div className="row"><button type="button" className="prep-primary" disabled={choice.busy} onClick={choice.proceed}>{t('Continue with automatic weights')}</button><button type="button" className="prep-secondary" disabled={choice.busy} onClick={choice.another}>{t('Choose another model file')}</button></div></>}
  </div></div>;
}
