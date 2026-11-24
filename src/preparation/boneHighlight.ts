/** "Show what a bone moves" in the Joint check view: the part of the source model's surface that one
 * bone moves, lit on the model. Source-space diagnostic only; nothing here changes a build input.
 *
 * The weights are read on the CPU once per chosen bone from the mesh's own skin streams (JOINTS_0 /
 * WEIGHTS_0 and any further sets the exact loader kept), summed per vertex for that bone's joint
 * index in each mesh's skeleton, and written into one float attribute per geometry. The material
 * only mixes a colour by that attribute, so the light follows the skinned surface while a movement
 * test bends the model, and nothing runs per frame. */
import { BufferAttribute, Float32BufferAttribute, SkinnedMesh, type Bone as ThreeBone, type BufferGeometry, type Material, type Object3D, type WebGLProgramParametersWithUniforms, type WebGLRenderer } from 'three';

/** The attribute the material reads. */
export const LIT_ATTRIBUTE = 'dmbLit';
/** The light's colour (the view's pink for "selected weighted bone"). */
const LIT_COLOR = 'vec3(1.0, 0.443, 0.843)';
/** A vertex counts as moved from this weight on (rounding noise below it). */
const MOVED = 1e-3;

type Streams = {joints: BufferAttribute; weights: BufferAttribute}[];
const lane = (attribute: BufferAttribute, index: number, at: number) => at === 0 ? attribute.getX(index) : at === 1 ? attribute.getY(index) : at === 2 ? attribute.getZ(index) : attribute.getW(index);

/** The influence sets of a skinned geometry: skinIndex/skinWeight, then joints_N/weights_N (as the exact loader keeps them). */
export function skinStreams(geometry: BufferGeometry): Streams {
  const sets: Streams = [];
  for (let index = 0; index < 8; index++) {
    const joints = geometry.getAttribute(index ? `joints_${index}` : 'skinIndex') as BufferAttribute | undefined;
    const weights = geometry.getAttribute(index ? `weights_${index}` : 'skinWeight') as BufferAttribute | undefined;
    if (!joints || !weights) break;
    sets.push({joints, weights});
  }
  return sets;
}

/** Each vertex's weight for joint `joint` (an index into the mesh's skeleton) written into `out`;
 * returns how many vertices it moves. `joint` < 0: nothing (all zero). */
export function boneWeights(streams: Streams, count: number, joint: number, out: Float32Array): number {
  let moved = 0;
  for (let vertex = 0; vertex < count; vertex++) {
    let sum = 0;
    if (joint >= 0) for (const set of streams) for (let at = 0; at < 4; at++) if (Math.round(lane(set.joints, vertex, at)) === joint) sum += lane(set.weights, vertex, at);
    out[vertex] = sum;
    if (sum > MOVED) moved++;
  }
  return moved;
}

/** Adds the lit colour to a material's shader, after whatever it already does (the exact skin
 * patch of the source preview). Returns a function that undoes it. */
export function litMaterial(material: Material): () => void {
  const previous = material.onBeforeCompile, previousKey = material.customProgramCacheKey;
  material.onBeforeCompile = (shader: WebGLProgramParametersWithUniforms, renderer: WebGLRenderer) => {
    previous.call(material, shader, renderer);
    shader.vertexShader = `attribute float ${LIT_ATTRIBUTE};\nvarying float vDmbLit;\n` + shader.vertexShader.replace('#include <begin_vertex>', `#include <begin_vertex>\nvDmbLit = ${LIT_ATTRIBUTE};`);
    shader.fragmentShader = 'varying float vDmbLit;\n' + shader.fragmentShader.replace('#include <color_fragment>', `#include <color_fragment>\ndiffuseColor.rgb = mix(diffuseColor.rgb, ${LIT_COLOR}, clamp(vDmbLit, 0.0, 1.0) * 0.85);`);
    if (shader.fragmentShader.includes('#include <emissivemap_fragment>')) shader.fragmentShader = shader.fragmentShader.replace('#include <emissivemap_fragment>', `#include <emissivemap_fragment>\ntotalEmissiveRadiance += ${LIT_COLOR} * 0.9 * clamp(vDmbLit, 0.0, 1.0);`);
  };
  material.customProgramCacheKey = () => `${previousKey.call(material)}|dmb-lit-v1`;
  material.needsUpdate = true;
  return () => { material.onBeforeCompile = previous; material.customProgramCacheKey = previousKey; material.needsUpdate = true; };
}

/** What a highlight found for one bone: whether the bone is in the view's rig, and how many vertices it moves. */
export interface LitResult { found: boolean; moved: number }

/** The line under the view: what is lit, or that the bone moves nothing. */
export function litMessage(name: string, result: LitResult | null): string {
  if (!result) return '';
  if (!result.found) return `${name}: this bone is not in the preview.`;
  return result.moved ? `Lit: what ${name} moves.` : `${name}: this bone moves no part of the model.`;
}

/** One loaded source preview's highlight: patches its skinned meshes once, then lights one bone at a time. */
export class BoneHighlight {
  private readonly meshes: {mesh: SkinnedMesh; streams: Streams; values: Float32Array; attribute: Float32BufferAttribute}[] = [];
  private readonly undo: (() => void)[] = [];
  constructor(scene: Object3D) {
    const seen = new Set<BufferGeometry>(), patched = new Set<Material>();
    scene.traverse(object => {
      if (!(object instanceof SkinnedMesh)) return;
      // Two meshes sharing one geometry can have different joint orders: each gets its own copy.
      let copied: BufferGeometry | undefined;
      if (seen.has(object.geometry)) { const original = object.geometry; copied = original.clone(); object.geometry = copied; this.undo.push(() => { object.geometry = original; copied!.dispose(); }); }
      seen.add(object.geometry);
      // (the attribute copies the array it is given: write into its own array)
      const count = object.geometry.getAttribute('position').count, attribute = new Float32BufferAttribute(new Float32Array(count), 1), values = attribute.array as Float32Array;
      object.geometry.setAttribute(LIT_ATTRIBUTE, attribute);
      const geometry = object.geometry;
      this.undo.push(() => geometry.deleteAttribute(LIT_ATTRIBUTE));
      this.meshes.push({mesh: object, streams: skinStreams(object.geometry), values, attribute});
      for (const material of Array.isArray(object.material) ? object.material : [object.material]) if (!patched.has(material)) { patched.add(material); this.undo.push(litMaterial(material)); }
    });
  }
  /** Lights what `bone` moves (null: nothing lit). */
  show(bone: ThreeBone | null | undefined): LitResult {
    let moved = 0;
    for (const entry of this.meshes) {
      // A bone that is no joint of this mesh's skin moves none of it.
      const joint = bone ? entry.mesh.skeleton.bones.indexOf(bone) : -1;
      moved += boneWeights(entry.streams, entry.values.length, joint, entry.values);
      entry.attribute.needsUpdate = true;
    }
    return {found: Boolean(bone), moved};
  }
  dispose() { for (const step of this.undo.reverse()) step(); this.undo.length = 0; this.meshes.length = 0; }
}
