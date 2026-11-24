import { Bone, BufferGeometry, Float32BufferAttribute, MeshStandardMaterial, Scene, ShaderLib, Skeleton, SkinnedMesh, Uint16BufferAttribute, type WebGLProgramParametersWithUniforms, type WebGLRenderer } from 'three';
import { describe, expect, it } from 'vitest';
import { BoneHighlight, LIT_ATTRIBUTE, litMessage } from './boneHighlight';

/** Three vertices: the first on joint 0, the second shared 0.25 / 0.75 by joints 0 and 1, the third on
 * joint 1. Joint order of the skin: root, arm (and a third bone, tip, with no weights). */
function rig(){
  const root=new Bone(),arm=new Bone(),tip=new Bone();root.name='root';arm.name='arm';tip.name='tip';root.add(arm);arm.add(tip);
  const geometry=new BufferGeometry();
  geometry.setAttribute('position',new Float32BufferAttribute([0,0,0,1,0,0,2,0,0],3));
  geometry.setAttribute('skinIndex',new Uint16BufferAttribute([0,0,0,0, 0,1,0,0, 1,0,0,0],4));
  geometry.setAttribute('skinWeight',new Float32BufferAttribute([1,0,0,0, .25,.75,0,0, 1,0,0,0],4));
  const mesh=new SkinnedMesh(geometry,new MeshStandardMaterial());
  const scene=new Scene();scene.add(root);scene.add(mesh);scene.updateMatrixWorld(true);
  mesh.bind(new Skeleton([root,arm,tip]));
  return {scene,mesh,root,arm,tip};
}
const values=(mesh:SkinnedMesh)=>Array.from((mesh.geometry.getAttribute(LIT_ATTRIBUTE) as Float32BufferAttribute).array);

describe('Show what a bone moves',()=>{
  it('writes the chosen bone’s weights per vertex, with that bone’s joint index in this mesh’s skin',()=>{
    const {scene,mesh,root,arm}=rig(),highlight=new BoneHighlight(scene);
    expect(values(mesh)).toEqual([0,0,0]);
    expect(highlight.show(arm)).toEqual({found:true,moved:2});
    expect(values(mesh)).toEqual([0,.75,1]);
    expect(highlight.show(root)).toEqual({found:true,moved:2});
    expect(values(mesh)).toEqual([1,.25,0]);
    expect(highlight.show(null)).toEqual({found:false,moved:0});
    expect(values(mesh)).toEqual([0,0,0]);
    highlight.dispose();
    expect(mesh.geometry.getAttribute(LIT_ATTRIBUTE)).toBeUndefined();
  });
  it('says when a bone moves no part of the model, and when it is not in the preview',()=>{
    const {scene,tip}=rig(),highlight=new BoneHighlight(scene);
    const result=highlight.show(tip);
    expect(result).toEqual({found:true,moved:0});
    expect(litMessage('Left thigh',result)).toBe('Left thigh: this bone moves no part of the model.');
    expect(litMessage('Left thigh',{found:true,moved:12})).toBe('Lit: what Left thigh moves.');
    expect(litMessage('rope_01',highlight.show(undefined))).toBe('rope_01: this bone is not in the preview.');
  });
  it('adds the lit colour after the material’s own shader patch and undoes it on dispose',()=>{
    const {scene,mesh}=rig(),material=mesh.material as MeshStandardMaterial;
    const calls:string[]=[];
    material.onBeforeCompile=()=>{calls.push('exact skin');};material.customProgramCacheKey=()=>'exact';
    const own=material.onBeforeCompile;
    const highlight=new BoneHighlight(scene);
    const shader={vertexShader:ShaderLib.standard.vertexShader,fragmentShader:ShaderLib.standard.fragmentShader,uniforms:{}} as unknown as WebGLProgramParametersWithUniforms;
    material.onBeforeCompile(shader,{} as WebGLRenderer);
    expect(calls).toEqual(['exact skin']);
    expect(shader.vertexShader).toContain(`attribute float ${LIT_ATTRIBUTE};`);
    expect(shader.vertexShader).toContain(`vDmbLit = ${LIT_ATTRIBUTE};`);
    expect(shader.fragmentShader).toContain('clamp(vDmbLit, 0.0, 1.0)');
    expect(material.customProgramCacheKey()).toBe('exact|dmb-lit-v1');
    highlight.dispose();
    expect(material.onBeforeCompile).toBe(own);
    expect(material.customProgramCacheKey()).toBe('exact');
  });
});
