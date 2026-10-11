"""Recognize MMD authoring helpers without loading add-ons or changing a scene.

Use saved metadata AND non-rendered, unskinned rigid-body relationships.
MMD Tools is never loaded. Both its unbuilt and built physics layouts are
recognized, including Blender 5's separate system-property storage.
"""
import json
import re


def uses_linked_libraries(bpy):
    """Check actual external datablocks, not Blender 5.2's append bookkeeping.

    Local appends may retain a Library ID although every appended datablock is
    local. The all_ids iterator (or the all-datablock user map on older Blender)
    also covers nested materials/node groups and unused linked IDs.
    No datablock or file is changed.
    """
    if not bpy.data.libraries:
        return False
    ids = getattr(bpy.data, 'all_ids', None)
    if ids is None:
        ids = bpy.data.user_map().keys()
    return any(block.library is not None for block in ids)


_KINDS = {1: 'NONE', 2: 'ROOT', 3: 'RIGID_GRP_OBJ', 4: 'JOINT_GRP_OBJ',
          5: 'TEMPORARY_GRP_OBJ', 22: 'JOINT', 23: 'RIGID_BODY',
          51: 'TRACK_TARGET', 52: 'NON_COLLISION_CONSTRAINT',
          53: 'SPRING_CONSTRAINT', 54: 'SPRING_GOAL'}
_GROUPS = {'rigidbodies': 'RIGID_GRP_OBJ', 'joints': 'JOINT_GRP_OBJ',
           'temporary': 'TEMPORARY_GRP_OBJ'}


def _stored(obj, key):
    """Read serialized add-on data without registering or invoking the add-on.

    Blender 5 split RNA storage from custom properties. Prefer that storage
    when a key exists, including an invalid value, over a stale legacy copy.
    Older Blender versions expose the same saved data as custom properties.
    """
    getter = getattr(obj, 'bl_system_properties_get', None)
    props = getter() if getter else None
    if props is not None and key in props:
        return props[key]
    return obj.get(key)


def _kind(obj):
    value = _stored(obj, 'mmd_type')
    if value is None:
        return None
    if type(value) is int:
        return _KINDS.get(value, 'UNKNOWN')
    return value if isinstance(value, str) and value in _KINDS.values() else 'UNKNOWN'


def _record(obj, key):
    value = _stored(obj, key)
    return value if hasattr(value, 'get') else {}


def _group(obj, name, root):
    if obj is None or obj.type != 'EMPTY' or obj.parent != root:
        return False
    kind = _kind(obj)
    return (kind == _GROUPS[name] or (kind is None and
            re.fullmatch(re.escape(name) + r'(?:\.\d{3,})?', obj.name) is not None))


def _root(obj):
    if obj is None or obj.type != 'EMPTY':
        return False
    version = obj.get('mmd_tools_version')
    kind = _kind(obj)
    versioned = isinstance(version, str) and re.fullmatch(r'\d+\.\d+\.\d+(?:[.-][\w.-]+)?', version) is not None
    return ((kind in {None, 'ROOT'} and versioned) or
            (kind == 'ROOT' and bool(_record(obj, 'mmd_root'))))


def _bare_collision(obj):
    return (obj.type == 'MESH' and obj.hide_render and obj.rigid_body is not None
            and not obj.vertex_groups and not obj.modifiers and not obj.data.shape_keys
            and not obj.animation_data and not obj.data.animation_data)


def _tagged_collision(obj):
    shape = _record(obj, 'mmd_rigid').get('shape')
    if type(shape) not in {int, str}:
        return False
    shape = {1: 'SPHERE', 2: 'BOX', 3: 'CAPSULE'}.get(shape, shape)
    return (_kind(obj) == 'RIGID_BODY' and shape in {'SPHERE', 'BOX', 'CAPSULE'}
            and shape == obj.rigid_body.collision_shape)


def _classify(objects):
    objects = set(objects)
    roots = {obj for obj in objects if _root(obj)}
    rigs = {root: [obj for obj in objects if obj.type == 'ARMATURE' and obj.parent == root]
            for root in roots}
    tracks = {}
    for root, children in rigs.items():
        for rig in children:
            for bone in rig.pose.bones:
                for constraint in bone.constraints:
                    if (constraint.name != 'mmd_tools_rigid_track' or
                            constraint.type not in {'COPY_TRANSFORMS', 'COPY_ROTATION', 'DAMPED_TRACK'}):
                        continue
                    target = constraint.target
                    if (target not in objects or target.type != 'EMPTY' or target.parent not in objects
                            or target.constraints or target.animation_data):
                        continue
                    kind = _kind(target)
                    if kind == 'TRACK_TARGET' or (kind is None and re.fullmatch(r'mmd_bonetrack(?:\.\d{3,})?', target.name)):
                        tracks.setdefault(target.parent, []).append((rig, bone.name, target))
    helpers, bindings, body_roots = {}, {}, {}
    for obj in objects:
        if not _bare_collision(obj) or _kind(obj) not in {None, 'RIGID_BODY'}:
            continue
        parent = obj.parent
        root = parent.parent if parent else None
        if root not in roots or parent not in objects or len(rigs[root]) != 1:
            continue
        rig = rigs[root][0]
        in_group = _group(parent, 'rigidbodies', root)
        bone_parent = (parent == rig and obj.parent_type == 'BONE'
                       and obj.parent_bone in rig.data.bones and obj.rigid_body.kinematic)
        if not in_group and not bone_parent:
            continue
        tagged = _tagged_collision(obj)
        if obj.constraints:
            if len(obj.constraints) != 1:
                continue
            constraint = obj.constraints[0]
            if (constraint.type != 'CHILD_OF' or constraint.name != 'mmd_tools_rigid_parent'
                    or constraint.target != rig or (not constraint.mute and not obj.rigid_body.kinematic)):
                continue
            name = constraint.subtarget
            if name not in rig.data.bones and (name or not tagged):
                continue
            if bone_parent and name != obj.parent_bone:
                continue
        else:
            # Built dynamics replace Child Of with a pose-bone -> tracking
            # empty -> rigid-body link. Never infer a binding from a name.
            if not tagged:
                continue
            name = _record(obj, 'mmd_rigid').get('bone', '')
            if not isinstance(name, str) or (name and name not in rig.data.bones):
                continue
            if bone_parent:
                if name != obj.parent_bone:
                    continue
            elif name and not any(r == rig and b == name for r, b, _ in tracks.get(obj, [])):
                continue
        helpers[obj] = 'mmd_rigid_body'
        bindings[obj] = (rig, name)
        body_roots[obj] = root
    for body, links in tracks.items():
        if body not in bindings:
            continue
        for rig, name, target in links:
            if bindings[body] == (rig, name):
                helpers[target] = 'mmd_track_target'
    # Older MMD versions build hidden spring-goal copies. Require both a
    # verified parent collider and a typed spring joint to another collider.
    for obj in objects:
        if (_bare_collision(obj) and _kind(obj) == 'SPRING_GOAL' and obj.rigid_body.kinematic
                and not obj.constraints and obj.parent in bindings):
            root = body_roots[obj.parent]
            for joint in objects:
                group = joint.parent
                rb = joint.rigid_body_constraint
                if (joint.type == 'EMPTY' and _kind(joint) == 'SPRING_CONSTRAINT'
                        and group in objects and _group(group, 'temporary', root) and rb
                        and not joint.constraints and not joint.animation_data
                        and ((rb.object1 == obj and body_roots.get(rb.object2) == root)
                             or (rb.object2 == obj and body_roots.get(rb.object1) == root))):
                    helpers[obj] = 'mmd_spring_goal'
                    body_roots[obj] = root
                    break
    for obj in objects:
        group = obj.parent
        root = group.parent if group else None
        if (obj.type != 'EMPTY' or root not in roots or group not in objects
                or obj.constraints or obj.animation_data):
            continue
        ordinary = _group(group, 'joints', root) and _kind(obj) in {None, 'JOINT'}
        temporary = (_group(group, 'temporary', root)
                     and _kind(obj) in {'NON_COLLISION_CONSTRAINT', 'SPRING_CONSTRAINT'})
        joint = obj.rigid_body_constraint
        if ((ordinary or temporary) and joint and body_roots.get(joint.object1) == root
                and body_roots.get(joint.object2) == root):
            helpers[obj] = 'mmd_joint' if ordinary else 'mmd_temporary_joint'
    for obj in objects:
        if (obj.parent in roots and any(_group(obj, name, obj.parent) for name in _GROUPS)
                and obj.children and all(child in helpers for child in obj.children)):
            helpers[obj] = 'mmd_helper_group'
    return helpers, bindings


def mmd_helpers(objects):
    """Return proven helper objects and their kinds; never mutate input data."""
    return _classify(objects)[0]


def helper_receipt(helpers):
    bodies = sum(kind == 'mmd_rigid_body' for kind in helpers.values())
    springs = sum(kind == 'mmd_spring_goal' for kind in helpers.values())
    return {'policy': 'mmd-authoring-helpers-v2', 'rigid_body_meshes': bodies,
            'spring_goal_meshes': springs,
            'joint_empties': sum(kind == 'mmd_joint' for kind in helpers.values()),
            'built_helpers': sum(kind in {'mmd_track_target', 'mmd_temporary_joint', 'mmd_spring_goal'}
                                 for kind in helpers.values()),
            'objects': [{'name': obj.name, 'kind': kind} for obj, kind in
                        sorted(helpers.items(), key=lambda item: item[0].name)],
            'warning': (f'Excluded {bodies + springs} MMD collision helper meshes from character geometry. '
                        'The source file is unchanged. MMD physics is not converted to Deadlock cloth.') if bodies else ''}


def physics_bone_bindings(objects, rig):
    """Read verified saved rigid-body bindings without running MMD or simulation.

    A bone name, deform flag or vertex weight alone never identifies physics.
    Kinematic collision bodies (arms/legs, for example) are not dynamic bones.
    """
    result = {}
    _, bindings = _classify(objects)
    for obj, (bound_rig, name) in bindings.items():
        if bound_rig != rig or not name:
            continue
        row = result.setdefault(name, {'policy': 'verified-rigid-body-bone-binding-v1',
                                       'dynamic_bodies': [], 'kinematic_bodies': []})
        dynamic = obj.rigid_body.type == 'ACTIVE' and not obj.rigid_body.kinematic
        row['dynamic_bodies' if dynamic else 'kinematic_bodies'].append(obj.name)
    for row in result.values():
        row['dynamic_bodies'].sort()
        row['kinematic_bodies'].sort()
    return result


def helper_diagnostic(obj):
    """Small factual receipt when an object cannot be safely classified."""
    parent = obj.parent
    return json.dumps({
        'mmd_type': _kind(obj), 'parent': parent.name if parent else None,
        'parent_mmd_type': _kind(parent) if parent else None,
        'hidden_from_render': bool(obj.hide_render), 'rigid_body': bool(obj.rigid_body),
        'kinematic': bool(obj.rigid_body and obj.rigid_body.kinematic),
        'vertex_groups': len(obj.vertex_groups),
        'constraints': [{'name': c.name, 'type': c.type, 'muted': bool(c.mute),
                         'target': c.target.name if getattr(c, 'target', None) else None,
                         'bone': getattr(c, 'subtarget', None)} for c in list(obj.constraints)[:8]],
        'drivers': len(obj.animation_data.drivers) if obj.animation_data else 0,
    }, ensure_ascii=False, sort_keys=True)


def remove_workspace_helpers(bpy, helpers):
    """Only used when producing a converted workspace file, never at intake."""
    for obj in helpers:
        # An unexpected non-helper child must not lose its transform/parent.
        if any(child not in helpers for child in obj.children):
            raise ValueError('MMD helper has a character child; automatic conversion stopped: ' + obj.name)
    # All references were collected before replacing the source armature.
    for obj in helpers:
        bpy.data.objects.remove(obj, do_unlink=True)
