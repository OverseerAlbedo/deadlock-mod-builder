import { useEffect, useMemo, useRef, useState } from 'react';
import { Canvas, useThree } from '@react-three/fiber';
import { Box3, BufferAttribute, BufferGeometry, Mesh, MeshStandardMaterial, PerspectiveCamera, Vector3 } from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { exactGltfLoader } from '../preview/skinning';
import { disposeGltf } from '../preview/GltfViewport';
import { PreviewBoundary } from '../preview/PreviewBoundary';
import type { GltfPreview } from '../preview/gltf-packet';
import { Check } from '../components/CustomizationPages';
import { MappingMotion } from './mapping-motion';
import { checkMapping, type MappingChecks } from './mapping-checks';
import { roleLabel } from './mapping';
import { ROLES, type Bone, type PreparationOptions, type Role } from './models';

const links:[Role,Role][]=[['pelvis','spine'],['spine','chest'],['chest','neck'],['neck','head'],...(['L','R'] as const).flatMap(side=>[
  ['chest',`upperarm_${side}`],[`upperarm_${side}`,`forearm_${side}`],[`forearm_${side}`,`hand_${side}`],['pelvis',`thigh_${side}`],[`thigh_${side}`,`shin_${side}`],[`shin_${side}`,`foot_${side}`],
] as [Role,Role][])];
const emptyBones:Bone[]=[];
function Controls({bounds,reset}:{bounds:Box3;reset:number}){
  const {camera,gl,invalidate,size}=useThree();
  useEffect(()=>{const controls=new OrbitControls(camera,gl.domElement),center=bounds.getCenter(new Vector3()),span=Math.max(bounds.getSize(new Vector3()).length(),.01);controls.target.copy(center);const vertical=(camera instanceof PerspectiveCamera?camera.fov:40)*Math.PI/360,half=Math.min(vertical,Math.atan(Math.tan(vertical)*size.width/Math.max(size.height,1))),distance=span*.55/Math.sin(half);camera.position.copy(center).add(new Vector3(.35,.2,.9).normalize().multiplyScalar(distance));camera.near=span/10000;camera.far=span*100;if('updateProjectionMatrix' in camera)camera.updateProjectionMatrix();controls.addEventListener('change',()=>invalidate());controls.update();invalidate();return()=>controls.dispose();},[bounds,reset,camera,gl,invalidate,size.width,size.height]);return null;
}
function Scene({motion,points,highlight,span,selected,select,markers,wireframe}:{motion:MappingMotion;points:Partial<Record<Role,Vector3>>;highlight:Vector3|null;span:number;selected:Role;select:(r:Role)=>void;markers:boolean;wireframe:boolean}){
  const {invalidate}=useThree();
  const geometry=useMemo(()=>{const positions:number[]=[];for(const [a,b] of links)if(points[a]&&points[b])positions.push(...points[a]!.toArray(),...points[b]!.toArray());return new BufferGeometry().setAttribute('position',new BufferAttribute(new Float32Array(positions),3));},[points]);
  useEffect(()=>()=>geometry.dispose(),[geometry]);
  useEffect(()=>{motion.gltf.scene.traverse(object=>{if(object instanceof Mesh)for(const material of Array.isArray(object.material)?object.material:[object.material])if(material instanceof MeshStandardMaterial)material.wireframe=wireframe;});invalidate();},[motion,points,highlight,markers,wireframe,invalidate]);
  return <><primitive object={motion.gltf.scene}/>{markers&&<>
    <lineSegments geometry={geometry} renderOrder={10}><lineBasicMaterial depthTest={false} color="#a6bbcb" transparent opacity={.7}/></lineSegments>
    {ROLES.map(role=>points[role]&&<mesh key={role} position={points[role]} renderOrder={11} onClick={e=>{e.stopPropagation();select(role);}}><sphereGeometry args={[span*(role===selected ? .012 : .007),12,8]}/><meshBasicMaterial depthTest={false} color={role===selected?'#ffffff':role.endsWith('_L')?'#64d5c2':role.endsWith('_R')?'#f4b975':'#89adff'}/></mesh>)}
    {highlight&&<mesh position={highlight} renderOrder={12}><sphereGeometry args={[span*.011,12,8]}/><meshBasicMaterial depthTest={false} wireframe color="#ff71d7"/></mesh>}
  </>}</>;
}
export default function MappingViewport({preview,target,sourceBones=emptyBones,options,selected,select,selectedBone}:{preview:GltfPreview;target:Bone[];sourceBones?:Bone[];options:PreparationOptions;selected:Role;select:(role:Role)=>void;selectedBone:string}){
  const [loaded,setLoaded]=useState<{motion:MappingMotion;bounds:Box3}|null>(null),[error,setError]=useState('');
  const [movement,setMovement]=useState<Role|'secondary'|null>(null),[secondaryJoint,setSecondaryJoint]=useState(''),[degrees,setDegrees]=useState(30),[axis,setAxis]=useState<'X'|'Y'|'Z'>('Y'),[reset,setReset]=useState(0),[markers,setMarkers]=useState(true),[wireframe,setWireframe]=useState(false);
  const [checks,setChecks]=useState<MappingChecks|null>(null),[checking,setChecking]=useState(false),[checkError,setCheckError]=useState(''),[checkRun,setCheckRun]=useState(0);
  const checkKey=JSON.stringify([options.mapping,options.source_roles,options.target_roles,options.preserve_physics_bones,options.physics_roots]);
  const latest=useRef({options,movement,degrees,axis,secondaryJoint});latest.current={options,movement,degrees,axis,secondaryJoint};
  const runningCheck=useRef<Promise<void>|null>(null);
  useEffect(()=>{let stale=false,dispose:()=>void=()=>{};const exact=exactGltfLoader();setLoaded(null);setError('');setMovement(null);
    void exact.loader.parseAsync(preview.glb,'').then(gltf=>{
      dispose=()=>{disposeGltf(gltf);exact.dispose();};if(stale){dispose();return;}
      try{const motion=new MappingMotion(gltf,target,sourceBones);dispose=()=>{motion.dispose();disposeGltf(gltf);exact.dispose();};setLoaded({motion,bounds:new Box3().setFromObject(gltf.scene)});}catch(cause){dispose();throw cause;}
    }).catch(cause=>{if(!stale)setError(cause instanceof Error?cause.message:String(cause));});
    return()=>{stale=true;dispose();exact.dispose();};
  },[preview,target,sourceBones]);
  useEffect(()=>{
    if(!loaded)return;let stale=false;setChecks(null);setCheckError('');setChecking(true);
    const timer=setTimeout(()=>{const previous=runningCheck.current;const run=async()=>{await previous;if(stale)return;const value=await checkMapping(loaded.motion,options,()=>stale);if(!stale)setChecks(value);};runningCheck.current=run().catch(cause=>{if(!stale)setCheckError(cause instanceof Error?cause.message:String(cause));}).finally(()=>{
      if(!stale){const value=latest.current;if(value.movement==='secondary')loaded.motion.bendPreserved(value.options,value.secondaryJoint,value.degrees,value.axis);else loaded.motion.pose(value.options,value.movement,value.degrees,value.axis);setChecking(false);setReset(n=>n+1);}
    });},180);
    return()=>{stale=true;clearTimeout(timer);};
  },[loaded,checkKey,checkRun]);
  const pose=useMemo(()=>{if(!loaded)return null;const result=movement==='secondary'?loaded.motion.bendPreserved(options,secondaryJoint,degrees,axis):loaded.motion.pose(options,movement,degrees,axis),points:Partial<Record<Role,Vector3>>={};for(const role of ROLES){const name=options.source_roles?.[role],point=name?loaded.motion.point(name):null;if(point)points[role]=point;}return {...result,points,highlight:loaded.motion.point(movement==='secondary'?secondaryJoint:selectedBone)};},[loaded,options,movement,degrees,axis,selectedBone,secondaryJoint]);
  if(error)return <p role="alert">Mapping preview could not be rendered: {error}</p>;
  if(!loaded||!pose)return <p className="quiet-empty">Loading source geometry and joints…</p>;
  const span=Math.max(loaded.bounds.getSize(new Vector3()).length(),.01),source=options.source_roles?.[selected]||'Unassigned',destination=options.target_roles?.[selected]||'Unassigned';
  const preservedGroups=loaded.motion.preserved(options),preservedNames=new Set(preservedGroups.flatMap(g=>g.source_bones));
  function probe(role:Role,rotation:'X'|'Y'|'Z',angle:number){setMovement(role);setAxis(rotation);setDegrees(angle);select(role);}
  return <div className="preview-shell mapping-preview">
    <p className="mapping-selection"><strong>{roleLabel(selected)}</strong> · {source} → {destination}</p>
    {selectedBone&&<p className="hint">Selected weighted bone: <strong>{selectedBone}</strong> → {preservedNames.has(selectedBone)?'Kept as an independent bone':options.mapping?.[selectedBone]||'Needs a target'}{!preservedNames.has(selectedBone)&&options.mapping_anchors?.[selectedBone]?` · follows ${options.mapping_anchors[selectedBone]}, keeping its offset`:''}</p>}
    <PreviewBoundary><div className="viewport"><Canvas frameloop="demand" dpr={[1,1.5]} camera={{fov:40,up:[0,1,0]}}><color attach="background" args={['#111519']}/><ambientLight intensity={1.5}/><directionalLight position={[6,10,8]} intensity={2}/><directionalLight position={[-6,4,-8]} intensity={.7}/><Controls bounds={loaded.bounds} reset={reset}/><Scene motion={loaded.motion} points={pose.points} highlight={pose.highlight} span={span} selected={selected} select={select} markers={markers} wireframe={wireframe}/></Canvas></div></PreviewBoundary>
    <div className="viewport-toolbar"><div className="row"><Check label="Role markers" value={markers} onChange={setMarkers} disabled={false}/><Check label="Wireframe" value={wireframe} onChange={setWireframe} disabled={false}/><button onClick={()=>setReset(v=>v+1)}>Reset view</button></div><span className="hint">Click a marker · Drag to orbit · Scroll to zoom</span></div>
    <p className="hint">White: selected role · Green: left · Orange: right · Blue: torso · Pink: selected weighted bone</p>
    {!pose.points[selected]&&<p role="status">This role has no matching source joint in the preview.</p>}
    <fieldset className="mapping-probes" aria-label="Mapping movement tests" disabled={checking}>
      <button aria-pressed={!movement} onClick={()=>setMovement(null)}>Rest pose</button>
      <button aria-pressed={movement==='head'} onClick={()=>probe('head','Y',35)}>Turn head</button>
      <button aria-pressed={movement==='upperarm_L'} onClick={()=>probe('upperarm_L','Z',40)}>Left arm</button>
      <button aria-pressed={movement==='upperarm_R'} onClick={()=>probe('upperarm_R','Z',-40)}>Right arm</button>
      <button aria-pressed={movement==='shin_L'} onClick={()=>probe('shin_L','X',-45)}>Left knee</button>
      <button aria-pressed={movement==='shin_R'} onClick={()=>probe('shin_R','X',-45)}>Right knee</button>
    </fieldset>
    {preservedGroups.length>0&&<div className="editor-fields"><label>Test preserved joint<select aria-label="Test preserved joint" disabled={checking} value={secondaryJoint} onChange={e=>{setSecondaryJoint(e.target.value);setMovement(e.target.value?'secondary':null);setDegrees(20);setAxis('X');}}><option value="">Choose a joint to bend…</option>{preservedGroups.map(g=><optgroup key={g.root} label={g.root}>{g.source_bones.filter(n=>loaded.motion.bones.has(n)).map(n=><option key={n} value={n}>{n}</option>)}</optgroup>)}</select></label><p className="hint">Moves the selected joint and its children using the existing skin weights. Source-space diagnostic only; no physics simulation.</p></div>}
    {movement&&<div className="mapping-angle"><label>Movement angle<input disabled={checking} aria-label="Movement angle" type="range" min={-80} max={80} step={1} value={degrees} onChange={e=>setDegrees(Number(e.target.value))}/><output>{degrees}°</output></label><label>Preview axis<select disabled={checking} aria-label="Preview axis" value={axis} onChange={e=>setAxis(e.target.value as typeof axis)}>{['X','Y','Z'].map(a=><option key={a}>{a}</option>)}</select></label></div>}
    <p role="status" className="hint">{pose.message}</p>
    <div className="mapping-checks"><strong>Automatic movement checks</strong>
      {checking?<p role="status">Checking head, torso, arms and legs…</p>:<button onClick={()=>setCheckRun(n=>n+1)}>Run movement checks again</button>}
      {checkError&&<p role="alert">{checkError}</p>}
      {checks&&<><p>{checks.cases.length} poses checked · {checks.cases.filter(c=>!c.finite||c.severeEdges>0||c.moved===0).length} need visual review.</p><details><summary>Show checked movements</summary><div className="table-scroll"><table><thead><tr><th>Movement</th><th>Moved samples</th><th>Severe edge strain</th></tr></thead><tbody>{checks.cases.map((c,i)=><tr key={i}><td><button disabled={checking} onClick={()=>probe(c.role,c.axis,c.degrees)}>{c.label}</button></td><td>{c.moved.toLocaleString()}</td><td>{c.finite?c.severeEdges.toLocaleString():'Non-finite geometry'}</td></tr>)}</tbody></table></div></details><p className="hint">{checks.sampledVertices.toLocaleString()} vertex samples and {checks.sampledEdges.toLocaleString()} edge samples per pose. Severe strain means over 4× extension or below 0.1× rest length. These checks flag likely distortion; they do not test collisions, final bind conversion or game physics.</p></>}
    </div>
    <p className="hint">This movement test shows your original model. {options.fit_mode==='preserve_proportions'?'Your fitting choice keeps the source torso and leg proportions. Enabled arm fitting aligns the converted arms to the hero’s joints.':'Conversion fits it to the hero’s rest pose and joint spacing, which can change the silhouette.'} Use “Preview converted model” in Prepared output to review that shape before building. Game animation and cloth simulation are not shown here.</p>
    {preview.metadata.warnings.length>0&&<details className="technical"><summary>Source preview notes</summary><ul>{preview.metadata.warnings.map((warning,i)=><li key={i}>{warning}</li>)}</ul></details>}
  </div>;
}
