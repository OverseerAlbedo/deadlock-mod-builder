import { useEffect, useMemo, useRef, useState } from 'react';
import { Canvas, useThree } from '@react-three/fiber';
import { Box3, BufferAttribute, BufferGeometry, DoubleSide, Matrix4, PerspectiveCamera, Quaternion, Vector3 } from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { PreviewBoundary } from '../preview/PreviewBoundary';
import type { Hand, Shape, Vec3, WeaponFit, WeaponHandPreview, WeaponPreview } from './models';
import type { PreviewShapes } from './gun';

type View='angle'|'front'|'side'|'top';
type Zoom='guns'|'hand'|'body';
/** Camera directions in hero model space: heroes face +X with +Z up, the right hand at -Y. */
const VIEWS:[View,string,Vec3][]=[['angle','Angled',[1,-.75,.45]],['front','Front',[1,0,.12]],['side','Side',[0,-1,.12]],['top','Top',[0,0,1]]];
const COLORS:Record<Hand,string>={right:'#ffb02e',left:'#4fd1c5'};

function cloud(points:Vec3[]){const geometry=new BufferGeometry();geometry.setAttribute('position',new BufferAttribute(new Float32Array(points.flat()),3));geometry.computeBoundingBox();return geometry;}
/** A solid mesh, placed by a fit matrix (row-major 4×4) when one is given. */
function solid(shape:Shape,matrix?:number[][]){
  const geometry=new BufferGeometry();
  geometry.setAttribute('position',new BufferAttribute(new Float32Array(shape.positions),3));
  geometry.setIndex(shape.triangles);
  if(matrix)geometry.applyMatrix4(new Matrix4().fromArray(matrix.flat()).transpose());
  geometry.computeVertexNormals();geometry.computeBoundingBox();
  return geometry;
}
function transform(matrix:number[][],p:Vec3):Vec3{return [0,1,2].map(i=>matrix[i][0]*p[0]+matrix[i][1]*p[1]+matrix[i][2]*p[2]+matrix[i][3]) as Vec3;}
/** Frames `bounds` from a view direction (`mirror`: from the hero's left). It frames again only
 * when the view or `frame` changes, so adjusting the gun keeps the camera where you turned it. */
function Controls({bounds,view,mirror,frame}:{bounds:Box3;view:View;mirror:boolean;frame:string}){
  const {camera,gl,invalidate,size}=useThree();
  const latest=useRef(bounds);latest.current=bounds;
  useEffect(()=>{
    const bounds=latest.current,center=bounds.getCenter(new Vector3()),span=Math.max(bounds.getSize(new Vector3()).length(),.01);
    const vertical=(camera instanceof PerspectiveCamera?camera.fov:40)*Math.PI/360,half=Math.min(vertical,Math.atan(Math.tan(vertical)*size.width/Math.max(size.height,1))),distance=span*.45/Math.sin(half);
    const [x,y,z]=VIEWS.find(([key])=>key===view)?.[2]??VIEWS[0][2],direction:Vec3=[x,mirror?-y:y,z];
    // From above, the hero's front is up on the screen.
    camera.up.set(...(view==='top'?[1,0,0]:[0,0,1]) as Vec3);
    camera.position.copy(center).add(new Vector3(...direction).normalize().multiplyScalar(distance));
    camera.near=span/1000;camera.far=span*100;
    if('updateProjectionMatrix' in camera)camera.updateProjectionMatrix();
    const controls=new OrbitControls(camera,gl.domElement);
    controls.target.copy(center);controls.addEventListener('change',()=>invalidate());controls.update();invalidate();
    return()=>controls.dispose();
  },[frame,view,mirror,camera,gl,invalidate,size.width,size.height]);
  return null;
}
function Arrow({from,direction,length,color}:{from:Vec3;direction:Vec3;length:number;color:string}){
  const turn=useMemo(()=>new Quaternion().setFromUnitVectors(new Vector3(0,1,0),new Vector3(...direction).normalize()),[direction]);
  const radius=length*.035,shaft=length*.72;
  return <group position={from} quaternion={turn}>
    <mesh position={[0,shaft/2,0]}><cylinderGeometry args={[radius,radius,shaft,10]}/><meshBasicMaterial color={color}/></mesh>
    <mesh position={[0,shaft+length*.14,0]}><coneGeometry args={[radius*2.8,length*.28,14]}/><meshBasicMaterial color={color}/></mesh>
  </group>;
}
/** Where shots go: from the original muzzle (white), and from your gun's muzzle (its colour). */
function FireArrows({fit,color}:{fit:WeaponFit;color:string}){
  const length=Math.max(fit.target.length*.5,1e-3);
  return <>
    <Arrow from={fit.target.muzzle} direction={fit.target.forward} length={length} color="#eef3f6"/>
    <Arrow from={transform(fit.matrix,fit.source.muzzle)} direction={fit.target.forward} length={length*.7} color={color}/>
  </>;
}
/** The hands to draw: both hands of a dual-wield hero, otherwise the one gun. */
function handsOf(preview:WeaponPreview):WeaponHandPreview[]{
  return preview.hands?.length?preview.hands:[{side:'right',fit:preview.fit,fitted:preview.fitted,reference:preview.reference}];
}
/** Your fitted guns as solid shapes (amber, the left gun teal) over the hero's see-through
 * original guns and body, with arrows along the line of fire. Points when an older
 * inspection has no shapes. */
export default function WeaponViewport({preview,shapes,focus='right'}:{preview:WeaponPreview;shapes?:PreviewShapes;focus?:Hand}){
  const hands=handsOf(preview),dual=hands.length>1;
  const [view,setView]=useState<View>('angle'),[zoom,setZoom]=useState<Zoom>('hand');
  const scene=useMemo(()=>{
    const guns=hands.map(hand=>{
      const shape=hand.fit?shapes?.guns[hand.side]:undefined,original=shapes?.originals[hand.side];
      return {side:hand.side,fit:hand.fit,
        gun:hand.fit?{geometry:shape?solid(shape,hand.fit.matrix):cloud(hand.fitted),solid:Boolean(shape)}:null,
        original:{geometry:original?solid(original):cloud(hand.reference),solid:Boolean(original)}};
    });
    const body=shapes?.body?{geometry:solid(shapes.body),solid:true}:preview.body?.length?{geometry:cloud(preview.body),solid:false}:null;
    return {guns,body};
  },[preview,shapes]);// eslint-disable-line react-hooks/exhaustive-deps
  useEffect(()=>()=>{for(const g of scene.guns){g.gun?.geometry.dispose();g.original.geometry.dispose();}scene.body?.geometry.dispose();},[scene]);
  const bounds=useMemo(()=>{
    const box=new Box3();
    const add=(geometry:BufferGeometry)=>{if(geometry.boundingBox&&!geometry.boundingBox.isEmpty())box.union(geometry.boundingBox);};
    for(const g of scene.guns)if(zoom!=='hand'||g.side===focus){if(g.gun)add(g.gun.geometry);add(g.original.geometry);}
    if(zoom==='body'&&scene.body)add(scene.body.geometry);
    if(box.isEmpty())box.set(new Vector3(-1,-1,-1),new Vector3(1,1,1));
    // Room around the guns for the hands and arms.
    return box.expandByScalar(Math.max(box.getSize(new Vector3()).length()*(zoom==='body'?.04:.12),.01));
  },[scene,zoom,focus]);
  const span=Math.max(bounds.getSize(new Vector3()).length(),.01);
  const solidAll=scene.guns.every(g=>(!g.gun||g.gun.solid)&&g.original.solid);
  const zooms:[Zoom,string][]=dual?[['hand',focus==='left'?'Left hand':'Right hand'],['guns','Both guns'],['body','Whole body']]:[['hand','Gun'],['body','Whole body']];
  return <PreviewBoundary><div className="prep-weapon-viewport" aria-label="Weapon fit preview">
    <div className="prep-weapon-view" role="toolbar" aria-label="Preview view">
      <span>View</span>{VIEWS.map(([key,label])=><button key={key} type="button" aria-pressed={view===key} onClick={()=>setView(key)}>{label}</button>)}
      <span>Show</span>{zooms.map(([key,label])=><button key={key} type="button" aria-pressed={zoom===key} onClick={()=>setZoom(key)}>{label}</button>)}
    </div>
    <div className="prep-weapon-canvas">
      <Canvas frameloop="demand" camera={{fov:40}} dpr={[1,2]}>
        <color attach="background" args={['#12171b']}/>
        <hemisphereLight args={['#f4f7f9','#20272c',1.5]}/>
        <directionalLight position={[2,-1.2,3]} intensity={1.8}/>
        <directionalLight position={[-2,1.5,-1]} intensity={.5}/>
        <Controls bounds={bounds} view={view} mirror={zoom==='hand'&&focus==='left'} frame={`${zoom}|${focus}`}/>
        {scene.body&&(scene.body.solid
          ?<mesh geometry={scene.body.geometry}><meshStandardMaterial color="#7d8b95" roughness={.9} transparent opacity={.38} depthWrite={false}/></mesh>
          :<points geometry={scene.body.geometry}><pointsMaterial color="#5c6b75" size={span*.004} sizeAttenuation transparent opacity={.45}/></points>)}
        {scene.guns.map(g=><group key={g.side}>
          {g.original.solid
            ?<mesh geometry={g.original.geometry}><meshStandardMaterial color="#b4c6d4" roughness={.6} transparent opacity={.3} depthWrite={false} side={DoubleSide}/></mesh>
            :<points geometry={g.original.geometry}><pointsMaterial color="#8fa3b3" size={span*.006} sizeAttenuation transparent opacity={.75}/></points>}
          {g.gun&&(g.gun.solid
            ?<mesh geometry={g.gun.geometry}><meshStandardMaterial color={COLORS[g.side]} roughness={.55} metalness={.05} flatShading side={DoubleSide}/></mesh>
            :<points geometry={g.gun.geometry}><pointsMaterial color={COLORS[g.side]} size={span*.008} sizeAttenuation/></points>)}
          {g.fit&&<FireArrows fit={g.fit} color={COLORS[g.side]}/>}
        </group>)}
      </Canvas>
    </div>
    <p className="hint"><span className="swatch grey"/>Original gun{dual?'s':''} (see-through) <span className="swatch amber"/>Your {dual?'right-hand ':''}gun{dual&&<><span className="swatch teal"/>Your left-hand gun</>}{scene.body?' · the hero\'s body is shown faintly':''}</p>
    <p className="hint">Arrows show where shots go: white from the original muzzle (shots keep leaving from there), coloured from your gun's muzzle. Your gun's front should point along them; if it points back, use Flip direction. Drag to turn, scroll to zoom.{solidAll?'':' Solid shapes appear after the model is prepared again.'}</p>
  </div></PreviewBoundary>;
}
