import type { Dispatch, SetStateAction } from 'react';
import type { IssueKind, ReviewIssue } from '../models';
import { PrepIcon, type PrepPanel } from '../PreparationChrome';
import { BUTTON_NAMES, TAB_NAMES } from '../pageNames';
/** The one-click fix offered next to an issue of this kind (`ReviewIssue.kind`, set by the review). */
const FIXES:Record<IssueKind,string>={stale_following:BUTTON_NAMES.refreshTheMatches,stale_physics_groups:BUTTON_NAMES.useDetectedGroups};
/** The tab an issue belongs to, from the backend's `tab` (`roles`, `landmarks`, `mapping`,
 * `physics bones`, `weapon`, `model fitting`, `shape keys`, …). */
export function issueSection(tab:string):PrepPanel{
  return /mapping/i.test(tab)?'mapping':/roles|landmark/i.test(tab)?'roles':/physics|secondary/i.test(tab)?'physics':/weapon|gun/i.test(tab)?'weapon':'review';
}
/** The result of Check for problems (or of Prepare model when it stopped). Each item is headed by
 * the tab it opens; `open` also gets the backend's tab so Fit & review can open the right group. */
export function CheckResults({issues,reviewKey,currentKey:key,setMatchesOpen,setPanel,jump,fix,busy=false,open}:{issues:ReviewIssue[];reviewKey:string;/** The current choices' key (the page's `key`). */currentKey:string;setMatchesOpen:Dispatch<SetStateAction<boolean>>;setPanel:Dispatch<SetStateAction<PrepPanel|null>>;jump:(id:string)=>void;
  /** Runs the one-click fix of an issue kind, then checks again. */fix?:(kind:IssueKind)=>void;busy?:boolean;
  /** Called with the backend's tab when Open section is pressed (after the tab is chosen). */open?:(tab:string)=>void}){
  return (
      <section className={`prep-check-results ${issues.length?'has-issues':'passed'}`} id="prep-check-results" aria-label="Review results" aria-live="polite"><div className="prep-check-heading"><PrepIcon name={issues.length?'info':'check'}/><div><h2>{issues.length?`${issues.length} thing${issues.length===1?'':'s'} to fix before preparing`:reviewKey===key?'No problems found.':'Choices changed. Check again.'}</h2>{issues.length>0&&<p>Open the matching section, make your changes, then click Prepare model again.</p>}</div></div>{issues.length>0&&<ul>{issues.map((issue,index)=>{const section=issueSection(issue.tab);const kind=issue.kind&&issue.kind in FIXES?issue.kind as IssueKind:null;return <li key={index}><div><strong>{TAB_NAMES[section]}</strong><p>{issue.message}</p></div><div className="row">{kind&&fix&&reviewKey===key&&<button disabled={busy} onClick={()=>fix(kind)}>{FIXES[kind]}</button>}<button onClick={()=>{if(section==='roles'||section==='mapping')setMatchesOpen(true);setPanel(section);open?.(issue.tab);jump('prep-review-workspace');}}>Open section<PrepIcon name="arrow" size={16}/></button></div></li>;})}</ul>}</section>
  );
}
