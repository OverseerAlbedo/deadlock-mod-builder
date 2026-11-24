import { useId, type Dispatch, type SetStateAction } from 'react';
import type { Project } from '../../models';
import { FLOOR_HELP, LEG_JOINT_CHOICES, LEG_JOINT_HELP, type FLOOR_CHOICES, type FloorFit, type LegJointFit, type PreparationAnalysis, type PreparationOptions, type PreparationResult } from '../models';
import { Check } from '../../components/CustomizationPages';
import { NumberField } from '../../components/MaterialEditor';
import { StudioCard } from '../../studio/Studio';
import { StudioChipPanel, StudioChips, StudioOptionCards, type StudioChip, type StudioOption } from '../../studio/StudioChoices';
import { PrepIcon } from '../PreparationChrome';
import { BUTTON_NAMES } from '../pageNames';

/** The five groups of Fit & review, one open at a time. */
export type FitGroup='proportions'|'feet'|'legs'|'arms'|'shapekeys';
type FitMode=NonNullable<PreparationOptions['fit_mode']>;
type ArmSkinning=NonNullable<PreparationOptions['arm_skinning']>;
type ShapeKeys=NonNullable<PreparationOptions['shape_key_mode']>;
const CONVERSION=' Changing this needs a new conversion.';
const FIT_OPTIONS:StudioOption<FitMode>[]=[
  {value:'match_target',title:'Match hero proportions',help:'Fits body regions to the hero’s joint positions. This can change the source silhouette, including the neck and clothing.'},
  {value:'preserve_proportions',title:'Preserve source proportions',help:'Keeps the source torso, neck, head and leg proportions after overall scale and alignment. Arm fitting can align the elbows and wrists for animation.'},
];
const ARM_OPTIONS:StudioOption<ArmSkinning>[]=[
  {value:'smooth_twist',title:'Smooth twist weights (reference style)',help:'Smooth twist weights spread arm movement over the hero’s twist joints; heroes without them keep your weights.'},
  {value:'source',title:'Keep mapped source weights',help:'Your model’s own arm weights, moved to the hero’s bones they are matched to.'},
];
const SHAPE_OPTIONS:StudioOption<ShapeKeys>[]=[
  {value:'current_static',title:'Keep the saved shape (static)'},
  {value:'basis_static',title:'Use the base shape (static)'},
];
/** A choice label of `models.ts` split into the card's title and its badge (` · default`, ` · recommended…`). */
function titled(label:string):{title:string;badge?:string}{const at=label.lastIndexOf(' · ');return at<0?{title:label}:{title:label.slice(0,at),badge:label.slice(at+3)};}
const floorTitle=(label:string)=>label.endsWith(' (default)')?{title:label.slice(0,-' (default)'.length),badge:'default'}:{title:label};
const RESIZES:readonly LegJointFit[]=['scaled','scaled_joints'];
const FLOOR_WORDS:Record<FloorFit,string>={ankles:'Ankles',legs:'Legs',lift:'Lift'};
const LEG_WORDS:Record<LegJointFit,string>={pelvis:'Pelvis bone',joints:'Hero’s joints',hips:'Hip joints',scaled:'Resized',scaled_joints:'Resized, hero’s joints'};

/** The Fit & review tab: five chips (Proportions, Feet, Legs, Arms & hands, Shape keys) that each
 * show their current choice and open their group beside the viewer, option cards where the app had
 * drop-downs (each keeps the drop-down's name as a radio group), the last conversion's leg
 * measurements, what Prepare model confirms, Save mapping preset and the repeat actions. */
export function FitPanel({hero,weaponOnly,options,busy,changeOptions,floorChoices,setFloorReset,legsOnJoints,floorConflict,floorReset,preserving,legJoints,changeLegJoints,legMeasured,analysis,expert,followingCount,reviewed,project,savePreset,savedPreset,presetKey:key,result,review,prepareModel,group,setGroup,defaultFit='preserve_proportions'}:{
  /** Settings → Default fit for new models: the Model fitting card that carries the "default" badge. */defaultFit?:FitMode;hero:string;weaponOnly:boolean;options:PreparationOptions;busy:boolean;changeOptions:(values:Partial<PreparationOptions>)=>void;floorChoices:typeof FLOOR_CHOICES;setFloorReset:Dispatch<SetStateAction<boolean>>;legsOnJoints:boolean;floorConflict:boolean;floorReset:boolean;preserving:boolean;legJoints:NonNullable<PreparationOptions['fit_leg_joints']>;changeLegJoints:(value:string)=>void;legMeasured:string|null;analysis:PreparationAnalysis;expert:boolean;followingCount:number;reviewed:boolean;project:Project;savePreset:()=>Promise<void>;savedPreset:string;/** The current choices' key (the page's `key`). */presetKey:string;result:PreparationResult|null;review:()=>Promise<void>;prepareModel:()=>Promise<void>;
  /** The open group (the page keeps it, so Open section can choose it); null = the default. */group:FitGroup|null;setGroup:(group:FitGroup)=>void}){
  const base=useId().replace(/:/g,'');
  const morphs=analysis.analysis.morphs?.length??0,face=Boolean(analysis.geometry_customization?.face);
  const shapeNeeded=morphs>0&&!face&&!options.shape_key_mode;
  const fitMode=options.fit_mode??'match_target',floor=options.fit_floor??'ankles',arms=options.arm_skinning??'source';
  const chips:StudioChip<FitGroup>[]=[
    ...(!weaponOnly?[
      {id:'proportions' as const,label:'Proportions',detail:fitMode==='preserve_proportions'?'Preserve source':'Match hero'},
      {id:'feet' as const,label:'Feet',detail:FLOOR_WORDS[floor]??floor,tone:floorConflict?'attention' as const:undefined},
      {id:'legs' as const,label:'Legs',detail:preserving?LEG_WORDS[legJoints]??legJoints:'Not used',tone:preserving?undefined:'quiet' as const},
      {id:'arms' as const,label:'Arms & hands',detail:arms==='smooth_twist'?'Smooth twist':'Your weights'},
    ]:[]),
    ...(morphs>0?[{id:'shapekeys' as const,label:'Shape keys',detail:face?'Expression':options.shape_key_mode==='current_static'?'Saved shape':options.shape_key_mode==='basis_static'?'Base shape':'Needs your choice',tone:shapeNeeded?'attention' as const:undefined}]:[]),
  ];
  const fallback:FitGroup|undefined=shapeNeeded?'shapekeys':chips[0]?.id;
  const open=chips.some(chip=>chip.id===group)?group!:fallback;
  const legOptions:StudioOption<LegJointFit>[]=LEG_JOINT_CHOICES.map(([value,label])=>{const {title,badge}=titled(label);return {value,title,badge:badge??(RESIZES.includes(value)?'resizes':undefined),badgeTone:badge==='default'?'quiet':badge?'ok':'attention',help:LEG_JOINT_HELP[value]};});
  // Match hero proportions stays first; the badge follows Settings → Default fit for new models.
  const fitOptions:StudioOption<FitMode>[]=FIT_OPTIONS.map(option=>option.value===defaultFit?{...option,badge:'default',badgeTone:'quiet'}:option);
  const floorOptions:StudioOption<FloorFit>[]=floorChoices.map(([value,label])=>{const {title,badge}=floorTitle(label);return {value,title,badge,badgeTone:'quiet',help:FLOOR_HELP[value]};});
  const confirms=weaponOnly?'Clicking Prepare model confirms the gun and its fit.':'Clicking Prepare model confirms the joint matches, the orientation and the moving parts shown in these tabs.';
  return <div className="prep-fit">
    {open&&<StudioCard className="prep-fit-groups">
      <p className="studio-card-sub prep-fit-intro">Choose how your model fits {hero}. Prepare model checks everything, converts and uses the result.</p>
      <StudioChips label="Fit groups" base={base} chips={chips} active={open} onSelect={setGroup}/>
      {floorConflict&&<p className="prep-fit-alert" role="alert">Feet on the floor → Shorten or lengthen the legs cannot be used with this Leg joints choice: it would take the legs off the hero’s joints again. Choose another Feet on the floor option.</p>}
      {floorReset&&!floorConflict&&<p className="prep-fit-note" role="status">Feet on the floor was set back to Line up the ankles: shortening or lengthening the legs would take them off the hero’s joints again.</p>}
      <StudioChipPanel base={base} id={open}>
        {open==='proportions'&&<>
          <h4 className="prep-fit-heading">Model fitting</h4>
          <p className="studio-note prep-fit-sub">How your model is fitted to {hero}.{CONVERSION}</p>
          <StudioOptionCards label="Model fitting" name={`${base}-fit`} options={fitOptions} value={fitMode} disabled={busy} onChange={value=>changeOptions({fit_mode:value,reviewed:false})}/>
        </>}
        {open==='feet'&&<>
          <h4 className="prep-fit-heading">Feet on the floor</h4>
          <p className="studio-note prep-fit-sub">Prepare model reports how far the soles were from the floor.{CONVERSION}{legsOnJoints?' Shorten or lengthen the legs is not offered while Leg joints puts the legs on the hero’s joints: it would take them off the joints again.':''}</p>
          <StudioOptionCards label="Feet on the floor" name={`${base}-floor`} options={floorOptions} value={floor} disabled={busy} onChange={value=>{setFloorReset(false);changeOptions({fit_floor:value,reviewed:false});}}/>
        </>}
        {open==='legs'&&<>
          <h4 className="prep-fit-heading">Leg joints</h4>
          {preserving?<>
            <p className="studio-note prep-fit-sub">For knees that stretch in deep bends.{CONVERSION}</p>
            <StudioOptionCards label="Leg joints" name={`${base}-legs`} options={legOptions} value={legJoints} disabled={busy} onChange={changeLegJoints}/>
          </>:<p className="studio-note prep-fit-sub">Leg joints is offered only with Model fitting → Preserve source proportions (under Proportions). Match hero proportions places the legs on the hero’s joints by itself.</p>}
        </>}
        {open==='arms'&&<>
          <h4 className="prep-fit-heading">Arm skinning</h4>
          <StudioOptionCards label="Arm skinning" name={`${base}-arms`} options={ARM_OPTIONS} value={arms} disabled={busy} onChange={value=>changeOptions({arm_skinning:value,reviewed:false})}/>
          <div className="prep-fit-switches">
            {fitMode==='preserve_proportions'&&<><Check label="Align arms to hero joints (recommended)" value={options.fit_arm_joints??true} disabled={busy} onChange={fit_arm_joints=>changeOptions({fit_arm_joints,reviewed:false})}/><p className="hint">Keeps limb width; turn off to keep your arm lengths (elbows can then clip).</p></>}
            <Check label="Bend fingers like the hero (recommended)" value={options.fit_finger_curl??true} disabled={busy} onChange={fit_finger_curl=>changeOptions({fit_finger_curl,reviewed:false})}/><p className="hint">So the hero’s grip closes your model’s fingers around the gun.</p>
            <NumberField label="Maximum skin influences" value={options.influence_limit??4} min={1} max={4} step={1} disabled={busy} update={influence_limit=>changeOptions({influence_limit,reviewed:false})}/><p className="hint">Bones per vertex; 4 is the default.</p>
          </div>
        </>}
        {open==='shapekeys'&&<>
          <h4 className="prep-fit-heading">Shape keys</h4>
          <p className="studio-note prep-fit-sub"><strong>{morphs} shape key{morphs===1?'':'s'} detected.</strong> Choose how to keep the model’s shape during conversion.</p>
          {face?<p className="hint">Your static expression from Appearance will be baked. Animated shape keys are not exported.</p>:<>
            <StudioOptionCards label="Shape key handling" name={`${base}-shapes`} options={SHAPE_OPTIONS} value={options.shape_key_mode??''} disabled={busy} onChange={value=>changeOptions({shape_key_mode:value,reviewed:false})}/>
            {/* Back to no choice: the key is removed (the old drop-down's empty entry stored '', which
                the review and Blender read the same way), so Prepare model asks for a choice again.
                Unset has no behaviour of its own. */}
            {options.shape_key_mode&&<p className="prep-fit-clear"><button type="button" className="tertiary" disabled={busy} onClick={()=>changeOptions({shape_key_mode:undefined,reviewed:false})}>Clear this choice</button> <span className="hint">Prepare model then asks again.</span></p>}
            <p className="hint">One shape is baked into the prepared model; animated expressions are not exported. Your file keeps all shape keys.</p>
          </>}
        </>}
      </StudioChipPanel>
    </StudioCard>}
    {legMeasured&&<StudioCard className="prep-last-conversion" title="Last conversion" actions={<small className="studio-note">measured after the fit</small>}>
      <p className="prep-fit-measured" role="status" aria-label="Leg joints measured">{legMeasured}</p>
    </StudioCard>}
    <StudioCard className="prep-confirmations" title="What Prepare model confirms">
      <p className="studio-note">{confirms}{followingCount>0?` ${followingCount} small bones follow a body joint and lose their own movement (for example extra twist or cloth helpers).`:''}</p>
      {(result||expert)&&<div className="prep-confirm-actions">
        {result&&<><button disabled={busy} onClick={()=>void review()}>{BUTTON_NAMES.checkForProblems}</button><button disabled={busy} onClick={()=>void prepareModel()}>{BUTTON_NAMES.prepareAgain}</button></>}
        {expert&&<button className="prep-preset-button" disabled={busy||!reviewed||project.model_input_mode!=='rigged_humanoid'} onClick={()=>void savePreset()}><PrepIcon name="folder" size={17}/>Save mapping preset</button>}
      </div>}
      {expert&&<p className="hint">Offered automatically next time for the same rig, bones, orientation and hero.</p>}
      {expert&&savedPreset===key&&<p role="status">Mapping preset saved for this rig pair.</p>}
    </StudioCard>
  </div>;
}
