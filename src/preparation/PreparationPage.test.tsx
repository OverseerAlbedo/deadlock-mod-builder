import { useState } from 'react';
import { fireEvent,render,screen } from '@testing-library/react';
import { beforeEach,describe,expect,it,vi } from 'vitest';
import { PreparationPage } from './PreparationPage';
import type { PreparationAnalysis } from './models';
import type { Project } from '../models';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
const analysis={source:'C:\\source.blend',source_original:'C:\\source.blend',source_sha256:'a'.repeat(64),target_model:'models/heroes/test.vmdl',target_sha256:'b'.repeat(64),analysis:sourceFixture.inspection,target:sourceFixture.target,target_skeleton:sourceFixture.skeleton,target_scale_audit:sourceFixture.scale_audit,suggestions:sourceFixture.cases[1].expected,rotation:[0,0,0],geometry_customization:{props:[],face:null},clothing:null} as unknown as PreparationAnalysis;
const initial={...projectFixture.cases[0].expected,original_model:analysis.target_model,preparation_source:analysis.source_original,preparation_options:{},model_input_mode:'rigged_humanoid'} as unknown as Project;
function Harness({start=initial}:{start?:Project}){const[project,setProject]=useState(start);return <PreparationPage project={project} update={setProject} busy={false} operate={async(_,task)=>task()}/>;}
const openPanel=(name:string)=>fireEvent.click(screen.getByRole('tab',{name:new RegExp(name)}));
beforeEach(()=>{vi.clearAllMocks();native.preparationInspect.mockResolvedValue(analysis);native.preparationSuggest.mockResolvedValue(analysis.suggestions);native.preparationReview.mockResolvedValue([]);});
describe('Native preparation review boundary',()=>{
  it('keeps bulk tools visible and explains the missing body-role dependency',async()=>{
    native.preparationInspect.mockResolvedValue({...analysis,suggestions:{...analysis.suggestions,
      source:{...analysis.suggestions.source,errors:['Assign humanoid roles: neck'],missing:['neck']},
      assistance:{policy:'test',labels:{},warnings:[],anchors:{},entries:[]}}});
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');openPanel('Bone mapping');
    expect(screen.getByRole('button',{name:'Follow pelvis, keep offset for all (0)'})).toBeDisabled();
    expect(screen.getByRole('button',{name:'Follow suggested joints, keep offsets for all (0)'})).toBeDisabled();
    expect(screen.getByText('Source: Assign humanoid roles: neck')).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Review body roles'}));
    expect(screen.getByRole('tab',{name:/Body roles/})).toHaveAttribute('aria-selected','true');
  });
  it('refreshes follow suggestions with manual roles intact after a role edit',async()=>{
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');
    const alternative=analysis.analysis.bones.find(b=>b.name!==analysis.suggestions.source!.roles.head)!;
    fireEvent.change(screen.getByLabelText('Source head'),{target:{value:alternative.name}});openPanel('Bone mapping');
    expect(screen.getByText('Body roles changed. Refresh suggestions to use your current choices.')).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Refresh suggestions using my roles'}));
    await vi.waitFor(()=>expect(native.preparationSuggest).toHaveBeenCalledWith(expect.anything(),expect.objectContaining({source_roles:expect.objectContaining({head:alternative.name}),target_roles:analysis.suggestions.target.roles})));
  });
  it('re-detects body roles instead of pinning stale saved roles',async()=>{
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');
    fireEvent.change(screen.getByLabelText('Target shin_R'),{target:{value:analysis.target[0].name}});
    fireEvent.click(screen.getByRole('button',{name:'Re-detect body mapping'}));
    await vi.waitFor(()=>expect(native.preparationSuggest).toHaveBeenCalledWith(expect.anything(),expect.objectContaining({source_roles:{},target_roles:{}})));
  });
  it('retains reviewed choices when refreshing the original source after resuming its workspace copy',async()=>{
    const options={mapping:analysis.suggestions.mapping,source_roles:analysis.suggestions.source!.roles,target_roles:analysis.suggestions.target.roles,influence_limit:3,shape_key_mode:'current_static',fit_mode:'preserve_proportions',preserve_physics_bones:true,physics_roots:['saved branch']};
    native.preparationResume.mockResolvedValue({analysis,result:{dmx:'C:\\old.dmx',blend:'C:\\prepared.blend',report:'C:\\report.json',textures:'',target_model:analysis.target_model,warnings:[]}});
    render(<Harness start={{...initial,preparation_source:'C:\\workspace\\source.blend',preparation_report:'C:\\report.json',preparation_options:options}}/>);
    fireEvent.click(screen.getByRole('button',{name:'Resume saved preparation'}));await screen.findByText('Prepared output');
    fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');
    expect(native.preparationSuggest).toHaveBeenCalledWith(expect.anything(),expect.objectContaining(options));
    expect(screen.getByLabelText('Model fitting')).toHaveValue('preserve_proportions');
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    expect(native.preparationReview.mock.calls.at(-1)![2]).toMatchObject({influence_limit:3,shape_key_mode:'current_static',preserve_physics_bones:true,physics_roots:['saved branch'],reviewed:false});
  });
  it('requires fresh review when fitting changes and sends the selected mode to conversion',async()=>{
    native.preparationConvert.mockResolvedValue({dmx:'C:\\prepared.dmx',blend:'C:\\prepared.blend',report:'C:\\report.json',textures:'',target_model:analysis.target_model,warnings:[]});
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');openPanel('Fit & review');
    expect(screen.getByLabelText('Model fitting')).toHaveValue('match_target');
    const confirm='I have reviewed the roles, mappings, orientation, and landmarks';
    fireEvent.click(screen.getByLabelText(confirm));fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    fireEvent.change(screen.getByLabelText('Model fitting'),{target:{value:'preserve_proportions'}});
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();expect(screen.getByLabelText(confirm)).not.toBeChecked();
    fireEvent.click(screen.getByText('About model fitting',{selector:'summary'}));
    expect(screen.getByText(/Keeps the source torso, neck, head and leg proportions/)).toBeVisible();
    const arms=screen.getByLabelText('Align arms to hero joints (recommended)');
    expect(arms).toBeChecked();
    fireEvent.click(arms);
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();
    fireEvent.click(arms);
    fireEvent.click(screen.getByLabelText(confirm));fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    fireEvent.click(screen.getByRole('button',{name:'Convert model'}));await screen.findByText('Prepared output');
    expect(native.preparationConvert.mock.calls.at(-1)![2]).toMatchObject({fit_mode:'preserve_proportions',fit_arm_joints:true,mapping:analysis.suggestions.mapping});
    fireEvent.change(screen.getByLabelText('Model fitting'),{target:{value:'match_target'}});
    expect(screen.queryByRole('button',{name:'Use prepared model'})).not.toBeInTheDocument();
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();
  });
  it('uses smooth twist weights for fresh inspection and requires review after changing arm skinning',async()=>{
    native.preparationConvert.mockResolvedValue({dmx:'C:/prepared.dmx',blend:'C:/prepared.blend',report:'C:/report.json',textures:'',target_model:analysis.target_model,warnings:[]});
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');openPanel('Fit & review');
    expect(screen.getByLabelText('Arm skinning')).toHaveValue('smooth_twist');
    const confirm='I have reviewed the roles, mappings, orientation, and landmarks';
    fireEvent.click(screen.getByLabelText(confirm));fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    fireEvent.change(screen.getByLabelText('Arm skinning'),{target:{value:'source'}});
    expect(screen.getByLabelText(confirm)).not.toBeChecked();expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();
    fireEvent.click(screen.getByLabelText(confirm));fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    fireEvent.click(screen.getByRole('button',{name:'Convert model'}));await screen.findByText('Prepared output');
    expect(native.preparationConvert.mock.calls.at(-1)![2].arm_skinning).toBe('source');
  });
  it('follows all pelvis suggestions across pages and filters, then follows the other suggested joints',async()=>{
    const hips=analysis.suggestions.source!.roles.pelvis!,pelvis=analysis.suggestions.target.roles.pelvis!;
    const skull=analysis.suggestions.source!.roles.head!,head=analysis.suggestions.target.roles.head!;
    const names=Array.from({length:65},(_,i)=>`pelvis_extra_${i}`);
    const entries=[...names.map(source=>({source,target:null,group:'Pelvis extras',label:source,reason:'Review required.',automatic:false,follow:{anchor:hips,target:pelvis,group:'Pelvis extras',automatic:false,reason:'No independent motion.'}})),{source:'hair_extra',target:null,group:'Hair',label:'Hair',reason:'Review required.',automatic:false,follow:{anchor:skull,target:head,group:'Hair',automatic:false,reason:'No independent motion.'}}];
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,weighted_bones:[...names,'hair_extra']},suggestions:{...analysis.suggestions,assistance:{policy:'test',labels:{},warnings:[],anchors:{},entries}}});
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Humanoid roles');openPanel('Bone mapping');
    fireEvent.change(screen.getByPlaceholderText('Name or English label'),{target:{value:'pelvis_extra_0'}});
    fireEvent.click(screen.getByRole('button',{name:'Follow pelvis, keep offset for all (65)'}));
    expect(screen.getByLabelText('I accept 65 bones following body joints without their original independent movement')).not.toBeChecked();
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    const choices=native.preparationReview.mock.calls.at(-1)![2];
    for(const name of names){expect(choices.mapping[name]).toBe(pelvis);expect(choices.mapping_anchors[name]).toBe(hips);}
    expect(choices.mapping.hair_extra).toBeUndefined();
    fireEvent.click(screen.getByRole('button',{name:'Follow suggested joints, keep offsets for all (1)'}));
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    expect(native.preparationReview.mock.calls.at(-1)![2].mapping_anchors.hair_extra).toBe(skull);
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();
  });
  it('requires an explicit static shape choice and renews review after changing that choice',async()=>{
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,morphs:[{name:'body/Smile',mesh:'body',shape:'Smile'}]},geometry_customization:{props:[],face:null}});
    native.preparationReview.mockImplementation(async(_project,_analysis,options)=>options.shape_key_mode?[]:[{tab:'shape keys',message:'Choose Shape key handling before conversion.'}]);
    native.preparationConvert.mockResolvedValue({dmx:'C:\\prepared.dmx',blend:'C:\\prepared.blend',report:'C:\\report.json',textures:'',target_model:analysis.target_model,warnings:[]});
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByLabelText('Shape key handling');openPanel('Fit & review');
    expect(screen.getByLabelText('Shape key handling')).toHaveValue('');
    fireEvent.click(screen.getByLabelText('I have reviewed the roles, mappings, orientation, and landmarks'));
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText(/Choose Shape key handling before conversion/);
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();
    fireEvent.change(screen.getByLabelText('Shape key handling'),{target:{value:'current_static'}});
    expect(screen.getByLabelText('I have reviewed the roles, mappings, orientation, and landmarks')).not.toBeChecked();
    fireEvent.click(screen.getByLabelText('I have reviewed the roles, mappings, orientation, and landmarks'));
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    fireEvent.click(screen.getByRole('button',{name:'Convert model'}));await screen.findByText('Prepared output');
    expect(native.preparationConvert.mock.calls.at(-1)![2].shape_key_mode).toBe('current_static');
    fireEvent.change(screen.getByLabelText('Shape key handling'),{target:{value:'basis_static'}});
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();
    expect(screen.queryByRole('button',{name:'Use prepared model'})).not.toBeInTheDocument();
  });
  it('resolves a reviewed secondary group with explicit motion acknowledgement and saves choices after review',async()=>{
    const anchor=analysis.suggestions.source!.roles.head!,target=analysis.suggestions.target.roles.head!,names=['extra_hair_a','extra_hair_b'];
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,weighted_bones:names},suggestions:{...analysis.suggestions,assistance:{policy:'test',labels:{},warnings:[],anchors:{},entries:names.map(source=>({source,target:null,group:'Hair',label:source,reason:'Review required.',automatic:false,follow:{anchor,target,group:`Hair → ${target}`,automatic:false,reason:'No new simulation.'}}))}}});
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Humanoid roles');openPanel('Bone mapping');
    fireEvent.click(screen.getByRole('button',{name:`Make 2 bones follow ${target}`}));
    openPanel('Fit & review');
    const acknowledgement=screen.getByLabelText('I accept 2 bones following body joints without their original independent movement');expect(acknowledgement).not.toBeChecked();
    expect(screen.getByRole('button',{name:'Save mapping preset'})).toBeDisabled();
    fireEvent.click(acknowledgement);fireEvent.click(screen.getByLabelText('I have reviewed the roles, mappings, orientation, and landmarks'));
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    const choices=native.preparationReview.mock.calls.at(-1)![2];expect(choices.mapping_anchors).toEqual({extra_hair_a:anchor,extra_hair_b:anchor});expect(choices.follow_motion_reviewed).toBe(true);
    fireEvent.click(screen.getByRole('button',{name:'Save mapping preset'}));await screen.findByText('Mapping preset saved for this rig pair.');expect(native.preparationSaveMappingPreset).toHaveBeenCalledWith(expect.anything(),choices);
  });
  it('fills supported roles on inspection and updates role mappings together',async()=>{
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Humanoid roles');
    expect(screen.getByLabelText('Source pelvis')).toHaveValue(analysis.suggestions.source!.roles.pelvis);
    const alternate=analysis.analysis.bones.find(b=>b.name!==analysis.suggestions.source!.roles.head)!;
    fireEvent.change(screen.getByLabelText('Source head'),{target:{value:alternate.name}});
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    const choices=native.preparationReview.mock.calls.at(-1)![2];expect(choices.mapping[alternate.name]).toBe(choices.target_roles.head);expect(choices.reviewed).toBe(false);
  });
  it('groups unresolved weights and assigns an explicit shared target without approving conversion',async()=>{
    const bone=analysis.analysis.bones[0];
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,weighted_bones:[bone.name]},suggestions:{...analysis.suggestions,mapping:{},assistance:{policy:'test',labels:{},warnings:[],entries:[{source:bone.name,target:null,group:'Extra deformation bones',label:bone.name,reason:'Review required.',automatic:false}]}}});
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Humanoid roles');openPanel('Bone mapping');
    fireEvent.click(screen.getByLabelText(`Select bone ${bone.name}`));
    fireEvent.click(screen.getByText('Assign multiple bones',{selector:'summary'}));
    fireEvent.change(screen.getByLabelText('Target for selected bones'),{target:{value:analysis.target[0].name}});
    fireEvent.click(screen.getByRole('button',{name:`Assign ${analysis.target[0].name} to 1 selected bones`}));
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    expect(native.preparationReview.mock.calls.at(-1)![2].mapping[bone.name]).toBe(analysis.target[0].name);
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();
  });
  it('shows the helper exclusion note without implying conversion or game validation',async()=>{
    const warning='Excluded 284 MMD collision helper meshes from character geometry. The source file is unchanged. MMD physics is not converted to Deadlock cloth.';
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,warnings:[warning]}});
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));
    await screen.findByText('Source inspection');fireEvent.click(screen.getByText('Inspection details & notes',{selector:'summary'}));
    expect(await screen.findByText(warning)).toBeVisible();
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();
    expect(native.preparationConvert).not.toHaveBeenCalled();
  });
  it('requires a fresh successful native review and rechecks immediately before conversion',async()=>{
    native.preparationConvert.mockResolvedValue({dmx:'C:\\prepared.dmx',blend:'C:\\prepared.blend',report:'C:\\report.json',textures:'',target_model:analysis.target_model,warnings:[]});
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');openPanel('Fit & review');
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();
    fireEvent.click(screen.getByLabelText('I have reviewed the roles, mappings, orientation, and landmarks'));fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    fireEvent.click(screen.getByRole('button',{name:'Convert model'}));await screen.findByText('Prepared output');
    expect(native.preparationReview).toHaveBeenCalledTimes(2);expect(native.preparationConvert).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole('button',{name:'Re-export corrections'}));await screen.findByText('Prepared output');
    expect(native.preparationConvert.mock.calls.at(-1)![0]).toMatchObject({preparation_report:'C:\\report.json',preparation_blend:'C:\\prepared.blend'});
    expect(native.preparationConvert.mock.calls.at(-1)![3]).toBe(true);
    fireEvent.click(screen.getByText('Advanced conversion options',{selector:'summary'}));
    fireEvent.change(screen.getByLabelText('Maximum skin influences'),{target:{value:'3'}});
    expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();expect(screen.queryByRole('button',{name:'Use prepared model'})).not.toBeInTheDocument();
  });
  it('surfaces blocking native review issues without calling conversion',async()=>{
    native.preparationReview.mockResolvedValue([{tab:'Mapping',message:'Map weighted source bone mixamorig:Hips.'}]);
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');fireEvent.click(screen.getByRole('button',{name:'Check review'}));
    await screen.findByText(/Map weighted source bone/);fireEvent.click(screen.getByRole('button',{name:'Open section'}));expect(screen.getByRole('tab',{name:/Bone mapping/})).toHaveAttribute('aria-selected','true');expect(screen.getByRole('button',{name:'Convert model'})).toBeDisabled();expect(native.preparationConvert).not.toHaveBeenCalled();
  });
});


describe('Preparation workspace navigation',()=>{
  it('keeps choices across keyboard navigation and body-region filters without running native work',async()=>{
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');
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
  it('keeps acceptance explicit after conversion and keeps reconversion available',async()=>{
    const output={dmx:'C:/prepared.dmx',blend:'C:/prepared.blend',report:'C:/report.json',textures:'',target_model:analysis.target_model,warnings:[]};
    native.preparationConvert.mockResolvedValue(output);native.preparationAccept.mockResolvedValue(initial);
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'Inspect source'}));await screen.findByText('Source inspection');openPanel('Fit & review');
    fireEvent.click(screen.getByLabelText('I have reviewed the roles, mappings, orientation, and landmarks'));
    fireEvent.click(screen.getByRole('button',{name:'Check review'}));await screen.findByText('Review checks passed.');
    fireEvent.click(screen.getByRole('button',{name:'Convert model'}));await screen.findByText('Prepared output');
    expect(screen.getByRole('button',{name:'Convert model'})).toBeEnabled();
    expect(native.preparationAccept).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button',{name:'Use prepared model'}));
    await vi.waitFor(()=>expect(native.preparationAccept).toHaveBeenCalledWith(expect.anything(),output));
  });
});
