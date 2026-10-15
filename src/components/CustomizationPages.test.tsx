import { fireEvent,render,screen,waitFor } from '@testing-library/react';
import { beforeEach,describe,expect,it,vi } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import { PhysicsPage,changeCustomization } from './CustomizationPages';
import { heroLabel } from './HeroPicker';
const api=vi.hoisted(()=>({includedPhysics:vi.fn(),validateAssetPath:vi.fn()}));
vi.mock('../api',()=>({api}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
beforeEach(()=>vi.clearAllMocks());
describe('Persisted customization choices',()=>{
  it('preserves unknown and unedited options when activating one section',()=>{const project=structuredClone(reference.cases[0].expected) as unknown as Project;project.customization={future:{nested:7},physics:{source:'reference.vmdl_c',enabled:false},audio:[{source:'clip.wav',target:'sounds/test.vsnd_c',allow_shared:true}]};const result=changeCustomization(project,{physics:{...project.customization.physics,enabled:true}});expect(result.customization.audio).toEqual(project.customization.audio);expect(result.customization.future).toEqual({nested:7});expect(result.customization.physics?.source).toBe('reference.vmdl_c');expect(project.customization.physics?.enabled).toBe(false);});
  it('uses the hash-checked backend result for included cloth without inventing a path',async()=>{const project=structuredClone(reference.cases[1].expected) as unknown as Project;const update=vi.fn(),fail=vi.fn();const physics={source:'C:\\Bundled\\accepted.vmdl_c',hero_path:project.original_model,enabled:true};api.includedPhysics.mockResolvedValue(physics);render(<PhysicsPage project={project} update={update} disabled={false} fail={fail} summary={null}/>);fireEvent.click(screen.getByRole('button',{name:'Use included Vindicta cloth setup'}));await waitFor(()=>expect(update).toHaveBeenCalledOnce());expect(api.includedPhysics).toHaveBeenCalledWith(project.original_model);expect(update.mock.calls[0][0].customization.physics).toEqual(physics);expect(fail).not.toHaveBeenCalled();});
  it('hides the included Vindicta cloth choice in a build without the private reference',()=>{const project=structuredClone(reference.cases[1].expected) as unknown as Project;render(<PhysicsPage project={project} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null} vindictaReference={false}/>);expect(screen.queryByRole('button',{name:'Use included Vindicta cloth setup'})).toBeNull();expect(api.includedPhysics).not.toHaveBeenCalled();});
  it('keeps discovered model identity separate from its friendly label',()=>{const hero={name:'hornet',path:'models/heroes_staging/hornet_v3/hornet.vmdl_c',likely_hero:true};expect(heroLabel(hero)).toBe('Vindicta');expect(hero.path).toBe('models/heroes_staging/hornet_v3/hornet.vmdl_c');});
});
