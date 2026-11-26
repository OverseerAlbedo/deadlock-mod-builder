import { useMemo, useState } from 'react';
import { T } from '../i18n/T';
import aliasesData from '../../resources/catalog/hero_aliases.json';
import rosterData from '../../resources/catalog/hero_roster.json';
import type { HeroRecord } from '../models';
import { api } from '../api';
import { m, mc, t, tc, tp, useT, type Values } from '../i18n';
import { shownHero, useGameNames } from '../i18n/gameNames';
// i18n-area: shell

const aliases:Record<string,string[]> = aliasesData;
const roster = rosterData as {name:string;path:string;extras?:{name:string;path:string}[];playable?:boolean}[];
const normalize = (path:string) => path.replace(/\\/g,'/').replace(/_c$/,'').toLowerCase();
/** What a model is in the game: the model a hero uses in matches, another model of a hero
 * (alternate), a hero whose files are in the game but who cannot be picked yet (the game's
 * pre-release state; the roster flag is the fallback without game data), or a hero the game
 * disables (in development). */
export type HeroTag = {text:string;tone:'game'|'alternate'|'unplayable'};
export function heroTag(hero:HeroRecord): HeroTag|null {
  const main=roster.find(item=>item.path.toLowerCase()===normalize(hero.path));
  // English (also compared); drawn translated.
  if(hero.game?.state==='disabled')return {text:m('In development'),tone:'unplayable'};
  if(hero.game?.state==='pre_release'||(!hero.game&&main?.playable===false))return {text:m('Not playable yet'),tone:'unplayable'};
  if(main||hero.game?.state==='active')return {text:m('In game'),tone:'game'};
  if(hero.game?.state==='unused'||heroLabel(hero).includes(' · '))return {text:m('Alternate'),tone:'alternate'};
  return null;
}
/** Why a replacement for this model would not be seen in a match, if it would not. */
export function availabilityNote(hero:HeroRecord): string|null {
  const note=availabilityMessage(hero);
  return note&&note.text.replace(/\{name\}/g,String(note.values.name));
}
/** The same note as English text with its value, translated where it is drawn. */
function availabilityMessage(hero:HeroRecord): {text:string;values:Values}|null {
  const name=heroLabel(hero).split(' · ')[0];
  if(hero.game?.state==='disabled')return {text:m('{name} is a hero the game has switched off (in development). A replacement would not be seen in a match.'),values:{name}};
  if(hero.game?.state==='pre_release'||(!hero.game&&roster.find(item=>item.path.toLowerCase()===normalize(hero.path))?.playable===false))return {text:m("{name}'s files are in the game, but {name} cannot be picked in a match yet. A replacement will only be seen once the hero is released."),values:{name}};
  return null;
}
/** A picker label drawn with the game's own name of the hero: the main name in the interface
 * language, then the English main name (smaller, so English guides still match), then the rest of
 * the label as it is. Without a name of the interface language: the English label alone. */
export function ShownHeroLabel({label}:{label:string}){
  const at=label.indexOf(' · '),main=at<0?label:label.slice(0,at),rest=shownLabelRest(at<0?'':label.slice(at)),shown=shownHero(main);
  if(shown===main)return <>{main}{rest}</>;
  return <>{shown}<span className="hero-english" lang="en">{main}</span>{rest}</>;
}
/** The words after a hero label's name that this file writes itself (`heroLabel`), as drawn; names
 * from the roster or a file name stay as they are. The label itself stays English (it is compared). */
const LABEL_WORDS:Record<string,string>={'Alternate model':mc('hero label','Alternate model'),'Not used in game':mc('hero label','Not used in game')};
export function shownLabelRest(rest:string):string{
  const words=rest.slice(3);
  return rest.startsWith(' · ')&&Object.prototype.hasOwnProperty.call(LABEL_WORDS,words)?` · ${tc('hero label',LABEL_WORDS[words])}`:rest;
}
/** A hero label as text: the game's own name of the hero, then the rest as `shownLabelRest` draws it. */
export function shownPickerLabel(label:string):string{
  const at=label.indexOf(' · ');
  return at<0?shownHero(label):shownHero(label.slice(0,at))+shownLabelRest(label.slice(at));
}
export function heroLabel(hero:HeroRecord): string {
  // The installed game's hero definitions decide which model is the playable body.
  if((hero.game?.state==='active'||hero.game?.state==='pre_release')&&hero.hero)return hero.hero;
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
/** The line at the top of the copy's hero list: what the choice is for. */
export const COPY_PICKER_LINE = m('Choose the hero for the copy: it opens as a new project, and the project open now stays as it is.');
/** `copy`: the picker of "Make a copy for another hero" (the current hero cannot be chosen, and
 * the notes about the current hero are left to the main picker). */
export function HeroPicker({current,disabled,select,fail,copy=false}:{current:string;disabled:boolean;select:(hero:HeroRecord)=>void;fail:(title:string,error:unknown)=>void;copy?:boolean}){
  useT();
  const gameNames=useGameNames();
  const [catalog,setCatalog]=useState<HeroRecord[]>([]),[query,setQuery]=useState(''),[expanded,setExpanded]=useState(false),[loading,setLoading]=useState(false),[all,setAll]=useState(false),[loaded,setLoaded]=useState(false);
  const alternate=alternateTarget(current);
  const currentRecord=catalog.find(hero=>hero.path.replace(/_c$/,'').toLowerCase()===current.replace(/_c$/,'').toLowerCase());
  const unused=currentRecord?.game?.state==='unused'?currentRecord.game.model:null;
  const note=current?availabilityMessage(currentRecord??{path:current,name:'',likely_hero:true}):null;
  const gameModel=unused?catalog.find(hero=>hero.path.replace(/_c$/,'').toLowerCase()===unused):undefined;
  // Found by the English label, the model name, the path, or the game's name in the interface
  // language; always sorted on the English label.
  const matches=useMemo(()=>catalog.filter(hero=>{const label=heroLabel(hero),main=label.split(' · ')[0],shown=shownHero(main);return `${label} ${hero.name} ${hero.path}${shown===main?'':` ${shown}`}`.toLowerCase().includes(query.toLowerCase());}).sort((a,b)=>heroLabel(a).localeCompare(heroLabel(b))),[catalog,query,gameNames]);
  async function load(includeAll=all){setLoading(true);try{setCatalog(await api.heroes(includeAll));setLoaded(true);setExpanded(true);}catch(error){fail(m('Could not find characters'),error);}finally{setLoading(false);}}
  return <div className={`hero-picker${copy?' hero-copy-picker':''}`}><div className="row"><span className="muted">{copy?t('The same character on another hero, as a new project'):current?t('Change the selected character'):t('Select a character from the game archive')}</span><button aria-expanded={expanded} disabled={disabled||loading} onClick={()=>{if(expanded)setExpanded(false);else if(loaded)setExpanded(true);else void load();}}>{loading?t('Reading archive…'):expanded?t('Close selector'):copy?t('Make a copy for another hero…'):t('Choose character…')}</button></div>{copy?null:unused?<p className="hint" role="alert">{currentRecord?.hero?<T text="The installed game loads <code>{model}</code> for {hero}, not this model. A replacement for it would not be visible in game." values={{model:unused,hero:shownHero(currentRecord.hero)}} tags={{code:chunk=><span className="mono">{chunk}</span>}}/>:<T text="The installed game loads <code>{model}</code> for this character, not this model. A replacement for it would not be visible in game." values={{model:unused}} tags={{code:chunk=><span className="mono">{chunk}</span>}}/>}{gameModel&&<> <button disabled={disabled} onClick={()=>select(gameModel)}>{t("Use the game's model")}</button></>}</p>:alternate&&<p className="hint"><T text="This is an alternate {hero} model and may not appear during normal gameplay. The main character target is <code>{path}</code>." values={{hero:shownHero(alternate.name),path:alternate.path}} tags={{code:chunk=><span className="mono">{chunk}</span>}}/></p>}{!copy&&note&&<p className="hint" role="status">{t(note.text,{...note.values,name:shownHero(String(note.values.name))})}</p>}{expanded&&<div className="hero-selector">{copy&&<p className="hint">{t(COPY_PICKER_LINE)}</p>}<div className="toolbar"><input aria-label={copy?t('Search heroes for the copy'):t('Search characters')} autoFocus value={query} placeholder={t('Search characters or model paths…')} onChange={event=>setQuery(event.target.value)}/><label className="checkbox"><input type="checkbox" checked={all} disabled={disabled||loading} onChange={event=>{setAll(event.target.checked);void load(event.target.checked);}}/>{t('All models')}</label><button disabled={disabled||loading} onClick={()=>void load()}>{t('Refresh')}</button></div><ul className="hero-results" aria-label={copy?t('Heroes for the copy'):t('Available characters')}>{matches.slice(0,300).map(hero=><li key={hero.path}><button disabled={disabled||loading||(copy&&hero.path===current)} className={hero.path===current?'selected':''} onClick={()=>{select(hero);setExpanded(false);}}><strong><ShownHeroLabel label={heroLabel(hero)}/>{(()=>{const tag=heroTag(hero);return tag&&<span className={`hero-tag ${tag.tone}`}>{t(tag.text)}</span>;})()}</strong><span className="mono">{hero.path}</span></button></li>)}</ul>{!matches.length&&<p className="hint">{t('No matching model found in the configured game archive.')}</p>}{matches.length>300&&<p className="hint">{tp(matches.length,{other:'Showing 300 of {total} matches. Narrow the search to find a specific model.'},{total:matches.length.toLocaleString()})}</p>}</div>}</div>;
}
