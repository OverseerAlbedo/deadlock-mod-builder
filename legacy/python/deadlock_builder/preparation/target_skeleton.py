"""Source 2 local T*R*S binds, preserved through the model-22 DMX adapter.

Authored uniform bone scale is part of the bind transform, not an import error.
Preservation bounds compare observed exports against the original skeleton;
deviation from unit scale is diagnostic and never an approximation allowance.
"""
import math
import struct
import numpy as np
from ..safety import BuildError

SCALE_POLICY = 'source2-local-scale-dmx-v1'
LOCAL_SCALE_TOLERANCE = 1e-4
WORLD_SCALE_TOLERANCE = 1e-3
WORLD_POSITION_TOLERANCE_MM = 0.5


def _float32(value):
    try:
        return struct.unpack('<f', struct.pack('<f', value))[0]
    except (OverflowError, struct.error) as ex:
        raise BuildError('Target bind value cannot be represented as a DMX float32.') from ex


def _bind_state(skeleton):
    fields = ('m_boneName', 'm_nParent', 'm_bonePosParent',
              'm_boneRotParent', 'm_boneScaleParent')
    if any(field not in skeleton for field in fields):
        raise BuildError('Target hero skeleton is missing bind transform fields.')
    names = skeleton['m_boneName']
    if not names or any(len(skeleton[field]) != len(names) for field in fields):
        raise BuildError('Target hero skeleton has inconsistent bone arrays.')
    if any(not isinstance(name, str) or not name for name in names) or len(set(names)) != len(names):
        raise BuildError('Target hero skeleton has invalid or duplicate bone names.')
    parents = list(skeleton['m_nParent'])
    scales, positions, rotations = [], [], []
    for i, name in enumerate(names):
        if type(parents[i]) is not int or not -1 <= parents[i] < len(names):
            raise BuildError('Invalid target hero parent index: ' + name)
        try:
            value = skeleton['m_boneScaleParent'][i]
            if isinstance(value, (bool, str, list, tuple, dict)):
                raise ValueError('scale must be a number')
            scale = float(value)
            pos = np.asarray(skeleton['m_bonePosParent'][i], dtype=float)
            quat = np.asarray(skeleton['m_boneRotParent'][i], dtype=float)
        except (TypeError, ValueError) as ex:
            raise BuildError('Invalid target bind transform: ' + name) from ex
        if (not math.isfinite(scale) or scale <= 0 or not math.isfinite(_float32(scale))
                or _float32(scale) <= 0):
            raise BuildError(f'Target hero bone {name!r} requires a positive finite float32 scale.')
        if pos.shape != (3,) or quat.shape != (4,) or not np.isfinite(pos).all() or not np.isfinite(quat).all():
            raise BuildError('Invalid target bind transform shape or values: ' + name)
        if np.linalg.norm(quat) < .99:
            raise BuildError('Invalid target bind quaternion: ' + name)
        scales.append(scale); positions.append(pos); rotations.append(quat)
    from ..bone_validation import skeleton_identity
    skeleton_identity(skeleton, skeleton)
    world, rigid, inverse, accumulated, active = {}, {}, {}, {}, set()

    def matrix(i):
        if i in world:
            return world[i]
        if i in active:
            raise BuildError('Cyclic target skeleton.')
        active.add(i)
        x, y, z, w = rotations[i] / np.linalg.norm(rotations[i])
        rot = np.array([[1-2*y*y-2*z*z, 2*x*y-2*z*w, 2*x*z+2*y*w],
                        [2*x*y+2*z*w, 1-2*x*x-2*z*z, 2*y*z-2*x*w],
                        [2*x*z-2*y*w, 2*y*z+2*x*w, 1-2*x*x-2*y*y]])
        unscaled = np.eye(4); unscaled[:3, :3] = rot; unscaled[:3, 3] = positions[i]
        full = unscaled.copy(); full[:3, :3] *= scales[i]
        cumulative = scales[i]
        parent = parents[i]
        if parent >= 0:
            full = matrix(parent) @ full
            unscaled = rigid[parent] @ unscaled
            cumulative *= accumulated[parent]
        if not np.isfinite(full).all() or not math.isfinite(cumulative) or cumulative <= 0:
            raise BuildError('Target accumulated bind is not finite/invertible: ' + names[i])
        try:
            inv = np.linalg.inv(full)
        except np.linalg.LinAlgError as ex:
            raise BuildError('Target accumulated bind is not invertible: ' + names[i]) from ex
        if (not np.isfinite(inv).all() or
                not np.allclose(full @ inv, np.eye(4), rtol=0, atol=1e-7) or
                not np.allclose(inv @ full, np.eye(4), rtol=0, atol=1e-7)):
            raise BuildError('Target world/inverse bind consistency failed: ' + names[i])
        world[i], rigid[i], inverse[i], accumulated[i] = full, unscaled, inv, cumulative
        active.remove(i)
        return full

    for i in range(len(names)):
        matrix(i)
    return {'names': list(names), 'parents': parents, 'scales': scales,
            'positions': positions, 'rotations': rotations, 'world': world,
            'rigid': rigid, 'inverse': inverse, 'accumulated': accumulated}


def _compare_states(actual, reference, require_exact_float32_scale):
    lookup = {name: i for i, name in enumerate(actual['names'])}
    rows = []
    for i, name in enumerate(reference['names']):
        j = lookup[name]
        original, observed = reference['scales'][i], actual['scales'][j]
        if require_exact_float32_scale and observed != _float32(original):
            raise BuildError('DMX local scale differs from the original float32 value: ' + name)
        local_delta = abs(observed - original)
        world_delta = abs(actual['accumulated'][j] - reference['accumulated'][i])
        position_delta = float(np.linalg.norm(actual['world'][j][:3, 3] - reference['world'][i][:3, 3]) * 25.4)
        if (local_delta > LOCAL_SCALE_TOLERANCE or world_delta > WORLD_SCALE_TOLERANCE
                or position_delta > WORLD_POSITION_TOLERANCE_MM):
            raise BuildError(f'Target bind preservation budget exceeded at {name!r}: '
                             f'local scale difference {local_delta:.9g}; cumulative scale difference '
                             f'{world_delta:.9g}; world joint difference {position_delta:.9g} mm.')
        rows.append({'name': name, 'original_scale': original, 'export_scale': observed,
                     'local_scale_delta': local_delta, 'cumulative_scale': actual['accumulated'][j],
                     'original_cumulative_scale': reference['accumulated'][i],
                     'world_scale_delta': world_delta, 'world_joint_delta_mm': position_delta,
                     'omitted_scale_world_joint_drift_mm': float(np.linalg.norm(
                         reference['world'][i][:3, 3] - reference['rigid'][i][:3, 3]) * 25.4)})
    return {'policy': SCALE_POLICY, 'bone_count': len(rows),
            'local_scale_tolerance': LOCAL_SCALE_TOLERANCE,
            'world_scale_tolerance': WORLD_SCALE_TOLERANCE,
            'world_joint_tolerance_mm': WORLD_POSITION_TOLERANCE_MM,
            'max_local_scale_delta': max(row['local_scale_delta'] for row in rows),
            'max_world_scale_delta': max(row['world_scale_delta'] for row in rows),
            'max_world_joint_delta_mm': max(row['world_joint_delta_mm'] for row in rows),
            'max_omitted_scale_world_joint_drift_mm': max(row['omitted_scale_world_joint_drift_mm'] for row in rows),
            'max_omitted_scale_world_scale_drift': max(abs(s - 1) for s in reference['accumulated'].values()),
            'approximated_bones': 0, 'scaled_bones': sum(s != 1 for s in reference['scales']),
            'bones': rows, 'original_skeleton_modified': False,
            'exact_scale_preservation': all(actual['scales'][lookup[name]] in (scale, _float32(scale))
                                          for name, scale in zip(reference['names'], reference['scales'])),
            'world_inverse_bind_consistent': True, 'native_compiler_validated': False}


def audit_scale_preservation(actual, reference, require_exact_float32_scale=False):
    """Compare local and accumulated binds; never substitute expected scales."""
    from ..bone_validation import skeleton_identity
    skeleton_identity(actual, reference)
    return _compare_states(_bind_state(actual), _bind_state(reference), require_exact_float32_scale)


def prepare_target(skeleton):
    state = _bind_state(skeleton)
    names, parents = state['names'], state['parents']
    bones = [{'name': name, 'parent': names[parents[i]] if parents[i] >= 0 else None,
              'local_position': state['positions'][i].tolist(),
              'local_rotation': state['rotations'][i].tolist(), 'local_scale': state['scales'][i],
              'position': state['world'][i][:3, 3].tolist(), 'matrix': state['world'][i].tolist(),
              'inverse_bind_matrix': state['inverse'][i].tolist(), 'length': 1.0}
             for i, name in enumerate(names)]
    return bones, _compare_states(state, state, False)


def scale_notice(audit):
    if not audit['scaled_bones']:
        return 'Target skeleton uses unit bone scales; DMX stores them explicitly.'
    return (f"Target scale compatibility: {audit['scaled_bones']} authored bone scales preserved in local "
            f"DMX transforms and accumulated binds; maximum exported local scale difference "
            f"{audit['max_local_scale_delta']:.8g}, maximum world joint difference "
            f"{audit['max_world_joint_delta_mm']:.4f} mm (limit {audit['world_joint_tolerance_mm']:g} mm). "
            'Native compiler and in-game validation remain required.')
