"""Native Blender regressions; generated assets only, optional read-only user asset.

Run with Blender --background --factory-startup --disable-autoexec
--python-exit-code 23 --python scripts/test_blender_mmd_helpers.py.
DMB_MMD_SOURCE optionally supplies a real .blend; it is never saved or changed.
"""
import hashlib
import json
import os
from pathlib import Path
import struct
import sys

import bpy
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT), str(ROOT / 'scripts'),
               str(ROOT / 'src-tauri/crates/deadlock-preparation/resources/blender')]
from test_blender_preparation import fixture
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid
from worker import import_source, describe, prepare
from algorithms import suggest_mapping
from blend_source import snapshot_blend
from source_helpers import mmd_helpers, physics_bone_bindings, remove_workspace_helpers

OUT = Path(os.environ.get('DMB_PREP_TEST_OUTPUT', str(ROOT / 'build/mmd-helper-test')))
OUT.mkdir(parents=True, exist_ok=True)
results = []


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fingerprint(meshes, rigs):
    """Compare actual streams, including every shape-key coordinate and weight."""
    records = {}
    for obj in meshes:
        mesh = obj.data
        h = hashlib.sha256()

        def value(v):
            h.update(json.dumps(v, sort_keys=True, ensure_ascii=False).encode('utf-8'))
            h.update(b'\0')

        def array(items, prop, size, dtype=np.float32):
            data = np.empty(len(items) * size, dtype=dtype)
            items.foreach_get(prop, data)
            h.update(data.tobytes())

        value([obj.name, [list(row) for row in obj.matrix_world],
               obj.parent.name if obj.parent else None])
        array(mesh.vertices, 'co', 3)
        array(mesh.edges, 'vertices', 2, np.int32)
        array(mesh.loops, 'vertex_index', 1, np.int32)
        array(mesh.polygons, 'loop_total', 1, np.int32)
        array(mesh.polygons, 'material_index', 1, np.int32)
        array(mesh.corner_normals, 'vector', 3)
        for uv in mesh.uv_layers:
            value(uv.name)
            array(uv.data, 'uv', 2)
        for color in mesh.color_attributes:
            value([color.name, color.domain, color.data_type])
            array(color.data, 'color', 4)
        value([group.name for group in obj.vertex_groups])
        for vertex in mesh.vertices:
            h.update(struct.pack('<I', len(vertex.groups)))
            for group in vertex.groups:
                h.update(struct.pack('<If', group.group, group.weight))
        if mesh.shape_keys:
            for key in mesh.shape_keys.key_blocks:
                value([key.name, key.value, key.relative_key.name, key.slider_min,
                       key.slider_max, key.vertex_group, key.mute])
                array(key.data, 'co', 3)
        for slot in obj.material_slots:
            mat = slot.material
            value([slot.link, mat.name if mat else None])
            if mat:
                value([list(mat.diffuse_color), mat.roughness, mat.metallic])
                if mat.node_tree:
                    value([(node.name, node.bl_idname) for node in mat.node_tree.nodes])
                    value([(link.from_node.name, link.from_socket.name,
                            link.to_node.name, link.to_socket.name) for link in mat.node_tree.links])
        records[obj.name] = h.hexdigest()
    for rig in rigs:
        bones = [[bone.name, bone.parent.name if bone.parent else None, bone.use_deform,
                  [list(row) for row in bone.matrix_local], bone.length]
                 for bone in rig.data.bones]
        records[rig.name] = hashlib.sha256(json.dumps(bones).encode()).hexdigest()
    return records


def empty(name, parent=None):
    obj = bpy.data.objects.new(name, None)
    bpy.context.collection.objects.link(obj)
    obj.parent = parent
    return obj


def sample(morph=True):
    body = fixture()[0]
    rig = body.parent
    root = empty('MMD character')
    root['mmd_tools_version'] = '4.5.14'
    rig.parent = root
    group = empty('rigidbodies', root)
    helpers = []
    for index in range(2):
        bpy.ops.mesh.primitive_cube_add(size=.03)
        obj = bpy.context.object
        obj.name = '000_下半身_' if index == 0 else '001_上半身_'
        obj.parent = group
        obj.hide_render = True
        bpy.ops.rigidbody.object_add()
        constraint = obj.constraints.new('CHILD_OF')
        constraint.name = 'mmd_tools_rigid_parent'
        constraint.target = rig
        constraint.subtarget = rig.data.bones[0].name
        constraint.mute = True
        helpers.append(obj)
    joints = empty('joints', root)
    joint = empty('MMD joint', joints)
    bpy.context.view_layer.objects.active = joint
    joint.select_set(True)
    bpy.ops.rigidbody.constraint_add()
    joint.rigid_body_constraint.object1, joint.rigid_body_constraint.object2 = helpers
    if morph:
        body.shape_key_add(name='Basis')
        shape = body.shape_key_add(name='Smile')
        shape.data[0].co.x += .01
    bpy.context.view_layer.update()
    return body, rig, helpers, root


# Intake and packed staging preserve all character streams, helpers and source.
body, rig, helpers, root = sample()
image = bpy.data.images.new('Packed material', width=2, height=2)
image.pixels = [.1, .2, .3, 1.] * 4
image.filepath_raw = str(OUT / 'texture.png')
image.file_format = 'PNG'
image.save()
image.pack()
material = body.data.materials[0]
material.use_nodes = True
node = material.node_tree.nodes.new('ShaderNodeTexImage')
node.image = image
material.node_tree.links.new(node.outputs['Color'], material.node_tree.nodes.get('Principled BSDF').inputs['Base Color'])
source = OUT / 'mmd-source.blend'
bpy.ops.wm.save_as_mainfile(filepath=str(source))
before_file = sha(source)
before = fingerprint([body], [rig])
receipts = []
meshes, rigs = import_source(str(source), allow_shape_keys=True, helper_reports=receipts)
assert len(meshes) == len(rigs) == 1
assert receipts[0]['rigid_body_meshes'] == 2 and receipts[0]['joint_empties'] == 1
assert fingerprint(meshes, rigs) == before
snapshot = OUT / 'staged.blend'
snapshot_blend(bpy, source, snapshot)
meshes, rigs = import_source(str(snapshot), allow_shape_keys=True)
assert fingerprint(meshes, rigs) == before
assert len(mmd_helpers(bpy.context.scene.objects)) == 5
assert sha(source) == before_file
results.append({'case': 'intake-and-staging-preserve-character-and-source', 'passed': True})

# A real mesh constraint, hidden rigid body, renamed group, wrong rig, active
# constraint or skinned helper cannot be silently excluded.
mutations = {
    'character-constraint': lambda body, rig, helpers, root: body.constraints.new('COPY_LOCATION'),
    'missing-mmd-root': lambda body, rig, helpers, root: root.__delitem__('mmd_tools_version'),
    'ordinary-rigidbody': lambda body, rig, helpers, root: setattr(helpers[0].parent, 'name', 'ordinary physics'),
    'visible-helper': lambda body, rig, helpers, root: setattr(helpers[0], 'hide_render', False),
    'active-constraint': lambda body, rig, helpers, root: setattr(helpers[0].constraints[0], 'mute', False),
    'skinned-helper': lambda body, rig, helpers, root: helpers[0].vertex_groups.new(name=rig.data.bones[0].name),
    'wrong-bone': lambda body, rig, helpers, root: setattr(helpers[0].constraints[0], 'subtarget', 'missing bone'),
}
for name, mutate in mutations.items():
    values = sample(morph=False)
    mutate(*values)
    path = OUT / (name + '.blend')
    bpy.ops.wm.save_as_mainfile(filepath=str(path))
    before_hash = sha(path)
    try:
        import_source(str(path))
    except ValueError as error:
        assert 'constraints/drivers on character mesh' in str(error), str(error)
    else:
        raise AssertionError('Unsafe source was accepted: ' + name)
    assert sha(path) == before_hash
    results.append({'case': name + '-remains-blocked', 'passed': True})

# These layouts are saved by different MMD Tools versions/build states. Body
# names deliberately carry no character-specific classification rules.
def tagged(values):
    body, rig, helpers, root = values
    root['mmd_type'] = 2
    root['mmd_root'] = {'name': 'Generated legacy MMD'}
    helpers[0].parent['mmd_type'] = 3
    for obj in helpers:
        obj['mmd_type'] = 23
        obj['mmd_rigid'] = {'shape': 2, 'bone': obj.constraints[0].subtarget}
        obj.rigid_body.collision_shape = 'BOX'


def variant(name):
    values = sample(morph=False)
    body, rig, helpers, root = values
    tagged(values)
    if name == 'legacy-metadata-and-renamed-group':
        del root['mmd_tools_version']
        helpers[0].parent.name = 'Collision volumes'
    elif name == 'live-kinematic-parent':
        helpers[0].rigid_body.kinematic = True
        helpers[0].constraints[0].mute = False
    elif name == 'built-bone-parent':
        helpers[0].rigid_body.kinematic = True
        helpers[0].parent = rig
        helpers[0].parent_type = 'BONE'
        helpers[0].parent_bone = helpers[0].constraints[0].subtarget
    elif name == 'unbound-collision':
        helpers[0].constraints[0].subtarget = ''
        helpers[0]['mmd_rigid']['bone'] = ''
    elif name == 'built-dynamic-tracker':
        obj = helpers[0]
        bone = obj.constraints[0].subtarget
        obj.constraints.clear()
        tracker = empty('arbitrary tracker name', obj)
        tracker['mmd_type'] = 51
        c = rig.pose.bones[bone].constraints.new('COPY_TRANSFORMS')
        c.name = 'mmd_tools_rigid_track'
        c.target = tracker
    elif name == 'legacy-spring-goal':
        bpy.ops.mesh.primitive_cube_add(size=.03)
        goal = bpy.context.object
        goal.parent = helpers[0]
        goal.hide_render = True
        goal['mmd_type'] = 54
        bpy.ops.rigidbody.object_add()
        goal.rigid_body.kinematic = True
        group = empty('Generated temporary helpers', root)
        group['mmd_type'] = 5
        joint = empty('Generated spring joint', group)
        joint['mmd_type'] = 53
        bpy.context.view_layer.objects.active = joint
        bpy.ops.rigidbody.constraint_add(type='GENERIC_SPRING')
        joint.rigid_body_constraint.object1 = helpers[1]
        joint.rigid_body_constraint.object2 = goal
    return values


for name in ['legacy-metadata-and-renamed-group', 'live-kinematic-parent',
             'built-bone-parent', 'unbound-collision', 'built-dynamic-tracker', 'legacy-spring-goal']:
    body, rig, helpers, root = variant(name)
    bpy.context.view_layer.update()
    before = fingerprint([body], [rig])
    path = OUT / (name + '.blend')
    bpy.ops.wm.save_as_mainfile(filepath=str(path))
    before_hash = sha(path)
    receipts = []
    meshes, rigs = import_source(str(path), helper_reports=receipts)
    assert len(meshes) == len(rigs) == 1
    assert receipts[0]['rigid_body_meshes'] == 2
    assert receipts[0]['policy'] == 'mmd-authoring-helpers-v2'
    assert fingerprint(meshes, rigs) == before
    bindings = physics_bone_bindings(bpy.context.scene.objects, rigs[0])
    assert sum(len(row['dynamic_bodies']) + len(row['kinematic_bodies'])
               for row in bindings.values()) == (1 if name == 'unbound-collision' else 2)
    if name == 'live-kinematic-parent':
        assert sum(len(row['kinematic_bodies']) for row in bindings.values()) == 1
    if name == 'legacy-spring-goal':
        assert receipts[0]['built_helpers'] == 2
    remove_workspace_helpers(bpy, mmd_helpers(bpy.context.scene.objects))
    assert len([obj for obj in bpy.context.scene.objects if obj.type == 'MESH']) == 1
    assert sha(path) == before_hash
    results.append({'case': name + '-intake-and-bindings', 'passed': True})

# Metadata never overrides evidence that an object carries character geometry
# or unrecognized controls. Include the reported naming pattern as a decoy.
def break_tracker(values):
    body, rig, helpers, root = values
    constraints = rig.pose.bones[helpers[0]['mmd_rigid']['bone']].constraints
    for constraint in list(constraints):
        constraints.remove(constraint)


extra_guards = {
    'typed-visible-body': lambda v: setattr(v[2][0], 'hide_render', False),
    'typed-skinned-body': lambda v: v[2][0].vertex_groups.new(name=v[1].data.bones[0].name),
    'typed-morphed-body': lambda v: v[2][0].shape_key_add(name='Basis'),
    'typed-extra-constraint': lambda v: v[2][0].constraints.new('COPY_LOCATION'),
    'typed-driver': lambda v: v[2][0].driver_add('location', 0),
    'dynamic-live-parent': lambda v: setattr(v[2][0].constraints[0], 'mute', False),
    'unbound-without-type': lambda v: v[2][0].__delitem__('mmd_type'),
    'broken-built-track': break_tracker,
    'wrong-group-type': lambda v: v[2][0].parent.__setitem__('mmd_type', 4),
}
for name, mutate in extra_guards.items():
    mode = 'built-dynamic-tracker' if name == 'broken-built-track' else 'unbound-collision'
    if name == 'dynamic-live-parent':
        mode = 'legacy-metadata-and-renamed-group'
    values = variant(mode)
    values[2][0].name = '017_+Breast_L_A01'
    mutate(values)
    path = OUT / (name + '.blend')
    bpy.ops.wm.save_as_mainfile(filepath=str(path))
    before_hash = sha(path)
    try:
        import_source(str(path), allow_shape_keys=True)
    except ValueError as error:
        assert ('constraints/drivers' in str(error) or 'Unverified rigid-body' in str(error)), str(error)
        assert 'Detection details:' in str(error)
    else:
        raise AssertionError('Unverified object was discarded: ' + name)
    assert sha(path) == before_hash
    results.append({'case': name + '-remains-blocked', 'passed': True})

# Never remove a character child along with a proven collision helper.
body, rig, helpers, root = variant('built-dynamic-tracker')
body.parent = helpers[0]
try:
    remove_workspace_helpers(bpy, mmd_helpers(bpy.context.scene.objects))
except ValueError as error:
    assert 'character child' in str(error)
else:
    raise AssertionError('Character child lost its parent')
assert body.name in bpy.context.scene.objects
results.append({'case': 'helper-with-character-child-stays-protected', 'passed': True})

# Existing shape-key protection stays active during conversion.
try:
    import_source(str(source))
except ValueError as error:
    assert 'without dropping shape keys' in str(error)
else:
    raise AssertionError('Conversion silently dropped shape keys')
results.append({'case': 'unreviewed-morph-conversion-stays-blocked', 'passed': True})

# The converted workspace and correction re-import contain only real geometry.
body, rig, helpers, root = sample(morph=False)
source_plain = OUT / 'convert-source.blend'
bpy.ops.wm.save_as_mainfile(filepath=str(source_plain))
source_plain_hash = sha(source_plain)
meshes, rigs = import_source(str(source_plain))
info = describe(meshes, rigs)
mapping = suggest_mapping(info['bones'], humanoid())
options = {'reviewed': True, 'mapping': mapping['mapping'],
           'source_roles': mapping['source']['roles'], 'target_roles': mapping['target']['roles']}
work = OUT / 'converted'
work.mkdir(exist_ok=True)
report = prepare({'mode': 'rigged_humanoid', 'options': options, 'target': humanoid(),
                  'workspace': str(work), 'hero_root': 'models/test/hero'}, meshes, rigs)
assert len(report['meshes']) == 1
assert len([obj for obj in bpy.context.scene.objects if obj.type == 'MESH']) == 1
meshes, rigs = import_source(report['blend'])
assert len(meshes) == len(rigs) == 1
assert not any(obj.rigid_body or obj.rigid_body_constraint for obj in bpy.context.scene.objects)
assert sha(source_plain) == source_plain_hash
results.append({'case': 'conversion-and-correction-exclude-physics-geometry', 'passed': True})

# Optional fixtures created by the official add-on in a DIFFERENT process.
# This process never registers MMD Tools or executes source-file scripts.
authored = os.environ.get('DMB_MMD_AUTHORED_FIXTURES')
if authored:
    assert not hasattr(bpy.types.Object, 'mmd_type')
    directory = Path(authored)
    reference_geometry = None
    for row in json.loads((directory / 'fixtures.json').read_text())['fixtures']:
        path = directory / (row['name'] + '.blend')
        before_hash = sha(path)
        assert before_hash == row['sha256']
        bpy.ops.wm.open_mainfile(filepath=str(path), load_ui=False, use_scripts=False)
        body = bpy.data.objects['body']
        rig = body.parent
        before = fingerprint([body], [rig])
        receipts = []
        meshes, rigs = import_source(str(path), helper_reports=receipts)
        assert len(meshes) == len(rigs) == 1
        assert receipts[0]['rigid_body_meshes'] == row['collision_meshes'] == 4
        assert fingerprint(meshes, rigs) == before
        bindings = physics_bone_bindings(bpy.context.scene.objects, rigs[0])
        assert sum(len(r['dynamic_bodies']) for r in bindings.values()) == 2
        assert sum(len(r['kinematic_bodies']) for r in bindings.values()) == 1
        work = OUT / ('authored-' + row['name'])
        work.mkdir(exist_ok=True)
        info = describe(meshes, rigs)
        mapping = suggest_mapping(info['bones'], humanoid())
        options = {'reviewed': True, 'mapping': mapping['mapping'],
                   'source_roles': mapping['source']['roles'], 'target_roles': mapping['target']['roles']}
        report = prepare({'mode': 'rigged_humanoid', 'options': options, 'target': humanoid(),
                          'workspace': str(work), 'hero_root': 'models/test/hero'}, meshes, rigs)
        geometry = [{key: mesh[key] for key in ('positions', 'normals', 'uvs', 'colors', 'weights', 'faces')}
                    for mesh in report['meshes']]
        if reference_geometry is None:
            reference_geometry = geometry
        assert geometry == reference_geometry, 'Saved physics state changed converted character geometry'
        meshes, rigs = import_source(report['blend'])
        assert len(meshes) == len(rigs) == 1
        assert not any(obj.rigid_body or obj.rigid_body_constraint for obj in bpy.context.scene.objects)
        assert sha(path) == before_hash
        results.append({'case': 'official-mmd-' + row['name'] + '-intake-conversion-correction', 'passed': True,
                        'source_sha256': before_hash, 'excluded_collision_meshes': row['collision_meshes']})

real = os.environ.get('DMB_MMD_SOURCE')
if real:
    real = Path(real).resolve()
    before_hash = sha(real)
    try:
        bpy.ops.wm.open_mainfile(filepath=str(real), load_ui=False, use_scripts=False)
        helpers = mmd_helpers(bpy.context.scene.objects)
        real_meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH' and obj not in helpers]
        real_rigs = [obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE']
        before = fingerprint(real_meshes, real_rigs)
        receipts = []
        meshes, rigs = import_source(str(real), allow_shape_keys=True, helper_reports=receipts)
        info = describe(meshes, rigs)
        assert fingerprint(meshes, rigs) == before
        baseline = os.environ.get('DMB_MMD_BASELINE_HELPERS')
        if baseline:
            import importlib.util
            spec = importlib.util.spec_from_file_location('baseline_helpers', baseline)
            old = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(old)
            assert old.mmd_helpers(bpy.context.scene.objects) == mmd_helpers(bpy.context.scene.objects)
            assert old.physics_bone_bindings(bpy.context.scene.objects, rigs[0]) == physics_bone_bindings(bpy.context.scene.objects, rigs[0])
        results.append({'case': 'read-only-user-source-intake', 'passed': True,
                        'prior_helper_and_physics_bindings_identical': bool(baseline),
                        'source_sha256': before_hash, 'character_stream_sha256': before,
                        'vertices': info['vertices'], 'meshes': len(meshes), 'bones': len(info['bones']),
                        'morphs': len(info['morphs']), 'helper_meshes': sum(item['rigid_body_meshes'] for item in receipts)})
    finally:
        assert sha(real) == before_hash, 'Original user asset changed'

(OUT / 'results.json').write_text(json.dumps({'blender': bpy.app.version_string, 'cases': results}, indent=2))
print(json.dumps(results, indent=2))
