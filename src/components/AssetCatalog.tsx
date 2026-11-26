import { useState } from 'react';
import type { AssetKind } from '../models';
import { m, tp, useT } from '../i18n';
// i18n-area: common
/** Tags, readable name, group and variation family of a sound (Rust `sound_tags::SoundInfo`). */
export interface SoundInfo {name:string;group:string;group_name:string;group_order:number;tags:string[];voice?:string[];family:string;family_source:'event'|'name';variation?:number;events?:string[]}
export interface CatalogEntry {path:string;target:string;label:string;supported:boolean;reason?:string;original_sha256?:string|null;shared?:boolean;association?:string;width?:number;height?:number;wav_supported?:boolean;sound?:SoundInfo;[key:string]:unknown}
/** A list group of the Motion page (Rust `clip_names::Group`, in its order). */
export type ClipGroup = 'movement'|'jumping'|'weapon'|'abilities'|'hits'|'other';
/** A direction a clip moves in (Rust `clip_names::Direction`, from its root motion). */
export type ClipDirection = 'forward'|'forward_right'|'right'|'backward_right'|'backward'|'backward_left'|'left'|'forward_left'|'in_place';
/** One of the hero's animation clips (Rust `service::AnimationDiscovery`: `motion::AnimationRow`,
 * its `Summary` and `motion::ClipDetails`). Names and groups are made in Rust at discovery. */
export interface AnimationEntry extends CatalogEntry {skeleton:string;frames:number;duration:number;tracks:number;events:number;additive:boolean;name:string;group:ClipGroup;direction?:ClipDirection;/** An animation graph of the hero names this clip. */graph_named:boolean;/** A copy the compiler derived from another clip: that clip's path. */derived_from?:string;/** The last stored frame equals the first. */ends_where_starts?:boolean}
/** One of the hero's own game materials for Hero colors (Rust `materials::MaterialRow`). */
export interface HeroMaterialEntry {path:string;target:string;label:string;shader:string;supported:boolean;reason:string;tint:[number,number,number]|null;albedo:[number,number,number]|null;tint_masked:boolean;color_texture:string|null;used_by:string[];shared:boolean;original_sha256:string}
export interface ComponentCatalog {hero_path:string;icons:CatalogEntry[];animations:AnimationEntry[];effects:CatalogEntry[];audio:CatalogEntry[];materials?:HeroMaterialEntry[];variants:{path:string;target:string;label:string;hero:string;supported:boolean;requires_model_project:boolean}[];animation_unavailable:string|null;warnings:string[];includes_animations?:boolean}
/** The search box's name per kind (English; drawn translated). */
const SEARCH_LABELS: Record<AssetKind, string> = {icons: m('Search icons destinations'), animations: m('Search animations destinations'), effects: m('Search effects destinations'), audio: m('Search audio destinations')};
export function DestinationPicker({kind,catalog,current,disabled,choose,discover}:{kind:AssetKind;catalog:ComponentCatalog|null;current:string;disabled:boolean;choose:(entry:CatalogEntry)=>void;discover:()=>void}){
  const t=useT();
  const[expanded,setExpanded]=useState(false),[search,setSearch]=useState('');
  const entries=(catalog?.[kind]??[]).filter(entry=>`${entry.label} ${entry.path}`.toLowerCase().includes(search.toLowerCase()));
  return <div className="destination-picker"><div className="row"><button disabled={disabled} onClick={()=>setExpanded(!expanded)}>{expanded?t('Close destination list'):t('Choose game destination…')}</button>{expanded&&<button disabled={disabled} onClick={discover}>{catalog?t('Refresh discovery'):t('Discover destinations')}</button>}</div>{expanded&&<div className="hero-selector"><input aria-label={t(SEARCH_LABELS[kind])} placeholder={t('Search game destinations…')} value={search} onChange={event=>setSearch(event.target.value)}/>{catalog?<><p className="hint">{entries.length>100?tp(entries.length,{other:'{count} matching destinations · Showing first 100; narrow your search'}):tp(entries.length,{other:'{count} matching destinations'})}</p>{kind==='animations'&&catalog.animation_unavailable&&<p className="hint">{catalog.animation_unavailable}</p>}<ul className="hero-results">{entries.slice(0,100).map(entry=><li key={entry.path}><button disabled={disabled||!entry.supported} className={current===entry.target?'selected':''} onClick={()=>{choose(entry);setExpanded(false);}}><strong>{entry.label||entry.path}{entry.shared?` · ${t('Shared resource')}`:''}</strong><span className="mono">{entry.path}</span>{!entry.supported&&<span>{t('Unavailable: {reason}',{reason:entry.reason||t('This resource is not supported for replacement.')})}</span>}{entry.width&&entry.height?<span>{entry.width} × {entry.height}</span>:null}</button></li>)}</ul></>:<p className="hint">{t('Discover destinations from the configured game archive to select an existing resource.')}</p>}</div>}</div>;
}
