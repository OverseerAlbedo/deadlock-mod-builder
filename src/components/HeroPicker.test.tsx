import {describe,expect,it} from 'vitest';
import {alternateTarget,heroLabel} from './HeroPicker';
import type {HeroRecord} from '../models';
const hero=(path:string)=>({path,name:'wraith'} as HeroRecord);
describe('Main and alternate character targets',()=>{
  it('distinguishes the supplied staging Wraith target from the main roster model',()=>{
    expect(heroLabel(hero('models/heroes_wip/wraith/wraith.vmdl_c'))).toBe('Wraith');
    expect(heroLabel(hero('models/heroes_staging/wraith/wraith.vmdl_c'))).toBe('Wraith · Alternate model');
    expect(alternateTarget('models/heroes_staging/wraith/wraith.vmdl_c')?.path).toBe('models/heroes_wip/wraith/wraith.vmdl');
    expect(alternateTarget('models/heroes_wip/wraith/wraith.vmdl_c')).toBeUndefined();
    expect(alternateTarget('')).toBeUndefined();
  });
});
