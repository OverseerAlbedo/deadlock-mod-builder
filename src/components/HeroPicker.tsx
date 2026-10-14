import { useMemo, useState } from 'react';
import aliasesData from '../../resources/catalog/hero_aliases.json';
import rosterData from '../../resources/catalog/hero_roster.json';
import type { HeroRecord } from '../models';
import { api } from '../api';

const aliases:Record<string,string[]> = aliasesData;
const roster = rosterData as {name:string;path:string;extras?:{name:string;path:string}[];playable?:boolean}[];
const normalize = (path:string) => path.replace(/\\/g,'/').replace(/_c$/,'').toLowerCase();
/** What a model is in the game: the model a hero uses in matches, another model of a hero
 * (alternate), a hero whose files are in the game but who cannot be picked yet (Nurse Harrow,
 * Violet), or a hero the game disables (in development). */
export type HeroTag = {text:string;tone:'game'|'alternate'|'unplayable'};
export function heroTag(hero:HeroRecord): HeroTag|null {
  const main=roster.find(item=>item.path.toLowerCase()===normalize(hero.path));
  if(hero.game?.state==='disabled')return {text:'In development',tone:'unplayable'};
  if(main?.playable===false)return {text:'Not playable yet',tone:'unplayable'};
  if(main||hero.game?.state==='active')return {text:'In game',tone:'game'};
  if(hero.game?.state==='unused'||heroLabel(hero).includes(' · '))return {text:'Alternate',tone:'alternate'};
  return null;
}
/** Why a replacement for this model would not be seen in a match, if it would not. */
export function availabilityNote(hero:HeroRecord): string|null {
  const name=heroLabel(hero).split(' · ')[0];
  if(hero.game?.state==='disabled')return `${name} is a hero the game has switched off (in development). A replacement would not be seen in a match.`;
  if(roster.find(item=>item.path.toLowerCase()===normalize(hero.path))?.playable===false)return `${name}'s files are in the game, but ${name} cannot be picked in a match yet. A replacement will only be seen once the hero is released.`;
  return null;
}
export function heroLabel(hero:HeroRecord): string {
  // The installed game's hero definitions decide which model is the playable body.
  if(hero.game?.state==='active'&&hero.hero)return hero.hero;
  if(hero.game?.state==='unused')return `${hero.hero??staticLabel(hero)} · Not used in game`;
  return staticLabel(hero);
}
function staticLabel(hero:HeroRecord): string {
  const path=hero.path.replace(/\\/g,'/').replace(/_c$/,'').toLowerCase();
  const known=roster.find(item=>item.path.toLowerCase()===path); if(known)return known.name;
  for(const item of roster){const extra=item.extras?.find(candidate=>candidate.path.toLowerCase()===path);if(extra)return `${item.name} · ${extra.name}`;}
  const parts=path.split('/');const ownerIndex=parts.findIndex(part=>['heroes','heroes_staging','heroes_wip'].includes(part));
  const owner=ownerIndex>=0?parts[ownerIndex+1]:'';const stem=parts.at(-1)?.replace(/\.vmdl$/,'')??'';
  const name=aliases[owner]?.[0]??aliases[owner.replace(/_v\d+$/,'')]?.[0]??aliases[stem]?.[0];
  if(!name)return hero.name;
  const alias=Object.keys(aliases).filter(key=>aliases[key][0]===name).sort((a,b)=>b.length-a.length).find(key=>stem===key||stem.startsWith(key+'_'));
  const suffix=(alias?stem.slice(alias.length):stem).replace(/^_+/,'').replace(/_/g,' ');
  return suffix?`${name} · ${suffix}`:roster.some(item=>item.name===name)?`${name} · Alternate model`:name;
}
export function alternateTarget(path:string): {name:string;path:string}|undefined {
  if(!path)return undefined;
  const normalized=path.replace(/\\/g,'/').replace(/_c$/,'').toLowerCase();
  if(roster.some(item=>item.path.toLowerCase()===normalized))return undefined;
  const label=heroLabel({path,name:''} as HeroRecord);
  return roster.find(item=>label.startsWith(item.name+' · '));
}
export function HeroPicker({current,disabled,select,fail}:{current:string;disabled:boolean;select:(hero:HeroRecord)=>void;fail:(title:string,error:unknown)=>void}){
  const [catalog,setCatalog]=useState<HeroRecord[]>([]),[query,setQuery]=useState(''),[expanded,setExpanded]=useState(false),[loading,setLoading]=useState(false),[all,setAll]=useState(false),[loaded,setLoaded]=useState(false);
  const alternate=alternateTarget(current);
  const currentRecord=catalog.find(hero=>hero.path.replace(/_c$/,'').toLowerCase()===current.replace(/_c$/,'').toLowerCase());
  const unused=currentRecord?.game?.state==='unused'?currentRecord.game.model:null;
  const note=current?availabilityNote(currentRecord??{path:current,name:'',likely_hero:true}):null;
  const gameModel=unused?catalog.find(hero=>hero.path.replace(/_c$/,'').toLowerCase()===unused):undefined;
  const matches=useMemo(()=>catalog.filter(hero=>`${heroLabel(hero)} ${hero.name} ${hero.path}`.toLowerCase().includes(query.toLowerCase())).sort((a,b)=>heroLabel(a).localeCompare(heroLabel(b))),[catalog,query]);
  async function load(includeAll=all){setLoading(true);try{setCatalog(await api.heroes(includeAll));setLoaded(true);setExpanded(true);}catch(error){fail('Could not find characters',error);}finally{setLoading(false);}}
  return <div className="hero-picker"><div className="row"><span className="muted">{current?'Change the selected character':'Select a character from the game archive'}</span><button aria-expanded={expanded} disabled={disabled||loading} onClick={()=>{if(expanded)setExpanded(false);else if(loaded)setExpanded(true);else void load();}}>{loading?'Reading archive…':expanded?'Close selector':'Choose character…'}</button></div>{unused?<p className="hint" role="alert">The installed game loads <span className="mono">{unused}</span> for {currentRecord?.hero??'this character'}, not this model. A replacement for it would not be visible in game.{gameModel&&<> <button disabled={disabled} onClick={()=>select(gameModel)}>Use the game's model</button></>}</p>:alternate&&<p className="hint">This is an alternate {alternate.name} model and may not appear during normal gameplay. The main character target is <span className="mono">{alternate.path}</span>.</p>}{note&&<p className="hint" role="status">{note}</p>}{expanded&&<div className="hero-selector"><div className="toolbar"><input aria-label="Search characters" autoFocus value={query} placeholder="Search characters or model paths…" onChange={event=>setQuery(event.target.value)}/><label className="checkbox"><input type="checkbox" checked={all} disabled={disabled||loading} onChange={event=>{setAll(event.target.checked);void load(event.target.checked);}}/>All models</label><button disabled={disabled||loading} onClick={()=>void load()}>Refresh</button></div><ul className="hero-results" aria-label="Available characters">{matches.slice(0,300).map(hero=><li key={hero.path}><button disabled={disabled||loading} className={hero.path===current?'selected':''} onClick={()=>{select(hero);setExpanded(false);}}><strong>{heroLabel(hero)}{(()=>{const tag=heroTag(hero);return tag&&<span className={`hero-tag ${tag.tone}`}>{tag.text}</span>;})()}</strong><span className="mono">{hero.path}</span></button></li>)}</ul>{!matches.length&&<p className="hint">No matching model found in the configured game archive.</p>}{matches.length>300&&<p className="hint">Showing 300 of {matches.length.toLocaleString()} matches. Narrow the search to find a specific model.</p>}</div>}</div>;
}
