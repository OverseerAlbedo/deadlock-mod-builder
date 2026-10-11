import {fireEvent,render,screen} from '@testing-library/react';
import {describe,expect,it,vi} from 'vitest';
import type {HeroRecord} from '../models';
const mocks=vi.hoisted(()=>({heroes:vi.fn()}));
vi.mock('../api',async original=>({...await original<typeof import('../api')>(),api:{heroes:mocks.heroes}}));
import {alternateTarget,heroLabel,HeroPicker} from './HeroPicker';
const hero=(path:string)=>({path,name:'wraith'} as HeroRecord);
describe('Main and alternate character targets',()=>{
  it('distinguishes the supplied staging Wraith target from the main roster model',()=>{
    expect(heroLabel(hero('models/heroes_wip/wraith/wraith.vmdl_c'))).toBe('Wraith');
    expect(heroLabel(hero('models/heroes_staging/wraith/wraith.vmdl_c'))).toBe('Wraith · Alternate model');
    expect(alternateTarget('models/heroes_staging/wraith/wraith.vmdl_c')?.path).toBe('models/heroes_wip/wraith/wraith.vmdl');
    expect(alternateTarget('models/heroes_wip/wraith/wraith.vmdl_c')).toBeUndefined();
    expect(alternateTarget('')).toBeUndefined();
  });
  it('uses the model the game loads for Rem as her main target',()=>{
    expect(heroLabel(hero('models/heroes_wip/familiar/familiar_wip.vmdl_c'))).toBe('Rem');
    expect(heroLabel(hero('models/heroes_wip/familiar/familiar.vmdl_c'))).toBe('Rem · Unused model');
    expect(alternateTarget('models/heroes_wip/familiar/familiar.vmdl_c')?.path).toBe('models/heroes_wip/familiar/familiar_wip.vmdl');
  });
  it('follows the installed game definitions and offers to switch a stale target',async()=>{
    const active={name:'Familiar Wip',path:'models/heroes_wip/familiar/familiar_wip.vmdl_c',likely_hero:true,hero:'Rem',game:{state:'active',heroes:['hero_familiar']}} as HeroRecord;
    const stale={name:'Familiar',path:'models/heroes_wip/familiar/familiar.vmdl_c',likely_hero:true,hero:'Rem',game:{state:'unused',model:'models/heroes_wip/familiar/familiar_wip.vmdl'}} as HeroRecord;
    expect(heroLabel(active)).toBe('Rem');
    expect(heroLabel(stale)).toBe('Rem · Not used in game');
    mocks.heroes.mockResolvedValue([stale,active]);
    const select=vi.fn();
    render(<HeroPicker current={stale.path} disabled={false} select={select} fail={vi.fn()}/>);
    fireEvent.click(screen.getByRole('button',{name:'Choose character…'}));
    expect(await screen.findByRole('alert')).toHaveTextContent('The installed game loads models/heroes_wip/familiar/familiar_wip.vmdl for Rem');
    fireEvent.click(screen.getByRole('button',{name:"Use the game's model"}));
    expect(select).toHaveBeenCalledWith(active);
  });
});
