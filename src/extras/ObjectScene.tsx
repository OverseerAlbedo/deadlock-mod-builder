/** Extras → Objects: the 3D view of the game's object and the user's fitted object, both from
 * the fit answer (already in one frame; "yours" is already fitted). The game's object reads as
 * the reference: a faint neutral fill with a dashed outline of its edges; the user's object is
 * solid in the theme's accent. Mounted once and kept: a new answer, the view mode or Reset view
 * change the scene in place (the mode by setting `visible` on the two groups, never by
 * remounting). The camera frames what the mode shows (`frameBounds`) and is framed again for
 * another object, Reset view or another mode, and when a new answer no longer fits inside the
 * box last framed; a fit change that stays inside never moves the view. Loaded on demand (lazy). */
// i18n-area: objects
import { Component, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { T } from "../i18n/T";
import { Canvas, useThree } from "@react-three/fiber";
import { BufferAttribute, BufferGeometry, Color, DoubleSide, EdgesGeometry, LineDashedMaterial, LineSegments, PerspectiveCamera, Vector3, type Group } from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { decodePreview, floatBuffer, indexBuffer, type PreviewScene } from "../preview/packet";
import type { FitAnswer } from "./objectCatalog";
import type { ObjectBounds, ObjectCollisionShape } from "./objectTypes";

export type ObjectViewMode = "game" | "yours" | "both";
export interface ObjectSceneProps {
  answer: FitAnswer;
  mode: ObjectViewMode;
  /** Bumped by Reset view. */
  reset: number;
  /** Frames again when it changes (the selected object). */
  frameKey: string;
  /** Draw the game's collision shapes (when the answer has them) as outlines in the warning colour. */
  collision?: boolean;
  /** An object with several bones: the game's animation and the frame shown (skin matrices applied in place, never a remount). */
  motion?: ObjectMotion | null;
  frame?: number;
}
/** A motion packet: per frame and bone (in `bones` order) a row-major 3×4 skin matrix (model space). */
export interface ObjectMotion { bones: string[]; frames: number; data: Float32Array }
/** Points moved by their joints' skin matrices at `frame` (linear blend; a joint the motion lacks stays put). Pure (tested). */
export function skinPositions(rest: Float32Array, pairs: Float32Array, influences: number, joints: string[], motion: ObjectMotion, frame: number): Float32Array {
  const out = new Float32Array(rest.length);
  const f = Math.max(0, Math.min(motion.frames - 1, Math.floor(frame)));
  const slot = joints.map((name) => motion.bones.indexOf(name));
  const n = motion.bones.length;
  for (let v = 0; v < rest.length / 3; v++) {
    const [x, y, z] = [rest[v * 3], rest[v * 3 + 1], rest[v * 3 + 2]];
    let [ox, oy, oz, total] = [0, 0, 0, 0];
    for (let s = 0; s < influences; s++) {
      const joint = pairs[(v * influences + s) * 2], weight = pairs[(v * influences + s) * 2 + 1];
      const b = slot[joint] ?? -1;
      if (!(weight > 0) || b < 0) continue;
      const m = motion.data.subarray((f * n + b) * 12, (f * n + b) * 12 + 12);
      ox += weight * (m[0] * x + m[1] * y + m[2] * z + m[3]);
      oy += weight * (m[4] * x + m[5] * y + m[6] * z + m[7]);
      oz += weight * (m[8] * x + m[9] * y + m[10] * z + m[11]);
      total += weight;
    }
    const keep = 1 - Math.min(1, total);
    out[v * 3] = ox + keep * x; out[v * 3 + 1] = oy + keep * y; out[v * 3 + 2] = oz + keep * z;
  }
  return out;
}

type V3 = [number, number, number];
const SEGMENTS = 32;
/** Line segment end points (pairs) outlining the game's collision shapes: a sphere as three
 * great circles; a capsule as its two end rings, four lines along it and the half circles of its
 * two caps; a hull or mesh shape as the twelve edges of its bounds. Pure (tested). */
export function collisionLines(shapes: ObjectCollisionShape[]): number[] {
  const out: number[] = [];
  const add = (a: V3, b: V3) => out.push(...a, ...b);
  const at = (c: V3, u: V3, v: V3, r: number, t: number): V3 => [0, 1, 2].map((k) => c[k] + r * (Math.cos(t) * u[k] + Math.sin(t) * v[k])) as V3;
  const arc = (c: V3, u: V3, v: V3, r: number, from: number, to: number) => {
    for (let i = 0; i < SEGMENTS; i++) add(at(c, u, v, r, from + ((to - from) * i) / SEGMENTS), at(c, u, v, r, from + ((to - from) * (i + 1)) / SEGMENTS));
  };
  for (const shape of shapes) {
    if (shape.shape === "sphere") {
      const [x, y, z]: V3[] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
      arc(shape.center, x, y, shape.radius, 0, 2 * Math.PI);
      arc(shape.center, y, z, shape.radius, 0, 2 * Math.PI);
      arc(shape.center, z, x, shape.radius, 0, 2 * Math.PI);
      continue;
    }
    if (shape.shape === "box") {
      // The twelve edges of the bounds.
      const corner = (n: number): V3 => [0, 1, 2].map((k) => ((n >> k) & 1 ? shape.max[k] : shape.min[k])) as V3;
      for (let n = 0; n < 8; n++) for (const k of [0, 1, 2]) if (!((n >> k) & 1)) add(corner(n), corner(n | (1 << k)));
      continue;
    }
    const axis = new Vector3(...shape.end).sub(new Vector3(...shape.start));
    const length = axis.length();
    const d = length > 1e-9 ? axis.clone().normalize() : new Vector3(0, 0, 1);
    const helper = Math.abs(d.z) < 0.9 ? new Vector3(0, 0, 1) : new Vector3(1, 0, 0);
    const u = new Vector3().crossVectors(d, helper).normalize(), v = new Vector3().crossVectors(d, u).normalize();
    const [U, W, D] = [u.toArray() as V3, v.toArray() as V3, d.toArray() as V3];
    const minus = (p: V3): V3 => [-p[0], -p[1], -p[2]];
    arc(shape.start, U, W, shape.radius, 0, 2 * Math.PI);
    arc(shape.end, U, W, shape.radius, 0, 2 * Math.PI);
    const off = (p: V3, side: V3): V3 => [0, 1, 2].map((k) => p[k] + shape.radius * side[k]) as V3;
    for (const side of [U, W, minus(U), minus(W)]) add(off(shape.start, side), off(shape.end, side));
    // The caps: half circles beyond each end.
    arc(shape.end, U, D, shape.radius, 0, Math.PI);
    arc(shape.end, W, D, shape.radius, 0, Math.PI);
    arc(shape.start, U, minus(D), shape.radius, 0, Math.PI);
    arc(shape.start, W, minus(D), shape.radius, 0, Math.PI);
  }
  return out;
}

/** A theme colour as three.js reads it: the computed colour of a probe element, so tokens
 * written as `hsl(calc(...))` resolve. */
function themeColor(token: string, fallback: string): string {
  if (typeof document === "undefined" || typeof getComputedStyle !== "function") return fallback;
  const probe = document.createElement("span");
  probe.style.color = token.startsWith("--") ? `var(${token}, ${fallback})` : token;
  probe.style.display = "none";
  document.body.appendChild(probe);
  const value = getComputedStyle(probe).color;
  probe.remove();
  return value || fallback;
}

function geometriesOf(scene: PreviewScene) {
  return scene.metadata.meshes.map((mesh) => {
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new BufferAttribute(floatBuffer(scene, mesh.positions), 3));
    geometry.setAttribute("normal", new BufferAttribute(floatBuffer(scene, mesh.normals), 3));
    geometry.setIndex(new BufferAttribute(indexBuffer(scene, mesh.indices), 1));
    geometry.computeBoundingSphere();
    if (mesh.skin) geometry.userData = { rest: floatBuffer(scene, mesh.positions).slice(), pairs: floatBuffer(scene, mesh.skin.data), influences: mesh.skin.influences, joints: mesh.skin.joint_names };
    return geometry;
  });
}
/** Both objects' bounds together (the game's, plus yours when drawn). */
export function viewBounds(answer: FitAnswer): ObjectBounds {
  const a = answer.view.game_bounds, b = answer.yours ? answer.view.yours_bounds : undefined;
  const all: ObjectBounds[] = [a, ...(b ? [b] : []), ...(answer.view.collision ?? []).map(shapeBounds)];
  if (all.length === 1) return a;
  return { min: [0, 1, 2].map((i) => Math.min(...all.map((x) => x.min[i]))) as V3, max: [0, 1, 2].map((i) => Math.max(...all.map((x) => x.max[i]))) as V3 };
}
/** The box the camera frames for a view mode: the game's object in "game", the user's fitted
 * object in "yours" (the game's while there is none), both in "both"; the collision shapes are
 * added when they are drawn. Pure (tested). */
export function frameBounds(answer: FitAnswer, mode: ObjectViewMode, collision = false): ObjectBounds {
  const game = answer.view.game_bounds, yours = answer.yours ? answer.view.yours_bounds : undefined;
  const objects = mode === "game" || !yours ? [game] : mode === "yours" ? [yours] : [game, yours];
  return unionBounds([...objects, ...(collision ? (answer.view.collision ?? []).map(shapeBounds) : [])]);
}
/** Whether `inner` lies inside `outer` (edges included). Pure (tested). */
export function boundsInside(inner: ObjectBounds, outer: ObjectBounds): boolean {
  return [0, 1, 2].every((i) => inner.min[i] >= outer.min[i] && inner.max[i] <= outer.max[i]);
}
function unionBounds(all: ObjectBounds[]): ObjectBounds {
  if (all.length === 1) return all[0];
  return { min: [0, 1, 2].map((i) => Math.min(...all.map((x) => x.min[i]))) as V3, max: [0, 1, 2].map((i) => Math.max(...all.map((x) => x.max[i]))) as V3 };
}
function shapeBounds(shape: ObjectCollisionShape): ObjectBounds {
  if (shape.shape === "box") return { min: shape.min, max: shape.max };
  const points = shape.shape === "sphere" ? [shape.center] : [shape.start, shape.end];
  return {
    min: [0, 1, 2].map((i) => Math.min(...points.map((p) => p[i])) - shape.radius) as V3,
    max: [0, 1, 2].map((i) => Math.max(...points.map((p) => p[i])) + shape.radius) as V3,
  };
}

/** Frames the camera on `bounds` for another object, Reset view or another view mode, and when a
 * new answer no longer fits inside the box last framed (a smaller change keeps the view). */
function Controls({ bounds, reset, frameKey, mode }: { bounds: ObjectBounds; reset: number; frameKey: string; mode: ObjectViewMode }) {
  const { camera, gl, invalidate } = useThree();
  const boundsRef = useRef(bounds);
  boundsRef.current = bounds;
  const framed = useRef<ObjectBounds | null>(null);
  const [outgrown, setOutgrown] = useState(0);
  const boxKey = JSON.stringify(bounds);
  useEffect(() => {
    if (framed.current && !boundsInside(boundsRef.current, framed.current)) setOutgrown((n) => n + 1);
  }, [boxKey]);
  useEffect(() => {
    framed.current = boundsRef.current;
    const { min, max } = boundsRef.current;
    const center = new Vector3(...min).add(new Vector3(...max)).multiplyScalar(0.5);
    const radius = Math.max(new Vector3(...max).distanceTo(new Vector3(...min)) * 0.5, 0.01);
    camera.up.set(0, 0, 1);
    const fov = camera instanceof PerspectiveCamera ? camera.fov : 35;
    const distance = (radius / Math.sin((fov * Math.PI) / 360)) * 1.08;
    // From the side, a little above and in front: an object pointing +X reads left to right.
    camera.position.copy(center).add(new Vector3(0.25, -1, 0.35).normalize().multiplyScalar(distance));
    if (camera instanceof PerspectiveCamera) { camera.near = Math.max(radius / 1000, 0.0001); camera.far = radius * 1000; camera.updateProjectionMatrix(); }
    const controls = new OrbitControls(camera, gl.domElement);
    controls.target.copy(center);
    controls.enableDamping = false;
    controls.minDistance = radius * 0.1;
    controls.maxDistance = radius * 30;
    const redraw = () => invalidate();
    controls.addEventListener("change", redraw);
    controls.update();
    invalidate();
    return () => { controls.removeEventListener("change", redraw); controls.dispose(); };
  }, [camera, gl, invalidate, reset, frameKey, mode, outgrown]);
  return null;
}

function Objects({ answer, mode, collision, motion, frame }: { answer: FitAnswer; mode: ObjectViewMode; collision: boolean; motion?: ObjectMotion | null; frame: number }) {
  const { invalidate } = useThree();
  const shapes = useMemo(() => {
    const points = collisionLines(answer.view.collision ?? []);
    if (!points.length) return null;
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new BufferAttribute(new Float32Array(points), 3));
    const lines = new LineSegments(geometry);
    lines.computeLineDistances();
    return lines.geometry;
  }, [answer]);
  useEffect(() => () => shapes?.dispose(), [shapes]);
  const collisionGroup = useRef<Group>(null);
  const game = useMemo(() => answer.game.map((bytes) => geometriesOf(decodePreview(bytes))).flat(), [answer]);
  const yours = useMemo(() => (answer.yours ? geometriesOf(decodePreview(answer.yours)) : []), [answer]);
  const outlines = useMemo(() => game.map((geometry) => {
    const lines = new LineSegments(new EdgesGeometry(geometry, 28));
    lines.computeLineDistances();
    return lines.geometry;
  }), [game]);
  useEffect(() => () => { [...game, ...yours, ...outlines].forEach((g) => g.dispose()); }, [game, yours, outlines]);
  // The animation moves the skinned meshes in place (the rest pose without a motion).
  useEffect(() => {
    for (const geometry of [...game, ...yours]) {
      const skin = geometry.userData as { rest?: Float32Array; pairs?: Float32Array; influences?: number; joints?: string[] };
      if (!skin.rest || !skin.pairs) continue;
      const placed = motion ? skinPositions(skin.rest, skin.pairs, skin.influences ?? 1, skin.joints ?? [], motion, frame) : skin.rest;
      (geometry.getAttribute("position") as BufferAttribute).copyArray(placed).needsUpdate = true;
      geometry.computeBoundingSphere();
    }
    invalidate();
  }, [game, yours, motion, frame, invalidate]);
  const colors = useMemo(() => ({
    accent: new Color(themeColor("--accent", "#9d8bff")),
    reference: new Color(themeColor("--secondary", "#9aa3bd")),
  }), [answer]);
  const span = useMemo(() => {
    const { min, max } = answer.view.game_bounds;
    return Math.max(new Vector3(...max).distanceTo(new Vector3(...min)), 0.01);
  }, [answer]);
  const dash = useMemo(() => new LineDashedMaterial({ color: colors.reference, dashSize: span / 60, gapSize: span / 90, transparent: true, opacity: 0.95 }), [colors, span]);
  useEffect(() => () => dash.dispose(), [dash]);
  // The collision in the warning colour, dashed more coarsely than the game's outline.
  const warn = useMemo(() => new LineDashedMaterial({ color: new Color(themeColor("rgb(var(--warn-rgb))", "#d6aa40")), dashSize: span / 30, gapSize: span / 60, depthTest: false, transparent: true, opacity: 0.95 }), [span, answer]);
  useEffect(() => () => warn.dispose(), [warn]);
  const gameGroup = useRef<Group>(null);
  const yoursGroup = useRef<Group>(null);
  useEffect(() => {
    if (gameGroup.current) gameGroup.current.visible = mode !== "yours";
    if (yoursGroup.current) yoursGroup.current.visible = mode !== "game";
    if (collisionGroup.current) collisionGroup.current.visible = collision;
    invalidate();
  }, [mode, answer, collision, invalidate]);
  const { min, max } = viewBounds(answer);
  const center = new Vector3(...min).add(new Vector3(...max)).multiplyScalar(0.5);
  const size = Math.max(span, new Vector3(...max).distanceTo(new Vector3(...min)));
  const grid = useMemo(() => [themeColor("--border", "#30363d"), themeColor("--border-subtle", "#24292f")], []);
  return (
    <>
      <ambientLight intensity={0.8} />
      <directionalLight position={[center.x + size, center.y - size, center.z + size]} intensity={2.1} />
      <directionalLight position={[center.x - size, center.y + size, center.z + size * 0.4]} intensity={0.6} />
      <group ref={gameGroup}>
        {game.map((geometry, index) => (
          <mesh key={`fill${index}`} geometry={geometry} renderOrder={2}>
            <meshBasicMaterial color={colors.reference} transparent opacity={0.12} depthWrite={false} side={DoubleSide} />
          </mesh>
        ))}
        {!motion && outlines.map((geometry, index) => <lineSegments key={`edge${index}`} geometry={geometry} material={dash} renderOrder={3} />)}
      </group>
      <group ref={yoursGroup}>
        {yours.map((geometry, index) => (
          <mesh key={index} geometry={geometry} renderOrder={1}>
            <meshStandardMaterial color={colors.accent} roughness={0.55} metalness={0.05} side={DoubleSide} />
          </mesh>
        ))}
      </group>
      <group ref={collisionGroup}>{shapes && <lineSegments geometry={shapes} material={warn} renderOrder={4} />}</group>
      <gridHelper args={[size * 1.6, 16, grid[0], grid[1]]} position={[center.x, center.y, min[2]]} rotation={[Math.PI / 2, 0, 0]} />
    </>
  );
}

class SceneBoundary extends Component<{ children: ReactNode; fallback: (message: string) => ReactNode }, { message: string | null }> {
  state: { message: string | null } = { message: null };
  static getDerivedStateFromError(error: Error) { return { message: error.message }; }
  render() { return this.state.message ? this.props.fallback(this.state.message) : this.props.children; }
}
export default function ObjectScene(props: ObjectSceneProps) {
  return (
    <SceneBoundary fallback={(message) => <p className="objects-view-line" role="status"><T text="The 3D view could not draw this object: {message}" values={{ message }} /></p>}>
      <Canvas
        frameloop="demand"
        dpr={[1, 1.5]}
        gl={{ antialias: true, alpha: true }}
        camera={{ position: [2, -4, 2], up: [0, 0, 1], fov: 35 }}
        fallback={<p className="objects-view-line"><T text="A WebGL 2 capable graphics driver is required for the 3D view." /></p>}
      >
        <Objects answer={props.answer} mode={props.mode} collision={props.collision ?? false} motion={props.motion} frame={props.frame ?? 0} />
        <Controls bounds={frameBounds(props.answer, props.mode, props.collision ?? false)} reset={props.reset} frameKey={props.frameKey} mode={props.mode} />
      </Canvas>
    </SceneBoundary>
  );
}
