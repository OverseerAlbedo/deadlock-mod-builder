import { useState } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ProjectOverview } from './ProjectOverview';
import { Appearance } from './Appearance';
import { PhysicsPage, ExtrasPage } from './CustomizationPages';
import { FilePathInput } from './FilePathInput';
import { BuildPage } from '../build/BuildPage';
import { inspected, reviewedProject } from '../build/testFixtures';
import { projectReadiness } from '../build/readiness';
import { qualifiedIncludedCloth } from '../cloth/includedSetup';
import { revealSection } from './workflowNavigation';
import baseline from '../../physics-references/Vindicta-R12-Balanced/baseline.json';
import type { Project } from '../models';
const native=vi.hoisted(()=>({validateAssetPath:vi.fn(),includedPhysics:vi.fn(),heroes:vi.fn(),open:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:native.open}));
beforeEach(()=>{vi.clearAllMocks();native.validateAssetPath.mockImplementation(async path=>path);native.heroes.mockResolvedValue([{name:'Vindicta',path:'models/heroes_staging/hornet_v3/hornet.vmdl',likely_hero:true}]);});
const buildProps={project:reviewedProject,update:vi.fn(),report:inspected,fresh:true,prepared:null,original:null,busy:false,inspect:vi.fn(),prepare:vi.fn(),validateOriginal:vi.fn(),build:vi.fn(),result:null,install:vi.fn(),installed:null,go:vi.fn()};
describe('Compact workflow and honest build decisions',()=>{
  it('requires fresh analysis, disables known blockers, and restores Build after resolution',()=>{
    const build=vi.fn();const view=render(<BuildPage {...buildProps} build={build} report={null} fresh={false}/>);
    expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
    expect(screen.getByRole('heading',{name:'1 issue blocking build'})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Build mod'}));expect(build).not.toHaveBeenCalled();
    view.rerender(<BuildPage {...buildProps} build={build} report={{...inspected,needs_review:true,unresolved_meshes:['body']}}/>);
    expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
    fireEvent.click(screen.getByRole('button',{name:'Configure mappings'}));
    expect(screen.getByText('Mesh mapping',{selector:'summary > span'}).closest('details')).toHaveAttribute('open');
    view.rerender(<BuildPage {...buildProps} build={build}/>);
    expect(screen.getByRole('button',{name:'Build mod'})).toBeEnabled();
    fireEvent.click(screen.getByRole('button',{name:'Build mod'}));expect(build).toHaveBeenCalledWith(false,false);
    view.rerender(<BuildPage {...buildProps} build={build} fresh={false}/>);
    expect(screen.getByRole('button',{name:'Build mod'})).toBeDisabled();
  });
  it('shows output errors only when packaging is requested',()=>{
    const bad={...reviewedProject,vpk_number:0};
    expect(projectReadiness(bad,inspected,true).ready).toBe(false);
    expect(projectReadiness(bad,inspected,true).issues.some(issue=>issue.section==='output-settings')).toBe(true);
    expect(projectReadiness({...bad,build_vpk:false},inspected,true).ready).toBe(true);
    expect(projectReadiness({...bad,compile_model:false},inspected,true).ready).toBe(true);
  });
  it('uses contextual actions and opens the relevant material or mesh controls',()=>{
    const go=vi.fn();render(<BuildPage {...buildProps} go={go}/>);
    expect(screen.queryByRole('button',{name:'Review'})).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Configure materials'}));expect(go).toHaveBeenCalledWith('appearance','materials');
    fireEvent.click(screen.getByRole('button',{name:'Configure mesh processing'}));expect(go).toHaveBeenCalledWith('appearance','model-processing');
    fireEvent.click(screen.getByRole('button',{name:'Configure output'}));
    expect(screen.getByText('Output settings',{selector:'summary > span'}).closest('details')).toHaveAttribute('open');
  });
  it('makes analysis progress explicit and leaves it unavailable without input prerequisites',()=>{
    const view=render(<BuildPage {...buildProps} report={null} fresh={false} busy analyzing/>);
    expect(screen.getByRole('button',{name:'Analyzing…'})).toBeDisabled();
    expect(screen.getByText('Processing')).toBeInTheDocument();
    view.rerender(<BuildPage {...buildProps} report={null} fresh={false} project={{...reviewedProject,original_model:'',dmx:''}}/>);
    expect(screen.queryByRole('button',{name:'Analyze project'})).not.toBeInTheDocument();
    expect(screen.getByText('Choose the required inputs above to enable analysis.')).toBeInTheDocument();
  });
  it('gives Overview one next step, keeps presets, and opens the real hero selector',async()=>{
    const loadOutfit=vi.fn(),saveOutfit=vi.fn(),selectHero=vi.fn();
    render(<ProjectOverview document={{path:null,project:{...reviewedProject,original_model:'',dmx:''}}} report={null} fresh={false} disabled={false} update={vi.fn()} selectHero={selectHero} fail={vi.fn()} go={vi.fn()} loadOutfit={loadOutfit} saveOutfit={saveOutfit}/>);
    expect(screen.getAllByRole('region',{name:'Next step'})).toHaveLength(1);
    expect(screen.getByRole('progressbar',{name:'Areas ready'})).toHaveAttribute('max','7');
    expect(screen.queryByText('Set up your project')).not.toBeInTheDocument();
    fireEvent.click(within(screen.getByRole('region',{name:'Next step'})).getByRole('button',{name:'Choose character'}));
    const selector=await screen.findByRole('list',{name:'Available characters'});
    fireEvent.change(screen.getByRole('textbox',{name:'Search characters'}),{target:{value:'hornet_v3'}});
    fireEvent.click(within(selector).getByRole('button'));expect(selectHero).toHaveBeenCalledWith(expect.objectContaining({name:'Vindicta'}));
    fireEvent.click(screen.getByText('Customization presets',{selector:'summary'}));
    fireEvent.click(screen.getByRole('button',{name:'Load preset…'}));fireEvent.click(screen.getByRole('button',{name:'Save preset…'}));
    expect(loadOutfit).toHaveBeenCalledOnce();expect(saveOutfit).toHaveBeenCalledOnce();
    expect(screen.getByText('Advanced project details',{selector:'summary > span'}).closest('details')).not.toHaveAttribute('open');
  });
  it('keeps unanalysed mesh processing neutral and shows an actual reported failure',()=>{
    const openPreparation=vi.fn();const props={document:{path:null,project:{...reviewedProject,auto_split:false}},summary:null,choose:vi.fn(),analyze:vi.fn(),busy:false,drag:null,update:vi.fn(),fail:vi.fn(),preview:null,loadPreview:vi.fn(),importPackage:vi.fn(),analyzeMaterials:vi.fn(),openPreparation};
    const view=render(<Appearance {...props}/>);
    expect(screen.getByText('Automatic splitting disabled').closest('.state-label')).toHaveClass('state-neutral');
    expect(screen.queryByText(/must be resolved before compilation/)).not.toBeInTheDocument();
    revealSection('model-processing');
    expect(screen.getByRole('checkbox',{name:'Automatically split oversized meshes'})).toBeVisible();
    expect(screen.getByText('Configure mesh processing',{selector:'summary > span'}).closest('details')).toHaveAttribute('open');
    fireEvent.click(screen.getByRole('button',{name:'Open Model preparation →'}));expect(openPreparation).toHaveBeenCalledOnce();
    view.rerender(<Appearance {...props} report={{...inspected,needs_review:true,vertex_preflight:{...inspected.vertex_preflight,errors:['Native mesh exceeds capacity']}}} fresh/>);
    expect(screen.getByText('Mesh processing needs attention').closest('.state-label')).toHaveClass('state-attention');
    expect(screen.getByText('Native mesh exceeds capacity')).toBeInTheDocument();
  });
  it('explains material prerequisites and keeps assignments available under Manage',()=>{
    const props={document:{path:null,project:{...reviewedProject,original_model:'',dmx:''}},summary:null,choose:vi.fn(),analyze:vi.fn(),busy:false,drag:null,update:vi.fn(),fail:vi.fn(),preview:null,loadPreview:vi.fn(),importPackage:vi.fn(),analyzeMaterials:vi.fn()};
    render(<Appearance {...props}/>);
    expect(screen.getByRole('button',{name:'Analyze materials'})).toBeDisabled();
    expect(screen.getByText('Choose a character and replacement model first to analyze materials.')).toBeInTheDocument();
    fireEvent.click(screen.getByText('Manage material assignments',{selector:'summary'}));
    expect(screen.getByRole('textbox',{name:'Search materials'})).toBeVisible();
  });
  it('does not promote Vindicta cloth from a hero name or a saved reference alone',()=>{
    const project={...reviewedProject,original_model:'models/heroes_staging/hornet_v3/hornet.vmdl',customization:{enabled:true,physics:{enabled:true,source:'C:\\cloth.vmdl_c'}}} as Project;
    const report={...inspected,original:{tested_physics:{reference:project.customization.physics?.source,source_sha256:inspected.source_sha256,selection:{reference_sha256:baseline.reference_sha256}}}};
    expect(qualifiedIncludedCloth(project,report,true)).toBe(true);
    expect(qualifiedIncludedCloth(project,report,false)).toBe(false);
    expect(qualifiedIncludedCloth(project,inspected,true)).toBe(false);
    const view=render(<PhysicsPage project={project} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null}/>);
    expect(screen.queryByText(/source binding checked/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('Configure replacement cloth',{selector:'summary > span'}));
    expect(screen.getByRole('button',{name:'Use included Vindicta cloth setup'})).toBeEnabled();
    view.rerender(<PhysicsPage project={{...project,original_model:'models/heroes_staging/bebop/bebop.vmdl'}} update={vi.fn()} disabled={false} fail={vi.fn()} summary={null}/>);
    expect(screen.queryByRole('button',{name:'Use included Vindicta cloth setup'})).not.toBeInTheDocument();
  });
  it('opens a spacious Extras library with clear category tabs and build options',()=>{
    render(<ExtrasPage project={{...reviewedProject,customization:{}}} update={vi.fn()} disabled={false} fail={vi.fn()}/>);
    expect(screen.queryByText('No extras added')).not.toBeInTheDocument();
    for(const name of [/Icons/,/Sounds/,/Effects/])expect(screen.getByRole('tab',{name})).toBeEnabled();
    expect(screen.getByText('Start with the original')).toBeVisible();
    expect(screen.getByText('Build options',{selector:'summary'}).closest('details')).not.toHaveAttribute('open');
  });
  it('validates both folder browse and native folder drops through the existing service',async()=>{
    native.open.mockResolvedValue('C:\\Textures');
    function Folder(){const [value,update]=useState('');return <FilePathInput label="Texture folder" kind="texture_directory" value={value} update={update} disabled={false} fail={vi.fn()}/>;}
    render(<Folder/>);expect(screen.getByText('Drop a folder here')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'Browse Texture folder'}));
    await waitFor(()=>expect(native.validateAssetPath).toHaveBeenCalledWith('C:\\Textures','texture_directory'));
    expect(screen.getByRole('button',{name:'Browse Texture folder'})).toHaveTextContent('Change folder');
    const target=screen.getByLabelText('Texture folder').closest('[data-native-drop]')!;
    await act(async()=>target.dispatchEvent(new CustomEvent('dmb-file-drop',{detail:['C:\\OtherTextures']})));
    await waitFor(()=>expect(native.validateAssetPath).toHaveBeenCalledWith('C:\\OtherTextures','texture_directory'));
    expect(screen.getByText('OtherTextures')).toBeInTheDocument();
  });
});
