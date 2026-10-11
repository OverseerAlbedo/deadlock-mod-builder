// @vitest-environment node
import { readFileSync } from 'node:fs';
import { describe,expect,it } from 'vitest';
import { Bone as ThreeBone, BufferGeometry, Float32BufferAttribute, Group, Matrix4, MeshBasicMaterial, Skeleton, SkinnedMesh, Uint16BufferAttribute, Vector3 } from 'three';
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { MappingMotion, originalBoneName } from './mapping-motion';
import { exactGltfLoader } from '../preview/skinning';
import type { Bone } from './models';
function fixture(){
  const waist=new ThreeBone(),head=new ThreeBone();waist.name='sanitized_waist';head.name='sanitized_head';head.position.set(0,2,0);waist.add(head);
  const scene=new Group();scene.add(waist);scene.updateMatrixWorld(true);
  const geometry=new BufferGeometry().setAttribute('position',new Float32BufferAttribute([1,0,0,1,2,0],3)).setAttribute('skinIndex',new Uint16BufferAttribute([0,0,0,0,1,0,0,0],4)).setAttribute('skinWeight',new Float32BufferAttribute([1,0,0,0,1,0,0,0],4));
  const mesh=new SkinnedMesh(geometry,new MeshBasicMaterial());mesh.bind(new Skeleton([waist,head]));scene.add(mesh);
  const gltf={scene,parser:{associations:new Map([[waist,{nodes:0}],[head,{nodes:1}]]),json:{nodes:[{name:'腰'},{name:'頭'}]}}} as unknown as GLTF;
  const motion=new MappingMotion(gltf,[{name:'pelvis',parent:null,position:[0,0,0]},{name:'head',parent:'pelvis',position:[0,0,2]}]);
  return {waist,head,mesh,motion,gltf};
}
const options={source_roles:{pelvis:'腰',head:'頭'},target_roles:{pelvis:'pelvis',head:'head'},mapping:{'腰':'pelvis','頭':'head'}};
describe('mapping diagnostic deformation',()=>{
  it('bends a kept branch independently, follows its body anchor, and resets without changing skin weights',()=>{
    const root=new ThreeBone(),head=new ThreeBone(),part=new ThreeBone(),tip=new ThreeBone();
    root.name='pelvis';head.name='head';part.name='part';tip.name='tip';head.position.y=2;part.position.y=.2;tip.position.y=.5;
    root.add(head);head.add(part);part.add(tip);const scene=new Group();scene.add(root);scene.updateMatrixWorld(true);
    const geometry=new BufferGeometry().setAttribute('position',new Float32BufferAttribute([1,0,0,1,2,0,1,2.7,0],3)).setAttribute('skinIndex',new Uint16BufferAttribute([0,0,0,0,1,0,0,0,3,0,0,0],4)).setAttribute('skinWeight',new Float32BufferAttribute([1,0,0,0,1,0,0,0,1,0,0,0],4));
    const mesh=new SkinnedMesh(geometry,new MeshBasicMaterial());mesh.bind(new Skeleton([root,head,part,tip]));scene.add(mesh);
    const source:Bone[]=[{name:'pelvis',parent:null,position:[0,0,0]},{name:'head',parent:'pelvis',position:[0,2,0]},{name:'part',parent:'head',position:[0,2.2,0],physics:{policy:'verified-rigid-body-bone-binding-v1',dynamic_bodies:['rigid'],kinematic_bodies:[]}},{name:'tip',parent:'part',position:[0,2.7,0]}];
    const motion=new MappingMotion({scene,parser:{associations:new Map(),json:{nodes:[]}}} as unknown as GLTF,source.slice(0,2),source);
    const choices={source_roles:{pelvis:'pelvis',head:'head'},target_roles:{pelvis:'pelvis',head:'head'},mapping:{pelvis:'pelvis',head:'head',part:'pelvis',tip:'pelvis'},preserve_physics_bones:true,physics_roots:['part']};
    const rest=[0,1,2].map(i=>mesh.getVertexPosition(i,new Vector3()).clone()),weights=Array.from(geometry.getAttribute('skinWeight').array);
    expect(motion.bendPreserved(choices,'part',30,'X').affected).toBe(2);
    expect(mesh.getVertexPosition(0,new Vector3()).distanceTo(rest[0])).toBe(0);
    expect(mesh.getVertexPosition(1,new Vector3()).distanceTo(rest[1])).toBe(0);
    expect(mesh.getVertexPosition(2,new Vector3()).distanceTo(rest[2])).toBeGreaterThan(.1);
    motion.pose(choices,'head',30,'Y');expect(mesh.getVertexPosition(2,new Vector3()).distanceTo(rest[2])).toBeGreaterThan(.1);
    expect(motion.bendPreserved(choices,'head',30).affected).toBe(0);
    for(const i of [0,1,2])expect(mesh.getVertexPosition(i,new Vector3()).distanceTo(rest[i])).toBeLessThan(1e-7);
    expect(Array.from(geometry.getAttribute('skinWeight').array)).toEqual(weights);motion.dispose();
  });
  it('uses original glTF names, changes the mapped geometry, and restores the exact bind pose',()=>{
    const {motion,mesh,waist,gltf}=fixture(),weights=Array.from(mesh.geometry.getAttribute('skinWeight').array);
    expect(originalBoneName(gltf,waist)).toBe('腰');
    motion.pose(options,'head',90,'Y');
    expect(mesh.getVertexPosition(0,new Vector3()).toArray()).toEqual([1,0,0]);
    const head=mesh.getVertexPosition(1,new Vector3());expect(head.x).toBeCloseTo(0);expect(head.y).toBeCloseTo(2);expect(head.z).toBeCloseTo(-1);
    motion.pose({...options,mapping:{'腰':'head','頭':'head'}},'head',90,'Y');
    expect(mesh.getVertexPosition(0,new Vector3()).z).toBeCloseTo(-1);
    motion.pose(options,null,0);
    expect(mesh.getVertexPosition(0,new Vector3()).toArray()).toEqual([1,0,0]);expect(mesh.getVertexPosition(1,new Vector3()).toArray()).toEqual([1,2,0]);
    expect(Array.from(mesh.geometry.getAttribute('skinWeight').array)).toEqual(weights);motion.dispose();
  });
  it('shows a wrong landmark at the waist and does not guess a mapping for unresolved skin joints',()=>{
    const {motion,mesh}=fixture();
    const wrong={...options,source_roles:{...options.source_roles,head:'腰'}};
    expect(motion.point(wrong.source_roles.head)?.y).toBe(0);
    motion.pose({...options,mapping:{}},'head',40,'Y');expect(mesh.getVertexPosition(1,new Vector3()).toArray()).toEqual([1,2,0]);
    expect(motion.pose(options,'head',0).affected).toBe(0);motion.dispose();
  });
  it('retains all extra influence streams during mapped movement and reset',async()=>{
    const bytes=readFileSync('src-tauri/crates/deadlock-preview/tests/fixtures/gltf/eight.glb'),exact=exactGltfLoader(),gltf=await exact.loader.parseAsync(bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength),'');
    const meshes:SkinnedMesh[]=[];gltf.scene.traverse(o=>{if(o instanceof SkinnedMesh)meshes.push(o);});const mesh=meshes[0];
    const before=Array.from(mesh.geometry.getAttribute('weights_1').array);
    const motion=new MappingMotion(gltf,[{name:'head',parent:null,position:[0,0,0]}]),name=motion.bones.keys().next().value!;
    const rest=mesh.getVertexPosition(0,new Vector3()).clone();
    const mapping=Object.fromEntries([...motion.bones.keys()].map(n=>[n,'head']));
    motion.pose({source_roles:{head:name},target_roles:{head:'head'},mapping},'head',30,'Y');
    const pivot=new Vector3().setFromMatrixPosition(motion.rest.get(motion.bones.get(name)!)!),delta=new Matrix4().makeTranslation(...pivot.toArray()).multiply(new Matrix4().makeRotationY(Math.PI/6)).multiply(new Matrix4().makeTranslation(...pivot.clone().negate().toArray()));
    const observed=mesh.getVertexPosition(0,new Vector3()).applyMatrix4(mesh.matrixWorld),expected=rest.clone().applyMatrix4(mesh.matrixWorld).applyMatrix4(delta);
    expect(observed.distanceTo(expected)).toBeLessThan(1e-5);
    motion.pose({},null,0);expect(mesh.getVertexPosition(0,new Vector3()).distanceTo(rest)).toBeLessThan(1e-7);expect(Array.from(mesh.geometry.getAttribute('weights_1').array)).toEqual(before);motion.dispose();exact.dispose();
  });
});
