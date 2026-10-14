"""Spring settings a source rig already has, as starting values for the moving
parts: VRM spring bones (from the VRM add-on's data in a .blend, or straight
from a .vrm/.glb file) and MMD rigid bodies (damping and the springs of their
joints). Each bone gets {'source', 'goal', 'drag', 'gravity'} on the builder's
scale: goal is the pull back to the pose (0-1), drag the share of speed lost
each frame (0-1), gravity a multiple of normal gravity. The conversions are
approximate; the Physics tab offers them only when the user asks for them.
"""
import json
import struct
from pathlib import Path


def _clamp(value, low, high):
    return max(low, min(high, float(value)))


def vrm_values(stiffness, drag_force, gravity_power):
    """VRM spring bone: stiffness 0-4 (1 is usual) pulls back to the pose,
    dragForce 0-1 is the speed lost each frame, gravityPower 0-2 the gravity."""
    return {'source': 'vrm', 'goal': round(_clamp(0.45 * float(stiffness), 0.02, 0.95), 4),
            'drag': round(_clamp(0.5 * float(drag_force), 0.0, 0.5), 4),
            'gravity': round(_clamp(gravity_power, 0.0, 1.5), 4)}


def mmd_values(angular_damping, spring):
    """MMD rigid body (Bullet): angular damping is the share of spin lost per
    second (0.99 is usual), so per frame at 30 fps it is 1 - (1 - d)^(1/30). The
    rotation springs of its joints range from 0 (most hair) to 100,000 (a locked
    joint); the pull back to the pose follows their order of magnitude. Dynamic
    MMD bodies always fall under gravity."""
    import math
    damping = _clamp(angular_damping, 0.0, 0.999)
    return {'source': 'mmd', 'goal': round(_clamp(0.05 + 0.15 * math.log10(1.0 + max(float(spring), 0.0)), 0.05, 0.9), 4),
            'drag': round(1.0 - (1.0 - damping) ** (1.0 / 30.0), 4), 'gravity': 1.0}


def _glb_json(path):
    data = Path(path).read_bytes()
    if data[:4] == b'glTF':
        length, kind = struct.unpack_from('<II', data, 12)
        if kind != 0x4E4F534A:
            return None
        return json.loads(data[20:20 + length].decode('utf-8'))
    if Path(path).suffix.lower() == '.gltf':
        return json.loads(data.decode('utf-8'))
    return None


def gltf_springs(path):
    """VRM 1.0 (VRMC_springBone) and VRM 0.x (VRM.secondaryAnimation) springs of
    a .vrm/.glb/.gltf file, by node name (Blender names bones after nodes)."""
    try:
        doc = _glb_json(path)
    except (OSError, ValueError, struct.error):
        return {}
    if not doc:
        return {}
    nodes = doc.get('nodes', [])
    name = lambda i: nodes[i].get('name', f'node_{i}') if isinstance(i, int) and 0 <= i < len(nodes) else None
    out = {}
    extensions = doc.get('extensions', {})
    for spring in extensions.get('VRMC_springBone', {}).get('springs', []):
        for joint in spring.get('joints', []):
            bone = name(joint.get('node'))
            if bone:
                out[bone] = vrm_values(joint.get('stiffness', 1.0), joint.get('dragForce', 0.5), joint.get('gravityPower', 0.0))
    for group in extensions.get('VRM', {}).get('secondaryAnimation', {}).get('boneGroups', []):
        values = vrm_values(group.get('stiffiness', group.get('stiffness', 1.0)), group.get('dragForce', 0.4), group.get('gravityPower', 0.0))
        stack = [i for i in group.get('bones', []) if isinstance(i, int)]
        while stack:  # a VRM 0.x group's settings apply to its bones' whole chains
            index = stack.pop()
            bone = name(index)
            if bone and bone not in out:
                out[bone] = values
                stack.extend(c for c in nodes[index].get('children', []) if isinstance(c, int))
    return out


def _idprops(value, *keys):
    for key in keys:
        try:
            value = value[key]
        except (KeyError, IndexError, TypeError):
            return None
    return value


def blend_vrm_springs(rig):
    """The VRM add-on's spring bones saved in a .blend (read as stored data, so
    the add-on does not need to be installed)."""
    out = {}
    extension = rig.data.get('vrm_addon_extension') if hasattr(rig.data, 'get') else None
    if extension is None:
        return out
    for spring in _idprops(extension, 'spring_bone1', 'springs') or []:
        for joint in _idprops(spring, 'joints') or []:
            bone = _idprops(joint, 'node', 'bone_name')
            if bone:
                out[str(bone)] = vrm_values(joint.get('stiffness', 1.0), joint.get('drag_force', 0.5), joint.get('gravity_power', 0.0))
    for group in _idprops(extension, 'vrm0', 'secondary_animation', 'bone_groups') or []:
        values = vrm_values(group.get('stiffiness', group.get('stiffness', 1.0)), group.get('drag_force', 0.4), group.get('gravity_power', 0.0))
        for item in _idprops(group, 'bones') or []:
            root = _idprops(item, 'bone_name')
            bone = rig.data.bones.get(str(root)) if root else None
            for b in ([bone] + list(bone.children_recursive)) if bone else []:
                out.setdefault(b.name, values)
    return out


def mmd_springs(objects, rig):
    """MMD rigid bodies that move a bone of `rig`: their angular damping and the
    rotation springs of the joints holding them."""
    from source_helpers import _classify
    _, bindings = _classify(objects)
    springs = {}
    for obj in objects:
        constraint = getattr(obj, 'rigid_body_constraint', None)
        if not constraint or constraint.type != 'GENERIC_SPRING':
            continue
        stiffness = [getattr(constraint, f'spring_stiffness_ang_{axis}', 0.0) for axis in 'xyz'
                     if getattr(constraint, f'use_spring_ang_{axis}', False)]
        value = sum(stiffness) / 3 if stiffness else 0.0
        # An MMD joint's spring turns its second body against the first (the
        # child against its parent), so it belongs to the second body only.
        if constraint.object2 is not None:
            springs.setdefault(constraint.object2.name, []).append(value)
    out = {}
    for obj, (bound_rig, bone) in bindings.items():
        if bound_rig != rig or not bone or obj.rigid_body is None:
            continue
        if obj.rigid_body.type != 'ACTIVE' or obj.rigid_body.kinematic:
            continue
        values = springs.get(obj.name, [])
        out[bone] = mmd_values(obj.rigid_body.angular_damping, sum(values) / len(values) if values else 0.0)
    return out


def source_springs(objects, rig, source_path=None):
    """Spring starting values of `rig`'s bones: VRM first, then MMD."""
    out = {}
    if rig is not None:
        out.update(blend_vrm_springs(rig))
        if source_path and Path(source_path).suffix.lower() in {'.vrm', '.glb', '.gltf'}:
            for bone, values in gltf_springs(source_path).items():
                out.setdefault(bone, values)
        for bone, values in mmd_springs(objects, rig).items():
            out.setdefault(bone, values)
    return out
