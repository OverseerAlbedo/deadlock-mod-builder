import { useState } from 'react';
import { fireEvent,render,screen,waitFor,within } from '@testing-library/react';
import { describe,expect,it,vi } from 'vitest';
import { AppearanceOverrides, FaceDetails, MaterialEditor, colorFilter, needsLook } from './MaterialEditor';import type { Project } from '../models';
import fixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
vi.mock('./ImagePreview',()=>({ImagePreview:({path,filter}:{path:string;filter?:string})=><span data-testid="texture-preview" data-filter={filter}>{path}</span>}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:{validateAssetPath:vi.fn(async(path:string)=>path)}}));
const id='models/test/materials/prepared/cloth2_e034ebe4.vmat';
const initial={...fixture.cases[0].expected,original_model:'models/test/hero.vmdl',dmx:'C:\\prepared.dmx',materials:{[id]:{preset:'generic',mode:'generate',textures:{},confirmed:false}}} as unknown as Project;
describe('Texture assignment visibility',()=>{
  it('accepts all resolved choices including filtered rows without choosing ambiguous images',()=>{
    const ambiguous={mode:'generate',confirmed:false,ambiguous:['color'],candidates:{color:['a.png','b.png']}};
    const project={...initial,materials:{...initial.materials,other:{mode:'generate',confirmed:false,textures:{color:'chosen.png'}},ambiguous,keep:{mode:'keep',confirmed:false}}} as Project;
    const update=vi.fn();
    const view=render(<MaterialEditor project={project} update={update} disabled={false} fail={vi.fn()} analyze={vi.fn()}/>);
    fireEvent.change(screen.getByLabelText('Search materials'),{target:{value:'cloth2'}});
    fireEvent.click(screen.getByRole('button',{name:'Accept all'}));
    const result=update.mock.calls[0][0] as Project;
    expect(result.materials[id].confirmed).toBe(true);
    expect(result.materials.other).toEqual({...project.materials.other,confirmed:true});
    expect(result.materials.keep.confirmed).toBe(true);
    expect(result.materials.ambiguous).toEqual(ambiguous);
    expect(project.materials.other.confirmed).toBe(false);
    view.rerender(<MaterialEditor project={result} update={update} disabled={false} fail={vi.fn()} analyze={vi.fn()}/>);
    expect(screen.getByRole('button',{name:'Accept all'})).toBeDisabled();
    expect(screen.getByText(/1 material still needs a choice/)).toBeVisible();
  });
  it('makes Accept all the green button while something can be accepted, and keeps Analyze materials ordinary',()=>{
    const view=render(<MaterialEditor project={initial} update={vi.fn()} disabled={false} fail={vi.fn()} analyze={vi.fn()}/>);
    expect(screen.getByRole('button',{name:'Accept all'})).toHaveClass('accept-all');
    expect(screen.getByRole('button',{name:'Accept all'})).toBeEnabled();
    expect(screen.getByRole('button',{name:'Analyze materials'})).not.toHaveClass('primary');
    expect(screen.getByRole('button',{name:'Analyze materials'})).not.toHaveClass('accept-all');
    // Nothing left to accept: disabled and ordinary.
    const accepted={...initial,materials:{[id]:{...initial.materials[id],confirmed:true}}} as Project;
    view.rerender(<MaterialEditor project={accepted} update={vi.fn()} disabled={false} fail={vi.fn()} analyze={vi.fn()}/>);
    expect(screen.getByRole('button',{name:'Accept all'})).toBeDisabled();
    expect(screen.getByRole('button',{name:'Accept all'})).not.toHaveClass('accept-all');
  });
  it('shows missing versus assigned images independently of the generic surface preset',()=>{
    const props={project:initial,update:vi.fn(),disabled:false,fail:vi.fn(),analyze:vi.fn()};
    const view=render(<MaterialEditor {...props}/>);
    // The list is always open now (it was under "Manage material assignments").
    fireEvent.click(screen.getByRole('button',{name:/cloth2_e034ebe4.vmat/}));
    expect(screen.getByText('0 texture assignments')).toBeVisible();
    expect(screen.getByText(/No base-color image selected/)).toBeVisible();
    // The channels are under an Images heading (they were in an open "Texture channels" disclosure).
    expect(screen.getByRole('heading',{name:'Images'})).toBeVisible();
    const path='C:\\Tangtang\\textures\\T_actor_tangtang_cloth_02_D.png';
    view.rerender(<MaterialEditor {...props} project={{...initial,materials:{[id]:{...initial.materials[id],textures:{color:path}}}}}/>);
    expect(screen.getByText('1 texture assignments')).toBeVisible();
    expect(screen.getByText('Base color image: T_actor_tangtang_cloth_02_D.png')).toBeVisible();
    expect(screen.getByTestId('texture-preview')).toHaveTextContent(path);
    expect(screen.getByLabelText('Surface preset')).toHaveValue('generic');
    expect(screen.getByLabelText('I have reviewed this material choice')).not.toBeChecked();
  });
  it('requires a choice for ambiguous images and keeps choosing no image explicit',async()=>{
    const files=['C:\\tex\\iris_01_D.png','C:\\tex\\iris_02_D.png'];
    const changes=vi.fn();
    function Harness(){const [project,setProject]=useState<Project>({...initial,materials:{[id]:{...initial.materials[id],ambiguous:['color'],candidates:{color:files}}}});return <MaterialEditor project={project} update={value=>{changes(value);setProject(value);}} disabled={false} fail={vi.fn()} analyze={vi.fn()}/>;}
    render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:/cloth2_e034ebe4.vmat/}));
    expect(screen.getByLabelText('I have reviewed this material choice')).toBeDisabled();
    fireEvent.change(screen.getByLabelText('Base color candidates'),{target:{value:files[1]}});
    await waitFor(()=>expect(screen.getByText('1 texture assignments')).toBeVisible());
    expect(changes.mock.calls.at(-1)![0].materials[id].textures.color).toBe(files[1]);
    expect(screen.getByLabelText('I have reviewed this material choice')).toBeEnabled();
    fireEvent.change(screen.getByLabelText('Base color candidates'),{target:{value:'__none__'}});
    await waitFor(()=>expect(screen.getByText('0 texture assignments')).toBeVisible());
    expect(changes.mock.calls.at(-1)![0].materials[id].textures).toEqual({color:''});
  });
});

describe('Hand picks and images the preparation found',()=>{
  const found='C:\\w\\prep-2\\textures\\whites_color_0092be636f5d.jpg',mine='D:\\art\\T_Sample_Prop.jpg';
  function Harness({material,changes}:{material:Project['materials'][string];changes:(p:Project)=>void}){
    const [project,setProject]=useState<Project>({...initial,materials:{[id]:material}});
    return <MaterialEditor project={project} update={value=>{changes(value);setProject(value);}} disabled={false} fail={vi.fn()} analyze={vi.fn()}/>;
  }
  it('drops the "Found by" label of a channel whenever its image is set by hand',async()=>{
    const changes=vi.fn();
    render(<Harness changes={changes} material={{mode:'generate',confirmed:false,textures:{color:found},candidates:{color:[found,mine],normal:['C:\\w\\n.png']},texture_match_sources:{color:'Blender image connection',normal:'Blender image connection'}}}/>);
    fireEvent.click(screen.getByRole('button',{name:/cloth2_e034ebe4.vmat/}));
    expect(screen.getByText('Found by: Blender image connection')).toBeVisible();
    // A listed candidate is still the user's choice.
    fireEvent.change(screen.getByLabelText('Base color candidates'),{target:{value:mine}});
    await waitFor(()=>expect(screen.queryByText(/Found by:/)).toBeNull());
    const material=changes.mock.calls.at(-1)![0].materials[id];
    expect(material.textures.color).toBe(mine);
    expect(material.texture_match_sources).toEqual({normal:'Blender image connection'});
    // "Use no image" too; the last label goes with its key.
    fireEvent.change(screen.getByLabelText('Normal candidates'),{target:{value:'__none__'}});
    await waitFor(()=>expect(changes.mock.calls.at(-1)![0].materials[id].texture_match_sources).toBeUndefined());
  });
  it('offers the image found beside a reviewed hand pick without asking for a review',async()=>{
    const changes=vi.fn();
    const material={mode:'generate',confirmed:true,textures:{color:mine},candidates:{color:[found]}};
    render(<Harness changes={changes} material={material}/>);
    expect(needsLook(material)).toBe(false);
    expect(screen.getByText('Reviewed')).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:/cloth2_e034ebe4.vmat/}));
    expect(screen.getByText(/The preparation found another image for this material: whites_color_0092be636f5d.jpg/)).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Use it'}));
    await waitFor(()=>expect(changes.mock.calls.at(-1)![0].materials[id].textures.color).toBe(found));
    expect(changes.mock.calls.at(-1)![0].materials[id].confirmed).toBe(false);
    expect(screen.queryByText(/found another image/)).toBeNull();
  });
  it('offers nothing after Analyze materials when what it found is the chosen picture, and still offers a different one',()=>{
    const copy='C:\\w\\material-analysis\\T_Sample_Prop.jpg';
    // Before Analyze materials the list held a copy of the pick (same bytes, another file).
    const before={mode:'generate',confirmed:true,textures:{color:mine},candidates:{color:[copy]}};
    function Analyzed(){
      const [project,setProject]=useState<Project>({...initial,materials:{[id]:before}});
      // The material planner's answer: copies of the chosen image are left out (planner.rs), only the pick itself may stay.
      return <MaterialEditor project={project} update={setProject} disabled={false} fail={vi.fn()} analyze={()=>setProject(p=>({...p,materials:{[id]:{...before,ambiguous:[],candidates:{color:[mine],normal:[]}}}}))}/>;
    }
    const view=render(<Analyzed/>);
    fireEvent.click(screen.getByRole('button',{name:/cloth2_e034ebe4.vmat/}));
    expect(screen.getByText(/found another image/)).toBeVisible();
    fireEvent.click(screen.getByRole('button',{name:'Analyze materials'}));
    expect(screen.queryByText(/found another image/)).toBeNull();
    expect(screen.getByText('Reviewed')).toBeVisible();
    expect(needsLook({...before,ambiguous:[],candidates:{color:[mine]}})).toBe(false);
    view.unmount();
    // Analyze while the material is not reviewed yet, then Accept all: still no offer.
    function Later(){
      const [project,setProject]=useState<Project>({...initial,materials:{[id]:{mode:'generate',confirmed:false,textures:{color:mine}}}});
      return <MaterialEditor project={project} update={setProject} disabled={false} fail={vi.fn()} analyze={()=>setProject(p=>({...p,materials:{[id]:{...p.materials[id],ambiguous:[],candidates:{color:[mine]}}}}))}/>;
    }
    const later=render(<Later/>);
    fireEvent.click(screen.getByRole('button',{name:'Analyze materials'}));
    fireEvent.click(screen.getByRole('button',{name:'Accept all'}));
    fireEvent.click(screen.getByRole('button',{name:/cloth2_e034ebe4.vmat/}));
    expect(screen.getByText('Reviewed')).toBeVisible();
    expect(screen.queryByText(/found another image/)).toBeNull();
    later.unmount();
    // Nothing left at all, or a different picture next to the pick.
    for(const [candidates,shown] of [[[],false],[[found],true]] as const){
      const material={mode:'generate',confirmed:true,textures:{color:mine},ambiguous:[],candidates:{color:[...candidates]}};
      const page=render(<Harness changes={vi.fn()} material={material}/>);
      fireEvent.click(screen.getByRole('button',{name:/cloth2_e034ebe4.vmat/}));
      expect(Boolean(screen.queryByText(/found another image/))).toBe(shown);
      expect(needsLook(material)).toBe(false);
      page.unmount();
    }
  });
});

describe('Experimental color editor',()=>{
  it('adds hue, saturation and brightness to a surface change and previews them',()=>{
    const project={...initial,materials:{[id]:{mode:'generate',confirmed:true,textures:{color:'C:\textures\cloth.png'}}},customization:{...initial.customization,enabled:true,appearance:{enabled:true,materials:[{enabled:true,target:id}]}}} as unknown as Project;
    function Harness(){const [value,setValue]=useState(project);return <AppearanceOverrides project={value} update={setValue} disabled={false} fail={vi.fn()}/>;}
    render(<Harness/>);
    fireEvent.click(screen.getByLabelText('Adjust colors 1 (experimental)'));
    fireEvent.change(screen.getByLabelText('Hue shift 1'),{target:{value:'90'}});
    fireEvent.change(screen.getByLabelText('Saturation 1'),{target:{value:'0.5'}});
    expect(screen.getByText('90°')).toBeVisible();
    expect(screen.getByTestId('texture-preview')).toHaveAttribute('data-filter','hue-rotate(90deg) saturate(0.5) brightness(1)');
    fireEvent.click(screen.getByLabelText('Adjust colors 1 (experimental)'));
    expect(screen.queryByLabelText('Hue shift 1')).toBeNull();
    expect(colorFilter({})).toBe('hue-rotate(0deg) saturate(1) brightness(1)');
  });
  it('keeps the color ranges Extras → Colors set when another value of the row changes',()=>{
    const ranges=[{enabled:true,hue:20,hue_width:15,hue_shift:120}];
    const project={...initial,materials:{[id]:{mode:'generate',confirmed:true,textures:{color:'C:\\textures\\cloth.png'}}},customization:{...initial.customization,enabled:true,appearance:{enabled:true,materials:[{enabled:true,target:id,hue:10,ranges}]}}} as unknown as Project;
    const changes=vi.fn();
    function Harness(){const [value,setValue]=useState(project);return <AppearanceOverrides project={value} update={next=>{changes(next);setValue(next);}} disabled={false} fail={vi.fn()}/>;}
    render(<Harness/>);
    expect(screen.getByText('1 color range from Extras → Colors; edit them there.')).toBeVisible();
    expect(screen.getByText(/Extras → Colors edits these colors/)).toBeVisible();
    const row=()=>(changes.mock.lastCall![0] as Project).customization.appearance!.materials![0];
    fireEvent.click(screen.getByLabelText('Apply tint 1'));
    expect(row()).toMatchObject({color:[1,1,1],hue:10,ranges});
    fireEvent.click(screen.getByLabelText('Override roughness 1'));
    fireEvent.change(screen.getByLabelText('Saturation 1'),{target:{value:'0.5'}});
    expect(row()).toMatchObject({roughness:.5,saturation:.5,ranges});
    // Switching the color editor off keeps the ranges too.
    fireEvent.click(screen.getByLabelText('Adjust colors 1 (experimental)'));
    expect(row().hue).toBeUndefined();
    expect(row().ranges).toEqual(ranges);
  });
});
describe('Face details',()=>{
  it('lists only the face layers the model has, hidden by default, and shows a kind when switched on',()=>{
    const layer=(face_layer:string)=>({mode:'generate',confirmed:true,textures:{},face_layer});
    const project={...initial,materials:{...initial.materials,'a/blush.vmat':layer('blush'),'a/blush2.vmat':layer('blush'),'a/tears.vmat':layer('tears')}} as unknown as Project;
    const update=vi.fn();
    const view=render(<FaceDetails project={project} update={update} disabled={false}/>);
    // Each kind is a Hidden / Shown switch now (it was a "Show …" tick); the group keeps the tick's name.
    const shown=(name:string)=>within(screen.getByRole('radiogroup',{name})).getByRole('radio',{name:'Shown'});
    const hidden=(name:string)=>within(screen.getByRole('radiogroup',{name})).getByRole('radio',{name:'Hidden'});
    expect(shown('Show blush (2 materials)')).not.toBeChecked();
    expect(hidden('Show blush (2 materials)')).toBeChecked();
    expect(shown('Show tears (1 material)')).not.toBeChecked();
    expect(screen.queryByText(/other hidden layers/i)).toBeNull();
    fireEvent.click(shown('Show blush (2 materials)'));
    const result=update.mock.calls[0][0] as Project;
    expect(result.customization.face_layers).toEqual({blush:true});
    expect(result.customization.enabled).toBe(true);
    view.rerender(<FaceDetails project={result} update={update} disabled={false}/>);
    expect(shown('Show blush (2 materials)')).toBeChecked();
    // Hidden writes false, as unticking did.
    fireEvent.click(hidden('Show blush (2 materials)'));
    expect((update.mock.lastCall![0] as Project).customization.face_layers).toEqual({blush:false});
    // A model without face layers shows nothing.
    view.rerender(<FaceDetails project={initial} update={update} disabled={false}/>);
    expect(screen.queryByText('Face details')).toBeNull();
  });
});
