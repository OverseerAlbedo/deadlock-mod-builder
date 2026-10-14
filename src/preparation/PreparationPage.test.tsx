import { useState } from 'react';
import { act, fireEvent,render,screen } from '@testing-library/react';
import { beforeEach,describe,expect,it,vi } from 'vitest';
import { PreparationPage } from './PreparationPage';
import type { PreparationAnalysis } from './models';
import type { Project } from '../models';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn(),preparationWeaponPreview:vi.fn(),validateAssetPath:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
const analysis={source:'C:\\source.blend',source_original:'C:\\source.blend',source_sha256:'a'.repeat(64),target_model:'models/heroes/test.vmdl',target_sha256:'b'.repeat(64),analysis:sourceFixture.inspection,target:sourceFixture.target,target_skeleton:sourceFixture.skeleton,target_scale_audit:sourceFixture.scale_audit,suggestions:sourceFixture.cases[1].expected,rotation:[0,0,0],geometry_customization:{props:[],face:null},clothing:null} as unknown as PreparationAnalysis;
const initial={...projectFixture.cases[0].expected,original_model:analysis.target_model,preparation_source:analysis.source_original,preparation_options:{},model_input_mode:'rigged_humanoid'} as unknown as Project;
const output={dmx:'C:\\prepared.dmx',blend:'C:\\prepared.blend',report:'C:\\report.json',textures:'',target_model:analysis.target_model,warnings:[]};
function Harness({start=initial,next,expert}:{start?:Project;next?:()=>void;expert?:boolean}){const[project,setProject]=useState(start);return <><PreparationPage project={project} update={setProject} busy={false} operate={async(_,task)=>task()} next={next} expert={expert}/><output data-testid="project">{JSON.stringify(project)}</output></>;}
const current=()=>JSON.parse(screen.getByTestId('project').textContent!) as Project;
const openPanel=(name:string)=>fireEvent.click(screen.getByRole('tab',{name:new RegExp(name)}));
/** Inspect without preparing (the review tabs are then shown). */
async function check(heading='What we found in your model'){fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByText(heading);}
beforeEach(()=>{vi.clearAllMocks();native.preparationInspect.mockResolvedValue(analysis);native.preparationSuggest.mockResolvedValue(analysis.suggestions);native.preparationReview.mockResolvedValue([]);
  native.preparationConvert.mockResolvedValue(output);native.validateAssetPath.mockImplementation(async(path:string)=>path);native.preparationWeaponPreview.mockReturnValue(new Promise(()=>undefined));native.preparationAccept.mockImplementation(async(project:Project,result:typeof output)=>({...project,dmx:result.dmx,preparation_report:result.report,preparation_blend:result.blend}));});
describe('Native preparation review boundary',()=>{
  it('keeps bulk tools visible and explains the missing body-role dependency',async()=>{
    native.preparationInspect.mockResolvedValue({...analysis,suggestions:{...analysis.suggestions,
      source:{...analysis.suggestions.source,errors:['Assign humanoid roles: neck'],missing:['neck']},
      assistance:{policy:'test',labels:{},warnings:[],anchors:{},entries:[]}}});
    render(<Harness/>);await check();openPanel('Bone mapping');
    expect(screen.getByRole('button',{name:'Follow pelvis, keep offset for all (0)'})).toBeDisabled();
    expect(screen.getByRole('button',{name:'Follow suggested joints, keep offsets for all (0)'})).toBeDisabled();
    expect(screen.getByText('Source: Assign humanoid roles: neck')).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Review body roles'}));
    expect(screen.getByRole('tab',{name:/Body roles/})).toHaveAttribute('aria-selected','true');
  });
  it('refreshes follow suggestions with manual roles intact after a role edit',async()=>{
    render(<Harness/>);await check();
    const alternative=analysis.analysis.bones.find(b=>b.name!==analysis.suggestions.source!.roles.head)!;
    fireEvent.change(screen.getByLabelText('Source head'),{target:{value:alternative.name}});openPanel('Bone mapping');
    expect(screen.getByText('Body roles changed. Refresh suggestions to use your current choices.')).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Refresh suggestions using my roles'}));
    await vi.waitFor(()=>expect(native.preparationSuggest).toHaveBeenCalledWith(expect.anything(),expect.objectContaining({source_roles:expect.objectContaining({head:alternative.name}),target_roles:analysis.suggestions.target.roles})));
  });
  it('re-detects body roles instead of pinning stale saved roles',async()=>{
    render(<Harness/>);await check();
    fireEvent.change(screen.getByLabelText('Target shin_R'),{target:{value:analysis.target[0].name}});
    fireEvent.click(screen.getByRole('button',{name:'Re-detect body mapping'}));
    await vi.waitFor(()=>expect(native.preparationSuggest).toHaveBeenCalledWith(expect.anything(),expect.objectContaining({source_roles:{},target_roles:{}})));
  });
  it('retains the saved choices when checking the original source again after reopening its preparation',async()=>{
    const options={mapping:analysis.suggestions.mapping,source_roles:analysis.suggestions.source!.roles,target_roles:analysis.suggestions.target.roles,influence_limit:3,shape_key_mode:'current_static',fit_mode:'preserve_proportions',preserve_physics_bones:true,physics_roots:['saved branch']};
    native.preparationResume.mockResolvedValue({analysis,result:{...output,dmx:'C:\\old.dmx'}});
    render(<Harness start={{...initial,preparation_source:'C:\\workspace\\source.blend',preparation_report:'C:\\report.json',preparation_options:options}}/>);
    fireEvent.click(screen.getByRole('button',{name:'Reopen the last preparation'}));await screen.findByText('Prepared output');
    await check();
    expect(native.preparationSuggest).toHaveBeenCalledWith(expect.anything(),expect.objectContaining(options));
    expect(screen.getByLabelText('Model fitting')).toHaveValue('preserve_proportions');
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    expect(native.preparationReview.mock.calls.at(-1)![2]).toMatchObject({influence_limit:3,shape_key_mode:'current_static',preserve_physics_bones:true,physics_roots:['saved branch']});
  });
  it('prepares in one click with the chosen fit, then continues to the next step',async()=>{
    const next=vi.fn();
    render(<Harness next={next}/>);await check();openPanel('Fit & review');
    expect(screen.getByLabelText('Model fitting')).toHaveValue('match_target');
    fireEvent.change(screen.getByLabelText('Model fitting'),{target:{value:'preserve_proportions'}});
    fireEvent.click(screen.getByText('About model fitting',{selector:'summary'}));
    expect(screen.getByText(/Keeps the source torso, neck, head and leg proportions/)).toBeVisible();
    expect(screen.getByLabelText('Align arms to hero joints (recommended)')).toBeChecked();
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));
    await vi.waitFor(()=>expect(next).toHaveBeenCalledOnce());
    // Prepare checks the current choices again right before converting, and the click is the review.
    expect(native.preparationReview).toHaveBeenCalledOnce();
    expect(native.preparationConvert.mock.calls.at(-1)![2]).toMatchObject({fit_mode:'preserve_proportions',mapping:analysis.suggestions.mapping,reviewed:true});
    expect(native.preparationAccept).toHaveBeenCalledWith(expect.anything(),output);
    expect(current().dmx).toBe(output.dmx);
    expect(await screen.findByText('Your prepared model is in use')).toBeVisible();
  });
  it('uses smooth twist weights for fresh inspection and sends a changed arm skinning',async()=>{
    render(<Harness/>);await check();openPanel('Fit & review');
    expect(screen.getByLabelText('Arm skinning')).toHaveValue('smooth_twist');
    fireEvent.change(screen.getByLabelText('Arm skinning'),{target:{value:'source'}});
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));await screen.findByText('Prepared output');
    expect(native.preparationConvert.mock.calls.at(-1)![2].arm_skinning).toBe('source');
  });
  it('follows all pelvis suggestions across pages and filters, then follows the other suggested joints',async()=>{
    const hips=analysis.suggestions.source!.roles.pelvis!,pelvis=analysis.suggestions.target.roles.pelvis!;
    const skull=analysis.suggestions.source!.roles.head!,head=analysis.suggestions.target.roles.head!;
    const names=Array.from({length:65},(_,i)=>`pelvis_extra_${i}`);
    const entries=[...names.map(source=>({source,target:null,group:'Pelvis extras',label:source,reason:'Review required.',automatic:false,follow:{anchor:hips,target:pelvis,group:'Pelvis extras',automatic:false,reason:'No independent motion.'}})),{source:'hair_extra',target:null,group:'Hair',label:'Hair',reason:'Review required.',automatic:false,follow:{anchor:skull,target:head,group:'Hair',automatic:false,reason:'No independent motion.'}}];
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,weighted_bones:[...names,'hair_extra']},suggestions:{...analysis.suggestions,assistance:{policy:'test',labels:{},warnings:[],anchors:{},entries}}});
    render(<Harness/>);await check('Humanoid roles');openPanel('Bone mapping');
    fireEvent.change(screen.getByPlaceholderText('Name or English label'),{target:{value:'pelvis_extra_0'}});
    fireEvent.click(screen.getByRole('button',{name:'Follow pelvis, keep offset for all (65)'}));
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    const choices=native.preparationReview.mock.calls.at(-1)![2];
    for(const name of names){expect(choices.mapping[name]).toBe(pelvis);expect(choices.mapping_anchors[name]).toBe(hips);}
    expect(choices.mapping.hair_extra).toBeUndefined();
    fireEvent.click(screen.getByRole('button',{name:'Follow suggested joints, keep offsets for all (1)'}));
    openPanel('Fit & review');
    expect(screen.getByText(/66 small bones follow a body joint and lose their own movement/)).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));await screen.findByText('Prepared output');
    // Preparing accepts the following joints explicitly.
    expect(native.preparationConvert.mock.calls.at(-1)![2]).toMatchObject({follow_motion_reviewed:true,reviewed:true});
    expect(native.preparationConvert.mock.calls.at(-1)![2].mapping_anchors.hair_extra).toBe(skull);
  });
  it('stops at a missing shape key choice and prepares once it is made',async()=>{
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,morphs:[{name:'body/Smile',mesh:'body',shape:'Smile'}]},geometry_customization:{props:[],face:null}});
    native.preparationReview.mockImplementation(async(_project,_analysis,options)=>options.shape_key_mode?[]:[{tab:'shape keys',message:'Choose Shape key handling before conversion.'}]);
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByLabelText('Shape key handling');openPanel('Fit & review');
    expect(screen.getByLabelText('Shape key handling')).toHaveValue('');
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));await screen.findByText(/Choose Shape key handling before conversion/);
    expect(native.preparationConvert).not.toHaveBeenCalled();
    expect(screen.getByText('1 thing to fix before preparing')).toBeVisible();
    fireEvent.change(screen.getByLabelText('Shape key handling'),{target:{value:'current_static'}});
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));await screen.findByText('Prepared output');
    expect(native.preparationConvert.mock.calls.at(-1)![2].shape_key_mode).toBe('current_static');
  });
  it('resolves a secondary group to follow a body joint and saves choices after a passed check',async()=>{
    const anchor=analysis.suggestions.source!.roles.head!,target=analysis.suggestions.target.roles.head!,names=['extra_hair_a','extra_hair_b'];
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,weighted_bones:names},suggestions:{...analysis.suggestions,assistance:{policy:'test',labels:{},warnings:[],anchors:{},entries:names.map(source=>({source,target:null,group:'Hair',label:source,reason:'Review required.',automatic:false,follow:{anchor,target,group:`Hair → ${target}`,automatic:false,reason:'No new simulation.'}}))}}});
    render(<Harness/>);await check('Humanoid roles');openPanel('Bone mapping');
    fireEvent.click(screen.getByRole('button',{name:`Make 2 bones follow ${target}`}));
    openPanel('Fit & review');
    expect(screen.getByText(/2 small bones follow a body joint/)).toBeVisible();
    expect(screen.getByRole('button',{name:'Save mapping preset'})).toBeDisabled();
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    const choices=native.preparationReview.mock.calls.at(-1)![2];expect(choices.mapping_anchors).toEqual({extra_hair_a:anchor,extra_hair_b:anchor});
    fireEvent.click(screen.getByRole('button',{name:'Save mapping preset'}));await screen.findByText('Mapping preset saved for this rig pair.');expect(native.preparationSaveMappingPreset).toHaveBeenCalledWith(expect.anything(),choices);
  });
  it('fills supported roles on inspection and updates role mappings together',async()=>{
    render(<Harness/>);await check('Humanoid roles');
    expect(screen.getByLabelText('Source pelvis')).toHaveValue(analysis.suggestions.source!.roles.pelvis);
    const alternate=analysis.analysis.bones.find(b=>b.name!==analysis.suggestions.source!.roles.head)!;
    fireEvent.change(screen.getByLabelText('Source head'),{target:{value:alternate.name}});
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    const choices=native.preparationReview.mock.calls.at(-1)![2];expect(choices.mapping[alternate.name]).toBe(choices.target_roles.head);
    expect(native.preparationConvert).not.toHaveBeenCalled();
  });
  it('groups unresolved weights and assigns an explicit shared target without converting',async()=>{
    const bone=analysis.analysis.bones[0];
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,weighted_bones:[bone.name]},suggestions:{...analysis.suggestions,mapping:{},assistance:{policy:'test',labels:{},warnings:[],entries:[{source:bone.name,target:null,group:'Extra deformation bones',label:bone.name,reason:'Review required.',automatic:false}]}}});
    render(<Harness/>);await check('Humanoid roles');openPanel('Bone mapping');
    expect(screen.getByText('1 joint still need a match below. Everything else was matched automatically.')).toBeVisible();
    fireEvent.click(screen.getByLabelText(`Select bone ${bone.name}`));
    fireEvent.click(screen.getByText('Assign multiple bones',{selector:'summary'}));
    fireEvent.change(screen.getByLabelText('Target for selected bones'),{target:{value:analysis.target[0].name}});
    fireEvent.click(screen.getByRole('button',{name:`Assign ${analysis.target[0].name} to 1 selected bones`}));
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    expect(native.preparationReview.mock.calls.at(-1)![2].mapping[bone.name]).toBe(analysis.target[0].name);
    expect(native.preparationConvert).not.toHaveBeenCalled();
  });
  it('shows the helper exclusion note without converting',async()=>{
    const warning='Excluded 284 MMD collision helper meshes from character geometry. The source file is unchanged. MMD physics is not converted to Deadlock cloth.';
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,warnings:[warning]}});
    render(<Harness/>);await check();fireEvent.click(screen.getByText('Inspection details & notes',{selector:'summary'}));
    expect(await screen.findByText(warning)).toBeVisible();
    expect(native.preparationConvert).not.toHaveBeenCalled();
  });
  it('re-exports corrections from the prepared Blender file',async()=>{
    render(<Harness/>);await check();
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));await screen.findByText('Prepared output');
    expect(native.preparationReview).toHaveBeenCalledOnce();expect(native.preparationConvert).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole('button',{name:'Re-export corrections'}));
    await vi.waitFor(()=>expect(native.preparationConvert).toHaveBeenCalledTimes(2));
    expect(native.preparationConvert.mock.calls.at(-1)![0]).toMatchObject({preparation_report:'C:\\report.json',preparation_blend:'C:\\prepared.blend'});
    expect(native.preparationConvert.mock.calls.at(-1)![3]).toBe(true);
    expect(native.preparationConvert.mock.calls.at(-1)![2]).toMatchObject({reviewed:true});
  });
  it('surfaces blocking native review issues without calling conversion',async()=>{
    native.preparationReview.mockResolvedValue([{tab:'Mapping',message:'Map weighted source bone mixamorig:Hips.'}]);
    render(<Harness/>);await check();fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));
    await screen.findByText(/Map weighted source bone/);fireEvent.click(screen.getByRole('button',{name:'Open section'}));expect(screen.getByRole('tab',{name:/Bone mapping/})).toHaveAttribute('aria-selected','true');expect(native.preparationConvert).not.toHaveBeenCalled();
    expect(screen.getByText('Something needs your choice')).toBeVisible();
  });
});

describe('Automatic preparation',()=>{
  it('checks, prepares and uses a model chosen in step 1 without any clicks',async()=>{
    const next=vi.fn();
    render(<Harness start={{...initial,preparation_source:''}} next={next}/>);
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path:'C:\\models\\mine.blend',auto:true}}));});
    await vi.waitFor(()=>expect(next).toHaveBeenCalledOnce());
    expect(native.preparationInspect).toHaveBeenCalledWith(expect.anything(),'C:\\models\\mine.blend',[0,0,0]);
    expect(native.preparationConvert.mock.calls[0][2]).toMatchObject({reviewed:true,complete_mapping:true});
    expect(current().dmx).toBe(output.dmx);
  });
  it('stops automatic preparation where a choice is needed',async()=>{
    const next=vi.fn();
    native.preparationReview.mockResolvedValue([{tab:'Mapping',message:'Map weighted source bone mixamorig:Hips.'}]);
    render(<Harness next={next}/>);
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path:analysis.source_original,auto:true}}));});
    await screen.findByText(/Map weighted source bone/);
    expect(native.preparationConvert).not.toHaveBeenCalled();expect(next).not.toHaveBeenCalled();
  });
  it('checks the model again by itself when the gun file changes',async()=>{
    render(<Harness/>);await check();
    expect(native.preparationInspect).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole('tab',{name:/Weapon/}));
    fireEvent.change(screen.getByLabelText('Gun from another file (optional)'),{target:{value:'C:\\guns\\pair.blend'}});
    fireEvent.blur(screen.getByLabelText('Gun from another file (optional)'));
    await vi.waitFor(()=>expect(native.preparationInspect).toHaveBeenCalledTimes(2));
    expect(native.preparationInspect.mock.calls[1][0].preparation_options.weapon_source).toContain('pair.blend');
  });
  it('says when the model is already prepared and offers to continue',async()=>{
    const next=vi.fn();
    render(<Harness start={{...initial,dmx:'C:\\prepared.dmx',preparation_report:'C:\\report.json'}} next={next}/>);
    expect(screen.getByText('Your model is prepared.')).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:/Continue to Appearance & weapons/}));
    expect(next).toHaveBeenCalledOnce();
    render(<Harness start={{...initial,dmx:'C:\\ready.dmx',preparation_report:''}}/>);
    expect(screen.getByText(/already game-ready \(DMX\)/)).toBeVisible();
  });
});

describe('Preparation workspace navigation',()=>{
  it('keeps choices across keyboard navigation and body-region filters without running native work',async()=>{
    render(<Harness/>);await check();
    const original=(screen.getByLabelText('Source pelvis') as HTMLSelectElement).value;
    fireEvent.click(screen.getByRole('button',{name:'Legs & feet'}));
    expect(screen.queryByLabelText('Source pelvis')).not.toBeInTheDocument();
    expect(screen.getByLabelText('Source shin_L')).toBeVisible();
    const roles=screen.getByRole('tab',{name:/Body roles/});roles.focus();fireEvent.keyDown(roles,{key:'End'});
    expect(screen.getByRole('tab',{name:/Fit & review/})).toHaveFocus();
    expect(screen.getByLabelText('Arm skinning')).toBeVisible();
    fireEvent.keyDown(screen.getByRole('tab',{name:/Fit & review/}),{key:'Home'});
    fireEvent.click(screen.getByRole('button',{name:'All joints'}));
    expect((screen.getByLabelText('Source pelvis') as HTMLSelectElement).value).toBe(original);
    expect(native.preparationReview).not.toHaveBeenCalled();expect(native.preparationConvert).not.toHaveBeenCalled();
  });
});

describe('Simple view',()=>{
  it('collapses joint matching when everything matched and opens it on request',async()=>{
    render(<Harness expert={false}/>);await check();
    expect(screen.queryByRole('tab',{name:/Body roles/})).not.toBeInTheDocument();
    expect(screen.queryByRole('tab',{name:/Bone mapping/})).not.toBeInTheDocument();
    expect(screen.getByRole('tab',{name:/Fit & review/})).toHaveAttribute('aria-selected','true');
    expect(screen.getByText(/bones matched to/)).toBeVisible();
    expect(screen.queryByLabelText('Preparation mode')).not.toBeInTheDocument();
    expect(screen.getByText('Customize fit',{exact:false,selector:'summary'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Edit matches'}));
    expect(screen.getByRole('tab',{name:/Body roles/})).toHaveAttribute('aria-selected','true');
  });
  it('shows joint matching by itself when something needs a choice',async()=>{
    native.preparationInspect.mockResolvedValue({...analysis,suggestions:{...analysis.suggestions,source:{...analysis.suggestions.source,errors:['Assign humanoid roles: neck'],missing:['neck']}}});
    render(<Harness expert={false}/>);await check();
    expect(screen.getByRole('tab',{name:/Body roles/})).toHaveAttribute('aria-selected','true');
    expect(screen.queryByText(/bones matched to/)).not.toBeInTheDocument();
  });
  it('switches one 3D view between your model, the joint check and the result',async()=>{
    render(<Harness expert={false}/>);await check();
    const view=screen.getByRole('group',{name:'What the 3D view shows'});
    expect(screen.getByRole('button',{name:'Result'})).toBeDisabled();
    expect(screen.getByRole('button',{name:'Load model for mapping review'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Your model'}));
    expect(screen.getByRole('button',{name:'Load your model'})).toBeInTheDocument();
    expect(view).toBeInTheDocument();
  });
});

