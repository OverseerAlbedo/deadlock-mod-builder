import { useEffect, useState } from 'react';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PreparationPage } from './PreparationPage';
import type { PreparationAnalysis } from './models';
import type { Project } from '../models';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn(),preparationWeaponPreview:vi.fn(),validateAssetPath:vi.fn(),previewSource:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
const probe=vi.hoisted(()=>({mounts:0,unmounts:0}));
// The joint-check view, as a probe that counts its mounts.
vi.mock('./MappingViewport',()=>({default:function Probe(){useEffect(()=>{probe.mounts++;return()=>{probe.unmounts++;};},[]);return <div data-testid="joint-view">joint view</div>;}}));
vi.mock('../preview/gltf-packet',async original=>({...await original<typeof import('../preview/gltf-packet')>(),decodeGltfPreview:()=>({metadata:{source_sha256:'a'.repeat(64)}})}));
const analysis={source:'C:\\source.blend',source_original:'C:\\source.blend',source_sha256:'a'.repeat(64),target_model:'models/heroes/test.vmdl',target_sha256:'b'.repeat(64),analysis:sourceFixture.inspection,target:sourceFixture.target,target_skeleton:sourceFixture.skeleton,target_scale_audit:sourceFixture.scale_audit,suggestions:sourceFixture.cases[1].expected,rotation:[0,0,0],geometry_customization:{props:[],face:null},clothing:null} as unknown as PreparationAnalysis;
const initial={...projectFixture.cases[0].expected,original_model:analysis.target_model,preparation_source:analysis.source_original,preparation_options:{},model_input_mode:'rigged_humanoid'} as unknown as Project;
function Harness({start=initial,expert}:{start?:Project;expert?:boolean}){const[project,setProject]=useState(start);return <PreparationPage project={project} update={setProject} busy={false} operate={async(_,task)=>task()} expert={expert}/>;}
async function check(){fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));await screen.findByRole('tablist',{name:'Preparation review sections'});}
beforeEach(()=>{vi.clearAllMocks();probe.mounts=0;probe.unmounts=0;native.preparationInspect.mockResolvedValue(analysis);native.preparationSuggest.mockResolvedValue(analysis.suggestions);native.preparationReview.mockResolvedValue([]);native.previewSource.mockResolvedValue(new ArrayBuffer(0));native.validateAssetPath.mockImplementation(async(path:string)=>path);native.preparationWeaponPreview.mockReturnValue(new Promise(()=>undefined));});

describe('Prepare model frame',()=>{
  it('shows the model file and the start buttons openly before a check, with no menus',()=>{
    render(<Harness start={{...initial,preparation_source:''}}/>);
    expect(screen.getByLabelText('Model file')).toBeVisible();
    expect(screen.getByLabelText('Gun from another file (optional)')).toBeVisible();
    for(const name of ['Prepare automatically','Load and check only',/Preview the model and animation/])expect(screen.getByRole('button',{name})).toBeVisible();
    expect(screen.queryByRole('button',{name:/Model file & orientation/})).toBeNull();
    expect(screen.getByText('Not checked yet')).toBeVisible();
  });
  it('keeps one 3D view mounted with its loaded preview while the tab, the fit group and the menus change',async()=>{
    render(<Harness/>);await check();
    fireEvent.click(screen.getByRole('button',{name:'Load model for mapping review'}));
    expect(await screen.findByTestId('joint-view')).toBeVisible();
    for(const tab of [/Joint matches/,/Moving parts/,/Gun/,/Fit & review/,/Body roles/]){
      fireEvent.click(screen.getByRole('tab',{name:tab}));
      expect(screen.getByTestId('joint-view')).toBeVisible();
    }
    fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    fireEvent.click(screen.getByRole('tab',{name:/^Feet/}));
    fireEvent.click(screen.getByRole('button',{name:/^Notes/}));
    fireEvent.click(screen.getByRole('button',{name:/Model file & orientation/}));
    expect(screen.getByTestId('joint-view')).toBeVisible();
    expect(probe.mounts).toBe(1);
    expect(probe.unmounts).toBe(0);
    expect(native.previewSource).toHaveBeenCalledOnce();
  });
  it('opens one menu at a time under the bar, keeps it open while working in it and closes it on Escape',async()=>{
    render(<Harness/>);await check();
    const model=screen.getByRole('button',{name:/Model file & orientation/}),notes=screen.getByRole('button',{name:/^Notes/});
    expect(model).toHaveAttribute('aria-expanded','false');
    fireEvent.click(model);
    expect(model).toHaveAttribute('aria-expanded','true');
    const panel=screen.getByRole('region',{name:'Model file & orientation'});
    expect(model).toHaveAttribute('aria-controls',panel.id);
    expect(within(panel).getByRole('button',{name:'Load and check only'})).toBeVisible();
    // A click elsewhere on the page leaves it open (a form in it stays usable).
    fireEvent.mouseDown(document.body);fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    expect(screen.getByRole('region',{name:'Model file & orientation'})).toBeVisible();
    // Another menu replaces it.
    fireEvent.click(notes);
    expect(screen.queryByRole('region',{name:'Model file & orientation'})).toBeNull();
    const notesPanel=screen.getByRole('region',{name:'Notes'});
    expect(within(notesPanel).getByRole('button',{name:'Re-detect body mapping'})).toBeVisible();
    fireEvent.keyDown(within(notesPanel).getByRole('button',{name:'Re-detect body mapping'}),{key:'Escape'});
    expect(screen.queryByRole('region',{name:'Notes'})).toBeNull();
    expect(notes).toHaveFocus();
    expect(native.preparationSuggest).not.toHaveBeenCalled();
  });
  it('keeps the Model file form in place while a new path or rotation drops the check',async()=>{
    render(<Harness/>);await check();
    fireEvent.click(screen.getByRole('button',{name:/Model file & orientation/}));
    fireEvent.click(screen.getByRole('button',{name:'Edit path'}));
    const path=screen.getByLabelText('Model file') as HTMLInputElement;
    fireEvent.change(path,{target:{value:'C:\\models\\other.fbx'}});
    // The check is dropped: the panel stays where it was, now open without a menu.
    expect(screen.queryByRole('tablist',{name:'Preparation review sections'})).toBeNull();
    fireEvent.change(path,{target:{value:'C:\\models\\other2.fbx'}});
    expect(path.isConnected).toBe(true);
    expect(screen.getByLabelText('Model file')).toBe(path);
    const rotation=screen.getByLabelText('Source rotation X (degrees)');
    fireEvent.change(rotation,{target:{value:'90'}});
    expect(rotation.isConnected).toBe(true);
    expect(screen.getByLabelText('Model file')).toBe(path);
    expect(screen.getByRole('region',{name:'Model file & orientation'})).toBeVisible();
  });
  it('shows a status word on each tab and the stats in the bar',async()=>{
    render(<Harness/>);await check();
    expect(screen.getByText('Checked')).toBeVisible();
    expect(screen.getByText(/\d+ bones · \d+ mesh(es)? · [\d,]+ vertices · 0 need a target/)).toBeVisible();
    // The app's order; Moving parts before the matches (kept moving parts leave the matching list).
    const tabs=within(screen.getByRole('tablist',{name:'Preparation review sections'})).getAllByRole('tab').map(tab=>tab.textContent);
    const labels=['Body roles','Moving parts','Joint matches','Gun','Fit & review'];
    expect(tabs).toHaveLength(labels.length);
    labels.forEach((label,index)=>expect(tabs[index]!.startsWith(`${label} `)).toBe(true));
    expect(tabs[0]).toBe('Body roles ✓');expect(tabs[2]).toBe('Joint matches ✓');expect(tabs[4]).toBe('Fit & review ✓');
  });
  it('marks the Shape keys chip and the tab when a shape key choice is needed, and opens that group',async()=>{
    native.preparationInspect.mockResolvedValue({...analysis,analysis:{...analysis.analysis,morphs:[{name:'body/Smile',mesh:'body',shape:'Smile'}]}});
    render(<Harness/>);await check();
    expect(screen.getByRole('tab',{name:/Fit & review/})).toHaveTextContent('Needs your choice');
    fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    expect(screen.getByRole('tab',{name:/^Shape keys/})).toHaveAttribute('aria-selected','true');
    expect(screen.getByRole('radiogroup',{name:'Shape key handling'})).toBeVisible();
    expect(within(screen.getByRole('radiogroup',{name:'Shape key handling'})).getAllByRole('radio').some(radio=>(radio as HTMLInputElement).checked)).toBe(false);
  });
  it('lists the Leg joints choices as cards in their display order with their help, and no Last conversion without one',async()=>{
    render(<Harness start={{...initial,preparation_options:{fit_mode:'preserve_proportions'}}}/>);await check();
    fireEvent.click(screen.getByRole('tab',{name:/Fit & review/}));
    fireEvent.click(screen.getByRole('tab',{name:/^Legs/}));
    const cards=within(screen.getByRole('radiogroup',{name:'Leg joints'})).getAllByRole('radio');
    expect(cards.map(card=>(card as HTMLInputElement).value)).toEqual(['pelvis','joints','hips','scaled','scaled_joints']);
    expect(cards[3].closest('label')).toHaveTextContent(/resizes.*try it only when the other choices do not suit your character/);
    expect(screen.queryByText('Last conversion')).toBeNull();
  });
});
