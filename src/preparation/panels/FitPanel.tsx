// i18n-area: prepare
import { useId, type Dispatch, type SetStateAction } from 'react';
import { m, tp, useT } from '../../i18n';
import { T } from '../../i18n/T';
import type { Project } from '../../models';
import { FLOOR_HELP, LEG_JOINT_CHOICES, LEG_JOINT_HELP, nextSentence, type FLOOR_CHOICES, type FloorFit, type LegJointFit, type PreparationAnalysis, type PreparationOptions, type PreparationResult } from '../models';
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
// The option cards: the stored value is the identifier; the title, badge and help are English
// here and translated where the cards are drawn. Each radio group keeps its English name in
// English (tests and messages find the groups by it).
const FIT_OPTIONS:StudioOption<FitMode>[]=[
  {value:'match_target',title:m('Match hero proportions'),help:m('Fits body regions to the hero’s joint positions. This can change the source silhouette, including the neck and clothing.')},
  {value:'preserve_proportions',title:m('Preserve source proportions'),help:m('Keeps the source torso, neck, head and leg proportions after overall scale and alignment. Arm fitting can align the elbows and wrists for animation.')},
];
const ARM_OPTIONS:StudioOption<ArmSkinning>[]=[
  {value:'smooth_twist',title:m('Smooth twist weights (reference style)'),help:m('Smooth twist weights spread arm movement over the hero’s twist joints; heroes without them keep your weights.')},
  {value:'source',title:m('Keep mapped source weights'),help:m('Your model’s own arm weights, moved to the hero’s bones they are matched to.')},
];
const SHAPE_OPTIONS:StudioOption<ShapeKeys>[]=[
  {value:'current_static',title:m('Keep the saved shape (static)')},
  {value:'basis_static',title:m('Use the base shape (static)')},
];
/** The titles and badges of the Feet on the floor and Leg joints cards (`models.ts` keeps the
 * same choices as one label each). */
const FLOOR_TITLES:Record<FloorFit,string>={ankles:m('Line up the ankles'),legs:m('Shorten or lengthen the legs'),lift:m('Move the whole character up or down')};
const FLOOR_BADGES:Partial<Record<FloorFit,string>>={ankles:m('default')};
const LEG_TITLES:Record<LegJointFit,string>={pelvis:m('As before (body placed by the pelvis bone)'),joints:m('Hip joints, and legs on the hero’s joints (no resizing)'),hips:m('Place the body by the hip joints'),scaled:m('Hip joints, and resize the character to the hero’s leg length'),scaled_joints:m('Hip joints, resize, and legs on the hero’s joints')};
const LEG_BADGES:Partial<Record<LegJointFit,string>>={pelvis:m('default'),joints:m('recommended if the knees stretch')};
const RESIZES:readonly LegJointFit[]=['scaled','scaled_joints'];
const FLOOR_WORDS:Record<FloorFit,string>={ankles:m('Ankles'),legs:m('Legs'),lift:m('Lift')};
const LEG_WORDS:Record<LegJointFit,string>={pelvis:m('Pelvis bone'),joints:m('Hero’s joints'),hips:m('Hip joints'),scaled:m('Resized'),scaled_joints:m('Resized, hero’s joints')};

/** The Fit & review tab: five chips (Proportions, Feet, Legs, Arms & hands, Shape keys) that each
 * show their current choice and open their group beside the viewer, option cards where the app had
 * drop-downs (each keeps the drop-down's name as a radio group), the last conversion's leg
 * measurements, what Prepare model confirms, Save mapping preset and the repeat actions. */
export function FitPanel({hero,weaponOnly,options,busy,changeOptions,floorChoices,setFloorReset,legsOnJoints,floorConflict,floorReset,preserving,legJoints,changeLegJoints,legMeasured,analysis,expert,followingCount,reviewed,project,savePreset,savedPreset,presetKey:key,result,review,prepareModel,group,setGroup,defaultFit='preserve_proportions'}:{
  /** Settings → Default fit for new models: the Model fitting card that carries the "default" badge. */defaultFit?:FitMode;hero:string;weaponOnly:boolean;options:PreparationOptions;busy:boolean;changeOptions:(values:Partial<PreparationOptions>)=>void;floorChoices:typeof FLOOR_CHOICES;setFloorReset:Dispatch<SetStateAction<boolean>>;legsOnJoints:boolean;floorConflict:boolean;floorReset:boolean;preserving:boolean;legJoints:NonNullable<PreparationOptions['fit_leg_joints']>;changeLegJoints:(value:string)=>void;legMeasured:string|null;analysis:PreparationAnalysis;expert:boolean;followingCount:number;reviewed:boolean;project:Project;savePreset:()=>Promise<void>;savedPreset:string;/** The current choices' key (the page's `key`). */presetKey:string;result:PreparationResult|null;review:()=>Promise<void>;prepareModel:()=>Promise<void>;
  /** The open group (the page keeps it, so Open section can choose it); null = the default. */group:FitGroup|null;setGroup:(group:FitGroup)=>void}){
  const t=useT();
  const base=useId().replace(/:/g,'');
  const morphs=analysis.analysis.morphs?.length??0,face=Boolean(analysis.geometry_customization?.face);
  const shapeNeeded=morphs>0&&!face&&!options.shape_key_mode;
  const fitMode=options.fit_mode??'match_target',floor=options.fit_floor??'ankles',arms=options.arm_skinning??'source';
  const chips:StudioChip<FitGroup>[]=[
    ...(!weaponOnly?[
      {id:'proportions' as const,label:t('Proportions'),detail:fitMode==='preserve_proportions'?t('Preserve source'):t('Match hero')},
      {id:'feet' as const,label:t('Feet'),detail:FLOOR_WORDS[floor]?t(FLOOR_WORDS[floor]):floor,tone:floorConflict?'attention' as const:undefined},
      {id:'legs' as const,label:t('Legs'),detail:preserving?LEG_WORDS[legJoints]?t(LEG_WORDS[legJoints]):legJoints:t('Not used'),tone:preserving?undefined:'quiet' as const},
      {id:'arms' as const,label:t('Arms & hands'),detail:arms==='smooth_twist'?t('Smooth twist'):t('Your weights')},
    ]:[]),
    ...(morphs>0?[{id:'shapekeys' as const,label:t('Shape keys'),detail:face?t('Expression'):options.shape_key_mode==='current_static'?t('Saved shape'):options.shape_key_mode==='basis_static'?t('Base shape'):t('Needs your choice'),tone:shapeNeeded?'attention' as const:undefined}]:[]),
  ];
  const fallback:FitGroup|undefined=shapeNeeded?'shapekeys':chips[0]?.id;
  const open=chips.some(chip=>chip.id===group)?group!:fallback;
  // The badge tone follows the English badge (default is quiet, the recommendation ok, a resize a caution).
  const legOptions:StudioOption<LegJointFit>[]=LEG_JOINT_CHOICES.map(([value])=>{const badge=LEG_BADGES[value]??(RESIZES.includes(value)?m('resizes'):undefined);return {value,title:t(LEG_TITLES[value]),badge:badge&&t(badge),badgeTone:badge==='default'?'quiet':LEG_BADGES[value]?'ok':'attention',help:t(LEG_JOINT_HELP[value])};});
  // Match hero proportions stays first; the badge follows Settings → Default fit for new models.
  const fitOptions:StudioOption<FitMode>[]=FIT_OPTIONS.map(option=>({...option,title:t(option.title),help:option.help?t(option.help as string):undefined,...(option.value===defaultFit?{badge:t('default'),badgeTone:'quiet' as const}:{})}));
  const floorOptions:StudioOption<FloorFit>[]=floorChoices.map(([value])=>({value,title:t(FLOOR_TITLES[value]),badge:FLOOR_BADGES[value]&&t(FLOOR_BADGES[value]!),badgeTone:'quiet',help:t(FLOOR_HELP[value])}));
  const armOptions:StudioOption<ArmSkinning>[]=ARM_OPTIONS.map(option=>({...option,title:t(option.title),help:t(option.help as string)}));
  const shapeOptions:StudioOption<ShapeKeys>[]=SHAPE_OPTIONS.map(option=>({...option,title:t(option.title)}));
  const conversion=t('Changing this needs a new conversion.');
  const confirms=weaponOnly?t('Clicking Prepare model confirms the gun and its fit.'):t('Clicking Prepare model confirms the joint matches, the orientation and the moving parts shown in these tabs.');
  const floorSub=nextSentence(t('Prepare model reports how far the soles were from the floor.'),conversion);
  return <div className="prep-fit">
    {open&&<StudioCard className="prep-fit-groups">
      <p className="studio-card-sub prep-fit-intro">{t('Choose how your model fits {hero}. Prepare model checks everything, converts and uses the result.',{hero})}</p>
      <StudioChips label={t('Fit groups')} base={base} chips={chips} active={open} onSelect={setGroup}/>
      {floorConflict&&<p className="prep-fit-alert" role="alert">{t('Feet on the floor → Shorten or lengthen the legs cannot be used with this Leg joints choice: it would take the legs off the hero’s joints again. Choose another Feet on the floor option.')}</p>}
      {floorReset&&!floorConflict&&<p className="prep-fit-note" role="status">{t('Feet on the floor was set back to Line up the ankles: shortening or lengthening the legs would take them off the hero’s joints again.')}</p>}
      <StudioChipPanel base={base} id={open}>
        {open==='proportions'&&<>
          <h4 className="prep-fit-heading">{t('Model fitting')}</h4>
          <p className="studio-note prep-fit-sub">{nextSentence(t('How your model is fitted to {hero}.',{hero}),conversion)}</p>
          <StudioOptionCards label={t('Model fitting')} name={`${base}-fit`} options={fitOptions} value={fitMode} disabled={busy} onChange={value=>changeOptions({fit_mode:value,reviewed:false})}/>
        </>}
        {open==='feet'&&<>
          <h4 className="prep-fit-heading">{t('Feet on the floor')}</h4>
          <p className="studio-note prep-fit-sub">{legsOnJoints?nextSentence(floorSub,t('Shorten or lengthen the legs is not offered while Leg joints puts the legs on the hero’s joints: it would take them off the joints again.')):floorSub}</p>
          <StudioOptionCards label={t('Feet on the floor')} name={`${base}-floor`} options={floorOptions} value={floor} disabled={busy} onChange={value=>{setFloorReset(false);changeOptions({fit_floor:value,reviewed:false});}}/>
        </>}
        {open==='legs'&&<>
          <h4 className="prep-fit-heading">{t('Leg joints')}</h4>
          {preserving?<>
            <p className="studio-note prep-fit-sub">{nextSentence(t('For knees that stretch in deep bends.'),conversion)}</p>
            <StudioOptionCards label={t('Leg joints')} name={`${base}-legs`} options={legOptions} value={legJoints} disabled={busy} onChange={changeLegJoints}/>
          </>:<p className="studio-note prep-fit-sub">{t('Leg joints is offered only with Model fitting → Preserve source proportions (under Proportions). Match hero proportions places the legs on the hero’s joints by itself.')}</p>}
        </>}
        {open==='arms'&&<>
          <h4 className="prep-fit-heading">{t('Arm skinning')}</h4>
          <StudioOptionCards label={t('Arm skinning')} name={`${base}-arms`} options={armOptions} value={arms} disabled={busy} onChange={value=>changeOptions({arm_skinning:value,reviewed:false})}/>
          <div className="prep-fit-switches">
            {fitMode==='preserve_proportions'&&<><Check label={t('Align arms to hero joints (recommended)')} value={options.fit_arm_joints??true} disabled={busy} onChange={fit_arm_joints=>changeOptions({fit_arm_joints,reviewed:false})}/><p className="hint">{t('Keeps limb width; turn off to keep your arm lengths (elbows can then clip).')}</p></>}
            <Check label={t('Bend fingers like the hero (recommended)')} value={options.fit_finger_curl??true} disabled={busy} onChange={fit_finger_curl=>changeOptions({fit_finger_curl,reviewed:false})}/><p className="hint">{t('So the hero’s grip closes your model’s fingers around the gun.')}</p>
            <NumberField label={t('Maximum skin influences')} value={options.influence_limit??4} min={1} max={4} step={1} disabled={busy} update={influence_limit=>changeOptions({influence_limit,reviewed:false})}/><p className="hint">{t('Bones per vertex; 4 is the default.')}</p>
          </div>
        </>}
        {open==='shapekeys'&&<>
          <h4 className="prep-fit-heading">{t('Shape keys')}</h4>
          <p className="studio-note prep-fit-sub">{morphs===1
            ?<T text="<strong>{count} shape key detected.</strong> Choose how to keep the model’s shape during conversion." values={{count:morphs}}/>
            :<T text="<strong>{count} shape keys detected.</strong> Choose how to keep the model’s shape during conversion." values={{count:morphs}}/>}</p>
          {face?<p className="hint">{t('Your static expression from Appearance will be baked. Animated shape keys are not exported.')}</p>:<>
            <StudioOptionCards label={t('Shape key handling')} name={`${base}-shapes`} options={shapeOptions} value={options.shape_key_mode??''} disabled={busy} onChange={value=>changeOptions({shape_key_mode:value,reviewed:false})}/>
            {/* Back to no choice: the key is removed (the old drop-down's empty entry stored '', which
                the review and Blender read the same way), so Prepare model asks for a choice again.
                Unset has no behaviour of its own. */}
            {options.shape_key_mode&&<p className="prep-fit-clear"><button type="button" className="tertiary" disabled={busy} onClick={()=>changeOptions({shape_key_mode:undefined,reviewed:false})}>{t('Clear this choice')}</button> <span className="hint">{t('Prepare model then asks again.')}</span></p>}
            <p className="hint">{t('One shape is baked into the prepared model; animated expressions are not exported. Your file keeps all shape keys.')}</p>
          </>}
        </>}
      </StudioChipPanel>
    </StudioCard>}
    {legMeasured&&<StudioCard className="prep-last-conversion" title={t('Last conversion')} actions={<small className="studio-note">{t('measured after the fit')}</small>}>
      <p className="prep-fit-measured" role="status" aria-label={t('Leg joints measured')}>{legMeasured}</p>
    </StudioCard>}
    <StudioCard className="prep-confirmations" title={t('What Prepare model confirms')}>
      <p className="studio-note">{followingCount>0?nextSentence(confirms,tp(followingCount,{other:'{count} small bones follow a body joint and lose their own movement (for example extra twist or cloth helpers).'})):confirms}</p>
      {(result||expert)&&<div className="prep-confirm-actions">
        {result&&<><button disabled={busy} onClick={()=>void review()}>{t(BUTTON_NAMES.checkForProblems)}</button><button disabled={busy} onClick={()=>void prepareModel()}>{t(BUTTON_NAMES.prepareAgain)}</button></>}
        {expert&&<button className="prep-preset-button" disabled={busy||!reviewed||project.model_input_mode!=='rigged_humanoid'} onClick={()=>void savePreset()}><PrepIcon name="folder" size={17}/>{t('Save mapping preset')}</button>}
      </div>}
      {expert&&<p className="hint">{t('Offered automatically next time for the same rig, bones, orientation and hero.')}</p>}
      {expert&&savedPreset===key&&<p role="status">{t('Mapping preset saved for this rig pair.')}</p>}
    </StudioCard>
  </div>;
}
