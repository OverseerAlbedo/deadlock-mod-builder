"""Anatomical rest alignment for optional humanoid preparation.

Bone matrices are coordinate frames, not anatomical directions. Foreign rolls and
Valve authoring axes must never be baked into the replacement's rest geometry.
Use reviewed joint positions for alignment; leave the target bind matrices intact.
"""
import numpy as np
from leg_chains import equivalents, POLICY as LEG_CHAINS_POLICY
from arm_helpers import detect as arm_matches, POLICY as ARM_POLICY

POLICY = 'anatomical-rest-alignment-v1'
PROPORTIONS_POLICY = 'source-proportions-v1'
ARM_FIT_POLICY = 'joint-centered-arm-fit-v1'
FIT_MODES = {'match_target', 'preserve_proportions'}
NEXT = {'pelvis': 'spine', 'spine': 'chest', 'chest': 'neck', 'neck': 'head',
        **{a+'_'+s: b+'_'+s for s in ('L', 'R') for a, b in
           (('upperarm', 'forearm'), ('forearm', 'hand'), ('thigh', 'shin'), ('shin', 'foot'))}}


def rigid_attachment_map(transform, pivot):
    """Move a retained decoration with a fitted limb without stretching its rig.

    Its attachment point follows the full limb fit, while internal distances
    and fabric shape retain the source's rigid mapping.
    """
    matrix = np.asarray(transform, dtype=float)
    if np.allclose(matrix[:3,:3].T@matrix[:3,:3], np.eye(3), atol=1e-10, rtol=0):
        return matrix
    u, _, vh = np.linalg.svd(matrix[:3,:3])
    rotation = u@vh
    if np.linalg.det(rotation) <= 0:
        raise ValueError('Cannot fit a mirrored secondary attachment.')
    point = np.asarray(pivot, dtype=float)
    result = np.eye(4); result[:3,:3] = rotation
    result[:3,3] = (matrix@np.r_[point,1.])[:3]-rotation@point
    return result


def unit(value, label):
    value = np.asarray(value, dtype=float)
    length = np.linalg.norm(value)
    if not np.isfinite(value).all() or length < 1e-7:
        raise ValueError('Cannot align humanoid: degenerate ' + label + '. Review the joint landmarks.')
    return value / length


def anatomical_frames(positions, roles):
    p = {role: np.asarray(positions[name], dtype=float) for role, name in roles.items()}
    up = unit(p['head'] - p['pelvis'], 'body height')
    lateral = p['thigh_L'] - p['thigh_R']
    lateral = unit(lateral - up * np.dot(lateral, up), 'left/right hips')
    forward = unit(np.cross(lateral, up), 'body facing')
    frames = {}
    for role, origin in p.items():
        if role in NEXT:
            direction = p[NEXT[role]] - origin
            reference = forward
        elif role.startswith('hand_'):
            direction = origin - p['forearm_' + role[-1]]
            reference = forward
        elif role.startswith('foot_'):
            direction, reference = forward, up
        else:  # head: upright relative to the reviewed neck
            direction, reference = origin - p['neck'], forward
        y = unit(direction, role + ' segment')
        z = reference - y * np.dot(reference, y)
        # A limb aimed straight forward has no stable roll from these landmarks.
        # Stop for review instead of inventing a discontinuous local frame.
        if np.linalg.norm(z) < .05:
            raise ValueError('Cannot align humanoid role ' + role +
                             ': pose is ambiguous along the facing axis. Use a neutral T/A pose.')
        z = unit(z, role + ' facing'); x = unit(np.cross(y, z), role + ' lateral')
        frame = np.eye(4); frame[:3, :3] = np.column_stack((x, y, z)); frame[:3, 3] = origin
        frames[role] = frame
    return frames


def body_basis(positions, roles):
    """A rigid body frame from reviewed landmarks, independent of bone roll."""
    p = {role: np.asarray(positions[name], dtype=float) for role, name in roles.items()}
    up = unit(p['head'] - p['pelvis'], 'body height')
    lateral = p['thigh_L'] - p['thigh_R']
    lateral = unit(lateral - up * np.dot(lateral, up), 'left/right hips')
    return np.column_stack((lateral, up, unit(np.cross(lateral, up), 'body facing')))


def proportion_maps(source, target, mapping, source_roles, target_roles, source_parents,
                    follow_anchors, sf, tf, parallel, matched, fit_arm_joints):
    """Retain source torso/legs, with optional joint-centered arm fitting.

    Torso, neck and head share one rigid transform. Each limb starts at its
    source shoulder/hip and initially uses source segment lengths with target
    directions. Optional arm fitting then reconciles those animation pivots. Secondary geometry inherits its reviewed anchor's entire map;
    target finger/helper pivots must not independently pull the source mesh.
    Target bind matrices are read-only. The final vertical translation aligns
    the lowest reviewed ankle, rather than stretching legs to reach the floor.
    """
    sp = {name: np.asarray(matrix)[:3, 3] for name, matrix in source.items()}
    tp = {name: np.asarray(matrix)[:3, 3] for name, matrix in target.items()}
    body = np.eye(4)
    body[:3, :3] = body_basis(tp, target_roles) @ body_basis(sp, source_roles).T
    body[:3, 3] = tp[target_roles['pelvis']] - body[:3, :3] @ sp[source_roles['pelvis']]
    role_maps = {role: body.copy() for role in source_roles}
    for side in ('L', 'R'):
        for chain in (('upperarm', 'forearm', 'hand'), ('thigh', 'shin', 'foot')):
            parent_map = body
            for part in chain:
                role = part + '_' + side
                origin = sp[source_roles[role]]
                pivot = (parent_map @ np.r_[origin, 1.])[:3]
                transform = np.eye(4)
                transform[:3, :3] = tf[role][:3, :3] @ sf[role][:3, :3].T
                transform[:3, 3] = pivot - transform[:3, :3] @ origin
                role_maps[role] = transform
                parent_map = transform
    floor = min(tp[target_roles['foot_' + side]][2] for side in ('L', 'R'))
    fitted_floor = min((role_maps['foot_' + side] @ np.r_[sp[source_roles['foot_' + side]], 1.])[2]
                       for side in ('L', 'R'))
    for transform in role_maps.values():
        transform[2, 3] += floor - fitted_floor
    body[2, 3] += floor - fitted_floor
    arm_fit = {'policy': ARM_FIT_POLICY, 'enabled': fit_arm_joints, 'joints': [],
               'width_preserved': True, 'target_bind_matrices_modified': False}
    shoulder_maps = {}
    if fit_arm_joints:
        # Source-length arms rotate around displaced hero pivots in game. Fit
        # the reviewed joint chain, scaling only along each segment. Keep the
        # source cross-section, torso, head, legs and cloth anchors unchanged.
        for side in ('L', 'R'):
            for part in ('upperarm', 'forearm', 'hand'):
                role = part+'_'+side
                origin = sp[source_roles[role]]
                before = (role_maps[role] @ np.r_[origin,1.])[:3]
                destination = tp[target_roles[role]]
                ratio = 1.
                if part != 'hand':
                    next_role = NEXT[role]
                    ratio = (np.linalg.norm(tp[target_roles[next_role]]-destination) /
                             np.linalg.norm(sp[source_roles[next_role]]-origin))
                transform = np.eye(4)
                transform[:3,:3] = tf[role][:3,:3] @ np.diag([1.,ratio,1.]) @ sf[role][:3,:3].T
                transform[:3,3] = destination-transform[:3,:3]@origin
                role_maps[role] = transform
                arm_fit['joints'].append({'role': role, 'before_offset_mm': float(np.linalg.norm(before-destination)*25.4),
                    'after_offset_mm': float(np.linalg.norm((transform@np.r_[origin,1.])[:3]-destination)*25.4),
                    'length_scale': float(ratio)})
            # Blend the shoulder into the retained torso. Hold the source
            # clavicle root at its existing fitted location, and move its arm
            # endpoint to the hero shoulder; never copy a foreign bone roll.
            for name, helper in matched.items():
                if helper['kind'] != 'shoulder' or name not in sp:
                    continue
                endpoint = source_roles['upperarm_'+side]
                seen, at = set(), endpoint
                while at is not None and at != name and at not in seen:
                    seen.add(at); at = source_parents.get(at)
                if at != name:
                    continue
                axis = sp[endpoint]-sp[name]
                length2 = float(axis@axis)
                if length2 < 1e-12:
                    continue
                delta = tp[target_roles['upperarm_'+side]]-(body@np.r_[sp[endpoint],1.])[:3]
                transform = body.copy()
                transform[:3,:3] += np.outer(delta,axis)/length2
                transform[:3,3] -= (transform[:3,:3]-body[:3,:3])@sp[name]
                if np.linalg.det(transform[:3,:3]) <= 1e-8:
                    raise ValueError('Arm fitting folds the shoulder bridge. Review shoulder landmarks or disable Align arms to hero joints.')
                shoulder_maps[name] = transform
    source_role = {name: role for role, name in source_roles.items()}
    source_role.update({name: source_role[primary] for name, primary in parallel.items()})
    result, inherited = {}, []
    for name, dest in mapping.items():
        if name not in source or dest not in target:
            continue
        anchor, seen = follow_anchors.get(name, name), set()
        while anchor not in source_role and anchor not in shoulder_maps:
            if anchor is None or anchor in seen:
                if np.allclose(np.asarray(source[name]), np.asarray(target[dest]), rtol=0, atol=1e-6):
                    anchor = None  # Already at its target bind (re-exported correction).
                    break
                raise ValueError('Mapped bone ' + name + ' has no reviewed humanoid ancestor. Review its mapping.')
            seen.add(anchor)
            anchor = source_parents.get(anchor)
        if anchor is None:
            result[name] = np.eye(4)
            continue
        role = source_role.get(anchor, 'chest')
        result[name] = (shoulder_maps[anchor] if anchor in shoulder_maps else role_maps[role]).copy()
        if anchor != name:
            inherited.append({'bone': name, 'target': dest, 'rotation_from_role': role})
    offsets = []
    for role, name in source_roles.items():
        fitted = (role_maps[role] @ np.r_[sp[name], 1.])[:3]
        actual = tp[target_roles[role]]
        offsets.append({'role': role, 'fitted_position': fitted.tolist(),
                        'target_position': actual.tolist(), 'distance_mm': float(np.linalg.norm(fitted - actual) * 25.4)})
    return result, {'policy': PROPORTIONS_POLICY, 'fit_mode': 'preserve_proportions',
                    'parallel_leg_policy': LEG_CHAINS_POLICY, 'parallel_leg_joints': parallel,
                    'following_anchors': dict(follow_anchors),
                    'foreign_bone_axes_used_for_geometry': False, 'target_bind_matrices_modified': False,
                    'body_alignment': body.tolist(), 'source_limb_lengths_preserved': not fit_arm_joints,
                    'source_leg_lengths_preserved': True, 'arm_joint_fit': arm_fit,
                    'joint_offsets': offsets, 'max_joint_offset_mm': max(x['distance_mm'] for x in offsets),
                    'inherited_secondary_rotations': inherited}


def rest_maps(source, target, mapping, source_roles, target_roles, source_parents, follow_anchors=None, anchor_mapping=None, fit_mode='match_target', source_bones=None, target_bones=None, fit_arm_joints=True):
    """Return rigid anatomical transforms; never copy foreign bone roll.

    Non-primary weighted bones inherit the closest reviewed ancestor's rotation
    and retain their own mapped joint pivot. Unanchored bones are rejected.
    """
    if not isinstance(fit_mode, str) or fit_mode not in FIT_MODES:
        raise ValueError('Choose a supported Model fitting option before conversion.')
    follow_anchors = follow_anchors or {}
    anchor_mapping = mapping if anchor_mapping is None else anchor_mapping
    # Production supplies deform flags from inspection. Non-deforming control
    # rigs must not become anatomical anchors for otherwise unrelated weights.
    parallel = equivalents(source_bones if source_bones is not None else
                           [{'name': n, 'position': np.asarray(m)[:3, 3],
                             'parent': source_parents.get(n)} for n, m in source.items()], source_roles)
    for name, primary in parallel.items():
        if name in mapping and (anchor_mapping.get(name) != anchor_mapping.get(primary)
                                or name in follow_anchors):
            raise ValueError('Parallel leg joint ' + name + ' must use the same joint as ' + primary +
                             '. Re-detect body mapping before preparation.')
    role_bones = set(source_roles.values())
    def role_orphan(name):
        seen, parent = {name}, source_parents.get(name)
        while parent is not None and parent not in seen:
            if parent in role_bones:
                return False
            seen.add(parent); parent = source_parents.get(parent)
        return True
    for name, anchor in follow_anchors.items():
        if name == anchor or name not in source or anchor not in source or anchor in follow_anchors or name in source_roles.values():
            raise ValueError('Invalid body-following anchor for ' + name)
        # A bone with no body joint above it (root-level prop/accessory bones)
        # may follow a reviewed body joint directly, keeping its rest offset.
        if anchor in role_bones and role_orphan(name):
            if name not in anchor_mapping or anchor_mapping.get(name) != anchor_mapping.get(anchor):
                raise ValueError('Body-following target no longer matches its anchor: ' + name)
            continue
        seen, parent = {name}, source_parents.get(name)
        while parent != anchor:
            if parent is None or parent in seen:
                raise ValueError('Body-following anchor is not an ancestor: ' + name)
            seen.add(parent); parent = source_parents.get(parent)
        if name not in anchor_mapping or anchor_mapping.get(name) != anchor_mapping.get(anchor):
            raise ValueError('Body-following target no longer matches its anchor: ' + name)
    sf = anatomical_frames({n: np.asarray(m)[:3, 3] for n, m in source.items()}, source_roles)
    tf = anatomical_frames({n: np.asarray(m)[:3, 3] for n, m in target.items()}, target_roles)
    matched = arm_matches(source_bones or [], target_bones or [], source_roles, target_roles)
    matched = {n: m for n, m in matched.items() if anchor_mapping.get(n) == m['target'] and n not in follow_anchors}
    if fit_mode == 'preserve_proportions':
        result, audit = proportion_maps(source, target, mapping, source_roles, target_roles, source_parents,
                               follow_anchors, sf, tf, parallel, matched, fit_arm_joints)
        audit.update(arm_deformation_policy=ARM_POLICY, arm_deformation=matched)
        return result, audit
    source_role = {name: role for role, name in source_roles.items()}
    source_role.update({name: source_role[primary] for name, primary in parallel.items()})
    rotations = {r: tf[r][:3, :3] @ sf[r][:3, :3].T for r in sf}
    # Arm helper weights can use existing hero twist joints while geometry
    # retains the whole segment's rest transform. Snapping each helper to its
    # new twist pivot would pinch skin whenever proportions differ.
    result, inherited = {}, []
    for name, dest in mapping.items():
        if name not in source or dest not in target:
            continue  # Existing weight validator reports invalid weighted mappings.
        anchor, seen = follow_anchors.get(name, name), set()
        while anchor not in source_role:
            if anchor is None or anchor in seen:
                if np.allclose(np.asarray(source[name]), np.asarray(target[dest]), rtol=0, atol=1e-6):
                    anchor = None  # Already at its target bind (re-exported correction).
                    break
                raise ValueError('Mapped bone ' + name + ' has no reviewed humanoid ancestor. Review its mapping.')
            seen.add(anchor); anchor = source_parents.get(anchor)
        if anchor is None:
            result[name] = np.eye(4)
            continue
        role = source_role[anchor]
        rotation = rotations[role]
        matrix = np.eye(4); matrix[:3, :3] = rotation
        # A many-to-one helper shares the parent's rest transform. Its own bind
        # offset must not be subtracted: doing so collapses the weighted mesh
        # toward the shared joint. Direct joint matches retain the old behavior.
        geometry_anchor = follow_anchors.get(name, parallel.get(name, name))
        geometry_dest = dest
        helper = matched.get(geometry_anchor)
        if helper and helper['kind'] == 'twist':
            geometry_anchor = source_roles[helper['role']]
            geometry_dest = target_roles[helper['role']]
        matrix[:3, 3] = np.asarray(target[geometry_dest])[:3, 3] - rotation @ np.asarray(source[geometry_anchor])[:3, 3]
        result[name] = matrix
        if anchor != name: inherited.append({'bone': name, 'target': dest, 'rotation_from_role': role})
    return result, {'policy': 'ancestor-offset-preservation-v1' if follow_anchors else POLICY,
                    'arm_deformation_policy': ARM_POLICY, 'arm_deformation': matched,
                    'parallel_leg_policy': LEG_CHAINS_POLICY, 'parallel_leg_joints': parallel,
                    'following_anchors': dict(follow_anchors), 'foreign_bone_axes_used_for_geometry': False,
                    'target_bind_matrices_modified': False, 'inherited_secondary_rotations': inherited}
