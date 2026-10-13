"""Gun detection in the source and application of a native weapon-fit plan.

Detection reports candidate gun objects with a point sample; the fit itself
(frames, scale, roll, which bones to skin to) is computed by the native
application. Blender only applies the given matrix to the selected meshes and
assigns the planned skinning, so the saved prepared.blend stays editable.
"""
import re
import numpy as np
from scene_isolation import weapon_words, world_points, last_receipt, triangulate_ngons
import mesh_export as mx

POLICY = 'weapon-fit-v1'
SAMPLE = 4096
# Triangle budget of a candidate's preview shape (see _shape).
SHAPE_TRIANGLES = 6000
# Whole words of part names that tell the gun's front, rear and grip
# ("Nose • front rim", "Rear bumper", "Grip panel"). Charms hang anywhere and
# are skipped.
FRONT_WORDS = {'muzzle', 'barrel', 'nose', 'front', 'emitter', 'suppressor', 'silencer', 'compensator'}
REAR_WORDS = {'rear', 'back', 'stock', 'butt', 'buttstock'}
GRIP_WORDS = {'grip', 'handle', 'handgrip'}


def _name_words(name):
    """Lower-case words of a name, split at spaces, symbols and camel case."""
    return {w.lower() for w in re.findall(r'[A-Z]+(?![a-z])|[A-Z]?[a-z]+|\d+', name)}


def _hints(rows):
    """Centres of the parts named for the gun's front, rear and grip. They
    tell the native fit which end is the muzzle (weapon.rs fit)."""
    from secondary_bones import charm_named
    sums = {}
    for row in rows:
        if charm_named(row['object']):
            continue
        words = _name_words(row['object'])
        for key, vocabulary in (('front', FRONT_WORDS), ('rear', REAR_WORDS), ('grip', GRIP_WORDS)):
            if words & vocabulary:
                total = sums.setdefault(key, [np.zeros(3), 0])
                total[0] += row['points'].sum(axis=0)
                total[1] += len(row['points'])
                break
    return {key: (total / count).tolist() for key, (total, count) in sums.items() if count}


def _shape(rows, limit=SHAPE_TRIANGLES):
    """The candidate as a small triangle mesh for the fit preview, in the
    space of its sample. Vertices in one grid cell merge (vertex clustering)
    on a coarser grid until at most `limit` triangles are left; the same
    method as weapon.rs simplify."""
    points, triangles, base = [], [], 0
    for row in rows:
        mesh = row['obj'].data
        if hasattr(mesh, 'calc_loop_triangles'):
            mesh.calc_loop_triangles()
        count = len(mesh.loop_triangles)
        if not count or len(row['points']) != len(mesh.vertices):
            continue
        corners = np.empty(count * 3, dtype=np.int32)
        mesh.loop_triangles.foreach_get('vertices', corners)
        triangles.append(corners.reshape(-1, 3).astype(np.int64) + base)
        points.append(row['points'])
        base += len(row['points'])
    if not triangles:
        return None
    points, triangles = np.concatenate(points), np.concatenate(triangles)
    low, extent = points.min(axis=0), float(np.ptp(points, axis=0).max())
    if not extent > 0:
        return None
    cells = 128
    while True:
        cell = np.floor((points - low) / (extent / cells)).astype(np.int64)
        key = (cell[:, 0] * (cells + 1) + cell[:, 1]) * (cells + 1) + cell[:, 2]
        unique, inverse = np.unique(key, return_inverse=True)
        inverse = inverse.reshape(-1)
        kept = inverse[triangles]
        kept = kept[(kept[:, 0] != kept[:, 1]) & (kept[:, 1] != kept[:, 2]) & (kept[:, 0] != kept[:, 2])]
        if len(kept):
            _, first = np.unique(np.sort(kept, axis=1), axis=0, return_index=True)
            kept = kept[np.sort(first)]
        if len(kept) <= limit or cells <= 8:
            break
        # Triangles fall roughly with the square of the cell count.
        cells = max(8, int(cells * min(.9, max(.5, (limit / len(kept)) ** .5))))
    if not len(kept):
        return None
    sums = np.zeros((len(unique), 3))
    np.add.at(sums, inverse, points)
    centres = sums / np.bincount(inverse, minlength=len(unique))[:, None]
    used, numbered = np.unique(kept, return_inverse=True)
    decimals = int(min(6, max(0, 3 - np.floor(np.log10(extent)))))
    return {'positions': np.round(centres[used], decimals).ravel().tolist(), 'triangles': numbered.reshape(-1).tolist()}


# Principled BSDF inputs the material conversion reads (material_textures.py CHANNELS).
READ_INPUTS = ('Base Color', 'Roughness', 'Metallic', 'Normal', 'Emission Color', 'Emission Strength', 'Alpha')


def _material_signature(material):
    """What the material conversion reads from a material: the images on each
    Principled BSDF input (or the input's value when nothing is connected)
    and every image in its node tree. None when it cannot be compared (no
    single Principled BSDF, or no image)."""
    tree = material.node_tree if material else None
    principled = [node for node in tree.nodes if node.type == 'BSDF_PRINCIPLED'] if tree else []
    if len(principled) != 1:
        return None

    def images(socket, depth=0):
        found = set()
        for link in socket.links:
            node = link.from_node
            if node.type == 'TEX_IMAGE' and node.image:
                found.add(node.image.as_pointer())
            if depth < 8:
                for upstream in node.inputs:
                    found |= images(upstream, depth + 1)
        return found
    signature = []
    for name in READ_INPUTS:
        socket = principled[0].inputs.get(name)
        if socket is None:
            continue
        if socket.is_linked:
            signature.append((name, tuple(sorted(images(socket)))))
        else:
            value = socket.default_value
            signature.append((name, tuple(round(float(v), 4) for v in value) if hasattr(value, '__len__') else round(float(value), 4)))
    every = tuple(sorted(node.image.as_pointer() for node in tree.nodes if node.type == 'TEX_IMAGE' and node.image))
    return (tuple(signature), every) if every else None


def share_materials(objects):
    """Gun parts whose materials the conversion reads the same way (the same
    images on the same inputs, the same values) use one material, the one on
    the most faces. A gun baked to one texture set otherwise becomes a game
    material per part colour, each with its own copy of the textures the
    compiler combines (normal and roughness). Returns (kept, [folded names])
    per merged group."""
    materials, faces = {}, {}
    for obj in objects:
        counts = [0] * len(obj.material_slots)
        for polygon in obj.data.polygons:
            if polygon.material_index < len(counts):
                counts[polygon.material_index] += 1
        for slot, count in zip(obj.material_slots, counts):
            if slot.material:
                materials[slot.material.name] = slot.material
                faces[slot.material.name] = faces.get(slot.material.name, 0) + count
    groups = {}
    for name, material in sorted(materials.items()):
        signature = _material_signature(material)
        if signature is not None:
            groups.setdefault(signature, []).append(name)
    keep, merged = {}, []
    for names in groups.values():
        if len(names) < 2:
            continue
        kept = max(names, key=lambda n: (faces[n], n))
        merged.append((kept, [n for n in names if n != kept]))
        for name in names:
            keep[name] = materials[kept]
    for obj in objects:
        for slot in obj.material_slots:
            if slot.material and slot.material.name in keep:
                slot.material = keep[slot.material.name]
    return merged


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


def _weighted(obj, deform):
    """Deform bones the object is weighted to (the gun's own bones and charms)."""
    names = {g.index: g.name for g in obj.vertex_groups}
    return {names.get(g.group) for v in obj.data.vertices for g in v.groups if g.weight > 0 and names.get(g.group) in deform}


def candidates(meshes, rigs, height, weapon_model=False):
    """Rigid, elongated or weapon-named objects that could be a held weapon.

    Objects from the gun file are always one candidate. A weapon model's
    source may be a gun-only file: there the gun may hold most of the scene's
    vertices, and when nothing qualifies the whole scene (without ground or
    backdrop) is the gun.
    """
    rig = rigs[0] if rigs else None
    deform = {b.name for b in rig.data.bones if b.use_deform} if rig else set()
    receipt = {row['name']: row for row in (last_receipt() or {}).get('objects', [])}
    total_vertices = sum(len(m.data.vertices) for m in meshes) or 1
    rows = []
    for obj in meshes:
        count = len(obj.data.vertices)
        row = receipt.get(obj.name, {})
        from_file = bool(row.get('gun_file'))
        if not from_file and (count < 8 or (count > total_vertices * .6 and not weapon_model)):
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
        kind = 'loose'
        if row.get('bone'):
            bone, fraction = row['bone'], 1.
            kind = ('placed' if row.get('placed') else 'attached') if from_file else 'bone_parent' if 'parented' in row.get('reason', '') else 'attached'
        elif bone and fraction >= .9:
            kind = 'weights'
        rigid = fraction >= .9
        plausible = from_file or bool(words) or (rigid and elongation >= 1.8 and length < height * .9 and length > height * .03)
        if not plausible:
            continue
        rows.append({'object': obj.name, 'obj': obj, 'points': points, 'vertices': count, 'bone': bone if rigid else None,
                     'kind': kind if rigid or from_file else 'skinned', 'keywords': words, 'elongation': elongation,
                     'length': length, 'gun_file': from_file,
                     # The gun's own bones: those it is weighted to, and a gun-file
                     # charm's holder bone (not weighted) that the charm hangs from.
                     'bones': (_weighted(obj, deform) | set(row.get('gun_bones', []))) if rig else set(),
                     'side': row.get('side') if from_file else None})
    if weapon_model and not rows:
        rows = _whole_scene(meshes, rig, deform)
    # Multi-part guns: parts on the same bone whose bounds touch form one
    # weapon; the parts of each gun from the gun file (the left and the right
    # gun of a pair) form one weapon.
    groups = []
    for row in rows:
        box = np.array([row['points'].min(axis=0), row['points'].max(axis=0)])
        margin = max(row['length'] * .1, height * .005)
        if row['gun_file'] or row.get('whole'):
            key = ('file', row['side']) if row['gun_file'] else ('whole', None)
            group = next((g for g in groups if g['whole'] and g['key'] == key), None)
            if group is None:
                groups.append({'bone': row['bone'], 'box': box, 'rows': [row], 'whole': True, 'key': key})
            else:
                group['rows'].append(row)
                group['box'] = np.array([np.minimum(group['box'][0], box[0]), np.maximum(group['box'][1], box[1])])
            continue
        for group in groups:
            if group['whole']:
                continue
            if group['bone'] == row['bone'] and np.all(box[0] <= group['box'][1] + margin) and np.all(group['box'][0] <= box[1] + margin):
                group['rows'].append(row)
                group['box'] = np.array([np.minimum(group['box'][0], box[0]), np.maximum(group['box'][1], box[1])])
                break
        else:
            groups.append({'bone': row['bone'], 'box': box, 'rows': [row], 'whole': False, 'key': None})
    result = []
    for group in groups:
        points = np.concatenate([r['points'] for r in group['rows']])
        stride = max(1, len(points) // SAMPLE)
        names = sorted(r['object'] for r in group['rows'])
        _, values, _ = _pca(points)
        candidate = {
            'id': '+'.join(names), 'objects': names, 'vertices': int(sum(r['vertices'] for r in group['rows'])),
            'bounds': group['box'].tolist(), 'sample': points[::stride][:SAMPLE].tolist(),
            'attach_bone': group['bone'], 'attach_kind': group['rows'][0]['kind'],
            'keywords': sorted({w for r in group['rows'] for w in r['keywords']}),
            'elongation': float(np.sqrt(values[0] / max(values[1], 1e-12))),
            'gun_file': any(r['gun_file'] for r in group['rows']),
            'bones': sorted({b for r in group['rows'] for b in r.get('bones', ())}),
        }
        if group['rows'][0].get('side'):
            candidate['side'] = group['rows'][0]['side']
        hints = _hints(group['rows'])
        if hints:
            candidate['hints'] = hints
        shape = _shape(group['rows'])
        if shape:
            candidate['shape'] = shape
        result.append(candidate)
    return result


def _whole_scene(meshes, rig, deform):
    """A gun-only weapon source: every mesh except ground planes and backdrops."""
    from weapon_source import backdrops
    if not meshes:
        return []
    main = max(meshes, key=lambda m: len(m.data.vertices))
    flat = backdrops(meshes)
    rows = []
    for obj in meshes:
        points = world_points(obj)
        if not len(points) or (obj is not main and obj in flat):
            continue
        bone, fraction = _dominant(obj, deform) if rig else (None, 0.)
        materials = [s.material.name for s in obj.material_slots if s.material]
        rows.append({'object': obj.name, 'obj': obj, 'points': points, 'vertices': len(points),
                     'bone': bone if fraction >= .9 else None, 'kind': 'whole_file',
                     'keywords': weapon_words(obj.name, obj.data.name, *materials), 'elongation': 0.,
                     'length': float(np.ptp(points, axis=0).max()), 'gun_file': False, 'whole': True})
    # One weapon needs one bone (or none): mixed rigid bones are skinned.
    if len({r['bone'] for r in rows}) > 1:
        for row in rows:
            row['bone'] = None
    return rows


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


def hands(plan):
    """The plan of each hand: the right (or only) gun, then the left gun of a
    dual-wield hero."""
    if not plan:
        return []
    return [plan] + ([plan['left']] if plan.get('left') else [])


def split_hands(bpy, meshes, plan):
    """Separate each hand's gun from the body meshes.

    Returns (body, [(hand plan, objects), ...]). A hand that copies the right
    hand's gun (one gun in both hands) gets duplicates of those objects, made
    before anything is fitted. Gun-file guns that no hand uses are removed."""
    if not plan:
        return meshes, []
    planned = hands(plan)
    dropped = {name for objects in plan.get('drop', []) for name in objects}
    wanted = {name for hand in planned for name in hand.get('objects', [])} | dropped
    by_name = {m.name: m for m in meshes}
    missing = wanted - set(by_name)
    if missing:
        raise ValueError('The selected gun objects are no longer in the source: ' + ', '.join(sorted(missing)) +
                         '. Inspect the source again and choose the gun in the Weapon tab.')
    body = [m for m in meshes if m.name not in wanted]
    used = {name for hand in planned for name in hand.get('objects', [])}
    for name in sorted(dropped - used):
        bpy.data.objects.remove(by_name[name], do_unlink=True)
    result = []
    for hand in planned:
        objects = [by_name[name] for name in hand.get('objects', [])]
        if hand.get('copy'):
            copies = []
            for obj in objects:
                twin = obj.copy()
                twin.data = obj.data.copy()
                for collection in obj.users_collection:
                    collection.objects.link(twin)
                copies.append(twin)
            objects = copies
        result.append((hand, objects))
    return body, result


# Merged gun meshes stay far below the engine's per-mesh limit (524,288).
MERGE_CORNERS = 200_000


def merge_parts(bpy, objects, name):
    """Join a gun's parts into as few objects as possible (one for a normal
    gun) without losing anything the export uses.

    Bone weights (vertex groups) and colour attributes are merged by name,
    materials by slot and UV maps by position (each part's maps are renamed
    by index first). Each part's corner normals are stored before joining and
    restored from that attribute, so shading is unchanged. Parts with shape
    keys or without faces are left as they are. Returns the resulting objects."""
    joinable = [o for o in objects if o.type == 'MESH' and len(o.data.polygons) and not o.data.shape_keys]
    rest = [o for o in objects if o not in joinable]
    if len(joinable) < 2:
        return objects
    for obj in joinable:
        mesh = obj.data
        if mesh.users > 1:
            obj.data = mesh = mesh.copy()
        for index, layer in enumerate(mesh.uv_layers):
            layer.name = 'dmb_uv' + str(index)
        for index, layer in enumerate(mesh.color_attributes):
            layer.name = 'dmb_color' + str(index)
        # World-space corner normals, kept through the join.
        world = np.asarray(obj.matrix_world, dtype=float)
        to_world = np.linalg.inv(world[:3, :3]).T
        normals = mx.corner_normals(mesh) @ to_world.T
        lengths = np.linalg.norm(normals, axis=1)
        safe = lengths > 1e-12
        normals[safe] /= lengths[safe, None]
        normals[~safe] = (0., 0., 1.)
        values = normals.ravel().tolist()
        if 'dmb_normal' in mesh.attributes:
            mesh.attributes.remove(mesh.attributes['dmb_normal'])
        mesh.attributes.new('dmb_normal', 'FLOAT_VECTOR', 'CORNER').data.foreach_set('vector', values)
    # Chunks below the corner budget, in a stable order (largest part first).
    joinable.sort(key=lambda o: (-len(o.data.loops), o.name))
    chunks, current, count = [], [], 0
    for obj in joinable:
        loops = len(obj.data.loops)
        if current and count + loops > MERGE_CORNERS:
            chunks.append(current)
            current, count = [], 0
        current.append(obj)
        count += loops
    chunks.append(current)
    merged = []
    for index, chunk in enumerate(chunks):
        target = chunk[0]
        expected = (sum(len(o.data.vertices) for o in chunk), sum(len(o.data.polygons) for o in chunk),
                    sum(len(o.data.loops) for o in chunk))
        if len(chunk) > 1:
            view_layer = bpy.context.view_layer
            with bpy.context.temp_override(active_object=target, object=target, selected_objects=chunk,
                                           selected_editable_objects=chunk, view_layer=view_layer):
                bpy.ops.object.join()
        mesh = target.data
        if (len(mesh.vertices), len(mesh.polygons), len(mesh.loops)) != expected:
            raise ValueError('Merging the gun parts changed their geometry. Turn off "Merge the gun\'s parts" and convert again.')
        target.name = name if index == 0 else name + '_' + str(index + 1)
        merged.append(target)
    return merged + rest


def _stored_normals(mesh):
    """World-space corner normals saved by merge_parts, or None."""
    attribute = mesh.attributes.get('dmb_normal')
    if attribute is None or attribute.domain != 'CORNER' or len(attribute.data) != len(mesh.loops):
        return None
    values = np.empty(len(mesh.loops) * 3)
    attribute.data.foreach_get('vector', values)
    return values.reshape(-1, 3)


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


def _keep_charms(obj, planned, charms, deform):
    """Vertices weighted to charm bones (moving parts on the gun) keep that share
    of their weight on the charm's exported bones; the rest follows the plan."""
    names = {g.index: g.name for g in obj.vertex_groups}
    rows = []
    for vertex, row in zip(obj.data.vertices, planned):
        own = [(names.get(g.group), g.weight) for g in vertex.groups if g.weight > 0 and names.get(g.group) in deform]
        total = sum(w for _, w in own)
        kept = {}
        for name, weight in own:
            if name in charms:
                kept[charms[name]] = kept.get(charms[name], 0.) + weight / total
        share = min(sum(kept.values()), 1.)
        if share <= 1e-6:
            rows.append(row)
            continue
        rows.append([(name, weight * (1. - share)) for name, weight in row if weight * (1. - share) > 1e-6] + sorted(kept.items()))
    return rows


def apply(bpy, objects, plan, target_names, mat_paths, charms=None, deform=()):
    """Bake the fitted gun into target model space and assign its skin.
    charms: source bones of moving parts on the gun and their exported bones."""
    from mathutils import Matrix
    matrix = np.asarray(plan['matrix'], dtype=float)
    if matrix.shape != (4, 4) or not np.isfinite(matrix).all() or np.linalg.det(matrix[:3, :3]) <= 0:
        raise ValueError('The weapon fit is invalid. Reset the weapon adjustments and inspect again.')
    normal_matrix = np.linalg.inv(matrix[:3, :3]).T
    exported = []
    prefix = plan.get('mesh_prefix', 'weapon_')
    if plan.get('merge') and len(objects) > 1:
        objects = merge_parts(bpy, objects, prefix + 'gun')
    for obj in objects:
        mesh = obj.data
        if mesh.users > 1:
            obj.data = mesh = mesh.copy()
        triangulate_ngons(obj)
        world = np.asarray(obj.matrix_world, dtype=float)
        points = world_points(obj)
        fitted = points @ matrix[:3, :3].T + matrix[:3, 3]
        weights = _weights(plan, fitted)
        if charms:
            weights = _keep_charms(obj, weights, charms, deform)
        for row in weights:
            if not row or any(name not in target_names for name, _ in row):
                raise ValueError('The weapon skin references a bone that is not in the target skeleton.')
        stored = _stored_normals(mesh)
        source_normals = stored if stored is not None else mx.corner_normals(mesh) @ np.linalg.inv(world[:3, :3])
        normals = mx.transformed_normals(source_normals, normal_matrix, 'Invalid weapon normal on ' + obj.name, 1e-12)
        if stored is not None:
            mesh.attributes.remove(mesh.attributes['dmb_normal'])
        uvs = mx.uvs(mesh)
        colors = mx.colors(mesh)
        faces = mx.faces(mesh, mx.material_paths(obj, mat_paths))
        mx.set_positions(mesh, fitted)
        obj.parent = None
        obj.matrix_parent_inverse = Matrix.Identity(4)
        obj.matrix_world = Matrix.Identity(4)
        obj.modifiers.clear()
        mx.assign_weights(obj, target_names, weights)
        if hasattr(mesh, 'normals_split_custom_set'):
            mesh.normals_split_custom_set(normals)
        tangents = mx.tangents(mesh)
        name = obj.name if obj.name.startswith(prefix) else re.sub(r'[^A-Za-z0-9_.-]', '_', prefix + obj.name)[:60]
        obj.name = name
        exported.append({'name': obj.name, 'positions': fitted.tolist(), 'normals': normals, 'uvs': uvs,
                         'colors': colors, 'weights': weights, 'faces': faces, 'deformation': [],
                         'tangents': tangents, 'weapon': {'policy': POLICY, 'source_object': plan['objects']}})
    return exported
