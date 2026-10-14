import { useEffect, useMemo, useRef, useState } from 'react';
import { Canvas, useFrame, useThree } from '@react-three/fiber';
import { Bone, BufferAttribute, BufferGeometry, GridHelper, Group, Matrix4, MeshStandardMaterial, PerspectiveCamera, Skeleton, SkinnedMesh, Vector3 } from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { floatBuffer, indexBuffer, type PreviewScene } from './packet';
import { JUMP_HEIGHT, MotionRig, sampleSkins, travel, type MotionPacket, type PreviewNode, type Tuning, type Vec3 } from './motion';
import { PreviewBoundary } from './PreviewBoundary';

const LABELS: Record<string, string> = {idle: 'Stand', run: 'Run', jump: 'Jump'};
const SPEEDS = [1, .75, .5, .25];

/** Rest-pose middle of the vertices mostly weighted to each bone (where a breast or charm swings). */
function centroids(scene: PreviewScene, index: Map<string, number>): Map<number, Vec3> {
  const sums = new Map<number, [number, number, number, number]>();
  for (const mesh of scene.metadata.meshes) {
    if (!mesh.skin) continue;
    const positions = floatBuffer(scene, mesh.positions), skin = floatBuffer(scene, mesh.skin.data), n = mesh.skin.influences;
    for (let v = 0; v < mesh.vertex_count; v++) {
      for (let k = 0; k < n; k++) {
        const at = (v * n + k) * 2;
        if (skin[at + 1] < .5) continue;
        const bone = index.get(mesh.skin.joint_names[Math.round(skin[at])]);
        if (bone === undefined) continue;
        const sum = sums.get(bone) ?? [0, 0, 0, 0];
        sum[0] += positions[v * 3]; sum[1] += positions[v * 3 + 1]; sum[2] += positions[v * 3 + 2]; sum[3]++;
        sums.set(bone, sum);
      }
    }
  }
  return new Map([...sums].map(([bone, s]) => [bone, [s[0] / s[3], s[1] / s[3], s[2] / s[3]] as Vec3]));
}

/** The prepared character as skinned meshes on one skeleton whose bone matrices
 * are the rest-to-pose skins (bone inverses stay identity). */
function useCharacter(scene: PreviewScene, packet: MotionPacket) {
  return useMemo(() => {
    const index = new Map(packet.header.bones.map((b, i) => [b.name, i]));
    const bones = packet.header.bones.map(() => { const bone = new Bone(); bone.matrixAutoUpdate = false; bone.matrixWorldAutoUpdate = false; return bone; });
    const skeleton = new Skeleton(bones, bones.map(() => new Matrix4()));
    const material = new MeshStandardMaterial({color: '#b9c3cc', roughness: .75, metalness: .02});
    const meshes = scene.metadata.meshes.map(mesh => {
      const geometry = new BufferGeometry();
      geometry.setAttribute('position', new BufferAttribute(floatBuffer(scene, mesh.positions), 3));
      geometry.setAttribute('normal', new BufferAttribute(floatBuffer(scene, mesh.normals), 3));
      geometry.setIndex(new BufferAttribute(indexBuffer(scene, mesh.indices), 1));
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
      object.bind(skeleton, new Matrix4());
      return object;
    });
    return {meshes, skeleton, bones, material, tips: centroids(scene, index)};
  }, [scene, packet]);
}

function Player({scene, packet, nodes, tuning, clip, playing, speed, simulate}: {scene: PreviewScene; packet: MotionPacket; nodes: PreviewNode[]; tuning: Record<string, Tuning | undefined>; clip: string; playing: boolean; speed: number; simulate: boolean}) {
  const character = useCharacter(scene, packet);
  const rig = useMemo(() => new MotionRig(packet.header, nodes, tuning, character.tips), [packet, nodes, tuning, character]);
  const time = useRef(0), drawn = useRef(false);
  const group = useRef<Group>(null), grid = useRef<GridHelper>(null);
  const shown = packet.header.clips.find(c => c.name === clip) ?? packet.header.clips[0];
  const skins = useMemo(() => new Float32Array(packet.header.bones.length * 12), [packet]);
  const matrix = useMemo(() => new Matrix4(), []);
  const {min, max} = scene.metadata.bounds;
  const center = new Vector3(...min).add(new Vector3(...max)).multiplyScalar(.5);
  const span = Math.max(new Vector3(...max).distanceTo(new Vector3(...min)), .01), cell = span * 3 / 30;
  useEffect(() => { time.current = 0; rig.reset(); drawn.current = false; }, [clip, rig]);
  useEffect(() => () => { character.meshes.forEach(m => m.geometry.dispose()); character.material.dispose(); character.skeleton.dispose(); }, [character]);
  useFrame((_, delta) => {
    if (!shown || (!playing && drawn.current)) return;
    drawn.current = true;
    const dt = playing ? Math.min(delta, .1) * speed : 0;
    time.current += dt;
    const loop = time.current % (shown.frames / shown.fps);
    sampleSkins(packet, shown, loop, skins);
    // A run keeps going forward (the floor passes under the character); a jump
    // rises and lands within each loop.
    const offset = travel(shown, shown.name === 'run' ? time.current : loop);
    const out = rig.step(skins, offset, dt, simulate);
    const bones = packet.header.bones.length;
    for (let i = 0; i < bones; i++) {
      const o = i * 12;
      matrix.set(out[o], out[o + 1], out[o + 2], out[o + 3], out[o + 4], out[o + 5], out[o + 6], out[o + 7], out[o + 8], out[o + 9], out[o + 10], out[o + 11], 0, 0, 0, 1);
      character.bones[i].matrixWorld.copy(matrix);
    }
    if (group.current) group.current.position.z = offset[2];
    if (grid.current) grid.current.position.x = center.x - (offset[0] % cell);
  });
  return <>
    <group ref={group}>{character.meshes.map((mesh, i) => <primitive key={i} object={mesh}/>)}</group>
    <gridHelper ref={grid} args={[span * 3, 30, '#4a545d', '#2a3137']} position={[center.x, center.y, min[2]]} rotation={[Math.PI / 2, 0, 0]}/>
  </>;
}

function Camera({scene}: {scene: PreviewScene}) {
  const {camera, gl, invalidate} = useThree();
  useEffect(() => {
    const {min, max} = scene.metadata.bounds;
    const center = new Vector3(...min).add(new Vector3(...max)).multiplyScalar(.5).add(new Vector3(0, 0, JUMP_HEIGHT * .25));
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

/** The prepared character playing the hero's stand, run and jump clips, with its
 * moving parts swinging. The floor passes under a running character and a jump
 * shows its height, so the moving parts react as they would in game. */
export default function MovementViewport({scene, packet, nodes, tuning}: {scene: PreviewScene; packet: MotionPacket; nodes: PreviewNode[]; tuning: Record<string, Tuning | undefined>}) {
  const clips = packet.header.clips;
  const [clip, setClip] = useState(clips.find(c => c.name === 'run')?.name ?? clips[0]?.name ?? '');
  const [playing, setPlaying] = useState(true), [speed, setSpeed] = useState(1), [simulate, setSimulate] = useState(true);
  if (!clips.length) return <p className="hint">This hero's files have no stand, run or jump clips to play.</p>;
  const shown = clips.find(c => c.name === clip);
  return <PreviewBoundary><div className="movement-preview">
    <div className="prep-weapon-view" role="toolbar" aria-label="Movement">
      <span>Animation</span>{clips.map(c => <button key={c.name} type="button" aria-pressed={clip === c.name} title={c.sources.join(' → ')} onClick={() => setClip(c.name)}>{LABELS[c.name] ?? c.name}</button>)}
      <span>Play</span><button type="button" aria-pressed={playing} onClick={() => setPlaying(!playing)}>{playing ? 'Pause' : 'Play'}</button>
      <span>Speed</span>{SPEEDS.map(s => <button key={s} type="button" aria-pressed={speed === s} onClick={() => setSpeed(s)}>{s}×</button>)}
      <label className="checkbox"><input type="checkbox" checked={simulate} onChange={event => setSimulate(event.target.checked)}/>Moving parts</label>
    </div>
    <div className="movement-canvas">
      <Canvas frameloop="always" camera={{fov: 35}} dpr={[1, 1.5]}>
        <color attach="background" args={['#12171b']}/>
        <hemisphereLight args={['#f4f7f9', '#20272c', 1.4]}/>
        <directionalLight position={[60, -40, 120]} intensity={1.8}/>
        <directionalLight position={[-60, 50, 40]} intensity={.5}/>
        <Camera scene={scene}/>
        <Player scene={scene} packet={packet} nodes={nodes} tuning={tuning} clip={clip} playing={playing} speed={speed} simulate={simulate}/>
      </Canvas>
    </div>
    <p className="hint">{nodes.length ? `${nodes.length} moving bones swing with your Fine-tune values (changes show right away). ` : 'Analyze on Build & install to swing the moving parts here. '}{shown?.name === 'run' ? 'Running at about hero speed, the floor passing underneath; ' : shown?.name === 'jump' ? 'A jump of about 1.4 m, landing into the stand; ' : ''}an approximation of the game's cloth: check the result in game.</p>
  </div></PreviewBoundary>;
}
