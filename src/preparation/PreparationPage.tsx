import { lazy, Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { api, basename } from '../api';
import type { JsonValue, Project } from '../models';
import { ROLES, type MappingEntry, type PreparationAnalysis, type PreparationOptions, type PreparationResult, type ReviewIssue, type Role, type Suggestions, type Vec3 } from './models';
import { applyFollowing, applySuggestions, assignTargets, changeRole, pendingFollowing, roleLabel, roleWarnings } from './mapping';
import { PhysicsBoneChoices } from './PhysicsBoneChoices';
import { SceneObjects } from './SceneObjects';
import { WeaponPanel, samePath, weaponDefaults } from './WeaponPanel';
import { weaponLike } from '../components/WeaponModels';
import { physicsGroups, keptPhysicsBones, automaticPhysicsOptions, breastBones, breastMotionOptions, gunChoiceOptions, hangsFromGun, SECONDARY_PREFIX } from './secondary-bones';
import { Check, SelectField } from '../components/CustomizationPages';
import { NumberField } from '../components/MaterialEditor';
import { Disclosure, Section } from '../components/Primitives';
import { CompactPathField, PathValue } from '../components/Paths';
import { decodeGltfPreview, type GltfPreview } from '../preview/gltf-packet';
import { decodePreview, type PreviewScene } from '../preview/packet';
import { PrepIcon, PreparationTabs, PreparationPanel, type PrepPanel } from './PreparationChrome';
import './preparation.css';
const GltfViewport=lazy(()=>import('../preview/GltfViewport'));
const MappingViewport=lazy(()=>import('./MappingViewport'));
const Viewport=lazy(()=>import('../preview/Viewport'));
type Operate=(label:string,operation:()=>Promise<void>,cancel?:boolean)=>Promise<unknown>;
const asOptions=(options:PreparationOptions)=>JSON.parse(JSON.stringify(options)) as Record<string,JsonValue>;
/** The raw inspection, serialized only once the panel is opened (it can be several MB). */
function RawInspection({value}:{value:unknown}){
  const [open,setOpen]=useState(false);
  const text=useMemo(()=>open?JSON.stringify(value,null,2):'',[open,value]);
  return <details className="technical" onToggle={event=>setOpen((event.currentTarget as HTMLDetailsElement).open)}><summary>Native source inspection</summary>{open&&<pre>{text}</pre>}</details>;
}

/** The event that brings a model from step 1 (or a drop): the path, and whether to prepare it automatically. */
export type ModelSourceEvent=string|{path:string;auto?:boolean};
const keyOf=(project:Project,analysis:PreparationAnalysis|null)=>JSON.stringify([project.original_model,project.model_input_mode,project.preparation_options,analysis?.source_sha256]);

export function PreparationPage({project,update,busy,operate,projectPath,saveProject,next}:{project:Project;update:(project:Project)=>void;busy:boolean;operate:Operate;
  /** Where the project is saved (a separate gun model's project is saved next to it). */projectPath?:string|null;
  /** Saves the project when it was never saved; resolves to its path. */saveProject?:()=>Promise<string|undefined>;
  /** Continue to the next step (Appearance & weapons) once the model is prepared. */next?:()=>void}){
  // Long operations update the project more than once: always use the newest updater.
  const updateRef=useRef(update);updateRef.current=update;
  const [preview,setPreview]=useState<GltfPreview|null>(null),[mappingPreview,setMappingPreview]=useState<GltfPreview|null>(null),[preparedPreview,setPreparedPreview]=useState<PreviewScene|null>(null);
  const [source,setSource]=useState(project.preparation_source),[rotation,setRotation]=useState<Vec3>([0,0,0]);
  const [analysis,setAnalysis]=useState<PreparationAnalysis|null>(null),[issues,setIssues]=useState<ReviewIssue[]|null>(null),[reviewKey,setReviewKey]=useState(''),[result,setResult]=useState<PreparationResult|null>(null);
  const [selectedRole,setSelectedRole]=useState<Role>('pelvis'),[selectedBone,setSelectedBone]=useState(''),[filter,setFilter]=useState('unmapped'),[group,setGroup]=useState('all'),[search,setSearch]=useState(''),[page,setPage]=useState(0);
  const [selectedBones,setSelectedBones]=useState<Set<string>>(new Set()),[batchTarget,setBatchTarget]=useState('');
  const [savedPreset,setSavedPreset]=useState('');
  const generation=useRef(0);
  useEffect(()=>{generation.current++;setAnalysis(null);setMappingPreview(null);setPreparedPreview(null);setIssues(null);setResult(null);},[project.original_model]);
  useEffect(()=>()=>{generation.current++;},[]);
  const options=project.preparation_options as PreparationOptions;
  const key=keyOf(project,analysis);
  // The current choices passed Check for problems (Prepare model checks them again).
  const reviewed=issues!==null&&issues.length===0&&reviewKey===key;
  function invalidate(){generation.current++;setIssues(null);setResult(null);setPreparedPreview(null);setSavedPreset('');}
  function sourceChanged(path:string){invalidate();setPreview(null);setMappingPreview(null);setSource(path);setAnalysis(null);}
  // A model chosen in step 1 is inspected and prepared automatically.
  const [autoSource,setAutoSource]=useState<string|null>(null);
  useEffect(()=>{const receive=(event:Event)=>{const detail=(event as CustomEvent<ModelSourceEvent>).detail;const path=typeof detail==='string'?detail:detail.path;sourceChanged(path);setAutoSource(typeof detail!=='string'&&detail.auto?path:null);};window.addEventListener('dmb-model-source',receive);return()=>window.removeEventListener('dmb-model-source',receive);},[]);
  useEffect(()=>{if(autoSource&&autoSource===source&&!busy&&project.original_model){setAutoSource(null);void inspect(true);}},[autoSource,source,busy,project.original_model]);// eslint-disable-line react-hooks/exhaustive-deps
  function changeOptions(values:Partial<PreparationOptions>){invalidate();update({...project,preparation_options:asOptions({...options,...values})});}
  function selectRole(role:Role){setSelectedRole(role);setSelectedBone('');}
  function setRole(side:'source'|'target',role:Role,name:string){selectRole(role);changeOptions(changeRole(options,side,role,name));}
  async function browse(){await operate('Could not choose the source model',async()=>{const path=await open({title:'Choose source for preparation',multiple:false,filters:[{name:'Source models',extensions:['blend','glb','gltf','fbx']}]});if(typeof path==='string')sourceChanged(path);});}
  async function loadPreview(mapping=false){const version=generation.current;await operate('Source preview did not complete',async()=>{const value=decodeGltfPreview(await api.previewSource(mapping&&analysis?analysis.source:source));if(version!==generation.current)return;if(mapping&&analysis&&value.metadata.source_sha256!==analysis.source_sha256)throw new Error('Source changed since inspection. Inspect it again before reviewing the mapping.');if(mapping)setMappingPreview(value);else setPreview(value);},true);}
  // Choices that only take effect through a new inspection (the gun file and Scene objects),
  // as they were when the current analysis was made.
  const choicesKey=(o:PreparationOptions)=>JSON.stringify([o.weapon_source??'',o.scene??null]);
  const inspectedWith=useRef<string|null>(null);
  /** Applies an inspection's suggestions; returns the updated project. */
  function acceptSuggestions(value:PreparationAnalysis,suggestions:Suggestions,base:PreparationOptions):Project{
    const next=automaticPhysicsOptions(value.analysis.bones,value.analysis.weighted_bones,applySuggestions(base,suggestions)),humanoid=value.analysis.humanoid;
    if('landmarks' in humanoid&&!next.landmarks)next.landmarks=humanoid.landmarks;
    const weapon=weaponDefaults(value,base.weapon);if(weapon)next.weapon=weapon;else delete next.weapon;
    setAnalysis({...value,suggestions});setIssues(null);setResult(null);setPreparedPreview(null);setPage(0);setSelectedBones(new Set());
    inspectedWith.current=choicesKey(next);
    const updated={...project,model_input_mode:project.model_input_mode==='weapon'?'weapon':value.analysis.suggested_mode,preparation_source:value.source_original,preparation_options:asOptions(next)} as Project;
    updateRef.current(updated);
    return updated;
  }
  /** Inspects the source; returns the analysis and the updated project (undefined when superseded). */
  async function runInspect(version:number):Promise<{analysis:PreparationAnalysis;project:Project}|undefined>{
    // Scene-object choices and the gun choice belong to one source file.
    const sameSource=source===project.preparation_source||source===analysis?.source_original||source===analysis?.source;
    const sent=sameSource?project:{...project,preparation_options:asOptions({...options,scene:undefined,weapon:undefined})};
    const value=await api.preparationInspect(sent,source,rotation);if(version!==generation.current)return undefined;
    let choices=value.suggestions,base:PreparationOptions={influence_limit:options.influence_limit??4,fit_mode:options.fit_mode??'match_target',arm_skinning:options.arm_skinning??'smooth_twist',complete_mapping:true,...(sameSource&&options.scene?{scene:options.scene}:{}),...(sameSource&&options.weapon?{weapon:options.weapon}:{}),...(options.weapon_source?{weapon_source:options.weapon_source}:{}),...(options.breast_motion!==undefined?{breast_motion:options.breast_motion}:{})};
    if(sameSource&&Object.keys(options.mapping??{}).length){
      // Retain only choices that exist in this fresh pair of rigs.
      const validRoles=(roles:PreparationOptions['source_roles'],names:Set<string>)=>Object.fromEntries(Object.entries(roles??{}).filter(([,name])=>names.has(name)));
      base={...options,complete_mapping:true,arm_skinning:options.arm_skinning??'smooth_twist',source_roles:validRoles(options.source_roles,new Set(value.analysis.bones.map(b=>b.name))),target_roles:validRoles(options.target_roles,new Set(value.target.map(b=>b.name)))};
      choices=await api.preparationSuggest(value,base);if(version!==generation.current)return undefined;
    }
    setMappingPreview(null);
    return {analysis:value,project:acceptSuggestions(value,choices,base)};
  }
  /** One click: check the choices, convert and use the prepared model. Returns false when something needs attention. */
  async function runPrepare(value:PreparationAnalysis,current:Project,version:number):Promise<boolean>{
    const chosen=current.preparation_options as PreparationOptions;
    // Clicking Prepare is the review: it confirms the fit and the joints that follow the body.
    const confirmed:PreparationOptions={...chosen,reviewed:true,...(Object.keys(chosen.mapping_anchors??{}).length?{follow_motion_reviewed:true}:{})};
    const ready={...current,preparation_options:asOptions(confirmed)} as Project;
    const checked=await api.preparationReview(ready,value,confirmed);
    if(version!==generation.current)return false;
    if(checked.length){updateRef.current(ready);setIssues(checked);setReviewKey(keyOf(ready,value));return false;}
    setIssues(checked);setReviewKey(keyOf(ready,value));setResult(null);setPreparedPreview(null);
    const converted=await api.preparationConvert(ready,value,confirmed,false);
    if(version!==generation.current)return false;
    setResult(converted);
    const accepted=await api.preparationAccept(ready,converted);
    if(version!==generation.current)return false;
    updateRef.current(accepted);
    return true;
  }
  async function inspect(auto=false){const version=++generation.current;await operate(auto?'Your model could not be prepared automatically':'Source preparation inspection did not complete',async()=>{
    const inspected=await runInspect(version);
    if(!inspected||!auto)return;
    if(await runPrepare(inspected.analysis,inspected.project,version))next?.();
  },true);}
  async function prepareModel(){if(!analysis)return;const version=generation.current;await operate('Your model could not be prepared',async()=>{if(await runPrepare(analysis,project,version))next?.();},true);}
  async function suggestions(resetRoles=true){if(!analysis)return;const version=++generation.current;await operate('Automatic mapping did not complete',async()=>{const value=await api.preparationSuggest(analysis,resetRoles?{...options,source_roles:{},target_roles:{}}:options);if(version===generation.current)acceptSuggestions(analysis,value,options);});}
  async function review(){if(!analysis)return;const version=generation.current;await operate('Preparation review did not complete',async()=>{const checked=await api.preparationReview(project,analysis,options);if(version===generation.current){setIssues(checked);setReviewKey(key);}});}
  async function savePreset(){if(!analysis||!reviewed)return;const version=generation.current;await operate('Mapping preset could not be saved',async()=>{await api.preparationSaveMappingPreset(analysis,options);if(version===generation.current)setSavedPreset(key);});}
  async function convert(correction=false){if(!analysis)return;const version=generation.current;await operate('Model conversion did not complete',async()=>{const checked=await api.preparationReview(project,analysis,options);if(version!==generation.current)return;setIssues(checked);setReviewKey(key);if(checked.length)return;const confirmed={...options,reviewed:true,...(Object.keys(options.mapping_anchors??{}).length?{follow_motion_reviewed:true}:{})};const correctionBlend=result?.blend,conversionProject={...(correction&&result?{...project,preparation_blend:result.blend,preparation_report:result.report}:project),preparation_options:asOptions(confirmed)} as Project;setResult(null);setPreparedPreview(null);const value=await api.preparationConvert(conversionProject,analysis,confirmed,correction,correction?correctionBlend:undefined);if(version===generation.current)setResult(value);},true);}
  async function accept(){if(result)await operate('Prepared model could not be accepted',async()=>update(await api.preparationAccept(project,result)),true);}
  async function resume(){const version=++generation.current;await operate('Saved preparation could not be resumed',async()=>{const value=await api.preparationResume(project);if(version!==generation.current)return;setSource(value.analysis.source_original);setRotation(value.analysis.rotation);setAnalysis(value.analysis);inspectedWith.current=choicesKey(options);setMappingPreview(null);setPreparedPreview(null);setResult(value.result);setIssues(null);},true);}
  async function previewPrepared(){if(!result)return;const version=generation.current;await operate('Prepared model preview did not complete',async()=>{const value=decodePreview(await api.preview(result.dmx));if(version===generation.current)setPreparedPreview(value);},true);}
  const labels=analysis?.suggestions.assistance?.labels??{};
  const entries=useMemo(()=>{
    const assist=new Map((analysis?.suggestions.assistance?.entries??[]).map(e=>[e.source,e]));
    return (analysis?.analysis.weighted_bones??[]).map(name=>assist.get(name)??{source:name,target:null,group:'Other bones',label:name,reason:'Choose and review a target bone.',automatic:false} as MappingEntry);
  },[analysis]);
  const physics=physicsGroups(analysis?.analysis.bones??[],analysis?.analysis.weighted_bones??[],options),kept=keptPhysicsBones(physics,options);
  const bodyEntries=entries.filter(e=>!kept.has(e.source));
  const unmapped=bodyEntries.filter(e=>!options.mapping?.[e.source]).length,groups=[...new Set(entries.map(e=>e.group))].sort();
  const followingCount=Object.keys(options.mapping_anchors??{}).filter(n=>!kept.has(n)).length;
  const proposed=bodyEntries.filter(e=>e.follow&&!options.mapping?.[e.source]);
  const rolesChanged=Boolean(analysis)&&ROLES.some(role=>options.source_roles?.[role]!==analysis?.suggestions.source?.roles[role]||options.target_roles?.[role]!==analysis?.suggestions.target.roles[role]);
  const roleProblems=[...(analysis?.suggestions.source?.errors??[]).map(error=>`Source: ${error}`),...(analysis?.suggestions.target.errors??[]).map(error=>`Hero: ${error}`)];
  const allFollowing=rolesChanged?[]:pendingFollowing(options,bodyEntries);
  const pelvisFollowing=!rolesChanged&&options.target_roles?.pelvis?pendingFollowing(options,bodyEntries,options.target_roles.pelvis):[];
  const offsetUpgrades=bodyEntries.filter(e=>e.follow?.automatic&&options.mapping?.[e.source]===e.follow.target&&!options.mapping_anchors?.[e.source]);
  const proposalGroups=[...new Set(proposed.map(e=>e.follow!.group))].sort();
  const visible=entries.filter(e=>(filter==='all'||(filter==='following'?!kept.has(e.source)&&Boolean(options.mapping_anchors?.[e.source]):!kept.has(e.source)&&!options.mapping?.[e.source]))&&(group==='all'||e.group===group)&&`${e.source} ${e.label} ${options.mapping?.[e.source]??''}`.toLowerCase().includes(search.toLowerCase()));
  const pageCount=Math.max(1,Math.ceil(visible.length/30)),currentPage=Math.min(page,pageCount-1),warnings=roleWarnings(options);
  const boneLabel=(name:string)=>labels[name]?`${name} — ${labels[name]}`:name;
  const [panel,setPanel]=useState<PrepPanel>('roles');
  const [roleGroup,setRoleGroup]=useState('all');
  const weaponOnly=project.model_input_mode==='weapon',weaponTab=weaponOnly||(analysis?.analysis.weapons?.length??0)>0||(Boolean(analysis)&&project.model_input_mode!=='prepared');
  const activePanel:PrepPanel=weaponOnly?(panel==='review'?'review':'weapon'):project.model_input_mode!=='rigged_humanoid'&&(panel==='physics'||panel==='mapping')?'roles':panel==='weapon'&&!weaponTab?'roles':panel;
  const shownRoles=ROLES.filter(role=>roleGroup==='all'||(roleGroup==='torso'?!role.includes('_'):roleGroup==='arms'?/upperarm|forearm|hand/.test(role):/thigh|shin|foot/.test(role)));
  const hero=project.hero_name||basename(project.original_model).replace(/\.vmdl(_c)?$/,'');
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
  const prepared=project.preparation_report&&project.dmx;
  const usedResult=Boolean(result)&&project.dmx===result?.dmx;
  return <div className="prep-workspace">
    <header className="prep-heading"><div><span className="prep-eyebrow">STEP 2 OF 5</span><h1>Prepare model<span className="prep-hero-chip">{hero}</span></h1><p>Your model is fitted to {hero}'s skeleton so it moves with the hero's animations. Most models are ready in one click.</p></div><button className="prep-secondary" disabled={busy||!project.preparation_report} onClick={()=>void resume()}><PrepIcon name="refresh" size={17}/>Reopen the last preparation</button></header>
    {!analysis&&project.dmx&&<section className="prep-card prep-done" role="status"><PrepIcon name="check"/><div><strong>{prepared?'Your model is prepared.':'Your model is already game-ready (DMX), so it does not need preparing.'}</strong><p className="hint">{prepared?`${basename(project.dmx)} is in use. Prepare again only to change the fit, the gun or the moving parts.`:'You can go straight to Appearance & weapons.'}</p></div><div className="row">{next&&<button className="prep-primary" disabled={busy} onClick={next}>Continue to Appearance & weapons<PrepIcon name="arrow" size={18}/></button>}{prepared&&<button className="prep-secondary" disabled={busy} onClick={()=>void resume()}>Change the preparation</button>}</div></section>}
    <nav className="prep-steps" aria-label="Preparation workflow"><button aria-current={!analysis?'step':undefined} onClick={()=>jump('prep-source')}><b>{analysis?<PrepIcon name="check" size={14}/>:1}</b><span>Your model<small>Load and check</small></span></button><i/><button disabled={!analysis} aria-current={analysis&&!reviewed&&!result?'step':undefined} onClick={()=>{setPanel('roles');jump('prep-review-workspace');}}><b>2</b><span>Check the fit<small>Joints, moving parts and gun</small></span></button><i/><button disabled={!analysis} aria-current={reviewed&&!result?'step':undefined} onClick={()=>{setPanel('review');jump('prep-review-workspace');}}><b>3</b><span>Prepare<small>One click</small></span></button><i/><button disabled={!result} aria-current={result?'step':undefined} onClick={()=>jump('prep-output')}><b>4</b><span>Use it<small>Preview the result</small></span></button></nav>
    <Section title="Your model" className={`prep-card prep-source ${analysis?'is-inspected':''}`} id="prep-source" actions={analysis?<span className="prep-badge"><PrepIcon name="check" size={14}/>Checked</span>:<span className="prep-file-types">BLEND · GLB · glTF · FBX</span>}>
      <CompactPathField label="Model file" value={source} disabled={busy} update={sourceChanged} browse={browse} placeholder="Choose a Blender, GLB, glTF or FBX file…"/>
      <Disclosure title="Model orientation" hint="Optional: turn the model before checking it (lying-down models are turned upright automatically)"><div className="override-grid">{(['X','Y','Z'] as const).map((axis,index)=><NumberField key={axis} label={`Source rotation ${axis} (degrees)`} value={rotation[index]} min={-360} max={360} step={1} disabled={busy} update={value=>{const next=[...rotation] as Vec3;next[index]=value;invalidate();setRotation(next);setAnalysis(null);setMappingPreview(null);}}/>)}</div></Disclosure>
      <div className="prep-source-actions"><button className="prep-primary" disabled={busy||!source||!project.original_model} onClick={()=>{setPanel('roles');void inspect(true);}}><PrepIcon name="review" size={18}/>Prepare automatically</button><button className="prep-secondary" disabled={busy||!source||!project.original_model} onClick={()=>{setPanel('roles');void inspect();}}>Load and check only</button><button disabled={busy||!source} onClick={()=>void loadPreview()}><PrepIcon name="eye" size={18}/>Preview the model and animation</button></div>
      <p className="hint">{project.original_model?'Prepare automatically checks your model, matches its joints to the hero and prepares it in one go; it stops only if something needs your choice. Your file is never changed.':'Choose the hero in Character & model first.'}</p>
      {reinspectReason&&<p className="hint" role="status">{reinspectReason==='gun'?'Loading the gun file…':'Checking your model again with the new Scene objects choices…'}</p>}
    </Section>
    {preview&&<Section title="Source preview" className="prep-card prep-source-preview" actions={<button onClick={()=>setPreview(null)}>Close source preview</button>}><Suspense fallback={<p>Loading source renderer…</p>}><GltfViewport preview={preview}/></Suspense></Section>}
    {analysis&&<>
      <Section title="What we found in your model" className="prep-inspection" actions={<button disabled={busy} onClick={()=>void suggestions()}><PrepIcon name="refresh" size={16}/>Re-detect body mapping</button>}>
        <div className="prep-inspection-summary"><div className="prep-inspection-message"><span className="prep-status-icon"><PrepIcon name="check" size={24}/></span><div><h3>Your model is checked</h3><p>{unmapped?`${unmapped} joint${unmapped===1?'':'s'} still need a match below. Everything else was matched automatically.`:'Every joint was matched automatically. Click Prepare model below, or look through the tabs first.'}</p></div></div><div className="prep-stats"><div><strong>{analysis.analysis.bones.length}</strong><span>Bones</span></div><div><strong>{analysis.analysis.meshes.length}</strong><span>Meshes</span></div><div><strong>{analysis.analysis.vertices.toLocaleString()}</strong><span>Vertices</span></div><div className={unmapped?'needs-attention':''}><strong>{unmapped}</strong><span>Need a target</span></div></div></div>
        <div className="prep-inspection-settings"><SelectField label="Preparation mode" value={project.model_input_mode} values={[["rigged_humanoid","Rigged humanoid"],["unrigged_humanoid","Unrigged humanoid"],["prepared","Prepared source"],...(project.model_input_mode==='weapon'||weaponLike(project.original_model)?[["weapon","Weapon model (gun only)"] as [string,string]]:[])]} onChange={model_input_mode=>{invalidate();update({...project,model_input_mode,preparation_options:asOptions({...options,reviewed:false})});}} disabled={busy}/><details className="prep-inspection-details"><summary><PrepIcon name="info" size={16}/>Inspection details & notes{Boolean(analysis.analysis.warnings?.length||analysis.suggestions.assistance?.warnings.length)&&<span className="prep-count">{(analysis.analysis.warnings?.length??0)+(analysis.suggestions.assistance?.warnings.length??0)}</span>}</summary>
          <p>Auto-map uses names, hierarchy, sides, joint positions and skin-region evidence. Existing direct choices are retained. Review the suggested assignments and any remaining groups.</p>
          {(analysis.analysis.warnings?.length??0)>0&&<ul aria-label="Source import notes">{analysis.analysis.warnings!.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}
          {(analysis.suggestions.assistance?.warnings.length??0)>0&&<ul aria-label="Suggestion checks">{analysis.suggestions.assistance!.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}
          <RawInspection value={analysis.analysis}/>
        </details></div>
        {analysis.analysis.scene&&<SceneObjects receipt={analysis.analysis.scene} choices={options.scene??{}} change={scene=>changeOptions({scene})} busy={busy} reinspect={()=>void inspect()}/>}
        {analysis.suggestions.assistance?.preset&&<p className="prep-preset-status" role="status"><PrepIcon name="check" size={15}/>{analysis.suggestions.assistance.preset}</p>}
      </Section>
      <div id="prep-review-workspace" className="prep-review-workspace">
        <PreparationTabs active={activePanel} change={showPanel} rigged={project.model_input_mode==='rigged_humanoid'} unmapped={unmapped} weapon={weaponTab} weaponOnly={weaponOnly}/>
        <div className="prep-review-layout"><div className="prep-editor-column">
          <PreparationPanel id="roles" active={activePanel}>
          <Section title="Humanoid roles" className="prep-card prep-roles-card"><p className="hint">Select a role to highlight its joint. Changing either bone also updates that role’s mapping.</p>
            {warnings.length>0&&<ul role="alert">{warnings.map(w=><li key={w}>{w}</li>)}</ul>}
            <div className="prep-role-filter" role="group" aria-label="Show body region">{[['all','All joints'],['torso','Torso & head'],['arms','Arms & hands'],['legs','Legs & feet']].map(([value,label])=><button key={value} aria-pressed={roleGroup===value} onClick={()=>setRoleGroup(value)}>{label}</button>)}</div>
            <div className="table-scroll prep-role-scroll"><table className="role-table"><thead><tr><th>Role</th><th>Source bone</th><th>Target bone</th></tr></thead><tbody>{shownRoles.map(role=><tr key={role} className={selectedRole===role?'selected-role':''}>
              <td><button className="tertiary" aria-pressed={selectedRole===role} onClick={()=>selectRole(role)}>{roleLabel(role)}</button></td>
              <td>{project.model_input_mode==='unrigged_humanoid'?<div className="vector-fields">{[0,1,2].map(index=><input key={index} type="number" step="0.01" aria-label={`${role} landmark ${'XYZ'[index]}`} value={options.landmarks?.[role]?.[index]??''} disabled={busy} onChange={event=>{if(!Number.isFinite(event.target.valueAsNumber))return;const point=[...(options.landmarks?.[role]??[0,0,0])];point[index]=event.target.valueAsNumber;changeOptions({landmarks:{...options.landmarks,[role]:point},reviewed:false});}}/>)}</div>:<select aria-label={`Source ${role}`} disabled={busy} value={options.source_roles?.[role]??''} onChange={event=>setRole('source',role,event.target.value)}><option value="">Choose a source bone…</option>{analysis.analysis.bones.map(bone=><option key={bone.name} value={bone.name}>{boneLabel(bone.name)}</option>)}</select>}</td>
              <td><select aria-label={`Target ${role}`} disabled={busy} value={options.target_roles?.[role]??''} onChange={event=>setRole('target',role,event.target.value)}><option value="">Choose a target bone…</option>{analysis.target.map(bone=><option key={bone.name} value={bone.name}>{bone.name}</option>)}</select></td>
            </tr>)}</tbody></table></div>
          </Section>
          </PreparationPanel>
          {project.model_input_mode==='rigged_humanoid'&&<PreparationPanel id="physics" active={activePanel}><PhysicsBoneChoices groups={physics} gunParts={physics.filter(g=>hangsFromGun(g.root,analysis.analysis.bones,options.source_roles??{})).map(g=>g.root)} options={options} change={changeOptions} busy={busy} breasts={{count:breastBones(analysis.analysis.bones,analysis.analysis.weighted_bones,options.source_roles??{}).length,toggle:enabled=>changeOptions(breastMotionOptions(analysis.analysis.bones,analysis.analysis.weighted_bones,options,enabled)),added:Boolean(analysis.breast_generation),result:analysis.analysis.breast_bones,reinspect:()=>void inspect()}}/></PreparationPanel>}
          {project.model_input_mode==='rigged_humanoid'&&<PreparationPanel id="mapping" active={activePanel}>
          {project.model_input_mode==='rigged_humanoid'&&<Section title="Weighted bone mapping" className="prep-card prep-mapping-card">
            <p><strong>{unmapped}</strong> weighted bones still need a target. Every weighted bone must be resolved before conversion.</p>
            <div className="mapping-proposals"><div className="row">
              <button disabled={busy||!pelvisFollowing.length} onClick={()=>changeOptions(applyFollowing(options,pelvisFollowing))}>Follow pelvis, keep offset for all ({pelvisFollowing.length})</button>
              <button disabled={busy||!allFollowing.length} onClick={()=>changeOptions(applyFollowing(options,allFollowing))}>Follow suggested joints, keep offsets for all ({allFollowing.length})</button>
            </div><p className="hint">Applies to remaining suggestions across every page, including hidden rows. Existing assignments stay unchanged. The pelvis button covers pelvis suggestions; the other button uses each bone’s suggested joint. Separate hair, cloth and facial motion is not reproduced.</p>
            {rolesChanged?<div role="status"><p>Body roles changed. Refresh suggestions to use your current choices.</p><button disabled={busy} onClick={()=>void suggestions(false)}>Refresh suggestions using my roles</button></div>:roleProblems.length>0?<div role="status"><p>Follow suggestions are waiting for valid body roles.</p><ul>{roleProblems.map((problem,index)=><li key={index}>{problem}</li>)}</ul><button onClick={()=>showPanel('roles')}>Review body roles</button><button disabled={busy} onClick={()=>void suggestions(false)}>Refresh suggestions using my roles</button></div>:allFollowing.length===0?<p className="hint">{unmapped?'No remaining bones have a verified body joint to follow. Review their targets below or preserve eligible moving parts in Physics bones.':'All weighted bones already have a target or are kept as independent bones.'}</p>:null}
            </div>
            {followingCount>0&&<p className="hint">{followingCount} mappings preserve offsets while following a body joint. Their original independent movement is not reproduced.</p>}
            {offsetUpgrades.length>0&&<div className="mapping-proposals"><p>{offsetUpgrades.length} existing helper mappings share a body joint without offset protection.</p><button disabled={busy} onClick={()=>changeOptions(applyFollowing(options,offsetUpgrades,true))}>Preserve offsets for {offsetUpgrades.length} existing helper mappings</button><p className="hint">Keeps your chosen targets and prevents these helpers from pulling the rest geometry toward a shared joint. Review the converted shape afterward.</p></div>}
            {proposalGroups.length>0&&<div className="mapping-proposals"><h3>Review remaining groups</h3><p className="hint">These bones have no established equivalent on this hero. You can make a group follow its mapped ancestor, preserving its rest offset. Hair and clothing will have no new simulation; fingers and face joints lose their separate articulation. Leave a group unresolved if that is not acceptable.</p>
              <div className="mapping-proposal-list">{proposalGroups.map(g=>{const candidates=proposed.filter(e=>e.follow!.group===g),valid=!rolesChanged&&candidates.every(e=>options.mapping?.[e.follow!.anchor]===e.follow!.target);return <div className="mapping-proposal" key={g}><strong>{g} · {candidates.length} bones</strong><div className="row"><button disabled={busy} onClick={()=>{setSearch(candidates[0].source);setPage(0);setGroup('all');setFilter('unmapped');setSelectedBone(candidates[0].source);}}>Show first bone</button><button disabled={busy||!valid} onClick={()=>changeOptions(applyFollowing(options,candidates))}>Make {candidates.length} bones follow {candidates[0].follow!.target}</button></div>{!valid&&<p className="hint">Roles changed. Refresh suggestions using your current roles.</p>}</div>;})}</div>
            </div>}
            <div className="mapping-filters"><label>Show<select value={filter} onChange={e=>{setFilter(e.target.value);setPage(0);}}><option value="unmapped">Needs a target</option><option value="following">Body-following mappings</option><option value="all">All mappings</option></select></label><label>Group<select value={group} onChange={e=>{setGroup(e.target.value);setPage(0);}}><option value="all">All groups</option>{groups.map(g=><option key={g}>{g}</option>)}</select></label><label>Find bone<input value={search} onChange={e=>{setSearch(e.target.value);setPage(0);}} placeholder="Name or English label"/></label></div>
            <details className="mapping-batch prep-bulk"><summary>Assign multiple bones<span>{selectedBones.size?`${selectedBones.size} selected`:'Optional bulk tools'}</span></summary><div className="prep-bulk-content">
              <div className="row"><button disabled={busy||!visible.length} onClick={()=>setSelectedBones(new Set(visible.filter(e=>!kept.has(e.source)).map(e=>e.source)))}>Select filtered bones ({visible.length})</button><button disabled={!selectedBones.size} onClick={()=>setSelectedBones(new Set())}>Clear selection</button><span>{selectedBones.size} selected</span></div>
              <label>Target for selected bones<select aria-label="Target for selected bones" disabled={busy||!selectedBones.size} value={batchTarget} onChange={e=>setBatchTarget(e.target.value)}><option value="">Choose a shared target…</option>{analysis.target.map(b=><option key={b.name} value={b.name}>{b.name}</option>)}</select></label>
              <button disabled={busy||!selectedBones.size||!batchTarget} onClick={()=>{changeOptions(assignTargets(options,[...selectedBones].filter(n=>!kept.has(n)),batchTarget));setSelectedBones(new Set());}}>Assign {batchTarget||'target'} to {selectedBones.size} selected bones</button>
              <p className="hint">Use a shared target only when you intend these bones to move together. Merging joints can change bind geometry and removes their independent motion. Inspect the movement test and the converted model before accepting.</p>
            </div></details>
            <div className="table-scroll prep-mapping-scroll"><table className="mapping-table"><thead><tr><th>Select</th><th>Source bone</th><th>Original skeleton bone</th></tr></thead><tbody>{visible.slice(currentPage*30,(currentPage+1)*30).map(entry=><tr key={entry.source}>
              <td><label className="prep-bone-selection"><input type="checkbox" aria-label={`Select bone ${entry.source}`} checked={selectedBones.has(entry.source)} disabled={busy||kept.has(entry.source)} onChange={e=>setSelectedBones(current=>{const next=new Set(current);if(e.target.checked)next.add(entry.source);else next.delete(entry.source);return next;})}/><span className="sr-only">Select this bone</span></label></td>
              <td><button className="tertiary" aria-label={`Show bone ${entry.source}`} onClick={()=>setSelectedBone(entry.source)}>{entry.source}</button><small className="hint">{entry.label!==entry.source?`${entry.label} · `:''}{entry.group}</small></td>
              <td>{kept.has(entry.source)?<p className="hint">Kept as an independent bone: {SECONDARY_PREFIX}{entry.source}</p>:<><select aria-label={`Map ${entry.source}`} disabled={busy} value={options.mapping?.[entry.source]??''} onChange={event=>{setSelectedBone(entry.source);changeOptions(assignTargets(options,[entry.source],event.target.value));}}><option value="">Choose a target bone…</option>{analysis.target.map(bone=><option key={bone.name} value={bone.name}>{bone.name}</option>)}</select>
                {!options.mapping?.[entry.source]&&entry.target&&<button disabled={busy} onClick={()=>changeOptions(assignTargets(options,[entry.source],entry.target!))}>Use proposed {entry.target}</button>}
                {!options.mapping?.[entry.source]&&entry.follow&&<button disabled={busy||rolesChanged||options.mapping?.[entry.follow.anchor]!==entry.follow.target} onClick={()=>changeOptions(applyFollowing(options,[entry]))}>Follow {entry.follow.target}, keep offset</button>}
                <small className="hint">{options.mapping_anchors?.[entry.source]?`Follows ${options.mapping_anchors[entry.source]} with rest offset preserved. Independent source motion is not transferred.`:entry.reason}</small></>}
              </td>
            </tr>)}</tbody></table></div>
            {!visible.length&&<p className="quiet-empty">No bones match this filter.</p>}
            {pageCount>1&&<div className="row"><button disabled={currentPage===0} onClick={()=>setPage(currentPage-1)}>Previous bones</button><span>{currentPage+1} / {pageCount}</span><button disabled={currentPage+1>=pageCount} onClick={()=>setPage(currentPage+1)}>Next bones</button></div>}
          </Section>}
          </PreparationPanel>}
          {weaponTab&&<PreparationPanel id="weapon" active={activePanel}><WeaponPanel project={project} analysis={analysis} options={options} change={values=>changeOptions(analysis?gunChoiceOptions(analysis.analysis.bones,analysis.analysis.weighted_bones,options,values):values)} busy={busy} operate={operate} reinspect={()=>void inspect()} separate={{projectPath,saveProject,updateProject:update}}/></PreparationPanel>}
          <PreparationPanel id="review" active={activePanel}>
      <Section title="Conversion review" className="prep-card prep-review-card"><p className="prep-section-intro">Choose the fit, confirm your review, then check and convert using the action bar below.</p><div className="prep-fit-settings">{!weaponOnly&&<>
        <SelectField label="Arm skinning" value={options.arm_skinning??'source'} values={[["smooth_twist","Smooth twist weights (reference style)"],["source","Keep mapped source weights"]]} disabled={busy} onChange={value=>changeOptions({arm_skinning:value as PreparationOptions['arm_skinning'],reviewed:false})}/>
        <details className="prep-option-help"><summary>About arm skinning</summary><p className="hint">Smooth twist weights spread arm movement across the hero’s existing twist joints. Joint reach and mesh shape stay the same. Heroes without suitable twist joints keep their source weights. Convert the original source again after changing this choice.</p></details>
        <SelectField label="Model fitting" value={options.fit_mode??'match_target'} values={[["match_target","Match hero proportions (existing fit)"],["preserve_proportions","Preserve source proportions"]]} disabled={busy} onChange={value=>changeOptions({fit_mode:value as PreparationOptions['fit_mode'],reviewed:false})}/>
        <details className="prep-option-help"><summary>About model fitting</summary><p className="hint">{options.fit_mode==='preserve_proportions'?'Keeps the source torso, neck, head and leg proportions after overall scale and alignment. Arm fitting below can align the elbows and wrists for animation. Clothing follows its existing weights. Test joint bending, weapon grip and clipping in game.':'Fits body regions to the hero’s joint positions. This can change the source silhouette, including the neck and clothing.'} Review the result with Preview converted model after conversion. Changing this choice requires a new conversion from the original source.</p></details>
        {options.fit_mode==='preserve_proportions'&&<><Check label="Align arms to hero joints (recommended)" value={options.fit_arm_joints??true} disabled={busy} onChange={fit_arm_joints=>changeOptions({fit_arm_joints,reviewed:false})}/><p className="hint">Fits arm length to the hero’s shoulder, elbow and wrist positions while keeping limb width and torso shape. Turn off to retain original arm lengths; offset pivots can cause elbow clipping.</p></>}<Check label="Bend fingers like the hero (recommended)" value={options.fit_finger_curl??true} disabled={busy} onChange={fit_finger_curl=>changeOptions({fit_finger_curl,reviewed:false})}/><p className="hint">Bends your model’s fingers to the hero’s resting curl, so the hero’s animations close them around the gun. Finger lengths stay the same. Turn off to keep the fingers as they are in your file.</p></>}
        {Boolean(analysis.analysis.morphs?.length)&&<div className="editor-fields"><p><strong>{analysis.analysis.morphs.length} shape key{analysis.analysis.morphs.length===1?'':'s'} detected.</strong> Choose how to keep the model’s shape during conversion.</p>
          {analysis.geometry_customization?.face?<p className="hint">Your static expression from Appearance will be baked. Animated shape keys are not exported.</p>:<><SelectField label="Shape key handling" value={options.shape_key_mode??''} values={[["","Choose shape key handling…"],["current_static","Keep the saved shape (static)"],["basis_static","Use the base shape (static)"]]} disabled={busy} onChange={value=>changeOptions({shape_key_mode:value as PreparationOptions['shape_key_mode'],reviewed:false})}/><details className="prep-option-help"><summary>Static shape only · what is kept?</summary><p className="hint">Static conversion bakes one shape into the prepared model. Animated expressions and morphs are not exported; your original file keeps all shape keys. “Saved shape” uses the source file’s saved frame and values, not temporary preview changes.</p></details></>}
        </div>}
        </div><details className="prep-advanced"><summary>Advanced conversion options</summary><NumberField label="Maximum skin influences" value={options.influence_limit??4} min={1} max={4} step={1} disabled={busy} update={influence_limit=>changeOptions({influence_limit,reviewed:false})}/><p className="hint">Maximum number of bones influencing each vertex. The existing default is 4.</p></details><div className="prep-confirmations"><h3>What Prepare model confirms</h3><p>{weaponOnly?'Clicking Prepare model confirms the gun and its fit.':'Clicking Prepare model confirms the joint matches, the orientation and the moving parts shown in these tabs.'}{followingCount>0?` ${followingCount} small bones follow a body joint and lose their own movement (for example extra twist or cloth helpers).`:''}</p></div><div className="prep-preset"><button disabled={busy||!reviewed||project.model_input_mode!=='rigged_humanoid'} onClick={()=>void savePreset()}><PrepIcon name="folder" size={17}/>Save mapping preset</button>
        <p className="hint">A saved preset is offered automatically for the same source rig, weighted bone set, orientation and target rig. It stores choices; each conversion still requires review and validation.</p>
        {savedPreset===key&&<p role="status">Mapping preset saved for this rig pair.</p>}
        </div>
        {result&&<div className="prep-repeat-actions"><button disabled={busy} onClick={()=>void review()}>Check for problems</button><button disabled={busy} onClick={()=>void prepareModel()}>Prepare again</button></div>}
      </Section>
          </PreparationPanel>
        </div>
        <aside className="prep-preview-column" aria-label="Visual mapping review"><Section title="Mapping preview" className="prep-card prep-mapping-preview" actions={<button disabled={busy} onClick={()=>void loadPreview(true)}>{mappingPreview?'Reload model':'Load model for mapping review'}</button>}>
          {mappingPreview?<Suspense fallback={<p>Loading mapping renderer…</p>}><MappingViewport preview={mappingPreview} target={analysis.target} sourceBones={analysis.analysis.bones} options={options} selected={selectedRole} select={selectRole} selectedBone={selectedBone}/></Suspense>:<div className="prep-preview-empty"><div className="prep-preview-orbit"><PrepIcon name="roles" size={88}/></div><span className="prep-eyebrow">VISUAL MAPPING CHECK</span><h3>See how your joints line up</h3><p>Load the model, select a role, and try head, arm and knee movements with your current assignments.</p><small>Use Load model for mapping review above to open the viewer.</small></div>}
          {rotation.some(v=>v!==0)&&<p className="hint">The viewer shows the source’s original orientation. Conversion uses the inspected rotation: {rotation.join(', ')}°.</p>}
        </Section><div className="prep-preview-tip"><PrepIcon name="info" size={18}/><p>Check the joints here, then open <button onClick={()=>setPanel('review')}>Fit &amp; review</button> to check your choices and convert.</p></div></aside>
        </div>
      </div>
      {issues&&<section className={`prep-check-results ${issues.length?'has-issues':'passed'}`} id="prep-check-results" aria-label="Review results" aria-live="polite"><div className="prep-check-heading"><PrepIcon name={issues.length?'info':'check'}/><div><h2>{issues.length?`${issues.length} thing${issues.length===1?'':'s'} to fix before preparing`:reviewKey===key?'No problems found.':'Choices changed. Check again.'}</h2>{issues.length>0&&<p>Open the matching section, make your changes, then click Prepare model again.</p>}</div></div>{issues.length>0&&<ul>{issues.map((issue,index)=>{const section:PrepPanel=/mapping/i.test(issue.tab)?'mapping':/roles|landmark/i.test(issue.tab)?'roles':/physics|secondary/i.test(issue.tab)?'physics':/weapon|gun/i.test(issue.tab)?'weapon':'review';return <li key={index}><div><strong>{issue.tab}</strong><p>{issue.message}</p></div><button onClick={()=>{setPanel(section);jump('prep-review-workspace');}}>Open section<PrepIcon name="arrow" size={16}/></button></li>;})}</ul>}</section>}
    </>}
    {result&&<Section title="Prepared output" className="prep-card prep-output" id="prep-output"><div className="prep-output-ready"><PrepIcon name="check"/><div><strong>Your converted model is ready to preview</strong><p>Inspect the result, then use it in your project.</p></div></div><details className="prep-output-files"><summary>Prepared files<PrepIcon name="folder" size={16}/></summary><dl className="summary-list"><dt>Model</dt><dd><PathValue path={result.dmx}/></dd><dt>Editable Blender file</dt><dd><PathValue path={result.blend}/></dd><dt>Preparation report</dt><dd><PathValue path={result.report}/></dd></dl></details>{result.warnings.length>0&&<ul>{result.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}<p className="hint">Preview the actual converted rest geometry here. Prepared geometry still requires native compiler validation and an in-game test.</p><div className="prep-output-actions"><button className="prep-primary" disabled={busy} onClick={()=>void previewPrepared()}>Preview converted model</button><button disabled={busy||project.preparation_blend!==result.blend} onClick={()=>void operate('Blender could not be opened',()=>api.preparationOpenBlend(project))}>Open accepted Blender file</button><button disabled={busy||!analysis} onClick={()=>void convert(true)}>Re-export corrections</button></div>{preparedPreview&&<Suspense fallback={<p>Loading converted model…</p>}><Viewport scene={preparedPreview} project={project} disabled={busy}/></Suspense>}{!usedResult&&<p className="hint">Use {basename(result.dmx)} to store this preparation in the project before opening its editable file.</p>}</Section>}
    {analysis&&<footer className="prep-action-bar"><div className="prep-action-status"><span className={`prep-status-icon ${reviewed||result?'ready':''}`}><PrepIcon name={result||reviewed?'check':'review'} size={21}/></span><div><strong>{usedResult?'Your prepared model is in use':result?'Prepared model available':issues?.length?'Something needs your choice':'Ready to prepare'}</strong><p>{usedResult?'Continue with materials and weapons, or preview the result below.':result?'Preview it, then use it in your project.':issues?.length?'Fix the items listed above, then click Prepare model again.':'Prepare model checks your choices, converts the model and uses it in one click.'}</p></div></div><div className="prep-main-actions">{usedResult?<><button onClick={()=>jump('prep-output')}>Preview the result</button>{next&&<button className="prep-primary" disabled={busy} onClick={next}>Continue to Appearance & weapons<PrepIcon name="arrow" size={18}/></button>}</>:result?<><button onClick={()=>jump('prep-output')}>View prepared output</button><button className="prep-primary" disabled={busy} onClick={()=>void accept()}>Use this model<PrepIcon name="arrow" size={18}/></button></>:<><button className="prep-quiet" onClick={()=>{setPanel('review');jump('prep-review-workspace');}}>Fit &amp; review</button><button className="prep-secondary" disabled={busy} onClick={()=>void review()}><PrepIcon name="review" size={17}/>Check for problems</button><button className="prep-primary" disabled={busy} onClick={()=>void prepareModel()}>Prepare model<PrepIcon name="arrow" size={18}/></button></>}</div></footer>}
  </div>;
}
