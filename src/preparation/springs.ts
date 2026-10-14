import type { PhysicsGroup } from './secondary-bones';

/** A source rig's own spring setting of one bone on the builder's scale (see
 * resources/blender/source_springs.py): pose pull, speed lost per frame, gravity. */
export interface SpringValues {source:'vrm'|'mmd';goal:number;drag:number;gravity:number}

/** The average spring values of each moving part (by its root) over its bones that
 * have them; parts without any are left out. */
export function partSprings(groups:PhysicsGroup[],springs:Record<string,SpringValues>|undefined):Record<string,SpringValues>{
  const out:Record<string,SpringValues>={};
  if(!springs)return out;
  for(const group of groups){
    const rows=[group.root,...group.source_bones].filter((b,i,all)=>all.indexOf(b)===i).map(b=>springs[b]).filter((v):v is SpringValues=>Boolean(v));
    if(!rows.length)continue;
    const mean=(key:'goal'|'drag'|'gravity')=>Math.round(rows.reduce((s,r)=>s+r[key],0)/rows.length*1e4)/1e4;
    out[group.root]={source:rows[0].source,goal:mean('goal'),drag:mean('drag'),gravity:mean('gravity')};
  }
  return out;
}
