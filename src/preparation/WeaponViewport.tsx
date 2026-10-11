import { useEffect, useMemo } from 'react';
import { Canvas, useThree } from '@react-three/fiber';
import { Box3, BufferAttribute, BufferGeometry, PerspectiveCamera, Vector3 } from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { PreviewBoundary } from '../preview/PreviewBoundary';
import type { Vec3, WeaponPreview } from './models';

function cloud(points:Vec3[]){const geometry=new BufferGeometry();geometry.setAttribute('position',new BufferAttribute(new Float32Array(points.flat()),3));return geometry;}
function transform(matrix:number[][],p:Vec3):Vec3{return [0,1,2].map(i=>matrix[i][0]*p[0]+matrix[i][1]*p[1]+matrix[i][2]*p[2]+matrix[i][3]) as Vec3;}
function Controls({bounds}:{bounds:Box3}){
  const {camera,gl,invalidate,size}=useThree();
  useEffect(()=>{const controls=new OrbitControls(camera,gl.domElement),center=bounds.getCenter(new Vector3()),span=Math.max(bounds.getSize(new Vector3()).length(),.01);controls.target.copy(center);const vertical=(camera instanceof PerspectiveCamera?camera.fov:40)*Math.PI/360,half=Math.min(vertical,Math.atan(Math.tan(vertical)*size.width/Math.max(size.height,1))),distance=span*.6/Math.sin(half);camera.up.set(0,0,1);camera.position.copy(center).add(new Vector3(.2,-1,.35).normalize().multiplyScalar(distance));camera.near=span/10000;camera.far=span*100;if('updateProjectionMatrix' in camera)camera.updateProjectionMatrix();controls.addEventListener('change',()=>invalidate());controls.update();invalidate();return()=>controls.dispose();},[bounds,camera,gl,invalidate,size.width,size.height]);
  return null;
}
/** Both guns in hero model space: the original (grey) and the fitted source gun (amber). */
export default function WeaponViewport({preview}:{preview:WeaponPreview}){
  const {reference,fitted,bounds,markers,span}=useMemo(()=>{
    const reference=cloud(preview.reference),fitted=cloud(preview.fitted),bounds=new Box3();
    for(const p of [...preview.reference,...preview.fitted])bounds.expandByPoint(new Vector3(...p));
    const t=preview.fit.target,s=preview.fit.source,m=preview.fit.matrix;
    const markers:[Vec3,string][]=[[t.grip,'#9fb3c4'],[t.muzzle,'#9fb3c4'],[transform(m,s.grip),'#ffb547'],[transform(m,s.muzzle),'#ffb547']];
    return {reference,fitted,bounds,markers,span:Math.max(bounds.getSize(new Vector3()).length(),.01)};
  },[preview]);
  useEffect(()=>()=>{reference.dispose();fitted.dispose();},[reference,fitted]);
  return <PreviewBoundary><div className="prep-weapon-viewport" aria-label="Weapon fit preview">
    <Canvas frameloop="demand" camera={{fov:40}} dpr={[1,2]}>
      <color attach="background" args={['#12171b']}/>
      <Controls bounds={bounds}/>
      <points geometry={reference}><pointsMaterial color="#8fa3b3" size={span*.012} sizeAttenuation transparent opacity={.75}/></points>
      <points geometry={fitted}><pointsMaterial color="#ffb02e" size={span*.016} sizeAttenuation/></points>
      {markers.map(([p,color],i)=><mesh key={i} position={p}><sphereGeometry args={[span*(i<2?.018:.012),12,8]}/><meshBasicMaterial color={color} wireframe={i<2}/></mesh>)}
    </Canvas>
    <p className="hint"><span className="swatch grey"/>Hero's original gun <span className="swatch amber"/>Your gun after fitting. Large circles: original grip and muzzle; small dots: yours. Drag to orbit.</p>
  </div></PreviewBoundary>;
}
