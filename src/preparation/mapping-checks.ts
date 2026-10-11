/** Bounded diagnostic samples; these never certify native animation or cloth. */
import { SkinnedMesh, Vector3 } from 'three';
import { MappingMotion } from './mapping-motion';
import type { PreparationOptions, Role } from './models';

export interface PoseCheck {role:Role;label:string;axis:'X'|'Y'|'Z';degrees:number;finite:boolean;moved:number;severeEdges:number;testedEdges:number;maxDisplacement:number}
export interface MappingChecks {cases:PoseCheck[];sampledVertices:number;sampledEdges:number}
const probes:[Role,string,'X'|'Y'|'Z',number][]=[
  ['head','Turn head','Y',35],['spine','Bend torso','X',25],['chest','Twist chest','Y',30],
  ['upperarm_L','Left arm','Z',40],['upperarm_R','Right arm','Z',-40],
  ['forearm_L','Left elbow','Z',60],['forearm_R','Right elbow','Z',-60],
  ['forearm_L','Left forearm twist','X',45],['forearm_R','Right forearm twist','X',-45],
  ['thigh_L','Left hip','X',45],['thigh_R','Right hip','X',45],
  ['shin_L','Left knee','X',-60],['shin_R','Right knee','X',-60],
];
interface Sample {mesh:SkinnedMesh;indices:number[];edges:[number,number][];rest:Vector3[]}
const tick=()=>new Promise<void>(resolve=>setTimeout(resolve,0));
async function positions(sample:Sample,cancelled:()=>boolean){
  const result:Vector3[]=[];
  for(let i=0;i<sample.indices.length;i++){
    if(i%4096===0){await tick();if(cancelled())throw new Error('Movement checks cancelled.');}
    result.push(sample.mesh.getVertexPosition(sample.indices[i],new Vector3()).applyMatrix4(sample.mesh.matrixWorld));
  }
  return result;
}
export async function checkMapping(motion:MappingMotion,options:PreparationOptions,cancelled=()=>false):Promise<MappingChecks>{
  motion.pose(options,null,0);
  const meshes:SkinnedMesh[]=[];motion.gltf.scene.traverse(o=>{if(o instanceof SkinnedMesh)meshes.push(o);});
  if(!meshes.length)throw new Error('This preview has no skinned mesh to test.');
  const samples:Sample[]=[];
  for(const mesh of meshes){
    const count=mesh.geometry.getAttribute('position').count,index=mesh.geometry.index,triangles=Math.floor((index?.count??count)/3);
    const vertexStride=Math.max(1,Math.ceil(count/12000)),triangleStride=Math.max(1,Math.ceil(triangles/12000));
    const ids=new Set<number>();for(let i=0;i<count;i+=vertexStride)ids.add(i);
    const raw:[number,number][]=[],seen=new Set<string>();
    for(let i=0;i<triangles;i+=triangleStride){const t=[0,1,2].map(j=>index?index.getX(i*3+j):i*3+j);for(let j=0;j<3;j++){const a=Math.min(t[j],t[(j+1)%3]),b=Math.max(t[j],t[(j+1)%3]),key=`${a}:${b}`;if(seen.has(key))continue;seen.add(key);ids.add(a);ids.add(b);raw.push([a,b]);}}
    const indices=[...ids],lookup=new Map(indices.map((n,i)=>[n,i]));
    const sample:Sample={mesh,indices,edges:raw.map(([a,b])=>[lookup.get(a)!,lookup.get(b)!]),rest:[]};
    sample.rest=await positions(sample,cancelled);samples.push(sample);
  }
  const cases:PoseCheck[]=[];
  try {
    for(const [role,label,axis,degrees] of probes){
      if(cancelled())throw new Error('Movement checks cancelled.');
      motion.pose(options,role,degrees,axis);
      const check:PoseCheck={role,label,axis,degrees,finite:true,moved:0,severeEdges:0,testedEdges:0,maxDisplacement:0};
      for(const sample of samples){
        const current=await positions(sample,cancelled);
        current.forEach((p,i)=>{if(!p.toArray().every(Number.isFinite)){check.finite=false;return;}const d=p.distanceTo(sample.rest[i]);if(d>1e-7)check.moved++;check.maxDisplacement=Math.max(check.maxDisplacement,d);});
        for(const [a,b] of sample.edges){const length=sample.rest[a].distanceTo(sample.rest[b]);if(length<1e-8)continue;const ratio=current[a].distanceTo(current[b])/length;check.testedEdges++;if(!Number.isFinite(ratio))check.finite=false;else if(ratio>4||ratio<.1)check.severeEdges++;}
      }
      cases.push(check);await tick();
    }
  } finally { motion.pose(options,null,0); }
  return {cases,sampledVertices:samples.reduce((sum,s)=>sum+s.indices.length,0),sampledEdges:samples.reduce((sum,s)=>sum+s.edges.length,0)};
}
