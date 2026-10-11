/** Source preview only. Retains all eight glTF influence sets without changing any build input. */
import { BufferAttribute, DataTexture, FloatType, Matrix4, RGBAFormat, SkinnedMesh, Vector3, Vector4, type Material } from 'three';
import { GLTFLoader, type GLTF, type GLTFParser } from 'three/examples/jsm/loaders/GLTFLoader.js';

const field=(attribute:BufferAttribute,index:number,lane:number)=>lane===0?attribute.getX(index):lane===1?attribute.getY(index):lane===2?attribute.getZ(index):attribute.getW(index);
function attachExactSkin(mesh:SkinnedMesh,maxTextureSize:number):DataTexture|null{
  const geometry=mesh.geometry,sets:{joints:BufferAttribute;weights:BufferAttribute}[]=[];
  for(let index=0;index<8;index++){
    const joints=geometry.getAttribute(index?`joints_${index}`:'skinIndex') as BufferAttribute|undefined;
    const weights=geometry.getAttribute(index?`weights_${index}`:'skinWeight') as BufferAttribute|undefined;
    if(!joints&&!weights)break;if(!joints||!weights)throw new Error('Preview skin streams are incomplete.');sets.push({joints,weights});
  }
  if(!sets.length)throw new Error('Preview skinned mesh has no influence streams.');
  const extra=sets.length-1,count=geometry.getAttribute('position').count;let texture:DataTexture|null=null,width=1;
  if(extra){
    const texels=count*extra*2;width=Math.min(maxTextureSize,2048);const height=Math.ceil(texels/width);
    if(height>maxTextureSize||width*height*16>256*1024*1024)throw new Error('Extended skin influence data exceeds this GPU preview limit. Use a smaller preview copy; source files are unchanged.');
    const data=new Float32Array(width*height*4);
    for(let vertex=0;vertex<count;vertex++)for(let set=1;set<sets.length;set++)for(let lane=0;lane<4;lane++){
      const offset=(vertex*extra+set-1)*8;data[offset+lane]=field(sets[set].joints,vertex,lane);data[offset+4+lane]=field(sets[set].weights,vertex,lane);
    }
    texture=new DataTexture(data,width,height,RGBAFormat,FloatType);texture.needsUpdate=true;
  }
  const base=new Vector4(),weighted=new Vector4(),accumulated=new Vector4(),bone=new Matrix4();
  mesh.applyBoneTransform=<T extends Vector3|Vector4>(index:number,target:T):T=>{
    base.set(target.x,target.y,target.z,target instanceof Vector4?target.w:1).applyMatrix4(mesh.bindMatrix);accumulated.set(0,0,0,0);
    for(const set of sets)for(let lane=0;lane<4;lane++){
      const weight=field(set.weights,index,lane);if(weight===0)continue;const joint=field(set.joints,index,lane);
      bone.multiplyMatrices(mesh.skeleton.bones[joint].matrixWorld,mesh.skeleton.boneInverses[joint]);accumulated.addScaledVector(weighted.copy(base).applyMatrix4(bone),weight);
    }
    accumulated.w=base.w;accumulated.applyMatrix4(mesh.bindMatrixInverse);target.x=accumulated.x;target.y=accumulated.y;target.z=accumulated.z;if(target instanceof Vector4)target.w=accumulated.w;return target;
  };
  const extraCode=extra?`for(int dmbSet=0;dmbSet<${extra};dmbSet++){int dmbOffset=(gl_VertexID*${extra}+dmbSet)*2;vec4 dmbJ=texelFetch(dmbInfluences,ivec2(dmbOffset%dmbWidth,dmbOffset/dmbWidth),0);dmbOffset++;vec4 dmbW=texelFetch(dmbInfluences,ivec2(dmbOffset%dmbWidth,dmbOffset/dmbWidth),0);dmbSkin+=getBoneMatrix(dmbJ.x)*dmbW.x+getBoneMatrix(dmbJ.y)*dmbW.y+getBoneMatrix(dmbJ.z)*dmbW.z+getBoneMatrix(dmbJ.w)*dmbW.w;}`:'';
  const patch=(material:Material)=>{
    const next=material.clone();next.customProgramCacheKey=()=>`dmb-source-skin-v1-${extra}`;
    next.onBeforeCompile=shader=>{
      if(texture){shader.uniforms.dmbInfluences={value:texture};shader.uniforms.dmbWidth={value:width};}
      shader.vertexShader=(texture?'uniform highp sampler2D dmbInfluences;uniform int dmbWidth;\n':'')+shader.vertexShader;
      shader.vertexShader=shader.vertexShader.replace('#include <skinbase_vertex>',`#ifdef USE_SKINNING\nmat4 dmbSkin=getBoneMatrix(skinIndex.x)*skinWeight.x+getBoneMatrix(skinIndex.y)*skinWeight.y+getBoneMatrix(skinIndex.z)*skinWeight.z+getBoneMatrix(skinIndex.w)*skinWeight.w;${extraCode}\ndmbSkin[0][3]=0.;dmbSkin[1][3]=0.;dmbSkin[2][3]=0.;dmbSkin[3][3]=1.;dmbSkin=bindMatrixInverse*dmbSkin*bindMatrix;\n#endif`)
      .replace('#include <skinning_vertex>','#ifdef USE_SKINNING\ntransformed=(dmbSkin*vec4(transformed,1.0)).xyz;\n#endif')
      .replace('#include <skinnormal_vertex>','#ifdef USE_SKINNING\nmat3 dmbLinear=mat3(dmbSkin);if(abs(determinant(dmbLinear))>1e-12){objectNormal=transpose(inverse(dmbLinear))*objectNormal;}else{objectNormal=dmbPseudoNormal(dmbLinear)*objectNormal;}\n#ifdef USE_TANGENT\nobjectTangent=dmbLinear*objectTangent;\n#endif\n#endif');
      shader.vertexShader=pseudoInverseNormal+'\n'+shader.vertexShader;
    };return next;
  };
  mesh.material=Array.isArray(mesh.material)?mesh.material.map(patch):patch(mesh.material);mesh.frustumCulled=false;return texture;
}
// Jacobi eigensystem gives A (AᵀA)⁺ for singular blended transforms. The regular path uses exact GLSL inverse.
const pseudoInverseNormal=`mat3 dmbPseudoNormal(mat3 a){mat3 b=transpose(a)*a;mat3 v=mat3(1.0);for(int sweep=0;sweep<8;sweep++){for(int pair=0;pair<3;pair++){int p=pair==2?1:0;int q=pair==0?1:2;float x=b[q][p];if(abs(x)>1e-15){float tau=(b[q][q]-b[p][p])/(2.0*x);float t=sign(tau)/(abs(tau)+sqrt(1.0+tau*tau));if(tau==0.0)t=1.0;float c=inversesqrt(1.0+t*t);float s=t*c;mat3 j=mat3(1.0);j[p][p]=c;j[q][q]=c;j[q][p]=s;j[p][q]=-s;b=transpose(j)*b*j;v=v*j;}}}float threshold=max(max(b[0][0],b[1][1]),b[2][2])*1e-12;mat3 d=mat3(0.0);for(int i=0;i<3;i++){if(b[i][i]>threshold)d[i][i]=1.0/b[i][i];}return a*v*d*transpose(v);}`;

export function exactGltfLoader(maxTextureSize=4096){
  const textures:DataTexture[]=[];const loader=new GLTFLoader();
  loader.register((parser:GLTFParser)=>{
    const originals=new Map<number,BufferAttribute>();
    return {name:'DMB_exact_skin_weights',beforeRoot:async()=>{
      const indices=new Set<number>();for(const mesh of parser.json.meshes??[])for(const primitive of mesh.primitives??[])if(typeof primitive.attributes?.WEIGHTS_0==='number')indices.add(primitive.attributes.WEIGHTS_0);
      await Promise.all([...indices].map(async index=>{const attribute=await parser.getDependency('accessor',index) as BufferAttribute;originals.set(index,attribute.clone());}));
    },afterRoot:async(gltf:GLTF)=>{
      gltf.scene.traverse(object=>{if(!(object instanceof SkinnedMesh))return;const association=parser.associations.get(object);if(association?.meshes===undefined||association.primitives===undefined)throw new Error('Preview mesh source identity is unavailable.');
        const index=parser.json.meshes[association.meshes].primitives[association.primitives].attributes.WEIGHTS_0;const original=originals.get(index);if(!original)throw new Error('Original skin weights are unavailable.');object.geometry.setAttribute('skinWeight',original.clone());const texture=attachExactSkin(object,maxTextureSize);if(texture)textures.push(texture);
      });
    }};
  });return {loader,dispose:()=>textures.forEach(texture=>texture.dispose())};
}
