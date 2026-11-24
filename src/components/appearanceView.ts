/** What the Appearance page's 3D view needs from the preview packet: where each material sits
 * (label anchors) and where the head is (the Face view). Computed once per packet. */
import { floatBuffer, type PreviewScene } from '../preview/packet';
import type { ViewFrame } from '../preview/Viewport';

/** The middle of each material's vertices (at most `samples` per material read), in model space. */
export function materialCenters(scene: PreviewScene, samples = 4000): Map<string, [number, number, number]> {
  const sums = new Map<string, [number, number, number, number]>();
  for (const mesh of scene.metadata.meshes) {
    if (!mesh.vertex_count) continue;
    const positions = floatBuffer(scene, mesh.positions);
    const step = Math.max(1, Math.floor(mesh.vertex_count / samples));
    const sum = sums.get(mesh.material) ?? [0, 0, 0, 0];
    for (let i = 0; i < mesh.vertex_count; i += step) { sum[0] += positions[i * 3]; sum[1] += positions[i * 3 + 1]; sum[2] += positions[i * 3 + 2]; sum[3]++; }
    sums.set(mesh.material, sum);
  }
  return new Map([...sums].filter(([, sum]) => sum[3] > 0).map(([material, [x, y, z, n]]) => [material, [x / n, y / n, z / n]]));
}

/** The head bone: a bone named "head" (any case, with a prefix or side word around it), the first
 * such bone that is not an end or helper bone. */
export function headBone(scene: PreviewScene) {
  const bones = scene.metadata.bones;
  const exact = bones.find(bone => /^(.*[_.: ])?head$/i.test(bone.name));
  return exact ?? bones.find(bone => /head/i.test(bone.name) && !/(end|top|tip|nub|helper|twist)/i.test(bone.name)) ?? null;
}

/** The camera frame of the Face view: around the head bone, sized from the model's height. Null
 * without a head bone (the page then offers only the whole model). */
export function faceFrame(scene: PreviewScene): ViewFrame | null {
  const head = headBone(scene);
  if (!head) return null;
  const {min, max} = scene.metadata.bounds;
  const height = Math.max(max[2] - min[2], .01);
  return {center: [head.position[0], head.position[1], head.position[2] + height * .06], radius: height * .2};
}
