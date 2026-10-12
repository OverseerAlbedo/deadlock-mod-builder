"""Take the gun from its own file (the Weapon tab's gun file).

At inspection and conversion the worker appends the gun file's meshes to the
temporary character scene, after the character itself was isolated:

* cameras, lights, empties, hidden objects, the gun's own rig and ground
  planes or backdrops are set aside; parts away from the gun are set aside
  unless you keep them in Scene objects;
* shape keys, modifiers, constraints and mirrored transforms are baked, so the
  gun is used at its saved rest shape;
* every object is renamed ``gun_file_<name>`` so it is offered as the gun and
  never collides with a character object;
* with a character rig, the gun is put in the right hand: its grip (the handle
  below the barrel, measured like weapon.rs does) is moved onto the hand
  unless the gun already sits there, and it is rigidly weighted to the hand,
  so "Keep it in the hand" works too. Without a hand it keeps its position.

Each object gets a row in the scene-object receipt. The user's gun file is
never written; only the staged workspace copy is read.
"""
from pathlib import Path
import numpy as np
import scene_isolation as iso

POLICY = 'gun-file-v1'
PREFIX = 'gun_file_'


def _load(bpy, path):
    """Import the gun file into the current scene; return its new objects."""
    prior = set(bpy.context.scene.objects)
    suffix = Path(path).suffix.lower()
    notes = []
    if suffix in {'.glb', '.gltf'}:
        properties = bpy.ops.import_scene.gltf.get_rna_type().properties
        optional = {'disable_bone_shape': True} if 'disable_bone_shape' in properties else {}
        bpy.ops.import_scene.gltf(filepath=path, merge_vertices=False, **optional)
    elif suffix == '.fbx':
        bpy.ops.import_scene.fbx(filepath=path, use_anim=False, automatic_bone_orientation=False)
    elif suffix == '.blend':
        # Append the scenes as local data (no scripts, no UI) and keep the one
        # holding the most visible geometry.
        with bpy.data.libraries.load(path, link=False) as (available, loaded):
            loaded.scenes = list(available.scenes)
        scenes = [scene for scene in loaded.scenes if scene is not None]
        if not scenes:
            raise ValueError('The gun file has no scene.')

        def visible_vertices(scene):
            return sum(len(o.data.vertices) for o in scene.objects if o.type == 'MESH' and not o.hide_render)
        chosen = max(scenes, key=visible_vertices)
        if len(scenes) > 1:
            notes.append('The gun file has ' + str(len(scenes)) + ' scenes; the gun was taken from ' + chosen.name + '.')
        for obj in list(chosen.objects):
            bpy.context.scene.collection.objects.link(obj)
        for scene in scenes:
            bpy.data.scenes.remove(scene)
        # Blender 5.2 defers evaluation of appended object transforms.
        bpy.context.view_layer.update()
    else:
        raise ValueError('Unsupported gun file. Choose a Blender, GLB, glTF or FBX file.')
    if bpy.context.object and bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')
    return [o for o in bpy.context.scene.objects if o not in prior], notes


def grip_point(points):
    """Where a hand holds the gun: the handle hanging below the barrel.

    The same rules as weapon.rs `measure` without hints: the thinner end is the
    muzzle, the handle is the part hanging farthest below the barrel line, and
    a gun without a handle is held at its rear third.
    """
    points = np.asarray(points, dtype=float)
    centre = points.mean(axis=0)
    covariance = np.cov((points - centre).T) if len(points) > 3 else np.eye(3)
    values, vectors = np.linalg.eigh(covariance)
    forward = vectors[:, int(np.argmax(values))]
    along = (points - centre) @ forward
    low, high = float(along.min()), float(along.max())
    length = high - low
    if length <= 1e-9:
        return centre

    def radius(end):
        slab = points[np.abs(along - end) < length * .12]
        if not len(slab):
            return np.inf
        offsets = slab - slab.mean(axis=0)
        offsets -= np.outer(offsets @ forward, forward)
        return float(np.linalg.norm(offsets, axis=1).mean())
    if radius(low) < radius(high):
        forward, along, low, high = -forward, -along, -high, -low
    barrel = points[along >= high - length * .35].mean(axis=0)
    offsets = points - barrel
    offsets -= np.outer(offsets @ forward, forward)
    mean = offsets.mean(axis=0)
    rms = float(np.sqrt((offsets ** 2).sum(axis=1).mean()))
    if np.linalg.norm(mean) > rms * .15:
        drop = offsets @ (mean / np.linalg.norm(mean))
        deepest = float(drop.max())
        if deepest > length * .08:
            return points[drop >= deepest * .5].mean(axis=0)
    return points[along <= low + length * .35].mean(axis=0)


def backdrop(box, size):
    """Ground planes and backdrops around a gun: flat and at least gun-sized,
    or far larger than the gun. Long thin parts (barrels) are not flat."""
    extent = np.sort(box[1] - box[0])
    flat = extent[0] < extent[2] * .03 and extent[1] > extent[2] * .3
    return bool((flat and extent[2] > size * .9) or max(box[1][0] - box[0][0], box[1][1] - box[0][1]) > size * 2.5)


def _bake_shape_keys(obj, receipt):
    """Keep the saved shape-key mix as the gun's rest shape."""
    keys = obj.data.shape_keys
    if not keys or len(keys.key_blocks) < 2:
        if keys:
            obj.shape_key_clear()
        return
    mix = obj.shape_key_add(name='dmb_gun_rest', from_mix=True)
    coordinates = [point.co.copy() for point in mix.data]
    obj.shape_key_clear()
    for vertex, point in zip(obj.data.vertices, coordinates):
        vertex.co = point
    receipt.note('The shape keys of ' + obj.name + ' were baked into its saved shape.')


def connected(meshes, keep):
    """The largest part and everything near it (chained); kept-by-you parts too.

    The largest part (by extent, not vertex count: a dense strap or lens is not
    the gun) starts the chain; a part joins when it comes within a quarter of
    that part's length of a joined part."""
    boxes = {m: iso.bounds(iso.world_points(m)) for m in meshes}
    main = max(meshes, key=lambda m: float(np.linalg.norm(boxes[m][1] - boxes[m][0])))
    margin = float((boxes[main][1] - boxes[main][0]).max()) * .25
    joined = {main}
    changed = True
    while changed:
        changed = False
        for mesh in meshes:
            if mesh not in joined and any(iso._overlaps(boxes[mesh], boxes[other], margin) for other in joined):
                joined.add(mesh)
                changed = True
    return [m for m in meshes if m in joined or m.name in keep], main


def append(bpy, request, meshes, rigs, scene_choices=None):
    """Append the gun file's meshes; return (meshes, receipt summary)."""
    from mathutils import Matrix
    source = request['source']
    label = Path(source).name
    imported, notes = _load(bpy, source)
    realized = iso.realize_instances(bpy, imported)
    imported = list(imported) + realized
    for obj in imported:
        obj.name = PREFIX + obj.name
    choices = iso._choices(scene_choices)
    receipt = iso._LAST.get('object') or iso.Receipt()
    character = (iso._LAST.get('receipt') or {}).get('character_rig')
    for note in notes:
        receipt.note(note)
    view_layer = bpy.context.view_layer
    collections = iso._renderable_collections(view_layer)
    gun_rigs = [o for o in imported if o.type == 'ARMATURE']
    widgets = {b.custom_shape for rig in gun_rigs for b in rig.pose.bones if b.custom_shape}
    found, hidden = [], []
    for obj in imported:
        if obj.type == 'ARMATURE':
            receipt.mark(obj, 'excluded', 'rig of the gun file ' + label + ' (the gun is used at its rest shape)')
        elif obj in widgets:
            receipt.mark(obj, 'excluded', 'bone display shape in the gun file')
        elif obj.type in iso.IGNORED:
            receipt.mark(obj, 'ignored', obj.type.lower().replace('_', ' ') + ' in the gun file (not gun geometry)')
        elif obj.name in choices['exclude']:
            receipt.mark(obj, 'excluded', 'from the gun file ' + label + '; excluded by you')
        elif obj.type not in {'MESH'} | iso.CONVERTIBLE:
            receipt.mark(obj, 'excluded', obj.type.lower().replace('_', ' ') + ' objects cannot be exported as gun geometry')
        elif not iso.renderable(obj, view_layer, collections) and obj.name not in choices['include']:
            hidden.append(obj)
            receipt.mark(obj, 'excluded', 'hidden from rendering in the gun file (include it to keep it)')
        elif obj.type == 'MESH':
            found.append(obj)
        else:
            converted = iso._convert_to_mesh(bpy, obj)
            if converted is None:
                receipt.mark(obj, 'excluded', obj.type.lower() + ' object in the gun file without faces')
            else:
                receipt.mark(obj, 'converted', obj.type.lower() + ' in the gun file converted to a mesh at its current shape')
                found.append(converted)
    if not found:
        found = [o for o in hidden if o.type == 'MESH']
    for obj in list(found):
        if not len(obj.data.polygons):
            receipt.mark(obj, 'excluded', 'no faces (edges or points only)')
            found.remove(obj)
    if not found:
        raise ValueError('The gun file ' + label + ' has no visible mesh. Check that the gun is not hidden from rendering.')
    # Freeze each part at its rest shape: no own rig, parent, shape keys,
    # constraints or generative modifiers.
    for obj in found:
        if obj.data.users > 1:
            obj.data = obj.data.copy()
        iso.unbind_static(bpy, obj)
        _bake_shape_keys(obj, receipt)
        iso._normalize(bpy, obj, None, receipt, allow_shape_keys=False)
    for rig in gun_rigs:
        if iso.alive(rig):
            bpy.data.objects.remove(rig, do_unlink=True)
    main = max(found, key=lambda m: len(m.data.vertices))
    main_box = iso.bounds(iso.world_points(main))
    size = max(float((main_box[1] - main_box[0]).max()), 1e-6)
    scenery = [m for m in found if m is not main and m.name not in choices['include'] and backdrop(iso.bounds(iso.world_points(m)), size)]
    for mesh in scenery:
        receipt.mark(mesh, 'excluded', 'ground or backdrop in the gun file')
    gun = [m for m in found if m not in scenery]
    kept, _ = connected(gun, choices['include'])
    for mesh in gun:
        if mesh not in kept:
            receipt.mark(mesh, 'excluded', 'away from the gun in the gun file (keep it to use it)')
    if sum(len(o.data.vertices) for o in meshes + kept) > 5_000_000:
        raise ValueError('The character and the gun file together exceed the five million preparation vertex limit.')
    # Put the gun in the right hand when the character has one.
    rig = rigs[0] if rigs else None
    hand = request.get('hand')
    bone, placed = None, False
    points = np.concatenate([iso.world_points(m) for m in kept])
    if rig is not None and hand and hand in rig.data.bones:
        data = rig.data.bones[hand]
        matrix = np.asarray(rig.matrix_world, dtype=float)
        palm = matrix[:3, :3] @ ((np.asarray(data.head_local) + np.asarray(data.tail_local)) / 2) + matrix[:3, 3]
        span = float(np.linalg.norm(points.max(axis=0) - points.min(axis=0)))
        if float(np.min(np.linalg.norm(points - palm, axis=1))) > span * .25:
            offset = Matrix.Translation(tuple(palm - grip_point(points)))
            for obj in kept:
                obj.matrix_world = offset @ obj.matrix_world
            placed = True
        bone = hand
    elif rig is not None:
        deform = {b.name for b in rig.data.bones if b.use_deform}
        used = set()
        for obj in meshes:
            names = {g.index: g.name for g in obj.vertex_groups}
            used.update(names[g.group] for v in obj.data.vertices for g in v.groups if g.weight > 0 and names.get(g.group) in deform)
        bone = iso.nearest_bone(rig, points, used or deform)
    for obj in kept:
        if bone:
            iso._rigid_bind(obj, rig, bone)
        where = ('moved into the right hand (' + bone + ') by its grip' if placed else
                 'already in the hand; attached to ' + bone if bone == hand and bone else
                 'kept where it is and attached to ' + bone + ' (no right hand was found)' if bone else
                 'kept where it is (the character has no rig)')
        receipt.mark(obj, 'bound' if bone else 'included', 'from the gun file ' + label + '; ' + where,
                     **({'bone': bone} if bone else {}), gun_file=True, placed=placed)
    # Set-aside gun-file objects never reach the prepared scene.
    keep = {o.as_pointer() for o in kept}
    leftovers = {o.as_pointer(): o for o in imported + found if iso.alive(o) and o.as_pointer() not in keep}
    for obj in leftovers.values():
        if obj.name in bpy.context.scene.objects:
            bpy.data.objects.remove(obj, do_unlink=True)
    iso._LAST['object'] = receipt
    iso._LAST['receipt'] = receipt.as_dict(character)
    summary = {'policy': POLICY, 'file': label, 'objects': sorted(o.name for o in kept), 'bone': bone,
               'hand': hand if rig is not None and hand and hand in rig.data.bones else None, 'placed': placed}
    return meshes + kept, summary
