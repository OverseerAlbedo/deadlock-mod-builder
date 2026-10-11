"""Real Blender proof of scaled target binds, skinning and correction metadata.

Run with Blender --background --python-exit-code 23 --python this_file.py.
No Valve compilation or game validation is performed by this test.
"""
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT), str(ROOT / 'scripts'), str(ROOT / 'src-tauri/crates/deadlock-preparation/resources/blender')]
import bpy
import numpy as np
from mathutils import Matrix, Quaternion
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import skeleton
from test_blender_preparation import fixture
from worker import import_source, describe, prepare
from algorithms import identify, suggest_mapping
from exact_bind import (SCALE_POLICY, METADATA_KEY, canonical_bones, load_correction_bind,
                        _local_matrix, equivalent_pose_matrix)

OUT = Path(os.environ.get('DMB_EXACT_BIND_TEST_OUTPUT', str(ROOT / 'build/blender-exact-bind-test'))).resolve()
OUT.mkdir(parents=True, exist_ok=True)


def scaled_target():
    s = skeleton()
    changes = {'pelvis': 1.2, 'spine': .98, 'chest': 1.1, 'upperarm_L': .9,
               'upperarm_R': .9, 'forearm_L': 1.08, 'forearm_R': 1.08,
               'thigh_L': 1.07, 'thigh_R': 1.07, 'head': .8}
    names = s['m_boneName']; worlds = {}; result = []
    for i, name in enumerate(names):
        parent = names[s['m_nParent'][i]] if s['m_nParent'][i] >= 0 else None
        scale = changes.get(name, 1.)
        rotation = [0., 0., 0., 1.]
        if name in {'pelvis', 'chest'}:
            q = Quaternion((0, 0, 1), .15 if name == 'pelvis' else -.08)
            rotation = [q.x, q.y, q.z, q.w]
        local = _local_matrix(s['m_bonePosParent'][i], rotation, scale)
        worlds[name] = worlds[parent] @ local if parent else local
        result.append({'name': name, 'parent': parent, 'local_position': s['m_bonePosParent'][i],
                       'local_rotation': rotation, 'local_scale': scale,
                       'matrix': worlds[name].tolist(), 'position': worlds[name][:3, 3].tolist(),
                       'inverse_bind_matrix': np.linalg.inv(worlds[name]).tolist(), 'length': .1})
    return result


def evaluated_points(obj):
    bpy.context.view_layer.update()
    evaluated = obj.evaluated_get(bpy.context.evaluated_depsgraph_get())
    mesh = evaluated.to_mesh()
    try:
        return np.array([evaluated.matrix_world @ v.co for v in mesh.vertices], dtype=float)
    finally:
        evaluated.to_mesh_clear()


def expected_skin(obj, matrices):
    values = []
    for vertex in obj.data.vertices:
        point = np.r_[np.array(obj.matrix_world @ vertex.co), 1.]
        row = [(obj.vertex_groups[g.group].name, float(g.weight)) for g in vertex.groups if g.weight > 0]
        total = sum(w for _, w in row)
        values.append(sum((w / total * (matrices[n] @ point)[:3] for n, w in row), np.zeros(3)))
    return np.array(values)


def proves_skin(rig, meshes, target):
    canonical = canonical_bones(target)
    byname = {b['name']: b for b in canonical}
    rest_errors = []
    for obj in meshes:
        rest = np.array([obj.matrix_world @ v.co for v in obj.data.vertices])
        rest_errors.append(float(np.max(np.abs(evaluated_points(obj) - rest))))
        np.testing.assert_allclose(evaluated_points(obj), rest, atol=1e-5, rtol=1e-6)
        np.testing.assert_array_equal(obj.matrix_parent_inverse, np.eye(4))
    # Nonunit local pose scale and local rotations on scaled parents/descendants.
    # Scalar bind scale commutes with these basis transforms, so Blender's
    # normalized display rig should implement the full Source 2 LBS exactly.
    for name, angle, scale in [('pelvis', .27, 1.12), ('chest', -.23, .93),
                               ('upperarm_L', .35, 1.18), ('forearm_L', -.42, .84)]:
        p = rig.pose.bones[name]; p.rotation_mode = 'QUATERNION'
        p.rotation_quaternion = Quaternion((1, 0, 0), angle)
        p.scale = (scale, scale, scale)
    bpy.context.view_layer.update()
    animated = {}; skin = {}; matrix_errors = []
    for b in canonical:
        name, parent = b['name'], b['parent']
        local = _local_matrix(b['local_position'], b['local_rotation'], b['local_scale'])
        local = local @ np.array(rig.pose.bones[name].matrix_basis)
        animated[name] = animated[parent] @ local if parent else local
        skin[name] = animated[name] @ np.array(b['inverse_bind_matrix'])
        expected = equivalent_pose_matrix(animated[name], b['inverse_bind_matrix'], rig.data.bones[name].matrix_local)
        matrix_errors.append(float(np.max(np.abs(np.array(rig.pose.bones[name].matrix) - expected))))
        np.testing.assert_allclose(rig.pose.bones[name].matrix, expected, atol=1e-5, rtol=1e-6)
    posed_errors = []
    for obj in meshes:
        expected = expected_skin(obj, skin)
        actual = evaluated_points(obj)
        posed_errors.append(float(np.max(np.abs(actual - expected))))
        np.testing.assert_allclose(actual, expected, atol=1e-5, rtol=1e-6)
    for p in rig.pose.bones:
        p.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()
    return {'max_rest_error': max(rest_errors), 'max_posed_skin_error': max(posed_errors),
            'max_equivalent_pose_matrix_error': max(matrix_errors),
            'local_pose_scale_and_rotations_tested': True}


target = scaled_target()
target_roles = identify(target)
assert not target_roles['errors'], target_roles
results = []
for mode in ('rigged_humanoid', 'unrigged_humanoid'):
    work = OUT / mode; work.mkdir(exist_ok=True)
    objects = fixture('mixamo', rigged=mode == 'rigged_humanoid')
    if mode == 'rigged_humanoid':
        source_rig = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
        source_rig.matrix_world = Matrix.Translation((3., 2., 0.)) @ Matrix.Scale(1.25, 4)
    source = work / 'source.glb'
    bpy.ops.export_scene.gltf(filepath=str(source), export_format='GLB')
    meshes, rigs = import_source(str(source)); info = describe(meshes, rigs)
    options = {'reviewed': True, 'target_roles': target_roles['roles'], 'influence_limit': 4}
    if rigs:
        suggested = suggest_mapping(info['bones'], target, target_roles=target_roles['roles'])
        options.update(mapping=suggested['mapping'], source_roles=suggested['source']['roles'])
    else:
        options['landmarks'] = info['humanoid']['landmarks']
    report = prepare({'source': str(source), 'mode': mode, 'target': target, 'options': options,
                      'workspace': str(work), 'hero_root': 'models/test/exact_bind'}, meshes, rigs)
    final = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
    metadata = json.loads(final[METADATA_KEY])
    assert metadata['bones'] == canonical_bones(target)
    assert metadata['policy'] == SCALE_POLICY
    for bone in final.data.bones:
        np.testing.assert_allclose(bone.matrix_local.to_scale(), [1, 1, 1], atol=1e-6)
    skin = proves_skin(final, meshes, target)
    # A real save/reopen path must preserve canonical doubles and the exact
    # display-rest snapshot; returning through correction must not move geometry.
    meshes, rigs = import_source(report['blend'])
    restored, proof = load_correction_bind(rigs[0], target)
    for bone in target:
        np.testing.assert_array_equal(restored[bone['name']], bone['matrix'])
    before = {o.name: np.array([o.matrix_world @ v.co for v in o.data.vertices]) for o in meshes}
    corrected = work / 'corrected'; corrected.mkdir(exist_ok=True)
    correction_options = {'reviewed': True, 'target_roles': target_roles['roles'], 'source_roles': target_roles['roles'],
                          'mapping': {b['name']: b['name'] for b in target}, 'influence_limit': 4}
    correction = prepare({'source': report['blend'], 'correction': True, 'mode': 'rigged_humanoid', 'target': target, 'options': correction_options,
                          'workspace': str(corrected), 'hero_root': 'models/test/exact_bind'}, meshes, rigs)
    assert correction['correction_bind']['hierarchy_and_display_rest_unchanged']
    for mesh in correction['meshes']:
        np.testing.assert_allclose(mesh['positions'], before[mesh['name']], rtol=0, atol=1e-10)
    failures = ('missing_metadata', 'changed_rest', 'changed_parent', 'changed_object', 'changed_canonical',
                'posed', 'pose_constraint', 'animation', 'deform_flag', 'inherit_scale')
    for failure in failures:
        _, rigs = import_source(report['blend']); rig = rigs[0]
        if failure == 'missing_metadata':
            del rig[METADATA_KEY]
        elif failure in {'changed_rest', 'changed_parent'}:
            bpy.context.view_layer.objects.active = rig; rig.select_set(True); bpy.ops.object.mode_set(mode='EDIT')
            if failure == 'changed_rest': rig.data.edit_bones['hand_L'].head.x += .001
            else: rig.data.edit_bones['hand_L'].parent = rig.data.edit_bones['chest']
            bpy.ops.object.mode_set(mode='OBJECT')
        elif failure == 'changed_object':
            rig.location.x += .01; bpy.context.view_layer.update()
        elif failure == 'changed_canonical':
            bad = json.loads(rig[METADATA_KEY]); bad['bones'][0]['local_scale'] *= 1.01
            rig[METADATA_KEY] = json.dumps(bad)
        elif failure == 'posed':
            rig.pose.bones['head'].rotation_mode = 'XYZ'
            rig.pose.bones['head'].rotation_euler.x = .2
            bpy.context.view_layer.update()
        elif failure == 'pose_constraint':
            rig.pose.bones['head'].constraints.new('COPY_LOCATION')
        elif failure == 'animation':
            rig.animation_data_create().action = bpy.data.actions.new('unsupported correction animation')
        elif failure == 'deform_flag':
            rig.data.bones['head'].use_deform = False
        else:
            rig.data.bones['head'].inherit_scale = 'NONE'
        try:
            load_correction_bind(rig, target)
        except ValueError:
            pass
        else:
            raise AssertionError('Exact correction accepted ' + failure)
    results.append({'mode': mode, 'passed': True, 'skin': skin, 'saved_reopened_metadata_exact': True,
                    'zero_edit_correction_geometry_unchanged': True, 'correction_guards': len(failures)})

(OUT / 'results.json').write_text(json.dumps({'blender': bpy.app.version_string, 'scale_policy': SCALE_POLICY,
    'cases': results, 'native_compiler_validated': False, 'in_game_validated': False}, indent=2))
print(json.dumps(results, indent=2))
