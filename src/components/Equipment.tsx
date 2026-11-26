// i18n-area: appearance
import type { EquipmentMesh, ModelSummary, Project, PropChoices, VariantChoices } from '../models';
import { Check, SelectField, changeCustomization } from './CustomizationPages';
import { NumberField } from './MaterialEditor';
import { FilePathInput } from './FilePathInput';
import { HeroPicker } from './HeroPicker';
import { StudioCard } from '../studio/Studio';
import { weaponLike } from './WeaponModels';
import { m, mc, tc, tp, useT } from '../i18n';

type Props={project:Project;update:(project:Project)=>void;disabled:boolean;fail:(title:string,error:unknown)=>void;summary:ModelSummary|null};
const HANDLING=[["keep",m('Keep original')],["hide",m('Hide')],["replace",m('Replace with prepared meshes')]] as const;
const SHORT={keep:mc('accessory handling','Keep'),hide:mc('accessory handling','Hide'),replace:mc('accessory handling','Replace')} as const;
/** A prop's position and rotation fields, by axis, with the prop's number. */
const VECTOR_LABELS:Record<'translation'|'rotation',readonly [string,string,string]>={
  translation:[m('Position X ({index})'),m('Position Y ({index})'),m('Position Z ({index})')],
  rotation:[m('Rotation X ({index})'),m('Rotation Y ({index})'),m('Rotation Z ({index})')],
};
/** The equipment rows a project saves (accessory changes, props, alternate forms), for the tab's word. */
export function equipmentCount(project:Project){const equipment=project.customization.equipment??{};return (equipment.meshes??[]).length+(equipment.props??[]).length+(equipment.variants??[]).filter(row=>!weaponLike(row.model??'')).length;}
/** The equipment rows (`customization.equipment`) change through this one function, so a weapon
 * model row the Gun tab writes and the rows here share the same object. */
export function equipmentChange(project:Project,update:(project:Project)=>void){
  const equipment=project.customization.equipment??{};
  return (values:Partial<NonNullable<Project['customization']['equipment']>>)=>update(changeCustomization(project,{equipment:{...equipment,...values}}));
}
/** Accessories & props: the hero's own weapons and accessories (keep, hide, or replace with your
 * prepared meshes), props attached to a bone, and alternate forms. Weapon models are on the Gun tab. */
export function Equipment({project,update,disabled,fail,summary}:Props){
  const t=useT();
  const equipment=project.customization.equipment??{},meshes=equipment.meshes??[],props=equipment.props??[],variants=equipment.variants??[];
  const change=equipmentChange(project,update);
  const meshEdit=(index:number,value:Partial<EquipmentMesh>)=>change({enabled:true,meshes:meshes.map((row,i)=>i===index?{...row,...value}:row)});
  const propEdit=(index:number,value:Partial<PropChoices>)=>change({enabled:true,props:props.map((row,i)=>i===index?{...row,...value}:row)});
  const variantEdit=(index:number,value:Partial<VariantChoices>)=>change({enabled:true,variants:variants.map((row,i)=>i===index?{...row,...value}:row)});
  const forms=variants.filter(v=>!weaponLike(v.model??'')).length;
  return <StudioCard title={t('Accessories and props')} className="equipment-card" sub={t('Keep, hide, or replace accessories. Props require model preparation; alternate forms use separate projects to retain their own skeletons.')}>
    <Check label={t('Include equipment changes')} value={equipment.enabled??false} onChange={enabled=>change({enabled})} disabled={disabled}/>
    <div className="equipment-head"><h4>{tp(meshes.length,{other:'Weapons and accessories ({count})'})}</h4><button disabled={disabled||meshes.length>=256} onClick={()=>change({enabled:true,meshes:[...meshes,{target:'',action:'keep',sources:[],include_lods:true}]})}>{t('Add accessory change')}</button></div>
    {meshes.map((row,index)=>{const action=row.action??'keep';return <div className={`equipment-row ${action}`} key={index}>
      <div className="equipment-row-head"><strong>{t('Accessory {index}',{index:index+1})}</strong>
        <div className="segmented" role="radiogroup" aria-label={t('Accessory handling {index}',{index:index+1})}>{HANDLING.map(([value,label])=><button key={value} type="button" role="radio" aria-checked={action===value} title={t(label)} disabled={disabled} onClick={()=>meshEdit(index,{action:value})}>{tc('accessory handling',SHORT[value])}</button>)}</div>
        <button className="tertiary" disabled={disabled} onClick={()=>change({meshes:meshes.filter((_,i)=>i!==index)})}>{t('Remove')}</button></div>
      <SelectField label={t('Original mesh {index}',{index:index+1})} value={row.target??''} values={[["",t('Choose a saved original mesh…')],...[...new Set([...Object.keys(project.meshes),...(row.target?[row.target]:[])])].map(name=>[name,name] as const)]} onChange={target=>meshEdit(index,{target})} disabled={disabled}/>
      {action==='replace'&&<><label className="field">{t('Prepared mesh names {index}',{index:index+1})}<input disabled={disabled} value={(row.sources??[]).join(', ')} onChange={event=>meshEdit(index,{sources:event.target.value.split(',').map(name=>name.trim()).filter(Boolean)})}/></label><SelectField label={t('Add prepared mesh {index}',{index:index+1})} value="" values={[["",t('Choose a source mesh…')],...(summary?.meshes??[]).map(mesh=>[mesh.name,mesh.name] as const)]} onChange={name=>{if(name)meshEdit(index,{sources:[...new Set([...(row.sources??[]),name])]});}} disabled={disabled}/></>}
      <Check label={t('Apply accessory {index} to matching detail levels',{index:index+1})} value={row.include_lods??true} onChange={include_lods=>meshEdit(index,{include_lods})} disabled={disabled}/>
    </div>;})}
    {meshes.length>0&&<p className="hint">{t('Replace uses one or more of your prepared meshes, chosen in the list beside it.')}</p>}
    <div className="equipment-head"><h4>{tp(props.length,{other:'Attached props ({count})'})}</h4><button disabled={disabled||props.length>=64} onClick={()=>change({enabled:true,props:[...props,{enabled:true,source:'',bone:'',translation:[0,0,0],rotation:[0,0,0],scale:1}]})}>{t('Attach a prop')}</button></div>
    {props.map((row,index)=><div className="equipment-row" key={index}><div className="equipment-row-head"><Check label={t('Include prop {index}',{index:index+1})} value={row.enabled??true} onChange={enabled=>propEdit(index,{enabled})} disabled={disabled}/><button className="tertiary" disabled={disabled} onClick={()=>change({props:props.filter((_,i)=>i!==index)})}>{t('Remove')}</button></div>
      <FilePathInput label={t('Prop model {index}',{index:index+1})} name={`Prop model ${index+1}`} kind="prop" value={row.source??''} update={source=>propEdit(index,{source})} disabled={disabled} fail={fail}/>
      <SelectField label={t('Attach prop {index} to bone',{index:index+1})} value={row.bone??''} values={[["",t('Choose a model bone…')],...[...new Set([...(summary?.bones??[]),...(row.bone?[row.bone]:[])])].map(name=>[name,name] as const)]} onChange={bone=>propEdit(index,{bone})} disabled={disabled}/>
      {(['translation','rotation'] as const).map(key=><div className="vector-fields" key={key}>{[0,1,2].map(axis=><NumberField key={axis} label={t(VECTOR_LABELS[key][axis],{index:index+1})} value={row[key]?.[axis]??0} min={-100000} max={100000} step={.1} update={value=>{const vector:[number,number,number]=[...(row[key]??[0,0,0])];vector[axis]=value;propEdit(index,{[key]:vector});}} disabled={disabled}/>)}</div>)}
      <NumberField label={t('Prop scale {index}',{index:index+1})} value={row.scale??1} min={.0001} max={10000} step={.0001} update={scale=>propEdit(index,{scale})} disabled={disabled}/></div>)}
    <div className="equipment-head"><h4>{tp(forms,{other:'Alternate forms ({count})'})}</h4><button disabled={disabled||variants.length>=16} onClick={()=>change({enabled:true,variants:[...variants,{enabled:true,model:'',project:''}]})}>{t('Add alternate form')}</button></div>
    {variants.map((row,index)=>weaponLike(row.model??'')?null:<div className="equipment-row" key={index}><div className="equipment-row-head"><Check label={t('Include alternate form {index}',{index:index+1})} value={row.enabled??true} onChange={enabled=>variantEdit(index,{enabled})} disabled={disabled}/><button className="tertiary" disabled={disabled} onClick={()=>change({variants:variants.filter((_,i)=>i!==index)})}>{t('Remove')}</button></div><p className="path mono">{row.model||t('No alternate model selected')}</p><HeroPicker current={row.model??''} disabled={disabled} select={hero=>variantEdit(index,{model:hero.path})} fail={fail}/><FilePathInput label={t('Project for alternate form {index}',{index:index+1})} name={`Project for alternate form ${index+1}`} kind="variant_project" value={row.project??''} update={path=>variantEdit(index,{project:path})} disabled={disabled} fail={fail}/></div>)}
    <p className="hint">{t('A hero’s other form (a transformation, for example) uses a separate project so it keeps its own skeleton.')}</p>
  </StudioCard>;
}
