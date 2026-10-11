import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SceneObjects } from './SceneObjects';
import { WeaponPanel, weaponDefaults } from './WeaponPanel';
import { PreparationPage } from './PreparationPage';
import type { PreparationAnalysis, PreparationOptions, SceneChoices, SceneReceipt, WeaponCandidate } from './models';
import type { Project } from '../models';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn(),preparationWeaponPreview:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
vi.mock('./WeaponViewport',()=>({default:()=><div>gun viewport</div>}));

const receipt:SceneReceipt={policy:'character-scene-isolation-v1',character_rig:'Armature',notes:['Applied modifiers on Body: Subdivision.'],warning:'',objects:[
  {name:'Armature',type:'ARMATURE',status:'character',reason:'character rig'},
  {name:'NPC_Rig',type:'ARMATURE',status:'excluded',reason:'another rig'},
  {name:'Body',type:'MESH',status:'included',reason:'character geometry'},
  {name:'Floor',type:'MESH',status:'excluded',reason:'not attached to the character (ground or backdrop)'},
  {name:'Camera',type:'CAMERA',status:'ignored',reason:'camera (not character geometry)'},
]};
const pistol:WeaponCandidate={id:'Pistol',objects:['Pistol'],vertices:24,bounds:[[0,0,0],[1,1,1]],sample:[[0,0,0]],attach_bone:'hand_R',attach_kind:'bone_parent',keywords:['pistol'],elongation:3};
const base={source:'C:\\source.blend',source_original:'C:\\source.blend',source_sha256:'a'.repeat(64),target_model:'models/heroes/test.vmdl',target_sha256:'b'.repeat(64),analysis:sourceFixture.inspection,target:sourceFixture.target,target_skeleton:sourceFixture.skeleton,target_scale_audit:sourceFixture.scale_audit,suggestions:sourceFixture.cases[1].expected,rotation:[0,0,0],geometry_customization:{props:[],face:null},clothing:null} as unknown as PreparationAnalysis;
const heroGun={policy:'weapon-fit-v1',source:'meshes',separate_model:false,meshes:['test_gun','test_gun_lod1'],primary:['test_gun'],model_meshes:['test_body','test_gun','test_gun_lod1'],bones:['weapon'],notes:[]};
const withGun=(reference:PreparationAnalysis['weapon_reference']):PreparationAnalysis=>({...base,analysis:{...base.analysis,weapons:[pistol],scene:receipt},weapon_reference:reference,weapon_suggestion:{enabled:true,action:reference?.meshes.length?'replace':'remove',objects:['Pistol']}});
const project={...projectFixture.cases[0].expected,original_model:base.target_model,preparation_source:base.source_original,preparation_options:{},model_input_mode:'rigged_humanoid'} as unknown as Project;
beforeEach(()=>{vi.clearAllMocks();});

describe('Scene objects',()=>{
  function Harness({reinspect}:{reinspect:()=>void}){const[choices,setChoices]=useState<SceneChoices>({});return <><SceneObjects receipt={receipt} choices={choices} change={setChoices} busy={false} reinspect={reinspect}/><output data-testid="choices">{JSON.stringify(choices)}</output></>;}
  it('lists kept and set-aside objects and records overrides for the next inspection',()=>{
    const reinspect=vi.fn();render(<Harness reinspect={reinspect}/>);
    expect(screen.getByText('not attached to the character (ground or backdrop)')).toBeVisible();
    expect(screen.getByText('Applied modifiers on Body: Subdivision.')).toBeVisible();
    expect(screen.getByLabelText('Keep Camera')).toBeDisabled();
    expect(screen.getByLabelText('Keep Floor')).not.toBeChecked();
    fireEvent.click(screen.getByLabelText('Keep Floor'));
    fireEvent.click(screen.getByLabelText('Keep Body'));
    fireEvent.change(screen.getByLabelText('Character rig'),{target:{value:'NPC_Rig'}});
    expect(JSON.parse(screen.getByTestId('choices').textContent!)).toEqual({exclude:['Body'],include:['Floor'],character:'NPC_Rig'});
    fireEvent.click(screen.getByRole('button',{name:'Inspect again with these choices'}));
    expect(reinspect).toHaveBeenCalled();
  });
});

describe('Weapon tab',()=>{
  function Harness({analysis,start={}}:{analysis:PreparationAnalysis;start?:PreparationOptions}){const[options,setOptions]=useState<PreparationOptions>(start);return <><WeaponPanel project={project} analysis={analysis} options={options} change={values=>setOptions(current=>({...current,...values}))} busy={false} operate={async(_,task)=>task()}/><output data-testid="options">{JSON.stringify(options)}</output></>;}
  const current=()=>JSON.parse(screen.getByTestId('options').textContent!) as PreparationOptions;
  it('replaces a gun that is part of the hero model and previews the fit',async()=>{
    const analysis=withGun(heroGun);
    native.preparationWeaponPreview.mockResolvedValue({fitted:[[0,0,0]],reference:[[0,0,0]],fit:{policy:'weapon-fit-v1',scale:26.5,muzzle_offset_mm:14.8,skin:'rigid',skin_bones:['weapon'],notes:[],matrix:[[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]],source:{grip:[0,0,0],muzzle:[1,0,0],forward:[1,0,0],up:[0,0,1],length:1,confidence:'high',notes:[]},target:{grip:[0,0,0],muzzle:[1,0,0],forward:[1,0,0],up:[0,0,1],length:1,confidence:'high',notes:[]}}});
    render(<Harness analysis={analysis} start={{weapon:weaponDefaults(analysis,undefined)}}/>);
    expect(screen.getByText(/part of the hero model \(test_gun, test_gun_lod1\)/)).toBeVisible();
    expect(screen.getByLabelText(/Replace the hero's gun with it/)).toBeChecked();
    fireEvent.change(screen.getByLabelText('Gun size'),{target:{value:'keep_size'}});
    expect(current().weapon).toMatchObject({enabled:true,action:'replace',objects:['Pistol'],fit:'keep_size'});
    expect(current().reviewed).toBe(false);
    fireEvent.click(screen.getByRole('button',{name:'Preview gun fit'}));
    expect(await screen.findByText('gun viewport')).toBeVisible();
    expect(screen.getByText('15 mm')).toBeVisible();
    expect(native.preparationWeaponPreview).toHaveBeenCalledWith(project,analysis,expect.objectContaining({weapon:expect.objectContaining({fit:'keep_size'})}));
    fireEvent.click(screen.getByLabelText('Leave it out of the character'));
    expect(current().weapon).toMatchObject({action:'remove'});
    expect(screen.queryByLabelText('Gun size')).not.toBeInTheDocument();
  });
  it('explains separate gun models and only offers leave-out or keep',()=>{
    const analysis=withGun({...heroGun,meshes:[],primary:[],source:'skeleton',notes:['This hero model has no gun mesh of its own; its gun is a separate model.']});
    render(<Harness analysis={analysis} start={{weapon:weaponDefaults(analysis,undefined)}}/>);
    expect(screen.getByText(/This hero's gun is a separate model/)).toBeVisible();
    expect(screen.getByLabelText(/Replace the hero's gun with it/)).toBeDisabled();
    expect(screen.getByLabelText('Leave it out of the character')).toBeChecked();
    fireEvent.change(screen.getByLabelText('Gun in your file'),{target:{value:''}});
    expect(current().weapon).toMatchObject({enabled:false,objects:[]});
  });
  it('keeps a still-valid earlier gun choice when inspecting again',()=>{
    const analysis=withGun(heroGun);
    expect(weaponDefaults(analysis,{enabled:true,action:'keep',objects:['Pistol']})).toEqual({enabled:true,action:'keep',objects:['Pistol']});
    expect(weaponDefaults(analysis,{enabled:true,action:'keep',objects:['Gone']})).toEqual(analysis.weapon_suggestion);
  });
});

describe('Preparation page with a gun in the source',()=>{
  function Harness(){const[current,setCurrent]=useState(project);return <PreparationPage project={current} update={setCurrent} busy={false} operate={async(_,task)=>task()}/>;}
  it('applies the gun suggestion, completes the mapping and opens the Weapon tab from review issues',async()=>{
    const analysis=withGun(heroGun);
    native.preparationInspect.mockResolvedValue(analysis);native.preparationSuggest.mockResolvedValue(analysis.suggestions);
    native.preparationReview.mockResolvedValue([{tab:'weapon',message:'Choose which object in the source is the gun (Weapon tab).'}]);
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');
    expect(screen.getByText('Scene objects')).toBeVisible();
    fireEvent.click(screen.getByRole('tab',{name:/Weapon/}));
    expect(screen.getByLabelText(/Replace the hero's gun with it/)).toBeChecked();
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));
    await screen.findByText('Choose which object in the source is the gun (Weapon tab).');
    const sent=native.preparationReview.mock.calls.at(-1)![2];
    expect(sent.weapon).toMatchObject({action:'replace',objects:['Pistol']});
    expect(sent.complete_mapping).toBe(true);
    fireEvent.click(screen.getByRole('tab',{name:/Body roles/}));
    fireEvent.click(screen.getByRole('button',{name:/Open section/}));
    expect(screen.getByRole('tab',{name:/Weapon/})).toHaveAttribute('aria-selected','true');
  });
});
