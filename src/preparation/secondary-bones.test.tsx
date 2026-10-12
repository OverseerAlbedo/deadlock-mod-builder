import { useState } from 'react';
import { cleanup,fireEvent,render,screen } from '@testing-library/react';
import { describe,expect,it,vi } from 'vitest';
import { PhysicsBoneChoices } from './PhysicsBoneChoices';
import { physicsGroups,keptPhysicsBones,automaticPhysicsOptions,breastBones,breastMotionOptions,charmNamed,gunChoiceOptions,hangsFromGun,PHYSICS_BINDING_POLICY } from './secondary-bones';
import type { Bone,PreparationOptions } from './models';
import hanging from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/hanging-chains.json';
const physical={policy:PHYSICS_BINDING_POLICY,dynamic_bodies:['saved rigid'],kinematic_bodies:[]};
const bone=(name:string,parent:string|null,physics?:Bone['physics']):Bone=>({name,parent,position:[0,0,0],physics});
const bones=[bone('pelvis',null),bone('head','pelvis'),bone('arm','pelvis',{...physical,dynamic_bodies:[],kinematic_bodies:['fixed collider']}),bone('branchA','pelvis',physical),bone('helperA','branchA'),bone('tipA','helperA',physical),bone('branchB','head',physical),bone('tail_named_without_physics','pelvis')];
const options:PreparationOptions={source_roles:{pelvis:'pelvis',head:'head',upperarm_L:'arm'},target_roles:{pelvis:'hero pelvis',head:'hero head',upperarm_L:'hero arm'},mapping:{branchA:'hero pelvis'},mapping_anchors:{branchA:'pelvis'},reviewed:true};
const weighted=['branchA','tipA','branchB','arm','tail_named_without_physics'];
describe('existing physics rig choices',()=>{
  it('automatically keeps usable named chains without rigid bodies and respects an explicit opt out',()=>{
    const source=[bone('pelvis',null),bone('head','pelvis'),{...bone('hair_root','head'),position:[0,0,4] as [number,number,number]},{...bone('hair_tip','hair_root'),position:[0,0,2] as [number,number,number]},bone('rear_support','pelvis')];
    const weights=['hair_root','hair_tip','rear_support'];
    expect(physicsGroups(source,weights,options).map(g=>g.root)).toEqual(['hair_root']);
    const automatic=automaticPhysicsOptions(source,weights,options);
    expect(automatic).toMatchObject({preserve_physics_bones:true,physics_roots:['hair_root'],automatic_secondary_motion:true});
    expect(automaticPhysicsOptions(source,weights,{...options,preserve_physics_bones:false}).preserve_physics_bones).toBe(false);
    expect(physicsGroups(source.map(b=>({...b,position:[0,0,0]})),weights,options)).toEqual([]);
  });
  it('uses dynamic bindings and ancestry, keeps helpers, and excludes names alone or body-role branches',()=>{
    const groups=physicsGroups(bones,weighted,options);
    expect(groups.map(g=>[g.root,g.target_anchor,g.source_bones])).toEqual([['branchA','hero pelvis',['branchA','helperA','tipA']],['branchB','hero head',['branchB']]]);
    expect(keptPhysicsBones(groups,options).size).toBe(0);
    expect([...keptPhysicsBones(groups,{...options,preserve_physics_bones:true,physics_roots:['branchA']})]).toEqual(['branchA','helperA','tipA']);
    const bodyDynamic=bones.map(b=>b.name==='arm'?{...b,physics:physical}:b);
    expect(physicsGroups(bodyDynamic,weighted,options)).toEqual(groups);
    expect(physicsGroups([...bones,bones[0]],weighted,options)).toEqual([]);
  });
  it('selects all groups on first enable, supports individual review, and retains earlier mapping choices',()=>{
    const changed=vi.fn(),groups=physicsGroups(bones,weighted,options);
    function Harness(){const [value,setValue]=useState(options);return <PhysicsBoneChoices groups={groups} options={value} busy={false} change={change=>{const next={...value,...change};changed(next);setValue(next);}}/>;}
    render(<Harness/>);
    fireEvent.click(screen.getByLabelText('Keep existing physics bones (rig only)'));
    expect(screen.getByText('2 groups selected · 4 bones · 3 weighted bones')).toBeVisible();
    expect(screen.getByText(/does not create in-game swaying/)).toBeVisible();
    expect(changed.mock.lastCall![0]).toMatchObject({reviewed:false,mapping:options.mapping,mapping_anchors:options.mapping_anchors,physics_roots:['branchA','branchB']});
    fireEvent.click(screen.getByLabelText('branchB · 1 bones → hero head'));
    expect(changed.mock.lastCall![0].physics_roots).toEqual(['branchA']);
    fireEvent.click(screen.getByLabelText('Keep existing physics bones (rig only)'));
    expect(changed.mock.lastCall![0]).toMatchObject({preserve_physics_bones:false,mapping:options.mapping,mapping_anchors:options.mapping_anchors});
    fireEvent.click(screen.getByLabelText('Keep existing physics bones (rig only)'));
    fireEvent.click(screen.getByText('Keep all detected groups'));
    expect(changed.mock.lastCall![0].physics_roots).toEqual(['branchA','branchB']);
    fireEvent.click(screen.getByText('Clear group selection'));
    expect(changed.mock.lastCall![0].physics_roots).toEqual([]);
  });
  it('asks older inspections to refresh without inventing physics from bone names',()=>{
    render(<PhysicsBoneChoices groups={[]} options={{}} change={vi.fn()} busy={false}/>);
    expect(screen.getByLabelText('Keep existing physics bones (rig only)')).toBeDisabled();
    expect(screen.getByText(/press Inspect source again/)).toBeVisible();
  });
});

describe('unnamed and robe moving parts',()=>{
  it('matches the native rule: robes and long hanging chains on the torso, never fingers or short chains',()=>{
    const roles=hanging.roles as PreparationOptions['source_roles'];
    const groups=physicsGroups(hanging.bones as Bone[],hanging.weighted,{source_roles:roles,target_roles:roles});
    expect(groups.map(g=>g.root)).toEqual(hanging.expected_roots);
    expect(groups.find(g=>g.root==='Strap_00')?.source_anchor).toBe('Bip001_Pelvis');
    expect(physicsGroups(hanging.bones as Bone[],hanging.weighted,{source_roles:roles,target_roles:roles,breast_motion:true}).map(g=>g.root)).toEqual(hanging.expected_roots_with_breast_motion);
    expect(breastBones(hanging.bones as Bone[],hanging.weighted,roles!)).toEqual(hanging.breast_bones);
    const renamed=hanging.renamed_breast_bones as Record<string,string>,rename=(n:string)=>renamed[n]??n;
    expect(breastBones((hanging.bones as Bone[]).map(b=>({...b,name:rename(b.name)})),hanging.weighted.map(rename),roles!)).toEqual(hanging.breast_bones.map(rename));
  });
  it('adds groups detected for the first time to an automatic project but never re-adds one you left off',()=>{
    const roles=hanging.roles as PreparationOptions['source_roles'];
    const saved:PreparationOptions={source_roles:roles,target_roles:roles,preserve_physics_bones:true,automatic_secondary_motion:true,physics_roots:['Strap_00']};
    expect(automaticPhysicsOptions(hanging.bones as Bone[],hanging.weighted,saved).physics_roots).toEqual(['Strap_00',...hanging.expected_roots.filter(r=>r!=='Strap_00')]);
    const declined={...saved,physics_offered:hanging.expected_roots};
    expect(automaticPhysicsOptions(hanging.bones as Bone[],hanging.weighted,declined).physics_roots).toEqual(['Strap_00']);
    expect(automaticPhysicsOptions(hanging.bones as Bone[],hanging.weighted,{...saved,automatic_secondary_motion:false}).physics_roots).toEqual(['Strap_00']);
  });
});

describe('gun charms',()=>{
  const roles=hanging.roles as PreparationOptions['source_roles'],bones=hanging.bones as Bone[],gun=hanging.gun;
  const fresh:PreparationOptions={source_roles:roles,target_roles:roles};
  it('matches the native rule: chains hanging from the gun move, gun parts and parts on the hand do not',()=>{
    const groups=physicsGroups(bones,hanging.weighted,fresh);
    expect(groups.filter(g=>hangsFromGun(g.root,bones,roles!,gun)).map(g=>g.root)).toEqual(hanging.gun_charm_roots);
    expect(groups.find(g=>g.root==='Pistol_Link_00')?.source_bones).toEqual(['Pistol_Link_00','Pistol_Link_01','Pistol_Link_02']);
    expect(groups.flatMap(g=>g.source_bones).some(b=>/^(Pistol_Slide|Pistol_Mag|Pistol_Fin|Glove_Tag)/.test(b))).toBe(false);
    // Without the inspection's gun marks only named charms move.
    const unmarked=bones.map(b=>{const copy={...b};delete copy.weapon;return copy;});
    expect(physicsGroups(unmarked,hanging.weighted,fresh).map(g=>g.root)).toEqual(hanging.expected_roots.filter(r=>!hanging.gun_charm_roots.includes(r)));
    expect(charmNamed('Key_Chain_00')&&charmNamed('キーホルダー')&&!charmNamed('Keystone')).toBe(true);
  });
  it('gives gun charms the charm part type on a new model and keeps other part types',()=>{
    const next=automaticPhysicsOptions(bones,hanging.weighted,fresh);
    expect(next.physics_roots).toEqual(hanging.expected_roots);
    expect(next.physics_kinds).toEqual({Pistol_Link_00:'charm',Pistol_Strap_00:'charm'});
    expect(automaticPhysicsOptions(bones,hanging.weighted,{...fresh,physics_kinds:{Pistol_Strap_00:'cloth'}}).physics_kinds).toEqual({Pistol_Link_00:'charm',Pistol_Strap_00:'cloth'});
  });
  it('leaves gun charms out with the gun and offers them again when the gun comes back',()=>{
    const kept={...automaticPhysicsOptions(bones,hanging.weighted,fresh),weapon:{enabled:true,action:'replace' as const,objects:[gun]}};
    const out=gunChoiceOptions(bones,hanging.weighted,kept,{weapon:{...kept.weapon,action:'remove'}});
    expect(out.physics_roots).toEqual(hanging.expected_roots.filter(r=>!hanging.gun_charm_roots.includes(r)));
    const removed={...kept,...out};
    expect(physicsGroups(bones,hanging.weighted,removed).map(g=>g.root)).toEqual(out.physics_roots);
    expect(gunChoiceOptions(bones,hanging.weighted,removed,{weapon:{...kept.weapon,action:'keep'}}).physics_roots).toEqual([...out.physics_roots!,...hanging.gun_charm_roots]);
    // Changing the fit only does not touch the moving parts.
    expect(gunChoiceOptions(bones,hanging.weighted,kept,{weapon:{...kept.weapon,roll:15}})).toEqual({weapon:{...kept.weapon,roll:15}});
  });
  it('shows charms on a replaced gun hanging from the fitted gun',()=>{
    const options={...automaticPhysicsOptions(bones,hanging.weighted,fresh),weapon:{enabled:true,action:'replace' as const,objects:[gun]}};
    const groups=physicsGroups(bones,hanging.weighted,options);
    render(<PhysicsBoneChoices groups={groups} gunParts={hanging.gun_charm_roots} options={options} change={vi.fn()} busy={false}/>);
    expect(screen.getByLabelText('Pistol_Link_00 · 3 bones → the fitted gun')).toBeChecked();
    expect(screen.getByLabelText('Key_Chain_00 · 2 bones → Bip001_L_Hand')).toBeChecked();
    expect(screen.getByText(/Gun charms: Pistol_Link_00, Pistol_Strap_00 hang from the gun. They keep their own bones on the fitted gun/)).toBeVisible();
  });
});

describe('optional breast motion',()=>{
  const roles=hanging.roles as PreparationOptions['source_roles'];
  const base:PreparationOptions={source_roles:roles,target_roles:roles,preserve_physics_bones:true,automatic_secondary_motion:true,physics_roots:['Strap_00']};
  it('is off by default and adds or removes only the breast groups with the breast part type',()=>{
    expect(automaticPhysicsOptions(hanging.bones as Bone[],hanging.weighted,{source_roles:roles,target_roles:roles}).physics_roots).not.toContain('Bone_Breast_L_00');
    const on=breastMotionOptions(hanging.bones as Bone[],hanging.weighted,base,true);
    expect(on).toMatchObject({breast_motion:true,preserve_physics_bones:true,automatic_secondary_motion:true,physics_roots:['Strap_00','Bone_Breast_L_00','Bone_Breast_R_00','lPectoral','rPectoral'],physics_kinds:{Bone_Breast_L_00:'breast',Bone_Breast_R_00:'breast',lPectoral:'breast',rPectoral:'breast'}});
    const off=breastMotionOptions(hanging.bones as Bone[],hanging.weighted,on,false);
    expect(off).toMatchObject({breast_motion:false,physics_roots:['Strap_00'],physics_kinds:{}});
    expect(physicsGroups(hanging.bones as Bone[],hanging.weighted,off).map(g=>g.root)).toEqual(hanging.expected_roots);
  });
  it('offers breast motion with or without breast bones and explains each state',()=>{
    const toggle=vi.fn();
    render(<PhysicsBoneChoices groups={[]} options={base} change={vi.fn()} busy={false} breasts={{count:2,toggle}}/>);
    fireEvent.click(screen.getByLabelText('Keep or add breast bones (for breast motion)'));
    expect(toggle).toHaveBeenCalledWith(true);
    expect(screen.getByText(/2 breast bones were found; they follow the chest/)).toBeVisible();
    cleanup();
    render(<PhysicsBoneChoices groups={[]} options={base} change={vi.fn()} busy={false} breasts={{count:0,toggle}}/>);
    expect(screen.getByText(/turning this on adds them where the chest has breasts/)).toBeVisible();
    cleanup();
    const reinspect=vi.fn();
    render(<PhysicsBoneChoices groups={[]} options={{...base,breast_motion:true}} change={vi.fn()} busy={false} breasts={{count:0,toggle,reinspect}}/>);
    fireEvent.click(screen.getByRole('button',{name:'Inspect again to add breast bones'}));
    expect(reinspect).toHaveBeenCalled();
    cleanup();
    render(<PhysicsBoneChoices groups={[]} options={{...base,breast_motion:true}} change={vi.fn()} busy={false} breasts={{count:0,toggle,result:{status:'flat',reason:'The chest is too flat for breast motion.'}}}/>);
    expect(screen.getByText(/No breast bones were added: The chest is too flat/)).toBeVisible();
    expect(screen.queryByRole('button',{name:'Inspect again to add breast bones'})).toBeNull();
    cleanup();
    render(<PhysicsBoneChoices groups={[]} options={{...base,breast_motion:true}} change={vi.fn()} busy={false} breasts={{count:2,toggle,added:true}}/>);
    expect(screen.getByText(/Breast bones were added where the chest has breasts. Turn the bounce on or off/)).toBeVisible();
  });
  it('returns added breast bones to the chest when breast motion is turned off',()=>{
    const bones=[...hanging.bones as Bone[],{name:'DMB_Breast_L',parent:'Bip001_Spine1',position:[0.05,-0.03,1.23]},{name:'DMB_Breast_R',parent:'Bip001_Spine1',position:[-0.05,-0.03,1.23]}] as Bone[];
    const off=breastMotionOptions(bones,[...hanging.weighted,'DMB_Breast_L','DMB_Breast_R'],{...base,target_roles:{chest:'spine_3'}},false);
    expect(off.mapping).toMatchObject({DMB_Breast_L:'spine_3',DMB_Breast_R:'spine_3'});
  });
});
