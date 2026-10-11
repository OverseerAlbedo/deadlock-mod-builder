import { useState } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
import { api, basename, type WeaponModel } from '../api';
import type { Project, VariantChoices } from '../models';
import { Check } from './CustomizationPages';
import { FilePathInput } from './FilePathInput';

export const weaponLike=(path:string)=>/weapon|gun|pistol|rifle|shotgun|revolver|smg|blaster|launcher|cannon|wpn|firearm/i.test(basename(path))||/\/weapons\//i.test(path);

/** Heroes whose gun is a separate model: build that model from the character file in its own project. */
export function WeaponModels({project,variants,change,disabled,fail}:{project:Project;variants:VariantChoices[];change:(variants:VariantChoices[])=>void;disabled:boolean;fail:(title:string,error:unknown)=>void}){
  const [found,setFound]=useState<WeaponModel[]|null>(null),[path,setPath]=useState(''),[working,setWorking]=useState(false);
  const rows=variants.map((row,index)=>({row,index})).filter(({row})=>weaponLike(row.model??''));
  async function discover(){setWorking(true);try{const list=await api.discoverWeaponModels(project);setFound(list);if(list.length&&!path)setPath(list[0].path);}catch(error){fail('Weapon models could not be listed',error);}finally{setWorking(false);}}
  async function create(model:string){
    setWorking(true);
    try{
      const draft=(await api.newProject()).project;
      const child=await api.selectHero({...draft,name:`${project.name||'character'}_weapon`},model);
      const options=project.preparation_options as Record<string,unknown>;
      const prepared:Project={...child,model_input_mode:'weapon',preparation_source:project.preparation_source,
        preparation_options:{...(options.weapon?{weapon:{...(options.weapon as object),enabled:true,action:'replace'}}:{}),...(options.scene?{scene:options.scene as Project['preparation_options'][string]}:{})},
        vpk_number:project.vpk_number,install:false,compile_model:true,build_vpk:true};
      const file=await save({title:'Save the weapon project',defaultPath:`${prepared.name}.dmb.json`,filters:[{name:'Deadlock Mod Builder project',extensions:['json']}]});
      if(!file)return;
      const saved=await api.saveProject(file,prepared);
      change([...variants,{enabled:true,model,project:saved.path??file}]);
    }catch(error){fail('The weapon project could not be created',error);}
    finally{setWorking(false);}
  }
  const busy=disabled||working;
  return <details className="technical prep-weapon-models"><summary>Weapon model ({rows.length})</summary>
    <p className="hint">Some heroes carry their gun as a separate model. To replace it with the gun in your character file, create a weapon project for that model: it uses the same source file, keeps only the gun and fits it to the original gun's grip, barrel and size. It is built into this mod together with the character. Open the new project afterwards and run Model preparation once.</p>
    <div className="row"><button disabled={busy} onClick={()=>void discover()}>Find this hero's weapon models</button></div>
    {found&&(found.length?<div className="field"><label htmlFor="weapon-model-choice">Weapon model</label><select id="weapon-model-choice" value={path} disabled={busy} onChange={event=>setPath(event.target.value)}>{found.map(item=><option key={item.path} value={item.path}>{item.label} — {item.path}</option>)}</select></div>
      :<p className="hint">No separate weapon model was found next to this hero. Its gun is probably part of the hero model: use the Weapon tab in Model preparation instead.</p>)}
    <label className="field">Or enter a weapon model path<input value={path} disabled={busy} placeholder="models/…/weapon.vmdl" onChange={event=>setPath(event.target.value.trim())}/></label>
    <div className="row"><button disabled={busy||!/\.vmdl(_c)?$/i.test(path)} onClick={()=>void create(path.replace(/_c$/i,''))}>Create weapon project</button></div>
    {rows.map(({row,index})=><div className="asset-editor" key={index}>
      <div className="section-heading"><Check label={`Build weapon model ${basename(row.model??'')}`} value={row.enabled??true} onChange={enabled=>change(variants.map((v,i)=>i===index?{...v,enabled}:v))} disabled={busy}/><button className="tertiary" disabled={busy} onClick={()=>change(variants.filter((_,i)=>i!==index))}>Remove</button></div>
      <p className="path mono">{row.model}</p>
      <FilePathInput label={`Weapon project for ${basename(row.model??'')}`} kind="variant_project" value={row.project??''} update={project=>change(variants.map((v,i)=>i===index?{...v,project}:v))} disabled={busy} fail={fail}/>
    </div>)}
  </details>;
}
