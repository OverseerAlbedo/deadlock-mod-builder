import type { Project } from '../../models';
import type { PreparationAnalysis, PreparationOptions } from '../models';
import { WeaponPanel } from '../WeaponPanel';
import { gunChoiceOptions } from '../secondary-bones';
import type { Operate } from '../preparationShared';
/** The Gun tab. */
export function GunPanel({project,analysis,options,changeOptions,busy,operate,inspect,projectPath,saveProject,update}:{project:Project;analysis:PreparationAnalysis;options:PreparationOptions;changeOptions:(values:Partial<PreparationOptions>)=>void;busy:boolean;operate:Operate;inspect:(auto?:boolean)=>Promise<void>;projectPath?:string|null;saveProject?:()=>Promise<string|undefined>;update:(project:Project)=>void}){
  return (
    <WeaponPanel project={project} analysis={analysis} options={options} change={values=>changeOptions(analysis?gunChoiceOptions(analysis.analysis.bones,analysis.analysis.weighted_bones,options,values):values)} busy={busy} operate={operate} reinspect={()=>void inspect()} separate={{projectPath,saveProject,updateProject:update}}/>
  );
}
