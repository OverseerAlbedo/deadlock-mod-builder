import { fireEvent,render,screen } from '@testing-library/react';
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
