import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PreparationPage } from './PreparationPage';
import { WeaponPanel, weaponDefaults } from './WeaponPanel';
import type { PreparationAnalysis, PreparationOptions, WeaponCandidate } from './models';
import type { Project } from '../models';
import { MIRROR_KEY, setLanguage } from '../i18n';
import sourceFixture from '../../src-tauri/crates/deadlock-preparation/tests/fixtures/reference.json';
import projectFixture from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

// Prepare model driven in Chinese: the flows find their controls by the Chinese names, and
// what the page saves or sends stays English (option ids, bone names, operation labels).
const native=vi.hoisted(()=>({preparationInspect:vi.fn(),preparationResume:vi.fn(),preparationSuggest:vi.fn(),preparationReview:vi.fn(),preparationConvert:vi.fn(),preparationAccept:vi.fn(),preparationSaveMappingPreset:vi.fn(),preparationWeaponPreview:vi.fn(),validateAssetPath:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:native}));
vi.mock('./WeaponViewport',()=>({default:()=><div>gun viewport</div>}));

const analysis={source:'C:\\source.blend',source_original:'C:\\source.blend',source_sha256:'a'.repeat(64),target_model:'models/heroes/test.vmdl',target_sha256:'b'.repeat(64),analysis:sourceFixture.inspection,target:sourceFixture.target,target_skeleton:sourceFixture.skeleton,target_scale_audit:sourceFixture.scale_audit,suggestions:sourceFixture.cases[1].expected,rotation:[0,0,0],geometry_customization:{props:[],face:null},clothing:null} as unknown as PreparationAnalysis;
const initial={...projectFixture.cases[0].expected,original_model:analysis.target_model,preparation_source:analysis.source_original,preparation_options:{fit_mode:'match_target'},model_input_mode:'rigged_humanoid'} as unknown as Project;
const output={dmx:'C:\\prepared.dmx',blend:'C:\\prepared.blend',report:'C:\\report.json',textures:'',target_model:analysis.target_model,warnings:[]};
const operate=vi.fn(async(_label:string,task:()=>Promise<void>)=>task());
function Harness({start=initial,next}:{start?:Project;next?:()=>void}){const[project,setProject]=useState(start);return <><PreparationPage project={project} update={setProject} busy={false} operate={operate} next={next}/><output data-testid="project">{JSON.stringify(project)}</output></>;}
const current=()=>JSON.parse(screen.getByTestId('project').textContent!) as Project;
const options=()=>current().preparation_options as PreparationOptions;
const radios=(group:string)=>within(screen.getByRole('radiogroup',{name:group})).getAllByRole('radio') as HTMLInputElement[];
/** Load and check only, by its Chinese name (after a check it sits in the Model file & orientation menu). */
async function checkInChinese(){
  if(!screen.queryByRole('button',{name:'仅加载并检查'}))fireEvent.click(screen.getByRole('button',{name:/模型文件与朝向/}));
  fireEvent.click(screen.getByRole('button',{name:'仅加载并检查'}));
  await screen.findByRole('tablist',{name:'准备检查分区'});
}

beforeEach(async()=>{
  vi.clearAllMocks();
  native.preparationInspect.mockResolvedValue(analysis);native.preparationSuggest.mockResolvedValue(analysis.suggestions);native.preparationReview.mockResolvedValue([]);
  native.preparationConvert.mockResolvedValue(output);native.validateAssetPath.mockImplementation(async(path:string)=>path);native.preparationWeaponPreview.mockReturnValue(new Promise(()=>undefined));
  native.preparationAccept.mockImplementation(async(project:Project,result:typeof output)=>({...project,dmx:result.dmx,preparation_report:result.report,preparation_blend:result.blend}));
  await act(()=>setLanguage('zh-CN'));
});
afterEach(async()=>{cleanup();await act(()=>setLanguage('en'));localStorage.removeItem(MIRROR_KEY);});

describe('Prepare model in Chinese',()=>{
  it('checks the model, takes fit choices by their Chinese titles, prepares and saves the English ids',async()=>{
    const next=vi.fn();
    render(<Harness next={next}/>);
    expect(screen.getByRole('heading',{level:1,name:/准备模型/})).toBeInTheDocument();
    await checkInChinese();
    // The operation keeps its English label (it names the failure and the notification).
    expect(operate).toHaveBeenCalledWith('Source preparation inspection did not complete',expect.any(Function),true);
    for(const name of ['身体部位','活动部件','关节匹配','适配与检查'])expect(screen.getByRole('tab',{name:new RegExp(name)})).toBeInTheDocument();
    fireEvent.click(screen.getByRole('tab',{name:/适配与检查/}));
    fireEvent.click(within(screen.getByRole('radiogroup',{name:'模型适配'})).getByRole('radio',{name:/保留原模型比例/}));
    expect(options().fit_mode).toBe('preserve_proportions');
    fireEvent.click(screen.getByRole('tab',{name:/^双脚/}));
    expect(radios('双脚贴地').map(radio=>radio.value)).toEqual(['ankles','legs','lift']);
    fireEvent.click(within(screen.getByRole('radiogroup',{name:'双脚贴地'})).getByRole('radio',{name:/整体上移或下移角色/}));
    expect(options().fit_floor).toBe('lift');
    fireEvent.click(screen.getByRole('tab',{name:/^腿部/}));
    fireEvent.click(within(screen.getByRole('radiogroup',{name:'腿部关节'})).getByRole('radio',{name:/按髋关节放置，腿放在英雄的关节上（不缩放）\s*膝盖被拉伸时推荐/}));
    expect(options().fit_leg_joints).toBe('joints');
    fireEvent.click(screen.getAllByRole('button',{name:'准备模型'}).at(-1)!);
    await vi.waitFor(()=>expect(next).toHaveBeenCalledOnce());
    expect(operate).toHaveBeenLastCalledWith('Your model could not be prepared',expect.any(Function),true);
    expect(native.preparationConvert.mock.calls.at(-1)![2]).toMatchObject({fit_mode:'preserve_proportions',fit_floor:'lift',fit_leg_joints:'joints',reviewed:true});
    expect(await screen.findByText('正在使用你准备好的模型')).toBeVisible();
  });
  it('changes a body role by its Chinese name and saves the bone name as it is',async()=>{
    render(<Harness/>);await checkInChinese();
    const region=within(screen.getByRole('group',{name:'显示身体区域'}));
    fireEvent.click(region.getByRole('button',{name:'腿与脚'}));
    expect(region.getByRole('button',{name:'腿与脚'})).toHaveAttribute('aria-pressed','true');
    expect(screen.getByRole('button',{name:'左大腿'})).toBeInTheDocument();
    fireEvent.click(region.getByRole('button',{name:'所有关节'}));
    const alternative=analysis.analysis.bones.find(b=>b.name!==analysis.suggestions.source!.roles.head)!;
    fireEvent.change(screen.getByLabelText('源骨骼：头部 (head)'),{target:{value:alternative.name}});
    expect(options().source_roles?.head).toBe(alternative.name);
    fireEvent.click(screen.getByRole('tab',{name:/关节匹配/}));
    expect(screen.getByText('身体部位已更改。请刷新建议以使用你当前的选择。')).toBeVisible();
    fireEvent.click(screen.getAllByRole('button',{name:'按我的部位刷新建议'})[0]);
    await vi.waitFor(()=>expect(native.preparationSuggest).toHaveBeenCalledWith(expect.anything(),expect.objectContaining({source_roles:expect.objectContaining({head:alternative.name})})));
    expect(operate).toHaveBeenLastCalledWith('Automatic mapping did not complete',expect.any(Function));
  });
  it('shows the review messages as they come and offers the one-click fix in Chinese',async()=>{
    const message='7 body-following matches no longer fit your body roles (b0, b1, b2, b3, b4 and 2 more). On the Joint matches tab, press Refresh suggestions using my roles, or choose targets for these bones.';
    native.preparationReview.mockResolvedValue([{tab:'mapping',message,kind:'stale_following',bones:['b0']}]);
    render(<Harness/>);await checkInChinese();
    fireEvent.click(screen.getAllByRole('button',{name:'检查问题'}).at(-1)!);
    const results=within(await screen.findByRole('region',{name:'检查结果'}));
    expect(results.getByRole('heading',{name:'准备前有 1 项需要修复'})).toBeInTheDocument();
    // The message is the review's own (Rust), unchanged; the tab name above it is the page's.
    expect(results.getByText(message)).toBeInTheDocument();
    expect(results.getByText('关节匹配')).toBeInTheDocument();
    expect(results.getByRole('button',{name:'刷新匹配'})).toBeInTheDocument();
    fireEvent.click(results.getByRole('button',{name:/打开该部分/}));
    expect(screen.getByRole('tab',{name:/关节匹配/})).toHaveAttribute('aria-selected','true');
  });
  it('switches live between English and Chinese without losing the check',async()=>{
    await act(()=>setLanguage('en'));
    render(<Harness/>);
    fireEvent.click(screen.getByRole('button',{name:'Load and check only'}));
    await screen.findByRole('tablist',{name:'Preparation review sections'});
    expect(screen.getByRole('tab',{name:/Body roles/})).toBeInTheDocument();
    await act(()=>setLanguage('zh-CN'));
    expect(screen.getByRole('tablist',{name:'准备检查分区'})).toBeInTheDocument();
    expect(screen.getByRole('tab',{name:/身体部位/})).toHaveAttribute('aria-selected','true');
    expect(screen.getByRole('button',{name:'左大腿'})).toBeInTheDocument();
    await act(()=>setLanguage('en'));
    expect(screen.getByRole('tab',{name:/Body roles/})).toBeInTheDocument();
    expect(screen.getByRole('button',{name:'Left thigh'})).toBeInTheDocument();
    expect(native.preparationInspect).toHaveBeenCalledOnce();
  });
});

const pistol:WeaponCandidate={id:'Pistol',objects:['Pistol'],vertices:24,bounds:[[0,0,0],[1,1,1]],sample:[[0,0,0]],attach_bone:'hand_R',attach_kind:'bone_parent',keywords:['pistol'],elongation:3};
const heroGun={policy:'weapon-fit-v1',source:'meshes',separate_model:false,meshes:['test_gun'],primary:['test_gun'],model_meshes:['test_body','test_gun'],bones:['weapon'],notes:[]};
const withGun={...analysis,analysis:{...analysis.analysis,weapons:[pistol]},weapon_reference:heroGun,weapon_suggestion:{enabled:true,action:'replace',objects:['Pistol']}} as unknown as PreparationAnalysis;
function GunHarness({start}:{start:PreparationOptions}){const[chosen,setChosen]=useState<PreparationOptions>(start);return <><WeaponPanel project={{...initial,hero_name:'Haze'}} analysis={withGun} options={chosen} change={values=>setChosen(previous=>({...previous,...values}))} busy={false} operate={async(_,task)=>task()}/><output data-testid="options">{JSON.stringify(chosen)}</output></>;}
describe('The gun panel in Chinese',()=>{
  it('explains the gun in Chinese, keeps the hero and part names, and saves the chosen action as its id',()=>{
    render(<GunHarness start={{weapon:weaponDefaults(withGun,undefined)}}/>);
    expect(screen.getByText('Haze 的枪械是 Haze 模型的一部分，因此你的枪械可以直接替换它。')).toBeVisible();
    expect(screen.getByText('你的角色文件中有一把枪：Pistol（在你的角色文件中）。')).toBeVisible();
    expect(screen.getByLabelText(/用我的枪械替换 Haze 的枪械/)).toBeChecked();
    fireEvent.click(screen.getByLabelText(/保留 Haze 的枪械/));
    const saved=JSON.parse(screen.getByTestId('options').textContent!) as PreparationOptions;
    expect(saved.weapon).toMatchObject({enabled:true,action:'remove',objects:['Pistol']});
    expect(screen.getByText('Haze 保留自己的枪械；你的枪械不会用在角色上。')).toBeVisible();
  });
});
