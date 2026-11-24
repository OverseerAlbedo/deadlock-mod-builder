import { useEffect, useState } from 'react';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PreparationPage } from './PreparationPage';
import { roleLabel } from './mapping';
import type { PreparationAnalysis, WeaponCandidate } from './models';
import type { Project } from '../models';
import type { ModelLabelSet } from '../studio/ModelLabels';
import type { LitBone } from './MappingViewport';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn(),preparationWeaponPreview:vi.fn(),validateAssetPath:vi.fn(),previewSource:vi.fn(),discoverWeaponModels:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
const probe=vi.hoisted(()=>({mounts:0,unmounts:0}));
// The Joint check view as a probe: it counts its mounts and shows the labels and the lit bone it gets.
vi.mock('./MappingViewport',()=>({default:function Probe({labels,lit}:{labels?:ModelLabelSet;lit?:LitBone|null}){
  useEffect(()=>{probe.mounts++;return()=>{probe.unmounts++;};},[]);
  return <div data-testid="joint-view"><div role="group" aria-label="labels">{labels?.items.map(item=><button key={item.id} type="button" aria-pressed={Boolean(item.active)} onClick={()=>labels.select(item.id)}>{item.text}</button>)}</div>
    <output data-testid="lit">{lit===undefined?'no switch':lit===null?'nothing lit':`${lit.bone} as ${lit.name}`}</output></div>;}}));
vi.mock('./WeaponViewport',()=>({default:()=><div>gun viewport</div>}));
vi.mock('../preview/gltf-packet',async original=>({...await original<typeof import('../preview/gltf-packet')>(),decodeGltfPreview:()=>({metadata:{source_sha256:'a'.repeat(64)}})}));

const roles=(sourceFixture.cases[1].expected as unknown as {source:{roles:Record<string,string>}}).source.roles;
// Two hanging chains (kept moving parts) under the head and the hips.
const chains=[{name:'Hair_0',parent:roles.head,position:[0,.08,1.85]},{name:'Hair_1',parent:'Hair_0',position:[0,.12,1.7]},{name:'Hair_2',parent:'Hair_1',position:[0,.14,1.55]},
  {name:'Tail_0',parent:roles.pelvis,position:[0,.12,.95]},{name:'Tail_1',parent:'Tail_0',position:[0,.3,.8]},{name:'Tail_2',parent:'Tail_1',position:[0,.45,.6]}];
const inspection={...sourceFixture.inspection,bones:[...sourceFixture.inspection.bones,...chains],weighted_bones:[...sourceFixture.inspection.weighted_bones,...chains.map(b=>b.name)]};
const analysis={source:'C:\\source.blend',source_original:'C:\\source.blend',source_sha256:'a'.repeat(64),target_model:'models/heroes/test.vmdl',target_sha256:'b'.repeat(64),analysis:inspection,target:sourceFixture.target,target_skeleton:sourceFixture.skeleton,target_scale_audit:sourceFixture.scale_audit,suggestions:sourceFixture.cases[1].expected,rotation:[0,0,0],geometry_customization:{props:[],face:null},clothing:null} as unknown as PreparationAnalysis;
const pistol:WeaponCandidate={id:'Pistol',objects:['Pistol'],vertices:24,bounds:[[0,0,0],[1,1,1]],sample:[[0,0,0]],attach_bone:roles.hand_R,attach_kind:'bone_parent',keywords:['pistol'],elongation:3};
const heroGun={policy:'weapon-fit-v1',source:'meshes',separate_model:false,meshes:['test_gun'],primary:['test_gun'],model_meshes:['test_body','test_gun'],bones:['weapon'],notes:[]};
const withGun={...analysis,analysis:{...inspection,weapons:[pistol]},weapon_reference:heroGun,weapon_suggestion:{enabled:true,action:'replace',objects:['Pistol']}} as unknown as PreparationAnalysis;
const initial={...projectFixture.cases[0].expected,original_model:analysis.target_model,preparation_source:analysis.source_original,preparation_options:{},model_input_mode:'rigged_humanoid'} as unknown as Project;
function Harness({expert}:{expert?:boolean}){const[project,setProject]=useState(initial);return <PreparationPage project={project} update={setProject} busy={false} operate={async(_,task)=>task()} expert={expert}/>;}
async function load(){fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByRole('tablist',{name:'Preparation review sections'});fireEvent.click(screen.getByRole('button',{name:'Load model for mapping review'}));await screen.findByTestId('joint-view');}
const weighted=sourceFixture.inspection.weighted_bones[0];
const labels=()=>within(screen.getByRole('group',{name:'labels'})).queryAllByRole('button');
const lit=()=>screen.getByTestId('lit').textContent;
beforeEach(()=>{vi.clearAllMocks();probe.mounts=0;probe.unmounts=0;native.preparationInspect.mockResolvedValue(analysis);native.preparationSuggest.mockResolvedValue(analysis.suggestions);native.preparationReview.mockResolvedValue([]);native.previewSource.mockResolvedValue(new ArrayBuffer(0));native.validateAssetPath.mockImplementation(async(path:string)=>path);native.preparationWeaponPreview.mockReturnValue(new Promise(()=>undefined));native.discoverWeaponModels.mockResolvedValue([]);});

describe('Labels on the character and the lit bone',()=>{
  it('labels the chosen region’s joints on Body roles; a click selects the joint and lights what its bone moves',async()=>{
    render(<Harness/>);await load();
    // All joints: every role with a bone in the model, the first (Pelvis) selected and lit.
    expect(labels()).toHaveLength(17);
    expect(lit()).toBe(`${roles.pelvis} as ${roleLabel('pelvis')}`);
    fireEvent.click(screen.getByRole('button',{name:'Legs & feet'}));
    // The region's six joints and the selected one.
    expect(labels().map(b=>b.textContent)).toEqual([...['thigh_L','shin_L','foot_L','thigh_R','shin_R','foot_R'].map(r=>roleLabel(r as never)),roleLabel('pelvis')]);
    fireEvent.click(within(screen.getByRole('group',{name:'labels'})).getByRole('button',{name:roleLabel('shin_L')}));
    expect(within(screen.getByRole('tabpanel',{name:/Body roles/})).getByRole('button',{name:roleLabel('shin_L')})).toHaveAttribute('aria-pressed','true');
    expect(lit()).toBe(`${roles.shin_L} as ${roleLabel('shin_L')}`);
    // Clicking the row (not only its name) selects too.
    fireEvent.click(screen.getByLabelText('Source foot_R').closest('tr')!);
    expect(lit()).toBe(`${roles.foot_R} as ${roleLabel('foot_R')}`);
    // A closed bone select carries the whole chosen name as its tooltip.
    for(const label of ['Source foot_R','Target foot_R']){const select=screen.getByLabelText(label) as HTMLSelectElement;expect(select.title).toBeTruthy();expect(select.title).toBe(select.selectedOptions[0].textContent);}
    // The selected one stays labelled even outside the region.
    expect(labels().map(b=>b.textContent)).toContain(roleLabel('foot_R'));
  });
  it('labels and lights the selected bone on Joint matches, nothing before one is chosen',async()=>{
    render(<Harness/>);await load();
    fireEvent.click(screen.getByRole('tab',{name:/Joint matches/}));
    expect(labels()).toHaveLength(0);
    expect(lit()).toBe('nothing lit');
    fireEvent.click(screen.getByRole('button',{name:/^All \d+$/}));
    fireEvent.click(screen.getByRole('button',{name:`Show bone ${weighted}`}));
    expect(labels().map(b=>b.textContent)).toEqual([weighted]);
    expect(lit()).toBe(`${weighted} as ${weighted}`);
  });
  it('labels each kept moving part and marks its row on a click; no labels or switch on Gun and Fit & review',async()=>{
    render(<Harness/>);await load();
    fireEvent.click(screen.getByRole('tab',{name:/Moving parts/}));
    expect(labels().map(b=>b.textContent)).toEqual(['Hair_0','Tail_0']);
    expect(lit()).toBe('no switch');
    fireEvent.click(within(screen.getByRole('group',{name:'labels'})).getByRole('button',{name:'Tail_0'}));
    expect(screen.getByLabelText('Tail_0 · 3 bones → '+analysis.suggestions.target.roles.pelvis).closest('.prep-group-row')).toHaveClass('marked');
    for(const tab of [/Fit & review/,/Gun/]){fireEvent.click(screen.getByRole('tab',{name:tab}));expect(labels()).toHaveLength(0);expect(lit()).toBe('no switch');}
  });
  it('shows nothing on the character before the Joint check view is loaded, and nothing in Simple mode until the matches are opened',async()=>{
    render(<Harness expert={false}/>);
    fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByRole('tablist',{name:'Preparation review sections'});
    expect(native.previewSource).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button',{name:'Load model for mapping review'}));await screen.findByTestId('joint-view');
    // Simple, everything matched: Fit & review is open, so no labels.
    expect(labels()).toHaveLength(0);
    fireEvent.click(screen.getByRole('button',{name:'Edit matches'}));
    expect(labels()).toHaveLength(17);
  });
});

describe('Gun tab on Prepare model',()=>{
  it('shows the gun’s fit preview with every adjust button beside the settings, and keeps the page view mounted',async()=>{
    native.preparationInspect.mockResolvedValue(withGun);
    render(<Harness/>);await load();
    fireEvent.click(screen.getByRole('tab',{name:/Gun/}));
    const panel=screen.getByRole('tabpanel',{name:/Gun/});
    const fit=within(panel).getByRole('region',{name:'Gun fit'});
    for(const name of ['Flip direction','Bigger','Tilt up 5°','Aim right 5°','▲ Up','Reset'])expect(within(fit).getByRole('button',{name})).toBeInTheDocument();
    expect(within(panel).getByRole('group',{name:'What should your character hold?'})).toBeInTheDocument();
    expect(within(panel).getByLabelText(/Use my gun instead of/)).toBeChecked();
    expect(within(panel).getByLabelText('Gun size')).toBeInTheDocument();
    // The page's view is hidden by CSS only on this tab: still mounted, nothing loaded again.
    expect(screen.getByTestId('joint-view')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('tab',{name:/Body roles/}));
    expect(screen.getByTestId('joint-view')).toBeVisible();
    expect(probe.mounts).toBe(1);expect(probe.unmounts).toBe(0);expect(native.previewSource).toHaveBeenCalledOnce();
  });
});

describe('Gun tab of Appearance & weapons (the same panel)',()=>{
  it('shows the fit preview with its buttons and puts Apply gun changes under the summary',async()=>{
    const {WeaponsSection}=await import('./WeaponsSection');
    native.preparationResume.mockResolvedValue({analysis:withGun});
    native.preparationWeaponPreview.mockResolvedValue({fitted:[[0,0,0]],reference:[[0,0,0]],fit:{policy:'weapon-fit-v1',scale:1,muzzle_offset_mm:3,skin:'rigid',skin_bones:['weapon'],notes:[],matrix:[[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]],source:{grip:[0,0,0],muzzle:[1,0,0],forward:[1,0,0],up:[0,0,1],length:1,confidence:'high',notes:[]},target:{grip:[0,0,0],muzzle:[1,0,0],forward:[1,0,0],up:[0,0,1],length:1,confidence:'high',notes:[]}}});
    function Section(){const[project,setProject]=useState({...initial,preparation_report:'C:\prep\gun-tab\report.json',preparation_options:{weapon:withGun.weapon_suggestion}} as unknown as Project);return <WeaponsSection project={project} update={setProject} busy={false} operate={async(_,task)=>task()} openPreparation={()=>undefined}/>;}
    render(<Section/>);
    const fit=await screen.findByRole('region',{name:'Gun fit'});
    expect(await within(fit).findByText('gun viewport')).toBeInTheDocument();
    expect(screen.queryByRole('button',{name:'Apply gun changes'})).toBeNull();
    fireEvent.click(within(fit).getByRole('button',{name:'Bigger'}));
    const apply=screen.getByRole('button',{name:'Apply gun changes'});
    // Under the summary, in the settings column (not under the whole panel).
    expect(apply.closest('.prep-gun-settings')).not.toBeNull();
    expect(screen.getByRole('region',{name:'Gun'})).toContainElement(apply);
  });
});
