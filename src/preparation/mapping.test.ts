import { describe,expect,it } from 'vitest';
import { applyFollowing,applySuggestions,assignTargets,changeRole,pendingFollowing,roleWarnings } from './mapping';
import type { MappingEntry } from './models';
describe('role and skin mapping consistency',()=>{
  it('bulk following spans all eligible rows while preserving manual assignments and rejecting stale proposals',()=>{
    const row=(source:string,anchor='hips',target='pelvis'):MappingEntry=>({source,target:null,group:'helpers',label:source,reason:'review',automatic:false,follow:{anchor,target,group:'helpers',automatic:false,reason:'review'}});
    const entries=[...Array.from({length:70},(_,i)=>row(`helper_${i}`)),row('hair','skull','head'),row('manual'),row('stale','missing','pelvis')];
    const options={mapping:{hips:'pelvis',skull:'head',manual:'hand_L'},reviewed:true,follow_motion_reviewed:true};
    const pelvis=pendingFollowing(options,entries,'pelvis');expect(pelvis).toHaveLength(70);
    const next=applyFollowing(options,pelvis);
    expect(next.mapping?.helper_69).toBe('pelvis');expect(next.mapping_anchors?.helper_69).toBe('hips');
    expect(next.mapping?.hair).toBeUndefined();expect(next.mapping?.manual).toBe('hand_L');expect(next.mapping?.stale).toBeUndefined();
    const all=applyFollowing(next,pendingFollowing(next,entries));
    expect(all.mapping?.hair).toBe('head');expect(all.mapping_anchors?.hair).toBe('skull');
    expect(all.reviewed).toBe(false);expect(all.follow_motion_reviewed).toBe(false);
    expect(pendingFollowing(all,entries)).toHaveLength(0);
  });
  it('maps unweighted role controls and removes the obsolete generated pair',()=>{
    const changed=changeRole({source_roles:{pelvis:'腰',head:'頭'},target_roles:{pelvis:'pelvis',head:'head'},mapping:{'腰':'pelvis','頭':'head','下半身':'pelvis'},reviewed:true},'source','head','otherHead');
    expect(changed.mapping).toEqual({'腰':'pelvis',otherHead:'head','下半身':'pelvis'});expect(changed.reviewed).toBe(false);
    const targetChanged=changeRole(changed,'target','pelvis','hips');expect(targetChanged.mapping?.['腰']).toBe('hips');
  });
  it('warns immediately about assigning the waist control to both pelvis and head',()=>{
    const next=changeRole({source_roles:{pelvis:'腰',head:'頭'},target_roles:{pelvis:'pelvis',head:'head'}},'source','head','腰');
    expect(roleWarnings(next)[0]).toContain('both Pelvis / hips and Head');
  });
  it('batch assignment keeps unrelated weights and synchronizes primary target roles',()=>{
    const result=assignTargets({source_roles:{head:'頭'},target_roles:{head:'head'},mapping:{other:'arm'},reviewed:true},['頭','extra'],'neck');
    expect(result.mapping).toEqual({other:'arm','頭':'neck',extra:'neck'});expect(result.target_roles?.head).toBe('neck');expect(result.reviewed).toBe(false);
  });
  it('clearing a role removes its generated mapping but retains other manual choices',()=>{
    expect(changeRole({source_roles:{head:'頭'},target_roles:{head:'head'},mapping:{'頭':'head',extra:'head'}},'source','head','').mapping).toEqual({extra:'head'});
  });
  it('applies group proposals with offsets without changing existing direct choices or approval',()=>{
    const proposal=(source:string):MappingEntry=>({source,target:null,group:'extras',label:source,reason:'review',automatic:false,follow:{anchor:'arm',target:'arm_L',group:'extras',automatic:false,reason:'review'}});
    const changed=applyFollowing({mapping:{arm:'arm_L',manual:'head'},reviewed:true,follow_motion_reviewed:true},[proposal('extra'),proposal('manual')]);
    expect(changed.mapping).toEqual({arm:'arm_L',manual:'head',extra:'arm_L'});expect(changed.mapping_anchors).toEqual({extra:'arm'});expect(changed.follow_motion_reviewed).toBe(false);expect(changed.reviewed).toBe(false);
    const stale=applyFollowing({mapping:{arm:'other'}},[proposal('extra')]);expect(stale.mapping?.extra).toBeUndefined();
    const direct=assignTargets(changed,['extra'],'hand_L');expect(direct.mapping_anchors?.extra).toBeUndefined();
  });
  it('explicit offset upgrades retain the target and automatic suggestions require renewed review',()=>{
    const entry:MappingEntry={source:'extra',target:'arm_L',group:'helpers',label:'extra',reason:'review',automatic:false,follow:{anchor:'arm',target:'arm_L',group:'helpers',automatic:true,reason:'review'}};
    const next=applyFollowing({mapping:{arm:'arm_L',extra:'arm_L'}},[entry],true);expect(next.mapping_anchors).toEqual({extra:'arm'});
    const result=applySuggestions({...next,reviewed:true,follow_motion_reviewed:true},{target:{roles:{},missing:[],errors:[],status:'',evidence:''},mapping:next.mapping!,assistance:{policy:'test',entries:[],labels:{},warnings:[],anchors:next.mapping_anchors}});
    expect(result.mapping_anchors).toEqual(next.mapping_anchors);expect(result.reviewed).toBe(false);expect(result.follow_motion_reviewed).toBe(false);
  });
});
