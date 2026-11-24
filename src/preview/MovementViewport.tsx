import { useEffect, useLayoutEffect, useMemo, useRef, useState, type MutableRefObject } from 'react';
import { createPortal } from 'react-dom';
import { ModelLabels, ModelLabelTracker, useLabelLayer, type ModelLabelSet } from '../studio/ModelLabels';
import { Canvas, useFrame, useThree } from '@react-three/fiber';
import { Bone, BufferAttribute, BufferGeometry, DoubleSide, GridHelper, Group, Matrix4, MeshStandardMaterial, PerspectiveCamera, Skeleton, SkinnedMesh, Vector3, type Texture } from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { floatBuffer, indexBuffer, type PreviewScene } from './packet';
import { JUMP_HEIGHT, MotionRig, sampleSkins, travel, type MotionClip, type MotionPacket, type PreviewNode, type Tuning, type Vec3 } from './motion';
import { ClothPlayback, ClothRig, switchMovingParts } from './clothRig';
import { EASE_SECONDS, FALL_TOP_SPEED, JUMP_GRAVITY, MOTION_LABELS, clothJumpArc, clothMotion, speedsOf, type ClothMotion } from './clothMotion';
import type { ClothPacket } from './clothPacket';
import { PreviewBoundary } from './PreviewBoundary';
import type { Project } from '../models';
import { hiddenFaceLayer, shownFaceLayer, type TextureState } from './textures';

const LABELS: Record<string, string> = Object.fromEntries(MOTION_LABELS);
const ORDER = MOTION_LABELS.map(([name]) => name);
/** The cloth preview's jump: its height in metres (one decimal) and its time in the air in seconds
 * (two decimals), both from the clip's own arc. */
function jumpWords(clip: MotionClip): {metres: string; seconds: string} {
  const arc = clothJumpArc(clip), span = arc ? arc[1] - arc[0] : 0;
  return {metres: (JUMP_GRAVITY * span * span / 8 * .0254).toFixed(1), seconds: span.toFixed(2)};
}
/** Inches per second as metres per second (one decimal). */
const metres = (inches: number) => (inches * .0254).toFixed(1);
/** What the shown motion plays and what of it is measured or assumed, in a sentence or two. */
function motionNote(clip: MotionClip, packet: MotionPacket): string {
  const {speeds, fromGame} = speedsOf(packet.header);
  const read = fromGame ? '' : ' (the hero’s speeds could not be read from the game, so this is a general estimate)';
  const sideways = (side: string) => `Moving sideways to the ${side} at ${metres(speeds.strafe)} m/s with the hero’s own lean; the game names no sideways speed, so this uses the hero’s top speed without sprint${read}. `;
  switch (clip.name) {
    case 'run': return `Running forward at ${metres(speeds.forward)} m/s, the hero’s top speed with sprint${read}, with the hero’s own forward lean; the floor passes underneath. `;
    case 'back': return `Running backwards at ${metres(speeds.back)} m/s with the hero’s own lean; the game names no backward speed, so this uses the hero’s top speed without sprint${read}. `;
    case 'left': return sideways('left');
    case 'right': return sideways('right');
    case 'stop': return `Running, then stopping: the speed and the lean ease out over ${EASE_SECONDS} s${read}. `;
    case 'jump': { const jump = jumpWords(clip); return `A jump about ${jump.metres} m high, about ${jump.seconds} s in the air, landing into the stand. `; }
    case 'fall': return `A long fall with the jump’s gravity, shown with the character kept in view; the top speed of about ${metres(FALL_TOP_SPEED)} m/s is an estimate. `;
    default: return '';
  }
}
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
      if (mesh.uvs) geometry.setAttribute('uv', new BufferAttribute(floatBuffer(scene, mesh.uvs), 2));
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

/** Per-material textured materials on the skinned meshes (the grey material where a material has no
 * image), made once per texture set, never per frame. Hidden face layers are not drawn when textured;
 * shown ones are alpha-blended. Opaque otherwise and double-sided, like the still view. */
function useLooks(scene: PreviewScene, character: ReturnType<typeof useCharacter>, textures: Record<string, Texture> | null, face: {hidden: Set<string>; shown: Set<string>}) {
  // Made and assigned in a layout effect (before the frame is drawn, so no grey flash); its cleanup
  // puts the grey material back and frees what it made, so no mesh keeps a disposed material.
  useLayoutEffect(() => {
    const byMaterial = new Map<string, MeshStandardMaterial>();
    scene.metadata.meshes.forEach((mesh, i) => {
      const object = character.meshes[i];
      object.visible = !(textures && face.hidden.has(mesh.material));
      const map = textures?.[mesh.material];
      if (!map) { object.material = character.material; return; }
      let look = byMaterial.get(mesh.material);
      if (!look) {
        look = new MeshStandardMaterial({map, color: '#ffffff', roughness: .75, metalness: .02, side: DoubleSide, ...(face.shown.has(mesh.material) ? {transparent: true, depthWrite: false} : {})});
        byMaterial.set(mesh.material, look);
      }
      object.material = look;
    });
    return () => {
      for (const object of character.meshes) { object.material = character.material; object.visible = true; }
      byMaterial.forEach(look => look.dispose());
    };
  }, [scene, character, textures, face]);
}

/** The latest pose the player drew (rest-to-pose skins and the body's offset), for the labels. */
type Posed = {skins: Float32Array | null; offset: Vec3};
/** The cloth preview's motions of a movement packet, by clip name, at the hero's speeds. */
function clothMotions(packet: MotionPacket): Map<string, ClothMotion> {
  const {speeds} = speedsOf(packet.header);
  return new Map(packet.header.clips.map(clip => [clip.name, clothMotion(packet, clip, speeds, (c, t, out) => sampleSkins(packet, c, t, out))]));
}

function Player({scene, packet, nodes, tuning, clip, playing, speed, simulate, textures, face, posed, cloth}: {scene: PreviewScene; packet: MotionPacket; nodes: PreviewNode[]; tuning: Record<string, Tuning | undefined>; clip: string; playing: boolean; speed: number; simulate: boolean; textures: Record<string, Texture> | null; face: {hidden: Set<string>; shown: Set<string>}; posed?: MutableRefObject<Posed>; cloth: ClothRig | null}) {
  const character = useCharacter(scene, packet);
  useLooks(scene, character, textures, face);
  const rig = useMemo(() => new MotionRig(packet.header, cloth ? [] : nodes, tuning, character.tips), [packet, nodes, tuning, character, cloth]);
  const time = useRef(0), drawn = useRef(false), lastLoop = useRef(0);
  const group = useRef<Group>(null), grid = useRef<GridHelper>(null);
  const shown = packet.header.clips.find(c => c.name === clip) ?? packet.header.clips[0];
  const skins = useMemo(() => new Float32Array(packet.header.bones.length * 12), [packet]);
  const posedSkins = useMemo(() => new Float32Array(packet.header.bones.length * 12), [packet]);
  const matrix = useMemo(() => new Matrix4(), []);
  // The cloth model: one simulation and one clock per packet (a slider rewrites its values in
  // place); a new motion blends in from the current pose and speed.
  const motions = useMemo(() => clothMotions(packet), [packet]);
  const playback = useMemo(() => {
    const first = shown && motions.get(shown.name);
    if (!cloth || !first) return null;
    const p = new ClothPlayback(cloth, first, packet.header.bones.length);
    p.reset();
    return p;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new clip is played on the same clock (below)
  }, [cloth, packet, motions]);
  // Paused, a new motion shows its first pose at once; playing, it blends in.
  useEffect(() => { const m = shown && motions.get(shown.name); if (playback && m) { playback.play(m, !playing); drawn.current = false; } }, [playback, motions, shown, playing]);
  useEffect(() => { cloth?.setTuning(tuning); });
  const {min, max} = scene.metadata.bounds;
  const center = new Vector3(...min).add(new Vector3(...max)).multiplyScalar(.5);
  const span = Math.max(new Vector3(...max).distanceTo(new Vector3(...min)), .01), cell = span * 3 / 30;
  useEffect(() => { time.current = 0; rig.reset(); drawn.current = false; }, [clip, rig]);
  useEffect(() => { drawn.current = false; }, [playback]);
  // Moving parts switched on or off: drawn again at once, also while paused, on the animated pose.
  const simulated = useRef(simulate);
  useEffect(() => { if (simulated.current !== simulate) { simulated.current = simulate; switchMovingParts(playback, rig, drawn); } }, [simulate, playback, rig]);
  useEffect(() => () => { character.meshes.forEach(m => m.geometry.dispose()); character.material.dispose(); character.skeleton.dispose(); }, [character]);
  useFrame((_, delta) => {
    if (!shown || (!playing && drawn.current)) return;
    drawn.current = true;
    if (cloth && playback) {
      const frame = playback.advance(delta, speed, playing);
      // The view stays with the character: the floor passes underneath; a jump shows its
      // height, a fall keeps the character in view (no floor).
      const offset: Vec3 = frame.offset;
      const out = simulate ? cloth.pose(frame.alpha, frame.skins, posedSkins) : frame.skins;
      const bones = packet.header.bones.length;
      for (let i = 0; i < bones; i++) {
        const o = i * 12;
        matrix.set(out[o], out[o + 1], out[o + 2], out[o + 3], out[o + 4], out[o + 5], out[o + 6], out[o + 7], out[o + 8], out[o + 9], out[o + 10], out[o + 11], 0, 0, 0, 1);
        character.bones[i].matrixWorld.copy(matrix);
      }
      if (posed) { posed.current.skins = out; posed.current.offset = offset; }
      if (group.current) group.current.position.z = offset[2];
      if (grid.current) { grid.current.visible = frame.floor; grid.current.position.x = center.x - (offset[0] % cell); grid.current.position.y = center.y - (offset[1] % cell); }
      return;
    }
    const dt = playing ? Math.min(delta, .1) * speed : 0;
    time.current += dt;
    const loop = time.current % (shown.frames / shown.fps);
    // A jump ends settled in the stand and starts again from the take-off (a stop from the
    // stand into the run, a fall from its start): that restart is a cut, not a motion, so
    // the parts start the loop afresh.
    if (['jump', 'stop', 'fall'].includes(shown.name) && loop < lastLoop.current) rig.reset();
    lastLoop.current = loop;
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
    if (posed) { posed.current.skins = out; posed.current.offset = offset; }
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
/** `shading`/`onShading`/`shared`: the Solid / Textures choice and the textures shared with the still view. */
/** `toolbarSlot`/`footSlot`: elements of a surrounding view card that take the toolbar and the notes
 * (through a portal), so they read as the card's bar and footer. `labels`: labels on the moving parts,
 * following the moving pose (placed from the render loop, never through React state). */
/** `cloth`: the game's cloth model from the last Analyze (simulated in place of the simple swing);
 * `clothNote`: the quiet line saying why the simple swing is shown, with Analyze again (`onAnalyze`)
 * when that is the fix. */
export default function MovementViewport({scene, packet, nodes, tuning, project, shading = 'solid', onShading, shared, toolbarSlot, footSlot, labels, cloth = null, clothNote = null, onAnalyze}: {scene: PreviewScene; packet: MotionPacket; nodes: PreviewNode[]; tuning: Record<string, Tuning | undefined>; project?: Project; shading?: string; onShading?: (mode: string) => void; shared?: TextureState; toolbarSlot?: HTMLElement | null; footSlot?: HTMLElement | null; labels?: ModelLabelSet | null; cloth?: ClothPacket | null; clothNote?: {text: string; analyze: boolean} | null; onAnalyze?: () => void}) {
  const clips = packet.header.clips;
  // One cloth simulation per packet and skeleton; the sliders only rewrite its values.
  const clothRig = useMemo(() => {
    if (!cloth) return null;
    try { return ClothRig.create(cloth, packet.header, tuning); } catch { return 'frames' as const; }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- tuning reaches a running rig through setTuning
  }, [cloth, packet]);
  const simulation = clothRig instanceof ClothRig ? clothRig : null;
  const note = clothRig && !simulation ? {text: 'Moving parts swing by a simple approximation: the cloth data of the last Analyze does not fit this prepared model. Analyze again to see the game’s cloth simulation.', analyze: true} : simulation ? null : clothNote;
  const [clip, setClip] = useState(clips.find(c => c.name === 'run')?.name ?? clips[0]?.name ?? '');
  const [playing, setPlaying] = useState(true), [speed, setSpeed] = useState(1), [simulate, setSimulate] = useState(true);
  const textured = shading === 'textured' && Boolean(shared);
  const textures = textured && shared && Object.keys(shared.textures).length ? shared.textures : null;
  const materials = project?.materials, layers = project?.customization.face_layers;
  const face = useMemo(() => {
    const names = scene.metadata.meshes.map(mesh => mesh.material);
    return {hidden: new Set(project ? names.filter(name => hiddenFaceLayer(project, name)) : []), shown: new Set(project ? names.filter(name => shownFaceLayer(project, name)) : [])};
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only the materials and face layer choices matter
  }, [scene, materials, layers]);
  const posed = useRef<Posed>({skins: null, offset: [0, 0, 0]});
  const boneIndex = useMemo(() => new Map(packet.header.bones.map((b, i) => [b.name, i])), [packet]);
  const layer = useLabelLayer();
  if (!clips.length) return <p className="hint">This hero's files have no stand, run or jump clips to play.</p>;
  const shown = clips.find(c => c.name === clip);
  const ordered = [...clips].sort((a, b) => (ORDER.indexOf(a.name) + 1 || 99) - (ORDER.indexOf(b.name) + 1 || 99));
  // A label sits at the middle of its moving bones as they are drawn (rest pose until the first frame).
  const anchor = (id: string, out: Vector3) => {
    const bones = labels?.items.find(item => item.id === id)?.bones ?? [];
    const {skins, offset} = posed.current;
    let n = 0, sx = 0, sy = 0, sz = 0;
    for (const name of bones) {
      const i = boneIndex.get(name);
      if (i === undefined) continue;
      const [x, y, z] = packet.header.bones[i].position, o = i * 12;
      if (skins) { sx += skins[o] * x + skins[o + 1] * y + skins[o + 2] * z + skins[o + 3]; sy += skins[o + 4] * x + skins[o + 5] * y + skins[o + 6] * z + skins[o + 7]; sz += skins[o + 8] * x + skins[o + 9] * y + skins[o + 10] * z + skins[o + 11] + offset[2]; }
      else { sx += x; sy += y; sz += z; }
      n++;
    }
    if (!n) return false;
    out.set(sx / n, sy / n, sz / n);
    return true;
  };
  const inBar = Boolean(toolbarSlot);
  const group = inBar ? 'studio-view-group' : 'movement-group';
  const caption = (text: string) => inBar ? null : <span aria-hidden="true">{text}</span>;
  const toolbar = <div className={inBar ? 'studio-view-tools' : 'prep-weapon-view'} role="toolbar" aria-label="Movement">
    <span className={group} role="group" aria-label="Animation">{caption('Animation')}{ordered.map(c => <button key={c.name} type="button" aria-pressed={clip === c.name} title={c.sources.join(' → ')} onClick={() => setClip(c.name)}>{LABELS[c.name] ?? c.name}</button>)}</span>
    <span className={group} role="group" aria-label="Play">{caption('Play')}<button type="button" aria-pressed={playing} onClick={() => setPlaying(!playing)}>{playing ? 'Pause' : 'Play'}</button></span>
    <span className={group} role="group" aria-label="Speed">{caption('Speed')}{SPEEDS.map(s => <button key={s} type="button" aria-pressed={speed === s} onClick={() => setSpeed(s)}>{s}×</button>)}</span>
    <span className={inBar ? 'studio-view-check' : 'movement-group'}><label className="checkbox"><input type="checkbox" checked={simulate} onChange={event => setSimulate(event.target.checked)}/>Moving parts</label></span>
    {onShading && inBar && <span className="studio-view-label studio-view-push" aria-hidden="true">Look</span>}
    {onShading && <span className={group} role="group" aria-label="Look">{caption('Look')}<button type="button" aria-pressed={!textured} onClick={() => onShading('solid')}>Solid</button><button type="button" aria-pressed={textured} onClick={() => onShading('textured')}>Textures</button></span>}
  </div>;
  const notes = <>
    {textured && shared?.status === 'loading' && <p className="hint" role="status">Loading textures…</p>}
    {textured && shared?.status !== 'loading' && shared?.message && <p className="hint">{shared.message}</p>}
    {simulation
      ? <p className="hint">{textured ? 'Colours from your assigned base-color images; the game’s lighting and shaders differ. ' : ''}{`The game’s cloth model from your last Analyze (${simulation.model.count - simulation.model.staticNodes} moving points) moves with your Fine-tune values (changes show right away). `}{shown ? motionNote(shown, packet) : ''}An estimate of the game’s cloth from the same data: check the result in game.</p>
      : <p className="hint">{textured ? 'Colours from your assigned base-color images; the game’s lighting and shaders differ. ' : ''}{nodes.length ? `${nodes.length} moving bones swing with your Fine-tune values (changes show right away). ` : 'Analyze on Build & install to swing the moving parts here. '}{shown?.name === 'run' ? 'Running at about hero speed, the floor passing underneath; an approximation' : shown?.name === 'jump' ? 'A jump of about 1.4 m, landing into the stand; an approximation' : shown && shown.name !== 'idle' ? 'Played in place; an approximation' : 'An approximation'} of the game's cloth: check the result in game.</p>}
    {note && <p className="hint movement-cloth-note" role="note">{note.text}{note.analyze && onAnalyze && <> <button type="button" className="compact" onClick={onAnalyze}>Analyze again</button></>}</p>}
  </>;
  return <PreviewBoundary><div className="movement-preview">
    {toolbarSlot ? createPortal(toolbar, toolbarSlot) : toolbar}
    <div className="movement-canvas">
      <Canvas frameloop="always" camera={{fov: 35}} dpr={[1, 1.5]}>
        <color attach="background" args={['#12171b']}/>
        <hemisphereLight args={['#f4f7f9', '#20272c', 1.4]}/>
        <directionalLight position={[60, -40, 120]} intensity={1.8}/>
        <directionalLight position={[-60, 50, 40]} intensity={.5}/>
        <Camera scene={scene}/>
        <Player scene={scene} packet={packet} nodes={nodes} tuning={tuning} clip={clip} playing={playing} speed={speed} simulate={simulate} textures={textures} face={face} posed={posed} cloth={simulation}/>
        {labels && <ModelLabelTracker layer={layer} ids={labels.items.map(item => item.id)} anchor={anchor}/>}
      </Canvas>
      {labels && <ModelLabels layer={layer} set={labels}/>}
    </div>
    {footSlot ? createPortal(notes, footSlot) : notes}
  </div></PreviewBoundary>;
}
