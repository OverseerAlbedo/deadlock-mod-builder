"""Audit serialized model-22 current/bind transforms against original DATA."""
import math
import struct
from ..safety import BuildError
from .target_skeleton import audit_scale_preservation


def audit_dmx_bind(dmx, reference, require_explicit_scale=True):
    """Validate actual DMX p/q/s, hierarchy and palette, without reference copies.

    Current and bind lists may share transform elements. Missing scalar scale is
    interpreted as unit only for an explicitly requested legacy audit.
    """
    elements = dmx.elements

    def element(index, kinds):
        if type(index) is not int or not 0 <= index < len(elements) or elements[index].type not in kinds:
            raise BuildError('Prepared DMX has a missing or invalid bind element reference.')
        return elements[index]

    def attributes(elem):
        result = {attr.name: attr for attr in elem.attributes}
        if len(result) != len(elem.attributes):
            raise BuildError('Prepared DMX has duplicate transform attributes: ' + elem.name)
        return result

    def transform(index):
        elem = element(index, ('DmeTransform',))
        attrs = attributes(elem)
        result = []
        for name, kind, count in (('position', 10, 3), ('orientation', 13, 4)):
            attr = attrs.get(name)
            if attr is None or attr.type != kind or attr.array:
                raise BuildError(f'Prepared DMX requires a typed {name}: {elem.name}')
            try:
                value = list(struct.unpack('<' + 'f' * count, attr.raw))
            except (TypeError, struct.error) as ex:
                raise BuildError('Prepared DMX has invalid transform bytes: ' + elem.name) from ex
            if not all(math.isfinite(v) for v in value):
                raise BuildError('Prepared DMX has non-finite transform values: ' + elem.name)
            result.append(value)
        attr = attrs.get('scale')
        if attr is None:
            if require_explicit_scale:
                raise BuildError('Prepared DMX is missing explicit local scale: ' + elem.name)
            scale = 1.0
        else:
            if attr.type != 3 or attr.array:
                raise BuildError('Prepared DMX requires scalar float32 scale: ' + elem.name)
            try:
                scale = struct.unpack('<f', attr.raw)[0]
            except (TypeError, struct.error) as ex:
                raise BuildError('Prepared DMX has invalid scale bytes: ' + elem.name) from ex
            if not math.isfinite(scale) or scale <= 0:
                raise BuildError('Prepared DMX requires positive finite scale: ' + elem.name)
        return result[0], result[1], scale

    def identity_transform(index):
        pos, quat, scale = transform(index)
        if pos != [0., 0., 0.] or quat not in ([0., 0., 0., 1.], [0., 0., 0., -1.]) or scale != 1.:
            raise BuildError('Prepared DMX model/mesh transform must be identity for canonical binds.')

    models = [e for e in elements if e.type == 'DmeModel' and e.get('jointList')]
    if len(models) != 1:
        raise BuildError('Prepared DMX requires one unambiguous model joint palette.')
    model = models[0]
    model_index = next(i for i, e in enumerate(elements) if e is model)
    root = element(0, ('DmElement',))
    root_attrs = attributes(root)
    if any(root_attrs.get(name) is None or root_attrs[name].type != 1 or
           root_attrs[name].array or root_attrs[name].value != model_index for name in ('model', 'skeleton')):
        raise BuildError('Prepared DMX root model/skeleton does not reference its validated model.')
    attributes(model)
    identity_transform(model.get('transform'))
    palette = list(model.get('jointList'))
    if len(palette) != len(set(palette)):
        raise BuildError('Prepared DMX joint palette contains duplicate elements.')
    joints = {i: e for i, e in enumerate(elements) if e.type == 'DmeJoint'}
    joint_indices = [i for i in palette if i in joints]
    names = [joints[i].name for i in joint_indices]
    if (set(joint_indices) != set(joints) or len(names) != len(set(names)) or
            set(names) != set(reference['m_boneName']) or len({e.uuid for e in joints.values()}) != len(joints)):
        raise BuildError('Prepared DMX joint names/identities differ from the original skeleton.')
    binds = [element(i, ('DmeTransformList', 'DmeTransformsList'))
             for i in model.get('baseStates', [])]
    binds = [e for e in binds if e.name == 'bind']
    if len(binds) != 1:
        raise BuildError('Prepared DMX requires one explicit bind transform list.')
    bind_refs = list(binds[0].get('transforms', []))
    if len(bind_refs) != len(palette):
        raise BuildError('Prepared DMX bind transform list differs from its joint palette.')
    parents = {}
    for i, joint in joints.items():
        for child in joint.get('children', []):
            if child in joints:
                if child in parents:
                    raise BuildError('Prepared DMX joint has duplicate/multiple parent references.')
                parents[child] = i
    roots = {i for i in model.get('children', []) if i in joints}
    if roots != set(joints) - set(parents):
        raise BuildError('Prepared DMX root joint references disagree with its hierarchy.')
    # An identity local mesh transform is insufficient if the mesh was moved
    # below a transformed joint or wrapper DAG. The canonical prepared schema
    # has one owner per joint and direct model children for all palette meshes.
    model_children = list(model.get('children', []))
    palette_meshes = set(palette) - set(joints)
    if len(model_children) != len(set(model_children)) or set(model_children) != roots | palette_meshes:
        raise BuildError('Prepared DMX mesh DAGs must be direct children of the identity model.')
    for joint in joints.values():
        children = list(joint.get('children', []))
        if len(children) != len(set(children)) or any(child not in joints for child in children):
            raise BuildError('Prepared DMX joint children must contain only directly parented joints.')
    allowed_owners = {model_index, *joints}
    for i, elem in enumerate(elements):
        if i not in allowed_owners and any(child in palette for child in elem.get('children', [])):
            raise BuildError('Prepared DMX palette element has an unsupported/shared graph parent.')
    actual = {'m_boneName': names, 'm_nParent': [], 'm_bonePosParent': [],
              'm_boneRotParent': [], 'm_boneScaleParent': []}
    observed = {}
    for i, bind_ref in zip(palette, bind_refs):
        dag = element(i, ('DmeJoint', 'DmeDag'))
        current = transform(dag.get('transform'))
        bind = transform(bind_ref)
        if current != bind:
            raise BuildError('Prepared DMX current and bind transforms differ: ' + dag.name)
        if i not in joints:
            identity_transform(dag.get('transform'))
        else:
            observed[i] = current
    for i in joint_indices:
        parent = parents.get(i)
        actual['m_nParent'].append(joint_indices.index(parent) if parent is not None else -1)
        pos, quat, scale = observed[i]
        actual['m_bonePosParent'].append(pos)
        actual['m_boneRotParent'].append(quat)
        actual['m_boneScaleParent'].append(scale)
    from ..bone_validation import skeleton_identity
    identity = skeleton_identity(actual, reference)
    scale_audit = audit_scale_preservation(actual, reference, require_exact_float32_scale=True)
    return {'bind_identity': identity, 'target_scale_audit': scale_audit,
            'current_and_bind_equal': True, 'joint_palette_entries': len(palette),
            'world_inverse_bind_consistent': True, 'serialized_local_transforms_checked': True}
