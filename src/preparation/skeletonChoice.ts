/** Files without their skeleton (`missing_skeleton` of the inspection) that the user chose to prepare
 * with automatic weights, per project, for this session only (never saved). Simple mode's automatic
 * preparation stops before converting such a file until the choice is made; a changed file (another
 * `source_sha256`) or another project asks again. */
const continued=new Set<string>();
const keyOf=(project:string,source:string)=>`${project}\n${source}`;
export const continuedWithoutSkeleton=(project:string,source:string)=>continued.has(keyOf(project,source));
export function continueWithoutSkeleton(project:string,source:string){continued.add(keyOf(project,source));}
/** Tests start from no choices. */
export function forgetSkeletonChoices(){continued.clear();}
