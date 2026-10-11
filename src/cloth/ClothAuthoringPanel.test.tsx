import {fireEvent,render,screen,waitFor} from '@testing-library/react';
import {beforeEach,describe,expect,it,vi} from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import clothReference from '../../src-tauri/crates/deadlock-cloth-authoring/tests/fixtures/automatic-reference.json';
import type {Project} from '../models';
const mocks=vi.hoisted(()=>({invoke:vi.fn()}));vi.mock('@tauri-apps/api/core',()=>({invoke:mocks.invoke}));
import {ClothAuthoringPanel} from './ClothAuthoringPanel';
const project={...reference.cases[1].expected,customization:{enabled:true,cloth_setup:{meshes:['garment'],attachment:'top',collision:'legs'}}} as unknown as Project;
const analysis=clothReference.analysis[0].expected;
const receipt={status:'authored_for_native_review',workspace_path:'work/isolated',model_path:'work/isolated/full.vmdl',preview_model_path:'work/isolated/cloth.vmdl',preview_before_model_path:'work/isolated/before.vmdl',report_path:'work/isolated/report.json',analysis,surface:{render_path:'work/isolated/render.dmx',proxy_path:'work/isolated/proxy.dmx'},geometry_previews:{missing_staged_references:['models/retained.vanm']},pending_gates:['Independent original hair preservation'],build_authorized:false,runtime_ready:false,published:false,installed:false};
const operate=async(_label:string,fn:()=>Promise<void>)=>{await fn();};
beforeEach(()=>{vi.clearAllMocks();mocks.invoke.mockResolvedValue(receipt);});
describe('Isolated cloth authoring',()=>{
 it('uses saved surface choices and previews output without replacing the active model',async()=>{
  const update=vi.fn(),onPreview=vi.fn();render(<ClothAuthoringPanel project={project} update={update} onPreview={onPreview} busy={false} operate={operate}/>);
  fireEvent.click(screen.getByRole('button',{name:'Create automatic cloth preview'}));await screen.findByText(/Cloth workspace created for review/);
  expect(mocks.invoke).toHaveBeenCalledWith('cloth_setup',{project});expect(update).not.toHaveBeenCalled();expect(screen.getByRole('status')).toHaveTextContent('Runtime compatibility has not been established');
  fireEvent.click(screen.getByRole('button',{name:'Inspect staged render mesh'}));expect(onPreview).toHaveBeenCalledWith(receipt.surface.render_path);expect(screen.getByText('models/retained.vanm')).toBeInTheDocument();expect(screen.getByText('Independent original hair preservation')).toBeInTheDocument();
 });
 it('clears prior receipts when source choices change',async()=>{
  const props={project,update:vi.fn(),busy:false,operate};const view=render(<ClothAuthoringPanel {...props}/>);fireEvent.click(screen.getByRole('button',{name:'Create automatic cloth preview'}));await screen.findByText(/Cloth workspace created for review/);
  view.rerender(<ClothAuthoringPanel {...props} project={{...project,dmx:'different.dmx'}}/>);await waitFor(()=>expect(screen.queryByText(/Cloth workspace created for review/)).not.toBeInTheDocument());expect(screen.queryByText(receipt.workspace_path)).not.toBeInTheDocument();
 });
 it('requires one chosen surface for automatic setup and keeps custom-bone analysis a separate action',()=>{
  render(<ClothAuthoringPanel project={{...project,customization:{}}} update={vi.fn()} busy={false} operate={operate}/>);expect(screen.getByRole('button',{name:'Create automatic cloth preview'})).toBeDisabled();expect(screen.getByRole('button',{name:'Analyze skirt bones'})).toBeEnabled();expect(mocks.invoke).not.toHaveBeenCalled();
 });
});
