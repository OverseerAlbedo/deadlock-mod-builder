// @vitest-environment node
import { readFileSync } from 'node:fs';
import { describe,expect,it } from 'vitest';
import { AnimationMixer, LoopOnce, Mesh, Vector3 } from 'three';
import { exactGltfLoader } from './skinning';
const root='src-tauri/crates/deadlock-preview/tests/fixtures/gltf/';
const oracle=JSON.parse(readFileSync(root+'oracle.json','utf8')) as Record<string,{poses:{time:number;positions:number[][][];normals:number[][][]}[]}>;
describe('source animation preview against the independent Python oracle',()=>{
  for(const name of ['eight.glb','step.glb','cubicspline.glb'])it(name+' retains all influences and samples animation/morph poses',async()=>{
    const bytes=readFileSync(root+name),exact=exactGltfLoader();const gltf=await exact.loader.parseAsync(bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength),'');
    const mixer=new AnimationMixer(gltf.scene),action=mixer.clipAction(gltf.animations[0]);action.setLoop(LoopOnce,1);action.clampWhenFinished=true;action.play();
    const meshes:Mesh[]=[];gltf.scene.traverse(object=>{if(object instanceof Mesh)meshes.push(object);});
    if(name==='eight.glb')expect(meshes[0].geometry.getAttribute('weights_1')).toBeDefined();
    for(const pose of oracle[name].poses){action.reset().play();mixer.setTime(pose.time);gltf.scene.updateMatrixWorld(true);for(let mesh=0;mesh<meshes.length;mesh++)for(let vertex=0;vertex<pose.positions[mesh].length;vertex++){
      const observed=meshes[mesh].getVertexPosition(vertex,new Vector3()).applyMatrix4(meshes[mesh].matrixWorld).toArray();for(let axis=0;axis<3;axis++)expect(observed[axis],`${name} time ${pose.time} vertex ${vertex} axis ${axis}`).toBeCloseTo(pose.positions[mesh][vertex][axis],5);
    }}exact.dispose();mixer.stopAllAction();
  });
});
