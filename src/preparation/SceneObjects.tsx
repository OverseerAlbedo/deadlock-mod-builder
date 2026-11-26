// i18n-area: prepare
import { useMemo } from 'react';
import { m, tp, useT } from '../i18n';
import type { SceneChoices, SceneObject, SceneReceipt } from './models';

const LABEL:Record<string,string>={character:m('Character rig'),included:m('Kept'),repaired:m('Kept (repaired)'),bound:m('Kept (attached)'),converted:m('Kept (converted)'),excluded:m('Set aside'),ignored:m('Ignored')};
const kept=(row:SceneObject)=>['character','included','repaired','bound','converted'].includes(row.status);

/** The short count line of Scene objects: `{kept} kept · {n} set aside[ · {n} choices of yours]`. */
export function sceneSummary(receipt:SceneReceipt,choices:SceneChoices){
  const rows=receipt.objects.filter(r=>r.type!=='ARMATURE'||r.status!=='ignored');
  const pending=(choices.exclude?.length??0)+(choices.include?.length??0)+(choices.character?1:0);
  return {kept:rows.filter(kept).length,setAside:rows.filter(r=>r.status==='excluded').length,pending};
}
/** What the importer kept or set aside, with per-object overrides for the next inspection.
 * `bare` leaves out the disclosure around it (the page's Scene objects menu is the disclosure). */
export function SceneObjects({receipt,choices,change,busy,reinspect,bare=false}:{receipt:SceneReceipt;choices:SceneChoices;change:(choices:SceneChoices)=>void;busy:boolean;reinspect:()=>void;bare?:boolean}){
  const t=useT();
  const exclude=new Set(choices.exclude??[]),include=new Set(choices.include??[]);
  const rows=useMemo(()=>receipt.objects.filter(r=>r.type!=='ARMATURE'||r.status!=='ignored'),[receipt]);
  const rigs=receipt.objects.filter(r=>r.type==='ARMATURE');
  const {pending,setAside}=sceneSummary(receipt,choices);
  function toggle(row:SceneObject,keep:boolean){
    const nextExclude=new Set(exclude),nextInclude=new Set(include);
    nextExclude.delete(row.name);nextInclude.delete(row.name);
    if(keep&&!kept(row))nextInclude.add(row.name);
    if(!keep&&kept(row))nextExclude.add(row.name);
    change({...choices,exclude:[...nextExclude].sort(),include:[...nextInclude].sort()});
  }
  const body=<>
    <p className="hint">{t('The importer keeps the character and sets aside cameras, lights, hidden objects, other characters and scenery. Change any choice, then inspect again. Your source file is never changed.')}</p>
    {rigs.length>1&&<label className="field">{t('Character rig')}<select aria-label={t('Character rig')} disabled={busy} value={choices.character??''} onChange={event=>change({...choices,character:event.target.value||undefined})}><option value="">{receipt.character_rig?t('Automatic ({name})',{name:receipt.character_rig}):t('Automatic (none)')}</option>{rigs.map(r=><option key={r.name} value={r.name}>{r.name}</option>)}</select></label>}
    <div className="table-scroll prep-scene-scroll"><table className="prep-scene-table"><thead><tr><th>{t('Keep')}</th><th>{t('Object')}</th><th>{t('Result')}</th></tr></thead><tbody>{rows.map(row=>{
      const checked=include.has(row.name)||(kept(row)&&!exclude.has(row.name));
      const locked=row.status==='character'||row.status==='ignored'||row.type==='ARMATURE';
      return <tr key={row.name} className={kept(row)?'is-kept':'is-aside'}>
        <td><input type="checkbox" aria-label={t('Keep {name}',{name:row.name})} checked={checked} disabled={busy||locked} onChange={event=>toggle(row,event.target.checked)}/></td>
        <td><span className="mono">{row.name}</span><small className="hint">{row.type.toLowerCase()}</small></td>
        <td><b className={`prep-scene-status status-${row.status}`}>{LABEL[row.status]?t(LABEL[row.status]):row.status}</b><small className="hint">{row.reason}</small></td>
      </tr>;})}</tbody></table></div>
    {receipt.notes.length>0&&<ul className="prep-scene-notes" aria-label={t('Automatic repairs')}>{receipt.notes.map((note,index)=><li key={index}>{note}</li>)}</ul>}
    <div className="row"><button disabled={busy} onClick={reinspect}>{t('Inspect again with these choices')}</button>{pending>0&&<button className="tertiary" disabled={busy} onClick={()=>change({})}>{t('Reset choices')}</button>}</div>
  </>;
  if(bare)return <div className="prep-scene prep-scene-bare">{body}</div>;
  return <details className="prep-scene" open={setAside>0||pending>0}>
    <summary><strong>{t('Scene objects')}</strong><span>{pending?tp(pending,{one:'{kept} kept · {aside} set aside · {count} choice of yours',other:'{kept} kept · {aside} set aside · {count} choices of yours'},{kept:rows.filter(kept).length,aside:setAside}):t('{kept} kept · {aside} set aside',{kept:rows.filter(kept).length,aside:setAside})}</span></summary>
    {body}
  </details>;
}
