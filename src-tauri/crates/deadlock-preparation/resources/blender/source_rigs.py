"""Resolve one character made from modular rigs, without altering the input file.

Exact duplicate skeletons still use the existing importer path. Here a clearly
humanoid, render-bound rig is the anchor. Matching rest bones can be shared;
offset part bones get distinct identities and retain their original world binds.
Detached non-character rigs are omitted only when geometrically separate and
unconnected. Multiple characters and ambiguous nearby parts still need review.
"""
import hashlib
import json
import re
import unicodedata

import bpy
import numpy as np
from algorithms import name_role, estimate_landmarks
from source_helpers import physics_bone_bindings

POLICY = 'modular-character-source-rig-v2'
RECEIPT_KEY = 'dmb_modular_source_receipt'
CORE = {'pelvis', 'head', 'hand_L', 'hand_R', 'foot_L', 'foot_R'}


def _role(name):
    role = name_role(name)
    if role:
        return role
    text = re.sub(r'[\s_.-]', '', unicodedata.normalize('NFKC', name))
    aliases = {'腰':'pelvis', '下半身':'pelvis', '頭':'head'}
    for side, prefix in [('L', '左'), ('R', '右')]:
        for label, role in [('手首', 'hand'), ('足首', 'foot')]:
            aliases[prefix+label] = role+'_'+side
            aliases[label+side] = role+'_'+side
    return aliases.get(text)


def _points(meshes):
    return np.asarray([obj.matrix_world @ v.co for obj in meshes for v in obj.data.vertices], dtype=float)


def _weighted(meshes):
    return {obj.vertex_groups[g.group].name for obj in meshes for v in obj.data.vertices
            for g in v.groups if g.weight > 0}


def _profile(rig, meshes):
    points = _points(meshes)
    if not len(points) or not np.isfinite(points).all():
        raise ValueError('Invalid render geometry bound to '+rig.name)
    roles = {_role(b.name) for b in rig.data.bones} & CORE
    covered = set()
    for name in _weighted(meshes):
        bone = rig.data.bones.get(name)
        if bone is None or not bone.use_deform:
            continue
        seen = set()
        while bone is not None and bone.name not in seen:
            seen.add(bone.name)
            covered.add(_role(bone.name))
            bone = bone.parent
    named_body = roles == CORE and len(covered & CORE) >= 4
    geometric_body = False
    if not named_body:
        # The same upright humanoid cross-section check used for unrigged input
        # protects a second character whose bone names are not recognized.
        try:
            estimate_landmarks(points)
            geometric_body = True
        except ValueError:
            pass
    return {'character':named_body or geometric_body,
            'anatomical_roles':sorted(roles),
            'evidence':'weighted anatomical bones' if named_body else 'humanoid geometry' if geometric_body else 'partial or prop rig',
            'bounds':np.array([points.min(0), points.max(0)])}


def _same(a, b):
    return np.allclose(np.asarray(a), np.asarray(b), rtol=1e-7, atol=1e-6)


def _copied_bind(a, b):
    # EditBone rebuilds direction/roll from float32 head/tail coordinates.
    # Moving a tiny face bone into body space can round its axis by a few ppm.
    # This applies only to that copy operation, never to sharing two source
    # bones or to any original/compiled model bind-compatibility budget.
    a, b = np.asarray(a), np.asarray(b)
    return (np.allclose(a[:3, 3], b[:3, 3], rtol=1e-7, atol=1e-6)
            and np.allclose(a[:3, :3], b[:3, :3], rtol=1e-7, atol=4e-6)
            and np.allclose(a[:3, :3].T @ a[:3, :3], b[:3, :3].T @ b[:3, :3], rtol=1e-7, atol=1e-6))


def _new_name(rig, name, reserved):
    # Blender limits bone names by UTF-8 bytes. Reserve against every existing
    # vertex group as well as every bone, so renames never merge unrelated weights.
    for index in range(10000):
        digest = hashlib.sha256((rig+'\0'+name+'\0'+str(index)).encode()).hexdigest()[:10]
        prefix = 'dmb_part_'+digest+'_'
        suffix = name.encode()[:63-len(prefix)].decode('utf-8', errors='ignore')
        result = prefix+suffix
        if result not in reserved:
            reserved.add(result)
            return result
    raise ValueError('Could not allocate a unique source-part bone identity.')


def _part_plan(primary, other, meshes, span, reserved):
    primary_world = {b.name:primary.matrix_world @ b.matrix_local for b in primary.data.bones}
    world = {b.name:other.matrix_world @ b.matrix_local for b in other.data.bones}
    shared = set(primary_world) & set(world)
    anchors = {name:name for name in shared
               if (world[name].translation-primary_world[name].translation).length <= max(1e-6, span*.03)}
    explicit = (other.parent_bone if other.parent == primary and other.parent_type == 'BONE'
                and other.parent_bone in primary_world else None)
    if not anchors and not explicit:
        return None, shared
    weighted = _weighted(meshes)
    for name in weighted & set(world):
        bone = other.data.bones[name]
        while bone is not None and bone.name not in anchors:
            bone = bone.parent
        if bone is None and explicit is None:
            raise ValueError('Part rig '+other.name+' has a weighted branch without a character attachment: '+name+
                             '. Bind that branch to its intended character bone before importing.')
    mapping = {}
    reused = set()
    for b in other.data.bones:
        if b.name in anchors and _same(world[b.name], primary_world[b.name]) and b.use_deform == primary.data.bones[b.name].use_deform:
            mapping[b.name] = b.name
            reused.add(b.name)
        elif b.name in reserved:
            mapping[b.name] = _new_name(other.name, b.name, reserved)
        else:
            mapping[b.name] = b.name
            reserved.add(b.name)
    bones = []
    inverse = primary.matrix_world.inverted()
    for b in other.data.bones:
        if b.name in reused:
            continue
        local = inverse @ world[b.name]
        basis = np.asarray(local.to_3x3())
        if not np.isfinite(basis).all() or not np.allclose(basis.T @ basis, np.eye(3), rtol=1e-6, atol=1e-6) or np.linalg.det(basis) <= 0:
            raise ValueError('Part rig '+other.name+' has a relative scale/shear that cannot preserve its rest bind exactly. Apply matching rig transforms in a source copy.')
        parent = anchors.get(b.name) or (mapping[b.parent.name] if b.parent else explicit)
        bones.append({'name':mapping[b.name], 'source':b.name, 'parent':parent,
                      'matrix':local.copy(), 'world':world[b.name].copy(), 'length':b.length,
                      'deform':b.use_deform, 'inherit_scale':b.inherit_scale,
                      'inherit_rotation':b.use_inherit_rotation, 'local_location':b.use_local_location})
    return {'rig':other, 'meshes':meshes, 'mapping':mapping, 'bones':bones,
            'anchors':anchors, 'explicit':explicit, 'reused':sorted(reused)}, shared


def _merge(primary, plans):
    before = {b.name:primary.matrix_world @ b.matrix_local for b in primary.data.bones}
    if any(plan['bones'] for plan in plans):
        if bpy.context.object and bpy.context.object.mode != 'OBJECT':
            bpy.ops.object.mode_set(mode='OBJECT')
        prior_active = bpy.context.view_layer.objects.active
        prior_selected = list(bpy.context.selected_objects)
        hidden, viewport = primary.hide_get(), primary.hide_viewport
        primary.hide_set(False)
        primary.hide_viewport = False
        bpy.ops.object.select_all(action='DESELECT')
        primary.select_set(True)
        bpy.context.view_layer.objects.active = primary
        if primary.data.users > 1:
            primary.data = primary.data.copy()
        try:
            bpy.ops.object.mode_set(mode='EDIT')
            for plan in plans:
                for row in plan['bones']:
                    bone = primary.data.edit_bones.new(row['name'])
                    if bone.name != row['name']:
                        raise ValueError('Source-part bone identity changed while merging.')
                    bone.head = (0, 0, 0)
                    bone.tail = (0, max(row['length'], 1e-5), 0)
                    bone.matrix = row['matrix']
                    bone.use_deform = row['deform']
                    bone.inherit_scale = row['inherit_scale']
                    bone.use_inherit_rotation = row['inherit_rotation']
                    bone.use_local_location = row['local_location']
            for plan in plans:
                for row in plan['bones']:
                    if row['parent']:
                        primary.data.edit_bones[row['name']].parent = primary.data.edit_bones[row['parent']]
            bpy.ops.object.mode_set(mode='OBJECT')
        finally:
            if primary.mode != 'OBJECT':
                bpy.ops.object.mode_set(mode='OBJECT')
            primary.select_set(False)
            primary.hide_set(hidden)
            primary.hide_viewport = viewport
            for obj in prior_selected:
                obj.select_set(True)
            bpy.context.view_layer.objects.active = prior_active
    expected = dict(before)
    for plan in plans:
        expected.update({row['name']:row['world'] for row in plan['bones']})
    for name, world in expected.items():
        if not _copied_bind(primary.matrix_world @ primary.data.bones[name].matrix_local, world):
            raise ValueError('Modular rig import could not preserve the original rest transform: '+name)
    for plan in plans:
        for obj in plan['meshes']:
            world = obj.matrix_world.copy()
            for group in obj.vertex_groups:
                if group.name in plan['mapping']:
                    group.name = plan['mapping'][group.name]
            for modifier in obj.modifiers:
                if modifier.type == 'ARMATURE' and modifier.object == plan['rig']:
                    modifier.object = primary
            if obj.parent == plan['rig'] and obj.parent_type == 'BONE':
                obj.parent_bone = plan['mapping'].get(obj.parent_bone, obj.parent_bone)
            else:
                obj.parent_type = 'OBJECT'
            obj.parent = primary
            obj.matrix_world = world
            if not _same(obj.matrix_world, world):
                raise ValueError('Modular rig import changed mesh placement: '+obj.name)
    bpy.context.view_layer.update()


def resolve_modular_rigs(meshes, rigs, selected, owners, reports, validate_controls):
    details = '; '.join(r.name+' ['+', '.join(n for n, bound in owners.items() if r.name in bound)+']' for r in selected)
    bound = {r:[m for m in meshes if owners[m.name] == [r.name]] for r in selected}
    if any(len(owners[m.name]) > 1 for m in meshes):
        raise ValueError('Unsupported: multiple armatures affect the same character mesh. '+details)
    profiles = {r:_profile(r, bound[r]) for r in selected}
    candidates = [r for r in selected if profiles[r]['character']]
    if len(candidates) != 1:
        raise ValueError('Unsupported: multiple armatures could not be resolved to one character. '+details+
                         '. Keep one character and its attached parts; no character was selected by rig name or bone count.')
    primary = candidates[0]
    bounds = profiles[primary]['bounds']
    span = float(np.linalg.norm(bounds[1]-bounds[0]))
    if not np.isfinite(span) or span <= 1e-8:
        raise ValueError('Character has invalid bounds.')
    reserved = set(primary.data.bones.keys()) | {g.name for obj in meshes for g in obj.vertex_groups}
    plans, excluded = [], []
    for other in selected:
        if other == primary:
            continue
        plan, shared = _part_plan(primary, other, bound[other], span, reserved)
        if plan is not None:
            plans.append(plan)
            continue
        box = profiles[other]['bounds']
        gap = np.maximum(np.maximum(box[0]-bounds[1], bounds[0]-box[1]), 0)
        related = False
        for obj in [other, *bound[other]]:
            cursor = obj
            while cursor is not None:
                related |= cursor == primary or any(getattr(c, 'target', None) == primary for c in cursor.constraints)
                cursor = cursor.parent
        if not shared and not related and not profiles[other]['anatomical_roles'] and np.linalg.norm(gap) > span*.08:
            excluded.append(other)
        else:
            raise ValueError('Rig '+other.name+' may be part of the character but has no verified attachment. '+
                             'Bind it to the intended character bone; no nearby geometry was discarded.')
    retained = [m for m in meshes if not any(m in bound[r] for r in excluded)]
    # Every surviving source rig must pass the existing object/pose controls
    # checks before any secondary rig is removed from the workspace copy.
    validate_controls(primary, retained)
    for plan in plans:
        validate_controls(plan['rig'], plan['meshes'])
        if physics_bone_bindings(bpy.context.scene.objects, plan['rig']):
            raise ValueError('Part rig '+plan['rig'].name+' has independent physics helper bindings. '+
                             'Use a shared character armature for those helpers; their bindings were not discarded.')
        for mesh in plan['meshes']:
            parent = mesh.parent
            while parent is not None:
                if parent != plan['rig'] and (parent.constraints or (parent.animation_data and parent.animation_data.drivers)):
                    raise ValueError('Part mesh '+mesh.name+' has a live parent transform; bake its rest placement before importing.')
                parent = parent.parent
    receipt = {'policy':POLICY, 'selected_armatures':[primary.name],
               'selection_evidence':profiles[primary]['evidence'],
               'merged_armatures':[p['rig'].name for p in plans],
               'excluded_armatures':[r.name for r in excluded],
               'excluded_meshes':[m.name for r in excluded for m in bound[r]],
               'retained_meshes':[m.name for m in retained],
               'part_bones':{p['rig'].name:p['mapping'] for p in plans},
               'mesh_bindings':owners, 'rigid_body_meshes':0, 'joint_empties':0, 'objects':[]}
    _merge(primary, plans)
    for rig in [p['rig'] for p in plans]+excluded:
        if rig in excluded:
            for obj in bound[rig]:
                bpy.data.objects.remove(obj, do_unlink=True)
        bpy.data.objects.remove(rig, do_unlink=True)
    meshes[:] = retained
    receipt['warning'] = ('Character rig: '+primary.name+'. Combined attached part rigs: '+
                          (', '.join(receipt['merged_armatures']) or 'none')+
                          '. Excluded detached props from character preparation: '+
                          (', '.join(receipt['excluded_meshes']) or 'none')+
                          '. Part rest positions and skin weights were preserved. The original source file is unchanged.')
    primary[RECEIPT_KEY] = json.dumps(receipt)
    if reports is not None:
        reports.append(receipt)
    return [primary]


def append_saved_notice(meshes, rigs, reports):
    """Carry the stage-import explanation into inspection; never use it to select."""
    if reports is None or len(rigs) != 1 or any(r.get('policy') == POLICY for r in reports):
        return
    value = rigs[0].get(RECEIPT_KEY)
    if not isinstance(value, str) or len(value) > 131072:
        return
    try:
        receipt = json.loads(value)
        if (isinstance(receipt, dict) and receipt.get('policy') == POLICY
                and receipt.get('selected_armatures') == [rigs[0].name]
                and sorted(receipt.get('retained_meshes', [])) == sorted(m.name for m in meshes)
                and isinstance(receipt.get('warning'), str)):
            reports.append(receipt)
    except (ValueError, TypeError):
        pass
