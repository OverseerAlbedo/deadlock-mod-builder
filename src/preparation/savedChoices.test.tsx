import { useState } from 'react';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PreparationPage } from './PreparationPage';
import { changeRole } from './mapping';
import { physicsGroups, automaticPhysicsOptions } from './secondary-bones';
import { bodyRoots, keepDetectedGroups, physicsForRoles, refreshWithRoles, withRoleChange } from './savedChoices';
import type { Bone, PreparationAnalysis, PreparationOptions, Role, Suggestions, Vec3 } from './models';
import type { Project } from '../models';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

// A rig whose first body guess took a rope hanging from the hips for the left leg: the real leg
// chain then looks like a long hanging part and is kept as a moving part.
const at=(name:string,parent:string|null,position:Vec3):Bone=>({name,parent,position});
const rig:Bone[]=[
  at('Hips',null,[0,0,10]),at('Spine',  'Hips',[0,0,12]),at('Chest','Spine',[0,0,14]),at('Neck','Chest',[0,0,16]),at('Head','Neck',[0,0,17]),
  at('LegA_L','Hips',[1,0,10]),at('LegB_L','LegA_L',[1,0,5]),at('LegC_L','LegB_L',[1,0,1]),at('LegD_L','LegC_L',[1,1,0]),
  at('Rope_L_0','Hips',[1,0.5,9.5]),at('Rope_L_1','Rope_L_0',[1,0.5,7]),at('Rope_L_2','Rope_L_1',[1,0.5,4.5]),
  at('Hair_0','Head',[0,-1,17]),at('Hair_1','Hair_0',[0,-1.5,15]),
];
const weighted=['Hips','Spine','Chest','Neck','Head','LegA_L','LegB_L','LegC_L','LegD_L','Rope_L_0','Rope_L_1','Rope_L_2','Hair_0','Hair_1'];
const torso={pelvis:'Hips',spine:'Spine',chest:'Chest',neck:'Neck',head:'Head'};
const wrong={...torso,thigh_L:'Rope_L_0',shin_L:'Rope_L_1',foot_L:'Rope_L_2'} as Partial<Record<Role,string>>;
const corrected={...torso,thigh_L:'LegA_L',shin_L:'LegB_L',foot_L:'LegC_L'} as Partial<Record<Role,string>>;
const hero={pelvis:'pelvis',spine:'spine',chest:'chest',neck:'neck',head:'head',thigh_L:'thigh_L',shin_L:'shin_L',foot_L:'foot_L'} as Partial<Record<Role,string>>;
const roots=(o:PreparationOptions)=>physicsGroups(rig,weighted,o).map(g=>g.root);
/** The page's path for a role edit: changeRole, then the saved moving parts follow the roles. */
const edit=(o:PreparationOptions,role:Role,name:string)=>withRoleChange(rig,weighted,o,changeRole(o,'source',role,name));
const correct=(o:PreparationOptions)=>edit(edit(edit(o,'thigh_L','LegA_L'),'shin_L','LegB_L'),'foot_L','LegC_L');
const guessed=():PreparationOptions=>automaticPhysicsOptions(rig,weighted,{source_roles:wrong,target_roles:hero,mapping:Object.fromEntries(Object.entries(wrong).map(([role,name])=>[name!,hero[role as Role]!]))});

describe('saved moving parts follow the body roles',()=>{
  it('keeps the real leg as a moving part under the wrong guess (the case being repaired)',()=>{
    expect(roots({source_roles:wrong,target_roles:hero})).toEqual(['Hair_0','LegA_L']);
    expect(roots({source_roles:corrected,target_roles:hero})).toEqual(['Hair_0','Rope_L_0']);
    expect(guessed()).toMatchObject({preserve_physics_bones:true,automatic_secondary_motion:true,physics_roots:['Hair_0','LegA_L']});
  });
  it('drops a kept root that a role change made part of the body and keeps the roles as chosen',()=>{
    const first=edit(guessed(),'thigh_L','LegA_L');
    expect(first.physics_roots).toEqual(['Hair_0']);
    expect(first.source_roles).toMatchObject({thigh_L:'LegA_L',shin_L:'Rope_L_1'});
    expect(bodyRoots(rig,{...first,physics_roots:['LegA_L','LegB_L','Hips','Hair_0','not a bone']})).toEqual(['LegA_L','Hips']);
    const done=correct(guessed());
    expect(done.source_roles).toEqual(corrected);
    // The rope is a hanging part now; it was never shown, so it is added like any new group.
    expect(done.physics_roots).toEqual(['Hair_0','Rope_L_0']);
    expect(roots(done)).toEqual(expect.arrayContaining(done.physics_roots!));
    // A root that is not a bone of this rig (another source file) is left to the review.
    expect(physicsForRoles(rig,weighted,{...done,physics_roots:[...done.physics_roots!,'saved branch']}).physics_roots).toContain('saved branch');
  });
  it('adds newly detected groups only when they were not offered before',()=>{
    const shown={...guessed(),physics_offered:['Hair_0','LegA_L','Rope_L_0'],physics_kinds:{LegA_L:'cloth'}};
    const done=correct(shown);
    expect(done.physics_roots).toEqual(['Hair_0']);
    expect(done.physics_offered).toEqual(['Hair_0','Rope_L_0']);
    expect(done.physics_kinds).toEqual({});
    // Turned off by hand: nothing is added either.
    expect(correct({...guessed(),automatic_secondary_motion:false}).physics_roots).toEqual(['Hair_0']);
  });
  it('lets a root dropped on the way come back when the roles return to it',()=>{
    const start={...correct(guessed()),physics_offered:['Hair_0','Rope_L_0']};
    const back=edit(edit(edit(start,'thigh_L','Rope_L_0'),'shin_L','Rope_L_1'),'foot_L','Rope_L_2');
    expect(back.physics_roots).toEqual(['Hair_0','LegA_L']);
    const again=correct(back);
    expect(again.physics_roots).toEqual(['Hair_0','Rope_L_0']);
  });
  it('drops a kept part that hung from the wrongly chosen leg once the freed chain is kept, and only then',()=>{
    // The chain taken for the leg is a named sash here, so once freed it is a moving part with the tassel inside it.
    const sash=(n:string)=>n.replace('Rope_','Sash_'),wrongSash=Object.fromEntries(Object.entries(wrong).map(([r,n])=>[r,sash(n!)]));
    const tassel=[...rig.map(b=>({...b,name:sash(b.name),parent:b.parent&&sash(b.parent)})),at('Tassel_0','Sash_L_1',[1.2,0.5,7]),at('Tassel_1','Tassel_0',[1.2,0.5,6])],weights=[...weighted.map(sash),'Tassel_0','Tassel_1'];
    const first=automaticPhysicsOptions(tassel,weights,{source_roles:wrongSash,target_roles:hero});
    expect(first.physics_roots).toEqual(['Hair_0','LegA_L','Tassel_0']);
    const step=(o:PreparationOptions,role:Role,name:string)=>withRoleChange(tassel,weights,o,changeRole(o,'source',role,name));
    const fix=(o:PreparationOptions)=>step(step(step(o,'thigh_L','LegA_L'),'shin_L','LegB_L'),'foot_L','LegC_L');
    expect(physicsGroups(tassel,weights,{source_roles:corrected,target_roles:hero}).map(g=>g.root)).toEqual(['Hair_0','Sash_L_0']);
    expect(fix(first).physics_roots).toEqual(['Hair_0','Sash_L_0']);
    // The freed chain was shown and left off: the part is not silently dropped (the review reports it).
    expect(fix({...first,physics_offered:['Hair_0','LegA_L','Tassel_0','Sash_L_0']}).physics_roots).toEqual(['Hair_0','Tassel_0']);
  });
  it('changes nothing when an option change leaves the roles alone',()=>{
    const saved={...guessed(),source_roles:corrected};
    expect(withRoleChange(rig,weighted,saved,{...saved,fit_mode:'preserve_proportions'})).toEqual({...saved,fit_mode:'preserve_proportions'});
  });
  it('refreshes the matches with the user roles and keeps every role bone on its target',()=>{
    const options:PreparationOptions={source_roles:corrected,target_roles:hero,mapping:{LegA_L:'thigh_L',Hair_1:'chest'},mapping_anchors:{}};
    const reply:Suggestions={source:{roles:{...corrected,thigh_L:'Rope_L_0'},missing:[],errors:[],status:'',evidence:''},target:{roles:hero,missing:[],errors:[],status:'',evidence:''},mapping:{LegA_L:'pelvis',Hair_1:'chest',LegD_L:'foot_L'},assistance:{policy:'test',entries:[],labels:{},warnings:[],anchors:{LegA_L:'Hips',LegD_L:'LegC_L'}}};
    const {options:next,suggestions}=refreshWithRoles(options,reply);
    expect(next.source_roles).toEqual(corrected);
    expect(next.mapping).toMatchObject({LegA_L:'thigh_L',Hair_1:'chest',LegD_L:'foot_L',LegB_L:'shin_L'});
    expect(next.mapping_anchors).toEqual({LegD_L:'LegC_L'});
    expect(suggestions.source!.roles).toEqual(corrected);
  });
  it('uses the detected groups: undetected saved roots go, the others stay',()=>{
    const groups=physicsGroups(rig,weighted,{source_roles:corrected,target_roles:hero});
    expect(keepDetectedGroups(groups,{physics_roots:['Hair_0','LegA_L']})).toEqual({physics_roots:['Hair_0'],physics_offered:['Hair_0','Rope_L_0'],reviewed:false});
    expect(keepDetectedGroups(groups,{physics_roots:['LegA_L']}).physics_roots).toEqual(['Hair_0','Rope_L_0']);
  });
});

// The page, with the same rig and the fixture's hero.
const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn(),preparationWeaponPreview:vi.fn(),validateAssetPath:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
const identity=(roles:Partial<Record<Role,string>>)=>({roles,missing:[],errors:[],status:'Supported Humanoid',evidence:''});
const guessMapping=Object.fromEntries(Object.entries(wrong).map(([role,name])=>[name,hero[role as Role]!]));
const analysis={source:'C:\\source.blend',source_original:'C:\\source.blend',source_sha256:'a'.repeat(64),target_model:'models/heroes/test.vmdl',target_sha256:'b'.repeat(64),
  analysis:{...sourceFixture.inspection,bones:rig,weighted_bones:weighted,morphs:[],weapons:[]},target:sourceFixture.target,target_skeleton:sourceFixture.skeleton,target_scale_audit:sourceFixture.scale_audit,
  suggestions:{source:identity(wrong),target:identity(hero),mapping:guessMapping,assistance:{policy:'test',entries:[],labels:{},warnings:[],anchors:{}}},
  rotation:[0,0,0],geometry_customization:{props:[],face:null},clothing:null} as unknown as PreparationAnalysis;
const initial={...projectFixture.cases[0].expected,original_model:analysis.target_model,preparation_source:analysis.source_original,preparation_options:{},model_input_mode:'rigged_humanoid'} as unknown as Project;
const output={dmx:'C:\\prepared.dmx',blend:'C:\\prepared.blend',report:'C:\\report.json',textures:'',target_model:analysis.target_model,warnings:[]};
function Harness({start=initial,next,expert}:{start?:Project;next?:()=>void;expert?:boolean}){const[project,setProject]=useState(start);return <><PreparationPage project={project} update={setProject} busy={false} operate={async(_,task)=>task()} next={next} expert={expert}/><output data-testid="project">{JSON.stringify(project)}</output></>;}
const current=()=>JSON.parse(screen.getByTestId('project').textContent!) as Project;
const saved=()=>current().preparation_options as PreparationOptions;
/** What the backend's suggest gives back: the roles it was sent, the mapping it was sent plus role targets. */
const echo=async(_analysis:PreparationAnalysis,options:PreparationOptions):Promise<Suggestions>=>({source:identity(options.source_roles??{}),target:identity(options.target_roles??{}),mapping:{...options.mapping},assistance:{policy:'test',entries:[],labels:{},warnings:[],anchors:{...options.mapping_anchors}}});
async function check(){if(!screen.queryByRole('button',{name:'Load and check only'}))fireEvent.click(screen.getByRole('button',{name:/Model file & orientation/}));fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByRole('tablist',{name:'Preparation review sections'});}
function correctOnPage(){for(const [role,name] of [['thigh_L','LegA_L'],['shin_L','LegB_L'],['foot_L','LegC_L']])fireEvent.change(screen.getByLabelText(`Source ${role}`),{target:{value:name}});}
beforeEach(()=>{vi.clearAllMocks();native.preparationInspect.mockResolvedValue(analysis);native.preparationSuggest.mockImplementation(echo);native.preparationReview.mockResolvedValue([]);
  native.preparationConvert.mockResolvedValue(output);native.validateAssetPath.mockImplementation(async(path:string)=>path);native.preparationWeaponPreview.mockReturnValue(new Promise(()=>undefined));native.preparationAccept.mockImplementation(async(project:Project,result:typeof output)=>({...project,dmx:result.dmx,preparation_report:result.report,preparation_blend:result.blend}));});

describe('Prepare model page after a role correction',()=>{
  it('drops the leg from the moving parts as the roles are corrected, and Check for problems refreshes the matches first',async()=>{
    render(<Harness/>);await check();
    expect(saved().physics_roots).toEqual(['Hair_0','LegA_L']);
    correctOnPage();
    expect(saved().source_roles).toMatchObject(corrected);
    expect(saved().physics_roots).toEqual(['Hair_0','Rope_L_0']);
    expect(native.preparationSuggest).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('tab',{name:/Joint matches/}));
    expect(screen.getByText('Body roles changed. Refresh suggestions to use your current choices.')).toBeVisible();
    // A direct match made by hand goes into the refresh and comes back.
    fireEvent.change(screen.getByLabelText('Map LegD_L'),{target:{value:'shin_L'}});
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    expect(native.preparationSuggest).toHaveBeenCalledOnce();
    expect(native.preparationSuggest.mock.calls[0][1]).toMatchObject({source_roles:corrected,mapping:expect.objectContaining({LegD_L:'shin_L'})});
    const reviewed=native.preparationReview.mock.calls.at(-1)![2] as PreparationOptions;
    expect(reviewed.source_roles).toMatchObject(corrected);
    expect(reviewed.mapping).toMatchObject({LegD_L:'shin_L',LegA_L:'thigh_L',LegB_L:'shin_L',LegC_L:'foot_L'});
    expect(reviewed.physics_roots).toEqual(['Hair_0','Rope_L_0']);
    // The roles now match the suggestions: no prompt, and Prepare model does not refresh again.
    expect(screen.queryByText('Body roles changed. Refresh suggestions to use your current choices.')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));await screen.findByText('Prepared output');
    expect(native.preparationSuggest).toHaveBeenCalledOnce();
    expect(native.preparationConvert.mock.calls.at(-1)![2]).toMatchObject({source_roles:corrected,physics_roots:['Hair_0','Rope_L_0']});
  });
  it('Prepare model refreshes the matches by itself after a role change',async()=>{
    render(<Harness/>);await check();correctOnPage();
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));await screen.findByText('Prepared output');
    expect(native.preparationSuggest).toHaveBeenCalledOnce();
    expect(native.preparationReview.mock.calls.at(-1)![2]).toMatchObject({source_roles:corrected,reviewed:true});
  });
  it('a saved project with the stale leg root is repaired when it is checked',async()=>{
    // Roles corrected in an earlier session, the leg still saved as a moving part.
    const options={...guessed(),source_roles:corrected,mapping:{...guessMapping,LegA_L:'thigh_L',LegB_L:'shin_L',LegC_L:'foot_L'}};
    native.preparationResume.mockResolvedValue({analysis:{...analysis,suggestions:{...analysis.suggestions,source:identity(corrected)}},result:{...output,dmx:'C:\\old.dmx'}});
    render(<Harness start={{...initial,preparation_report:'C:\\report.json',preparation_options:options}}/>);
    fireEvent.click(screen.getByRole('button',{name:'Reopen the last preparation'}));await screen.findByText('Prepared output');
    fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    expect(native.preparationSuggest).not.toHaveBeenCalled();
    expect((native.preparationReview.mock.calls.at(-1)![2] as PreparationOptions).physics_roots).not.toContain('LegA_L');
  });
  it('after Reopen the last preparation, a refresh that changes nothing keeps the result',async()=>{
    // The reopened inspection still holds the first guess; the saved choices are corrected and consistent.
    const options={...correct(guessed()),mapping_anchors:{},influence_limit:4,complete_mapping:true};
    native.preparationResume.mockResolvedValue({analysis,result:{...output,dmx:'C:\\old.dmx'}});
    render(<Harness start={{...initial,preparation_report:'C:\\report.json',preparation_options:options}}/>);
    fireEvent.click(screen.getByRole('button',{name:'Reopen the last preparation'}));await screen.findByText('Prepared output');
    fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    expect(native.preparationSuggest).toHaveBeenCalledOnce();
    expect(screen.getByText('Prepared output')).toBeInTheDocument();
    expect(saved()).toEqual(options);
    fireEvent.click(screen.getByRole('tab',{name:/Joint matches/}));
    expect(screen.queryByText('Body roles changed. Refresh suggestions to use your current choices.')).not.toBeInTheDocument();
  });
  it('does not apply a refresh made for roles with errors',async()=>{
    native.preparationSuggest.mockImplementation(async(a:PreparationAnalysis,o:PreparationOptions)=>{const reply=await echo(a,o);return {...reply,source:{...reply.source!,errors:['Implausible segment: shin_L → foot_L']}};});
    render(<Harness/>);await check();correctOnPage();
    const before=saved();
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    expect(native.preparationSuggest).toHaveBeenCalledOnce();
    expect(saved()).toEqual(before);
    expect(native.preparationReview.mock.calls.at(-1)![2]).toEqual(before);
  });
  it('lists stale matches as one item with a button that refreshes them and checks again',async()=>{
    const bones=Array.from({length:20},(_,i)=>`bone_${i}`);
    native.preparationReview.mockResolvedValueOnce([{tab:'mapping',kind:'stale_following',bones,message:'20 body-following matches no longer fit your body roles (bone_0, bone_1, bone_2, bone_3, bone_4 and 15 more). On the Joint matches tab, press Refresh suggestions using my roles, or choose targets for these bones.'}]).mockResolvedValue([]);
    render(<Harness/>);await check();
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));await screen.findByText('1 thing to fix before preparing');
    expect(native.preparationConvert).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('tab',{name:/Joint matches/}));
    expect(screen.getByText(/20 body-following matches no longer fit your body roles\. Refresh suggestions/)).toBeVisible();
    const results=screen.getByRole('region',{name:'Review results'});
    fireEvent.click(within(results).getByRole('button',{name:'Refresh the matches'}));
    await screen.findByText('No problems found.');
    expect(native.preparationSuggest).toHaveBeenCalledOnce();
    expect(native.preparationReview).toHaveBeenCalledTimes(2);
  });
  it('offers Use the detected groups for saved groups that are not detected, in the results and on Moving parts',async()=>{
    const message='Some saved moving parts are no longer found in your model. On the Moving parts tab, press Use the detected groups (or Load and check only if you changed the model file).';
    native.preparationReview.mockResolvedValueOnce([{tab:'physics bones',kind:'stale_physics_groups',message}]).mockResolvedValue([]);
    const options={...guessed(),physics_roots:['Hair_0','LegA_L','gone_root']};
    native.preparationResume.mockResolvedValue({analysis,result:{...output,dmx:'C:\\old.dmx'}});
    render(<Harness start={{...initial,preparation_report:'C:\\report.json',preparation_options:options}}/>);
    fireEvent.click(screen.getByRole('button',{name:'Reopen the last preparation'}));await screen.findByText('Prepared output');
    fireEvent.click(screen.getByRole('tab',{name:/Moving parts/}));
    expect(screen.getByText(/A saved group is no longer detected: gone_root\. Press Use the detected groups/)).toBeVisible();
    fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('1 thing to fix before preparing');
    expect(screen.getByText(message)).toBeVisible();
    fireEvent.click(within(screen.getByRole('region',{name:'Review results'})).getByRole('button',{name:'Use the detected groups'}));
    await screen.findByText('No problems found.');
    expect(saved().physics_roots).toEqual(['Hair_0','LegA_L']);
    expect((native.preparationReview.mock.calls.at(-1)![2] as PreparationOptions).physics_roots).toEqual(['Hair_0','LegA_L']);
    expect(screen.queryByText(/no longer detected/)).not.toBeInTheDocument();
  });
});

describe('Automatic preparation in Simple mode',()=>{
  it('still prepares without any clicks and without an extra refresh',async()=>{
    const next=vi.fn();
    render(<Harness start={{...initial,preparation_source:''}} next={next} expert={false}/>);
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path:'C:\\models\\mine.blend',auto:true}}));});
    await vi.waitFor(()=>expect(next).toHaveBeenCalledOnce());
    expect(native.preparationSuggest).not.toHaveBeenCalled();
    expect(native.preparationConvert.mock.calls[0][2]).toMatchObject({reviewed:true,source_roles:wrong,physics_roots:['Hair_0','LegA_L']});
    expect(current().dmx).toBe(output.dmx);
  });
});

describe('A copy made for another hero',()=>{
  it('makes the matches again from the carried body roles and keeps every carried choice at its first preparation',async()=>{
    const next=vi.fn();
    const carried={source_roles:corrected,physics_roots:['Hair_0'],physics_kinds:{Hair_0:'hair'},physics_offered:['Hair_0','Rope_L_0'],preserve_physics_bones:true,automatic_secondary_motion:true,
      fit_mode:'preserve_proportions',fit_floor:'legs',shape_key_mode:'basis_static'} as PreparationOptions;
    render(<Harness start={{...initial,copied_report:'C:\\first\\preparation-report.json',preparation_options:carried as unknown as Project['preparation_options']}} next={next} expert={false}/>);
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path:initial.preparation_source,auto:true}}));});
    await vi.waitFor(()=>expect(next).toHaveBeenCalledOnce());
    // The matches are made from the user's roles, not from the fresh guess (which took the rope for the leg).
    expect(native.preparationSuggest.mock.calls[0][1]).toMatchObject({source_roles:corrected,fit_floor:'legs'});
    expect(native.preparationConvert.mock.calls[0][2]).toMatchObject({reviewed:true,source_roles:corrected,physics_roots:['Hair_0'],physics_kinds:{Hair_0:'hair'},
      fit_mode:'preserve_proportions',fit_floor:'legs',shape_key_mode:'basis_static'});
  });
  it('leaves an ordinary project without matches to the fresh guess, as before',async()=>{
    const next=vi.fn();
    render(<Harness start={{...initial,preparation_options:{source_roles:corrected} as unknown as Project['preparation_options']}} next={next} expert={false}/>);
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path:initial.preparation_source,auto:true}}));});
    await vi.waitFor(()=>expect(next).toHaveBeenCalledOnce());
    expect(native.preparationConvert.mock.calls[0][2]).toMatchObject({source_roles:wrong});
  });
});
