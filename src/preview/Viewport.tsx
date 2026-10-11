import { Component, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { Canvas, useThree } from '@react-three/fiber';
import { Box3, BufferAttribute, BufferGeometry, DoubleSide, PerspectiveCamera, Vector3, Texture, TextureLoader, SRGBColorSpace } from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { floatBuffer,indexBuffer,type PreviewScene } from './packet';
import type { Project } from '../models';
import { api } from '../api';
import { weightMaterial } from './weight-material';
import { Check, SelectField } from '../components/CustomizationPages';

function Controls({scene,reset}:{scene:PreviewScene;reset:number}){
  const {camera,gl,invalidate}=useThree();
  const bounds=scene.metadata.bounds;
  useEffect(()=>{
    const center=new Vector3(...bounds.min).add(new Vector3(...bounds.max)).multiplyScalar(.5);
    const radius=Math.max(new Vector3(...bounds.max).distanceTo(new Vector3(...bounds.min))*.5,.01);
    camera.up.set(0,0,1);camera.position.copy(center).add(new Vector3(radius*1.5,-radius*2.6,radius*1.3));
    if(camera instanceof PerspectiveCamera){camera.near=Math.max(radius/10000,.00001);camera.far=radius*1000;camera.updateProjectionMatrix();}
    const controls=new OrbitControls(camera,gl.domElement);controls.target.copy(center);controls.enableDamping=false;controls.minDistance=radius*.01;controls.maxDistance=radius*100;const redraw=()=>invalidate();controls.addEventListener('change',redraw);controls.update();invalidate();
    return()=>{controls.removeEventListener('change',redraw);controls.dispose();};
  },[camera,gl,invalidate,bounds,reset]);
  return null;
}
function Model({scene,wireframe,showBones,selected,select,mode,bone,textures,hidden}:{scene:PreviewScene;wireframe:boolean;showBones:boolean;selected:number|null;select:(index:number)=>void;mode:string;bone:string;textures:Record<string,Texture>;hidden:Set<number>}){
  const {gl}=useThree();
  const weights=useMemo(()=>mode==='weights'?scene.metadata.meshes.map(mesh=>weightMaterial(scene,mesh,bone,gl.capabilities.maxTextureSize)):[],[scene,mode,bone,gl]);
  useEffect(()=>()=>weights.forEach(value=>value.dispose()),[weights]);
  const geometries=useMemo(()=>scene.metadata.meshes.map(mesh=>{
    const geometry=new BufferGeometry();geometry.setAttribute('position',new BufferAttribute(floatBuffer(scene,mesh.positions),3));geometry.setAttribute('normal',new BufferAttribute(floatBuffer(scene,mesh.normals),3));
    if(mesh.uvs)geometry.setAttribute('uv',new BufferAttribute(floatBuffer(scene,mesh.uvs),2));
    if(mesh.colors)geometry.setAttribute('color',new BufferAttribute(floatBuffer(scene,mesh.colors),4));geometry.setIndex(new BufferAttribute(indexBuffer(scene,mesh.indices),1));geometry.computeBoundingSphere();return geometry;
  }),[scene]);
  const skeleton=useMemo(()=>{const positions:number[]=[];for(const bone of scene.metadata.bones){if(bone.parent>=0)positions.push(...bone.position,...scene.metadata.bones[bone.parent].position);}const geometry=new BufferGeometry();geometry.setAttribute('position',new BufferAttribute(new Float32Array(positions),3));return geometry;},[scene]);
  useEffect(()=>()=>{geometries.forEach(geometry=>geometry.dispose());skeleton.dispose();},[geometries,skeleton]);
  const size=new Box3(new Vector3(...scene.metadata.bounds.min),new Vector3(...scene.metadata.bounds.max)).getSize(new Vector3());
  const span=Math.max(size.length(),.01),center=new Vector3(...scene.metadata.bounds.min).add(new Vector3(...scene.metadata.bounds.max)).multiplyScalar(.5);
  return <>
    <ambientLight intensity={.7}/><directionalLight position={[center.x+span,center.y-span,center.z+span*2]} intensity={2.2}/><directionalLight position={[center.x-span,center.y+span,center.z+span]} intensity={.6}/>
    {geometries.map((geometry,index)=><mesh key={index} visible={!hidden.has(index)} geometry={geometry} onClick={event=>{event.stopPropagation();select(index);}}>{mode==='weights'?<primitive object={weights[index].material} attach="material"/>:<meshStandardMaterial map={mode==='textured'?textures[scene.metadata.meshes[index].material]:undefined} color={mode==='textured'&&textures[scene.metadata.meshes[index].material]?'#ffffff':selected===index?'#b9c5d4':'#969da5'} roughness={.72} metalness={0} side={DoubleSide} wireframe={wireframe} vertexColors={Boolean(scene.metadata.meshes[index].colors)}/>}</mesh>)}
    {showBones&&<lineSegments geometry={skeleton} renderOrder={2}><lineBasicMaterial color="#c5d3de" depthTest={false} transparent opacity={.75}/></lineSegments>}
    <gridHelper args={[span*2,20,'#424951','#292f35']} position={[center.x,center.y,scene.metadata.bounds.min[2]]} rotation={[Math.PI/2,0,0]}/>
  </>;
}
class ViewportBoundary extends Component<{children:ReactNode},{message:string|null}>{
  state:{message:string|null}={message:null};
  static getDerivedStateFromError(error:Error){return{message:error.message};}
  render(){return this.state.message?<div className="empty-state"><h2>3D preview could not render</h2><p>{this.state.message}</p></div>:this.props.children;}
}
export default function Viewport({scene,project,disabled=false}:{scene:PreviewScene;project?:Project;disabled?:boolean}){
  const [wireframe,setWireframe]=useState(false),[showBones,setShowBones]=useState(false),[reset,setReset]=useState(0),[selected,setSelected]=useState<number|null>(null);
  const [hidden,setHidden]=useState<Set<number>>(new Set());const generation=useRef(0);
  const [mode,setMode]=useState('solid'),[bone,setBone]=useState(''),[textures,setTextures]=useState<Record<string,Texture>>({}),[textureError,setTextureError]=useState(''),[loading,setLoading]=useState(false);
  const bones=useMemo(()=>[...new Set(scene.metadata.meshes.flatMap(mesh=>mesh.skin?.joint_names??[]))].sort(),[scene]);
  useEffect(()=>()=>Object.values(textures).forEach(texture=>texture.dispose()),[textures]);
  useEffect(()=>{generation.current++;setTextures({});setTextureError('');setBone('');setMode('solid');setHidden(new Set());return()=>{generation.current++;};},[scene]);
  async function loadTextures(){const version=generation.current;setLoading(true);setTextureError('');const loaded:Record<string,Texture>={};const failures:string[]=[];try{for(const material of new Set(scene.metadata.meshes.map(mesh=>mesh.material))){const assignment=project?.materials[material];const path=assignment?.textures?.color;if(!path||version!==generation.current)continue;try{const bytes=await api.previewImage(path,1024);const url=URL.createObjectURL(new Blob([bytes],{type:'image/png'}));try{const texture=await new TextureLoader().loadAsync(url);texture.colorSpace=SRGBColorSpace;texture.flipY=false;loaded[material]=texture;}finally{URL.revokeObjectURL(url);}}catch(error){failures.push(`${material}: ${error instanceof Error?error.message:JSON.stringify(error)}`);}}if(version!==generation.current){Object.values(loaded).forEach(texture=>texture.dispose());return;}setTextures(loaded);setMode('textured');setTextureError(failures.join('\n')||(!Object.keys(loaded).length?'No assigned color images could be loaded. Assign a PNG or JPEG color texture first.':''));}finally{if(version===generation.current)setLoading(false);}}
  const mesh=selected!==null?scene.metadata.meshes[selected]:null;
  return <div className="preview-shell"><div className="viewport"><ViewportBoundary key={`${scene.metadata.source_sha256}:${mode}`}><Canvas frameloop="demand" dpr={[1,1.5]} gl={{antialias:true}} camera={{position:[2,-4,2],up:[0,0,1],fov:45}} fallback={<div className="empty-state">A WebGL 2 capable graphics driver is required for this preview.</div>}><color attach="background" args={['#111519']}/><Model scene={scene} wireframe={wireframe} showBones={showBones} selected={selected} select={setSelected} mode={mode} bone={bone} textures={textures} hidden={hidden}/><Controls scene={scene} reset={reset}/></Canvas></ViewportBoundary></div><div className="viewport-toolbar"><span className="muted">Bind / rest pose</span><select aria-label="Preview shading" value={mode} onChange={event=>setMode(event.target.value)}><option value="solid">Solid</option><option value="textured">Mapped textures</option><option value="weights" disabled={!bones.length}>Skin weights</option></select><div className="row"><Check label="Skeleton" value={showBones} onChange={setShowBones} disabled={false}/><Check label="Wireframe" value={wireframe} onChange={setWireframe} disabled={false}/><button onClick={()=>setReset(value=>value+1)}>Reset view</button><button disabled={!hidden.size} onClick={()=>setHidden(new Set())}>Show all meshes</button></div></div><div className="editor-fields"><SelectField label="Inspect mesh" value={selected===null?'':String(selected)} values={[["","Choose a mesh…"],...scene.metadata.meshes.map((mesh,index)=>[String(index),mesh.name+(hidden.has(index)?' (hidden)':'')] as const)]} onChange={value=>setSelected(value===''?null:Number(value))} disabled={false}/></div>{mode==='weights'&&<div className="editor-fields"><SelectField label="Weight bone" value={bone} values={[["","Choose a joint…"],...bones.map(name=>[name,name] as const)]} onChange={setBone} disabled={false}/><p className="hint">Blue: zero influence · Red: full influence. Uses the source weights without normalization.</p></div>}{mode==='textured'&&<div className="editor-fields"><button disabled={disabled||loading} onClick={()=>void loadTextures()}>{loading?'Loading assigned images…':'Load assigned color images'}</button>{textureError&&<p className="hint">{textureError}</p>}<p className="hint">Assigned color images, limited to 1024 px for preview. Native shader and normal-map appearance can differ.</p></div>}{mesh&&<div className="mesh-inspector"><strong>{mesh.name}</strong><span>{mesh.vertex_count.toLocaleString()} vertices · {(mesh.index_count/3).toLocaleString()} triangles</span><span className="mono path">{mesh.material}</span><button onClick={()=>setHidden(current=>{const next=new Set(current);if(next.has(selected!))next.delete(selected!);else next.add(selected!);return next;})}>{hidden.has(selected!)?'Show mesh':'Hide mesh'}</button><button className="tertiary" onClick={()=>setSelected(null)}>Clear selection</button></div>}<p className="hint">Drag to orbit · Right-drag to pan · Scroll to zoom. This view does not reproduce Deadlock shaders, animation graphs, or cloth simulation.</p><details className="technical"><summary>Preview details and warnings</summary>{scene.metadata.warnings.map((warning,index)=><p className="hint" key={index}>{warning}</p>)}<dl><dt>Source</dt><dd className="mono path">{scene.metadata.source}</dd><dt>Snapshot hash</dt><dd className="mono path">{scene.metadata.source_sha256}</dd></dl></details></div>;
}
