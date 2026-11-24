import { useState } from 'react';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PreparationPage } from './PreparationPage';
import { RolesPanel } from './panels/RolesPanel';
import { ROLES, type MissingSkeleton, type PreparationAnalysis } from './models';
import type { Project } from '../models';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn(),preparationWeaponPreview:vi.fn(),validateAssetPath:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
const dialog=vi.hoisted(()=>({open:vi.fn()}));
vi.mock('@tauri-apps/plugin-dialog',()=>dialog);
import { forgetSkeletonChoices } from './skeletonChoice';
const rigged=sourceFixture.cases[1].expected as unknown as PreparationAnalysis['suggestions'];
const gap:MissingSkeleton={policy:'missing-skeleton-v1',groups:353,meshes:['Body'],modifier:true};
/** A file whose mesh is weighted to bones that are not in it: no armature, unrigged humanoid, the shape estimate failed. */
function unrigged(record?:MissingSkeleton):PreparationAnalysis{
  return {source:'C:\\source.blend',source_original:'C:\\source.blend',source_sha256:'a'.repeat(64),target_model:'models/heroes/test.vmdl',target_sha256:'b'.repeat(64),
    analysis:{...sourceFixture.inspection,armatures:0,bones:[],weighted_bones:[],suggested_mode:'unrigged_humanoid',
      humanoid:{status:'Unsupported',errors:['Unsupported: two separated legs were not identified.']},
      warnings:['Excluded non-deforming vertex groups from skeletal mapping and skin weights: a, b. These groups do not drive deform bones; the original source file is unchanged.'],
      ...(record?{missing_skeleton:record}:{})},
    target:sourceFixture.target,target_skeleton:sourceFixture.skeleton,target_scale_audit:sourceFixture.scale_audit,
    suggestions:{target:rigged.target,mapping:{}},rotation:[0,0,0],geometry_customization:{props:[],face:null},clothing:null} as unknown as PreparationAnalysis;
}
const initial={...projectFixture.cases[0].expected,original_model:'models/heroes/test.vmdl',preparation_source:'C:\\source.blend',preparation_options:{},model_input_mode:'rigged_humanoid'} as unknown as Project;
function Harness({expert}:{expert?:boolean}){const[project,setProject]=useState(initial);return <PreparationPage project={project} update={setProject} busy={false} operate={async(_,task)=>task()} expert={expert}/>;}
async function check(){fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByRole('tablist',{name:'Preparation review sections'});}
const notes=()=>screen.queryAllByRole('note',{name:'No skeleton in this file'});
/** What the check found that needs a look: shown openly under the top bar (it was the What we found card). */
const found=()=>screen.getByRole('region',{name:'What we found in your model'});
beforeEach(()=>{vi.clearAllMocks();native.preparationSuggest.mockImplementation(async(value:PreparationAnalysis)=>value.suggestions);native.preparationReview.mockResolvedValue([]);native.validateAssetPath.mockImplementation(async(path:string)=>path);native.preparationWeaponPreview.mockReturnValue(new Promise(()=>undefined));});

describe('A file with bone weights but no skeleton',()=>{
  for(const expert of [true,false]){
    it(`says so in What we found, outside the details (${expert?'Expert':'Simple'})`,async()=>{
      native.preparationInspect.mockResolvedValue(unrigged(gap));
      render(<Harness expert={expert}/>);await check();
      const note=within(found()).getByRole('note',{name:'No skeleton in this file'});
      expect(note).toBeVisible();
      expect(note.closest('details')).toBeNull();
      expect(note).toHaveTextContent('This file’s mesh has weights for 353 bones but no skeleton; the skeleton (armature) was not saved with it.');
      expect(note).toHaveTextContent('the joints cannot be matched to the hero’s from your model’s own bones');
      expect(note).toHaveTextContent('it leaves these weights out and makes new ones from joint positions it estimates from the body’s shape. For this model those positions could not be found');
      expect(note).toHaveTextContent('Choose a file that includes the skeleton, for example an FBX or the original model this mesh came from. Or add the skeleton back in Blender and save the file again.');
      expect(within(found()).getByText('This file has no skeleton. Read the note below before preparing.')).toBeVisible();
      expect(screen.queryByText(/bones matched to/)).toBeNull();
      if(!expert){
        // Simple mode keeps its way into Body roles, where the landmark fields are.
        expect(screen.getByText(/No bones to match: this file has no skeleton\./)).toBeVisible();
        fireEvent.click(screen.getByRole('button',{name:'Edit matches'}));
        expect(within(screen.getByRole('tabpanel',{name:/Body roles/})).getByRole('note',{name:'No skeleton in this file'})).toBeVisible();
      }
    });
    it(`shows nothing without the record (${expert?'Expert':'Simple'})`,async()=>{
      native.preparationInspect.mockResolvedValue(unrigged());
      render(<Harness expert={expert}/>);await check();
      expect(notes()).toHaveLength(0);
    });
  }
  it('repeats the note at the top of Body roles in Unrigged humanoid mode',async()=>{
    native.preparationInspect.mockResolvedValue(unrigged(gap));
    render(<Harness/>);await check();
    const roles=screen.getByRole('tabpanel',{name:/Body roles/});
    expect(within(roles).getByRole('note',{name:'No skeleton in this file'})).toBeVisible();
    expect(within(roles).getByLabelText('pelvis landmark X')).toBeVisible();
    expect(notes()).toHaveLength(2);
  });
  it('names the meshes in the plural and leaves out the shape sentence when the estimate worked',async()=>{
    const value=unrigged({...gap,groups:1,meshes:['Body','Hair']});
    value.analysis.humanoid={status:'Needs Review',landmarks:{}};
    native.preparationInspect.mockResolvedValue(value);
    render(<Harness/>);await check();
    const note=within(found()).getByRole('note',{name:'No skeleton in this file'});
    expect(note).toHaveTextContent('This file’s meshes have weights for 1 bone but no skeleton; the skeleton (armature) was not saved with them.');
    expect(note).not.toHaveTextContent('could not be found');
  });
});

describe('Automatic preparation of a file without its skeleton',()=>{
  it('stops at the landmark check with the note shown, and Open section leads to Body roles',async()=>{
    native.preparationInspect.mockResolvedValue(unrigged(gap));
    // The review's real answer for an unrigged model without landmarks (algorithms.rs review_issues).
    native.preparationReview.mockResolvedValue([{tab:'landmarks',message:`Enter three finite coordinates for: ${ROLES.join(', ')}.`,roles:[...ROLES]}]);
    render(<Harness expert={false}/>);
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path:'C:\\models\\mine.blend',auto:true}}));});
    await screen.findByText(/Enter three finite coordinates for: pelvis/);
    expect(native.preparationConvert).not.toHaveBeenCalled();
    expect(within(found()).getByRole('note',{name:'No skeleton in this file'})).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:/Open section/}));
    const roles=screen.getByRole('tabpanel',{name:/Body roles/});
    expect(roles).toBeVisible();
    expect(within(roles).getByRole('note',{name:'No skeleton in this file'})).toBeVisible();
  });
});

describe('Automatic preparation of a file without its skeleton whose shape estimate works',()=>{
  const output={dmx:'C:\\out\\model.dmx',blend:'C:\\out\\model.blend',report:'C:\\out\\receipt.json',textures:'C:\\out\\textures',target_model:'models/heroes/test.vmdl',warnings:[]};
  /** The body shape gave joint positions: the check passes and nothing else would stop the conversion. */
  function estimated(record?:MissingSkeleton){
    const value=unrigged(record);
    value.analysis.humanoid={status:'Needs Review',landmarks:{pelvis:[0,0,1]}} as unknown as PreparationAnalysis['analysis']['humanoid'];
    return value;
  }
  function Auto({expert,next}:{expert?:boolean;next:()=>void}){const[project,setProject]=useState(initial);return <PreparationPage project={project} update={setProject} busy={false} operate={async(_,task)=>task()} expert={expert} next={next}/>;}
  const choose=(path='C:\\models\\mine.blend')=>act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path,auto:true}}));});
  const stop=async()=>{const button=await screen.findByRole('button',{name:'Continue with automatic weights'});expect(found()).toContainElement(button);return button;};
  beforeEach(()=>{forgetSkeletonChoices();native.preparationConvert.mockResolvedValue(output);native.preparationAccept.mockImplementation(async(project:Project)=>({...project,dmx:output.dmx}));});
  it('stops before converting in Simple mode and shows the note with the two choices',async()=>{
    const next=vi.fn();
    native.preparationInspect.mockResolvedValue(estimated(gap));
    render(<Auto expert={false} next={next}/>);choose();
    await stop();
    expect(native.preparationReview).toHaveBeenCalledOnce();
    expect(native.preparationConvert).not.toHaveBeenCalled();
    expect(next).not.toHaveBeenCalled();
    const note=within(found()).getByRole('note',{name:'No skeleton in this file'});
    expect(note).toHaveTextContent('Preparation stopped here, before converting your model, so you can choose.');
    expect(within(note).getByRole('button',{name:'Choose another model file'})).toBeEnabled();
  });
  it('converts with automatic weights after Continue, and does not ask again for the same file',async()=>{
    const next=vi.fn();
    native.preparationInspect.mockResolvedValue(estimated(gap));
    render(<Auto expert={false} next={next}/>);choose();
    fireEvent.click(await stop());
    await vi.waitFor(()=>expect(next).toHaveBeenCalledOnce());
    expect(native.preparationConvert).toHaveBeenCalledOnce();
    expect(native.preparationConvert.mock.calls[0][2]).toMatchObject({reviewed:true});
    expect(native.preparationAccept).toHaveBeenCalledOnce();
    // Prepare again with the same file: straight through.
    choose();
    await vi.waitFor(()=>expect(next).toHaveBeenCalledTimes(2));
    expect(native.preparationConvert).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole('button',{name:'Continue with automatic weights'})).toBeNull();
  });
  it('asks again for a changed file',async()=>{
    native.preparationInspect.mockResolvedValue(estimated(gap));
    render(<Auto expert={false} next={vi.fn()}/>);choose();
    fireEvent.click(await stop());
    await vi.waitFor(()=>expect(native.preparationConvert).toHaveBeenCalledOnce());
    native.preparationInspect.mockResolvedValue({...estimated(gap),source_sha256:'c'.repeat(64)});
    choose();
    await stop();
    expect(native.preparationConvert).toHaveBeenCalledOnce();
  });
  it('lets the user choose another model file, which is checked and prepared automatically',async()=>{
    native.preparationInspect.mockResolvedValue(estimated(gap));
    render(<Auto expert={false} next={vi.fn()}/>);choose();
    await stop();
    dialog.open.mockResolvedValue('C:\\models\\with_skeleton.fbx');
    native.preparationInspect.mockResolvedValue(estimated());
    fireEvent.click(within(found()).getByRole('button',{name:'Choose another model file'}));
    await vi.waitFor(()=>expect(native.preparationConvert).toHaveBeenCalledOnce());
    expect(native.preparationInspect).toHaveBeenLastCalledWith(expect.anything(),'C:\\models\\with_skeleton.fbx',[0,0,0]);
  });
  it('changes nothing without the record, and nothing in Expert mode',async()=>{
    for(const [expert,record] of [[false,undefined],[true,gap]] as const){
      vi.clearAllMocks();forgetSkeletonChoices();
      native.preparationReview.mockResolvedValue([]);native.preparationSuggest.mockImplementation(async(value:PreparationAnalysis)=>value.suggestions);native.preparationConvert.mockResolvedValue(output);native.preparationAccept.mockImplementation(async(project:Project)=>({...project,dmx:output.dmx}));
      native.preparationInspect.mockResolvedValue(estimated(record));
      const next=vi.fn();
      const view=render(<Auto expert={expert} next={next}/>);choose();
      await vi.waitFor(()=>expect(next).toHaveBeenCalledOnce());
      expect(native.preparationConvert).toHaveBeenCalledOnce();
      expect(screen.queryByRole('button',{name:'Continue with automatic weights'})).toBeNull();
      view.unmount();
    }
  });
});

describe('Body roles and the missing-skeleton note',()=>{
  function panel(mode:Project['model_input_mode'],record?:MissingSkeleton){
    const analysis=unrigged(record);
    return render(<RolesPanel warnings={[]} roleGroup="all" setRoleGroup={()=>undefined} shownRoles={[...ROLES]} selectedRole="pelvis" selectRole={()=>undefined}
      project={{...initial,model_input_mode:mode}} options={{}} busy={false} changeOptions={()=>undefined} setRole={()=>undefined} analysis={analysis} boneLabel={name=>name}/>);
  }
  it('is shown only in Unrigged humanoid mode, and only with the record',()=>{
    panel('unrigged_humanoid',gap);expect(notes()).toHaveLength(1);
  });
  it('is not shown in Rigged humanoid mode',()=>{
    panel('rigged_humanoid',gap);expect(notes()).toHaveLength(0);
  });
  it('is not shown without the record',()=>{
    panel('unrigged_humanoid');expect(notes()).toHaveLength(0);
  });
});
