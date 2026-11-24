import { useEffect, useMemo, type MutableRefObject } from 'react';
import { Canvas, useFrame, useThree } from '@react-three/fiber';
import { Bone, BufferAttribute, BufferGeometry, Matrix4, MeshStandardMaterial, PerspectiveCamera, Skeleton, SkinnedMesh, Vector3 } from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { floatBuffer, indexBuffer, type PreviewScene } from '../preview/packet';
import { PreviewBoundary } from '../preview/PreviewBoundary';
import { packetLength, sampleClip } from './clipPacket';
import type { ClipPacket } from './motionTypes';

/** The player's time, shared by the controls and the render loop (never React state per frame). */
export interface PlayerClock {
  /** Seconds into the clip. */
  time: number;
  /** Seconds the shown clip lasts. */
  length: number;
  playing: boolean;
  speed: number;
  loop: boolean;
  /** Set by a seek, so a paused player draws once. */
  dirty: boolean;
  /** Called after each drawn frame (the controls move the time bar and the readout). */
  tick?: () => void;
  /** Called once when a clip without Loop reaches its end. */
  ended?: () => void;
}

/** The scene's geometry once (shared by both figures). */
function useGeometry(scene: PreviewScene) {
  const geometries = useMemo(() => scene.metadata.meshes.map(mesh => {
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(floatBuffer(scene, mesh.positions), 3));
    geometry.setAttribute('normal', new BufferAttribute(floatBuffer(scene, mesh.normals), 3));
    geometry.setIndex(new BufferAttribute(indexBuffer(scene, mesh.indices), 1));
    return geometry;
  }), [scene]);
  useEffect(() => () => geometries.forEach(g => g.dispose()), [geometries]);
  return geometries;
}

/** The prepared character on one skeleton whose bone matrices are a packet's rest-to-pose skins. */
function useFigure(scene: PreviewScene, geometries: BufferGeometry[], packet: ClipPacket, ghost: boolean) {
  const figure = useMemo(() => {
    const index = new Map(packet.header.bones.map((b, i) => [b.name, i]));
    const bones = packet.header.bones.map(() => { const bone = new Bone(); bone.matrixAutoUpdate = false; bone.matrixWorldAutoUpdate = false; return bone; });
    const skeleton = new Skeleton(bones, bones.map(() => new Matrix4()));
    const material = ghost
      ? new MeshStandardMaterial({color: '#9aa3ab', roughness: .8, metalness: 0, transparent: true, opacity: .28, depthWrite: false})
      : new MeshStandardMaterial({color: '#b9c3cc', roughness: .75, metalness: .02});
    const meshes = scene.metadata.meshes.map((mesh, m) => {
      const geometry = geometries[m].clone();
      const joints = new Uint16Array(mesh.vertex_count * 4), weights = new Float32Array(mesh.vertex_count * 4);
      if (mesh.skin) {
        const skin = floatBuffer(scene, mesh.skin.data), n = mesh.skin.influences;
        const palette = mesh.skin.joint_names.map(name => index.get(name) ?? 0);
        for (let v = 0; v < mesh.vertex_count; v++) {
          const rows: [number, number][] = [];
          for (let k = 0; k < n; k++) { const at = (v * n + k) * 2; if (skin[at + 1] > 1e-4) rows.push([palette[Math.round(skin[at])], skin[at + 1]]); }
          rows.sort((a, b) => b[1] - a[1]);
          const kept = rows.slice(0, 4), total = kept.reduce((s, r) => s + r[1], 0) || 1;
          kept.forEach(([joint, weight], k) => { joints[v * 4 + k] = joint; weights[v * 4 + k] = weight / total; });
          if (!kept.length) weights[v * 4] = 1;
        }
      } else {
        for (let v = 0; v < mesh.vertex_count; v++) weights[v * 4] = 1;
      }
      geometry.setAttribute('skinIndex', new BufferAttribute(joints, 4));
      geometry.setAttribute('skinWeight', new BufferAttribute(weights, 4));
      const object = new SkinnedMesh(geometry, material);
      object.frustumCulled = false;
      if (ghost) object.renderOrder = 1;
      object.bind(skeleton, new Matrix4());
      return object;
    });
    return {meshes, skeleton, bones, material, skins: new Float32Array(packet.header.bones.length * 12)};
  }, [scene, geometries, packet, ghost]);
  useEffect(() => () => { figure.meshes.forEach(m => m.geometry.dispose()); figure.material.dispose(); figure.skeleton.dispose(); }, [figure]);
  return figure;
}

const matrix = new Matrix4();
function pose(figure: ReturnType<typeof useFigure>, packet: ClipPacket, time: number) {
  const out = sampleClip(packet, Math.min(time, packetLength(packet.header.clips[0])), figure.skins);
  for (let i = 0; i < figure.bones.length; i++) {
    const o = i * 12;
    matrix.set(out[o], out[o + 1], out[o + 2], out[o + 3], out[o + 4], out[o + 5], out[o + 6], out[o + 7], out[o + 8], out[o + 9], out[o + 10], out[o + 11], 0, 0, 0, 1);
    figure.bones[i].matrixWorld.copy(matrix);
  }
}

function Figure({scene, geometries, packet, ghost, clock}: {scene: PreviewScene; geometries: BufferGeometry[]; packet: ClipPacket; ghost: boolean; clock: MutableRefObject<PlayerClock>}) {
  const figure = useFigure(scene, geometries, packet, ghost);
  // Drawn at the shared time; each figure holds at its own end.
  useFrame(() => pose(figure, packet, clock.current.time), ghost ? -1 : 0);
  return <group>{figure.meshes.map((mesh, i) => <primitive key={i} object={mesh}/>)}</group>;
}

/** Moves the shared clock (one per canvas). */
function Clock({clock}: {clock: MutableRefObject<PlayerClock>}) {
  const {invalidate} = useThree();
  useFrame((_, delta) => {
    const c = clock.current;
    if (c.playing) {
      c.time += Math.min(delta, .1) * c.speed;
      if (c.time > c.length) {
        if (c.loop && c.length > 0) c.time %= c.length;
        else { c.time = c.length; c.playing = false; c.ended?.(); }
      }
      c.tick?.();
    } else if (c.dirty) { c.dirty = false; c.tick?.(); }
    invalidate();
  }, -2);
  return null;
}

function Camera({scene}: {scene: PreviewScene}) {
  const {camera, gl, invalidate} = useThree();
  useEffect(() => {
    const {min, max} = scene.metadata.bounds;
    const center = new Vector3(...min).add(new Vector3(...max)).multiplyScalar(.5);
    const radius = Math.max(new Vector3(...max).distanceTo(new Vector3(...min)) * .5, .01) * 1.15;
    camera.up.set(0, 0, 1);
    camera.position.copy(center).add(new Vector3(radius * 1.7, -radius * 2.2, radius * .4));
    if (camera instanceof PerspectiveCamera) { camera.near = radius / 1000; camera.far = radius * 100; camera.updateProjectionMatrix(); }
    const controls = new OrbitControls(camera, gl.domElement);
    controls.target.copy(center); controls.update(); invalidate();
    return () => controls.dispose();
  }, [scene, camera, gl, invalidate]);
  return null;
}

/** The prepared character playing one clip, with the game's clip as a see-through second figure in
 * Both. The clip plays in place (root motion is not applied). */
export default function ClipViewport({scene, main, ghost, clock}: {scene: PreviewScene; main: ClipPacket; ghost: ClipPacket | null; clock: MutableRefObject<PlayerClock>}) {
  const geometries = useGeometry(scene);
  const {min, max} = scene.metadata.bounds;
  const center = new Vector3(...min).add(new Vector3(...max)).multiplyScalar(.5);
  const span = Math.max(new Vector3(...max).distanceTo(new Vector3(...min)), .01);
  const floor = Math.min(min[2], 0);
  return <PreviewBoundary><div className="clip-canvas" data-testid="clip-canvas">
    <Canvas frameloop="always" camera={{fov: 35}} dpr={[1, 1.5]} fallback={<div className="empty-state">A WebGL 2 capable graphics driver is required for this preview.</div>}>
      <color attach="background" args={['#12171b']}/>
      <hemisphereLight args={['#f4f7f9', '#20272c', 1.4]}/>
      <directionalLight position={[60, -40, 120]} intensity={1.8}/>
      <directionalLight position={[-60, 50, 40]} intensity={.5}/>
      <Camera scene={scene}/>
      <Clock clock={clock}/>
      <Figure scene={scene} geometries={geometries} packet={main} ghost={false} clock={clock}/>
      {ghost && <Figure scene={scene} geometries={geometries} packet={ghost} ghost clock={clock}/>}
      <gridHelper args={[span * 3, 30, '#4a545d', '#2a3137']} position={[center.x, center.y, floor]} rotation={[Math.PI / 2, 0, 0]}/>
    </Canvas>
  </div></PreviewBoundary>;
}
