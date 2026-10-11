"""Exact Source 2 bind data beside Blender's rigid edit-bone representation.

Blender EditBone/Bone rest matrices contain orientation and position, not bone
scale. Keep authored local transforms and their full T*R*S world/inverse binds
as canonical metadata. The display rig uses unit orientation at the exact world
joint positions. Its skinning can represent the same affine transforms using
P_blender = W_pose @ inverse(W_bind) @ B_blender_rest.

The inverse binds here are inverse(reconstructed DATA world), not the original
mesh MDAT matrices. Original MDAT differences are a native-validation concern.
"""
import hashlib
import json
import math
import numpy as np

SCALE_POLICY = 'source2-local-scale-dmx-v1'
METADATA_KEY = 'dmb_exact_bind_json'
DISPLAY_POLICY = 'rigid-display-with-exact-source2-bind-metadata-v1'
# The same physical safety budgets used for skeleton/bind validation. These
# apply only to Blender's float32 display conversion, never to authored values.
DISPLAY_ANGLE_LIMIT_DEGREES = .1
DISPLAY_JOINT_LIMIT_MM = .5
DISPLAY_WORLD_SCALE_LIMIT = 1e-3


def _matrix(value, label):
    result = np.asarray(value, dtype=float)
    if result.shape != (4, 4) or not np.isfinite(result).all():
        raise ValueError('Invalid exact bind matrix: ' + label)
    if not np.array_equal(result[3], [0., 0., 0., 1.]):
        raise ValueError('Non-affine exact bind matrix: ' + label)
    if np.linalg.det(result[:3, :3]) <= 0:
        raise ValueError('Mirrored/singular exact bind matrix: ' + label)
    return result


def rigid_display_matrix(full):
    """Normalize display axes only; retain the full bind separately, unchanged."""
    result = _matrix(full, 'display source').copy()
    linear = result[:3, :3]
    scale = math.sqrt(float(np.trace(linear.T @ linear)) / 3.)
    if not np.allclose(linear.T @ linear, np.eye(3) * scale * scale, rtol=1e-9, atol=1e-10):
        raise ValueError('Exact Source 2 adapter requires positive uniform bone scale, without shear.')
    result[:3, :3] /= scale
    return result


def _quaternion(rotation):
    """Only for legacy unit-scale programmatic fixtures without authored locals."""
    r = np.asarray(rotation, dtype=float)
    trace = np.trace(r)
    if trace > 0:
        s = math.sqrt(trace + 1.) * 2.
        q = [(r[2, 1] - r[1, 2]) / s, (r[0, 2] - r[2, 0]) / s,
             (r[1, 0] - r[0, 1]) / s, .25 * s]
    else:
        i = int(np.argmax(np.diag(r))); j, k = (i + 1) % 3, (i + 2) % 3
        s = math.sqrt(max(0., 1. + r[i, i] - r[j, j] - r[k, k])) * 2.
        q = [0., 0., 0., (r[k, j] - r[j, k]) / s]
        q[i], q[j], q[k] = .25 * s, (r[i, j] + r[j, i]) / s, (r[i, k] + r[k, i]) / s
    return (np.asarray(q) / np.linalg.norm(q)).tolist()


def _local_matrix(position, rotation, scale):
    p, q = np.asarray(position, dtype=float), np.asarray(rotation, dtype=float)
    if p.shape != (3,) or q.shape != (4,) or not np.isfinite(p).all() or not np.isfinite(q).all():
        raise ValueError('Invalid authored local transform in exact bind metadata.')
    length = np.linalg.norm(q)
    if length <= 0 or not math.isfinite(scale) or scale <= 0:
        raise ValueError('Invalid authored local quaternion/scale in exact bind metadata.')
    x, y, z, w = q / length
    r = np.array([[1-2*y*y-2*z*z, 2*x*y-2*z*w, 2*x*z+2*y*w],
                  [2*x*y+2*z*w, 1-2*x*x-2*z*z, 2*y*z-2*x*w],
                  [2*x*z-2*y*w, 2*y*z+2*x*w, 1-2*x*x-2*y*y]])
    m = np.eye(4); m[:3, :3] = r * scale; m[:3, 3] = p
    return m


def canonical_bones(target):
    """Validate and copy exact canonical data; never recover scale from bpy bones."""
    names = [b['name'] for b in target]
    if len(names) != len(set(names)) or not names:
        raise ValueError('Exact target requires unique named bones.')
    matrices = {b['name']: _matrix(b['matrix'], b['name']) for b in target}
    result = []
    for bone in target:
        name, parent = bone['name'], bone.get('parent')
        if parent is not None and parent not in matrices:
            raise ValueError('Exact target references a missing parent: ' + name)
        world = matrices[name]
        rigid_display_matrix(world)  # Positive scalar-only contract.
        local = np.linalg.inv(matrices[parent]) @ world if parent else world
        supplied = all(k in bone for k in ('local_position', 'local_rotation', 'local_scale'))
        if supplied:
            pos, rot, scale = list(bone['local_position']), list(bone['local_rotation']), float(bone['local_scale'])
            authored = _local_matrix(pos, rot, scale)
            if not np.allclose(local, authored, rtol=1e-9, atol=1e-9):
                raise ValueError('Authored local transform disagrees with exact world bind: ' + name)
        else:
            # Older integration fixtures pass only rigid world frames. Nonunit
            # production skeletons must provide their actual authored locals.
            if not np.allclose(local[:3, :3].T @ local[:3, :3], np.eye(3), rtol=1e-9, atol=1e-9):
                raise ValueError('Scaled exact target is missing authored local fields: ' + name)
            pos, rot, scale = local[:3, 3].tolist(), _quaternion(local[:3, :3]), 1.
        inverse = _matrix(bone.get('inverse_bind_matrix', np.linalg.inv(world)), name + ' inverse')
        if not np.allclose(world @ inverse, np.eye(4), rtol=1e-9, atol=1e-9):
            raise ValueError('Exact inverse bind is not inverse(reconstructed DATA world): ' + name)
        result.append({'name': name, 'parent': parent, 'local_position': list(map(float, pos)),
                       'local_rotation': list(map(float, rot)), 'local_scale': scale,
                       'matrix': world.tolist(), 'inverse_bind_matrix': inverse.tolist(),
                       'local_source': 'authored' if supplied else 'legacy-unit-fixture'})
    return result


def canonical_hash(bones):
    return hashlib.sha256(json.dumps(bones, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()).hexdigest()


def equivalent_pose_matrix(world_pose, inverse_bind, blender_rest):
    """Exact skin equivalence; compensation is on the pose, never on the mesh."""
    return _matrix(world_pose, 'pose') @ _matrix(inverse_bind, 'inverse bind') @ _matrix(blender_rest, 'display rest')


def _display_snapshot(rig):
    return {b.name: {'parent': b.parent.name if b.parent else None,
                     'matrix': [list(row) for row in b.matrix_local],
                     'head': list(b.head_local), 'tail': list(b.tail_local),
                     'use_deform': bool(b.use_deform), 'inherit_scale': b.inherit_scale} for b in rig.data.bones}


def audit_display_bind(display, canonical):
    """Measure bpy's rigid approximation without changing any canonical bind.

    EditBone -> Bone converts world frames to parent-relative head/tail/roll
    floats and reconstructs the hierarchy. Near the -Y roll singularity this
    introduces larger matrix-component errors than an ordinary float32 cast.
    Assess physical angular/joint error and residual stretch separately; an
    SVD extracts the closest proper rotation without silently accepting shear.
    """
    if set(display) != {b['name'] for b in canonical}:
        raise ValueError('Display skeleton differs from exact target palette.')
    rows = []
    for bone in canonical:
        actual = display[bone['name']]
        if actual['parent'] != bone['parent']:
            raise ValueError('Display skeleton hierarchy differs from exact target: ' + bone['name'])
        expected = rigid_display_matrix(bone['matrix'])
        matrix = _matrix(actual['matrix'], bone['name'] + ' Blender display')
        u, singular, vt = np.linalg.svd(matrix[:3, :3])
        rotation = u @ vt
        relative = expected[:3, :3].T @ rotation
        sine = np.linalg.norm([relative[2, 1] - relative[1, 2],
                               relative[0, 2] - relative[2, 0],
                               relative[1, 0] - relative[0, 1]]) / 2.
        angle = math.degrees(math.atan2(sine, (np.trace(relative) - 1.) / 2.))
        distance = float(np.linalg.norm(matrix[:3, 3] - expected[:3, 3])) * 25.4
        stretch = float(np.max(np.abs(singular - 1.)))
        orthogonality = float(np.max(np.abs(matrix[:3, :3].T @ matrix[:3, :3] - np.eye(3))))
        if (np.linalg.det(rotation) <= 0 or angle > DISPLAY_ANGLE_LIMIT_DEGREES
                or distance > DISPLAY_JOINT_LIMIT_MM or stretch > DISPLAY_WORLD_SCALE_LIMIT
                or orthogonality > 2 * DISPLAY_WORLD_SCALE_LIMIT + DISPLAY_WORLD_SCALE_LIMIT ** 2):
            raise ValueError('Blender display bind exceeds skeleton safety bounds: ' + bone['name']
                             + f' ({angle:.9g} degrees, {distance:.9g} mm, {stretch:.9g} residual stretch).')
        # ULP is diagnostic, not an allowance applied to canonical metadata.
        endpoints = np.asarray([actual['head'], actual['tail']], dtype=np.float32)
        ulp = float(np.max(np.spacing(np.abs(endpoints))))
        rows.append({'name': bone['name'], 'rotation_error_degrees': angle,
                     'joint_error_mm': distance, 'residual_stretch': stretch,
                     'axis_orthogonality_error': orthogonality, 'head_tail_float32_ulp_inches': ulp})
    return {'provenance': 'Blender-float32-parent-relative-head-tail-roll-reconstruction',
            'canonical_values_unchanged': True,
            'rotation_limit_degrees': DISPLAY_ANGLE_LIMIT_DEGREES, 'joint_limit_mm': DISPLAY_JOINT_LIMIT_MM,
            'residual_stretch_limit': DISPLAY_WORLD_SCALE_LIMIT,
            **{'max_' + key: max(r[key] for r in rows) for key in
               ('rotation_error_degrees', 'joint_error_mm', 'residual_stretch',
                'axis_orthogonality_error', 'head_tail_float32_ulp_inches')}, 'bones': rows}


def store_bind_metadata(rig, target):
    canonical = canonical_bones(target)
    display = _display_snapshot(rig)
    display_audit = audit_display_bind(display, canonical)
    metadata = {'policy': SCALE_POLICY, 'display_policy': DISPLAY_POLICY, 'bones': canonical,
                'canonical_sha256': canonical_hash(canonical), 'display_bones': display,
                'display_audit': display_audit,
                'object_matrix': [list(row) for row in rig.matrix_world],
                'inverse_bind_provenance': 'inverse-of-reconstructed-data-world'}
    rig[METADATA_KEY] = json.dumps(metadata, sort_keys=True, separators=(',', ':'), allow_nan=False)
    return {'policy': SCALE_POLICY, 'display_policy': DISPLAY_POLICY,
            'canonical_sha256': metadata['canonical_sha256'], 'bone_count': len(canonical),
            'exact_locals_world_and_inverse_binds_stored': True,
            'blender_edit_bone_scale_preserved': False,
            'display_audit': display_audit,
            'inverse_bind_provenance': metadata['inverse_bind_provenance']}


def load_correction_bind(rig, target):
    raw = rig.get(METADATA_KEY)
    if not isinstance(raw, str):
        raise ValueError('Prepared correction file is missing exact bind metadata. Re-import the original GLB/glTF/FBX and prepare again.')
    try:
        metadata = json.loads(raw)
    except (TypeError, ValueError) as ex:
        raise ValueError('Prepared correction exact bind metadata is invalid.') from ex
    if metadata.get('policy') != SCALE_POLICY or metadata.get('display_policy') != DISPLAY_POLICY:
        raise ValueError('Prepared correction bind policy is incompatible; prepare from the original source again.')
    canonical = canonical_bones(target)
    if (metadata.get('canonical_sha256') != canonical_hash(metadata.get('bones', []))
            or metadata.get('bones') != canonical):
        raise ValueError('Prepared correction canonical bind data differs from the selected target.')
    if _display_snapshot(rig) != metadata.get('display_bones'):
        raise ValueError('Prepared correction display hierarchy/rest bones changed. Restore the saved rest skeleton or prepare from the original source.')
    if [list(row) for row in rig.matrix_world] != metadata.get('object_matrix'):
        raise ValueError('Prepared correction armature object transform changed. Restore it before correction export.')
    if any(p.constraints for p in rig.pose.bones):
        raise ValueError('Prepared correction has pose constraints. Remove them and restore the saved rest pose before correction export.')
    if (rig.animation_data and (rig.animation_data.action or rig.animation_data.nla_tracks or rig.animation_data.drivers)):
        raise ValueError('Prepared correction has pose animation/drivers. Remove them and restore the saved rest pose before correction export.')
    if any(not np.array_equal(np.asarray(p.matrix_basis), np.eye(4)) for p in rig.pose.bones):
        raise ValueError('Prepared correction is posed. Restore the saved rest pose before exporting mesh/weight corrections.')
    return {b['name']: np.asarray(b['matrix'], dtype=float) for b in canonical}, {
        'policy': SCALE_POLICY, 'canonical_sha256': canonical_hash(canonical),
        'source': 'verified-prepared-blend-exact-bind-metadata', 'hierarchy_and_display_rest_unchanged': True}
