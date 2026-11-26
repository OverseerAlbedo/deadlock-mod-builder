import { fineTuneChip, MotionFineTune, motionStatusLine, MotionStatus, motionTabWords, MotionTools, MovingParts, PreviewHint, previewHintShown, preparedMotionRoots, type MotionProps } from './SecondaryMotion';
import { PreviewLinkContext, type PreviewActions, type PreviewLink, type PreviewState } from './previewLink';
import { ImagePreview } from './ImagePreview';
import { DestinationPicker, type ComponentCatalog } from './AssetCatalog';
import { useId, useMemo, useRef, useState, type ReactNode } from 'react';
import { StudioActionBar, StudioCard, StudioFrame, StudioTabPanel, StudioTabs, StudioTopBar, type StudioTab } from '../studio/Studio';
import { PhysicsLabelsContext, type PhysicsLabels } from './physicsLabels';
import type { TuningKind } from './MotionTuning';
import type { AssetKind, ClothingChoices, ClothSetupChoices, ModelSummary, PathKind, PhysicsChoices, SecondaryMotionChoices, Project, ResourceReplacement } from '../models';
import { api } from '../api';
import { FilePathInput } from './FilePathInput';
import { Disclosure, Section, StateLabel } from './Primitives';
import { PathValue } from './Paths';
import { modelEnabled, testedPhysicsEnabled } from '../build/readiness';
import type { ProjectInspection } from '../build/models';
import { includedClothHero, qualifiedIncludedCloth } from '../cloth/includedSetup';
import { ExtrasWorkspace, type ExtrasProps } from '../extras/ExtrasPage';
import { changeCustomization, changeSecondaryMotion } from './customization';
export { changeCustomization } from './customization';
import { fill, m, mc, tc, useT } from '../i18n';
// i18n-area: physics

type Props={catalog?:ComponentCatalog|null;discover?:()=>void;project:Project;update:(project:Project)=>void;disabled:boolean;fail:(title:string,error:unknown)=>void};
export function Check({label,value,onChange,disabled}:{label:string;value:boolean;onChange:(value:boolean)=>void;disabled:boolean}){
  return <label className="checkbox checkbox-line"><input type="checkbox" checked={value} disabled={disabled} onChange={event=>onChange(event.target.checked)}/>{label}</label>;
}
export function SelectField({label,value,values,onChange,disabled}:{label:string;value:string;values:readonly(readonly[string,string])[];onChange:(value:string)=>void;disabled:boolean}){
  const id=useId();return <div className="field"><label htmlFor={id}>{label}</label><select id={id} value={value} disabled={disabled} onChange={event=>onChange(event.target.value)}>{values.map(([key,text])=><option value={key} key={key}>{text}</option>)}</select></div>;
}
type PhysicsTab='mode'|'finetune'|'parts'|'cloth';
const MODE_NAMES={preserve:m('Preserve original physics'),replacement:m('Replacement cloth setup'),automatic:m('Automatic motion')} as const;
/** Step 4: the studio frame. The top bar (mode, status, the motion tools), four tabs (Physics mode,
 * Fine-tune, Moving parts, Tested cloth setup) beside the one character preview, and the action
 * bar. The preview always sits in the same slot (same parent, same place, same element), so
 * switching the tab or the physics mode never remounts it and it keeps its Still/Movement state.
 * Inactive tab panels stay mounted (hidden), so what they hold survives a tab switch. Nothing here
 * runs Analyze or loads the movement preview. */
export function PhysicsPage({project,update,disabled,fail,summary,children,report=null,fresh=false,openPreparation,preview,next,expert=true,tangtangPresets=true,vindictaReference=true}:{expert?:boolean;/** Whether this build carries the TangTang presets. */tangtangPresets?:boolean;/** Whether the included Vindicta cloth reference is installed with this build. */vindictaReference?:boolean;report?:ProjectInspection|null;fresh?:boolean;summary:ModelSummary|null;children?:ReactNode;openPreparation?:()=>void;/** The 3D view of the prepared character. */preview?:ReactNode;/** Continue to Build & install. */next?:()=>void}&Props){
  const t=useT();
  const physics=project.customization.physics??{},cloth=project.customization.cloth_setup??{},clothing=project.customization.clothing??{};
  const physicsUpdate=(values:Partial<PhysicsChoices>)=>update(changeCustomization(project,{physics:{...physics,...values,hero_path:project.original_model},...(values.enabled?{secondary_motion:{...project.customization.secondary_motion,enabled:false}}:{})}));
  // What the one preview shows (reported by it through PreviewLinkContext), for the line above the sliders.
  const [previewState,setPreviewState]=useState<PreviewState|null>(null);
  const previewActions=useRef<PreviewActions>({});
  const previewLink=useMemo<PreviewLink>(()=>({report:setPreviewState,actions:previewActions}),[]);
  // Raised when a slider changes while the line asks for something: the line highlights once.
  const [flash,setFlash]=useState(0);
  const secondaryUpdate=(values:Partial<SecondaryMotionChoices>)=>{if('tuning' in values&&previewHintShown(previewState))setFlash(n=>n+1);update(changeSecondaryMotion(project,values));};
  const preserve=()=>update(changeCustomization(project,{physics:{...physics,enabled:false},secondary_motion:{...project.customization.secondary_motion,enabled:false}}));
  const clothUpdate=(values:Partial<ClothSetupChoices>)=>update(changeCustomization(project,{cloth_setup:{...cloth,...values}}));
  const clothingUpdate=(values:Partial<ClothingChoices>)=>update(changeCustomization(project,{clothing:{...clothing,...values}}));
  async function included(){try{const choice=await api.includedPhysics(project.original_model);update(changeCustomization(project,{physics:{...physics,...choice},secondary_motion:{...project.customization.secondary_motion,enabled:false}}));}catch(error){fail(m('Could not select the included cloth setup'),error);}}
  const meshes=summary?.path===project.dmx?summary.meshes.map(mesh=>mesh.name):[];
  const selected=cloth.meshes?.[0]??'', tested=testedPhysicsEnabled(project);
  const wanted=Boolean(project.customization.enabled&&physics.enabled),generated=Boolean(project.customization.enabled&&project.customization.secondary_motion?.enabled);
  const includedAvailable=includedClothHero(project)&&vindictaReference, qualified=qualifiedIncludedCloth(project,report,fresh);
  const mode=generated?'automatic':wanted?'replacement':'preserve';
  const motion:MotionProps={expert,tangtangPresets,project,update:secondaryUpdate,disabled,report,fresh,openPreparation,preparation:values=>update({...project,preparation_options:{...project.preparation_options,...values}})};
  // Automatic mode opens on Fine-tune, the others on Physics mode; the choice is kept while the page stays mounted.
  const [tab,setTab]=useState<PhysicsTab>(()=>generated?'finetune':'mode');
  const [kind,setKind]=useState<TuningKind|null>(null);
  const [highlight,setHighlight]=useState<{kind:string;at:number}|null>(null);
  const base=useId().replace(/:/g,'');
  const activeKind=generated?fineTuneChip(project,fresh?report:null,kind):null;
  // A label on the character: Fine-tune on that part type, or (on Moving parts) its first part marked.
  const pick=useRef<(kind:string)=>void>(()=>undefined);
  // A type Fine-tune does not offer (known only from an older Analyze) is shown on Moving parts instead.
  pick.current=picked=>{const chip=fineTuneChip(project,fresh?report:null,picked as TuningKind)===picked;if(tab==='parts'||!chip){setHighlight({kind:picked,at:Date.now()});setTab('parts');}else{setKind(picked as TuningKind);setTab('finetune');}};
  const labelsActive=tab==='finetune'?activeKind:null;
  const labels=useMemo<PhysicsLabels|null>(()=>generated?{active:labelsActive,select:picked=>pick.current(picked)}:null,[generated,labelsActive]);
  const words=motionTabWords(motion);
  const replacementState=tested&&physics.source?t('Replacement reference selected'):!modelEnabled(project)?t('Replacement model required'):t('Choose a tested reference');
  const status=mode==='automatic'?motionStatusLine(motion):mode==='replacement'?replacementState:t('The hero’s own hair and cloth move; your model’s own parts stay still');
  const tabs:StudioTab<PhysicsTab>[]=[
    {id:'mode',label:t('Physics mode'),status:mode==='automatic'?tc('physics mode','Automatic'):mode==='replacement'?tc('physics mode','Replacement'):tc('physics mode','Preserve'),tone:'ok'},
    {id:'finetune',label:t('Fine-tune'),...(generated?words.finetune:{status:t('Automatic only'),tone:'quiet' as const})},
    {id:'parts',label:t('Moving parts'),...(generated?words.parts:{status:t('Automatic only'),tone:'quiet' as const})},
    {id:'cloth',label:t('Tested cloth setup'),...(wanted?(tested&&physics.source?{status:t('in use'),tone:'ok' as const}:{status:t('choose one'),tone:'attention' as const}):{status:physics.source?t('saved, not used'):t('not used'),tone:'quiet' as const})},
  ];
  const hint=preview?<PreviewHint state={previewState} actions={previewActions} next={next} flash={flash}/>:null;
  /** The card a tab shows outside Automatic mode (`what`: the tab, Fine-tune or Moving parts). */
  const automaticOnly=(what:'Fine-tune'|'Moving parts')=><StudioCard title={t(what)}><p className="studio-note">{what==='Fine-tune'
    ?(mode==='replacement'?t('Fine-tune applies to Automatic secondary motion only. Physics mode is Replacement cloth setup now.'):t('Fine-tune applies to Automatic secondary motion only. Physics mode is Preserve original physics now.'))
    :(mode==='replacement'?t('Moving parts applies to Automatic secondary motion only. Physics mode is Replacement cloth setup now.'):t('Moving parts applies to Automatic secondary motion only. Physics mode is Preserve original physics now.'))}</p><button type="button" disabled={disabled} onClick={()=>secondaryUpdate({enabled:true})}>{t('Use Automatic secondary motion')}</button></StudioCard>;
  const action=mode==='automatic'
    ?(!preparedMotionRoots(project).length?{tone:'attention' as const,title:t('No moving parts to build yet'),text:t('Keep hair, cloth and tail bones in Prepare model, then use the prepared model.')}
      :fresh&&report&&'dmx' in report&&report.secondary_motion?{tone:'ok' as const,title:t('The last Analyze checked these moving parts'),text:t('Slider changes show in the Movement view right away and are used by the next build. No proxy export or manual file copying is needed.')}
      :{tone:'quiet' as const,title:t('Analyze to check the moving parts'),text:t('Run Analyze, then Build. No proxy export or manual file copying is needed.')})
    :mode==='replacement'?(tested&&physics.source?{tone:'ok' as const,title:t('Tested cloth setup chosen'),text:t('Analyze and Build check it against your model before installing.')}:{tone:'attention' as const,title:t('Choose a tested cloth setup'),text:t('Set it up on the Tested cloth setup tab, or choose another physics mode.')})
    :{tone:'quiet' as const,title:t('The hero keeps its own hair and cloth movement'),text:t('Nothing to set up here. Your model’s own hair and clothes stay still.')};
  return <StudioFrame label={t('Physics & preview')} className={`physics-studio motion-studio ${generated?'automatic':''}`}>
    <h1 className="sr-only">{t('Physics & preview')}</h1>
    <StudioTopBar title={project.name||t('Your project')} badge={t(MODE_NAMES[mode])} status={status}>{generated&&<MotionTools {...motion}/>}</StudioTopBar>
    <StudioTabs label={t('Physics & preview sections')} base={base} tabs={tabs} active={tab} onSelect={setTab}/>
    <div className={`motion-studio-grid ${preview?'':'no-preview'}`}>
      <div className="motion-studio-preview"><PreviewLinkContext.Provider value={previewLink}><PhysicsLabelsContext.Provider value={labels}>{preview}</PhysicsLabelsContext.Provider></PreviewLinkContext.Provider></div>
      <div className="motion-studio-panels">
        <StudioTabPanel base={base} id="mode" active={tab==='mode'}>
          <StudioCard title={t('How should hair, cloth and other parts move?')} sub={t('Choose one. You can change it at any time; the next build uses it.')}>
            <fieldset className="mode-options"><legend className="sr-only">{t('Physics mode')}</legend>
              <label className={`mode-option ${!wanted&&!generated?'selected':''}`}><input type="radio" name="physics-mode" checked={!wanted&&!generated} disabled={disabled} onChange={preserve}/><span><strong>{t('Preserve original physics')} <small>{t('Default')}</small></strong><span>{t('Keep the hero’s own hair and cloth movement. Your model’s own hair and clothes stay still in this mode.')}</span></span></label>
              <label className={`mode-option ${wanted?'selected':''}`}><input type="radio" name="physics-mode" checked={wanted} disabled={disabled} onChange={()=>physicsUpdate({enabled:true})}/><span><strong>{t('Replacement cloth setup')}</strong><span>{t('Use a tested hair and cloth setup made for your replacement model. Set it up on the Tested cloth setup tab.')}</span></span></label>
              <label className={`mode-option ${generated?'selected':''}`}><input type="radio" name="physics-mode" checked={generated} disabled={disabled} onChange={()=>secondaryUpdate({enabled:true})}/><span><strong>{t('Automatic secondary motion')}</strong><span>{t('Generate movement for the replacement’s preserved hair, ears, tail and clothing bones. Fine-tune and Moving parts apply to this mode.')}</span></span></label>
            </fieldset>
            {!modelEnabled(project)&&<p className="hint">{t('This project keeps the original model. Enable the replacement model in Extras to use tested cloth physics.')}</p>}
            {!project.customization.enabled&&physics.enabled&&<p className="hint">{t('A tested setup is saved but inactive while project customization is off.')}</p>}
            {generated&&<MotionStatus {...motion}/>}
          </StudioCard>
        </StudioTabPanel>
        <StudioTabPanel base={base} id="finetune" active={tab==='finetune'}>
          {generated?<StudioCard className="finetune-card"><MotionFineTune {...motion} chosen={kind} choose={setKind} previewHint={hint}/></StudioCard>:automaticOnly('Fine-tune')}
        </StudioTabPanel>
        <StudioTabPanel base={base} id="parts" active={tab==='parts'}>
          {generated?<StudioCard title={t('Moving parts')}><MovingParts {...motion} highlight={highlight} previewHint={hint}/></StudioCard>:automaticOnly('Moving parts')}
        </StudioTabPanel>
        <StudioTabPanel base={base} id="cloth" active={tab==='cloth'}>
          {qualified&&<div className="included-cloth-summary"><StateLabel tone="ready">{t('Vindicta balanced cloth')}</StateLabel><p>{t('Included hair and skirt reference · source binding checked. Compiled output is validated during build.')}</p></div>}
          <StudioCard title={t('Configure replacement cloth')} sub={t('A compiled hair and cloth setup made for your replacement model: the tested compiled reference and its activation setting.')}>
            {!wanted&&<p className="studio-note">{t('Used when Physics mode is Replacement cloth setup.')}</p>}
            {wanted&&physics.source&&<div className="cloth-selection-summary"><PathValue path={physics.source}/></div>}
            {includedAvailable&&<div className="included-cloth-choice"><div><strong>{t('Vindicta balanced cloth')}</strong><p className="hint">{t('For the matching Vindicta R12 replacement. Choosing this reference does not establish compatibility; Analyze project checks the source and build checks compiled output.')}</p></div><button disabled={disabled||!project.dmx} onClick={()=>void included()}>{t('Use included Vindicta cloth setup')}</button>{!project.dmx&&<p className="hint">{t('Choose a replacement model first.')}</p>}</div>}
            <Check label={t('Use tested hair and cloth physics')} value={physics.enabled??false} onChange={enabled=>physicsUpdate({enabled})} disabled={disabled}/>
            <FilePathInput label={t('Tested compiled model')} name="Tested compiled model" value={physics.source??''} kind="physics" update={source=>physicsUpdate({source})} disabled={disabled} fail={fail}/>
            <p className="hint">{t('The build verifies matching bones, rest poses, rigid bodies and cloth bindings before installing.')}</p>
          </StudioCard>
          <Disclosure title={t('Advanced cloth authoring')} hint={t('Create and inspect a new cloth candidate in a separate test workspace')}>
            <p className="section-description">{t('Automatic authoring prepares a candidate for ModelDoc testing. It does not enable new in-game cloth by itself.')}</p>
            <Section title={t('Cloth preview choices')}><SelectField label={t('Cloth mesh')} value={selected} values={[["",t('Choose a model mesh…')],...[...new Set([...meshes,...(selected?[selected]:[])])].map(name=>[name,meshes.includes(name)?name:t('{name} (saved, not checked)',{name})] as const)]} onChange={name=>clothUpdate({meshes:name?[name]:[]})} disabled={disabled}/><div className="field-grid"><SelectField label={t('Attachment edge')} value={cloth.attachment??'top'} values={[["top",t('Keep the top edge attached')],["bottom",t('Keep the bottom edge attached')]]} onChange={attachment=>clothUpdate({attachment:attachment as ClothSetupChoices['attachment']})} disabled={disabled}/><SelectField label={t('Proposed collision')} value={cloth.collision??'legs'} values={[["legs",t('Leg collision')],["none",t('No collision — preview only')]]} onChange={collision=>clothUpdate({collision:collision as ClothSetupChoices['collision']})} disabled={disabled}/></div></Section>
            {children}
          </Disclosure>
          <Disclosure title={t('Additional garment')} hint={clothing.enabled?t('Garment included in saved choices'):t('Optional · add a separate garment')} open={clothing.enabled&&!clothing.source?true:undefined}>
            <Check label={t('Include an added garment')} value={clothing.enabled??false} onChange={enabled=>clothingUpdate({enabled})} disabled={disabled}/>{clothing.enabled&&<div className="editor-fields"><FilePathInput label={t('Garment model')} name="Garment model" kind="garment" value={clothing.source??''} update={source=>clothingUpdate({source})} disabled={disabled} fail={fail}/><SelectField label={t('Garment movement')} value={clothing.mode??'weighted'} values={[["weighted",t('Follow the character’s bones')],["simulated",t('Independent cloth simulation')]]} onChange={mode=>clothingUpdate({mode:mode as ClothingChoices['mode']})} disabled={disabled}/><p className="hint">{clothing.mode==='simulated'?t('Author and test simulation in ModelDoc. Once the garment is part of the matching replacement DMX, select its tested compiled reference above.'):t('Fit the garment to the replacement before preparation. Binding, fitting, and build qualification remain separate operations.')}</p></div>}
          </Disclosure>
        </StudioTabPanel>
      </div>
    </div>
    <StudioActionBar tone={action.tone} title={action.title} text={action.text}>{next&&<button type="button" className="primary" onClick={next}>{t('Continue to Build & install')} <span aria-hidden="true">→</span></button>}</StudioActionBar>
  </StudioFrame>;
}
/** Each row kind's texts (English; shown with `t`): its title, its empty line, its hint and the
 * labels that name a row by its number. */
const assetSpec:Record<AssetKind,{title:string;empty:string;add:string;include:string;file:string;pathKind:PathKind;hint:string}>={
  icons:{title:m('Icons'),empty:m('No replacement icon'),add:m('Add icon'),include:m('Include icon {index}'),file:m('New icon {index}'),pathKind:'icon',hint:m('Images replace existing game artwork after format and size checks.')},
  animations:{title:m('Animation replacements'),empty:m('No replacement animations'),add:m('Add animation'),include:m('Include animation {index}'),file:m('New animation {index}'),pathKind:'animation',hint:m('Compatible VNM clips must match the original skeleton, timing, and events. Blender and FBX conversion is not provided by this editor.')},
  effects:{title:m('Effects'),empty:m('No custom effects'),add:m('Add effect'),include:m('Include effect {index}'),file:m('New effect {index}'),pathKind:'effect',hint:m('Compiled particle resources and their dependencies require validation before packaging.')},
  audio:{title:m('Audio'),empty:m('No custom sounds'),add:m('Add sound'),include:m('Include sound {index}'),file:m('New sound {index}'),pathKind:'audio',hint:m('Audio format, timing, and channels must match the original resource. Shared sounds can affect other characters.')},
};
const FIT_CHOICES:[string,string][]=[["contain",mc('image fit','Contain — retain the full image')],["cover",mc('image fit','Cover — crop to fill')],["stretch",mc('image fit','Stretch to the target size')]];
/** `only`: edit just these rows (their indices in the saved list; the Motion page's "Other saved rows"),
 * with no Add button; without it, every row. */
export function AssetRows({kind,only,title,...props}:{kind:AssetKind;only?:number[];title?:string}&Props){
  const t=useT();
  const {project,update,disabled,fail,catalog,discover}=props, spec=assetSpec[kind],rows=project.customization[kind]??[];
  const change=(next:ResourceReplacement[])=>update(changeCustomization(project,{[kind]:next}));
  const edit=(index:number,value:Partial<ResourceReplacement>)=>change(rows.map((row,i)=>i===index?{...row,...value}:row));
  if(!rows.length&&!only)return <div className="optional-asset-row"><div><h2>{t(spec.title)}</h2><p>{t(spec.empty)} <span className="optional-tag">{t('Optional')}</span></p></div><button disabled={disabled} onClick={()=>change([{enabled:true,source:'',target:''}])}>{t(spec.add)}</button></div>;
  return <Section title={title??t(spec.title)} actions={only?undefined:<button disabled={disabled||rows.length>=128} onClick={()=>change([...rows,{enabled:true,source:'',target:''}])}>{t(spec.add)}</button>}><p className="hint">{t(spec.hint)}</p>{rows.map((row,index)=>only&&!only.includes(index)?null:<div className="asset-editor" key={index}><div className="section-heading"><Check label={t(spec.include,{index:index+1})} value={row.enabled??true} onChange={enabled=>edit(index,{enabled})} disabled={disabled}/><button className="tertiary" disabled={disabled} onClick={()=>change(rows.filter((_,i)=>i!==index))}>{t('Remove')}</button></div><FilePathInput label={t(spec.file,{index:index+1})} name={fill(spec.file,{index:index+1})} kind={spec.pathKind} value={row.source??''} update={source=>edit(index,{source})} disabled={disabled} fail={fail}/><div className="destination"><span className="muted">{t('Game destination')}</span><code className="path">{row.target||t('Not selected')}</code><span className="hint">{row.target?t('Saved destination — ownership and compatibility must be verified before building.'):t('Select a discovered game destination before building. This row can be saved as a draft.')}</span></div>{discover&&<DestinationPicker kind={kind} catalog={catalog?.hero_path===project.original_model?catalog:null} current={row.target??''} disabled={disabled} discover={discover} choose={entry=>{const next={...row,target:entry.target,allow_shared:false};if(entry.original_sha256)next.original_sha256=entry.original_sha256;else delete next.original_sha256;change(rows.map((item,i)=>i===index?next:item));}}/>}{catalog?.[kind].find(entry=>entry.target===row.target)?.shared&&<Check label={t('Allow this replacement to affect other characters sharing the resource')} value={row.allow_shared??false} onChange={allow_shared=>edit(index,{allow_shared})} disabled={disabled}/>} {kind==='icons'&&row.source&&<ImagePreview path={row.source} disabled={disabled} fail={fail}/>} {kind==='icons'&&<SelectField label={t('Image fit {index}',{index:index+1})} value={row.fit??'contain'} values={FIT_CHOICES.map(([value,text])=>[value,tc('image fit',text)] as const)} onChange={fit=>edit(index,{fit:fit as ResourceReplacement['fit']})} disabled={disabled}/>}<details className="technical"><summary>{t('Advanced destination')}</summary><label className="field">{t('Exact existing game asset')}<input value={row.target??''} disabled={disabled} onChange={event=>{const copy={...row,target:event.target.value};delete copy.original_sha256;delete copy.allow_shared;change(rows.map((item,i)=>i===index?copy:item));}}/></label><p className="hint">{t('Only an existing resource belonging to the selected character can pass build validation. Editing this path invalidates its saved fingerprint and shared-resource permission.')}</p>{(kind==='audio'||kind==='effects')&&<Check label={t('Allow changes to other characters that share this asset')} value={row.allow_shared??false} onChange={allow_shared=>edit(index,{allow_shared})} disabled={disabled}/>}</details></div>)}</Section>;
}
export function ExtrasPage(props:ExtrasProps){return <ExtrasWorkspace {...props}/>;}
