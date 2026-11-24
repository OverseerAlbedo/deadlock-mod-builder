/** Reading a color pick on the 3D view as the user sees it: which surface under the pointer is
 * picked, and the texture coordinates of the small area around the click on that same part.
 * Pure: the view supplies `cast`, the hits of a ray through a point near the click (nearest
 * first), so this works without WebGL. */

/** One surface a ray meets: the mesh's index, its distance from the camera, its texture coordinate. */
export interface RayHit { mesh: number; distance: number; u: number; v: number }
/** The hits of a ray `dx`, `dy` screen pixels away from the click, nearest first. */
export type Cast = (dx: number, dy: number) => RayHit[];

/** How close behind a layer of another material the selected material's surface must lie to be
 * picked through it, as a share of the character's height: a shell or see-through layer drawn
 * over a part (a head's smoke, a glow), not a part on the other side of the body. */
export const SHELL_DEPTH_SHARE = 0.03;

/** The hit a pick uses. Normally the nearest surface. When a material is selected and the
 * nearest surface is another material, the selected material's nearest surface along the ray is
 * used instead if it lies within `depth` behind it (`behind`: picked through a layer in front).
 * Null when the ray meets nothing. */
export function choosePick(hits: RayHit[], materialOf: (mesh: number) => string | null, selected: string | null, depth: number): { hit: RayHit; behind: boolean } | null {
  const first = hits[0];
  if (!first) return null;
  if (!selected || materialOf(first.mesh) === selected) return { hit: first, behind: false };
  const under = hits.find((hit) => materialOf(hit.mesh) === selected && hit.distance - first.distance <= depth);
  return under ? { hit: under, behind: true } : { hit: first, behind: false };
}

/** Rays per side of the square read around a click (5 × 5), and its size in screen pixels. */
export const PICK_RAYS = 5;
export const PICK_SPAN_PX = 3;

/** The texture coordinates of the area around a click (about 3 × 3 screen pixels) that show the
 * picked part: only rays whose surface is the same mesh as `picked` (one mesh, one material), so
 * a border never mixes two parts. A ray counts when that mesh is its nearest surface, or, for a
 * pick through a layer in front (`behind`), when it lies within `depth` behind the nearest one.
 * The click's own coordinate is always first. */
export function samplesAround(cast: Cast, picked: RayHit, behind: boolean, depth: number): { u: number; v: number }[] {
  const samples = [{ u: picked.u, v: picked.v }];
  const step = PICK_SPAN_PX / PICK_RAYS;
  const half = (PICK_RAYS - 1) / 2;
  for (let row = 0; row < PICK_RAYS; row++) {
    for (let column = 0; column < PICK_RAYS; column++) {
      if (row === half && column === half) continue;
      const hits = cast((column - half) * step, (row - half) * step);
      const first = hits[0];
      if (!first) continue;
      const own = hits.find((hit) => hit.mesh === picked.mesh);
      if (!own) continue;
      if (own === first || (behind && own.distance - first.distance <= depth)) samples.push({ u: own.u, v: own.v });
    }
  }
  return samples;
}
