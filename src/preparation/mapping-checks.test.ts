// @vitest-environment node
import {describe,it,expect} from 'vitest';
import {Bone,BufferGeometry,Float32BufferAttribute,Group,MeshBasicMaterial,Skeleton,SkinnedMesh,Uint16BufferAttribute,Vector3} from 'three';
import type {GLTF} from 'three/examples/jsm/loaders/GLTFLoader.js';
import {MappingMotion} from './mapping-motion';
import {checkMapping} from './mapping-checks';
function fixture(){
  const root=new Bone(),head=new Bone(),helper=new Bone();root.name='hips';head.name='head';helper.name='hair';head.position.y=2;helper.position.x=.1;root.add(head);head.add(helper);
  const scene=new Group();scene.add(root);scene.updateMatrixWorld(true);
  const geometry=new BufferGeometry().setAttribute('position',new Float32BufferAttribute([1,2,0,1.01,2,0,1,2.01,0],3)).setAttribute('skinIndex',new Uint16BufferAttribute([1,0,0,0,2,0,0,0,1,0,0,0],4)).setAttribute('skinWeight',new Float32BufferAttribute([1,0,0,0,1,0,0,0,1,0,0,0],4));
  const mesh=new SkinnedMesh(geometry,new MeshBasicMaterial());mesh.bind(new Skeleton([root,head,helper]));scene.add(mesh);
  const gltf={scene,parser:{associations:new Map(),json:{nodes:[]}}} as unknown as GLTF;
  const motion=new MappingMotion(gltf,[{name:'pelvis',parent:null,position:[0,0,0]},{name:'head',parent:'pelvis',position:[0,2,0]}]);
  const options={source_roles:{head:'head',pelvis:'hips'},target_roles:{head:'head',pelvis:'pelvis'},mapping:{hips:'pelvis',head:'head',hair:'head'}};
  return {motion,mesh,options};
}
describe('automatic movement sampling',()=>{
  it('flags a stationary unresolved helper and leaves geometry and weights unchanged',async()=>{
    const {motion,mesh,options}=fixture(),rest=mesh.getVertexPosition(1,new Vector3()).clone(),weights=Array.from(mesh.geometry.getAttribute('skinWeight').array);
    const good=await checkMapping(motion,options);expect(good.cases[0].severeEdges).toBe(0);expect(good.cases[0].moved).toBe(3);
    const bad=await checkMapping(motion,{...options,mapping:{hips:'pelvis',head:'head'}});expect(bad.cases[0].severeEdges).toBeGreaterThan(0);
    expect(mesh.getVertexPosition(1,new Vector3()).distanceTo(rest)).toBeLessThan(1e-7);expect(Array.from(mesh.geometry.getAttribute('skinWeight').array)).toEqual(weights);motion.dispose();
  });
  it('cancels between samples and restores the rest pose',async()=>{
    const {motion,mesh,options}=fixture(),rest=mesh.getVertexPosition(0,new Vector3()).clone();let calls=0;
    await expect(checkMapping(motion,options,()=>++calls>4)).rejects.toThrow('cancelled');
    expect(mesh.getVertexPosition(0,new Vector3()).distanceTo(rest)).toBeLessThan(1e-7);motion.dispose();
  });
});
