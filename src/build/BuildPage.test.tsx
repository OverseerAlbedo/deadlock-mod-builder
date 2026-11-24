import { useState } from 'react';
import { fireEvent,render,screen,within } from '@testing-library/react';
import { describe,expect,it,vi } from 'vitest';
import { BuildPage, SLOT_RESCAN_MS } from './BuildPage';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import { inspected, reviewedProject } from './testFixtures';
import type { Project } from '../models';
const project={...reference.cases[0].expected,original_model:'models/heroes/test.vmdl',dmx:'',customization:{enabled:true,model_enabled:false}} as unknown as Project;
const base={project,update:vi.fn(),report:null,fresh:false,prepared:null,original:null,busy:false,inspect:vi.fn(),prepare:vi.fn(),validateOriginal:vi.fn(),build:vi.fn(),result:null,install:vi.fn(),installed:null};
describe('Native build results',()=>{
 it('supports assets-only analysis and build without inventing model input or compile readiness',()=>{
  render(<BuildPage {...base} report={{model_enabled:false,icons:[],animations:[],effects:[],audio:[],clothing:null,native_compile_required:true,warnings:[],native_validated:false,in_game_validated:false}} fresh/>);
  expect(screen.getByRole('button',{name:'Re-analyze project'})).toBeEnabled();expect(screen.getByText('Extra assets inspected · compilation still required')).toBeInTheDocument();expect(screen.getByRole('button',{name:'Prepare source files'})).toBeDisabled();
  // A build always replaces this project's own earlier output; another mod's installed VPK only when confirmed.
  fireEvent.click(screen.getByRole('button',{name:'Build mod'}));expect(base.build).toHaveBeenCalledWith(true,false);
 });
 it('counts the review notes in the analysis details, one note in the singular',()=>{
  const report=(warnings:string[]):Parameters<typeof BuildPage>[0]['report']=>({model_enabled:false,icons:[],animations:[],effects:[],audio:[],clothing:null,native_compile_required:true,warnings,native_validated:false,in_game_validated:false});
  const view=render(<BuildPage {...base} report={report(['A note'])} fresh/>);
  expect(screen.getByText('1 review note · full inspection available')).toBeInTheDocument();
  view.rerender(<BuildPage {...base} report={report(['A note','Another note'])} fresh/>);
  expect(screen.getByText('2 review notes · full inspection available')).toBeInTheDocument();
  view.rerender(<BuildPage {...base} report={report([])} fresh/>);
  expect(screen.getByText('0 review notes · full inspection available')).toBeInTheDocument();
 });
 it('counts effect edits in the extras summary',()=>{
  const edited={target:'particles/a.vpcf_c',edit:{hue:20,saturation:1,brightness:1,excluded:[],shared:[],colors:[],size:1,glow:1,opacity:1},parts:[]};
  const view=render(<BuildPage {...base} report={{model_enabled:false,icons:[],animations:[],effects:[],audio:[],effect_edits:[edited,{...edited,target:'particles/b.vpcf_c'}],clothing:null,native_compile_required:true,warnings:[],native_validated:false,in_game_validated:false}} fresh/>);
  expect(screen.getByText(/0 effects · 2 effect edits · 0 audio replacements/)).toBeInTheDocument();
  view.rerender(<BuildPage {...base} report={{model_enabled:false,icons:[],animations:[],effects:[],audio:[],clothing:null,native_compile_required:true,warnings:[],native_validated:false,in_game_validated:false}} fresh/>);
  expect(screen.getByText(/0 effects · 0 audio replacements/)).toBeInTheDocument();
 });
 it('offers to build only the extras when no replacement model is chosen',()=>{
  const update=vi.fn();
  const noModel={...project,customization:{enabled:true,icons:[{target:'panorama/images/heroes/test_card.vtex_c',source:'C:\\Art\\card.png'}]}} as unknown as Project;
  render(<BuildPage {...base} project={noModel} update={update}/>);
  const item=screen.getByText('Add a replacement model.').closest('li')!;
  expect(within(item).getByRole('button',{name:'Add model'})).toBeInTheDocument();
  fireEvent.click(within(item).getByRole('button',{name:'Build the extras without a replacement model'}));
  const changed=update.mock.calls[0][0] as Project;
  expect(changed.customization.model_enabled).toBe(false);
  expect(changed.customization.icons).toEqual(noModel.customization.icons);
 });
 it('lists only checks that fit an extras-only build under Check in game',()=>{
  const extrasOnly={...project,customization:{enabled:true,model_enabled:false,icons:[{target:'panorama/images/heroes/test_card.vtex_c',source:'C:\\Art\\card.png'}]}} as unknown as Project;
  const built={prepared:null,compiled:true,vpk:'C:\\Out\\pak09_dir.vpk',installed:null,manifest:null};
  const view=render(<BuildPage {...base} project={extrasOnly} result={built}/>);
  const check=()=>document.querySelector('.check-in-game')!;
  expect(check()).toHaveTextContent('What you replaced (1 icon) in the hero select screen and in a match');
  expect(check()).not.toHaveTextContent('The gun in the hand');
  expect(check()).not.toHaveTextContent('Hair, cloth');
  view.rerender(<BuildPage {...base} project={reviewedProject} result={built}/>);
  expect(check()).toHaveTextContent('The gun in the hand, and the fingers around it');
 });
 it('shows exactly what the native build completed without calling preparation a compiled package',()=>{
  render(<BuildPage {...base} result={{prepared:'C:\\workspace\\content',compiled:false,vpk:null,installed:null,manifest:null}}/>);
  expect(screen.getByText('Not performed')).toBeInTheDocument();expect(screen.getByText('Not packaged')).toBeInTheDocument();expect(screen.getByText('Not installed')).toBeInTheDocument();expect(screen.queryByRole('button',{name:'Validate and install this VPK'})).not.toBeInTheDocument();
 });
 it('prepares only a current reviewed model and forwards the offline workspace choice',()=>{
  const prepare=vi.fn();const props={...base,project:reviewedProject,report:inspected,fresh:true,prepare};
  const view=render(<BuildPage {...props}/>);
  fireEvent.click(screen.getByText('Developer / compiler tools',{selector:'summary > span'}));
  fireEvent.click(screen.getByLabelText('Prepare offline source workspace'));
  fireEvent.click(screen.getByRole('button',{name:'Prepare source files'}));
  expect(prepare).toHaveBeenCalledWith(true);
  view.rerender(<BuildPage {...props} fresh={false}/>);
  expect(screen.getByRole('button',{name:'Prepare source files'})).toBeDisabled();
 });

});
/** Build & install as App holds it: the analysis keeps the project it saw; a project
 * that changed since passes that project as `previous`. */
function Workflow({initial,first,next=inspected,build=vi.fn(),inspect=vi.fn(),built=false}:{initial:Project;first:Parameters<typeof BuildPage>[0]['report'];next?:Parameters<typeof BuildPage>[0]['report'];build?:()=>void;inspect?:()=>void;built?:boolean}){
 const [project,setProject]=useState(initial);
 const [analysis,setAnalysis]=useState({report:first,project:JSON.stringify(initial)});
 const fresh=analysis.project===JSON.stringify(project);
 const previous=fresh?null:{analyzed:JSON.parse(analysis.project) as Project,verified:built};
 return <>
  {/* Appearance → Materials, as the material editor writes a confirmed choice. */}
  <button onClick={()=>setProject({...project,materials:Object.fromEntries(Object.entries(project.materials).map(([name,material])=>[name,{...material,confirmed:true}]))})}>Confirm materials on Appearance</button>
  <BuildPage {...base} project={project} update={setProject} report={analysis.report} fresh={fresh} previous={previous} build={build} inspect={()=>{inspect();setAnalysis({report:next,project:JSON.stringify(project)});}}/>
 </>;
}
const unreviewed={...reviewedProject,review_lods:false,materials:{'body.vmat':{mode:'generate',confirmed:false}}} as unknown as Project;
const toReview={...inspected,needs_review:true,materials:unreviewed.materials};
const item=(text:RegExp)=>screen.getByText(text).closest('li')!;
describe('The list of the last analysis',()=>{
 it('ticks off as the project changes, and Build runs without analyzing again',()=>{
  const build=vi.fn(),inspect=vi.fn();
  render(<Workflow initial={unreviewed} first={toReview} build={build} inspect={inspect}/>);
  // Right after Analyze: both items open, Build disabled.
  expect(screen.getByRole('heading',{name:'2 issues blocking build'})).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
  // Tick the LOD box: the LOD item is done, the material item stays, no new analysis.
  fireEvent.click(screen.getByLabelText('I have reviewed the LOD appearance and performance tradeoffs'));
  expect(screen.getByRole('heading',{name:'1 of 2 left on the list from the last analysis'})).toBeInTheDocument();
  expect(item(/detail-level strategy/)).toHaveClass('review-item-done');
  expect(within(item(/detail-level strategy/)).getByText('Done')).toBeInTheDocument();
  expect(within(item(/detail-level strategy/)).queryByRole('button')).not.toBeInTheDocument();
  expect(item(/material assignment need review/)).toHaveClass('review-item-open');
  expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
  expect(screen.getByText('Last analysis · project changed since')).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Re-analyze project'})).toHaveTextContent('Analyze again');
  // Confirm the material (as the material editor writes it): everything is handled.
  fireEvent.click(screen.getByRole('button',{name:'Confirm materials on Appearance'}));
  expect(screen.getByRole('heading',{name:'Everything on the list from the last analysis is handled'})).toBeInTheDocument();
  expect(item(/material assignment need review/)).toHaveClass('review-item-done');
  expect(screen.getByText(/Everything on the list is handled\. Build checks the project again before it starts; Analyze again is optional\./)).toBeInTheDocument();
  expect(screen.getByText(/The project changed since the last analysis: Build checks it again before it starts/)).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Analyze project'})).not.toHaveClass('primary');
  // Build and Ctrl+B run straight away: one Analyze in total.
  expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled();
  fireEvent.click(screen.getByRole('button',{name:'Build mod'}));
  window.dispatchEvent(new Event('dmb-build-shortcut'));
  expect(build).toHaveBeenCalledTimes(2);
  expect(inspect).not.toHaveBeenCalled();
  // Analyze again (optional) with a clean report: a current analysis, ready to build.
  fireEvent.click(screen.getByRole('button',{name:'Analyze project'}));
  expect(inspect).toHaveBeenCalledTimes(1);
  expect(screen.getByText('Current analysis')).toBeInTheDocument();
  expect(screen.queryByRole('heading',{name:/left on the list|blocking build/})).not.toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled();
 });
 it('keeps an open item blocking Build and Ctrl+B, and an item waiting for the next check does not',()=>{
  const build=vi.fn();
  const view=render(<Workflow initial={{...unreviewed,review_lods:true,materials:reviewedProject.materials}} first={{...toReview,materials:reviewedProject.materials,unresolved_meshes:['face']}} build={build}/>);
  expect(screen.getByRole('heading',{name:'1 issue blocking build'})).toBeInTheDocument();
  // An unrelated edit keeps the mapping item open: Build and Ctrl+B stay off.
  fireEvent.click(screen.getByLabelText('Install after successful validation and packaging'));
  expect(screen.getByRole('heading',{name:'1 of 1 left on the list from the last analysis'})).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
  window.dispatchEvent(new Event('dmb-build-shortcut'));
  expect(build).not.toHaveBeenCalled();
  view.unmount();
  // Changed mappings: only the next check can tell, so the item waits and Build may run (it checks first).
  const mapped=vi.fn();
  const changed={...unreviewed,review_lods:true,materials:reviewedProject.materials,meshes:{body:'body',face:'face'}} as unknown as Project;
  render(<BuildPage {...base} project={changed} report={{...toReview,materials:reviewedProject.materials,unresolved_meshes:['face']}} fresh={false} previous={{analyzed:{...changed,meshes:{body:'body'}} as unknown as Project}} build={mapped}/>);
  expect(item(/model mapping need a choice/)).toHaveClass('review-item-changed');
  expect(within(item(/model mapping need a choice/)).getByText('Changed · checked again by Build or Analyze')).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled();
  window.dispatchEvent(new Event('dmb-build-shortcut'));
  expect(mapped).toHaveBeenCalledTimes(1);
 });
 it('still needs Analyze first without an analysis or after choosing another model',()=>{
  const view=render(<BuildPage {...base} project={reviewedProject} report={null} fresh={false}/>);
  expect(screen.getByRole('heading',{name:'1 issue blocking build'})).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
  expect(screen.queryByRole('button',{name:'Re-analyze project'})).not.toBeInTheDocument();
  view.rerender(<BuildPage {...base} project={{...unreviewed,review_lods:true,dmx:'C:\\Models\\Other.dmx'}} report={toReview} fresh={false} previous={{analyzed:unreviewed}}/>);
  expect(screen.getByText('Project changed. Analyze the current inputs again.')).toBeInTheDocument();
  expect(screen.queryByText(/detail-level strategy/)).not.toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
  expect(screen.getByText('Analysis required')).toBeInTheDocument();
 });
 it('disables Build after the build itself refused, with Analyze as the primary action',()=>{
  const build=vi.fn();
  const handled={...unreviewed,review_lods:true,materials:reviewedProject.materials} as unknown as Project;
  render(<BuildPage {...base} project={handled} report={toReview} fresh={false} previous={{analyzed:unreviewed,refused:true}} build={build}/>);
  expect(screen.getByRole('heading',{name:'The last build found something that still needs a choice'})).toBeInTheDocument();
  expect(screen.queryByText(/is handled/)).not.toBeInTheDocument();
  expect(item(/detail-level strategy/)).toHaveClass('review-item-done');
  expect(screen.getByText('Analyze to see what is left.')).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Analyze project'})).toHaveClass('primary');
  expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
  window.dispatchEvent(new Event('dmb-build-shortcut'));
  expect(build).not.toHaveBeenCalled();
 });
 it('says the last build checked the project after building from the kept list',()=>{
  const done={...unreviewed,review_lods:true,materials:reviewedProject.materials,meshes:{body:'body',face:'face'}} as unknown as Project;
  render(<BuildPage {...base} project={done} report={{...toReview,unresolved_meshes:['face']}} fresh={false} previous={{analyzed:unreviewed,verified:true}}/>);
  expect(screen.getByRole('heading',{name:'The last build checked this project'})).toBeInTheDocument();
  expect(item(/model mapping need a choice/)).toHaveClass('review-item-done');
  expect(screen.getByText('Analyze again to refresh the analysis details (optional).')).toBeInTheDocument();
  expect(screen.queryByText('Current analysis')).not.toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled();
 });
});
describe('VPK slot status',()=>{
 it('reads the addons folder again every few seconds and names the project’s own installed build',()=>{
  vi.useFakeTimers();
  try{
   const scan=vi.fn();
   const slots={configured_slot:project.vpk_number,configured_slot_occupied:false,configured_slot_own_install:true,available_slot:project.vpk_number,occupied_slots:[],previous_outputs:[project.vpk_number],scanned_folders:[]};
   const {unmount}=render(<BuildPage {...base} expert slots={slots} scanSlots={scan}/>);
   expect(scan).toHaveBeenCalledTimes(1);
   // A VPK deleted in Explorer while the app keeps focus is seen at the next read.
   vi.advanceTimersByTime(SLOT_RESCAN_MS*2);
   expect(scan).toHaveBeenCalledTimes(3);
   expect(screen.getAllByText(/Your installed build of this project/).length).toBeGreaterThan(0);
   unmount();
   vi.advanceTimersByTime(SLOT_RESCAN_MS*2);
   expect(scan).toHaveBeenCalledTimes(3);
  }finally{vi.useRealTimers();}
 });
});
describe('Rebuilding into the same slot',()=>{
 it('replaces the project’s own installed build without asking, but not another mod',()=>{
  const build=vi.fn();
  const report:Parameters<typeof BuildPage>[0]['report']={model_enabled:false,icons:[],animations:[],effects:[],audio:[],clothing:null,native_compile_required:true,warnings:[],native_validated:false,in_game_validated:false};
  const own={configured_slot:project.vpk_number,configured_slot_occupied:false,configured_slot_own_install:true,available_slot:project.vpk_number,occupied_slots:[],previous_outputs:[project.vpk_number],scanned_folders:[]};
  const {unmount}=render(<BuildPage {...base} build={build} report={report} fresh slots={own}/>);
  fireEvent.click(screen.getByRole('button',{name:'Build mod'}));
  expect(build).toHaveBeenLastCalledWith(true,true);
  unmount();
  render(<BuildPage {...base} build={build} report={report} fresh slots={{...own,configured_slot_own_install:false}}/>);
  fireEvent.click(screen.getByRole('button',{name:'Build mod'}));
  expect(build).toHaveBeenLastCalledWith(true,false);
 });
});
