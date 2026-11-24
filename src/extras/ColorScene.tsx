/** The Colors page's 3D view: the character with each material's (recolored) base color and
 * its tint as the material color. Click a part to choose its material; while a color is being
 * picked, a click reads the texture coordinates of the small area under it on the clicked part
 * instead (through a layer of another material just in front of the selected one: colorSample.ts). Pointing at a part reports its
 * material (the list marks the row). A highlighted material, or the pixels a color range
 * selects, pulses in a bright color while the rest dims a little: written into the materials
 * from the render loop (`useFrame`), never through React state per frame. Loaded on demand. */
import { Component, useEffect, useMemo, useRef, type ReactNode } from "react";
import { Canvas, useFrame, useThree, type ThreeEvent } from "@react-three/fiber";
import { BufferAttribute, BufferGeometry, Color, DataTexture, DoubleSide, PerspectiveCamera, Raycaster, SRGBColorSpace, Vector2, Vector3, type Mesh, type MeshStandardMaterial, type Texture } from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { floatBuffer, indexBuffer, type PreviewScene } from "../preview/packet";
import type { Rgb } from "./colorAdapter";
import { DIM, dimLevel, fadeOut, glowLevel, prefersReducedMotion, pulsing } from "./colorGlow";
import { choosePick, samplesAround, SHELL_DEPTH_SHARE, type Cast } from "./colorSample";

/** `x`, `y`: where the click was in the view (px); `w`: the view's width. `samples`: the
 * texture coordinates of the small area around the click on the same part (the click's own
 * first); `behind`: the selected material was picked through a layer of another one in front. */
export interface ScenePick { id: string; mesh: number; u: number; v: number; x: number; y: number; w: number; samples?: { u: number; v: number }[]; behind?: boolean }
/** What lights up: a whole material, or what one color range selects in it (`selection`). */
export interface Highlight { kind: "material" | "selection"; id: string; color: Rgb }
export interface ColorSceneProps {
  scene: PreviewScene;
  /** The material id each mesh shows (null: not one of the page's materials). */
  materialOf: (index: number) => string | null;
  textures: Record<string, Texture>;
  /** Each material's tint (the material color); white when absent. */
  colors: Record<string, Rgb>;
  /** Meshes not drawn (and not clickable). */
  hidden: Set<number>;
  selected: string | null;
  select: (id: string) => void;
  picking: boolean;
  pick: (hit: ScenePick) => void;
  /** The material under the pointer (null when it leaves the character). */
  hover?: (id: string | null) => void;
  highlight?: Highlight | null;
  /** The selection map (what the range selects) for a `selection` highlight. */
  selection?: Texture;
  /** Bumped by Reset view. */
  reset: number;
  /** What the camera frames instead of the scene's own bounds (the parts shown at first), for
   * example with weapon parts drawn that lie outside them; framed again when it changes. */
  frame?: PreviewScene["metadata"]["bounds"];
  /** Bumped whenever a texture was redrawn. */
  version: number;
}

function Controls({ scene, reset, frame }: { scene: PreviewScene; reset: number; frame?: PreviewScene["metadata"]["bounds"] }) {
  const { camera, gl, invalidate, size } = useThree();
  const bounds = frame ?? scene.metadata.bounds;
  const aspect = size.width / Math.max(1, size.height);
  useEffect(() => {
    const center = new Vector3(...bounds.min).add(new Vector3(...bounds.max)).multiplyScalar(0.5);
    const radius = Math.max(new Vector3(...bounds.max).distanceTo(new Vector3(...bounds.min)) * 0.5, 0.01);
    camera.up.set(0, 0, 1);
    // Facing the character from the front (game models face +X), a little from the side and
    // above, far enough for the whole height (and width, in a narrow view) to fit.
    const height = bounds.max[2] - bounds.min[2], width = Math.max(bounds.max[0] - bounds.min[0], bounds.max[1] - bounds.min[1]);
    const fov = camera instanceof PerspectiveCamera ? camera.fov : 35;
    const distance = Math.max(radius * 0.5, (Math.max(height, width / Math.max(aspect, 0.2)) / 2 / Math.tan((fov * Math.PI) / 360)) * 1.12 + width / 2);
    camera.position.copy(center).add(new Vector3(2.3, -0.7, 0.3).normalize().multiplyScalar(distance));
    if (camera instanceof PerspectiveCamera) { camera.near = Math.max(radius / 10000, 0.00001); camera.far = radius * 1000; camera.updateProjectionMatrix(); }
    const controls = new OrbitControls(camera, gl.domElement);
    controls.target.copy(center);
    controls.enableDamping = false;
    controls.minDistance = radius * 0.05;
    controls.maxDistance = radius * 20;
    const redraw = () => invalidate();
    controls.addEventListener("change", redraw);
    controls.update();
    invalidate();
    return () => { controls.removeEventListener("change", redraw); controls.dispose(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- framed again on Reset view, not on every resize
  }, [camera, gl, invalidate, bounds, reset]);
  return null;
}

const tintColor = (rgb: Rgb | undefined) => new Color().setRGB(...(rgb ?? [1, 1, 1]), SRGBColorSpace);
const now = () => performance.now() / 1000;
/** Seconds since the glow started: real time, or (development builds only) a fixed moment a
 * layout harness sets (`__dmbGlowAt`) to capture the pulse at its peak. */
const since = (start: number) => {
  const fixed = import.meta.env.DEV ? (globalThis as { __dmbGlowAt?: number }).__dmbGlowAt : undefined;
  return typeof fixed === "number" ? fixed : now() - start;
};
/** How strongly the glow color is added: a whole material, and the (thinner) selection. */
const GAIN = { material: 0.55, selection: 2.2 };
/** How far a lit material's own color turns toward the glow color at full level (white parts
 * would not show an added glow). */
const TOWARD = 0.65;

function Model({ scene, materialOf, textures, colors, hidden, selected, select, picking, pick, hover, highlight, selection, version }: ColorSceneProps) {
  const { invalidate, gl, camera } = useThree();
  const geometries = useMemo(() => scene.metadata.meshes.map((mesh) => {
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new BufferAttribute(floatBuffer(scene, mesh.positions), 3));
    geometry.setAttribute("normal", new BufferAttribute(floatBuffer(scene, mesh.normals), 3));
    if (mesh.uvs) geometry.setAttribute("uv", new BufferAttribute(floatBuffer(scene, mesh.uvs), 2));
    geometry.setIndex(new BufferAttribute(indexBuffer(scene, mesh.indices), 1));
    geometry.computeBoundingSphere();
    return geometry;
  }), [scene]);
  useEffect(() => () => geometries.forEach((geometry) => geometry.dispose()), [geometries]);
  // Every material keeps an emissive map bound (white, or the selection map), so lighting a
  // part never switches shaders.
  const white = useMemo(() => { const t = new DataTexture(new Uint8Array([255, 255, 255, 255]), 1, 1); t.needsUpdate = true; return t; }, []);
  useEffect(() => () => white.dispose(), [white]);
  useEffect(() => { invalidate(); }, [version, textures, colors, hidden, invalidate]);

  // The glow: what is lit, since when, and what fades out after it was let go.
  const materials = useRef<(MeshStandardMaterial | null)[]>([]);
  const latest = useRef({ colors, selection, materialOf });
  latest.current = { colors, selection, materialOf };
  const reduced = useMemo(prefersReducedMotion, []);
  const glow = useRef<{ lit: Highlight | null; since: number; fading: Highlight | null; fadingSince: number; fadeFrom: number; level: number }>({ lit: null, since: 0, fading: null, fadingSince: 0, fadeFrom: 0, level: 0 });
  const litKey = highlight ? `${highlight.kind}:${highlight.id}` : "";
  useEffect(() => {
    const state = glow.current;
    const key = state.lit ? `${state.lit.kind}:${state.lit.id}` : "";
    if (key === litKey) { if (highlight) state.lit = highlight; }
    else {
      if (state.lit) { state.fading = state.lit; state.fadingSince = now(); state.fadeFrom = state.level; }
      state.lit = highlight ?? null;
      state.since = now();
    }
    invalidate();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed by what is lit; its color is read live
  }, [litKey, highlight?.color.join(), invalidate]);
  // Pointing at a part: one raycast per animation frame while the pointer moves (not one per
  // pointer event, and none while a button is held to turn the view).
  const meshes = useRef<(Mesh | null)[]>([]);
  const hoverRef = useRef(hover);
  hoverRef.current = hover;
  const hovering = Boolean(hover);
  useEffect(() => {
    if (!hovering) return;
    const element = gl.domElement;
    const ray = new Raycaster();
    const point = new Vector2();
    let frame = 0, last: PointerEvent | null = null, current: string | null = null;
    const run = () => {
      frame = 0;
      if (!last) return;
      const started = performance.now();
      const box = element.getBoundingClientRect();
      point.set(((last.clientX - box.left) / box.width) * 2 - 1, -((last.clientY - box.top) / box.height) * 2 + 1);
      ray.setFromCamera(point, camera);
      const shown = meshes.current.filter((mesh): mesh is Mesh => Boolean(mesh));
      const hit = ray.intersectObjects(shown, false)[0];
      const index = hit ? meshes.current.indexOf(hit.object as Mesh) : -1;
      const id = index >= 0 ? latest.current.materialOf(index) : null;
      // Development builds only, read by a layout harness: the time one pointing raycast takes.
      if (import.meta.env.DEV) (globalThis as { __dmbHoverMs?: number[] }).__dmbHoverMs?.push(performance.now() - started);
      if (id !== current) { current = id; hoverRef.current?.(id); }
    };
    const move = (event: PointerEvent) => {
      if (event.buttons) return;
      last = event;
      if (!frame) frame = requestAnimationFrame(run);
    };
    const leave = () => {
      last = null;
      if (current !== null) { current = null; hoverRef.current?.(null); }
    };
    element.addEventListener("pointermove", move);
    element.addEventListener("pointerleave", leave);
    return () => { cancelAnimationFrame(frame); element.removeEventListener("pointermove", move); element.removeEventListener("pointerleave", leave); };
  }, [gl, camera, hovering]);
  const scratch = useMemo(() => new Color(), []);
  useFrame(() => {
    const state = glow.current;
    const t = now();
    const { colors: tints, selection: map, materialOf: of } = latest.current;
    const on = state.lit ? since(state.since) : 0;
    const off = state.fading ? fadeOut(t - state.fadingSince) : 0;
    if (state.fading && off <= 0) state.fading = null;
    const level = state.lit ? glowLevel(on, reduced) : 0;
    const dim = DIM * Math.max(state.lit ? dimLevel(on) : 0, off);
    state.level = level;
    const shown = state.lit ?? state.fading;
    const strength = state.lit ? level : off * state.fadeFrom;
    materials.current.forEach((material, index) => {
      if (!material) return;
      const id = of(index);
      const lit = Boolean(shown && id === shown.id);
      const tint: Rgb = (id && tints[id]) || [1, 1, 1];
      material.color.setRGB(tint[0], tint[1], tint[2], SRGBColorSpace);
      if (!lit || shown!.kind === "selection") material.color.multiplyScalar(1 - dim);
      else material.color.lerp(scratch.setRGB(...shown!.color, SRGBColorSpace), TOWARD * strength);
      const wanted = lit && shown!.kind === "selection" && map ? map : white;
      if (material.emissiveMap !== wanted) material.emissiveMap = wanted;
      if (lit && (shown!.kind === "material" || map)) {
        material.emissive.copy(scratch.setRGB(...shown!.color, SRGBColorSpace));
        material.emissiveIntensity = strength * GAIN[shown!.kind];
      } else {
        material.emissiveIntensity = 0;
      }
    });
    if ((state.lit && pulsing(on, reduced)) || state.fading) invalidate();
  });

  const min = scene.metadata.bounds.min, max = scene.metadata.bounds.max;
  const span = Math.max(new Vector3(...max).distanceTo(new Vector3(...min)), 0.01);
  const center = new Vector3(...min).add(new Vector3(...max)).multiplyScalar(0.5);
  const grid = useMemo(() => {
    const style = typeof getComputedStyle === "function" ? getComputedStyle(document.documentElement) : null;
    return [style?.getPropertyValue("--border").trim() || "#30363d", style?.getPropertyValue("--border-subtle").trim() || "#24292f"];
  }, []);
  function click(event: ThreeEvent<MouseEvent>, index: number) {
    // A drag that orbits the camera is not a click.
    if (event.delta > 5) return;
    event.stopPropagation();
    const id = materialOf(index);
    const where = { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY, w: (event.nativeEvent.target as HTMLElement | null)?.clientWidth ?? 0 };
    if (picking) {
      // Read the click as it is seen: the selected material through a layer in front of it, and
      // the small area around the click on that same part (rays at click time only).
      const read = readPick(event.nativeEvent);
      if (read) {
        const pickedId = materialOf(read.hit.mesh);
        if (pickedId) pick({ id: pickedId, mesh: read.hit.mesh, u: read.hit.u, v: read.hit.v, ...where, samples: read.samples, behind: read.behind });
        return;
      }
      if (id && event.uv) pick({ id, mesh: index, u: event.uv.x, v: event.uv.y, ...where });
      return;
    }
    if (id) select(id);
  }
  function readPick(native: MouseEvent) {
    const element = gl.domElement;
    const box = element.getBoundingClientRect();
    if (!box.width || !box.height) return null;
    const ray = new Raycaster();
    const point = new Vector2();
    const shown = meshes.current.filter((mesh): mesh is Mesh => Boolean(mesh));
    const cast: Cast = (dx, dy) => {
      point.set(((native.clientX + dx - box.left) / box.width) * 2 - 1, -((native.clientY + dy - box.top) / box.height) * 2 + 1);
      ray.setFromCamera(point, camera);
      return ray.intersectObjects(shown, false).flatMap((hit) => {
        const mesh = meshes.current.indexOf(hit.object as Mesh);
        return mesh >= 0 ? [{ mesh, distance: hit.distance, u: hit.uv?.x ?? 0, v: hit.uv?.y ?? 0 }] : [];
      });
    };
    const depth = SHELL_DEPTH_SHARE * Math.max(max[2] - min[2], 0.01);
    const chosen = choosePick(cast(0, 0), materialOf, selected, depth);
    if (!chosen) return null;
    return { ...chosen, samples: samplesAround(cast, chosen.hit, chosen.behind, depth) };
  }
  return (
    <>
      <ambientLight intensity={0.75} />
      <directionalLight position={[center.x + span * 0.4, center.y - span, center.z + span]} intensity={2} />
      <directionalLight position={[center.x - span, center.y + span, center.z + span * 0.5]} intensity={0.6} />
      <group>
        {geometries.map((geometry, index) => {
          if (hidden.has(index)) return null;
          const id = materialOf(index);
          const map = id ? textures[id] : undefined;
          return (
            <mesh key={index} ref={(mesh: Mesh | null) => { meshes.current[index] = mesh; }} geometry={geometry} onClick={(event) => click(event, index)}>
              <meshStandardMaterial
                key={map ? map.uuid : "plain"}
                ref={(material: MeshStandardMaterial | null) => { materials.current[index] = material; }}
                map={map ?? null}
                emissiveMap={white}
                emissiveIntensity={0}
                color={tintColor(id ? colors[id] : undefined)}
                roughness={0.75}
                metalness={0}
                side={DoubleSide}
              />
            </mesh>
          );
        })}
      </group>
      <gridHelper args={[span * 1.4, 14, grid[0], grid[1]]} position={[center.x, center.y, min[2]]} rotation={[Math.PI / 2, 0, 0]} />
    </>
  );
}
class SceneBoundary extends Component<{ children: ReactNode; fallback: (message: string) => ReactNode }, { message: string | null }> {
  state: { message: string | null } = { message: null };
  static getDerivedStateFromError(error: Error) { return { message: error.message }; }
  render() { return this.state.message ? this.props.fallback(this.state.message) : this.props.children; }
}
export default function ColorScene(props: ColorSceneProps) {
  return (
    <SceneBoundary fallback={(message) => <div className="color-stage-message" role="alert"><strong>The 3D view could not render</strong><p>{message}</p></div>}>
      <Canvas
        className={props.picking ? "picking" : ""}
        frameloop="demand"
        dpr={[1, 1.5]}
        gl={{ antialias: true, alpha: true }}
        camera={{ position: [2, -4, 2], up: [0, 0, 1], fov: 35 }}
        fallback={<div className="color-stage-message">A WebGL 2 capable graphics driver is required for the 3D view.</div>}
      >
        <Model {...props} />
        <Controls scene={props.scene} reset={props.reset} frame={props.frame} />
      </Canvas>
    </SceneBoundary>
  );
}
