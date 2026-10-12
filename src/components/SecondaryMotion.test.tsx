import { useState } from 'react';
import { fireEvent,render,screen } from '@testing-library/react';
import { describe,it,expect,vi } from 'vitest';
import { PhysicsPage } from './CustomizationPages';
import { SecondaryMotion } from './SecondaryMotion';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';
import { inspected } from '../build/testFixtures';
vi.mock('../api',()=>({api:{validateAssetPath:vi.fn()},basename:(s:string)=>s}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const original=structuredClone(reference.cases[0].expected) as unknown as Project;
function Harness(){const [project,update]=useState<Project>({...original,preparation_options:{preserve_physics_bones:true,physics_roots:['hair','tail']},customization:{enabled:true,physics:{enabled:true,source:'saved.vmdl_c'},audio:[{source:'sound.vsnd_c'}]}});return <><PhysicsPage project={project} update={update} disabled={false} fail={vi.fn()} summary={null}/><output data-testid="saved">{JSON.stringify(project)}</output></>;}
const saved=()=>JSON.parse(screen.getByTestId('saved').textContent!) as Project;
describe('Generated secondary motion',()=>{
 it('shows long-skirt support only for a current analysis and exposes its reviewed type',()=>{
   const project={...original,preparation_options:{preserve_physics_bones:true,physics_roots:['panel']},customization:{enabled:true,secondary_motion:{enabled:true}}};
   const report={...inspected,secondary_motion:{preset:'automatic',source_sha256:'source',proxy_sha256:'proxy',proxy_positions:18,groups:[],long_skirt:{policy:'mesh-supported-long-skirt-v1',groups:[{root:'panel'}],links:[],stretch_tethers:true,body_collisions_authored:false}}};
   const props={project,update:vi.fn(),disabled:false,report};
   const {rerender}=render(<SecondaryMotion {...props} fresh/>);
   expect(screen.getByText(/Long-skirt support: 1 group with mesh-verified panel connections/)).toBeVisible();
   fireEvent.click(screen.getByText('Adjust moving parts',{selector:'summary > span'}));
   fireEvent.change(screen.getByLabelText('Part type for panel'),{target:{value:'long_skirt'}});
   expect(props.update).toHaveBeenCalledWith({part_kinds:{panel:'long_skirt'}});
   rerender(<SecondaryMotion {...props} fresh={false}/>);
   expect(screen.queryByText(/Long-skirt support:/)).not.toBeInTheDocument();
 });
 it('reports shared hair movement and clears hair overrides without changing other parts',()=>{
   const project={...original,preparation_options:{preserve_physics_bones:true,physics_roots:['bangs','ponytail','tail']},customization:{enabled:true,secondary_motion:{enabled:true,profiles:{ponytail:'soft' as const,tail:'stiff' as const}}}};
   const report={...inspected,secondary_motion:{preset:'automatic',source_sha256:'source',proxy_sha256:'proxy',proxy_positions:18,groups:[],hair_motion:{policy:'weighted-strand-hair-motion-v1',shared_profile_groups:['bangs'],custom_profile_groups:['ponytail'],review_groups:[]}}};
   const update=vi.fn();
   const props={project,update,disabled:false,report};
   const {rerender}=render(<SecondaryMotion {...props} fresh/>);
   expect(screen.getByText(/Automatic hair movement: 1 group/)).toBeVisible();
   expect(screen.getByText(/Custom movement overrides: 1 hair group/)).toBeVisible();
   fireEvent.click(screen.getByRole('button',{name:'Use automatic hair movement'}));
   expect(update).toHaveBeenCalledWith({profiles:{tail:'stiff'}});
   rerender(<SecondaryMotion {...props} fresh={false}/>);
   expect(screen.queryByText(/Automatic hair movement:/)).not.toBeInTheDocument();
   expect(screen.queryByRole('button',{name:'Use automatic hair movement'})).not.toBeInTheDocument();
 });
 it('shows long-hair softening only when the current analysis applied it',()=>{
   const project={...original,preparation_options:{preserve_physics_bones:true,physics_roots:['ponytail']},customization:{enabled:true,secondary_motion:{enabled:true}}};
   const report={...inspected,secondary_motion:{preset:'automatic',source_sha256:'source',proxy_sha256:'proxy',proxy_positions:18,groups:[],hair_motion:{policy:'length-adaptive-hair-motion-v2',shared_profile_groups:['ponytail'],custom_profile_groups:[],review_groups:[],length_adjusted_groups:[{root:'ponytail',strand_length_inches:35,short_hair_limit_inches:14,goal_strength_multiplier:.4,scale_reference:'head_to_pelvis'}]}}};
   const props={project,update:vi.fn(),disabled:false,report};
   const {rerender}=render(<SecondaryMotion {...props} fresh/>);
   expect(screen.getByText('Long-hair softening applied to 1 group.')).toBeVisible();
   rerender(<SecondaryMotion {...props} fresh={false}/>);
   expect(screen.queryByText(/Long-hair softening applied/)).not.toBeInTheDocument();
 });
 it('shows jump protection only for a current protected analysis',()=>{
   const project={...original,preparation_options:{preserve_physics_bones:true,physics_roots:['ponytail']},customization:{enabled:true,secondary_motion:{enabled:true}}};
   const report={...inspected,secondary_motion:{preset:'automatic',source_sha256:'source',proxy_sha256:'proxy',proxy_positions:18,groups:[],hair_motion:{policy:'bounded-length-adaptive-hair-motion-v3',shared_profile_groups:['ponytail'],custom_profile_groups:[],review_groups:[],jump_protected_groups:[{root:'ponytail',anchor:'head',stretch_tethers:true}]}}};
   const props={project,update:vi.fn(),disabled:false,report};
   const {rerender}=render(<SecondaryMotion {...props} fresh/>);
   expect(screen.getByText('Long-hair jump protection: stretch resistance and height limits for 1 group.')).toBeVisible();
   rerender(<SecondaryMotion {...props} fresh={false}/>);
   expect(screen.queryByText(/Long-hair jump protection:/)).not.toBeInTheDocument();
 });
 it('shows checked twist coverage and review reasons without treating stale analysis as current',()=>{
   const project={...original,preparation_options:{preserve_physics_bones:true,physics_roots:['hair','tail']},customization:{enabled:true,secondary_motion:{enabled:true}}};
   const report={...inspected,secondary_motion:{preset:'automatic',source_sha256:'source',proxy_sha256:'proxy',proxy_positions:18,groups:[{source_root:'hair',root:'hair',anchor:'head',bones:['hair'],weighted_bones:['hair']}],parts:[{root:'hair',kind:'hair',reason:'Consistent hair names',roll_locked:true,review_recommended:false},{root:'tail',kind:'tail',reason:'Branching rig needs review',roll_locked:false,review_recommended:true}]}};
   const props={project,update:vi.fn(),disabled:false,fail:vi.fn(),summary:null,report};
   const {rerender}=render(<PhysicsPage {...props} fresh/>);
   expect(screen.getByText('Twist protection planned for 1 group · checked during Build.')).toBeVisible();
   fireEvent.click(screen.getByText('Adjust moving parts',{selector:'summary > span'}));
   expect(screen.getByText(/Branching rig needs review/)).toBeVisible();
   rerender(<PhysicsPage {...props} fresh={false}/>);
   expect(screen.queryByText(/Twist protection planned for/)).not.toBeInTheDocument();
   expect(screen.queryByText(/Branching rig needs review/)).not.toBeInTheDocument();
 });
 it('keeps selected physics groups pending until mesh mapping is complete',()=>{
   const project={...original,preparation_options:{preserve_physics_bones:true,physics_roots:['hair','tail']},customization:{enabled:true,secondary_motion:{enabled:true}}};
   render(<PhysicsPage project={project} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null} fresh report={{...inspected,needs_review:true,unresolved_meshes:['body'],secondary_motion:null}}/>);
   expect(screen.getByText('2 groups selected · complete Mesh mapping, then Analyze again')).toBeVisible();
   expect(screen.queryByText(/No prepared physics groups/)).not.toBeInTheDocument();
   expect(screen.queryByText(/groups checked/)).not.toBeInTheDocument();
 });
 it('saves the reference preset and restores optional group choices when returning to automatic',()=>{render(<Harness/>);fireEvent.click(screen.getByRole('radio',{name:/Automatic secondary motion/}));fireEvent.click(screen.getByText('Adjust moving parts',{selector:'summary > span'}));fireEvent.change(screen.getByLabelText('Movement for tail'),{target:{value:'soft'}});fireEvent.change(screen.getByLabelText('Secondary motion preset'),{target:{value:'tangtang_reference'}});expect(saved().customization.secondary_motion).toMatchObject({enabled:true,preset:'tangtang_reference',profiles:{tail:'soft'}});expect(screen.getByText(/flexible along their length, with anti-twist/)).toBeVisible();expect(screen.queryByLabelText('Movement for tail')).not.toBeInTheDocument();expect(saved().customization.audio).toEqual([{source:'sound.vsnd_c'}]);fireEvent.change(screen.getByLabelText('Secondary motion preset'),{target:{value:'automatic'}});expect(screen.getByLabelText('Movement for tail')).toHaveValue('soft');});
 it('switches modes without erasing references, tuning or other changes',()=>{render(<Harness/>);fireEvent.click(screen.getByRole('radio',{name:/Automatic secondary motion/}));expect(saved().customization.physics).toMatchObject({enabled:false,source:'saved.vmdl_c'});expect(saved().customization.audio).toEqual([{source:'sound.vsnd_c'}]);fireEvent.click(screen.getByText('Adjust moving parts',{selector:'summary > span'}));fireEvent.change(screen.getByLabelText('Movement for tail'),{target:{value:'soft'}});fireEvent.change(screen.getByLabelText('Secondary motion preset'),{target:{value:'tangtang_r2'}});expect(screen.getByText(/exact R2 setup/)).toBeVisible();fireEvent.click(screen.getByRole('radio',{name:/Preserve original physics/}));expect(saved().customization.secondary_motion).toMatchObject({enabled:false,preset:'tangtang_r2',profiles:{tail:'soft'}});fireEvent.click(screen.getByRole('radio',{name:/Automatic secondary motion/}));fireEvent.change(screen.getByLabelText('Secondary motion preset'),{target:{value:'automatic'}});expect(screen.getByLabelText('Movement for tail')).toHaveValue('soft');});
 it('explains missing preparation rather than promising a static DMX will move',()=>{const project={...original,preparation_options:{},customization:{enabled:true,secondary_motion:{enabled:true}}};render(<PhysicsPage project={project} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null}/>);expect(screen.getByRole('status')).toHaveTextContent('No prepared physics groups');});
});
describe('Breast motion on the Physics tab',()=>{
 const base={...original,preparation_options:{preserve_physics_bones:true,physics_roots:['胸上2.L','胸上2.R','hair']},customization:{enabled:true,secondary_motion:{enabled:true}}} as unknown as Project;
 it('turns existing breast groups on and off and sets their amount together, without a conversion',()=>{
   const update=vi.fn(),preparation=vi.fn();
   const {rerender}=render(<SecondaryMotion project={base} update={update} preparation={preparation} disabled={false} report={null} fresh={false}/>);
   const box=screen.getByLabelText('Breast motion');
   expect(box).toBeChecked();
   expect(screen.getByText(/On for 胸上2.L, 胸上2.R. Applies at the next Analyze and Build; no conversion needed/)).toBeVisible();
   fireEvent.change(screen.getByLabelText('Breast movement'),{target:{value:'soft'}});
   expect(update).toHaveBeenLastCalledWith({profiles:{'胸上2.L':'soft','胸上2.R':'soft'}});
   fireEvent.click(box);
   expect(update).toHaveBeenLastCalledWith({part_kinds:{'胸上2.L':'rigid','胸上2.R':'rigid'}});
   expect(preparation).not.toHaveBeenCalled();
   const off={...base,customization:{...base.customization,secondary_motion:{enabled:true,part_kinds:{'胸上2.L':'rigid' as const,'胸上2.R':'rigid' as const}}}} as Project;
   rerender(<SecondaryMotion project={off} update={update} preparation={preparation} disabled={false} report={null} fresh={false}/>);
   expect(screen.getByLabelText('Breast motion')).not.toBeChecked();
   expect(screen.getByText(/follow the chest without bouncing/)).toBeVisible();
   fireEvent.click(screen.getByLabelText('Breast motion'));
   expect(update).toHaveBeenLastCalledWith({part_kinds:{'胸上2.L':'breast','胸上2.R':'breast'}});
   expect(preparation).toHaveBeenCalledWith({breast_motion:true});
 });
 it('asks Model preparation for breast bones when the prepared model has none',()=>{
   const project={...base,preparation_options:{preserve_physics_bones:true,physics_roots:['hair']}} as unknown as Project;
   const preparation=vi.fn(),openPreparation=vi.fn();
   const {rerender}=render(<SecondaryMotion project={project} update={vi.fn()} preparation={preparation} openPreparation={openPreparation} disabled={false} report={null} fresh={false}/>);
   fireEvent.click(screen.getByLabelText('Breast motion'));
   expect(preparation).toHaveBeenCalledWith({breast_motion:true});
   const wanted={...project,preparation_options:{...project.preparation_options,breast_motion:true}} as unknown as Project;
   rerender(<SecondaryMotion project={wanted} update={vi.fn()} preparation={preparation} openPreparation={openPreparation} disabled={false} report={null} fresh={false}/>);
   expect(screen.getByText(/no breast bones that can move yet/)).toBeVisible();
   fireEvent.click(screen.getByRole('button',{name:'Open Model preparation'}));
   expect(openPreparation).toHaveBeenCalled();
 });
});
