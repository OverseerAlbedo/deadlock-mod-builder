/** "This part only" for the Colors preview: which image pixels one part of the model covers, for
 * a color range with a `part` spot. The same rule as Rust `deadlock_project::color_parts` (the
 * build): a part is one connected piece of the meshes that use a material (triangles joined
 * through shared corners, corners welded by position to 1/WELD of a model unit); the pieces at a
 * spot are those with a triangle on it (within SPOT_REACH; texture coordinates repeat), all of
 * them when several show the same pixels; a range changes only pixels whose centre lies within
 * MASK_REACH pixels of a chosen triangle, at the size of the image. Only + − × ÷, square roots,
 * floor and comparisons on numbers widened from the meshes' 32-bit floats, in Rust's order, so
 * both agree bit for bit (shared fixture `src-tauri/crates/deadlock-components/tests/fixtures/color-parts.json`). */

export const WELD = 200;
export const SPOT_REACH = 1 / 1024;
export const MASK_REACH = 1;
export const MAX_REPEATS = 4;

/** One mesh of a material: positions (x, y, z), texture coordinates (u, v) and corner indices. */
export interface PartMesh { positions: Float32Array; uvs: Float32Array; indices: Uint32Array | number[] }
/** u0, v0, u1, v1, u2, v2. */
export type UvTriangle = [number, number, number, number, number, number];

/** The piece of every triangle (one list per mesh; -1: a corner index out of range). Numbered by
 * first triangle in mesh order, as Rust `color_parts::pieces`. */
export function pieces(meshes: PartMesh[]): Int32Array[] {
  const welded = new Map<string, number>();
  const cornerIds = meshes.map((mesh) => {
    const ids = new Int32Array(Math.floor(mesh.positions.length / 3));
    const key = (x: number) => Math.floor(x * WELD + 0.5);
    for (let k = 0; k < ids.length; k++) {
      const name = `${key(mesh.positions[3 * k])},${key(mesh.positions[3 * k + 1])},${key(mesh.positions[3 * k + 2])}`;
      let id = welded.get(name);
      if (id === undefined) { id = welded.size; welded.set(name, id); }
      ids[k] = id;
    }
    return ids;
  });
  const parent = new Int32Array(welded.size).map((_, k) => k);
  const find = (a: number) => { while (parent[a] !== a) { parent[a] = parent[parent[a]]; a = parent[a]; } return a; };
  const corners = (mesh: PartMesh, ids: Int32Array, t: number): [number, number, number] | null => {
    const at = (k: number) => { const i = mesh.indices[3 * t + k]; return i === undefined || i >= ids.length ? -1 : ids[i]; };
    const a = at(0), b = at(1), c = at(2);
    return a < 0 || b < 0 || c < 0 ? null : [a, b, c];
  };
  meshes.forEach((mesh, m) => {
    for (let t = 0; t < Math.floor(mesh.indices.length / 3); t++) {
      const found = corners(mesh, cornerIds[m], t);
      if (!found) continue;
      const ra = find(found[0]), rb = find(found[1]);
      parent[rb] = ra;
      const rc = find(found[2]);
      parent[rc] = find(ra);
    }
  });
  const numbers = new Map<number, number>();
  return meshes.map((mesh, m) => {
    const list = new Int32Array(Math.floor(mesh.indices.length / 3));
    for (let t = 0; t < list.length; t++) {
      const found = corners(mesh, cornerIds[m], t);
      if (!found) { list[t] = -1; continue; }
      const root = find(found[0]);
      let number = numbers.get(root);
      if (number === undefined) { number = numbers.size; numbers.set(root, number); }
      list[t] = number;
    }
    return list;
  });
}

function uvTriangle(mesh: PartMesh, t: number): UvTriangle | null {
  const out: number[] = [];
  for (let k = 0; k < 3; k++) {
    const i = mesh.indices[3 * t + k];
    if (i === undefined || 2 * i + 1 >= mesh.uvs.length) return null;
    out.push(mesh.uvs[2 * i], mesh.uvs[2 * i + 1]);
  }
  return out as UvTriangle;
}

/** Inside the triangle, or within `reach` of one of its edges (Rust `color_parts::inside`). */
export function inside(t: UvTriangle, px: number, py: number, reach: number): boolean {
  const [ax, ay, bx, by, cx, cy] = t;
  const area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
  if (area === 0 || !Number.isFinite(area)) return false;
  const sign = area > 0 ? 1 : -1;
  const edges: [number, number, number, number][] = [[ax, ay, bx, by], [bx, by, cx, cy], [cx, cy, ax, ay]];
  if (edges.every(([x0, y0, x1, y1]) => ((x1 - x0) * (py - y0) - (y1 - y0) * (px - x0)) * sign >= 0)) return true;
  if (reach <= 0) return false;
  return edges.some(([x0, y0, x1, y1]) => {
    const dx = x1 - x0, dy = y1 - y0;
    const length = dx * dx + dy * dy;
    if (length === 0) return false;
    const along = Math.min(1, Math.max(0, ((px - x0) * dx + (py - y0) * dy) / length));
    const ex = x0 + along * dx - px, ey = y0 + along * dy - py;
    return ex * ex + ey * ey <= reach * reach;
  });
}

/** The pieces with a triangle on the spot (u, v) (Rust `color_parts::at_spot`), ascending. */
export function atSpot(meshes: PartMesh[], numbers: Int32Array[], u: number, v: number): number[] {
  const pu = u - Math.floor(u), pv = v - Math.floor(v);
  const found = new Set<number>();
  meshes.forEach((mesh, m) => {
    numbers[m].forEach((piece, t) => {
      if (piece < 0 || found.has(piece)) return;
      const w = uvTriangle(mesh, t);
      if (!w) return;
      const loU = Math.min(w[0], w[2], w[4]), hiU = Math.max(w[0], w[2], w[4]);
      const loV = Math.min(w[1], w[3], w[5]), hiV = Math.max(w[1], w[3], w[5]);
      if (![loU, hiU, loV, hiV].every(Number.isFinite)) return;
      const firstU = Math.floor(loU - SPOT_REACH), lastU = Math.min(Math.floor(hiU + SPOT_REACH), firstU + MAX_REPEATS);
      const firstV = Math.floor(loV - SPOT_REACH), lastV = Math.min(Math.floor(hiV + SPOT_REACH), firstV + MAX_REPEATS);
      for (let ku = firstU; ku <= lastU; ku++) for (let kv = firstV; kv <= lastV; kv++) {
        if (inside(w, pu + ku, pv + kv, SPOT_REACH)) { found.add(piece); return; }
      }
    });
  });
  return [...found].sort((a, b) => a - b);
}

/** The texture coordinates of every triangle of the chosen pieces, in mesh order. */
export function area(meshes: PartMesh[], numbers: Int32Array[], chosen: number[]): UvTriangle[] {
  const wanted = new Set(chosen);
  const out: UvTriangle[] = [];
  meshes.forEach((mesh, m) => numbers[m].forEach((piece, t) => {
    if (!wanted.has(piece)) return;
    const triangle = uvTriangle(mesh, t);
    if (triangle) out.push(triangle);
  }));
  return out;
}

const remEuclid = (x: number, m: number) => { const r = x % m; return r < 0 ? r + m : r; };
/** The part's image area at width × height: 1 where the range may change a pixel (rows top first). */
export function mask(triangles: UvTriangle[], width: number, height: number): Uint8Array {
  const out = new Uint8Array(width * height);
  if (!width || !height) return out;
  for (const t of triangles) {
    const p: UvTriangle = [t[0] * width, t[1] * height, t[2] * width, t[3] * height, t[4] * width, t[5] * height];
    if (!p.every(Number.isFinite)) continue;
    const x0 = Math.floor(Math.min(p[0], p[2], p[4]) - MASK_REACH);
    const x1 = Math.min(Math.floor(Math.max(p[0], p[2], p[4]) + MASK_REACH), x0 + MAX_REPEATS * width);
    const y0 = Math.floor(Math.min(p[1], p[3], p[5]) - MASK_REACH);
    const y1 = Math.min(Math.floor(Math.max(p[1], p[3], p[5]) + MASK_REACH), y0 + MAX_REPEATS * height);
    for (let y = y0; y <= y1; y++) {
      const row = remEuclid(y, height) * width;
      for (let x = x0; x <= x1; x++) if (inside(p, x + 0.5, y + 0.5, MASK_REACH)) out[row + remEuclid(x, width)] = 1;
    }
  }
  return out;
}

/** Each range's mask at one image size (null: the range reaches everywhere), or undefined when no
 * range is for one part. A part range without the model's parts reaches nothing: the build
 * refuses it rather than recolor the whole material, so the preview never shows that either. */
export function rangeMasks(ranges: { part?: { u: number; v: number } }[], parts: MaterialParts | null | undefined, width: number, height: number): (Uint8Array | null)[] | undefined {
  if (!ranges.some((range) => range.part)) return undefined;
  return ranges.map((range) => (range.part ? parts?.mask(range.part.u, range.part.v, width, height) ?? new Uint8Array(width * height) : null));
}
/** The part of a material at one spot: its pieces, their triangles and how many pieces it joins. */
export interface PartArea { pieces: number[]; triangles: UvTriangle[] }
/** The meshes of one material, with their pieces worked out once (the page keeps one per material). */
export class MaterialParts {
  readonly numbers: Int32Array[];
  private readonly masks = new Map<string, Uint8Array>();
  constructor(readonly meshes: PartMesh[]) { this.numbers = pieces(meshes); }
  /** The part at (u, v); null when the spot is on no part. */
  at(u: number, v: number): PartArea | null {
    const chosen = atSpot(this.meshes, this.numbers, u, v);
    return chosen.length ? { pieces: chosen, triangles: area(this.meshes, this.numbers, chosen) } : null;
  }
  /** The mask of the part at (u, v) at one size, kept per spot and size. */
  mask(u: number, v: number, width: number, height: number): Uint8Array | null {
    const key = `${u},${v},${width},${height}`;
    const known = this.masks.get(key);
    if (known) return known;
    const part = this.at(u, v);
    if (!part) return null;
    const made = mask(part.triangles, width, height);
    if (this.masks.size > 64) this.masks.delete(this.masks.keys().next().value!);
    this.masks.set(key, made);
    return made;
  }
}
