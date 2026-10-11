/** Disposable, source-space diagnostic. Does not retarget bind geometry or simulate cloth. */
import { Bone as ThreeBone, Matrix4, SkinnedMesh, Vector3, type Skeleton } from 'three';
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js';
import type { Bone, PreparationOptions, Role } from './models';
import { physicsGroups } from './secondary-bones';

export function descendants(bones:Bone[],parent:string):Set<string>{
  const result=new Set([parent]);let changed=true;
  while(changed){changed=false;for(const bone of bones)if(bone.parent&&result.has(bone.parent)&&!result.has(bone.name)){result.add(bone.name);changed=true;}}
  return result;
}
export function originalBoneName(gltf:GLTF,bone:ThreeBone):string {
  const index=gltf.parser.associations.get(bone)?.nodes;
  return index===undefined?bone.name:(gltf.parser.json.nodes[index].name??bone.name);
}
export class MappingMotion {
  readonly bones=new Map<string,ThreeBone>();
  readonly rest=new Map<ThreeBone,Matrix4>();
  private readonly skeletons=new Set<Skeleton>();
  private readonly flags=new Map<ThreeBone,[boolean,boolean]>();
  constructor(readonly gltf:GLTF,readonly target:Bone[],readonly source:Bone[]=[]){
    gltf.scene.updateMatrixWorld(true);
    gltf.scene.traverse(object=>{
      if(object instanceof ThreeBone){const name=originalBoneName(gltf,object);if(this.bones.has(name))throw new Error(`Duplicate source joint ${name}; mapping preview needs one unambiguous rig.`);this.bones.set(name,object);}
      if(object instanceof SkinnedMesh)this.skeletons.add(object.skeleton);
    });
    // An inverse bind is authoritative even when the exported scene has a saved pose.
    for(const skeleton of this.skeletons)for(let i=0;i<skeleton.bones.length;i++){
      const bone=skeleton.bones[i],bind=skeleton.boneInverses[i];
      if(!Number.isFinite(bind.determinant())||Math.abs(bind.determinant())<1e-15)throw new Error('Preview contains a singular inverse bind.');
      const matrix=bind.clone().invert(),previous=this.rest.get(bone);
      if(previous&&previous.elements.some((v,j)=>Math.abs(v-matrix.elements[j])>1e-5*Math.max(1,Math.abs(v))))throw new Error('Source meshes disagree about a shared joint bind; mapping preview is unavailable.');
      this.rest.set(bone,matrix);
    }
    for(const bone of this.bones.values()){
      if(!this.rest.has(bone))this.rest.set(bone,bone.matrixWorld.clone());
      this.flags.set(bone,[bone.matrixAutoUpdate,bone.matrixWorldAutoUpdate]);
      bone.matrixAutoUpdate=false;bone.matrixWorldAutoUpdate=false;
    }
    this.pose({},null,0);
  }
  point(name:string):Vector3|null {const bone=this.bones.get(name);return bone?new Vector3().setFromMatrixPosition(bone.matrixWorld):null;}
  preserved(options:PreparationOptions){return options.preserve_physics_bones?physicsGroups(this.source,[...this.bones.keys()],options).filter(g=>options.physics_roots?.includes(g.root)):[];}
  bendPreserved(options:PreparationOptions,name:string,degrees:number,axis:'X'|'Y'|'Z'='X'):{affected:number;message:string}{
    this.pose(options,null,0);
    const allowed=new Set(this.preserved(options).flatMap(g=>g.source_bones)),bone=this.bones.get(name);
    if(!bone||!allowed.has(name))return {affected:0,message:'Choose a preserved physics joint to test.'};
    const branch=descendants(this.source,name),pivot=new Vector3().setFromMatrixPosition(this.rest.get(bone)!);
    const delta=new Matrix4().makeTranslation(pivot.x,pivot.y,pivot.z).multiply(new Matrix4().makeRotationAxis(new Vector3(axis==='X'?1:0,axis==='Y'?1:0,axis==='Z'?1:0),degrees*Math.PI/180)).multiply(new Matrix4().makeTranslation(-pivot.x,-pivot.y,-pivot.z));
    let affected=0;
    for(const child of branch){const joint=this.bones.get(child);if(joint&&allowed.has(child)){joint.matrixWorld.multiplyMatrices(delta,this.rest.get(joint)!);affected++;}}
    this.gltf.scene.updateMatrixWorld(true);for(const skeleton of this.skeletons)skeleton.update();
    return {affected,message:`Bending ${name} and ${Math.max(0,affected-1)} child joints using existing weights. This is a rig test, not physics simulation.`};
  }
  pose(options:PreparationOptions,role:Role|null,degrees:number,axis:'X'|'Y'|'Z'='Y'):{affected:number;message:string}{
    for(const [bone,matrix] of this.rest)bone.matrixWorld.copy(matrix);
    let affected=0,message='Rest pose';
    if(role&&degrees!==0){
      const name=options.source_roles?.[role],destination=options.target_roles?.[role],bone=name?this.bones.get(name):undefined;
      if(!bone||!destination||!this.target.some(b=>b.name===destination))message='Choose valid source and target bones for this movement.';
      else {
        const pivot=new Vector3().setFromMatrixPosition(this.rest.get(bone)!),branch=descendants(this.target,destination);
        const rotation=new Matrix4().makeRotationAxis(new Vector3(axis==='X'?1:0,axis==='Y'?1:0,axis==='Z'?1:0),degrees*Math.PI/180);
        const delta=new Matrix4().makeTranslation(pivot.x,pivot.y,pivot.z).multiply(rotation).multiply(new Matrix4().makeTranslation(-pivot.x,-pivot.y,-pivot.z));
        const mappings={...options.mapping};
        for(const group of this.preserved(options))for(const name of group.source_bones)mappings[name]=group.target_anchor;
        for(const [source,target] of Object.entries(mappings)){
          const sourceBone=this.bones.get(source);if(!sourceBone||!branch.has(target))continue;
          sourceBone.matrixWorld.multiplyMatrices(delta,this.rest.get(sourceBone)!);affected++;
        }
        message=`${affected} mapped source joints follow this target branch. Unmapped joints stay in the rest pose.`;
      }
    }
    this.gltf.scene.updateMatrixWorld(true);for(const skeleton of this.skeletons)skeleton.update();
    return {affected,message};
  }
  dispose(){for(const [bone,flags] of this.flags){bone.matrixAutoUpdate=flags[0];bone.matrixWorldAutoUpdate=flags[1];}this.gltf.scene.updateMatrixWorld(true);}
}
