import { useState } from 'react';
import { fireEvent,render,screen,waitFor } from '@testing-library/react';
import { describe,expect,it,vi } from 'vitest';
import { MaterialEditor } from './MaterialEditor';
import type { Project } from '../models';
import fixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
vi.mock('./ImagePreview',()=>({ImagePreview:({path}:{path:string})=><span data-testid="texture-preview">{path}</span>}));
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
