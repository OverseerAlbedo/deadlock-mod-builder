import { useState } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { hasPreparationChoices, PreparationPage } from './PreparationPage';
import type { PreparationAnalysis } from './models';
import type { DefaultFit, Project } from '../models';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

// Settings → Appearance: Default fit for new models and Load 3D previews automatically, on Prepare model.
const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn(),preparationWeaponPreview:vi.fn(),validateAssetPath:vi.fn(),previewSource:vi.fn(),preview:vi.fn(),cancel:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
vi.mock('./MappingViewport',()=>({default:()=><div data-testid="joint-view"/>}));
vi.mock('../preview/GltfViewport',()=>({default:()=><div data-testid="model-view"/>}));
vi.mock('./WeaponViewport',()=>({default:()=><div>gun viewport</div>}));
vi.mock('../preview/gltf-packet',async original=>({...await original<typeof import('../preview/gltf-packet')>(),decodeGltfPreview:()=>({metadata:{source_sha256:'a'.repeat(64)}})}));

const analysis={source:'C:\\staged\\source.blend',source_original:'C:\\source.blend',source_sha256:'a'.repeat(64),target_model:'models/heroes/test.vmdl',target_sha256:'b'.repeat(64),analysis:sourceFixture.inspection,target:sourceFixture.target,target_skeleton:sourceFixture.skeleton,target_scale_audit:sourceFixture.scale_audit,suggestions:sourceFixture.cases[1].expected,rotation:[0,0,0],geometry_customization:{props:[],face:null},clothing:null} as unknown as PreparationAnalysis;
/** A project that was checked before (its model file is recorded) with no fit saved. */
const checkedBefore={...projectFixture.cases[0].expected,original_model:analysis.target_model,preparation_source:analysis.source_original,preparation_options:{},model_input_mode:'rigged_humanoid'} as unknown as Project;
/** A project whose model is checked for the first time. */
const fresh={...checkedBefore,preparation_source:''} as Project;
const output={dmx:'C:\\prepared.dmx',blend:'C:\\prepared.blend',report:'C:\\report.json',textures:'',target_model:analysis.target_model,warnings:[]};
type HarnessProps={start?:Project;defaultFit?:DefaultFit;autoPreviews?:boolean;active?:boolean;busy?:boolean;expert?:boolean;next?:()=>void};
/** `operate` as App's run: one operation at a time, failures caught (shown as a quiet line). */
function Harness({start=fresh,...rest}:HarnessProps){
  const[project,setProject]=useState(start);
  const[running,setRunning]=useState(false);
  const operate=async(_:string,task:()=>Promise<void>)=>{if(gate.active)return;gate.active=true;setRunning(true);try{await task();}catch{/* shown by App */}finally{gate.active=false;setRunning(false);}};
  return <><PreparationPage project={project} update={setProject} operate={operate} {...rest} busy={Boolean(rest.busy)||running}/><output data-testid="project">{JSON.stringify(project)}</output></>;
}
const gate={active:false};
const current=()=>JSON.parse(screen.getByTestId('project').textContent!) as Project;
const radios=(group:string)=>within(screen.getByRole('radiogroup',{name:group})).getAllByRole('radio') as HTMLInputElement[];
const chosen=(group:string)=>radios(group).find(radio=>radio.checked)?.value??'';
const openGroup=(name:string)=>fireEvent.click(screen.getByRole('tab',{name:new RegExp(`^${name}`)}));
/** Chooses the model file (as step 1 does without preparing). */
const pick=(path='C:\\source.blend')=>act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:path}));});
async function check(){if(!screen.queryByRole('button',{name:'Load and check only'}))fireEvent.click(screen.getByRole('button',{name:/Model file & orientation/}));fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByRole('tablist',{name:'Preparation review sections'});}
/** Lets the automatic load's short pause pass. */
const settle=()=>act(async()=>{await new Promise(resolve=>setTimeout(resolve,150));});
beforeEach(()=>{vi.clearAllMocks();gate.active=false;native.preparationInspect.mockResolvedValue(analysis);native.preparationSuggest.mockResolvedValue(analysis.suggestions);native.preparationReview.mockResolvedValue([]);native.preparationConvert.mockResolvedValue(output);
  native.validateAssetPath.mockImplementation(async(path:string)=>path);native.preparationWeaponPreview.mockReturnValue(new Promise(()=>undefined));native.previewSource.mockResolvedValue(new ArrayBuffer(0));native.cancel.mockResolvedValue(undefined);
  native.preparationAccept.mockImplementation(async(project:Project,result:typeof output)=>({...project,dmx:result.dmx,preparation_report:result.report,preparation_blend:result.blend}));});

describe('Default fit for new models',()=>{
  it('writes Preserve source proportions into a model checked for the first time, with the badge on its card and Leg joints offered',async()=>{
    render(<Harness/>);pick();await check();
    expect(current().preparation_options.fit_mode).toBe('preserve_proportions');
    fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    expect(chosen('Model fitting')).toBe('preserve_proportions');
    // Match hero proportions stays first; the badge goes with the setting.
    expect(radios('Model fitting').map(radio=>radio.value)).toEqual(['match_target','preserve_proportions']);
    expect(screen.getByRole('radio',{name:/Preserve source proportions\s*default/})).toBeInTheDocument();
    expect(screen.queryByRole('radio',{name:/Match hero proportions\s*default/})).toBeNull();
    openGroup('Legs');expect(chosen('Leg joints')).toBe('pelvis');
    fireEvent.click(screen.getByRole('button',{name:'Prepare model'}));await screen.findByText('Prepared output');
    expect(native.preparationConvert.mock.calls.at(-1)![2]).toMatchObject({fit_mode:'preserve_proportions'});
    expect(native.preparationConvert.mock.calls.at(-1)![2].fit_leg_joints).toBeUndefined();
  });
  it('writes Match hero proportions when the setting names it, and moves the badge',async()=>{
    render(<Harness defaultFit="match_target"/>);pick();await check();
    expect(current().preparation_options.fit_mode).toBe('match_target');
    fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    expect(chosen('Model fitting')).toBe('match_target');
    expect(screen.getByRole('radio',{name:/Match hero proportions\s*default/})).toBeInTheDocument();
    expect(screen.queryByRole('radio',{name:/Preserve source proportions\s*default/})).toBeNull();
  });
  it('gives the no-click automatic preparation the setting’s fit in Simple mode',async()=>{
    const next=vi.fn();
    render(<Harness expert={false} next={next}/>);
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path:'C:\\models\\mine.blend',auto:true}}));});
    await waitFor(()=>expect(next).toHaveBeenCalledOnce());
    expect(native.preparationConvert.mock.calls[0][2]).toMatchObject({fit_mode:'preserve_proportions',reviewed:true});
  });
  it('keeps Match hero proportions for a project checked before without a fit, whatever the setting says',async()=>{
    render(<Harness start={checkedBefore}/>);await check();
    expect(current().preparation_options.fit_mode).toBe('match_target');
    fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    expect(chosen('Model fitting')).toBe('match_target');
  });
  it('keeps a saved fit, and an earlier project keeps its fit for a changed model file',async()=>{
    render(<Harness start={{...checkedBefore,preparation_options:{fit_mode:'preserve_proportions'}}} defaultFit="match_target"/>);await check();
    expect(current().preparation_options.fit_mode).toBe('preserve_proportions');
    // Another model file in the same project (not automatic): the project's fit carries over.
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:'C:\\models\\other.fbx'}));});
    await check();
    expect(native.preparationInspect.mock.calls.at(-1)![1]).toBe('C:\\models\\other.fbx');
    expect(current().preparation_options.fit_mode).toBe('preserve_proportions');
  });
  it('treats a changed model file in a project prepared before without a fit as that project’s Match hero proportions',async()=>{
    render(<Harness start={{...checkedBefore,preparation_report:'C:\\report.json'}}/>);
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:'C:\\models\\other.fbx'}));});
    await check();
    expect(current().preparation_options.fit_mode).toBe('match_target');
  });
  it('writes the setting’s fit into an unrigged model checked for the first time, and Match hero proportions when the setting names it',async()=>{
    const unrigged={...analysis,analysis:{...analysis.analysis,suggested_mode:'unrigged_humanoid'}};
    native.preparationInspect.mockResolvedValue(unrigged);
    const view=render(<Harness/>);pick();await check();
    expect(current().model_input_mode).toBe('unrigged_humanoid');
    expect(current().preparation_options.fit_mode).toBe('preserve_proportions');
    view.unmount();
    render(<Harness defaultFit="match_target"/>);pick();await check();
    expect(current().preparation_options.fit_mode).toBe('match_target');
  });
  it('keeps Match hero proportions for an unrigged model in a project checked before without a fit',async()=>{
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,suggested_mode:'unrigged_humanoid'}});
    render(<Harness start={{...checkedBefore,model_input_mode:'unrigged_humanoid'} as Project}/>);await check();
    expect(current().model_input_mode).toBe('unrigged_humanoid');
    expect(current().preparation_options.fit_mode).toBe('match_target');
  });
  it('leaves a gun project’s fit as it was: a gun-only preparation does not use it',async()=>{
    render(<Harness start={{...fresh,model_input_mode:'weapon'} as Project}/>);pick('C:\\models\\gun.blend');await check();
    expect(current().model_input_mode).toBe('weapon');
    expect(current().preparation_options.fit_mode).toBe('match_target');
  });
  it('reads which projects have preparation choices of their own',()=>{
    expect(hasPreparationChoices(fresh)).toBe(false);
    expect(hasPreparationChoices({...fresh,preparation_options:{weapon_source:'C:\\gun.blend',reviewed:false}})).toBe(false);
    expect(hasPreparationChoices(checkedBefore)).toBe(true);
    expect(hasPreparationChoices({...fresh,preparation_report:'C:\\report.json'})).toBe(true);
    expect(hasPreparationChoices({...fresh,preparation_options:{mapping:{hips:'pelvis'}}})).toBe(true);
  });
});

describe('Load 3D previews automatically on Prepare model',()=>{
  it('loads nothing by itself when the setting is off',async()=>{
    render(<Harness/>);pick();await check();await settle();
    expect(native.previewSource).not.toHaveBeenCalled();
    expect(screen.getByRole('button',{name:'Load model for mapping review'})).toBeInTheDocument();
  });
  it('loads the shown Joint check view once after a check, and the other views when they are shown',async()=>{
    render(<Harness autoPreviews/>);pick();await check();
    expect(await screen.findByTestId('joint-view')).toBeInTheDocument();
    expect(native.previewSource).toHaveBeenCalledOnce();
    expect(native.previewSource).toHaveBeenLastCalledWith(analysis.source);
    fireEvent.click(screen.getByRole('tab',{name:/Joint matches/}));
    fireEvent.click(screen.getByRole('tab',{name:/Body roles/}));
    await settle();
    expect(native.previewSource).toHaveBeenCalledOnce();
    // Your model: the file as it is, loaded once when that view is shown.
    fireEvent.click(screen.getByRole('button',{name:'Your model'}));
    expect(await screen.findByTestId('model-view')).toBeInTheDocument();
    expect(native.previewSource).toHaveBeenCalledTimes(2);
    expect(native.previewSource).toHaveBeenLastCalledWith(analysis.source_original);
    fireEvent.click(screen.getByRole('button',{name:'Joint check'}));
    await settle();
    expect(native.previewSource).toHaveBeenCalledTimes(2);
  });
  it('waits while the page is hidden or the app is busy, then loads once',async()=>{
    const view=render(<Harness autoPreviews active={false}/>);pick();await check();await settle();
    expect(native.previewSource).not.toHaveBeenCalled();
    view.rerender(<Harness autoPreviews busy/>);await settle();
    expect(native.previewSource).not.toHaveBeenCalled();
    view.rerender(<Harness autoPreviews/>);
    await waitFor(()=>expect(native.previewSource).toHaveBeenCalledOnce());
    view.rerender(<Harness autoPreviews busy/>);view.rerender(<Harness autoPreviews/>);await settle();
    expect(native.previewSource).toHaveBeenCalledOnce();
  });
  it('does not try again after a failure for the same inspection, but does after a new check',async()=>{
    native.previewSource.mockRejectedValue(new Error('Blender is not set'));
    const view=render(<Harness autoPreviews/>);pick();await check();
    await waitFor(()=>expect(native.previewSource).toHaveBeenCalledOnce());
    view.rerender(<Harness autoPreviews busy/>);view.rerender(<Harness autoPreviews/>);await settle();
    expect(native.previewSource).toHaveBeenCalledOnce();
    expect(screen.getByRole('button',{name:'Load model for mapping review'})).toBeInTheDocument();
    await check();await settle();
    expect(native.preparationInspect).toHaveBeenCalledTimes(2);
    await waitFor(()=>expect(native.previewSource).toHaveBeenCalledTimes(2));
  });
  it('does not try a failed view again after Check for problems, Re-detect body mapping or a switch to another view and back',async()=>{
    native.previewSource.mockRejectedValue(new Error('Blender is not set'));
    render(<Harness autoPreviews/>);pick();await check();
    await waitFor(()=>expect(native.previewSource).toHaveBeenCalledOnce());
    fireEvent.click(screen.getByRole('button',{name:'Check for problems'}));await screen.findByText('No problems found.');
    fireEvent.click(screen.getByRole('button',{name:/^Notes/}));fireEvent.click(screen.getByRole('button',{name:'Re-detect body mapping'}));
    await waitFor(()=>expect(native.preparationSuggest).toHaveBeenCalled());
    await settle();
    expect(native.previewSource).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole('button',{name:'Your model'}));
    await waitFor(()=>expect(native.previewSource).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByRole('button',{name:'Joint check'}));await settle();
    expect(native.previewSource).toHaveBeenCalledTimes(2);
  });
  it('stops a load still running when the page is left, and loads again on return',async()=>{
    let stop:(error:Error)=>void=()=>undefined;
    native.previewSource.mockReturnValueOnce(new Promise<ArrayBuffer>((_,fail)=>{stop=fail;}));
    const view=render(<Harness autoPreviews/>);pick();await check();
    await waitFor(()=>expect(native.previewSource).toHaveBeenCalledOnce());
    view.rerender(<Harness autoPreviews active={false}/>);
    await waitFor(()=>expect(native.cancel).toHaveBeenCalledOnce());
    // The backend stops the run: the load ends with a cancellation.
    await act(async()=>{stop(new Error('Operation cancelled'));await Promise.resolve();});
    view.rerender(<Harness autoPreviews/>);
    await waitFor(()=>expect(native.previewSource).toHaveBeenCalledTimes(2));
  });
  it('leaves the no-click automatic preparation alone: nothing is previewed while it runs or after it moved on',async()=>{
    function Moving(){const[active,setActive]=useState(true);return <Harness autoPreviews expert={false} active={active} next={()=>setActive(false)}/>;}
    render(<Moving/>);
    act(()=>{window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path:'C:\\models\\mine.blend',auto:true}}));});
    await waitFor(()=>expect(native.preparationAccept).toHaveBeenCalledOnce());
    await settle();
    expect(native.preparationInspect).toHaveBeenCalledOnce();
    expect(native.previewSource).not.toHaveBeenCalled();
  });
});
