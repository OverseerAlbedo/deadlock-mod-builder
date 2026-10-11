import { useMemo } from 'react';
import type { SceneChoices, SceneObject, SceneReceipt } from './models';

const LABEL:Record<string,string>={character:'Character rig',included:'Kept',repaired:'Kept (repaired)',bound:'Kept (attached)',converted:'Kept (converted)',excluded:'Set aside',ignored:'Ignored'};
const kept=(row:SceneObject)=>['character','included','repaired','bound','converted'].includes(row.status);

/** What the importer kept or set aside, with per-object overrides for the next inspection. */
export function SceneObjects({receipt,choices,change,busy,reinspect}:{receipt:SceneReceipt;choices:SceneChoices;change:(choices:SceneChoices)=>void;busy:boolean;reinspect:()=>void}){
  const exclude=new Set(choices.exclude??[]),include=new Set(choices.include??[]);
  const rows=useMemo(()=>receipt.objects.filter(r=>r.type!=='ARMATURE'||r.status!=='ignored'),[receipt]);
  const rigs=receipt.objects.filter(r=>r.type==='ARMATURE');
  const pending=(choices.exclude?.length??0)+(choices.include?.length??0)+(choices.character?1:0);
  const setAside=rows.filter(r=>r.status==='excluded').length;
  function toggle(row:SceneObject,keep:boolean){
    const nextExclude=new Set(exclude),nextInclude=new Set(include);
    nextExclude.delete(row.name);nextInclude.delete(row.name);
    if(keep&&!kept(row))nextInclude.add(row.name);
    if(!keep&&kept(row))nextExclude.add(row.name);
    change({...choices,exclude:[...nextExclude].sort(),include:[...nextInclude].sort()});
  }
  return <details className="prep-scene" open={setAside>0||pending>0}>
    <summary><strong>Scene objects</strong><span>{rows.filter(kept).length} kept · {setAside} set aside{pending?` · ${pending} choice${pending===1?'':'s'} of yours`:''}</span></summary>
    <p className="hint">The importer keeps the character and sets aside cameras, lights, hidden objects, other characters and scenery. Change any choice, then inspect again. Your source file is never changed.</p>
    {rigs.length>1&&<label className="field">Character rig<select aria-label="Character rig" disabled={busy} value={choices.character??''} onChange={event=>change({...choices,character:event.target.value||undefined})}><option value="">Automatic ({receipt.character_rig??'none'})</option>{rigs.map(r=><option key={r.name} value={r.name}>{r.name}</option>)}</select></label>}
    <div className="table-scroll prep-scene-scroll"><table className="prep-scene-table"><thead><tr><th>Keep</th><th>Object</th><th>Result</th></tr></thead><tbody>{rows.map(row=>{
      const checked=include.has(row.name)||(kept(row)&&!exclude.has(row.name));
      const locked=row.status==='character'||row.status==='ignored'||row.type==='ARMATURE';
      return <tr key={row.name} className={kept(row)?'is-kept':'is-aside'}>
        <td><input type="checkbox" aria-label={`Keep ${row.name}`} checked={checked} disabled={busy||locked} onChange={event=>toggle(row,event.target.checked)}/></td>
        <td><span className="mono">{row.name}</span><small className="hint">{row.type.toLowerCase()}</small></td>
        <td><b className={`prep-scene-status status-${row.status}`}>{LABEL[row.status]??row.status}</b><small className="hint">{row.reason}</small></td>
      </tr>;})}</tbody></table></div>
    {receipt.notes.length>0&&<ul className="prep-scene-notes" aria-label="Automatic repairs">{receipt.notes.map((note,index)=><li key={index}>{note}</li>)}</ul>}
    <div className="row"><button disabled={busy} onClick={reinspect}>Inspect again with these choices</button>{pending>0&&<button className="tertiary" disabled={busy} onClick={()=>change({})}>Reset choices</button>}</div>
  </details>;
}
