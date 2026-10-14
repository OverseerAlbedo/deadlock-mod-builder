import { describe, expect, it } from 'vitest';
import { adjusted, adjustments, fittedHands, gunName, heroGun, leftMode, nudge, previewShapes, withAdjustments } from './gun';
import { removedGuns } from './secondary-bones';
import type { Bone, PreparationAnalysis, WeaponCandidate, WeaponReference } from './models';

const reference=(values:Partial<WeaponReference>):WeaponReference=>({policy:'weapon-fit-v1',source:'meshes',separate_model:false,meshes:[],primary:[],model_meshes:[],bones:[],notes:[],...values});
const gun=(values:Partial<WeaponCandidate>):WeaponCandidate=>({id:'Pistol',objects:['Pistol'],vertices:10,bounds:[[0,0,0],[1,1,1]],sample:[],attach_kind:'placed',keywords:[],elongation:3,...values});

describe('Gun helpers',()=>{
  it('tells how the hero carries its gun',()=>{
    expect(heroGun(reference({meshes:['g'],sides:[{side:'right',meshes:['r'],primary:['r'],bones:[]},{side:'left',meshes:['l'],primary:['l'],bones:[]}]}),false)).toBe('dual');
    expect(heroGun(reference({meshes:['g']}),false)).toBe('single');
    expect(heroGun(reference({unreadable:['haze_gun_l']}),false)).toBe('unreadable');
    expect(heroGun(reference({}),false)).toBe('separate');
    expect(heroGun(reference({separate_model:true}),true)).toBe('weapon_model');
    expect(heroGun(null,false)).toBe('unknown');
  });
  it('names guns by file and hand instead of listing every part',()=>{
    const objects=Array.from({length:284},(_,i)=>`gun_file_Part ${i} [L]`);
    expect(gunName(gun({objects,gun_file:true,side:'left'}),'C:\\guns\\Artzy_Tyrannical_Dual_Rig.blend')).toBe('Left gun from Artzy_Tyrannical_Dual_Rig.blend · 284 parts');
    expect(gunName(gun({objects:['Pistol','Mag']}))).toBe('Pistol and 1 more part (in your character file)');
  });
  it('turns, resizes and moves within range, per hand',()=>{
    const start={flip:false,roll:0,scale:1,offset:[0,0,0] as [number,number,number]};
    expect(nudge({...start,roll:180},{kind:'turn',degrees:90}).roll).toBe(-90);
    expect(nudge({...start,scale:9.5},{kind:'size',factor:1.1}).scale).toBe(10);
    expect(nudge(start,{kind:'move',axis:2,inches:-.25}).offset).toEqual([0,0,-.25]);
    expect(nudge({...start,flip:true,roll:45},{kind:'reset'})).toEqual(start);
    const choice=withAdjustments({objects:['R'],left:{objects:['L']}},'left',{...start,scale:.9});
    expect(adjustments(choice,'left').scale).toBe(.9);
    expect(adjustments(choice,'right').scale).toBe(1);
    expect(adjusted(choice)).toBe(true);
  });
  it('knows which hands get your gun',()=>{
    expect(leftMode({objects:['R']})).toBe('copy');
    expect(leftMode({objects:['R'],left:{objects:['L']}})).toBe('own');
    expect(fittedHands('dual',{left:{keep:true}})).toEqual(['right']);
    expect(fittedHands('dual',{})).toEqual(['right','left']);
    expect(fittedHands('single',{left:{objects:['L']}})).toEqual(['right']);
  });
  it('picks the solid shapes of each hand for the preview',()=>{
    const shape=(n:number)=>({positions:[n,0,0,0,1,0,0,0,1],triangles:[0,1,2]});
    const right=gun({id:'R',objects:['R'],side:'right',gun_file:true,shape:shape(1)}),left=gun({id:'L',objects:['L'],side:'left',gun_file:true,shape:shape(2)});
    const sides=[{side:'right' as const,meshes:['r'],primary:['r'],bones:[],shape:shape(4)},{side:'left' as const,meshes:['l'],primary:['l'],bones:[],shape:shape(5)}];
    const analysis={analysis:{weapons:[right,left]},weapon_reference:reference({meshes:['g'],body_shape:shape(3),sides})} as unknown as PreparationAnalysis;
    expect(previewShapes(analysis,{objects:['R'],left:{objects:['L']}},'dual')).toEqual({body:shape(3),guns:{right:shape(1),left:shape(2)},originals:{right:shape(4),left:shape(5)}});
    // An empty left hand gets a copy of the right gun; a kept left hand gets none.
    expect(previewShapes(analysis,{objects:['R']},'dual').guns.left).toEqual(shape(1));
    expect(previewShapes(analysis,{objects:['R'],left:{keep:true}},'dual').guns.left).toBeUndefined();
    const single={...analysis,weapon_reference:reference({meshes:['g'],shape:shape(6)})} as PreparationAnalysis;
    expect(previewShapes(single,{objects:['R']},'single')).toEqual({body:undefined,guns:{right:shape(1),left:undefined},originals:{right:shape(6)}});
    // The holding pose goes along when the hero's sources had an idle clip.
    const hand=[[0,-1,0,1],[1,0,0,2],[0,0,1,3],[0,0,0,1]];
    const holding={...analysis,weapon_reference:reference({meshes:['g'],body_shape:shape(3),sides,stance:{clip:'weapon_stand_idle',body_shape:shape(7),hands:{right:hand,left:hand}}})} as PreparationAnalysis;
    expect(previewShapes(holding,{objects:['R']},'dual').stance).toEqual({clip:'weapon_stand_idle',body:shape(7),hands:{right:hand,left:hand}});
  });
  it('leaves out both hands, or the pair\'s unused gun-file gun, like the native plan',()=>{
    const bones=[{name:'hand_R',parent:null,position:[0,0,0],weapon:['gun_file_R']},{name:'hand_L',parent:null,position:[0,0,0],weapon:['gun_file_L']},{name:'Gun',parent:null,position:[0,0,0],weapon:['Pistol']}] as Bone[];
    expect(removedGuns({weapon:{enabled:true,action:'replace',objects:['gun_file_R']}},bones)).toEqual(['gun_file_L']);
    expect(removedGuns({weapon:{enabled:true,action:'replace',objects:['gun_file_R'],left:{objects:['gun_file_L']}}},bones)).toEqual([]);
    expect(removedGuns({weapon:{enabled:true,action:'remove',objects:['Pistol']}},bones).sort()).toEqual(['Pistol','gun_file_L','gun_file_R']);
    expect(removedGuns({weapon:{enabled:true,action:'keep',objects:['gun_file_R']}},bones)).toEqual([]);
  });
});
