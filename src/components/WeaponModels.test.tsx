import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { WeaponModels, weaponLike } from './WeaponModels';
import type { Project } from '../models';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

const native=vi.hoisted(()=>({discoverWeaponModels:vi.fn(),newProject:vi.fn(),selectHero:vi.fn(),saveProject:vi.fn(),save:vi.fn(),validateAssetPath:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn(),save:native.save}));
const parent={...projectFixture.cases[0].expected,name:'jinx',original_model:'models/heroes_wip/test/test.vmdl',preparation_source:'C:\\jinx.blend',vpk_number:21,
  preparation_options:{weapon:{enabled:true,action:'remove',objects:['Pistol']},scene:{exclude:['Floor']}}} as unknown as Project;
beforeEach(()=>vi.clearAllMocks());

describe('Separate weapon models',()=>{
  it('recognizes weapon model paths',()=>{
    expect(weaponLike('models/weapons/test/test_pistol.vmdl')).toBe(true);
    expect(weaponLike('models/heroes_wip/test/test_gun.vmdl')).toBe(true);
    expect(weaponLike('models/heroes_wip/test/test_wip.vmdl')).toBe(false);
  });
  it('finds the hero weapon model and creates a linked gun-only project from the same source',async()=>{
    native.discoverWeaponModels.mockResolvedValue([{path:'models/heroes_wip/test/test_gun.vmdl',target:'models/heroes_wip/test/test_gun.vmdl',label:'Weapon model: test_gun.vmdl',hero:'Test',supported:true,requires_model_project:true,kind:'weapon'}]);
    native.newProject.mockResolvedValue({path:null,project:{...parent,id:'child',preparation_options:{},model_input_mode:'prepared'}});
    native.selectHero.mockImplementation(async(project:Project,path:string)=>({...project,original_model:path,hero_name:'Test'}));
    native.save.mockResolvedValue('C:\\mods\\jinx_weapon.dmb.json');
    native.saveProject.mockImplementation(async(path:string,project:Project)=>({path,project}));
    const change=vi.fn(),fail=vi.fn();
    render(<WeaponModels project={parent} variants={[]} change={change} disabled={false} fail={fail}/>);
    fireEvent.click(screen.getByText('Weapon model (0)'));
    fireEvent.click(screen.getByRole('button',{name:"Find this hero's weapon models"}));
    expect(await screen.findByRole('option',{name:/test_gun.vmdl/})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Create weapon project'}));
    await vi.waitFor(()=>expect(change).toHaveBeenCalled());
    expect(fail).not.toHaveBeenCalled();
    const child=native.saveProject.mock.calls[0][1] as Project;
    expect(child).toMatchObject({model_input_mode:'weapon',original_model:'models/heroes_wip/test/test_gun.vmdl',preparation_source:'C:\\jinx.blend',vpk_number:21,install:false,
      preparation_options:{weapon:{enabled:true,action:'replace',objects:['Pistol']},scene:{exclude:['Floor']}}});
    expect(change).toHaveBeenCalledWith([{enabled:true,model:'models/heroes_wip/test/test_gun.vmdl',project:'C:\\mods\\jinx_weapon.dmb.json'}]);
  });
  function mockCreation(){
    native.newProject.mockResolvedValue({path:null,project:{...parent,id:'child',preparation_options:{},model_input_mode:'prepared'}});
    native.selectHero.mockImplementation(async(project:Project,path:string)=>({...project,original_model:path,hero_name:'Test'}));
    native.save.mockResolvedValue('C:\\mods\\jinx_weapon.dmb.json');
    native.saveProject.mockImplementation(async(path:string,project:Project)=>({path,project}));
  }
  it('uses a separate gun file as the weapon project source when chosen',async()=>{
    mockCreation();
    native.validateAssetPath.mockImplementation(async(path:string)=>path);
    const change=vi.fn(),fail=vi.fn();
    render(<WeaponModels project={parent} variants={[]} change={change} disabled={false} fail={fail}/>);
    fireEvent.click(screen.getByText('Weapon model (0)'));
    expect(screen.getByLabelText('The character file (jinx.blend)')).toBeChecked();
    expect(screen.queryByLabelText('Gun file')).not.toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('models/…/weapon.vmdl'),{target:{value:'models/heroes_wip/test/test_gun.vmdl'}});
    fireEvent.click(screen.getByLabelText('A separate gun file (Blender, FBX or glTF)'));
    // A gun file must be chosen before the project can be created.
    expect(screen.getByRole('button',{name:'Create weapon project'})).toBeDisabled();
    fireEvent.change(screen.getByLabelText('Gun file'),{target:{value:'C:\\guns\\pistol.fbx'}});
    fireEvent.blur(screen.getByLabelText('Gun file'));
    await vi.waitFor(()=>expect(screen.getByRole('button',{name:'Create weapon project'})).toBeEnabled());
    expect(native.validateAssetPath).toHaveBeenCalledWith('C:\\guns\\pistol.fbx','weapon');
    fireEvent.click(screen.getByRole('button',{name:'Create weapon project'}));
    await vi.waitFor(()=>expect(change).toHaveBeenCalled());
    expect(fail).not.toHaveBeenCalled();
    const child=native.saveProject.mock.calls[0][1] as Project;
    // The character's gun and scene choices name objects of another file.
    expect(child).toMatchObject({model_input_mode:'weapon',original_model:'models/heroes_wip/test/test_gun.vmdl',preparation_source:'C:\\guns\\pistol.fbx',preparation_options:{}});
  });
  it("starts from the Weapon tab's gun file",async()=>{
    mockCreation();
    const withGunFile={...parent,preparation_options:{...parent.preparation_options,weapon_source:'C:\\guns\\pistol.blend'}} as unknown as Project;
    const change=vi.fn(),fail=vi.fn();
    render(<WeaponModels project={withGunFile} variants={[]} change={change} disabled={false} fail={fail}/>);
    fireEvent.click(screen.getByText('Weapon model (0)'));
    expect(screen.getByLabelText('A separate gun file (Blender, FBX or glTF)')).toBeChecked();
    fireEvent.change(screen.getByPlaceholderText('models/…/weapon.vmdl'),{target:{value:'models/heroes_wip/test/test_gun.vmdl'}});
    fireEvent.click(screen.getByRole('button',{name:'Create weapon project'}));
    await vi.waitFor(()=>expect(change).toHaveBeenCalled());
    expect((native.saveProject.mock.calls[0][1] as Project).preparation_source).toBe('C:\\guns\\pistol.blend');
  });
});
