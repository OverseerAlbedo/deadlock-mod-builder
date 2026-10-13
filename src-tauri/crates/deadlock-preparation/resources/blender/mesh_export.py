"""Fast readers and writers of mesh data for the export (numpy over
foreach_get/foreach_set instead of one Python property access per corner).

The values are the ones the per-element loops read before; only the way they
are gathered changed. Results are plain lists, as the JSON receipt needs.
"""
import numpy as np

_raw_groups = {}


def _group_key(obj):
    return (obj.as_pointer(), len(obj.data.vertices), tuple(g.name for g in obj.vertex_groups))


def raw_groups(obj):
    """Each vertex's (group index, weight) pairs with a positive weight.
    Blender has no bulk reader for vertex groups, so this Python pass is the
    expensive part; it runs once per object (cached by object, vertex count and
    group layout) instead of once per caller."""
    key = _group_key(obj)
    rows = _raw_groups.get(key)
    if rows is None:
        rows = [[(g.group, float(g.weight)) for g in v.groups if g.weight > 0] for v in obj.data.vertices]
        _raw_groups[key] = rows
    return rows


def clear_raw_groups():
    _raw_groups.clear()


def group_rows(obj, deform):
    """Skin rows (bone name, weight) of the deform groups, like skin_rows."""
    names = {g.index: g.name for g in obj.vertex_groups}
    return [[(names[g], w) for g, w in row if names.get(g) in deform] for row in raw_groups(obj)]


def used_group_names(objects):
    """Names of the groups any vertex of these objects is weighted to."""
    used = set()
    for obj in objects:
        names = {g.index: g.name for g in obj.vertex_groups}
        used.update(names[g] for row in raw_groups(obj) for g, _ in row if g in names)
    return used


def flat_rows(rows):
    """(vertex index, name, weight) columns of weight rows."""
    index = np.fromiter((i for i, row in enumerate(rows) for _ in row), dtype=np.int64, count=sum(len(r) for r in rows))
    names = [n for row in rows for n, _ in row]
    weights = np.fromiter((w for row in rows for _, w in row), dtype=float, count=len(index))
    return index, names, weights


def flat_raw_groups(obj):
    """(vertex index, group index, weight) columns of raw_groups."""
    rows = raw_groups(obj)
    counts = np.fromiter((len(r) for r in rows), dtype=np.int64, count=len(rows))
    index = np.repeat(np.arange(len(rows)), counts)
    groups = np.fromiter((g for r in rows for g, _ in r), dtype=np.int64, count=int(counts.sum()))
    weights = np.fromiter((w for r in rows for _, w in r), dtype=float, count=len(groups))
    return index, groups, weights


def positions(mesh):
    out = np.empty(len(mesh.vertices) * 3)
    mesh.vertices.foreach_get('co', out)
    return out.reshape(-1, 3)


def set_positions(mesh, points):
    mesh.vertices.foreach_set('co', np.ascontiguousarray(points, dtype=np.float32).ravel())


def corner_normals(mesh):
    """Object-space corner (split) normals, (corners, 3)."""
    out = np.empty(len(mesh.loops) * 3)
    if hasattr(mesh, 'corner_normals'):
        if hasattr(mesh, 'calc_normals_split'):
            mesh.calc_normals_split()
        mesh.corner_normals.foreach_get('vector', out)
    else:
        mesh.calc_normals_split()
        mesh.loops.foreach_get('normal', out)
    return out.reshape(-1, 3)


def loop_vertices(mesh):
    out = np.empty(len(mesh.loops), dtype=np.int32)
    mesh.loops.foreach_get('vertex_index', out)
    return out


def transformed_normals(normals, matrices, error, epsilon=1e-8):
    """Normals multiplied by a 3x3 matrix (one, or one per normal) and
    normalised; `error` is raised for a normal shorter than `epsilon`."""
    normals = np.asarray(normals, dtype=float)
    matrices = np.asarray(matrices, dtype=float)
    if not len(normals):
        return []
    out = normals @ matrices.T if matrices.ndim == 2 else np.einsum('nij,nj->ni', matrices, normals)
    lengths = np.linalg.norm(out, axis=1)
    if lengths.min() <= epsilon or not np.isfinite(lengths).all():
        raise ValueError(error)
    return (out / lengths[:, None]).tolist()


def uvs(mesh):
    result = []
    for layer in mesh.uv_layers:
        out = np.empty(len(mesh.loops) * 2)
        layer.data.foreach_get('uv', out)
        result.append(out.reshape(-1, 2).tolist())
    return result


def colors(mesh, label='color'):
    result = []
    for layer in mesh.color_attributes:
        if layer.domain not in {'POINT', 'CORNER'}:
            raise ValueError('Unsupported ' + label + ' domain: ' + layer.domain)
        out = np.empty(len(layer.data) * 4)
        layer.data.foreach_get('color', out)
        out = out.reshape(-1, 4)
        if layer.domain == 'POINT':
            out = out[loop_vertices(mesh)]
        result.append(out.tolist())
    return result


def faces(mesh, material_of_slot):
    """Polygons as the export lists them: vertex indices, corner indices and
    the material path of the slot (material_of_slot: slot index -> path)."""
    count = len(mesh.polygons)
    starts = np.empty(count, dtype=np.int32)
    totals = np.empty(count, dtype=np.int32)
    slots = np.empty(count, dtype=np.int32)
    mesh.polygons.foreach_get('loop_start', starts)
    mesh.polygons.foreach_get('loop_total', totals)
    mesh.polygons.foreach_get('material_index', slots)
    vertices = loop_vertices(mesh).tolist()
    return [{'vertices': vertices[s:s + t], 'corners': list(range(s, s + t)), 'material': material_of_slot[m]}
            for s, t, m in zip(starts.tolist(), totals.tolist(), slots.tolist())]


def material_paths(obj, mat_paths):
    return [mat_paths[slot.material.name] for slot in obj.material_slots]


def tangents(mesh):
    """Tangents from the first UV map (empty without UVs)."""
    if not mesh.uv_layers:
        return []
    mesh.calc_tangents(uvmap=mesh.uv_layers[0].name)
    out = np.empty(len(mesh.loops) * 3)
    signs = np.empty(len(mesh.loops))
    mesh.loops.foreach_get('tangent', out)
    mesh.loops.foreach_get('bitangent_sign', signs)
    return np.column_stack((out.reshape(-1, 3), signs)).tolist()


def assign_weights(obj, names, rows):
    """Replace the object's vertex groups by `names`, weighted per `rows`.
    Vertices sharing a bone and a weight are added together."""
    obj.vertex_groups.clear()
    groups = {name: obj.vertex_groups.new(name=name) for name in names}
    buckets = {}
    for index, row in enumerate(rows):
        for name, weight in row:
            buckets.setdefault((name, weight), []).append(index)
    for (name, weight), indices in buckets.items():
        groups[name].add(indices, weight, 'REPLACE')


def edges(mesh):
    out = np.empty(len(mesh.edges) * 2, dtype=np.int32)
    mesh.edges.foreach_get('vertices', out)
    return out.reshape(-1, 2)
