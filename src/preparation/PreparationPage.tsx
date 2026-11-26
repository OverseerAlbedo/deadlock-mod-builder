// i18n-area: prepare
import { useEffect, useMemo, useRef, useState } from 'react';
import { m, tp, tpc, useLanguage, useT } from '../i18n';
import { shownHero } from '../i18n/gameNames';
import { openFor } from '../dialogs';
import { api, basename } from '../api';
import type { DefaultFit, Project } from '../models';
import { idOf, useAutoLoad } from '../preview/autoLoad';
import { FLOOR_CHOICES, LEGS_ON_HERO_JOINTS, legFitLine, ROLES, type IssueKind, type MappingEntry, type PreparationAnalysis, type PreparationOptions, type PreparationResult, type ReviewIssue, type Role, type Suggestions, type Vec3 } from './models';
import { applySuggestions, boneLabelText, changeRole, pendingFollowing, roleLabel, roleWarnings } from './mapping';
import type { ModelLabelSet } from '../studio/ModelLabels';
import type { LitBone } from './MappingViewport';
import { roleRowId } from './panels/RolesPanel';
import { matchRowId } from './panels/MappingPanel';
import { groupRowId } from './PhysicsBoneChoices';
import { samePath, weaponDefaults } from './WeaponPanel';
import { partSprings } from './springs';
import { physicsGroups, keptPhysicsBones } from './secondary-bones';
import { keepDetectedGroups, physicsForRoles, refreshWithRoles, rolesDiffer, rootsRuledOut, sameChoices, withRoleChange } from './savedChoices';
import { SOURCE_EXTENSIONS } from './sourceFormats';
import { decodeGltfPreview, type GltfPreview } from '../preview/gltf-packet';
import { decodePreview, type PreviewScene } from '../preview/packet';
import { PrepIcon, type PrepPanel } from './PreparationChrome';
import { TAB_NAMES } from './pageNames';
import { asOptions, type Operate } from './preparationShared';
import { SceneObjects } from './SceneObjects';
import { undetectedRoots } from './savedChoices';
import { SourceFields, SourcePreview } from './panels/SourceCard';
import { FoundNotices, ModePanel, NotesPanel } from './panels/InspectionCard';
import { continueWithoutSkeleton, continuedWithoutSkeleton } from './skeletonChoice';
import { PrepTopBar, type PrepMenu } from './panels/PrepTopBar';
import { RolesPanel } from './panels/RolesPanel';
import { MovingPartsPanel } from './panels/MovingPartsPanel';
import { MappingPanel } from './panels/MappingPanel';
import { GunPanel } from './panels/GunPanel';
import { FitPanel, type FitGroup } from './panels/FitPanel';
import { ViewColumn } from './panels/ViewColumn';
import { CheckResults, issueSection } from './panels/CheckResults';
import { PreparedOutput } from './panels/PreparedOutput';
import { ActionBar } from './panels/ActionBar';
import { StudioFrame, StudioTabPanel, StudioTabs, type StudioTab, type StudioTone } from '../studio/Studio';
import { useStudioBarMenus } from '../studio/StudioBarMenu';
import './preparation.css';
import './prepStudio.css';

/** The event that brings a model from step 1 (or a drop): the path, and whether to prepare it automatically. */
export type ModelSourceEvent=string|{path:string;auto?:boolean};
const keyOf=(project:Project,analysis:PreparationAnalysis|null)=>JSON.stringify([project.original_model,project.model_input_mode,project.preparation_options,analysis?.source_sha256]);

/** Whether a project has preparation choices of its own: a check or a preparation was made
 * (`preparation_source`, `preparation_report`, `preparation_blend`) or a mapping or roles are
 * saved. Only a project without them takes Settings → Default fit for new models; any other
 * project keeps its saved fit, and an unset one keeps meaning Match hero proportions. */
export function hasPreparationChoices(project:Project):boolean{
  const options=project.preparation_options as PreparationOptions;
  return Boolean(project.preparation_source||project.preparation_report||project.preparation_blend||Object.keys(options.mapping??{}).length||Object.keys(options.source_roles??{}).length||Object.keys(options.target_roles??{}).length);
}

export function PreparationPage({project,update,busy,operate,projectPath,saveProject,next,expert=true,defaultFit='preserve_proportions',active=true,autoPreviews=false}:{project:Project;update:(project:Project)=>void;busy:boolean;operate:Operate;
  /** Expert shows every table and tool; Simple shows joint and bone matching only when something needs a choice. */expert?:boolean;
  /** Settings → Default fit for new models: written into a project checked for the first time (`hasPreparationChoices`). */defaultFit?:DefaultFit;
  /** The page is shown (App keeps it mounted while hidden). */active?:boolean;
  /** Settings → Load 3D previews automatically: the shown 3D view loads by itself after a check. */autoPreviews?:boolean;
  /** Where the project is saved (a separate gun model's project is saved next to it). */projectPath?:string|null;
  /** Saves the project when it was never saved; resolves to its path. */saveProject?:()=>Promise<string|undefined>;
  /** Continue to the next step (Appearance & weapons) once the model is prepared. */next?:()=>void}){
  const t=useT(),language=useLanguage();
  // Long operations update the project more than once: always use the newest updater.
  const updateRef=useRef(update);updateRef.current=update;
  const [preview,setPreview]=useState<GltfPreview|null>(null),[mappingPreview,setMappingPreview]=useState<GltfPreview|null>(null),[preparedPreview,setPreparedPreview]=useState<PreviewScene|null>(null);
  const [source,setSource]=useState(project.preparation_source),[rotation,setRotation]=useState<Vec3>([0,0,0]);
  const [analysis,setAnalysis]=useState<PreparationAnalysis|null>(null),[issues,setIssues]=useState<ReviewIssue[]|null>(null),[reviewKey,setReviewKey]=useState(''),[result,setResult]=useState<PreparationResult|null>(null);
  const [selectedRole,setSelectedRole]=useState<Role>('pelvis'),[selectedBone,setSelectedBone]=useState(''),[filter,setFilter]=useState('unmapped'),[group,setGroup]=useState('all'),[search,setSearch]=useState(''),[page,setPage]=useState(0);
  const [selectedBones,setSelectedBones]=useState<Set<string>>(new Set()),[batchTarget,setBatchTarget]=useState('');
  const [savedPreset,setSavedPreset]=useState('');
  const generation=useRef(0);
  /** Counts checks and reopened preparations: what Load 3D previews automatically loads once per. */
  const [inspections,setInspections]=useState(0);
  useEffect(()=>{generation.current++;setAnalysis(null);setMappingPreview(null);setPreparedPreview(null);setIssues(null);setResult(null);},[project.original_model]);
  useEffect(()=>()=>{generation.current++;},[]);
  const options=project.preparation_options as PreparationOptions;
  const key=keyOf(project,analysis);
  // The current choices passed Check for problems (Prepare model checks them again).
  const reviewed=issues!==null&&issues.length===0&&reviewKey===key;
  function invalidate(){generation.current++;setIssues(null);setResult(null);setPreparedPreview(null);setSavedPreset('');}
  /** Simple mode's automatic preparation stopped before converting a file without its skeleton
   * (`missing_skeleton`): the notice offers to continue with automatic weights or choose another file. */
  const [skeletonAsk,setSkeletonAsk]=useState(false);
  function sourceChanged(path:string){invalidate();setPreview(null);setMappingPreview(null);setSource(path);setAnalysis(null);setSkeletonAsk(false);}
  // A model chosen in step 1 is inspected and prepared automatically.
  const [autoSource,setAutoSource]=useState<string|null>(null);
  useEffect(()=>{const receive=(event:Event)=>{const detail=(event as CustomEvent<ModelSourceEvent>).detail;const path=typeof detail==='string'?detail:detail.path;sourceChanged(path);setAutoSource(typeof detail!=='string'&&detail.auto?path:null);};window.addEventListener('dmb-model-source',receive);return()=>window.removeEventListener('dmb-model-source',receive);},[]);
  useEffect(()=>{if(autoSource&&autoSource===source&&!busy&&project.original_model){setAutoSource(null);void inspect(true);}},[autoSource,source,busy,project.original_model]);// eslint-disable-line react-hooks/exhaustive-deps
  // A changed body role also brings the saved moving parts in line with it (savedChoices.ts).
  function changeOptions(values:Partial<PreparationOptions>){invalidate();const next={...options,...values};update({...project,preparation_options:asOptions(analysis?withRoleChange(analysis.analysis.bones,analysis.analysis.weighted_bones,options,next):next)});}
  function selectRole(role:Role){setSelectedRole(role);setSelectedBone('');}
  function setRole(side:'source'|'target',role:Role,name:string){selectRole(role);changeOptions(changeRole(options,side,role,name));}
  // The file window's own title and filter name are only shown.
  const sourceDialog=()=>({title:t('Choose source for preparation'),multiple:false,filters:[{name:t('Source models'),extensions:[...SOURCE_EXTENSIONS]}]});
  async function browse(){const dialog=sourceDialog();await operate(m('Could not choose the source model'),async()=>{const path=await openFor('model-source',dialog);if(typeof path==='string')sourceChanged(path);});}
  async function loadPreview(mapping=false,auto=false){const version=generation.current;let started=false;await operate(m('Source preview did not complete'),async()=>{started=true;const value=decodeGltfPreview(await api.previewSource(mapping&&analysis?analysis.source:source));if(version!==generation.current)return;if(mapping&&analysis&&value.metadata.source_sha256!==analysis.source_sha256)throw new Error('Source changed since inspection. Inspect it again before reviewing the mapping.');if(mapping)setMappingPreview(value);else setPreview(value);},true,true,auto);return started;}
  // Choices that only take effect through a new inspection (the gun file and Scene objects),
  // as they were when the current analysis was made.
  const choicesKey=(o:PreparationOptions)=>JSON.stringify([o.weapon_source??'',o.scene??null]);
  const inspectedWith=useRef<string|null>(null);
  /** Applies an inspection's suggestions; returns the analysis they now belong to and the updated project.
   * `keepRoles` (Refresh suggestions using my roles, and the refresh Prepare model and Check for problems
   * run after a role change) keeps the user's roles and the project's preparation mode. */
  function acceptSuggestions(value:PreparationAnalysis,reply:Suggestions,base:PreparationOptions,current:Project=project,keepRoles=false):{analysis:PreparationAnalysis;project:Project}{
    const {next,shown}=suggestedChoices(value,reply,base,keepRoles);
    setAnalysis(shown);setIssues(null);setResult(null);setPreparedPreview(null);setPage(0);setSelectedBones(new Set());
    inspectedWith.current=choicesKey(next);
    const mode=keepRoles||current.model_input_mode==='weapon'?current.model_input_mode:value.analysis.suggested_mode;
    const updated={...current,model_input_mode:mode,preparation_source:value.source_original,preparation_options:asOptions(next)} as Project;
    updateRef.current(updated);
    return {analysis:shown,project:updated};
  }
  /** The options and the shown analysis that a suggestion reply gives (no state changes). */
  function suggestedChoices(value:PreparationAnalysis,reply:Suggestions,base:PreparationOptions,keepRoles:boolean):{next:PreparationOptions;shown:PreparationAnalysis}{
    const refreshed=keepRoles?refreshWithRoles(base,reply):{options:applySuggestions(base,reply),suggestions:reply},suggestions=refreshed.suggestions;
    const next=physicsForRoles(value.analysis.bones,value.analysis.weighted_bones,refreshed.options),humanoid=value.analysis.humanoid;
    // The source rig's own MMD/VRM springs per kept moving part (used only when chosen on the Physics tab).
    const kept=Array.isArray(next.physics_roots)?new Set(next.physics_roots as string[]):new Set<string>();
    const springs=partSprings(physicsGroups(value.analysis.bones,value.analysis.weighted_bones,next).filter(g=>kept.has(g.root)),value.analysis.springs);
    if(Object.keys(springs).length)next.physics_springs=springs as unknown as PreparationOptions[string];else delete next.physics_springs;
    if('landmarks' in humanoid&&!next.landmarks)next.landmarks=humanoid.landmarks;
    const weapon=weaponDefaults(value,base.weapon);if(weapon)next.weapon=weapon;else delete next.weapon;
    return {next,shown:{...value,suggestions}};
  }
  /** Before Prepare model and Check for problems: after a body-role change the matches are refreshed
   * with the user's roles (no Blender), and saved moving parts that the roles rule out are dropped.
   * A refresh that changes no saved choice (for example after Reopen the last preparation) only
   * updates the suggestions shown; one made for roles with errors is not applied (the check then
   * lists the role errors). Undefined when superseded. */
  async function consistentChoices(value:PreparationAnalysis,current:Project,version:number):Promise<{analysis:PreparationAnalysis;project:Project}|undefined>{
    const chosen=current.preparation_options as PreparationOptions;
    if(current.model_input_mode==='rigged_humanoid'&&rolesDiffer(chosen,value.suggestions)){
      const reply=await api.preparationSuggest(value,chosen);
      if(version!==generation.current)return undefined;
      if(reply.source?.errors.length||reply.target.errors.length)return {analysis:value,project:current};
      const {next,shown}=suggestedChoices(value,reply,chosen,true);
      if(sameChoices(next,chosen)){setAnalysis(shown);return {analysis:shown,project:current};}
      return acceptSuggestions(value,reply,chosen,current,true);
    }
    if(!rootsRuledOut(value.analysis.bones,value.analysis.weighted_bones,chosen).length)return {analysis:value,project:current};
    const updated={...current,preparation_options:asOptions(physicsForRoles(value.analysis.bones,value.analysis.weighted_bones,chosen))} as Project;
    updateRef.current(updated);
    return {analysis:value,project:updated};
  }
  /** Inspects the source; returns the analysis and the updated project (undefined when superseded). */
  async function runInspect(version:number):Promise<{analysis:PreparationAnalysis;project:Project}|undefined>{
    // Scene-object choices and the gun choice belong to one source file.
    const sameSource=source===project.preparation_source||source===analysis?.source_original||source===analysis?.source;
    const sent=sameSource?project:{...project,preparation_options:asOptions({...options,scene:undefined,weapon:undefined})};
    const value=await api.preparationInspect(sent,source,rotation);if(version!==generation.current)return undefined;
    // A saved fit is kept; unset means Match hero proportions in a project with preparation
    // choices of its own. A model checked for the first time in a project gets the setting's
    // fit, rigged or not (an unrigged model's automatic rig converts with either fit). Gun
    // projects keep their fit as it is: a gun-only preparation does not use it.
    const firstCheck=!hasPreparationChoices(project)&&project.model_input_mode!=='weapon';
    const fit=options.fit_mode??(firstCheck?defaultFit:'match_target');
    setInspections(n=>n+1);
    let choices=value.suggestions,base:PreparationOptions={influence_limit:options.influence_limit??4,fit_mode:fit,arm_skinning:options.arm_skinning??'smooth_twist',complete_mapping:true,...(sameSource&&options.scene?{scene:options.scene}:{}),...(sameSource&&options.weapon?{weapon:options.weapon}:{}),...(options.weapon_source?{weapon_source:options.weapon_source}:{}),...(options.breast_motion!==undefined?{breast_motion:options.breast_motion}:{})};
    // A copy made for another hero brings the body roles on the source's bones but no matches:
    // its first check makes the matches again from those roles, keeping every carried choice.
    const copiedRoles=Boolean(project.copied_report)&&Object.keys(options.source_roles??{}).length>0;
    if(sameSource&&(Object.keys(options.mapping??{}).length||copiedRoles)){
      // Retain only choices that exist in this fresh pair of rigs.
      const validRoles=(roles:PreparationOptions['source_roles'],names:Set<string>)=>Object.fromEntries(Object.entries(roles??{}).filter(([,name])=>names.has(name)));
      base={...options,complete_mapping:true,arm_skinning:options.arm_skinning??'smooth_twist',source_roles:validRoles(options.source_roles,new Set(value.analysis.bones.map(b=>b.name))),target_roles:validRoles(options.target_roles,new Set(value.target.map(b=>b.name)))};
      choices=await api.preparationSuggest(value,base);if(version!==generation.current)return undefined;
    }
    setMappingPreview(null);
    return acceptSuggestions(value,choices,base);
  }
  /** One click: check the choices, convert and use the prepared model. Returns false when something needs attention.
   * `askSkeleton` (Simple mode's automatic path): a file without its skeleton stops after the check,
   * before converting, until the user chose to continue with automatic weights (`skeletonChoice.ts`). */
  async function runPrepare(inspected:PreparationAnalysis,start:Project,version:number,askSkeleton=false):Promise<boolean>{
    const fresh=await consistentChoices(inspected,start,version);
    if(!fresh)return false;
    const {analysis:value,project:current}=fresh;
    const chosen=current.preparation_options as PreparationOptions;
    // Clicking Prepare is the review: it confirms the fit and the joints that follow the body.
    const confirmed:PreparationOptions={...chosen,reviewed:true,...(Object.keys(chosen.mapping_anchors??{}).length?{follow_motion_reviewed:true}:{})};
    const ready={...current,preparation_options:asOptions(confirmed)} as Project;
    const checked=await api.preparationReview(ready,value,confirmed);
    if(version!==generation.current)return false;
    if(checked.length){updateRef.current(ready);setIssues(checked);setReviewKey(keyOf(ready,value));return false;}
    if(askSkeleton&&value.analysis.missing_skeleton&&!continuedWithoutSkeleton(current.id,value.source_sha256)){setSkeletonAsk(true);return false;}
    setIssues(checked);setReviewKey(keyOf(ready,value));setResult(null);setPreparedPreview(null);
    const converted=await api.preparationConvert(ready,value,confirmed,false);
    if(version!==generation.current)return false;
    setResult(converted);
    const accepted=await api.preparationAccept(ready,converted);
    if(version!==generation.current)return false;
    updateRef.current(accepted);
    return true;
  }
  async function inspect(auto=false){const version=++generation.current;setSkeletonAsk(false);await operate(auto?m('Your model could not be prepared automatically'):m('Source preparation inspection did not complete'),async()=>{
    const inspected=await runInspect(version);
    if(!inspected||!auto)return;
    if(await runPrepare(inspected.analysis,inspected.project,version,!expert))next?.();
  },true);}
  // Clicking Prepare model (or Continue with automatic weights) is the choice for a file without its skeleton.
  async function prepareModel(){if(!analysis)return;const version=generation.current;setSkeletonAsk(false);if(analysis.analysis.missing_skeleton)continueWithoutSkeleton(project.id,analysis.source_sha256);await operate(m('Your model could not be prepared'),async()=>{if(await runPrepare(analysis,project,version))next?.();},true);}
  /** Choose another model file from the no-skeleton stop: it is checked and prepared automatically, as from step 1. */
  async function chooseAnother(){const dialog=sourceDialog();await operate(m('Could not choose the source model'),async()=>{const path=await openFor('model-source',dialog);if(typeof path==='string'){sourceChanged(path);setAutoSource(path);}});}
  /** Re-detect body mapping (`resetRoles`) or Refresh suggestions using my roles (the roles stay). */
  async function suggestions(resetRoles=true){if(!analysis)return;const version=++generation.current;await operate(m('Automatic mapping did not complete'),async()=>{const value=await api.preparationSuggest(analysis,resetRoles?{...options,source_roles:{},target_roles:{}}:options);if(version===generation.current)acceptSuggestions(analysis,value,options,project,!resetRoles);});}
  async function review(){if(!analysis)return;const version=generation.current;await operate(m('Preparation review did not complete'),async()=>{const fresh=await consistentChoices(analysis,project,version);if(!fresh)return;const checked=await api.preparationReview(fresh.project,fresh.analysis,fresh.project.preparation_options as PreparationOptions);if(version===generation.current){setIssues(checked);setReviewKey(keyOf(fresh.project,fresh.analysis));}});}
  /** The one-click fix next to a check result, then the check again. */
  async function fixIssue(kind:IssueKind){if(!analysis)return;invalidate();const version=generation.current;await operate(m('Preparation review did not complete'),async()=>{
    let value=analysis,current=project;
    if(kind==='stale_following'){const reply=await api.preparationSuggest(value,options);if(version!==generation.current)return;({analysis:value,project:current}=acceptSuggestions(value,reply,options,current,true));}
    else{current={...project,preparation_options:asOptions({...options,...keepDetectedGroups(physics,options)})} as Project;updateRef.current(current);}
    const checked=await api.preparationReview(current,value,current.preparation_options as PreparationOptions);
    if(version===generation.current){setIssues(checked);setReviewKey(keyOf(current,value));}
  });}
  async function savePreset(){if(!analysis||!reviewed)return;const version=generation.current;await operate(m('Mapping preset could not be saved'),async()=>{await api.preparationSaveMappingPreset(analysis,options);if(version===generation.current)setSavedPreset(key);});}
  async function convert(correction=false){if(!analysis)return;const version=generation.current;await operate(m('Model conversion did not complete'),async()=>{const checked=await api.preparationReview(project,analysis,options);if(version!==generation.current)return;setIssues(checked);setReviewKey(key);if(checked.length)return;const confirmed={...options,reviewed:true,...(Object.keys(options.mapping_anchors??{}).length?{follow_motion_reviewed:true}:{})};const correctionBlend=result?.blend,conversionProject={...(correction&&result?{...project,preparation_blend:result.blend,preparation_report:result.report}:project),preparation_options:asOptions(confirmed)} as Project;setResult(null);setPreparedPreview(null);const value=await api.preparationConvert(conversionProject,analysis,confirmed,correction,correction?correctionBlend:undefined);if(version===generation.current)setResult(value);},true);}
  async function accept(){if(result)await operate(m('Prepared model could not be accepted'),async()=>update(await api.preparationAccept(project,result)),true);}
  async function resume(){const version=++generation.current;setSkeletonAsk(false);await operate(m('Saved preparation could not be resumed'),async()=>{const value=await api.preparationResume(project);if(version!==generation.current)return;setInspections(n=>n+1);setSource(value.analysis.source_original);setRotation(value.analysis.rotation);setAnalysis(value.analysis);inspectedWith.current=choicesKey(options);setMappingPreview(null);setPreparedPreview(null);setResult(value.result);setIssues(null);},true);}
  async function previewPrepared(auto=false){if(!result)return false;const version=generation.current;let started=false;await operate(m('Prepared model preview did not complete'),async()=>{started=true;const value=decodePreview(await api.preview(result.dmx));if(version===generation.current)setPreparedPreview(value);},true,true,auto);return started;}
  // The buttons' loads (the panels take a plain promise).
  const showSource=async(mapping=false)=>{await loadPreview(mapping);},showPrepared=async()=>{await previewPrepared();};
  const labels=analysis?.suggestions.assistance?.labels??{};
  const entries=useMemo(()=>{
    const assist=new Map((analysis?.suggestions.assistance?.entries??[]).map(e=>[e.source,e]));
    // A bone the suggestions left out: its group stays English (it is a filter value); its reason is only shown.
    return (analysis?.analysis.weighted_bones??[]).map(name=>assist.get(name)??{source:name,target:null,group:'Other bones',label:name,reason:t('Choose and review a target bone.'),automatic:false} as MappingEntry);
  },[analysis,language]);// eslint-disable-line react-hooks/exhaustive-deps
  const physics=physicsGroups(analysis?.analysis.bones??[],analysis?.analysis.weighted_bones??[],options),kept=keptPhysicsBones(physics,options);
  const bodyEntries=entries.filter(e=>!kept.has(e.source));
  const unmapped=bodyEntries.filter(e=>!options.mapping?.[e.source]).length,groups=[...new Set(entries.map(e=>e.group))].sort();
  const followingCount=Object.keys(options.mapping_anchors??{}).filter(n=>!kept.has(n)).length;
  const proposed=bodyEntries.filter(e=>e.follow&&!options.mapping?.[e.source]);
  const rolesChanged=Boolean(analysis)&&rolesDiffer(options,analysis!.suggestions);
  const staleMatches=issues?.find(issue=>issue.kind==='stale_following')?.bones?.length??0;
  const roleProblems=[...(analysis?.suggestions.source?.errors??[]).map(error=>t('Source: {error}',{error})),...(analysis?.suggestions.target.errors??[]).map(error=>t('Hero: {error}',{error}))];
  const allFollowing=rolesChanged?[]:pendingFollowing(options,bodyEntries);
  const pelvisFollowing=!rolesChanged&&options.target_roles?.pelvis?pendingFollowing(options,bodyEntries,options.target_roles.pelvis):[];
  const offsetUpgrades=bodyEntries.filter(e=>e.follow?.automatic&&options.mapping?.[e.source]===e.follow.target&&!options.mapping_anchors?.[e.source]);
  const proposalGroups=[...new Set(proposed.map(e=>e.follow!.group))].sort();
  const visible=entries.filter(e=>(filter==='all'||(filter==='following'?!kept.has(e.source)&&Boolean(options.mapping_anchors?.[e.source]):!kept.has(e.source)&&!options.mapping?.[e.source]))&&(group==='all'||e.group===group)&&`${e.source} ${e.label} ${options.mapping?.[e.source]??''}`.toLowerCase().includes(search.toLowerCase()));
  const pageCount=Math.max(1,Math.ceil(visible.length/30)),currentPage=Math.min(page,pageCount-1),warnings=roleWarnings(options);
  const boneLabel=(name:string)=>labels[name]?`${name} — ${boneLabelText(labels[name])}`:name;
  // Simple mode picks the panel that needs attention (or Fit & review); Expert starts on Body roles.
  const [panel,setPanel]=useState<PrepPanel|null>(expert?'roles':null);
  const [matchesOpen,setMatchesOpen]=useState(false),[view,setView]=useState<'model'|'joints'|'result'>('joints');
  const [roleGroup,setRoleGroup]=useState('all');
  const weaponOnly=project.model_input_mode==='weapon',weaponTab=weaponOnly||(analysis?.analysis.weapons?.length??0)>0||(Boolean(analysis)&&project.model_input_mode!=='prepared');
  // Joint and bone matching tabs: always in Expert, otherwise only when something needs a choice.
  const needsMatches=unmapped>0||warnings.length>0||roleProblems.length>0;
  const showMatches=expert||needsMatches||matchesOpen;
  const hiddenPanels:PrepPanel[]=showMatches?[]:['roles','mapping'];
  const wanted:PrepPanel=panel??(needsMatches?(unmapped>0&&project.model_input_mode==='rigged_humanoid'?'mapping':'roles'):'review');
  const chosen:PrepPanel=hiddenPanels.includes(wanted)?'review':wanted;
  const activePanel:PrepPanel=weaponOnly?(chosen==='review'?'review':'weapon'):project.model_input_mode!=='rigged_humanoid'&&(chosen==='physics'||chosen==='mapping')?'roles':chosen==='weapon'&&!weaponTab?'roles':chosen;
  const shownRoles=ROLES.filter(role=>roleGroup==='all'||(roleGroup==='torso'?!role.includes('_'):roleGroup==='arms'?/upperarm|forearm|hand/.test(role):/thigh|shin|foot/.test(role)));
  // Only shown: the game's name of the hero in the interface language.
  const hero=shownHero(project.hero_name)||basename(project.original_model).replace(/\.vmdl(_c)?$/,'');
  function jump(id:string){document.getElementById(id)?.scrollIntoView?.({block:'start',behavior:window.matchMedia?.('(prefers-reduced-motion: reduce)').matches?'auto':'smooth'});}
  function showPanel(next:PrepPanel){setPanel(next);}
  useEffect(()=>{if(issues)document.getElementById('prep-check-results')?.scrollIntoView?.({block:'nearest'});},[issues]);
  // Choices that need a new inspection re-run it automatically: a changed gun file at once,
  // Scene objects after a short pause (several boxes are often changed together).
  const inspectedGun=analysis?.weapon_file?.selection.source??'';
  const gunPending=Boolean(analysis)&&Boolean(options.weapon_source||inspectedGun)&&!samePath(options.weapon_source??'',inspectedGun);
  const choices=choicesKey(options);
  const reinspectReason=!analysis||busy?'':gunPending?'gun':inspectedWith.current!==null&&inspectedWith.current!==choices?'choices':'';
  const autoTried=useRef('');
  useEffect(()=>{
    if(!reinspectReason)return;
    const attempt=reinspectReason+choices;
    if(autoTried.current===attempt)return;
    const timer=window.setTimeout(()=>{autoTried.current=attempt;void inspect();},reinspectReason==='gun'?200:1200);
    return ()=>window.clearTimeout(timer);
  },[reinspectReason,choices]);// eslint-disable-line react-hooks/exhaustive-deps
  // Load 3D previews automatically: after a check the shown view loads by itself, once per
  // inspection (Your model, Joint check) or conversion (Result), never while a re-check waits.
  // Leaving the page stops a load still running (Blender may be preparing the source).
  const autoKey=!analysis||reinspectReason||autoSource!==null?null
    :view==='model'?(!preview&&source?`model:${inspections}:${source}`:null)
    :view==='joints'?(!mappingPreview?`joints:${inspections}`:null)
    :result&&!preparedPreview?`result:${idOf(result)}`:null;
  useAutoLoad({on:autoPreviews&&active,key:autoKey,busy,cancelOnLeave:true,load:()=>view==='model'?loadPreview(false,true):view==='joints'?loadPreview(true,true):previewPrepared(true)});
  const prepared=project.preparation_report&&project.dmx;
  // Model fitting -> Leg joints (Preserve source proportions only). Choices that put the
  // legs on the hero's joints cannot shorten or lengthen the legs to reach the floor.
  const preserving=options.fit_mode==='preserve_proportions',legJoints=options.fit_leg_joints??'pelvis';
  const legsOnJoints=preserving&&LEGS_ON_HERO_JOINTS.includes(legJoints);
  const floorConflict=legsOnJoints&&options.fit_floor==='legs';
  const floorChoices=legsOnJoints&&!floorConflict?FLOOR_CHOICES.filter(([value])=>value!=='legs'):FLOOR_CHOICES;
  const [floorReset,setFloorReset]=useState(false);
  function changeLegJoints(value:string){
    const fit_leg_joints=value as NonNullable<PreparationOptions['fit_leg_joints']>;
    // Shortened or lengthened legs would leave the hero's joints again: fall back visibly.
    const reset=LEGS_ON_HERO_JOINTS.includes(fit_leg_joints)&&options.fit_floor==='legs';
    setFloorReset(reset);
    changeOptions({fit_leg_joints,...(reset?{fit_floor:'ankles' as const}:{}),reviewed:false});
  }
  const legMeasured=preserving&&result?legFitLine(result.leg_fit):null;
  const usedResult=Boolean(result)&&project.dmx===result?.dmx;
  // The frame: top-bar menus, the open Fit & review group, the tab strip with a status word each.
  const menus=useStudioBarMenus<PrepMenu>('prep');
  const [fitGroup,setFitGroup]=useState<FitGroup|null>(null);
  function openIssue(tab:string){if(/shape/i.test(tab))setFitGroup('shapekeys');else if(/fitting/i.test(tab))setFitGroup('proportions');}
  const rigged=project.model_input_mode==='rigged_humanoid';
  const offered:PrepPanel[]=(['roles','physics','mapping','weapon','review'] as PrepPanel[]).filter(id=>!hiddenPanels.includes(id)&&(weaponOnly?id==='weapon'||id==='review':(rigged||(id!=='physics'&&id!=='mapping'))&&(id!=='weapon'||weaponTab)));
  const toFix=(id:PrepPanel)=>issues&&reviewKey===key?issues.filter(issue=>issueSection(issue.tab)===id).length:0;
  const morphs=analysis?.analysis.morphs?.length??0,shapeNeeded=morphs>0&&!analysis?.geometry_customization?.face&&!options.shape_key_mode;
  const missingGroups=analysis&&options.preserve_physics_bones?undetectedRoots(physics,options).length:0;
  const keptRoots=Array.isArray(options.physics_roots)?options.physics_roots.length:0;
  const word=(status:string,tone:StudioTone)=>({status,tone});
  const fixWord=(id:PrepPanel)=>{const n=toFix(id);return n?word(tp(n,{other:'{count} to fix'}),'attention'):null;};
  const tabWords:Record<PrepPanel,{status:string;tone:StudioTone}>={
    roles:fixWord('roles')??(roleProblems.length||warnings.length?word(t('Needs your choice'),'attention'):rolesChanged?word(t('changed'),'changed'):word('✓','ok')),
    physics:fixWord('physics')??(missingGroups?word(t('Needs your choice'),'attention'):options.preserve_physics_bones&&keptRoots?word(tp(keptRoots,{other:'{count} kept'}),'ok'):physics.length?word(t('Off'),'quiet'):word(t('None found'),'quiet')),
    mapping:fixWord('mapping')??(unmapped?word(tp(unmapped,{other:'{count} to match'}),'attention'):staleMatches?word(tp(staleMatches,{other:'{count} to refresh'}),'attention'):rolesChanged?word(t('refresh'),'changed'):word('✓','ok')),
    weapon:fixWord('weapon')??word('✓','ok'),
    review:fixWord('review')??(shapeNeeded||floorConflict?word(t('Needs your choice'),'attention'):word('✓','ok')),
  };
  const tabs:StudioTab<PrepPanel>[]=offered.map(id=>({id,label:t(TAB_NAMES[id]),...tabWords[id]}));
  // English here; the top bar translates it where it is drawn.
  const attention=analysis?.analysis.missing_skeleton?m('No skeleton'):roleProblems.length?m('Needs your choice'):null;
  const notesShown=expert||showMatches;
  // Labels on the character and the lit bone for the open tab (shown only once the Joint check
  // view is loaded; nothing is loaded for them). Body roles labels the chosen region's joints and
  // always the selected one, hiding what would overlap; Joint matches labels the selected bone;
  // Moving parts one label per kept group. A click selects the joint or finds the row.
  const [markedGroup,setMarkedGroup]=useState('');
  const reveal=(id:string)=>document.getElementById(id)?.scrollIntoView?.({block:'nearest',behavior:window.matchMedia?.('(prefers-reduced-motion: reduce)').matches?'auto':'smooth'});
  const sourceOf=(role:Role)=>options.source_roles?.[role];
  const labelRoles=[...new Set<Role>([...shownRoles,selectedRole])].filter(role=>Boolean(sourceOf(role)));
  const keptGroups=options.preserve_physics_bones?physics.filter(g=>keptRoots&&(options.physics_roots as string[]).includes(g.root)):[];
  const viewLabels:ModelLabelSet|undefined=activePanel==='roles'?{label:t('Body joints on the model'),crowd:'hide',items:labelRoles.map(role=>({id:role,text:roleLabel(role),active:role===selectedRole,bones:[sourceOf(role)!]})),select:id=>{selectRole(id as Role);reveal(roleRowId(id as Role));}}
    :activePanel==='mapping'&&selectedBone?{label:t('Selected bone on the model'),items:[{id:selectedBone,text:selectedBone,detail:t('selected'),active:true,bones:[selectedBone]}],select:id=>reveal(matchRowId(id))}
    :activePanel==='physics'&&keptGroups.length?{label:t('Kept moving parts on the model'),crowd:'hide',items:keptGroups.map(g=>({id:g.root,text:g.root,detail:tpc('model label',g.source_bones.length,{other:'{count} bones'}),active:g.root===markedGroup,bones:g.source_bones})),select:id=>{setMarkedGroup(id);reveal(groupRowId(id));}}
    :undefined;
  const litRole=sourceOf(selectedRole);
  const lit:LitBone|null|undefined=activePanel==='roles'?(litRole?{bone:litRole,name:roleLabel(selectedRole)}:null):activePanel==='mapping'?(selectedBone?{bone:selectedBone,name:selectedBone}:null):undefined;
  const labelHint=activePanel==='roles'?t('click a label to select that joint'):activePanel==='physics'?t('click a label to find it in the list'):undefined;
  const sourceFields=<SourceFields analysis={analysis} source={source} busy={busy} sourceChanged={sourceChanged} browse={browse} options={options} operate={operate} changeOptions={changeOptions} rotation={rotation} invalidate={invalidate} setRotation={setRotation} setAnalysis={setAnalysis} setMappingPreview={setMappingPreview} project={project} setPanel={setPanel} expert={expert} inspect={inspect} setView={setView} loadPreview={showSource}/>;
  const menuContent={
    model:sourceFields,
    mode:analysis&&notesShown?<ModePanel project={project} busy={busy} invalidate={invalidate} update={update} options={options}/>:undefined,
    scene:analysis?.analysis.scene?<SceneObjects bare receipt={analysis.analysis.scene} choices={options.scene??{}} change={scene=>changeOptions({scene})} busy={busy} reinspect={()=>void inspect()}/>:undefined,
    notes:analysis?<NotesPanel details={notesShown} busy={busy} suggestions={suggestions} unmapped={unmapped} analysis={analysis} prepared={Boolean(result)}/>:null,
  };
  return <div className="prep-workspace">
    <header className="prep-heading"><div><span className="prep-eyebrow">{t('STEP {step} OF {count}',{step:2,count:5})}</span><h1>{t('Prepare model')}<span className="prep-hero-chip">{hero}</span></h1><p>{t("Your model is fitted to {hero}'s skeleton so it moves with the hero's animations. Most models are ready in one click.",{hero})}</p></div><button className="prep-secondary" disabled={busy||!project.preparation_report} onClick={()=>void resume()}><PrepIcon name="refresh" size={17}/>{t('Reopen the last preparation')}</button></header>
    {!analysis&&project.dmx&&<section className="prep-card prep-done" role="status"><PrepIcon name="check"/><div><strong>{prepared?t('Your model is prepared.'):t('Your model is already game-ready (DMX), so it does not need preparing.')}</strong><p className="hint">{prepared?t('{file} is in use. Prepare again only to change the fit, the gun or the moving parts.',{file:basename(project.dmx)}):t('You can go straight to Appearance & weapons.')}</p></div><div className="row">{next&&<button className="prep-primary" disabled={busy} onClick={next}>{t('Continue to Appearance & weapons')}<PrepIcon name="arrow" size={18}/></button>}{prepared&&<button className="prep-secondary" disabled={busy} onClick={()=>void resume()}>{t('Change the preparation')}</button>}</div></section>}
    <StudioFrame label={t('Prepare model')} className="prep-studio">
      <PrepTopBar source={source} analysis={analysis} unmapped={unmapped} mode={project.model_input_mode} attention={attention} notesShown={notesShown} content={menuContent} menus={menus} sceneChoices={options.scene??{}}/>
      {preview&&!analysis&&<SourcePreview preview={preview} setPreview={setPreview}/>}
      {reinspectReason&&<p className="prep-reinspect" role="status">{reinspectReason==='gun'?t('Loading the gun file…'):t('Checking your model again with the new Scene objects choices…')}</p>}
      {analysis&&<>
        <FoundNotices analysis={analysis} unmapped={unmapped} choice={skeletonAsk?{busy,proceed:()=>void prepareModel(),another:()=>void chooseAnother()}:undefined}/>
        <div id="prep-review-workspace" className="prep-review-workspace">
          {!showMatches&&!weaponOnly&&<p className={analysis.analysis.missing_skeleton?'prep-matched prep-matched-warn':'prep-matched'} role="status"><PrepIcon name={analysis.analysis.missing_skeleton?'info':'check'} size={16}/>{analysis.analysis.missing_skeleton?t('No bones to match: this file has no skeleton.'):tp(bodyEntries.length,{other:'All {count} bones matched to {hero}.'},{count:bodyEntries.length.toLocaleString(),hero})} <button className="tertiary" disabled={busy} onClick={()=>{setMatchesOpen(true);setPanel('roles');}}>{t('Edit matches')}</button></p>}
          <StudioTabs label={t('Preparation review sections')} base="prep" tabs={tabs} active={activePanel} onSelect={showPanel}/>
          <div className="prep-studio-grid">
            <div className="prep-studio-view"><ViewColumn view={view} setView={setView} result={result} preparedPreview={preparedPreview} previewPrepared={showPrepared} mappingPreview={mappingPreview} analysis={analysis} options={options} selectedRole={selectedRole} selectRole={selectRole} selectedBone={selectedBone} busy={busy} loadPreview={showSource} preview={preview} source={source} project={project} rotation={rotation} labels={viewLabels} lit={lit} labelHint={labelHint}/></div>
            <div className="prep-studio-panels">
              <StudioTabPanel base="prep" id="roles" active={activePanel==='roles'} className="prep-panel">
                <RolesPanel warnings={warnings} roleGroup={roleGroup} setRoleGroup={setRoleGroup} shownRoles={shownRoles} selectedRole={selectedRole} selectRole={selectRole} project={project} options={options} busy={busy} changeOptions={changeOptions} setRole={setRole} analysis={analysis} boneLabel={boneLabel}/>
              </StudioTabPanel>
              {rigged&&<StudioTabPanel base="prep" id="physics" active={activePanel==='physics'} className="prep-panel"><MovingPartsPanel physics={physics} analysis={analysis} options={options} changeOptions={changeOptions} busy={busy} inspect={inspect} marked={markedGroup}/></StudioTabPanel>}
              {rigged&&<StudioTabPanel base="prep" id="mapping" active={activePanel==='mapping'} className="prep-panel">
                <MappingPanel staleMatches={staleMatches} unmapped={unmapped} busy={busy} pelvisFollowing={pelvisFollowing} changeOptions={changeOptions} options={options} allFollowing={allFollowing} rolesChanged={rolesChanged} suggestions={suggestions} roleProblems={roleProblems} showPanel={showPanel} followingCount={followingCount} offsetUpgrades={offsetUpgrades} proposalGroups={proposalGroups} proposed={proposed} setSearch={setSearch} setPage={setPage} setGroup={setGroup} setFilter={setFilter} setSelectedBone={setSelectedBone} filter={filter} group={group} groups={groups} search={search} selectedBones={selectedBones} visible={visible} setSelectedBones={setSelectedBones} kept={kept} batchTarget={batchTarget} setBatchTarget={setBatchTarget} analysis={analysis} currentPage={currentPage} pageCount={pageCount} selectedBone={selectedBone} total={entries.length}/>
              </StudioTabPanel>}
              {weaponTab&&<StudioTabPanel base="prep" id="weapon" active={activePanel==='weapon'} className="prep-panel"><GunPanel project={project} analysis={analysis} options={options} changeOptions={changeOptions} busy={busy} operate={operate} inspect={inspect} projectPath={projectPath} saveProject={saveProject} update={update}/></StudioTabPanel>}
              <StudioTabPanel base="prep" id="review" active={activePanel==='review'} className="prep-panel">
                <FitPanel hero={hero} weaponOnly={weaponOnly} options={options} busy={busy} changeOptions={changeOptions} floorChoices={floorChoices} setFloorReset={setFloorReset} legsOnJoints={legsOnJoints} floorConflict={floorConflict} floorReset={floorReset} preserving={preserving} legJoints={legJoints} changeLegJoints={changeLegJoints} legMeasured={legMeasured} analysis={analysis} expert={expert} followingCount={followingCount} reviewed={reviewed} project={project} savePreset={savePreset} savedPreset={savedPreset} presetKey={key} result={result} review={review} prepareModel={prepareModel} group={fitGroup} setGroup={setFitGroup} defaultFit={defaultFit}/>
              </StudioTabPanel>
            </div>
          </div>
        </div>
        {issues&&<CheckResults issues={issues} reviewKey={reviewKey} currentKey={key} setMatchesOpen={setMatchesOpen} setPanel={setPanel} jump={jump} fix={kind=>void fixIssue(kind)} busy={busy} open={openIssue}/>}
      </>}
      {result&&<PreparedOutput result={result} busy={busy} setView={setView} jump={jump} preparedPreview={preparedPreview} previewPrepared={showPrepared} project={project} operate={operate} analysis={analysis} convert={convert} usedResult={usedResult}/>}
      {analysis&&<ActionBar reviewed={reviewed} result={result} usedResult={usedResult} issues={issues} setView={setView} jump={jump} preparedPreview={preparedPreview} previewPrepared={showPrepared} next={next} busy={busy} accept={accept} setPanel={setPanel} review={review} prepareModel={prepareModel}/>}
    </StudioFrame>
  </div>;
}
