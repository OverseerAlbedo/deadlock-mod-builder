import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SceneObjects } from './SceneObjects';
import { WeaponPanel, weaponDefaults } from './WeaponPanel';
import { PreparationPage } from './PreparationPage';
import { WeaponsSection } from './WeaponsSection';
import type { PreparationAnalysis, PreparationOptions, SceneChoices, SceneReceipt, WeaponCandidate } from './models';
import type { Project } from '../models';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn(),preparationWeaponPreview:vi.fn(),validateAssetPath:vi.fn(),discoverWeaponModels:vi.fn(),newProject:vi.fn(),selectHero:vi.fn(),saveProject:vi.fn(),openProject:vi.fn()}));
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
beforeEach(()=>{vi.clearAllMocks();native.preparationWeaponPreview.mockReturnValue(new Promise(()=>undefined));});

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

const frame={grip:[0,0,0],muzzle:[1,0,0],forward:[1,0,0],up:[0,0,1],length:1,confidence:'high',notes:[]};
const fitOf=(scale:number,muzzle:number)=>({policy:'weapon-fit-v1',scale,muzzle_offset_mm:muzzle,skin:'rigid',skin_bones:['weapon'],notes:[],matrix:[[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]],source:frame,target:frame});
describe('Weapon tab',()=>{
  function Harness({analysis,start={},separate}:{analysis:PreparationAnalysis;start?:PreparationOptions;separate?:Parameters<typeof WeaponPanel>[0]['separate']}){const[options,setOptions]=useState<PreparationOptions>(start);return <><WeaponPanel project={project} analysis={analysis} options={options} change={values=>setOptions(current=>({...current,...values}))} busy={false} operate={async(_,task)=>task()} separate={separate}/><output data-testid="options">{JSON.stringify(options)}</output></>;}
  const current=()=>JSON.parse(screen.getByTestId('options').textContent!) as PreparationOptions;
  it('explains the hero gun in plain words, recommends replacing it and previews the fit live',async()=>{
    const analysis=withGun(heroGun);
    native.preparationWeaponPreview.mockResolvedValue({fitted:[[0,0,0]],reference:[[0,0,0]],fit:fitOf(26.5,14.8)});
    render(<Harness analysis={analysis} start={{weapon:weaponDefaults(analysis,undefined)}}/>);
    expect(screen.getByText(/gun is part of .*model, so your gun can replace it directly/)).toBeVisible();
    expect(screen.getByText(/Your character file has a gun: Pistol \(in your character file\)/)).toBeVisible();
    expect(screen.getByLabelText(/Use my gun instead of/)).toBeChecked();
    expect(screen.getByText(/Your gun replaces .* gun\. It is fitted to the original gun's grip, barrel and size/)).toBeVisible();
    // The preview appears without a button and follows every change.
    expect(await screen.findByText('gun viewport')).toBeVisible();
    expect(screen.getByText(/Size: the length of .*'s gun · the muzzle lines up with the original/)).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Bigger'}));
    expect(current().weapon).toMatchObject({enabled:true,action:'replace',objects:['Pistol'],scale:1.1});
    expect(current().reviewed).toBe(false);
    expect(await screen.findByText(/Size: the length of .*'s gun × 110% · /)).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Flip direction'}));
    fireEvent.click(screen.getByRole('button',{name:'Turn ⟳ 90°'}));
    fireEvent.click(screen.getByRole('button',{name:'▲ Up'}));
    expect(current().weapon).toMatchObject({flip:true,roll:90,offset:[0,.25,0]});
    await vi.waitFor(()=>expect(native.preparationWeaponPreview).toHaveBeenLastCalledWith(project,analysis,expect.objectContaining({weapon:expect.objectContaining({roll:90})})));
    // Expert settings stay available under Advanced.
    fireEvent.change(screen.getByLabelText('Gun size'),{target:{value:'keep_size'}});
    expect(current().weapon).toMatchObject({fit:'keep_size'});
    fireEvent.click(screen.getByLabelText(/Join each gun's parts into one mesh/));
    expect(current().weapon).toMatchObject({merge:false});
    fireEvent.click(screen.getByLabelText(/^Keep (?!my)/));
    expect(current().weapon).toMatchObject({action:'remove'});
    expect(screen.getByText(/keeps their own gun; your gun is left out of the character/)).toBeVisible();
    expect(screen.queryByRole('button',{name:'Bigger'})).not.toBeInTheDocument();
  });
  it('gives a dual-wield hero a gun in each hand, with copy, keep and swap',async()=>{
    const left:WeaponCandidate={...pistol,id:'gun_file_L',objects:['gun_file_L'],gun_file:true,side:'left',attach_kind:'placed'};
    const right:WeaponCandidate={...pistol,id:'gun_file_R',objects:['gun_file_R'],gun_file:true,side:'right',attach_kind:'placed'};
    const dual={...heroGun,meshes:['haze_gun_r','haze_gun_l'],sides:[{side:'right' as const,meshes:['haze_gun_r'],primary:['haze_gun_r'],bones:['weapon_hand_R']},{side:'left' as const,meshes:['haze_gun_l'],primary:['haze_gun_l'],bones:['weapon_hand_L']}]};
    const analysis={...withGun(dual),analysis:{...withGun(dual).analysis,weapons:[right,left]},weapon_suggestion:{enabled:true,action:'replace' as const,objects:['gun_file_R'],left:{objects:['gun_file_L']}}};
    native.preparationWeaponPreview.mockResolvedValue({fitted:[[0,0,0]],reference:[[0,0,0]],fit:fitOf(1,3),hands:[{side:'right',fit:fitOf(1,3),fitted:[[0,0,0]],reference:[[0,0,0]]},{side:'left',fit:{...fitOf(1.2,90),source:{...frame,direction:'names'}},fitted:[[0,0,0]],reference:[[0,0,0]]}]});
    render(<Harness analysis={analysis} start={{weapon_source:gunPath,weapon:weaponDefaults(analysis,undefined)}}/>);
    expect(screen.getByText(/holds two guns, one in each hand/)).toBeVisible();
    expect(screen.getByLabelText(/Use my guns instead of/)).toBeChecked();
    expect(screen.getByText(/Your right gun replaces .*right gun, and your left gun replaces the left one/)).toBeVisible();
    expect(screen.getByLabelText('Right hand')).toHaveDisplayValue('Right gun from pistol.blend · 1 part');
    expect(screen.getByLabelText('Left hand')).toHaveDisplayValue('Left gun from pistol.blend · 1 part');
    expect(await screen.findByText('gun viewport')).toBeVisible();
    // Each hand has its own adjustments.
    fireEvent.click(screen.getByRole('button',{name:'Left-hand gun'}));
    fireEvent.click(screen.getByRole('button',{name:'Smaller'}));
    expect(current().weapon?.left).toMatchObject({objects:['gun_file_L'],scale:.91});
    expect(current().weapon?.scale).toBeUndefined();
    expect(await screen.findByText(/× 91% · the muzzle ends 90 mm from the original .* · its front was found from the part names/)).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Swap hands'}));
    expect(current().weapon).toMatchObject({objects:['gun_file_L'],scale:.91,left:{objects:['gun_file_R'],scale:1}});
    fireEvent.change(screen.getByLabelText('Left hand'),{target:{value:'copy'}});
    expect(current().weapon?.left).toMatchObject({objects:[],keep:false});
    expect(screen.getByText(/a copy goes in each hand/)).toBeVisible();
    fireEvent.change(screen.getByLabelText('Left hand'),{target:{value:'keep'}});
    expect(current().weapon?.left).toMatchObject({keep:true});
    expect(screen.getByText(/the left hand keeps .* own gun/)).toBeVisible();
    expect(screen.queryByRole('button',{name:'Left-hand gun'})).not.toBeInTheDocument();
  });
  it('makes a separate gun model from your gun with one click and links it into the mod',async()=>{
    const analysis=withGun({...heroGun,meshes:[],primary:[],source:'skeleton',notes:['This hero model has no gun mesh of its own; its gun is a separate model.']});
    const child={...project,id:'child',original_model:'models/weapons/test/test_gun.vmdl',model_input_mode:'weapon'} as unknown as Project;
    native.discoverWeaponModels.mockResolvedValue([{path:'models/weapons/test/test_gun.vmdl_c',target:'models/weapons/test/test_gun.vmdl',label:'Weapon model: test_gun.vmdl',hero:'Test',supported:true,requires_model_project:true}]);
    native.newProject.mockResolvedValue({project:child,path:null});native.selectHero.mockResolvedValue(child);
    native.saveProject.mockImplementation(async(path:string,saved:Project)=>({project:saved,path}));
    const childAnalysis={...analysis,weapon_suggestion:{enabled:true,action:'replace',objects:['Pistol']}};
    native.preparationInspect.mockResolvedValue(childAnalysis);native.preparationReview.mockResolvedValue([]);
    native.preparationConvert.mockResolvedValue({dmx:'C:\\w\\prepared.dmx',blend:'C:\\w\\prepared.blend',report:'C:\\w\\report.json',textures:'C:\\w\\textures',target_model:child.original_model,warnings:[]});
    native.preparationAccept.mockImplementation(async(saved:Project)=>saved);
    native.preparationWeaponPreview.mockResolvedValue({fitted:[[0,0,0]],reference:[[0,0,0]],fit:fitOf(1,2)});
    const updateProject=vi.fn();
    render(<Harness analysis={analysis} start={{weapon:weaponDefaults(analysis,undefined)}} separate={{projectPath:'C:\\mods\\Mine.dmb.json',updateProject}}/>);
    expect(screen.getByText(/gun is a separate model\. The app can make it from your gun/)).toBeVisible();
    const use=await screen.findByRole('button',{name:/Use my gun instead of/});
    await vi.waitFor(()=>expect(use).toBeEnabled());
    fireEvent.click(use);
    await vi.waitFor(()=>expect(updateProject).toHaveBeenCalled());
    expect(native.selectHero).toHaveBeenCalledWith(expect.anything(),'models/weapons/test/test_gun.vmdl');
    const sent=native.preparationConvert.mock.calls[0];
    expect(sent[0]).toMatchObject({model_input_mode:'weapon',preparation_source:project.preparation_source});
    expect(sent[2]).toMatchObject({reviewed:true,weapon:{enabled:true,action:'replace',objects:['Pistol']}});
    expect(native.saveProject).toHaveBeenCalledWith('C:\\mods\\Mine.gun-test_gun.dmb.json',expect.anything());
    const linked=updateProject.mock.calls[0][0] as Project;
    expect(linked.customization.equipment).toMatchObject({enabled:true,variants:[{enabled:true,model:'models/weapons/test/test_gun.vmdl',project:'C:\\mods\\Mine.gun-test_gun.dmb.json'}]});
    // Your gun leaves the character's body: the gun model carries it now.
    expect((linked.preparation_options as PreparationOptions).weapon).toMatchObject({action:'remove',objects:['Pistol']});
    expect(await screen.findByText('gun viewport')).toBeVisible();
  });
  it('offers to stop building a separate weapon model when the hero gun is part of the hero model',()=>{
    const analysis=withGun(heroGun),updateProject=vi.fn();
    const linked={...project,customization:{...project.customization,equipment:{enabled:true,variants:[{enabled:true,model:'models/particle/haze_gun_magazine_model_fx.vmdl',project:'C:\\mods\\Haze_weapon.dmb.json'}]}}} as Project;
    render(<WeaponPanel project={linked} analysis={analysis} options={{weapon:weaponDefaults(analysis,undefined)}} change={vi.fn()} busy={false} operate={async(_,task)=>task()} separate={{updateProject}}/>);
    expect(screen.getByText(/also builds a separate weapon model \(haze_gun_magazine_model_fx\.vmdl\)/)).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Stop building it'}));
    expect((updateProject.mock.calls[0][0] as Project).customization.equipment?.variants).toEqual([]);
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
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByText('What we found in your model');
    expect(screen.getByText('Scene objects')).toBeVisible();
    fireEvent.click(screen.getByRole('tab',{name:/Weapon/}));
    expect(screen.getByLabelText(/Use my gun instead of/)).toBeChecked();
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));
    await screen.findByText('Choose which object in the source is the gun (Weapon tab).');
    const sent=native.preparationReview.mock.calls.at(-1)![2];
    expect(sent.weapon).toMatchObject({action:'replace',objects:['Pistol']});
    expect(sent.complete_mapping).toBe(true);
    fireEvent.click(screen.getByRole('tab',{name:/Body roles/}));
    fireEvent.click(screen.getByRole('button',{name:/Open section/}));
    expect(screen.getByRole('tab',{name:/Weapon/})).toHaveAttribute('aria-selected','true');
  });
});

// Native paths: the picker returns canonical (\\?\) spellings.
const verbatim='\\\\?\\',gunPath='C:\\guns\\pistol.blend',workspaceGun='C:\\workspace\\weapon-file\\pistol.blend';
describe('Gun file',()=>{
  function Harness({analysis,start={},reinspect=()=>undefined,mode='rigged_humanoid'}:{analysis:PreparationAnalysis;start?:PreparationOptions;reinspect?:()=>void;mode?:Project['model_input_mode']}){const[options,setOptions]=useState<PreparationOptions>(start);return <><WeaponPanel project={{...project,model_input_mode:mode}} analysis={analysis} options={options} change={values=>setOptions(current=>JSON.parse(JSON.stringify({...current,...values})) as PreparationOptions)} busy={false} operate={async(_,task)=>task()} reinspect={reinspect}/><output data-testid="options">{JSON.stringify(options)}</output></>;}
  const current=()=>JSON.parse(screen.getByTestId('options').textContent!) as PreparationOptions;
  const fromFile:WeaponCandidate={...pistol,id:'gun_file_Cube',objects:['gun_file_Cube'],keywords:['gun'],attach_kind:'placed',gun_file:true};
  const inspected=(source:string):PreparationAnalysis=>{const analysis=withGun(heroGun);return {...analysis,weapon_suggestion:{enabled:true,action:'replace',objects:['gun_file_Cube']},
    weapon_file:{selection:{source},source:workspaceGun,source_sha256:'c'.repeat(64),hand:'hand_R'},
    analysis:{...analysis.analysis,weapons:[pistol,fromFile],weapon_file:{policy:'gun-file-v1',file:'pistol.blend',objects:['gun_file_Cube'],bone:'hand_R',hand:'hand_R',placed:true}}};};
  it('takes the gun from another file and asks for a new inspection',async()=>{
    native.validateAssetPath.mockImplementation(async(path:string)=>verbatim+path);
    const reinspect=vi.fn();
    const analysis={...base,analysis:{...base.analysis,weapons:[],scene:receipt},weapon_reference:heroGun} as PreparationAnalysis;
    render(<Harness analysis={analysis} start={{reviewed:true,weapon:{enabled:true,action:'keep',objects:['Old']}}} reinspect={reinspect}/>);
    expect(screen.getByText('Choose a gun file above to use a gun from another file.')).toBeVisible();
    fireEvent.change(screen.getByLabelText('Gun from another file (optional)'),{target:{value:gunPath}});
    fireEvent.blur(screen.getByLabelText('Gun from another file (optional)'));
    await vi.waitFor(()=>expect(current().weapon_source).toBe(verbatim+gunPath));
    expect(native.validateAssetPath).toHaveBeenCalledWith(gunPath,'weapon');
    // The old gun choice named objects of the previous inspection.
    expect(current().weapon).toBeUndefined();
    expect(current().reviewed).toBe(false);
    expect(screen.getByText('Loading the gun from this file needs a new inspection.')).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Load the gun from this file'}));
    expect(reinspect).toHaveBeenCalledOnce();
  });
  it('offers the inspected gun file gun first and says where it was put',()=>{
    const analysis=inspected(gunPath.toUpperCase());
    render(<Harness analysis={analysis} start={{weapon_source:verbatim+'c:/guns/pistol.blend',weapon:weaponDefaults(analysis,undefined)}}/>);
    expect(screen.queryByText(/needs a new inspection/)).not.toBeInTheDocument();
    expect(screen.getByText('Your gun file pistol.blend has one gun; it was put in the right hand (hand_R).')).toBeVisible();
    expect(screen.getByText(/Your character file also has a gun \(Pistol\)\. It stays on the character unless you set it aside in Scene objects/)).toBeVisible();
    expect(screen.getByLabelText('Your gun')).toHaveDisplayValue('Gun from pistol.blend · 1 part');
    expect(screen.getByLabelText(/Use my gun instead of/)).toBeChecked();
    expect(current().weapon).toMatchObject({action:'replace',objects:['gun_file_Cube']});
  });
  it('asks for a new inspection after the gun file is cleared',()=>{
    render(<Harness analysis={inspected(gunPath)} start={{}}/>);
    expect(screen.getByText("Removing the gun file's gun needs a new inspection.")).toBeVisible();
  });
  it('weapon projects take the gun file as their source instead',()=>{
    render(<Harness analysis={{...base,analysis:{...base.analysis,weapons:[]},weapon_reference:{...heroGun,separate_model:true}} as PreparationAnalysis} mode="weapon"/>);
    expect(screen.queryByLabelText('Gun from another file (optional)')).not.toBeInTheDocument();
    expect(screen.getByText(/To use a gun from another file, choose that file as the source path and inspect again/)).toBeVisible();
  });
});

describe('Preparation page with a gun file',()=>{
  it('keeps the gun file through inspection and shows the Weapon tab without a gun in the source',async()=>{
    const start={...project,preparation_options:{weapon_source:gunPath}} as unknown as Project;
    function Harness(){const[current,setCurrent]=useState(start);return <><PreparationPage project={current} update={setCurrent} busy={false} operate={async(_,task)=>task()}/><output data-testid="project">{JSON.stringify(current.preparation_options)}</output></>;}
    const analysis={...base,analysis:{...base.analysis,weapons:[],scene:receipt},weapon_reference:heroGun} as PreparationAnalysis;
    native.preparationInspect.mockResolvedValue(analysis);native.preparationSuggest.mockResolvedValue(analysis.suggestions);
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByText('What we found in your model');
    expect(native.preparationInspect.mock.calls[0][0].preparation_options.weapon_source).toBe(gunPath);
    expect(JSON.parse(screen.getByTestId('project').textContent!).weapon_source).toBe(gunPath);
    fireEvent.click(screen.getByRole('tab',{name:/Weapon/}));
    expect(screen.getByText('Loading the gun from this file needs a new inspection.')).toBeVisible();
  });
});

describe('Appearance & weapons gun card',()=>{
  const operate=async(_label:string,run:()=>Promise<void>)=>{await run();};
  it('does not ask to prepare again when the gun file is the same file spelled differently',async()=>{
    const analysis={...withGun(heroGun),weapon_file:{selection:{source:'C:\\Guns\\Dual.blend'},source:'C:\\prep\\Dual.blend',source_sha256:'c'.repeat(64)}} as PreparationAnalysis;
    native.preparationResume.mockResolvedValue({analysis});
    // As saved by the file picker: long-path prefix, other case and slashes.
    const prepared={...project,preparation_report:'C:\\prep\\same-gun\\report.json',preparation_options:{weapon_source:'\\\\?\\c:/guns/dual.blend',weapon:analysis.weapon_suggestion}} as unknown as Project;
    const props={update:()=>undefined,busy:false,operate,openPreparation:()=>undefined};
    const {rerender}=render(<WeaponsSection project={prepared} {...props}/>);
    expect(await screen.findByText(/gun is part of .*model/)).toBeVisible();
    expect(screen.queryByText('Your gun file changed.')).not.toBeInTheDocument();
    rerender(<WeaponsSection project={{...prepared,preparation_options:{...prepared.preparation_options,weapon_source:'C:\\Guns\\Other.blend'}} as unknown as Project} {...props}/>);
    expect(screen.getByText('Your gun file changed.')).toBeVisible();
  });
});
