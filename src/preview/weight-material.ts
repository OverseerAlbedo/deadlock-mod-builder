import { DataTexture, DoubleSide, FloatType, MeshBasicMaterial, RGBAFormat } from 'three';
import { floatBuffer, type PreviewMesh, type PreviewScene } from './packet';
export function weightMaterial(scene:PreviewScene,mesh:PreviewMesh,bone:string,maxTextureSize:number){
  const skin=mesh.skin;const material=new MeshBasicMaterial({color:'#3a414b',side:DoubleSide});let texture:DataTexture|null=null;
  if(skin){if(skin.width>maxTextureSize||skin.height>maxTextureSize)throw new Error('Skin weight overlay exceeds the current GPU texture limit.');texture=new DataTexture(floatBuffer(scene,skin.data),skin.width,skin.height,RGBAFormat,FloatType);texture.needsUpdate=true;const selected=skin.joint_names.indexOf(bone);
    material.customProgramCacheKey=()=>`dmx-weight-${skin.influences}`;
    material.onBeforeCompile=shader=>{
      shader.uniforms.dmbWeights={value:texture};shader.uniforms.dmbWidth={value:skin.width};shader.uniforms.dmbBone={value:selected};
      shader.vertexShader='uniform highp sampler2D dmbWeights;uniform int dmbWidth;uniform int dmbBone;varying float dmbWeight;\n'+shader.vertexShader;
      shader.vertexShader=shader.vertexShader.replace('#include <begin_vertex>',`#include <begin_vertex>\ndmbWeight=0.;for(int slot=0;slot<${skin.influences};slot++){int pair=gl_VertexID*${skin.influences}+slot;int pixel=pair/2;vec4 packed=texelFetch(dmbWeights,ivec2(pixel%dmbWidth,pixel/dmbWidth),0);vec2 influence=(pair%2==0)?packed.xy:packed.zw;if(dmbBone>=0&&int(influence.x)==dmbBone)dmbWeight+=influence.y;}`);
      shader.fragmentShader='varying float dmbWeight;\n'+shader.fragmentShader;
      shader.fragmentShader=shader.fragmentShader.replace('#include <color_fragment>',`float w=clamp(dmbWeight,0.,1.);diffuseColor.rgb=clamp(vec3(1.5)-abs(vec3(4.*w-3.,4.*w-2.,4.*w-1.)),0.,1.);`);
    };
  }
  return {material,dispose:()=>{texture?.dispose();material.dispose();}};
}
