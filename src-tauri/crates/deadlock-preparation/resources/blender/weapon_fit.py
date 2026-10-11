"""Gun detection in the source and application of a native weapon-fit plan.

Detection reports candidate gun objects with a point sample; the fit itself
(frames, scale, roll, which bones to skin to) is computed by the native
application. Blender only applies the given matrix to the selected meshes and
assigns the planned skinning, so the saved prepared.blend stays editable.
"""
import re
import numpy as np
from scene_isolation import weapon_words, world_points, last_receipt

POLICY = 'weapon-fit-v1'
SAMPLE = 4096


def _pca(points):
    centre = points.mean(axis=0)
    covariance = np.cov((points - centre).T) if len(points) > 3 else np.eye(3)
    values, vectors = np.linalg.eigh(covariance)
    order = np.argsort(values)[::-1]
    return centre, np.maximum(values[order], 0.), vectors[:, order]


def _dominant(obj, deform):
    names = {g.index: g.name for g in obj.vertex_groups}
    totals = {}
    for vertex in obj.data.vertices:
        for group in vertex.groups:
            name = names.get(group.group)
            if group.weight > 0 and name in deform:
                totals[name] = totals.get(name, 0.) + group.weight
    total = sum(totals.values())
    if not total:
        return None, 0.
    name = max(totals, key=totals.get)
    return name, totals[name] / total


def candidates(meshes, rigs, height):
    """Rigid, elongated or weapon-named objects that could be a held weapon."""
    rig = rigs[0] if rigs else None
    deform = {b.name for b in rig.data.bones if b.use_deform} if rig else set()
    receipt = {row['name']: row for row in (last_receipt() or {}).get('objects', [])}
    total_vertices = sum(len(m.data.vertices) for m in meshes) or 1
    rows = []
    for obj in meshes:
        count = len(obj.data.vertices)
        if count < 8 or count > total_vertices * .6:
            continue
        points = world_points(obj)
        centre, values, vectors = _pca(points)
        extent = points @ vectors
        length = float(np.ptp(extent[:, 0]))
        width = float(max(np.ptp(extent[:, 1]), 1e-9))
        elongation = length / width
        materials = [s.material.name for s in obj.material_slots if s.material]
        words = weapon_words(obj.name, obj.data.name, *materials)
        bone, fraction = _dominant(obj, deform) if rig else (None, 0.)
        row = receipt.get(obj.name, {})
        kind = 'loose'
        if row.get('bone'):
            bone, fraction, kind = row['bone'], 1., 'bone_parent' if 'parented' in row.get('reason', '') else 'attached'
        elif bone and fraction >= .9:
            kind = 'weights'
        rigid = fraction >= .9
        plausible = bool(words) or (rigid and elongation >= 1.8 and length < height * .9 and length > height * .03)
        if not plausible:
            continue
        rows.append({'object': obj.name, 'points': points, 'vertices': count, 'bone': bone if rigid else None,
                     'kind': kind if rigid else 'skinned', 'keywords': words, 'elongation': elongation,
                     'length': length})
    # Multi-part guns: parts on the same bone whose bounds touch form one weapon.
    groups = []
    for row in rows:
        box = np.array([row['points'].min(axis=0), row['points'].max(axis=0)])
        margin = max(row['length'] * .1, height * .005)
        for group in groups:
            if group['bone'] == row['bone'] and np.all(box[0] <= group['box'][1] + margin) and np.all(group['box'][0] <= box[1] + margin):
                group['rows'].append(row)
                group['box'] = np.array([np.minimum(group['box'][0], box[0]), np.maximum(group['box'][1], box[1])])
                break
        else:
            groups.append({'bone': row['bone'], 'box': box, 'rows': [row]})
    result = []
    for group in groups:
        points = np.concatenate([r['points'] for r in group['rows']])
        stride = max(1, len(points) // SAMPLE)
        names = sorted(r['object'] for r in group['rows'])
        _, values, _ = _pca(points)
        result.append({
            'id': '+'.join(names), 'objects': names, 'vertices': int(sum(r['vertices'] for r in group['rows'])),
            'bounds': group['box'].tolist(), 'sample': points[::stride][:SAMPLE].tolist(),
            'attach_bone': group['bone'], 'attach_kind': group['rows'][0]['kind'],
            'keywords': sorted({w for r in group['rows'] for w in r['keywords']}),
            'elongation': float(np.sqrt(values[0] / max(values[1], 1e-12))),
        })
    return result


def split(meshes, plan):
    """Separate the planned weapon objects from the body meshes."""
    if not plan:
        return meshes, []
    wanted = set(plan.get('objects', []))
    found = [m for m in meshes if m.name in wanted]
    missing = wanted - {m.name for m in found}
    if missing:
        raise ValueError('The selected gun objects are no longer in the source: ' + ', '.join(sorted(missing)) +
                         '. Inspect the source again and choose the gun in the Weapon tab.')
    return [m for m in meshes if m.name not in wanted], found


def _weights(plan, points):
    skin = plan['skin']
    if skin['kind'] == 'rigid':
        return [[(skin['bone'], 1.)] for _ in points]
    from mathutils.kdtree import KDTree
    reference = skin['points']
    tree = KDTree(len(reference))
    for index, point in enumerate(reference):
        tree.insert(point, index)
    tree.balance()
    rows = []
    for point in points:
        found = tree.find_n(list(point), 3)
        merged = {}
        total = 0.
        for _, index, distance in found:
            influence = 1. / max(distance, 1e-6)
            total += influence
            for name, weight in skin['weights'][index]:
                merged[name] = merged.get(name, 0.) + weight * influence
        ordered = sorted(merged.items(), key=lambda item: (-item[1], item[0]))[:4]
        norm = sum(w for _, w in ordered)
        rows.append([(name, weight / norm) for name, weight in ordered if weight / norm > 1e-4])
    return rows


def apply(bpy, objects, plan, target_names, mat_paths):
    """Bake the fitted gun into target model space and assign its skin."""
    from mathutils import Matrix
    matrix = np.asarray(plan['matrix'], dtype=float)
    if matrix.shape != (4, 4) or not np.isfinite(matrix).all() or np.linalg.det(matrix[:3, :3]) <= 0:
        raise ValueError('The weapon fit is invalid. Reset the weapon adjustments and inspect again.')
    normal_matrix = np.linalg.inv(matrix[:3, :3]).T
    exported = []
    prefix = plan.get('mesh_prefix', 'weapon_')
    for obj in objects:
        mesh = obj.data
        if mesh.users > 1:
            obj.data = mesh = mesh.copy()
        world = np.asarray(obj.matrix_world, dtype=float)
        points = world_points(obj)
        fitted = points @ matrix[:3, :3].T + matrix[:3, 3]
        weights = _weights(plan, fitted)
        for row in weights:
            if not row or any(name not in target_names for name, _ in row):
                raise ValueError('The weapon skin references a bone that is not in the target skeleton.')
        if hasattr(mesh, 'calc_normals_split'):
            mesh.calc_normals_split()
        corners = mesh.corner_normals if hasattr(mesh, 'corner_normals') else None
        to_world = np.linalg.inv(world[:3, :3]).T
        normals = []
        for loop in mesh.loops:
            n = normal_matrix @ (to_world @ np.asarray(corners[loop.index].vector if corners else loop.normal))
            length = np.linalg.norm(n)
            if length < 1e-12:
                raise ValueError('Invalid weapon normal on ' + obj.name)
            normals.append((n / length).tolist())
        uvs = [[list(x.uv) for x in layer.data] for layer in mesh.uv_layers]
        colors = []
        for layer in mesh.color_attributes:
            if layer.domain not in {'POINT', 'CORNER'}:
                raise ValueError('Unsupported color domain: ' + layer.domain)
            colors.append([list(layer.data[l.vertex_index if layer.domain == 'POINT' else l.index].color) for l in mesh.loops])
        faces = [{'vertices': list(p.vertices), 'corners': list(p.loop_indices),
                  'material': mat_paths[obj.material_slots[p.material_index].material.name]} for p in mesh.polygons]
        for vertex, point in zip(mesh.vertices, fitted):
            vertex.co = point
        obj.parent = None
        obj.matrix_parent_inverse = Matrix.Identity(4)
        obj.matrix_world = Matrix.Identity(4)
        obj.modifiers.clear()
        obj.vertex_groups.clear()
        for name in target_names:
            obj.vertex_groups.new(name=name)
        for index, row in enumerate(weights):
            for name, weight in row:
                obj.vertex_groups[name].add([index], weight, 'REPLACE')
        if hasattr(mesh, 'normals_split_custom_set'):
            mesh.normals_split_custom_set(normals)
        tangents = []
        if mesh.uv_layers:
            mesh.calc_tangents(uvmap=mesh.uv_layers[0].name)
            tangents = [list(loop.tangent) + [loop.bitangent_sign] for loop in mesh.loops]
        name = obj.name if obj.name.startswith(prefix) else re.sub(r'[^A-Za-z0-9_.-]', '_', prefix + obj.name)[:60]
        obj.name = name
        exported.append({'name': obj.name, 'positions': fitted.tolist(), 'normals': normals, 'uvs': uvs,
                         'colors': colors, 'weights': weights, 'faces': faces, 'deformation': [],
                         'tangents': tangents, 'weapon': {'policy': POLICY, 'source_object': plan['objects']}})
    return exported
