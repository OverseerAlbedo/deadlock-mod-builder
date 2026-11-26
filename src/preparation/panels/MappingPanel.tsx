// i18n-area: prepare
import type { Dispatch, SetStateAction } from 'react';
import { m, tp, useT } from '../../i18n';
import { T } from '../../i18n/T';
import { nextSentence, type MappingEntry, type PreparationAnalysis, type PreparationOptions } from '../models';
import { applyFollowing, assignTargets, boneLabelText } from '../mapping';
import type { PrepPanel } from '../PreparationChrome';
import { BUTTON_NAMES, TAB_NAMES } from '../pageNames';
import { PrepCard, PrepCardHeading } from './PrepCard';
/** Rows per page of the bone list. */
export const MATCH_PAGE=30;
/** The row of a weighted bone. */
export const matchRowId=(name:string)=>`prep-match-${Array.from(name).map(c=>/[A-Za-z0-9_-]/.test(c)?c:`-${c.codePointAt(0)!.toString(16)}`).join('')}`;
/** A quick fix as a card: the whole card is the button; its name stays the action's full name. */
function QuickFix({name,title,text,count,disabled,onClick}:{name:string;title:string;text:string;count:number;disabled:boolean;onClick:()=>void}){
  const t=useT();
  return <button type="button" className="prep-quickfix" aria-label={name} disabled={disabled} onClick={onClick}>
    <strong>{title}</strong><small>{text}</small><span className="prep-quickfix-apply" aria-hidden="true">{count?t('Apply to {count}',{count}):t('Nothing to apply')}</span>
  </button>;
}
/** The Joint matches tab: quick fixes as cards, the refresh prompt after role changes, offset
 * protection, the remaining groups, the filters, the bulk tools and one row per weighted bone.
 * Bone, group and target names and the reasons of each row come from the suggestions (Rust) and are shown as they come. */
export function MappingPanel({staleMatches=0,unmapped,busy,pelvisFollowing,changeOptions,options,allFollowing,rolesChanged,suggestions,roleProblems,showPanel,followingCount,offsetUpgrades,proposalGroups,proposed,setSearch,setPage,setGroup,setFilter,setSelectedBone,filter,group,groups,search,selectedBones,visible,setSelectedBones,kept,batchTarget,setBatchTarget,analysis,currentPage,pageCount,selectedBone='',total}:{/** Body-following matches the last check found stale (shows the refresh button). */staleMatches?:number;unmapped:number;busy:boolean;pelvisFollowing:MappingEntry[];changeOptions:(values:Partial<PreparationOptions>)=>void;options:PreparationOptions;allFollowing:MappingEntry[];rolesChanged:boolean;suggestions:(resetRoles?:boolean)=>Promise<void>;roleProblems:string[];showPanel:(next:PrepPanel)=>void;followingCount:number;offsetUpgrades:MappingEntry[];proposalGroups:string[];proposed:MappingEntry[];setSearch:Dispatch<SetStateAction<string>>;setPage:Dispatch<SetStateAction<number>>;setGroup:Dispatch<SetStateAction<string>>;setFilter:Dispatch<SetStateAction<string>>;setSelectedBone:Dispatch<SetStateAction<string>>;filter:string;group:string;groups:string[];search:string;selectedBones:Set<string>;visible:MappingEntry[];setSelectedBones:Dispatch<SetStateAction<Set<string>>>;kept:Set<string>;batchTarget:string;setBatchTarget:Dispatch<SetStateAction<string>>;analysis:PreparationAnalysis;currentPage:number;pageCount:number;
  /** Optional: the bone shown in the Joint check view (its row is marked). */selectedBone?:string;
  /** Optional: every weighted bone (the All filter's count). */total?:number}){
  const t=useT();
  const refresh=<button type="button" disabled={busy} onClick={()=>void suggestions(false)}>{t(BUTTON_NAMES.refreshMatches)}</button>;
  const filters:[string,string,number|undefined][]=[['unmapped',m('Needs a match'),unmapped],['following',m('Body-following'),followingCount],['all',m('All'),total]];
  const shown=visible.slice(currentPage*MATCH_PAGE,(currentPage+1)*MATCH_PAGE);
  const stale=tp(staleMatches,{one:'{count} body-following match no longer fits your body roles.',other:'{count} body-following matches no longer fit your body roles.'});
  return (
    <PrepCard title={t('Joint matches')} className="prep-mapping-card" sub={<T text="<strong>{count}</strong> weighted bones still need a target. Every weighted bone must be resolved before conversion." values={{count:unmapped}}/>}>
      <PrepCardHeading>{t('Quick fixes')}</PrepCardHeading>
      <div className="prep-quickfixes">
        <QuickFix name={t('Follow pelvis, keep offset for all ({count})',{count:pelvisFollowing.length})} title={t('Follow pelvis, keep offset')} text={t('Every remaining pelvis suggestion.')} count={pelvisFollowing.length} disabled={busy||!pelvisFollowing.length} onClick={()=>changeOptions(applyFollowing(options,pelvisFollowing))}/>
        <QuickFix name={t('Follow suggested joints, keep offsets for all ({count})',{count:allFollowing.length})} title={t('Follow suggested joints, keep offsets')} text={t('Each bone follows its suggested joint.')} count={allFollowing.length} disabled={busy||!allFollowing.length} onClick={()=>changeOptions(applyFollowing(options,allFollowing))}/>
      </div>
      <p className="studio-note">{t('Applies to remaining suggestions across every page, including hidden rows. Existing assignments stay unchanged. The pelvis button covers pelvis suggestions; the other button uses each bone’s suggested joint. Separate hair, cloth and facial motion is not reproduced.')}</p>
      {rolesChanged?<div className="prep-callout warn" role="status"><p>{t('Body roles changed. Refresh suggestions to use your current choices.')}</p>{refresh}</div>
        :roleProblems.length>0?<div className="prep-callout warn" role="status"><p>{t('Follow suggestions are waiting for valid body roles.')}</p><ul>{roleProblems.map((problem,index)=><li key={index}>{problem}</li>)}</ul><div className="prep-callout-actions"><button type="button" onClick={()=>showPanel('roles')}>{t('Review body roles')}</button>{refresh}</div></div>
        :staleMatches>0?<div className="prep-callout warn" role="status"><p>{nextSentence(stale,t('Refresh suggestions to update them.'))}</p>{refresh}</div>
        :allFollowing.length===0?<p className="studio-note">{unmapped?t('No remaining bones have a verified body joint to follow. Review their targets below or preserve eligible moving parts on the {tab} tab.',{tab:t(TAB_NAMES.physics)}):t('All weighted bones already have a target or are kept as independent bones.')}</p>:null}
      {followingCount>0&&<p className="studio-note">{tp(followingCount,{other:'{count} mappings preserve offsets while following a body joint. Their original independent movement is not reproduced.'})}</p>}
      {offsetUpgrades.length>0&&<div className="prep-callout ok"><p>{tp(offsetUpgrades.length,{other:'{count} existing helper mappings share a body joint without offset protection.'})}</p><button type="button" disabled={busy} onClick={()=>changeOptions(applyFollowing(options,offsetUpgrades,true))}>{tp(offsetUpgrades.length,{other:'Preserve offsets for {count} existing helper mappings'})}</button><p className="studio-note">{t('Keeps your chosen targets and prevents these helpers from pulling the rest geometry toward a shared joint. Review the converted shape afterward.')}</p></div>}
      {proposalGroups.length>0&&<>
        <PrepCardHeading>{t('Groups with no equivalent on this hero')}</PrepCardHeading>
        <p className="studio-note prep-groups-note">{t('These bones have no established equivalent on this hero. You can make a group follow its mapped ancestor, preserving its rest offset. Hair and clothing will have no new simulation; fingers and face joints lose their separate articulation. Leave a group unresolved if that is not acceptable.')}</p>
        <div className="prep-proposal-cards">{proposalGroups.map(g=>{const candidates=proposed.filter(e=>e.follow!.group===g),valid=!rolesChanged&&candidates.every(e=>options.mapping?.[e.follow!.anchor]===e.follow!.target);return <div className="prep-proposal-card" key={g}><strong>{tp(candidates.length,{other:'{group} · {count} bones'},{group:g})}</strong><div className="prep-proposal-actions"><button type="button" disabled={busy} onClick={()=>{setSearch(candidates[0].source);setPage(0);setGroup('all');setFilter('unmapped');setSelectedBone(candidates[0].source);}}>{t('Show first bone')}</button><button type="button" disabled={busy||!valid} onClick={()=>changeOptions(applyFollowing(options,candidates))}>{tp(candidates.length,{other:'Make {count} bones follow {target}'},{target:candidates[0].follow!.target})}</button></div>{!valid&&<p className="studio-note">{t('Roles changed. Refresh suggestions using your current roles.')}</p>}</div>;})}</div>
      </>}
      <div className="prep-match-filters">
        <div className="studio-view-group prep-segmented" role="group" aria-label={t('Show')}>{filters.map(([value,label,count])=><button key={value} type="button" aria-pressed={filter===value} onClick={()=>{setFilter(value);setPage(0);}}>{t(label)}{count!==undefined&&<>{' '}<small>{count}</small></>}</button>)}</div>
        <label className="prep-match-group"><span className="sr-only">{t('Group')}</span><select value={group} onChange={e=>{setGroup(e.target.value);setPage(0);}}><option value="all">{t('All groups')}</option>{groups.map(g=><option key={g}>{g}</option>)}</select></label>
        <label className="prep-match-search"><span className="sr-only">{t('Find bone')}</span><input type="search" value={search} onChange={e=>{setSearch(e.target.value);setPage(0);}} placeholder={t('Name or English label')}/></label>
      </div>
      <div className="prep-bulk-row" role="group" aria-label={t('Assign multiple bones')}>
        <strong>{t('Several at once')}</strong>
        <button type="button" disabled={busy||!visible.length} onClick={()=>setSelectedBones(new Set(visible.filter(e=>!kept.has(e.source)).map(e=>e.source)))}>{t('Select filtered bones ({count})',{count:visible.length})}</button>
        <button type="button" disabled={!selectedBones.size} onClick={()=>setSelectedBones(new Set())}>{t('Clear selection')}</button>
        <span className="studio-note">{tp(selectedBones.size,{other:'{count} selected'})}</span>
        <span className="prep-bulk-assign"><select aria-label={t('Target for selected bones')} title={batchTarget||undefined} disabled={busy||!selectedBones.size} value={batchTarget} onChange={e=>setBatchTarget(e.target.value)}><option value="">{t('Choose a shared target…')}</option>{analysis.target.map(b=><option key={b.name} value={b.name}>{b.name}</option>)}</select>
          <button type="button" disabled={busy||!selectedBones.size||!batchTarget} onClick={()=>{changeOptions(assignTargets(options,[...selectedBones].filter(n=>!kept.has(n)),batchTarget));setSelectedBones(new Set());}}>{batchTarget?tp(selectedBones.size,{other:'Assign {target} to {count} selected bones'},{target:batchTarget}):tp(selectedBones.size,{other:'Assign target to {count} selected bones'})}</button></span>
        <p className="studio-note">{t('Use a shared target only when you intend these bones to move together. Merging joints can change bind geometry and removes their independent motion. Inspect the movement test and the converted model before accepting.')}</p>
      </div>
      <div className="prep-match-list">{shown.map(entry=>{const mapped=options.mapping?.[entry.source],isKept=kept.has(entry.source);return <div key={entry.source} id={matchRowId(entry.source)} className={`prep-match-row ${selectedBone===entry.source?'selected':''} ${isKept?'kept':''} ${!isKept&&!mapped?'open':''}`}>
        <input type="checkbox" aria-label={t('Select bone {name}',{name:entry.source})} checked={selectedBones.has(entry.source)} disabled={busy||isKept} onChange={e=>setSelectedBones(current=>{const next=new Set(current);if(e.target.checked)next.add(entry.source);else next.delete(entry.source);return next;})}/>
        <div className="prep-match-name"><button type="button" className="prep-match-bone" aria-label={t('Show bone {name}',{name:entry.source})} aria-pressed={selectedBone===entry.source} onClick={()=>setSelectedBone(entry.source)}>{entry.source}</button><small>{entry.label!==entry.source?`${boneLabelText(entry.label)} · `:''}{entry.group}</small></div>
        <div className="prep-match-target">{isKept?<p className="studio-note">{t('Kept as an independent bone')}</p>:<>
          <div className="prep-match-controls">
            <select aria-label={t('Map {name}',{name:entry.source})} title={mapped||undefined} className={mapped?'':'needs'} disabled={busy} value={mapped??''} onChange={event=>{setSelectedBone(entry.source);changeOptions(assignTargets(options,[entry.source],event.target.value));}}><option value="">{t('Choose a target bone…')}</option>{analysis.target.map(bone=><option key={bone.name} value={bone.name}>{bone.name}</option>)}</select>
            {!mapped&&entry.target&&<button type="button" disabled={busy} onClick={()=>changeOptions(assignTargets(options,[entry.source],entry.target!))}>{t('Use proposed {target}',{target:entry.target})}</button>}
            {!mapped&&entry.follow&&<button type="button" disabled={busy||rolesChanged||options.mapping?.[entry.follow.anchor]!==entry.follow.target} onClick={()=>changeOptions(applyFollowing(options,[entry]))}>{t('Follow {target}, keep offset',{target:entry.follow.target})}</button>}
          </div>
          <small className="studio-note">{options.mapping_anchors?.[entry.source]?t('Follows {anchor} with rest offset preserved. Independent source motion is not transferred.',{anchor:options.mapping_anchors[entry.source]}):entry.reason}</small></>}
        </div>
      </div>;})}</div>
      {!visible.length&&<p className="quiet-empty">{t('No bones match this filter.')}</p>}
      {visible.length>0&&<div className="prep-pager"><button type="button" disabled={currentPage===0} onClick={()=>setPage(currentPage-1)}>{t('Previous bones')}</button><span>{currentPage+1} / {pageCount}</span><button type="button" disabled={currentPage+1>=pageCount} onClick={()=>setPage(currentPage+1)}>{t('Next bones')}</button><span className="studio-note prep-pager-size">{tp(MATCH_PAGE,{other:'{count} bones per page'})}</span></div>}
    </PrepCard>
  );
}
