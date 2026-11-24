import type { PreparationAnalysis, PreparationOptions } from '../models';
import { PhysicsBoneChoices } from '../PhysicsBoneChoices';
import { breastBones, breastMotionOptions, buttBones, buttMotionOptions, hangsFromGun, type PhysicsGroup } from '../secondary-bones';
/** The Moving parts tab. `marked`: the group a label on the character pointed at. */
export function MovingPartsPanel({physics,analysis,options,changeOptions,busy,inspect,marked}:{physics:PhysicsGroup[];analysis:PreparationAnalysis;options:PreparationOptions;changeOptions:(values:Partial<PreparationOptions>)=>void;busy:boolean;inspect:(auto?:boolean)=>Promise<void>;marked?:string}){
  return (
    <PhysicsBoneChoices groups={physics} gunParts={physics.filter(g=>hangsFromGun(g.root,analysis.analysis.bones,options.source_roles??{})).map(g=>g.root)} options={options} change={changeOptions} busy={busy} marked={marked} breasts={{count:breastBones(analysis.analysis.bones,analysis.analysis.weighted_bones,options.source_roles??{}).length,toggle:enabled=>changeOptions(breastMotionOptions(analysis.analysis.bones,analysis.analysis.weighted_bones,options,enabled)),added:Boolean(analysis.breast_generation),result:analysis.analysis.breast_bones,reinspect:()=>void inspect()}} butts={{count:buttBones(analysis.analysis.bones,analysis.analysis.weighted_bones,options.source_roles??{}).length,toggle:enabled=>changeOptions(buttMotionOptions(analysis.analysis.bones,analysis.analysis.weighted_bones,options,enabled))}}/>
  );
}
