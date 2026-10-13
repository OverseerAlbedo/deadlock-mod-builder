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
* a file with a pair of guns (parts tagged [L]/[R], .L/.R or named left and
  right, or weighted to such bones) gives a left and a right gun;
* with a character rig, the gun is put in the right hand (the left gun of a
  pair in the left hand): its grip (the handle below the barrel, measured like
  weapon.rs does) is moved onto the hand unless the gun already sits there,
  and it is rigidly weighted to the hand, so "Keep it in the hand" works too.
  Without a hand it keeps its position.

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


def side_of(name):
    """Which hand a name belongs to: 'left', 'right' or None. Tags such as
    [L], (R), .L or _R decide first, then words (left, right, l, r, 左, 右).
    Keep in sync with weapon.rs side_of."""
    lower = name.lower()

    def tagged(tag):
        return ('[' + tag + ']' in lower or '(' + tag + ')' in lower
                or lower.endswith('.' + tag) or lower.endswith('_' + tag))
    left, right = tagged('l'), tagged('r')
    if left != right:
        return 'left' if left else 'right'
    parts, word, previous_lower = [], '', False
    for c in name:
        if not c.isalnum():
            if word:
                parts.append(word.lower())
                word = ''
            previous_lower = False
            continue
        if c.isupper() and previous_lower and word:
            parts.append(word.lower())
            word = ''
        previous_lower = c.islower()
        word += c
    if word:
        parts.append(word.lower())
    left = any(p == 'l' or p.startswith('left') for p in parts) or '左' in lower
    right = any(p == 'r' or p.startswith('right') for p in parts) or '右' in lower
    if left != right:
        return 'left' if left else 'right'
    return None


def _dominant_group(obj):
    """The vertex group carrying most of an object's weight (its own rig bone)."""
    names = {g.index: g.name for g in obj.vertex_groups}
    totals = {}
    for vertex in obj.data.vertices:
        for group in vertex.groups:
            if group.weight > 0 and group.group in names:
                totals[names[group.group]] = totals.get(names[group.group], 0.) + group.weight
    return max(totals, key=totals.get) if totals else None


def pair(gun, sides, keep):
    """One gun, or a right and a left gun when the parts name their hands.

    Returns [[side, parts], ...]: [[None, parts]] for one gun, otherwise the
    right gun then the left gun. Each gun is its largest part and what touches
    it (see connected); parts without a hand join the gun they touch."""
    size = lambda parts: sum(len(m.data.vertices) for m in parts)
    left = [m for m in gun if sides.get(m) == 'left']
    right = [m for m in gun if sides.get(m) == 'right']
    if not left or not right or min(size(left), size(right)) < .2 * max(size(left), size(right)):
        kept, _ = connected(gun, keep)
        return [[None, kept]]
    guns = [['right', connected(right, keep)[0]], ['left', connected(left, keep)[0]]]
    boxes = []
    for _, parts in guns:
        points = np.concatenate([iso.world_points(m) for m in parts])
        box = iso.bounds(points)
        boxes.append((box, float((box[1] - box[0]).max()) * .1, points.mean(axis=0)))
    for mesh in gun:
        if sides.get(mesh) is not None:
            continue
        points = iso.world_points(mesh)
        box = iso.bounds(points)
        touching = [i for i, (other, margin, _) in enumerate(boxes) if iso._overlaps(box, other, margin)]
        if touching:
            nearest = min(touching, key=lambda i: float(np.linalg.norm(points.mean(axis=0) - boxes[i][2])))
            guns[nearest][1].append(mesh)
    return guns


def _place(rig, hand, parts, meshes):
    """Put a gun's grip in a hand (when the hand exists and the gun is not
    already there) and choose the bone that holds it.
    Returns (bone, placed, offset): offset is the world move applied."""
    from mathutils import Matrix
    points = np.concatenate([iso.world_points(m) for m in parts])
    if rig is not None and hand and hand in rig.data.bones:
        data = rig.data.bones[hand]
        matrix = np.asarray(rig.matrix_world, dtype=float)
        palm = matrix[:3, :3] @ ((np.asarray(data.head_local) + np.asarray(data.tail_local)) / 2) + matrix[:3, 3]
        span = float(np.linalg.norm(points.max(axis=0) - points.min(axis=0)))
        if float(np.min(np.linalg.norm(points - palm, axis=1))) > span * .25:
            move = palm - grip_point(points)
            offset = Matrix.Translation(tuple(move))
            for obj in parts:
                obj.matrix_world = offset @ obj.matrix_world
            return hand, True, move
        return hand, False, np.zeros(3)
    if rig is not None:
        deform = {b.name for b in rig.data.bones if b.use_deform}
        used = set()
        for obj in meshes:
            names = {g.index: g.name for g in obj.vertex_groups}
            used.update(names[g.group] for v in obj.data.vertices for g in v.groups if g.weight > 0 and names.get(g.group) in deform)
        return iso.nearest_bone(rig, points, used or deform), False, np.zeros(3)
    return None, False, np.zeros(3)


CHARM_WORDS = ('strap', 'tassel', 'ribbon', 'streamer')


def _charm_chains(rig):
    """Charms in the gun file's rig: a bone named as a charm (keychain,
    pendant ...) or a strap/tassel/ribbon whose parent is not one, with all
    bones below it (at most eight)."""
    from secondary_bones import charm_named

    def moving(name):
        return charm_named(name) or any(word in name.lower() for word in CHARM_WORDS)
    chains = []
    for bone in rig.data.bones:
        if moving(bone.name) and not (bone.parent and moving(bone.parent.name)):
            members = [bone] + list(bone.children_recursive)
            if len(members) <= 8:
                chains.append(members)
    return chains


def _rest(rig, bone):
    """World head, tail and z axis of a bone at rest."""
    world = rig.matrix_world
    z = (world.to_3x3() @ bone.matrix_local.to_3x3()).col[2]
    return np.array(world @ bone.head_local), np.array(world @ bone.tail_local), np.array(z)


def capture_charms(found, gun_rigs):
    """Before the gun parts are frozen: the gun file's charm chains (bone rest
    data, the holder bone above each chain) and each part's charm weights.
    Returns (chains, weights)."""
    chains = []
    for gun_rig in gun_rigs:
        for members in _charm_chains(gun_rig):
            holder = members[0].parent
            chains.append({
                'holder': holder.name if holder else None,
                'holder_rest': _rest(gun_rig, holder) if holder else None,
                'bones': [(b.name, b.parent.name if b.parent else None, _rest(gun_rig, b)) for b in members],
            })
    names = {name for chain in chains for name, _, _ in chain['bones']}
    weights = {}
    if names:
        for obj in found:
            groups = {g.index: g.name for g in obj.vertex_groups}
            rows = []
            for vertex in obj.data.vertices:
                total = sum(g.weight for g in vertex.groups if g.weight > 0)
                charm = {groups[g.group]: g.weight for g in vertex.groups if g.weight > 0 and groups.get(g.group) in names}
                if charm and total > 0:
                    rows.append((vertex.index, charm, total))
            if rows:
                weights[obj] = (len(obj.data.vertices), rows)
    return chains, weights


def _charm_rows(obj, weights):
    """Captured charm weights of a part, unless freezing changed its vertices."""
    count, rows = weights.get(obj, (None, ()))
    return rows if count == len(obj.data.vertices) else ()


def keep_charms(bpy, rig, guns, chains, weights, receipt):
    """Give each gun's charm its own bones in the character rig so it can
    swing: the charm chain under a holder bone (the gun's own bone above the
    charm, not weighted) that hangs from the hand holding the gun, moved like
    the gun. The charm's parts keep their charm weights; the rest of their
    weight stays on the hand. The holder is a gun bone (gun_bones), so the
    charm is recognised as hanging from this gun. Returns {side: [bones]}."""
    from mathutils import Vector
    if rig is None or not chains:
        return {}
    plans = []
    for side, parts, hand, move in guns:
        if not hand or hand not in rig.data.bones:
            continue
        for chain in chains:
            names = {name for name, _, _ in chain['bones']}
            if not any(name in names for obj in parts for _, charm, _ in _charm_rows(obj, weights) for name in charm):
                continue
            plans.append((side, parts, hand, move, chain))
    if not plans:
        return {}
    added = {}
    inverse = rig.matrix_world.inverted()
    rotation = inverse.to_3x3()
    view_layer = bpy.context.view_layer
    view_layer.objects.active = rig
    for obj in view_layer.objects:
        obj.select_set(obj == rig)
    bpy.ops.object.mode_set(mode='EDIT')
    try:
        edit = rig.data.edit_bones
        for side, parts, hand, move, chain in plans:
            def bone(name, parent, rest, deform):
                head, tail, z = rest
                created = edit.new(PREFIX + name)
                created.head = inverse @ Vector((head + move).tolist())
                created.tail = inverse @ Vector((tail + move).tolist())
                if (created.tail - created.head).length < 1e-6:
                    created.tail = created.head + Vector((0., 0., 1e-3))
                created.align_roll(rotation @ Vector(z.tolist()))
                created.parent = edit[parent]
                created.use_connect = False
                created.use_deform = deform
                return created.name
            if chain['holder']:
                holder = bone(chain['holder'], hand, chain['holder_rest'], False)
            else:
                head = chain['bones'][0][2][0]
                holder = bone(chain['bones'][0][0] + '_holder', hand, (head, head + np.array([0., 0., 1e-2]), np.array([0., 1., 0.])), False)
            renamed = {}
            for name, parent, rest in chain['bones']:
                renamed[name] = bone(name, renamed.get(parent, holder), rest, True)
            chain['created'] = (holder, renamed)
            added.setdefault(side, []).extend(renamed.values())
    finally:
        bpy.ops.object.mode_set(mode='OBJECT')
    # The charm's vertices keep their charm weights; the rest stays on the hand.
    for side, parts, hand, move, chain in plans:
        holder, renamed = chain['created']
        for obj in parts:
            hand_group = obj.vertex_groups.get(hand)
            for index, charm, total in _charm_rows(obj, weights):
                shares = {renamed[name]: weight / total for name, weight in charm.items() if name in renamed}
                if not shares:
                    continue
                share = min(1., sum(shares.values()))
                if hand_group is not None:
                    if share >= 1. - 1e-6:
                        hand_group.remove([index])
                    else:
                        hand_group.add([index], 1. - share, 'REPLACE')
                for name, weight in shares.items():
                    group = obj.vertex_groups.get(name) or obj.vertex_groups.new(name=name)
                    group.add([index], weight, 'REPLACE')
            row = receipt.rows.get(obj.name)
            if row is not None and holder not in row.setdefault('gun_bones', []):
                row['gun_bones'].append(holder)
        which = {'right': 'The right gun\'s', 'left': 'The left gun\'s'}.get(side, 'The gun\'s')
        receipt.note(which + ' charm keeps its bones (' + ', '.join(renamed.values()) + ') so it can swing.')
    return added


def backdrop(box, size):
    """Ground planes and backdrops around a gun: flat and at least gun-sized,
    or far larger than the gun. Long thin parts (barrels) are not flat."""
    extent = np.sort(box[1] - box[0])
    flat = extent[0] < extent[2] * .03 and extent[1] > extent[2] * .3
    return bool((flat and extent[2] > size * .9) or max(box[1][0] - box[0][0], box[1][1] - box[0][1]) > size * 2.5)


def backdrops(meshes, keep=()):
    """The parts that are ground planes or backdrops: each part is compared with
    everything else in the file (not with one detailed part, which can be tiny),
    so a flat grip panel or decal is never taken for a floor."""
    boxes = [(mesh, iso.bounds(iso.world_points(mesh))) for mesh in meshes]
    boxes = [(mesh, box) for mesh, box in boxes if box is not None]
    if len(boxes) < 2:
        return set()
    lows = np.array([box[0] for _, box in boxes])
    highs = np.array([box[1] for _, box in boxes])
    found = set()
    for index, (mesh, box) in enumerate(boxes):
        if mesh.name in keep:
            continue
        others = np.delete(highs, index, axis=0).max(axis=0) - np.delete(lows, index, axis=0).min(axis=0)
        if backdrop(box, max(float(others.max()), 1e-6)):
            found.add(mesh)
    return found


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
    source = request['source']
    label = Path(source).name
    imported, notes = _load(bpy, source)
    realized = iso.realize_instances(bpy, imported)
    imported = list(imported) + realized
    # Names before the prefix: Blender cuts long names at 63 bytes, which could
    # cut off a trailing [L]/[R] hand tag.
    original = {obj.as_pointer(): obj.name for obj in imported}
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
    # The hand each part belongs to, from its name or the gun rig bone it is
    # weighted to (read before the part is frozen and loses its weights).
    sides = {}
    for obj in found:
        side = side_of(original.get(obj.as_pointer(), obj.name))
        if side is None:
            group = _dominant_group(obj)
            side = side_of(group) if group else None
        sides[obj] = side
    # Charms on the gun keep their bones: read them before the parts are frozen.
    chains, charm_weights = capture_charms(found, gun_rigs)
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
    flat = backdrops(found, choices['include'])
    scenery = [m for m in found if m is not main and m in flat]
    for mesh in scenery:
        receipt.mark(mesh, 'excluded', 'ground or backdrop in the gun file')
    gun = [m for m in found if m not in scenery]
    guns = pair(gun, sides, choices['include'])
    kept = [m for _, parts in guns for m in parts]
    for mesh in gun:
        if mesh not in kept:
            receipt.mark(mesh, 'excluded', 'away from the gun in the gun file (keep it to use it)')
    if sum(len(o.data.vertices) for o in meshes + kept) > 5_000_000:
        raise ValueError('The character and the gun file together exceed the five million preparation vertex limit.')
    # Put each gun in its hand when the character has one: the right hand (or
    # the only gun), and the left hand for the left gun of a pair.
    rig = rigs[0] if rigs else None
    placements = []
    held = []
    for side, parts in guns:
        hand = request.get('left_hand') if side == 'left' else request.get('hand')
        hand_name = 'left hand' if side == 'left' else 'right hand'
        bone, placed, move = _place(rig, hand, parts, meshes)
        held.append((side, parts, bone, move))
        placements.append({'side': side, 'objects': sorted(o.name for o in parts), 'bone': bone,
                           'hand': hand if rig is not None and hand and hand in rig.data.bones else None, 'placed': placed})
        which = ('left gun ' if side == 'left' else 'right gun ' if side == 'right' else '')
        for obj in parts:
            if bone:
                iso._rigid_bind(obj, rig, bone)
            where = (which + 'moved into the ' + hand_name + ' (' + bone + ') by its grip' if placed else
                     which + 'already in the hand; attached to ' + bone if bone == hand and bone else
                     which + 'kept where it is and attached to ' + bone + ' (no ' + hand_name + ' was found)' if bone else
                     which + 'kept where it is (the character has no rig)')
            receipt.mark(obj, 'bound' if bone else 'included', 'from the gun file ' + label + '; ' + where,
                         **({'bone': bone} if bone else {}), gun_file=True, placed=placed, **({'side': side} if side else {}))
    # A charm on the gun keeps its bones (under the hand that holds the gun).
    charm_bones = keep_charms(bpy, rig, held, chains, charm_weights, receipt)
    for placement in placements:
        if charm_bones.get(placement['side']):
            placement['charm_bones'] = charm_bones[placement['side']]
    # Set-aside gun-file objects never reach the prepared scene.
    keep = {o.as_pointer() for o in kept}
    leftovers = {o.as_pointer(): o for o in imported + found if iso.alive(o) and o.as_pointer() not in keep}
    for obj in leftovers.values():
        if obj.name in bpy.context.scene.objects:
            bpy.data.objects.remove(obj, do_unlink=True)
    iso._LAST['object'] = receipt
    iso._LAST['receipt'] = receipt.as_dict(character)
    first = placements[0]
    summary = {'policy': POLICY, 'file': label, 'objects': sorted(o.name for o in kept), 'bone': first['bone'],
               'hand': first['hand'], 'placed': any(p['placed'] for p in placements)}
    if len(placements) > 1:
        summary['guns'] = placements
    if charm_bones:
        summary['charm_bones'] = [name for bones in charm_bones.values() for name in bones]
    return meshes + kept, summary
