import { useState } from 'react';
import { fireEvent,render,screen,waitFor } from '@testing-library/react';
import { describe,expect,it,vi } from 'vitest';
import { AppearanceOverrides, FaceDetails, MaterialEditor, colorFilter } from './MaterialEditor';
import type { Project } from '../models';
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
  it('shows missing versus assigned images independently of the generic surface preset',()=>{
    const props={project:initial,update:vi.fn(),disabled:false,fail:vi.fn(),analyze:vi.fn()};
    const view=render(<MaterialEditor {...props}/>);
    fireEvent.click(screen.getByText('Manage material assignments'));
    fireEvent.click(screen.getByRole('button',{name:/cloth2_e034ebe4.vmat/}));
    expect(screen.getByText('0 texture assignments')).toBeVisible();
    expect(screen.getByText(/No base-color image selected/)).toBeVisible();
    expect(screen.getByText('Texture channels',{selector:'summary'}).closest('details')).toHaveAttribute('open');
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
    render(<Harness/>);fireEvent.click(screen.getByText('Manage material assignments'));fireEvent.click(screen.getByRole('button',{name:/cloth2_e034ebe4.vmat/}));
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
});
describe('Face details',()=>{
  it('lists only the face layers the model has, hidden by default, and shows a kind when switched on',()=>{
    const layer=(face_layer:string)=>({mode:'generate',confirmed:true,textures:{},face_layer});
    const project={...initial,materials:{...initial.materials,'a/blush.vmat':layer('blush'),'a/blush2.vmat':layer('blush'),'a/tears.vmat':layer('tears')}} as unknown as Project;
    const update=vi.fn();
    const view=render(<FaceDetails project={project} update={update} disabled={false}/>);
    expect(screen.getByRole('checkbox',{name:'Show blush (2 materials)'})).not.toBeChecked();
    expect(screen.getByRole('checkbox',{name:'Show tears (1 material)'})).not.toBeChecked();
    expect(screen.queryByText(/other hidden layers/)).toBeNull();
    fireEvent.click(screen.getByRole('checkbox',{name:'Show blush (2 materials)'}));
    const result=update.mock.calls[0][0] as Project;
    expect(result.customization.face_layers).toEqual({blush:true});
    expect(result.customization.enabled).toBe(true);
    view.rerender(<FaceDetails project={result} update={update} disabled={false}/>);
    expect(screen.getByRole('checkbox',{name:'Show blush (2 materials)'})).toBeChecked();
    // A model without face layers shows nothing.
    view.rerender(<FaceDetails project={initial} update={update} disabled={false}/>);
    expect(screen.queryByText('Face details')).toBeNull();
  });
});
