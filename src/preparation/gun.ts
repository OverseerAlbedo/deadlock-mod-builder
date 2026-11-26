// i18n-area: prepare
import { t, tp } from '../i18n';
import type { Hand, PreparationAnalysis, Shape, Vec3, WeaponCandidate, WeaponChoice, WeaponHand, WeaponReference } from './models';

/** How the hero carries its gun, as the Gun tab explains it. */
export type HeroGun = 'dual'|'single'|'separate'|'unreadable'|'weapon_model'|'unknown';

export function heroGun(reference:WeaponReference|null|undefined,weaponModel:boolean):HeroGun{
  if(weaponModel)return 'weapon_model';
  if(!reference)return 'unknown';
  const sides=reference.sides??[];
  if(sides.some(s=>s.side==='right')&&sides.some(s=>s.side==='left'))return 'dual';
  if(reference.meshes.length>0)return 'single';
  if(reference.unreadable?.length)return 'unreadable';
  return reference.separate_model?'weapon_model':'separate';
}

/** Whether the hero's gun can be replaced from the character's preparation. */
export const replaceable=(kind:HeroGun)=>kind==='dual'||kind==='single'||kind==='weapon_model';

const fileName=(path:string)=>path.split(/[\\/]/).pop()??path;

/** A gun named for people: its file and hand, or its object, instead of every part name. */
export function gunName(candidate:WeaponCandidate,gunFile?:string):string{
  const parts=candidate.objects.length;
  if(candidate.gun_file){
    const values={file:gunFile?fileName(gunFile):'',count:parts.toLocaleString()};
    if(candidate.side==='left')return gunFile?tp(parts,{one:'Left gun from {file} · {count} part',other:'Left gun from {file} · {count} parts'},values):tp(parts,{one:'Left gun from your gun file · {count} part',other:'Left gun from your gun file · {count} parts'},values);
    if(candidate.side==='right')return gunFile?tp(parts,{one:'Right gun from {file} · {count} part',other:'Right gun from {file} · {count} parts'},values):tp(parts,{one:'Right gun from your gun file · {count} part',other:'Right gun from your gun file · {count} parts'},values);
    return gunFile?tp(parts,{one:'Gun from {file} · {count} part',other:'Gun from {file} · {count} parts'},values):tp(parts,{one:'Gun from your gun file · {count} part',other:'Gun from your gun file · {count} parts'},values);
  }
  const first=candidate.objects[0]??t('Gun');
  return parts===1?t('{name} (in your character file)',{name:first}):tp(parts-1,{one:'{name} and {count} more part (in your character file)',other:'{name} and {count} more parts (in your character file)'},{name:first});
}

/** The gun-file guns and the character's own guns of an inspection. */
export function yourGuns(analysis:PreparationAnalysis){
  const all=analysis.analysis.weapons??[];
  return {all,file:all.filter(c=>c.gun_file),own:all.filter(c=>!c.gun_file)};
}

export const sameObjects=(a:string[]|undefined,b:string[]|undefined)=>{
  if(!a||!b||a.length!==b.length)return false;
  const x=[...a].sort(),y=[...b].sort();return x.every((n,i)=>n===y[i]);
};

/** The candidate a hand uses: the right (or only) gun, or the left gun of a dual-wield hero. */
export function handGun(choice:WeaponChoice,candidates:WeaponCandidate[],hand:Hand):WeaponCandidate|undefined{
  const objects=hand==='left'?choice.left?.objects:choice.objects;
  return objects?.length?candidates.find(c=>sameObjects(c.objects,objects)):undefined;
}

/** The left hand's state: its own gun, a copy of the right gun, or the hero's own gun. */
export type LeftMode='own'|'copy'|'keep';
export function leftMode(choice:WeaponChoice):LeftMode{
  if(choice.left?.keep)return 'keep';
  return choice.left?.objects?.length?'own':'copy';
}

/** Fit adjustments of one hand (the left hand's are under choice.left). */
export type Adjustments={flip:boolean;roll:number;pitch:number;yaw:number;scale:number;offset:Vec3};
export function adjustments(choice:WeaponChoice,hand:Hand):Adjustments{
  const source:WeaponChoice|WeaponHand=hand==='left'?choice.left??{}:choice;
  return {flip:source.flip??false,roll:source.roll??0,pitch:source.pitch??0,yaw:source.yaw??0,scale:source.scale??1,offset:(source.offset??[0,0,0]) as Vec3};
}

/** One click of a beginner control. */
/** turn: around the barrel (roll); tilt: muzzle up (pitch); aim: muzzle to the right (yaw). */
export type Nudge={kind:'flip'}|{kind:'turn'|'tilt'|'aim';degrees:number}|{kind:'size';factor:number}|{kind:'move';axis:0|1|2;inches:number}|{kind:'reset'};

const round=(value:number,step:number)=>Math.round(value/step)*step;
const wrap=(degrees:number)=>{let d=degrees;while(d>180)d-=360;while(d<=-180)d+=360;return d;};
/** Roll, pitch and yaw stay within (-180, 180]; size between 10% and 1000%; moves within ±50 inches. */
export function nudge(current:Adjustments,change:Nudge):Adjustments{
  switch(change.kind){
    case 'flip':return {...current,flip:!current.flip};
    case 'turn':return {...current,roll:wrap(current.roll+change.degrees)};
    case 'tilt':return {...current,pitch:wrap(current.pitch+change.degrees)};
    case 'aim':return {...current,yaw:wrap(current.yaw+change.degrees)};
    case 'size':return {...current,scale:Math.min(10,Math.max(.1,round(current.scale*change.factor,.01)))};
    case 'move':{const offset=[...current.offset] as Vec3;offset[change.axis]=Math.min(50,Math.max(-50,round(offset[change.axis]+change.inches,.05)));return {...current,offset};}
    case 'reset':return {flip:false,roll:0,pitch:0,yaw:0,scale:1,offset:[0,0,0]};
  }
}

/** The choice with one hand's adjustments replaced. */
export function withAdjustments(choice:WeaponChoice,hand:Hand,values:Adjustments):WeaponChoice{
  return hand==='left'?{...choice,left:{...choice.left,...values}}:{...choice,...values};
}

/** Whether any hand has been moved, turned, tilted, flipped or resized. */
export const adjusted=(choice:WeaponChoice)=>(['right','left'] as Hand[]).some(hand=>{const a=adjustments(choice,hand);return a.flip||a.roll!==0||a.pitch!==0||a.yaw!==0||a.scale!==1||a.offset.some(v=>v!==0);});

/** The hands that get your gun: both for a dual-wield hero (unless the left keeps the hero's gun). */
export function fittedHands(kind:HeroGun,choice:WeaponChoice):Hand[]{
  if(kind!=='dual')return ['right'];
  return leftMode(choice)==='keep'?['right']:['right','left'];
}

/** Solid shapes of the fit preview: your gun of each hand (in your scene; the
 * preview places it with the fit), the hero's gun of each hand and the hero's body.
 * Missing when the inspection predates them; the preview then shows points. */
export type PreviewShapes={body?:Shape;guns:Partial<Record<Hand,Shape>>;originals:Partial<Record<Hand,Shape>>;
  /** The hero holding its gun, when its sources have an idle clip: the posed body and how each hand's gun moves. */
  stance?:{clip:string;body:Shape;hands:Partial<Record<Hand,number[][]>>}};
export function previewShapes(analysis:PreparationAnalysis,choice:WeaponChoice,kind:HeroGun):PreviewShapes{
  const candidates=analysis.analysis.weapons??[],reference=analysis.weapon_reference;
  const right=handGun(choice,candidates,'right')?.shape,mode=leftMode(choice);
  const left=kind!=='dual'||mode==='keep'?undefined:mode==='own'?handGun(choice,candidates,'left')?.shape:right;
  const original=(hand:Hand)=>reference?.sides?.find(s=>s.side===hand)?.shape;
  const stance=reference?.stance?.body_shape?.positions.length?{clip:reference.stance.clip,body:reference.stance.body_shape,hands:reference.stance.hands}:undefined;
  return {body:reference?.body_shape,guns:{right,left},originals:kind==='dual'?{right:original('right'),left:original('left')}:{right:reference?.shape},stance};
}
