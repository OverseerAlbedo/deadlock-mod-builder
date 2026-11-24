import { useEffect, useMemo, useRef, useState } from 'react';
import type { Texture } from 'three';
import type { AppearanceMaterial, FaceLayerKind, JsonValue, Material, Project } from '../models';
import { ImagePreview } from './ImagePreview';
import { api, basename } from '../api';
import { FilePathInput } from './FilePathInput';
import { Check, SelectField, changeCustomization } from './CustomizationPages';
import { Section, StateLabel } from './Primitives';
import { StudioCard } from '../studio/Studio';
import { appearanceRowIndex, changeSummary, colorValues } from '../extras/colorAdapter';
import { addSurface, colorsEdits, includeSurfaces, removeSurface, rowsFor, setSurface, SURFACE_KEYS, SURFACE_LABELS, SURFACE_MAX, SURFACE_START, surfaceLimit, type SurfaceKey } from './surfaceRows';

export function NumberField({label,value,update,disabled,min=0,max=1,step=.01}:{label:string;value:number;update:(value:number)=>void;disabled:boolean;min?:number;max?:number;step?:number}){
  return <label className="number-field"><span>{label}</span><input type="number" value={value} min={min} max={max} step={step} disabled={disabled} onChange={event=>{if(event.target.value!==''&&Number.isFinite(event.target.valueAsNumber))update(event.target.valueAsNumber);}}/></label>;
}
/** A value with a slider and a number box (the box takes any number the field allows; the slider
 * shows it clamped to its range). */
export function SliderField({label,value,update,disabled,min=0,max=1,step=.01}:{label:string;value:number;update:(value:number)=>void;disabled:boolean;min?:number;max?:number;step?:number}){
  return <div className="slider-field"><span className="slider-field-label">{label}</span><input type="range" aria-label={`${label} slider`} min={min} max={max} step={step} value={Math.min(max,Math.max(min,value))} disabled={disabled} onChange={event=>update(Number(event.target.value))}/><input type="number" aria-label={label} value={value} min={min} max={max} step={step} disabled={disabled} onChange={event=>{if(event.target.value!==''&&Number.isFinite(event.target.valueAsNumber))update(event.target.valueAsNumber);}}/></div>;
}
export function ColorField({label,value,update,disabled}:{label:string;value:[number,number,number];update:(value:[number,number,number])=>void;disabled:boolean}){
  const hex='#'+value.map(x=>Math.min(255,Math.max(0,Math.round(x*255))).toString(16).padStart(2,'0')).join('');
  return <div className="color-field"><label>{label}<input type="color" aria-label={label} value={hex} disabled={disabled} onChange={event=>{const text=event.target.value;update([parseInt(text.slice(1,3),16)/255,parseInt(text.slice(3,5),16)/255,parseInt(text.slice(5,7),16)/255]);}}/></label><span className="mono">{hex.toUpperCase()}</span></div>;
}
const channels=[['color','Base color'],['normal','Normal'],['roughness','Roughness'],['metalness','Metalness'],['ao','Ambient occlusion'],['emission','Emission'],['opacity','Opacity']] as const;
const MAIN_CHANNELS=4;
const presets=['generic','cloth','painted metal','bare metal','skin','hair','emissive','transparent'];
type Props={project:Project;update:(project:Project)=>void;disabled:boolean;fail:(title:string,error:unknown)=>void};
const rgb=(color:[number,number,number]|undefined|null)=>color?`rgb(${color.map(x=>Math.round(Math.min(1,Math.max(0,x))*255)).join(' ')})`:undefined;

/** A material Build & install still asks about: generated, and not reviewed or with several
 * images to choose from (the same rule as `readiness.ts materialReviewCount`). */
export const needsLook=(material:Material)=>material.mode==='generate'&&(!material.confirmed||Boolean(material.ambiguous?.length));
/** The word a material's row and label show, and its tone. */
export function materialStatus(material:Material):{text:string;tone:'ok'|'attention'|'quiet'}{
  if(material.mode==='keep')return material.confirmed?{text:'Reviewed',tone:'ok'}:{text:'Keep source VMAT',tone:'quiet'};
  // Neither generated nor kept (an older saved row): Build & install does not ask about it.
  if(material.mode!=='generate')return material.confirmed?{text:'Reviewed',tone:'ok'}:{text:'Saved',tone:'quiet'};
  if(material.ambiguous?.length)return {text:'Choose an image',tone:'attention'};
  if(!material.confirmed)return !material.textures?.color&&!material.candidates?.color?.length?{text:'No image found',tone:'attention'}:{text:'Needs review',tone:'attention'};
  return {text:'Reviewed',tone:'ok'};
}
/** Where an image choice came from, as preparation recorded it (`texture_match_sources`). */
function matchSource(material:Material,key:string):string{
  const sources=material.texture_match_sources;
  if(sources&&typeof sources==='object'&&!Array.isArray(sources)){const value=(sources as Record<string,unknown>)[key];if(typeof value==='string')return value;}
  return '';
}
/** The material's `texture_match_sources` without `key` (the key removed when it empties), as a
 * partial update; nothing when there was no label for it. */
function withoutSource(material:Material|null,key:string):Partial<Material>{
  const sources=material?.texture_match_sources;
  if(!sources||typeof sources!=='object'||Array.isArray(sources)||!(key in sources))return {};
  const rest={...sources as Record<string,JsonValue>};delete rest[key];
  return {texture_match_sources:Object.keys(rest).length?rest:undefined};
}

/** A material's picture: its base-color image when the 3D view has loaded it (`texture`, drawn
 * small, nothing loaded for it), else its base tint. */
export function MaterialThumb({texture,material,size=36,label}:{texture?:Texture;material?:Material;size?:number;label?:string}){
  const canvas=useRef<HTMLCanvasElement>(null);
  useEffect(()=>{
    const element=canvas.current,image=texture?.image as CanvasImageSource&{width?:number;height?:number;naturalWidth?:number;naturalHeight?:number}|undefined;
    const context=element?.getContext?.('2d');
    if(!element||!context||!image)return;
    const width=image.naturalWidth||image.width||0,height=image.naturalHeight||image.height||0;
    if(!width||!height)return;
    const side=Math.min(width,height);
    try{context.clearRect(0,0,element.width,element.height);context.drawImage(image,(width-side)/2,(height-side)/2,side,side,0,0,element.width,element.height);}catch{/* an image that cannot be drawn keeps the tint */}
  },[texture]);
  const image=Boolean(material?.textures?.color);
  if(texture)return <canvas ref={canvas} className="material-thumb" width={size*2} height={size*2} style={{width:size,height:size}} role="img" aria-label={label??'Base color image'}/>;
  return <span className={`material-thumb ${material?.mode!=='keep'&&!image?'no-image':''} ${image?'has-image':''}`} style={{width:size,height:size,background:material?.mode==='keep'?undefined:image?undefined:rgb(material?.color??[.5,.5,.5])}} title={image?'Image assigned. Show Textures in the 3D view to see it here.':undefined} aria-hidden="true"/>;
}

export function MaterialEditor({project,update,disabled,fail,analyze,checked=false,selected:chosen,select:choose,thumbs,openColors,labelHint}:Props&{analyze:()=>void;checked?:boolean;
  /** The selected material, when the page holds it (the 3D view picks it too). */selected?:string|null;select?:(name:string|null)=>void;
  /** Loaded base-color images by material (from the 3D view), for the pictures in the list. */thumbs?:Record<string,Texture>;
  /** Opens Extras → Colors. */openColors?:()=>void;
  /** A line under the search (the page: "or click a part of the character"). */labelHint?:string}){
  const [own,setOwn]=useState<string|null>(null),[search,setSearch]=useState('');
  const names=Object.keys(project.materials);
  const selected=chosen!==undefined?chosen:own, setSelected=choose??setOwn;
  const materials=useMemo(()=>Object.keys(project.materials).filter(name=>name.toLowerCase().includes(search.toLowerCase())),[project.materials,search]);
  const active=selected?project.materials[selected]:null;
  const edit=(values:Partial<Material>)=>{if(selected&&active)update({...project,materials:{...project.materials,[selected]:{...active,...values}}});};
  // An image set by hand (a file, a listed candidate or no image) is the user's choice: the
  // preparation's "Found by" label no longer describes the channel, so it is dropped.
  const editTexture=(key:string,source:string)=>edit({textures:{...active?.textures,[key]:source},ambiguous:(active?.ambiguous??[]).filter(channel=>channel!==key),confirmed:false,...withoutSource(active,key)});
  async function candidate(key:string,value:string){if(!value)return;try{editTexture(key,value==='__none__'?'':await api.validateAssetPath(value,'texture'));}catch(error){fail('Texture could not be selected',error);}}
  const count=names.length;
  const assigned=Object.values(project.materials).reduce((sum,m)=>sum+Object.values(m.textures??{}).filter(Boolean).length,0);
  const review=Object.values(project.materials).filter(needsLook).length;
  const canAccept=(material:Material)=>!material.confirmed&&(material.mode==='keep'||!material.ambiguous?.length);
  const pending=Object.values(project.materials).filter(canAccept).length;
  const unresolved=Object.values(project.materials).filter(material=>material.mode!=='keep'&&Boolean(material.ambiguous?.length)).length;
  function acceptAll(){if(disabled||!pending)return;update({...project,materials:Object.fromEntries(Object.entries(project.materials).map(([name,material])=>[name,canAccept(material)?{...material,confirmed:true}:material]))});}
  const prerequisite=!project.original_model||!project.dmx;
  // Previous: the material above in the list; Next to review: the next one Build & install still asks about.
  const at=selected?names.indexOf(selected):-1;
  const previous=at>0?names[at-1]:null;
  const nextReview=[...names.slice(at+1),...names.slice(0,Math.max(0,at))].find(name=>name!==selected&&needsLook(project.materials[name]))??null;
  const appearance=project.customization.appearance??{};
  const list=<StudioCard className="material-list-card" title="Materials" badge={count?String(count):undefined}
    actions={<><button className={pending?'accept-all':undefined} disabled={disabled||!pending} onClick={acceptAll} title="Recommended: check the list first">Accept all</button><button disabled={disabled||!project.dmx||!project.original_model} onClick={analyze}>Analyze materials</button></>}>
    <div className="materials-summary"><StateLabel tone={review?'attention':checked?'ready':'neutral'}>{count?`${count} assignments${review?` · ${review} need review`:checked?' checked':' saved'}`:'No material assignments'}</StateLabel><span>{assigned} texture assignments</span></div>
    {prerequisite&&<p className="hint">Choose a character and replacement model first to analyze materials.</p>}
    {pending>0&&<p className="materials-accept-callout" role="status"><strong>{pending} {pending===1?'material is':'materials are'} ready.</strong> Check the list, then click <strong>Accept all</strong> (recommended).</p>}
    {count>0&&<p className="hint">Accept all confirms the current material and image choices across the entire list.{unresolved>0?` ${unresolved} material${unresolved===1?'':'s'} still need${unresolved===1?'s':''} a choice between multiple images.`:''}</p>}
    {!count?<p className="quiet-empty">Choose a character and model, then analyze materials to find their textures.</p>:<>
      <input className="material-search" aria-label="Search materials" placeholder="Search materials…" value={search} onChange={event=>setSearch(event.target.value)}/>
      {labelHint&&<p className="hint material-list-hint">{labelHint}</p>}
      <ul className="material-list">{materials.map(name=>{const material=project.materials[name],status=materialStatus(material);return <li key={name}><button className={selected===name?'selected':''} aria-current={selected===name?'true':undefined} disabled={disabled} onClick={()=>setSelected(name)}>
        <MaterialThumb texture={thumbs?.[name]} material={material}/>
        <span className="material-row-text"><span className="file-name" title={name}>{basename(name)}</span><span className={`material-status ${status.tone}`}>{status.text}</span>
        <small>{Object.values(material.textures??{}).filter(Boolean).length} textures · {material.mode==='keep'?'Keep source material':material.preset??'generic'}</small></span></button></li>;})}</ul></>}
    <div className="material-list-options">
      <Check label="Generate missing materials" value={project.generate_materials} onChange={generate_materials=>update({...project,generate_materials})} disabled={disabled}/>
      <Check label="Include surface changes" value={appearance.enabled??false} onChange={enabled=>update(includeSurfaces(project,enabled))} disabled={disabled}/>
      <button type="button" className="tertiary" disabled={disabled||(appearance.materials?.length??0)>=surfaceLimit} title="A surface change you point at a material yourself; it is listed under Other saved surface changes" onClick={()=>update(changeCustomization(project,{appearance:{...appearance,enabled:true,materials:[...(appearance.materials??[]),{enabled:true,target:''}]}}))}>Add a surface change by hand</button>
      <p className="hint">Analyze materials recovers image links from the prepared Blender model and matches files in your texture folder. Existing choices are preserved; multiple matches need a choice.</p>
    </div>
  </StudioCard>;
  const channel=(key:typeof channels[number][0],label:string)=>{const candidates=active?.candidates?.[key]??[],source=active?matchSource(active,key):'',current=active?.textures?.[key]??'';
    // A reviewed choice next to the one image the model links (Prepare again keeps the
    // choice and lists that image): a quiet offer, never a review item.
    const other=active?.confirmed&&current&&candidates.length===1&&candidates[0]!==current?candidates[0]:null;
    return <div className="material-channel" key={key}>
    <MaterialThumb texture={key==='color'&&selected?thumbs?.[selected]:undefined} material={key==='color'?active??undefined:undefined} size={28}/>
    <div className="material-channel-fields"><FilePathInput label={label} kind="texture" value={active?.textures?.[key]??''} update={path=>editTexture(key,path)} disabled={disabled} fail={fail}/>
      {candidates.length>0&&<>{candidates.length>1&&<p className="hint">Multiple {label.toLowerCase()} images found. Choose explicitly or use no image.</p>}<SelectField label={`${label} candidates`} value="" values={[["","Choose a candidate…"],["__none__","Use no image"],...candidates.map(path=>[path,path] as const)]} onChange={path=>void candidate(key,path)} disabled={disabled}/></>}
      {source&&active?.textures?.[key]&&<p className="hint material-source">Found by: {source}</p>}
      {other&&<p className="hint material-other-image">The preparation found another image for this material: {basename(other)}.<button type="button" className="tertiary" disabled={disabled} onClick={()=>void candidate(key,other)}>Use it</button></p>}
      {active?.textures?.[key]&&<ImagePreview path={active.textures[key]} disabled={disabled} fail={fail}/>}</div>
  </div>;};
  const more=channels.slice(MAIN_CHANNELS),moreOpen=Boolean(active&&more.some(([key])=>active.ambiguous?.includes(key)||(active.candidates?.[key]?.length??0)>0));
  const editor=active&&selected?<StudioCard className="material-inspector" title={basename(selected)} actions={<button className="tertiary" onClick={()=>setSelected(null)}>Close</button>}>
    <div className="material-inspector-head"><MaterialThumb texture={thumbs?.[selected]} material={active} size={56}/><div><p className="path mono">{selected}</p>{active.mode!=='keep'&&<p className="hint">{active.textures?.color?`Base color image: ${basename(active.textures.color)}`:'No base-color image selected. This material currently uses its base tint.'}</p>}</div></div>
    {active.mode!=='keep'&&<>
      <h4>Images</h4>
      <div className="material-channels">{channels.slice(0,MAIN_CHANNELS).map(([key,label])=>channel(key,label))}</div>
      <details className="material-more-images" open={moreOpen||undefined}><summary>More images: {more.map(([key,label])=><span key={key} className="material-more-chip">{label} <small>{active.textures?.[key]?'set':'none'}</small></span>)}</summary><div className="material-channels">{more.map(([key,label])=>channel(key,label))}</div></details>
      <p className="hint">Each image offers the candidates found for this material, “Use no image”, and your own file.</p>
    </>}
    <h4>Surface</h4>
    <SelectField label="Material handling" value={active.mode??'generate'} values={[["generate","Generate from these choices"],["keep","Keep source VMAT"]]} onChange={mode=>edit({mode,confirmed:false})} disabled={disabled}/>
    {active.mode!=='keep'&&<><SelectField label="Surface preset" value={active.preset??'generic'} values={presets.map(preset=>[preset,preset])} onChange={preset=>edit({preset,confirmed:false})} disabled={disabled}/>{(active.preset??'generic')==='generic'&&<p className="hint">Generic is a neutral fallback. It does not reproduce Blender procedural shading or infer colors.</p>}<ColorField label="Base tint" value={active.color??[.5,.5,.5]} update={color=>edit({color,confirmed:false})} disabled={disabled}/>{(['roughness','metalness'] as const).map(key=><div className="material-value" key={key}><Check label={`Override ${key}`} value={active[key]!=null} onChange={enabled=>edit({[key]:enabled?.5:null,confirmed:false})} disabled={disabled}/>{active[key]!=null&&<SliderField label={key} value={active[key]!} update={value=>edit({[key]:value,confirmed:false})} disabled={disabled}/>}</div>)}</>}
    <SurfaceOnTop project={project} material={selected} update={update} disabled={disabled} openColors={openColors}/>
    <div className="material-inspector-foot"><Check label="I have reviewed this material choice" value={active.confirmed??false} onChange={confirmed=>edit({confirmed})} disabled={disabled||(active.mode!=='keep'&&Boolean(active.ambiguous?.length))}/>
      <span className="material-walk"><button type="button" disabled={!previous} onClick={()=>previous&&setSelected(previous)}><span aria-hidden="true">←</span> Previous</button><button type="button" className={nextReview?'primary':undefined} disabled={!nextReview} title={nextReview?undefined:'Nothing else to review'} onClick={()=>nextReview&&setSelected(nextReview)}>Next to review <span aria-hidden="true">→</span></button></span></div>
  </StudioCard>:count>0?<StudioCard className="material-inspector empty"><p className="studio-note">Choose a material in the list{labelHint?' or click a part of the character':''} to see its images and settings.</p></StudioCard>:null;
  return <section className="materials-studio" id="materials" aria-label="Materials">
    <div className={`material-layout ${editor?'with-inspector':''}`}>{list}{editor}</div>
    <OtherSurfaceChanges project={project} update={update} disabled={disabled} fail={fail}/>
  </section>;
}

/** The Surface changes row Extras → Colors edits for this material, applied on top of it at build:
 * roughness, metalness, opacity and glow (each only when ticked; unticked keeps the material's own
 * value) and the colour, shown here and edited in Extras → Colors. The material's own generation
 * values above are a separate thing and stay as they are. */
function SurfaceOnTop({project,material,update,disabled,openColors}:{project:Project;material:string;update:(project:Project)=>void;disabled:boolean;openColors?:()=>void}){
  const rows=project.customization.appearance?.materials??[];
  const index=appearanceRowIndex(rows,material);
  const row:AppearanceMaterial|undefined=index>=0?rows[index]:undefined;
  const others=rowsFor(project,material).filter(i=>i!==index).length;
  const colorHere=index>=0&&!colorsEdits(project,index);
  function value(key:SurfaceKey,next:number|undefined){
    if(index>=0){
      const changed=setSurface(project,index,key,next);
      const after=changed.customization.appearance!.materials![index];
      // A row this left with nothing (no value, colour, range or other key, still included) goes, as Extras → Colors drops one.
      const empty=Object.keys(after).every(name=>name==='target'||(name==='enabled'&&after.enabled!==false));
      update(empty?removeSurface(project,index):changed);
    }else if(next!==undefined){const added=addSurface(project,material);if(added)update(setSurface(added,rows.length,key,next));}
  }
  const colors=row?colorValues(row):null;
  return <div className="surface-on-top">
    <h4>Changes on top</h4>
    <p className="hint">Applied by the build on top of this material, also to a kept source VMAT, while Include surface changes is on. Unchecked values preserve the existing material parameter.</p>
    {row&&<Check label="Include this material’s changes" value={row.enabled??true} onChange={enabled=>update(setSurface(project,index,'enabled',enabled))} disabled={disabled}/>}
    {SURFACE_KEYS.map(key=><div className="material-value" key={key}><Check label={`Change ${SURFACE_LABELS[key].toLowerCase()}`} value={row?.[key]!==undefined} onChange={on=>value(key,on?SURFACE_START[key]:undefined)} disabled={disabled||(!row&&rows.length>=surfaceLimit)}/>{row?.[key]!==undefined&&<SliderField label={`${SURFACE_LABELS[key]} on top`} value={row[key] as number} max={SURFACE_MAX[key]} step={key==='emission'?.1:.01} update={next=>value(key,next)} disabled={disabled}/>}</div>)}
    <div className="surface-color"><span className="surface-color-label">Color</span>{colors?.color&&<span className="material-thumb small" style={{background:rgb(colors.color)}} aria-hidden="true"/>}<span className="surface-color-summary">{colors?changeSummary(colors):'Unchanged'}</span>
      {colorHere?<span className="hint">Edited under Other saved surface changes below.</span>:openColors&&<button type="button" disabled={disabled} onClick={openColors}>Edit color in Extras → Colors</button>}</div>
    {!colorHere&&<p className="hint">Extras → Colors edits the color: a tint, hue, saturation and brightness, and color ranges, with your model in 3D.</p>}
    {others>0&&<p className="hint">{others} more saved change{others===1?'':'s'} for this material {others===1?'is':'are'} under Other saved surface changes. The build uses one included change per material.</p>}
    {row&&<button type="button" className="tertiary" disabled={disabled} onClick={()=>update(removeSurface(project,index))}>Remove this material’s changes</button>}
  </div>;
}

/** Saved surface changes the Materials editor does not show through a material: rows without a
 * material, for a material the prepared model no longer has, more rows for one material, and every
 * row while the replacement model is off (Extras → Colors then edits the hero's own colours). They
 * keep the full row editor. Nothing when there are none. */
function OtherSurfaceChanges({project,update,disabled,fail}:Props){
  const rows=project.customization.appearance?.materials??[];
  const only=rows.flatMap((_,index)=>colorsEdits(project,index)?[]:[index]);
  if(!only.length)return null;
  return <AppearanceOverrides project={project} update={update} disabled={disabled} fail={fail} only={only}/>;
}

/** Experimental color editor. The CSS preview approximates the build-time HSV change applied to a copy of the base-color image. */
export function colorFilter(row:AppearanceMaterial){return `hue-rotate(${row.hue??0}deg) saturate(${row.saturation??1}) brightness(${row.brightness??1})`;}
function ColorAdjustment({row,index,image,edit,remove,disabled,fail}:{row:AppearanceMaterial;index:number;image?:string;edit:(values:Partial<AppearanceMaterial>)=>void;remove:()=>void;disabled:boolean;fail:Props['fail']}){
  const active=row.hue!==undefined||row.saturation!==undefined||row.brightness!==undefined;
  const slider=(key:'hue'|'saturation'|'brightness',label:string,min:number,max:number,step:number,neutral:number,format:(value:number)=>string)=><label className="color-slider"><span>{label}</span><input type="range" aria-label={`${label} ${index+1}`} min={min} max={max} step={step} value={row[key]??neutral} disabled={disabled} onChange={event=>edit({[key]:Number(event.target.value)})}/><output>{format(row[key]??neutral)}</output></label>;
  return <div className="color-adjustment"><Check label={`Adjust colors ${index+1} (experimental)`} value={active} onChange={enabled=>enabled?edit({hue:0,saturation:1,brightness:1}):remove()} disabled={disabled}/>
    {active&&<><div className="color-sliders">{slider('hue','Hue shift',-180,180,1,0,v=>`${v}°`)}{slider('saturation','Saturation',0,2,.05,1,v=>`${Math.round(v*100)}%`)}{slider('brightness','Brightness',0,2,.05,1,v=>`${Math.round(v*100)}%`)}</div>
      <button className="tertiary" disabled={disabled} onClick={()=>edit({hue:0,saturation:1,brightness:1})}>Reset colors {index+1}</button>
      {image?<ImagePreview path={image} disabled={disabled} fail={fail} filter={colorFilter(row)} label="Preview adjusted base color"/>:<p className="hint">No base-color image is assigned in Materials; a plain base color is adjusted instead.</p>}
      <p className="hint">Build changes a copy of this material’s base-color image (or its plain color). Your files and other materials stay unchanged. The preview is approximate; check the result in game.</p></>}
  </div>;
}
const faceLayers:[FaceLayerKind,string][]=[['blush','blush'],['tears','tears'],['other','other hidden layers']];
/** The face layer kinds a model has, with their material counts. */
export const faceLayerCounts=(project:Project)=>faceLayers.map(([kind,label])=>({kind,label,count:Object.values(project.materials).filter(m=>m.face_layer===kind).length})).filter(row=>row.count>0);
/** Face layers the source keeps invisible until an expression shows them (MMD blush, tears): hidden
 * unless shown here. Hidden writes false, Shown true (`customization.face_layers`). */
export function FaceDetails({project,update,disabled}:{project:Project;update:(project:Project)=>void;disabled:boolean}){
  const counts=faceLayerCounts(project);
  if(!counts.length)return null;
  const shown=project.customization.face_layers??{};
  const set=(kind:FaceLayerKind,value:boolean)=>update(changeCustomization(project,{face_layers:{...shown,[kind]:value}}));
  return <StudioCard title="Face details" className="face-details" sub="Your model keeps these layers invisible until an expression shows them. Deadlock has no expressions, so they stay hidden unless you show them here. A shown layer keeps the soft edges of its own image.">
    <div id="face-details" className="face-layer-list">{counts.map(({kind,label,count})=><div className={`face-layer ${shown[kind]===true?'shown':''}`} key={kind}>
      <div><strong>{label[0].toUpperCase()+label.slice(1)}</strong><small>{count} material{count===1?'':'s'}</small></div>
      <div className="segmented" role="radiogroup" aria-label={`Show ${label} (${count} material${count===1?'':'s'})`}>
        <button type="button" role="radio" aria-checked={shown[kind]!==true} disabled={disabled} onClick={()=>set(kind,false)}>Hidden</button>
        <button type="button" role="radio" aria-checked={shown[kind]===true} disabled={disabled} onClick={()=>set(kind,true)}>Shown</button>
      </div></div>)}</div>
    <p className="hint">Applied when you build; no new preparation is needed.</p>
  </StudioCard>;
}
/** A static expression baked into the model at preparation (`customization.face`): the switch and
 * a weight per saved shape key. Shape keys come only from saved weights. */
export function StaticExpression({project,update,disabled}:{project:Project;update:(project:Project)=>void;disabled:boolean}){
  const face=project.customization.face??{},weights=face.weights??{},[search,setSearch]=useState('');
  const names=Object.keys(weights),shown=names.filter(name=>name.toLowerCase().includes(search.toLowerCase()));
  const set=(next:Record<string,number>)=>update(changeCustomization(project,{face:{...face,mode:'static',weights:next}}));
  return <StudioCard title="Static expression" className="static-expression" sub="Bakes one expression into the model. It does not add facial animation.">
    <Check label="Bake a static expression into the model" value={face.enabled??false} onChange={enabled=>update(changeCustomization(project,{face:{...face,enabled,mode:'static'}}))} disabled={disabled}/>
    {names.length?<>
      {names.length>8&&<input className="material-search" aria-label="Find a shape key" placeholder={`Find a shape key… (${names.length})`} value={search} onChange={event=>setSearch(event.target.value)}/>}
      <div className={`shape-keys ${face.enabled?'':'off'}`}>{shown.map(name=><SliderField key={name} label={name} value={weights[name]} update={next=>set({...weights,[name]:next})} disabled={disabled}/>)}</div>
      {names.some(name=>weights[name]!==0)&&<button type="button" className="tertiary" disabled={disabled} onClick={()=>set(Object.fromEntries(names.map(name=>[name,0])))}>Reset all</button>}
    </>:<p className="hint">No source shape keys have been discovered. Existing expression weights are preserved when you open and save a project.</p>}
    <p className="hint">A static expression requires model preparation. It does not add facial animation.</p>
  </StudioCard>;
}
/** The full Surface changes row editor. On the page it lists only the rows Extras → Colors does not
 * edit (`only`, as "Other saved surface changes"); without `only` every row, as before. */
export function AppearanceOverrides({project,update,disabled,fail,only}:Props&{only?:number[]}){
  const appearance=project.customization.appearance??{},rows=appearance.materials??[];
  const setRows=(materials:AppearanceMaterial[])=>update(changeCustomization(project,{appearance:{...appearance,enabled:true,materials}}));
  const edit=(index:number,values:Partial<AppearanceMaterial>)=>setRows(rows.map((row,i)=>i===index?{...row,...values}:row));
  const removeValue=(index:number,key:keyof AppearanceMaterial)=>{const item={...rows[index]};delete item[key];setRows(rows.map((row,i)=>i===index?item:row));};
  const shown=only?new Set(only):null;
  return <Section title={only?'Other saved surface changes':'Surface changes'} className={only?'other-surface-changes':''} actions={<button disabled={disabled||rows.length>=256} onClick={()=>setRows([...rows,{enabled:true,target:''}])}>Change a material</button>}>{!only&&<Check label="Include surface changes" value={appearance.enabled??false} onChange={enabled=>update(changeCustomization(project,{appearance:{...appearance,enabled}}))} disabled={disabled}/>}<p className="hint">{only?'Changes without a material of your model, more than one for a material, or saved while the replacement model is off. Extras → Colors does not edit these, so their colors are edited here.':'Extras → Colors edits these colors with your model in 3D, a color picker and color ranges.'}</p>{rows.map((row,index)=>shown&&!shown.has(index)?null:<div className="asset-editor" key={index}><div className="section-heading"><Check label={`Include material change ${index+1}`} value={row.enabled??true} onChange={enabled=>edit(index,{enabled})} disabled={disabled}/><button className="tertiary" onClick={()=>setRows(rows.filter((_,i)=>i!==index))} disabled={disabled}>Remove</button></div><SelectField label={`Material ${index+1}`} value={row.target??''} values={[["","Choose a saved material…"],...[...new Set([...Object.keys(project.materials),...(row.target?[row.target]:[])])].map(name=>[name,name] as const)]} onChange={target=>edit(index,{target})} disabled={disabled}/><Check label={`Apply tint ${index+1}`} value={Boolean(row.color)} onChange={enabled=>enabled?edit(index,{color:[1,1,1]}):removeValue(index,'color')} disabled={disabled}/>{row.color&&<ColorField label={`Tint ${index+1}`} value={row.color} update={color=>edit(index,{color})} disabled={disabled}/>}<div className="override-grid">{(['roughness','metalness','opacity','emission'] as const).map(key=><div key={key}><Check label={`Override ${key} ${index+1}`} value={row[key]!==undefined} onChange={enabled=>enabled?edit(index,{[key]:key==='opacity'?1:key==='roughness'?.5:0}):removeValue(index,key)} disabled={disabled}/>{row[key]!==undefined&&<NumberField label={`${key} ${index+1}`} value={row[key]!} max={key==='emission'?64:1} update={value=>edit(index,{[key]:value})} disabled={disabled}/>}</div>)}</div><ColorAdjustment row={row} index={index} image={row.target?project.materials[row.target]?.textures?.color||undefined:undefined} edit={values=>edit(index,values)} remove={()=>{const item={...rows[index]};delete item.hue;delete item.saturation;delete item.brightness;setRows(rows.map((value,i)=>i===index?item:value));}} disabled={disabled} fail={fail}/>{(row.ranges?.length??0)>0&&<p className="hint">{row.ranges!.length} color range{row.ranges!.length===1?'':'s'} from Extras → Colors; edit them there.</p>}<p className="hint">Unchecked values preserve the existing material parameter.</p></div>)}</Section>;
}
