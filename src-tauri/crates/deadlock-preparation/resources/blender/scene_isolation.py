"""Find the character inside a busy source scene and repair what can be repaired.

Runs only on the temporary scene Blender imported for this job; the user's
source file is never saved or changed. Instead of stopping on the first
unexpected object, every object receives a status and a reason:

* cameras, lights, empties, curves, text, grease pencil and other non-mesh
  objects are set aside (renderable curves bound to the character are
  converted to meshes);
* objects hidden from rendering, hidden alternates and objects in excluded
  collections are set aside unless the user includes them;
* when several characters are present, the most prominent one is kept and the
  others are set aside (the user can pick another character rig);
* loose props near the character are bound to it, far-away scenery is set
  aside; bone-parented or constrained props become rigidly weighted;
* generative modifiers are applied, simulations removed, instanced mesh data
  made single-user, mirrored transforms baked, stray attributes dropped,
  missing materials filled, and partly weighted vertices completed from their
  nearest weighted neighbour.

The receipt lists everything so the UI can show it and let the user override.
"""
import json
import re
import numpy as np

POLICY = 'character-scene-isolation-v1'
IGNORED = {'CAMERA', 'LIGHT', 'EMPTY', 'LIGHT_PROBE', 'SPEAKER', 'LATTICE', 'IMAGE'}
CONVERTIBLE = {'CURVE', 'SURFACE', 'FONT', 'META'}
SIMULATION_MODIFIERS = {'CLOTH', 'SOFT_BODY', 'COLLISION', 'DYNAMIC_PAINT', 'FLUID', 'PARTICLE_SYSTEM',
                        'PARTICLE_INSTANCE', 'OCEAN', 'EXPLODE', 'SURFACE'}
BASE_ATTRIBUTES = {'position', '.corner_vert', '.edge_verts', '.corner_edge', 'material_index', 'sharp_face',
                   'sharp_edge', 'normal', 'uv_seam'}
WEAPON_WORDS = ('gun', 'pistol', 'rifle', 'weapon', 'wpn', 'revolver', 'shotgun', 'smg', 'uzi', 'blaster',
                'launcher', 'cannon', 'magnum', 'sniper', 'carbine', 'firearm', 'glock', 'colt', 'deagle',
                'minigun', 'crossbow', 'musket', 'handgun', 'bazooka', 'grenade', 'katana', 'sword', 'blade',
                'knife', 'axe', 'hammer', 'spear', 'staff', 'scythe', 'bow', '銃', '武器', '剣', '刀', '枪', '槍')

_LAST = {}


def last_receipt():
    """Receipt of the most recent isolation in this Blender process."""
    return _LAST.get('receipt')


def weapon_words(*names):
    found = []
    for name in names:
        text = (name or '').lower()
        tokens = set(re.split(r'[^a-z0-9]+', text))
        for word in WEAPON_WORDS:
            if word.isascii():
                # Short words must be whole tokens; long ones may be embedded (MyPistol01).
                if word in tokens or (len(word) >= 5 and word in text) or any(t.startswith(word) and len(word) >= 3 and t[len(word):].isdigit() for t in tokens):
                    found.append(word)
            elif word in text:
                found.append(word)
    return sorted(set(found))


class Receipt:
    def __init__(self):
        self.rows = {}
        self.notes = []

    def mark(self, obj, status, reason, **extra):
        row = {'name': obj.name, 'type': obj.type, 'status': status, 'reason': reason}
        row.update(extra)
        self.rows[obj.name] = row

    def mark_name(self, name, kind, status, reason):
        """Row for an object an earlier step already removed from the scene."""
        self.rows[name] = {'name': name, 'type': kind, 'status': status, 'reason': reason}

    def note(self, text):
        if text not in self.notes:
            self.notes.append(text)

    def as_dict(self, character=None):
        rows = sorted(self.rows.values(), key=lambda r: ({'character': 0, 'included': 1, 'repaired': 2, 'bound': 3,
                                                          'converted': 4, 'excluded': 5, 'ignored': 6}.get(r['status'], 7), r['name']))
        excluded = [r['name'] for r in rows if r['status'] == 'excluded']
        ignored = [r for r in rows if r['status'] == 'ignored']
        summary = []
        if character:
            summary.append('Character rig: ' + character + '.')
        if excluded:
            summary.append('Set aside ' + str(len(excluded)) + ' object' + ('s' if len(excluded) != 1 else '') +
                           ' that are not part of the character: ' + ', '.join(excluded[:8]) +
                           (' …' if len(excluded) > 8 else '') + '.')
        if ignored:
            summary.append('Ignored ' + str(len(ignored)) + ' camera/light/empty objects.')
        summary.extend(self.notes)
        return {'policy': POLICY, 'character_rig': character, 'objects': rows,
                'notes': list(self.notes), 'warning': ' '.join(summary) if (excluded or self.notes) else ''}


def _choices(value):
    value = value if isinstance(value, dict) else {}
    def names(key):
        items = value.get(key) or []
        return {str(v) for v in items if isinstance(v, str)} if isinstance(items, list) else set()
    character = value.get('character') if isinstance(value.get('character'), str) else ''
    return {'exclude': names('exclude'), 'include': names('include'), 'character': character}


def _renderable_collections(view_layer):
    result = {}

    def walk(layer, visible):
        collection = layer.collection
        visible = visible and not layer.exclude and not collection.hide_render
        result[collection.name] = result.get(collection.name, False) or visible
        for child in layer.children:
            walk(child, visible)
    walk(view_layer.layer_collection, True)
    return result


def renderable(obj, view_layer, collections):
    if obj.hide_render:
        return False
    if view_layer.objects.get(obj.name) is None:
        return False
    return any(collections.get(c.name, False) for c in obj.users_collection)


def world_points(obj):
    matrix = np.asarray(obj.matrix_world, dtype=float)
    count = len(obj.data.vertices)
    if not count:
        return np.zeros((0, 3))
    raw = np.empty(count * 3)
    obj.data.vertices.foreach_get('co', raw)
    raw = raw.reshape(-1, 3)
    return raw @ matrix[:3, :3].T + matrix[:3, 3]


def bounds(points):
    return np.array([points.min(axis=0), points.max(axis=0)]) if len(points) else None


def _overlaps(box, other, margin):
    return bool(np.all(box[0] <= other[1] + margin) and np.all(other[0] <= box[1] + margin))


def _bound_rigs(mesh, rigs):
    bound = {m.object for m in mesh.modifiers if m.type == 'ARMATURE' and m.object in rigs}
    parent = mesh.parent
    while parent is not None:
        if parent in rigs:
            bound.add(parent)
            break
        parent = parent.parent
    if not bound:
        used = {mesh.vertex_groups[g.group].name for v in mesh.data.vertices for g in v.groups if g.weight > 0}
        matches = [r for r in rigs if used.intersection(b.name for b in r.data.bones)]
        if len(matches) == 1:
            bound.add(matches[0])
    return bound


def _convert_to_mesh(bpy, obj):
    depsgraph = bpy.context.evaluated_depsgraph_get()
    evaluated = obj.evaluated_get(depsgraph)
    data = bpy.data.meshes.new_from_object(evaluated, preserve_all_data_layers=True, depsgraph=depsgraph)
    if not len(data.polygons):
        bpy.data.meshes.remove(data)
        return None
    mesh = bpy.data.objects.new(obj.name + '_mesh', data)
    for collection in obj.users_collection:
        collection.objects.link(mesh)
    mesh.matrix_world = obj.matrix_world.copy()
    if obj.parent is not None:
        world = mesh.matrix_world.copy()
        mesh.parent = obj.parent
        mesh.parent_type = obj.parent_type
        if obj.parent_type == 'BONE':
            mesh.parent_bone = obj.parent_bone
        mesh.matrix_world = world
    for slot in getattr(obj, 'material_slots', []):
        if slot.material and slot.material.name not in [m.name for m in data.materials if m]:
            data.materials.append(slot.material)
    return mesh


def unbind_static(bpy, mesh):
    """Freeze a mesh at its rest shape in world space and drop its own rig binding."""
    world = mesh.matrix_world.copy()
    for modifier in [m for m in mesh.modifiers if m.type == 'ARMATURE']:
        mesh.modifiers.remove(modifier)
    mesh.parent = None
    mesh.matrix_world = world
    mesh.vertex_groups.clear()


def _rest_world_of_bone_child(rig, bone_name, obj):
    """World matrix the object would have if its bone were in rest pose."""
    from mathutils import Matrix
    pose_bone = rig.pose.bones.get(bone_name)
    bone = rig.data.bones.get(bone_name)
    if pose_bone is None or bone is None:
        return obj.matrix_world.copy()
    posed = rig.matrix_world @ pose_bone.matrix
    rest = rig.matrix_world @ bone.matrix_local
    try:
        return rest @ posed.inverted() @ obj.matrix_world
    except ValueError:
        return obj.matrix_world.copy()


def _rigid_bind(mesh, rig, bone_name):
    world = mesh.matrix_world.copy()
    mesh.parent = rig
    mesh.parent_type = 'OBJECT'
    mesh.matrix_world = world
    mesh.vertex_groups.clear()
    group = mesh.vertex_groups.new(name=bone_name)
    group.add(list(range(len(mesh.data.vertices))), 1.0, 'REPLACE')
    for modifier in [m for m in mesh.modifiers if m.type == 'ARMATURE']:
        mesh.modifiers.remove(modifier)
    mesh.modifiers.new('Armature', 'ARMATURE').object = rig
    # Keep any generative modifiers ahead of the armature in the stack order.
    rig.data.bones[bone_name].use_deform = True


def _segment_distance(point, head, tail):
    axis = tail - head
    length = float(axis @ axis)
    t = 0. if length < 1e-12 else float(np.clip((point - head) @ axis / length, 0., 1.))
    return float(np.linalg.norm(point - (head + axis * t)))


DIGITS = ('finger', 'thumb', 'pinky', 'toe', '指')


def nearest_bone(rig, points, allowed, counts=None):
    """Nearest weighted bone; small leaf bones (fingers) defer to a larger parent."""
    matrix = np.asarray(rig.matrix_world, dtype=float)
    centre = points.mean(axis=0)
    best = None
    for bone in rig.data.bones:
        if bone.name not in allowed:
            continue
        head = matrix[:3, :3] @ np.asarray(bone.head_local) + matrix[:3, 3]
        tail = matrix[:3, :3] @ np.asarray(bone.tail_local) + matrix[:3, 3]
        distance = _segment_distance(centre, head, tail)
        if best is None or distance < best[0]:
            best = (distance, bone.name)
    if best is None:
        return None
    # Finger and toe bones hand a prop to the hand or foot they belong to.
    digit = rig.data.bones[best[1]]
    while digit.parent is not None and any(w in digit.name.lower() for w in DIGITS):
        digit = digit.parent
    if digit.name != best[1] and digit.name in allowed:
        return digit.name
    if not counts:
        return best[1]
    chain, bone = [], rig.data.bones[best[1]]
    while bone is not None and len(chain) < 2:
        if bone.name in allowed:
            chain.append(bone.name)
        bone = bone.parent
    # Prefer the larger parent when weights tie (fingers versus the hand).
    return max(enumerate(chain), key=lambda item: counts.get(item[1], 0) * (1 + .05 * item[0]))[1]


class SkinSource:
    """Nearest weighted body vertex lookup for transferring weights."""

    def __init__(self, meshes, deform):
        from mathutils.kdtree import KDTree
        rows = []
        for obj in meshes:
            matrix = np.asarray(obj.matrix_world, dtype=float)
            names = {g.index: g.name for g in obj.vertex_groups}
            for vertex in obj.data.vertices:
                weights = [(names[g.group], g.weight) for g in vertex.groups if g.weight > 0 and names.get(g.group) in deform]
                if weights:
                    rows.append((matrix[:3, :3] @ np.asarray(vertex.co) + matrix[:3, 3], weights))
        self.rows = rows
        self.tree = KDTree(max(1, len(rows)))
        for index, (point, _) in enumerate(rows):
            self.tree.insert(point.tolist(), index)
        self.tree.balance()

    def __bool__(self):
        return bool(self.rows)

    def weights(self, point):
        _, index, _ = self.tree.find(list(point))
        return self.rows[index][1] if index is not None else []

    def distance(self, point):
        _, _, distance = self.tree.find(list(point))
        return float('inf') if distance is None else float(distance)

    def counts(self):
        result = {}
        for _, weights in self.rows:
            for name, weight in weights:
                result[name] = result.get(name, 0.) + weight
        return result


def transfer_weights(mesh, source, only_missing=False, deform=None):
    names = {g.index: g.name for g in mesh.vertex_groups}
    points = world_points(mesh)
    filled = 0
    for vertex, point in zip(mesh.data.vertices, points):
        if only_missing:
            current = [g for g in vertex.groups if g.weight > 0 and names.get(g.group) in deform]
            if current:
                continue
        for name, weight in source.weights(point):
            group = mesh.vertex_groups.get(name) or mesh.vertex_groups.new(name=name)
            group.add([vertex.index], weight, 'REPLACE')
        filled += 1
    return filled


def _apply_modifier(bpy, obj, modifier):
    name = modifier.name
    modifier.show_viewport = True
    hidden, disabled = obj.hide_get(), obj.hide_viewport
    obj.hide_set(False)
    obj.hide_viewport = False
    try:
        with bpy.context.temp_override(object=obj, active_object=obj, selected_objects=[obj], selected_editable_objects=[obj]):
            bpy.ops.object.modifier_apply(modifier=name)
    finally:
        obj.hide_viewport = disabled
        obj.hide_set(hidden)


def apply_modifiers(bpy, obj, receipt, defer_shape_keys):
    """Apply generative modifiers on the rest mesh, keeping armature binding."""
    work = [m for m in obj.modifiers if m.type != 'ARMATURE']
    if not work:
        return False
    if obj.data.shape_keys and len(obj.data.shape_keys.key_blocks) > 1:
        if defer_shape_keys:
            obj['dmb_pending_modifiers'] = json.dumps([m.name for m in work])
            return False
    if obj.data.users > 1:
        obj.data = obj.data.copy()
    applied, removed = [], []
    armatures = [m for m in obj.modifiers if m.type == 'ARMATURE']
    states = [(m, m.show_viewport) for m in armatures]
    for m in armatures:
        m.show_viewport = False
    try:
        for modifier in work:
            if modifier.type in SIMULATION_MODIFIERS or not modifier.show_render:
                removed.append(modifier.name)
                obj.modifiers.remove(modifier)
                continue
            try:
                _apply_modifier(bpy, obj, modifier)
                applied.append(modifier.name)
            except RuntimeError:
                removed.append(modifier.name)
                if modifier.name in obj.modifiers:
                    obj.modifiers.remove(obj.modifiers[modifier.name])
    finally:
        for m, state in states:
            if m.name in obj.modifiers:
                m.show_viewport = state
    if obj.get('dmb_pending_modifiers') is not None:
        del obj['dmb_pending_modifiers']
    if applied:
        receipt.note('Applied modifiers on ' + obj.name + ': ' + ', '.join(applied) + '.')
    if removed:
        receipt.note('Removed simulation/disabled modifiers on ' + obj.name + ': ' + ', '.join(removed) +
                     ' (the rest shape is kept; Deadlock cloth is set up separately).')
    return True


def apply_pending_modifiers(bpy, meshes):
    """Called after static shape baking so shape keys no longer block modifiers."""
    receipt = _LAST.get('object') or Receipt()
    for obj in meshes:
        if obj.get('dmb_pending_modifiers') is not None:
            apply_modifiers(bpy, obj, receipt, defer_shape_keys=False)
    if _LAST.get('receipt') is not None and _LAST.get('object') is not None:
        _LAST['receipt'] = receipt.as_dict(_LAST['receipt'].get('character_rig'))


def _bake_mirror(obj, receipt):
    from mathutils import Matrix
    matrix = obj.matrix_world.copy()
    if obj.data.users > 1:
        obj.data = obj.data.copy()
    obj.data.transform(matrix)
    obj.data.flip_normals()
    if obj.data.has_custom_normals and hasattr(obj.data, 'normals_split_custom_set'):
        obj.data.normals_split_custom_set([(0., 0., 0.) for _ in obj.data.loops])
    obj.parent = None
    obj.matrix_world = Matrix.Identity(4)
    obj.data.update()
    receipt.note('Baked the mirrored (negative scale) transform of ' + obj.name + ' into its geometry; its normals were recomputed.')


def _clean_attributes(obj, receipt):
    supported = set(BASE_ATTRIBUTES)
    custom = obj.data.attributes.get('custom_normal')
    if custom is not None and custom.domain == 'CORNER' and custom.data_type == 'INT16_2D' and obj.data.has_custom_normals:
        supported.add('custom_normal')
    supported.update(layer.name for layer in obj.data.uv_layers)
    supported.update(layer.name for layer in obj.data.color_attributes)
    extra = [a.name for a in obj.data.attributes if a.name not in supported and not a.name.startswith('.')]
    if extra:
        if obj.data.users > 1:
            obj.data = obj.data.copy()
        for name in extra:
            attribute = obj.data.attributes.get(name)
            if attribute is not None:
                obj.data.attributes.remove(attribute)
        receipt.note('Dropped extra mesh data layers on ' + obj.name + ' that are not exported: ' + ', '.join(extra[:6]) + '.')


def _ensure_materials(bpy, obj, receipt):
    if obj.material_slots and all(slot.material for slot in obj.material_slots):
        return
    placeholder = bpy.data.materials.get('dmb_unassigned') or bpy.data.materials.new('dmb_unassigned')
    if obj.data.users > 1:
        obj.data = obj.data.copy()
    if not obj.material_slots:
        obj.data.materials.append(placeholder)
    else:
        for slot in obj.material_slots:
            if not slot.material:
                slot.link = 'DATA'
                index = slot.slot_index
                obj.data.materials[index] = placeholder
    receipt.note(obj.name + ' had empty material slots; a placeholder material was assigned. Pick its look in Appearance.')


def _remove_rigid_body(bpy, obj):
    with bpy.context.temp_override(object=obj, active_object=obj, selected_objects=[obj]):
        bpy.ops.rigidbody.object_remove()


def isolate(bpy, imported, scene_choices=None, allow_shape_keys=False, reports=None,
            select_source_rig=None, helpers=frozenset(), rig_logic=True):
    """Return (meshes, rigs) for the character; record a receipt for the UI."""
    receipt = Receipt()
    choices = _choices(scene_choices)
    realized = realize_instances(bpy, imported)
    if realized:
        imported = list(imported) + realized
        receipt.note('Made ' + str(len(realized)) + ' objects from collection instances real so they can be exported.')
    view_layer = bpy.context.view_layer
    collections = _renderable_collections(view_layer)
    rigs = [o for o in imported if o.type == 'ARMATURE']
    widgets = {b.custom_shape for rig in rigs for b in rig.pose.bones if b.custom_shape}
    candidates, hidden = [], []
    for obj in imported:
        if obj in helpers:
            receipt.mark(obj, 'excluded', 'MMD physics helper (not render geometry)')
        elif obj in widgets:
            receipt.mark(obj, 'excluded', 'bone display shape')
        elif obj.type == 'ARMATURE':
            continue
        elif obj.type in IGNORED:
            receipt.mark(obj, 'ignored', obj.type.lower().replace('_', ' ') + ' (not character geometry)')
        elif obj.name in choices['exclude']:
            receipt.mark(obj, 'excluded', 'excluded by you')
        elif obj.type not in {'MESH'} | CONVERTIBLE:
            receipt.mark(obj, 'excluded', obj.type.lower().replace('_', ' ') + ' objects cannot be exported as character geometry')
        elif not renderable(obj, view_layer, collections) and obj.name not in choices['include']:
            hidden.append(obj)
            receipt.mark(obj, 'excluded', 'hidden from rendering (include it to keep it)')
        else:
            candidates.append(obj)
    # Convert visible curve/text/metaball geometry only when it is attached to a rig.
    meshes = []
    for obj in candidates:
        if obj.type == 'MESH':
            meshes.append(obj)
            continue
        attached = any(p in rigs for p in _parents(obj)) or obj.name in choices['include']
        converted = _convert_to_mesh(bpy, obj) if attached else None
        if converted is None:
            receipt.mark(obj, 'excluded', obj.type.lower() + ' object (not attached to the character)')
        else:
            receipt.mark(obj, 'converted', obj.type.lower() + ' converted to a mesh at its current shape')
            meshes.append(converted)
    if not meshes and hidden:
        # Everything renderable was set aside; fall back to the hidden meshes.
        meshes = [o for o in hidden if o.type == 'MESH']
        for obj in meshes:
            receipt.mark(obj, 'included', 'hidden from rendering, but no visible character mesh exists')
    meshes = [o for o in meshes if o.type == 'MESH']
    for obj in list(meshes):
        if not len(obj.data.polygons):
            receipt.mark(obj, 'excluded', 'no faces (edges or points only)')
            meshes.remove(obj)
    if not meshes:
        raise ValueError('No renderable mesh objects were found in the source. Check that the character is not hidden from rendering.')
    # Single-user data before any per-object edit.
    seen = set()
    for obj in meshes:
        if obj.data.as_pointer() in seen or obj.data.users > 1:
            obj.data = obj.data.copy()
        seen.add(obj.data.as_pointer())
    character = None
    if rig_logic and rigs:
        meshes, character = _select_character(bpy, meshes, rigs, choices, receipt, select_source_rig, reports)
    for obj in list(meshes):
        _normalize(bpy, obj, character, receipt, allow_shape_keys)
    if rig_logic and character is not None:
        meshes = _bind_loose(bpy, meshes, character, receipt, choices['include'])
        _complete_weights(meshes, character, receipt)
    elif rig_logic:
        meshes = _isolate_unrigged(meshes, receipt)
    for obj in meshes:
        if obj.name not in receipt.rows:
            receipt.mark(obj, 'included', 'character geometry')
    result = receipt.as_dict(character.name if character is not None else None)
    _LAST['receipt'] = result
    _LAST['object'] = receipt
    return meshes, ([character] if character is not None else [])


def alive(obj):
    try:
        obj.name
        return True
    except ReferenceError:
        return False


def realize_instances(bpy, objects):
    """Collection instances (linked character assets) become real objects."""
    realized = []
    for obj in list(objects):
        if obj.type == 'EMPTY' and obj.instance_type == 'COLLECTION' and obj.instance_collection is not None:
            before = set(bpy.context.scene.objects)
            try:
                with bpy.context.temp_override(object=obj, active_object=obj, selected_objects=[obj],
                                               selected_editable_objects=[obj]):
                    bpy.ops.object.duplicates_make_real(use_base_parent=True, use_hierarchy=True)
            except RuntimeError:
                continue
            realized.extend(o for o in bpy.context.scene.objects if o not in before)
    return realized


def _parents(obj):
    out = []
    parent = obj.parent
    while parent is not None:
        out.append(parent)
        parent = parent.parent
    return out


def _select_character(bpy, meshes, rigs, choices, receipt, select_source_rig, reports):
    owners = {m: _bound_rigs(m, rigs) for m in meshes}
    used = sorted({r for bound in owners.values() for r in bound}, key=lambda r: r.name)
    if choices['character']:
        chosen = next((r for r in rigs if r.name == choices['character']), None)
        if chosen is None:
            raise ValueError('The chosen character rig is not in this source: ' + choices['character'])
        primary = chosen
    elif not used:
        return meshes, None
    else:
        primary = None
        if select_source_rig is not None:
            # Meshes you chose to keep stay out of the modular-part pass, so a
            # detached prop on its own rig is attached instead of removed.
            bound = [m for m in meshes if owners[m] and m.name not in choices['include']]
            found = reports if reports is not None else []
            start = len(found)
            try:
                selected = select_source_rig(bound, rigs, found)
                if len(selected) == 1:
                    primary = selected[0]
                    for report in found[start:]:
                        _record_modular(receipt, report)
                    # Modular parts may have been merged into the primary or set aside.
                    meshes = [m for m in meshes if alive(m)]
                    rigs = [r for r in rigs if alive(r)]
                    owners = {m: _bound_rigs(m, rigs) for m in meshes}
                    used = sorted({r for bound in owners.values() for r in bound}, key=lambda r: r.name)
            except ValueError as error:
                receipt.note('This file contains more than one character rig, so the most prominent character was kept. '
                             'Choose another character rig in Scene objects if that is not the one you want.')
        if primary is None:
            primary = _rank_characters(used, owners)
    # Meshes bound to other rigs: nearby ones become loose accessories, others are set aside.
    primary_meshes = [m for m in meshes if primary in owners.get(m, set())]
    if not primary_meshes:
        raise ValueError('The character rig ' + primary.name + ' has no meshes bound to it.')
    box = bounds(np.concatenate([world_points(m) for m in primary_meshes]))
    span = float(np.linalg.norm(box[1] - box[0]))
    kept = []
    for mesh in meshes:
        bound = owners.get(mesh, set())
        if not bound or primary in bound:
            if primary in bound and len(bound) > 1:
                for modifier in [m for m in mesh.modifiers if m.type == 'ARMATURE' and m.object is not primary]:
                    mesh.modifiers.remove(modifier)
            kept.append(mesh)
            continue
        other = next(iter(bound))
        mesh_box = bounds(world_points(mesh))
        if mesh_box is not None and _overlaps(mesh_box, box, span * .05) and mesh.name not in choices['exclude']:
            unbind_static(bpy, mesh)
            receipt.mark(mesh, 'bound', 'had its own rig (' + other.name + '); attached to the character at its rest shape')
            kept.append(mesh)
        elif mesh.name in choices['include']:
            unbind_static(bpy, mesh)
            receipt.mark(mesh, 'bound', 'kept by you: had its own rig (' + other.name + '); attached to the nearest bone at its rest shape')
            kept.append(mesh)
        else:
            receipt.mark(mesh, 'excluded', 'belongs to another character/rig: ' + other.name)
    rigs = [r for r in rigs if alive(r)]
    for rig in rigs:
        if rig is not primary:
            receipt.mark(rig, 'excluded' if rig in used else 'ignored',
                         'another rig (its meshes were set aside or attached)' if rig in used else 'unused rig (no meshes bound)')
    receipt.mark(primary, 'character', 'character rig')
    # Other armatures can confuse later steps; remove them from this temporary scene.
    for rig in rigs:
        if rig is not primary and rig.name in bpy.context.scene.objects:
            for child in list(rig.children):
                if child not in kept:
                    continue
                world = child.matrix_world.copy()
                child.parent = None
                child.matrix_world = world
    return kept, primary


def _record_modular(receipt, report):
    """Rows for part rigs the modular pass combined or set aside (already removed)."""
    if not isinstance(report, dict) or 'excluded_meshes' not in report:
        return
    merged = set(report.get('merged_armatures') or [])
    for name in sorted(merged):
        receipt.mark_name(name, 'ARMATURE', 'included', 'part rig combined into the character rig (its bones and weights are kept)')
    for name, rigs in sorted((report.get('mesh_bindings') or {}).items()):
        if merged.intersection(rigs or []):
            receipt.mark_name(name, 'MESH', 'included', 'part on its own rig (' + ', '.join(sorted(merged.intersection(rigs))) + '), combined with the character')
    for name in report.get('excluded_armatures') or []:
        receipt.mark_name(name, 'ARMATURE', 'excluded', 'rig of a detached prop')
    for name in report.get('excluded_meshes') or []:
        receipt.mark_name(name, 'MESH', 'excluded', 'detached prop on its own rig, away from the character (keep it to attach it to the nearest bone)')


def _rank_characters(rigs, owners):
    def score(rig):
        points = [world_points(m) for m, bound in owners.items() if rig in bound]
        points = np.concatenate(points) if points else np.zeros((1, 3))
        box = bounds(points)
        height = max(float(box[1][2] - box[0][2]), 1e-6)
        centre = (box[0] + box[1]) / 2
        distance = float(np.linalg.norm(centre[:2])) / height
        deform = sum(1 for b in rig.data.bones if b.use_deform)
        return (len(points) * (1. / (1. + distance)) * (1 + min(deform, 60) / 60.), rig.name)
    return max(rigs, key=score)


def _normalize(bpy, obj, rig, receipt, allow_shape_keys):
    if obj.data.users > 1:
        obj.data = obj.data.copy()
    # Armature modifiers: one, pointing at the character.
    armatures = [m for m in obj.modifiers if m.type == 'ARMATURE']
    for modifier in armatures:
        if modifier.object is None or (rig is not None and modifier.object is not rig):
            obj.modifiers.remove(modifier)
    armatures = [m for m in obj.modifiers if m.type == 'ARMATURE']
    for extra in armatures[1:]:
        obj.modifiers.remove(extra)
        receipt.note('Removed a duplicate armature modifier on ' + obj.name + '.')
    # Simulation helpers.
    if obj.rigid_body is not None:
        try:
            _remove_rigid_body(bpy, obj)
            receipt.note('Removed a rigid-body simulation setting from ' + obj.name + '.')
        except RuntimeError:
            pass
    # Constraints and drivers: bake the current placement; bone constraints become rigid weights.
    target_bone = None
    if obj.constraints:
        for constraint in obj.constraints:
            if (rig is not None and getattr(constraint, 'target', None) is rig and getattr(constraint, 'subtarget', '')
                    and constraint.type in {'CHILD_OF', 'COPY_TRANSFORMS', 'COPY_LOCATION', 'COPY_ROTATION', 'ARMATURE'}
                    and not constraint.mute and constraint.influence > .5):
                target_bone = constraint.subtarget
        world = obj.matrix_world.copy()
        if target_bone:
            world = _rest_world_of_bone_child(rig, target_bone, obj)
        for constraint in list(obj.constraints):
            obj.constraints.remove(constraint)
        obj.matrix_world = world
        receipt.note('Baked the constraints of ' + obj.name + ' into its rest placement.')
    if obj.animation_data is not None and (obj.animation_data.drivers or obj.animation_data.action):
        world = obj.matrix_world.copy()
        obj.animation_data_clear()
        obj.matrix_world = world
        receipt.note('Removed object animation/drivers from ' + obj.name + ' (rest placement kept).')
    skinned = rig is not None and _has_skin(obj, {b.name for b in rig.data.bones if b.use_deform})
    if target_bone and target_bone in rig.data.bones and not skinned:
        _rigid_bind(obj, rig, target_bone)
        receipt.mark(obj, 'bound', 'constrained to bone ' + target_bone + '; now rigidly weighted to it', bone=target_bone)
    elif rig is not None and obj.parent is rig and obj.parent_type == 'BONE' and obj.parent_bone in rig.data.bones:
        bone = obj.parent_bone
        world = _rest_world_of_bone_child(rig, bone, obj)
        obj.parent = None
        obj.matrix_world = world
        if skinned:
            # Already skinned: keep its weights, only drop the bone parent.
            obj.parent = rig
            obj.matrix_world = world
            receipt.mark(obj, 'repaired', 'bone parent ' + bone + ' removed; its own skin weights are kept')
        else:
            _rigid_bind(obj, rig, bone)
            receipt.mark(obj, 'bound', 'parented to bone ' + bone + '; now rigidly weighted to it', bone=bone)
    # Generative modifiers, simulations, mirrored transforms, attributes, materials.
    apply_modifiers(bpy, obj, receipt, defer_shape_keys=allow_shape_keys)
    determinant = obj.matrix_world.to_3x3().determinant()
    if abs(determinant) < 1e-12:
        raise ValueError('Object ' + obj.name + ' has a collapsed (zero) scale. Fix its scale or exclude it.')
    if determinant < 0:
        # Keep armature binding: geometry moves to world space with identity transform.
        _bake_mirror(obj, receipt)
    _clean_attributes(obj, receipt)
    _ensure_materials(bpy, obj, receipt)


def _deform_weighted(meshes, rig):
    deform = {b.name for b in rig.data.bones if b.use_deform}
    used = set()
    for obj in meshes:
        names = {g.index: g.name for g in obj.vertex_groups}
        for vertex in obj.data.vertices:
            for group in vertex.groups:
                if group.weight > 0 and names.get(group.group) in deform:
                    used.add(names[group.group])
    return deform, used


def _has_skin(obj, deform):
    names = {g.index: g.name for g in obj.vertex_groups}
    return any(g.weight > 0 and names.get(g.group) in deform for v in obj.data.vertices for g in v.groups)


def _bind_loose(bpy, meshes, rig, receipt, keep=frozenset()):
    """Bind unweighted accessories near the character; set aside distant scenery.

    Objects you chose to keep are always attached, rigidly when they are away
    from the body, and keep their offset from the nearest bone."""
    deform, used = _deform_weighted(meshes, rig)
    skinned = [m for m in meshes if _has_skin(m, deform)]
    if not skinned:
        raise ValueError('No mesh is weighted to the character rig ' + rig.name + '. Weight the character or choose another rig.')
    box = bounds(np.concatenate([world_points(m) for m in skinned]))
    height = max(float(box[1][2] - box[0][2]), 1e-6)
    source = None
    kept = list(skinned)
    for mesh in meshes:
        if mesh in skinned:
            continue
        points = world_points(mesh)
        mesh_box = bounds(points)
        flat = mesh_box is not None and _scenery(mesh_box, height)
        near = mesh_box is not None and _overlaps(mesh_box, box, height * .05)
        kept_by_you = mesh.name in keep
        if (not near or flat) and not kept_by_you:
            receipt.mark(mesh, 'excluded', 'not attached to the character' + (' (ground or backdrop)' if flat else ' (outside the character)'))
            continue
        if source is None:
            source = SkinSource(skinned, deform)
        words = weapon_words(mesh.name, *[s.material.name for s in mesh.material_slots if s.material])
        # Props held by the character stand away from the skin; clothing and
        # accessories hug it. Props move rigidly, accessories copy skin weights.
        gaps = np.array([source.distance(p) for p in points[::max(1, len(points) // 400)]])
        rigid = bool(words) or float(np.median(gaps)) > height * .02 or not near
        if rigid:
            bone = nearest_bone(rig, points, used or deform, source.counts())
            if bone is None:
                receipt.mark(mesh, 'excluded', 'no weighted bone to attach to')
                continue
            unbind_static(bpy, mesh)
            _rigid_bind(mesh, rig, bone)
            receipt.mark(mesh, 'bound', ('kept by you; rigidly attached to ' + bone + ' at its current offset') if kept_by_you and not near
                         else 'loose prop near the character; rigidly attached to ' + bone, bone=bone)
        else:
            unbind_static(bpy, mesh)
            mesh.parent = rig
            mesh.matrix_parent_inverse = rig.matrix_world.inverted()
            transfer_weights(mesh, source)
            mesh.modifiers.new('Armature', 'ARMATURE').object = rig
            receipt.mark(mesh, 'bound', 'unweighted accessory; weights copied from the nearest body surface')
        kept.append(mesh)
    return kept


def _complete_weights(meshes, rig, receipt):
    deform = {b.name for b in rig.data.bones if b.use_deform}
    source = None
    for mesh in meshes:
        names = {g.index: g.name for g in mesh.vertex_groups}
        missing = sum(1 for v in mesh.data.vertices if not any(g.weight > 0 and names.get(g.group) in deform for g in v.groups))
        if not missing:
            continue
        if source is None:
            source = SkinSource(meshes, deform)
        if not source:
            return
        own = SkinSource([mesh], deform)
        filled = transfer_weights(mesh, own if own else source, only_missing=True, deform=deform)
        if not [m for m in mesh.modifiers if m.type == 'ARMATURE']:
            mesh.modifiers.new('Armature', 'ARMATURE').object = rig
        receipt.note(str(filled) + ' vertices of ' + mesh.name + ' had no bone weights; they copy the nearest weighted vertex.')


def _scenery(box, height):
    """Ground planes, backdrops and environment pieces around the character."""
    size = box[1] - box[0]
    largest = float(size.max())
    thin = float(size.min()) < largest * .03
    return (thin and largest > height * .9) or float(max(size[0], size[1])) > height * 2.5


def _isolate_unrigged(meshes, receipt):
    """Without a rig, keep the main body cluster; set aside scenery."""
    sized = sorted(meshes, key=lambda m: len(m.data.vertices), reverse=True)
    main = bounds(world_points(sized[0]))
    height = max(float(main[1][2] - main[0][2]), 1e-6)
    kept = []
    for mesh in sized:
        box = bounds(world_points(mesh))
        flat = _scenery(box, height)
        if mesh is sized[0] or (_overlaps(box, main, height * .05) and not flat):
            kept.append(mesh)
        else:
            receipt.mark(mesh, 'excluded', 'separate from the character' + (' (ground plane)' if flat else ''))
    return kept
