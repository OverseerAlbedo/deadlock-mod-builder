"""Exercise optional fitting in real bpy, including authored secondary branches.

Default: generated rigged/unrigged fixtures with nonunit target scales.
Optional --source/--project/--target: read-only qualification of an actual model.
This does not run Source 2 or claim game animation acceptance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender'), str(ROOT), str(ROOT/'scripts')]
import bpy
import numpy as np
from mathutils import Matrix, Quaternion
from worker import import_source, describe, prepare
from algorithms import suggest_mapping, identify
from static_face import apply_static_shapes
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.preparation.target_skeleton import prepare_target
from test_blender_preparation import fixture
from tests.preparation_fixture import skeleton

parser = argparse.ArgumentParser()
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--source', type=Path)
parser.add_argument('--project', type=Path)
parser.add_argument('--target', type=Path)
parser.add_argument('--geometry-only', action='store_true', help='Disconnect missing texture images in the temporary test scene only; never qualify texture recovery.')
args = parser.parse_args()
OUT = args.output.resolve(); OUT.mkdir(parents=True, exist_ok=True)
bpy.context.preferences.filepaths.use_scripts_auto_execute = False
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
original_hash = sha(args.source) if args.source else None
cases = []


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, allow_nan=False), encoding='utf-8')


def evaluated(obj):
    bpy.context.view_layer.update()
    evaluated_obj = obj.evaluated_get(bpy.context.evaluated_depsgraph_get())
    mesh = evaluated_obj.to_mesh()
    try:
        return np.asarray([evaluated_obj.matrix_world@v.co for v in mesh.vertices], dtype=float)
    finally:
        evaluated_obj.to_mesh_clear()


def test_case(label, rigged=True):
    directory = OUT/label; directory.mkdir(exist_ok=True)
    if args.source:
        assert args.project and args.target
        meshes, rigs = import_source(str(args.source), allow_shape_keys=True)
        choice = json.loads(args.project.read_text())['preparation_options'].copy()
        apply_static_shapes(meshes, choice['shape_key_mode'])
        target = json.loads(args.target.read_text())
    else:
        meshes = fixture(rigged=rigged, apose=True)
        rigs = [o for o in bpy.context.scene.objects if o.type == 'ARMATURE']
        base = skeleton()
        base['m_boneScaleParent'][base['m_boneName'].index('pelvis')] = 1.2
        base['m_boneScaleParent'][base['m_boneName'].index('chest')] = .8
        target, _ = prepare_target(base)
        info = describe(meshes, rigs)
        suggested = suggest_mapping(info['bones'], target) if rigged else {'target': identify(target)}
        choice = {'reviewed': True, 'influence_limit': 4, 'target_roles': suggested['target']['roles']}
        if rigged:
            choice.update(source_roles=suggested['source']['roles'], mapping=suggested['mapping'])
        else:
            choice['landmarks'] = info['humanoid']['landmarks']
    missing_images = []
    if args.geometry_only:
        trees = [m.node_tree for m in bpy.data.materials if m.node_tree] + list(bpy.data.node_groups)
        for tree in trees:
            for node in tree.nodes:
                image = getattr(node, 'image', None)
                if image and image.source == 'FILE' and not image.packed_file and not Path(bpy.path.abspath(image.filepath)).is_file():
                    missing_images.append(image.name)
                    node.image = None
    choice['fit_mode'] = 'preserve_proportions'
    info = describe(meshes, rigs)
    streams = {o.name: {'faces': [(tuple(p.vertices), p.material_index) for p in o.data.polygons],
                       'uvs': [[list(d.uv) for d in layer.data] for layer in o.data.uv_layers],
                       'colors': [[list(d.color) for d in layer.data] for layer in o.data.color_attributes]}
               for o in meshes}
    write(directory/'inspection.json', info); write(directory/'options.json', choice)
    write(directory/'original-target.json', target)
    original_target = json.dumps(target, sort_keys=True)
    report = prepare({'mode': 'rigged_humanoid' if rigged else 'unrigged_humanoid', 'target': target,
                      'options': choice, 'workspace': str(directory), 'hero_root': 'models/test/proportions'}, meshes, rigs)
    assert report['rest_alignment']['policy'] == 'source-proportions-v1'
    assert json.dumps(report['target'][:len(target)], sort_keys=True) == original_target
    assert json.dumps(target, sort_keys=True) == original_target
    max_rest_error = 0.
    for obj, mesh in zip(meshes, report['meshes']):
        assert [(tuple(p.vertices), p.material_index) for p in obj.data.polygons] == streams[obj.name]['faces']
        assert mesh['uvs'] == streams[obj.name]['uvs']
        assert [[list(d.color) for d in layer.data] for layer in obj.data.color_attributes] == streams[obj.name]['colors']
        np.testing.assert_allclose(evaluated(obj), mesh['positions'], atol=5e-5, rtol=1e-6)
        max_rest_error = max(max_rest_error, float(np.max(np.abs(evaluated(obj)-mesh['positions']))))
        assert np.isfinite(mesh['normals']).all()
        assert all(abs(sum(w for _, w in row)-1.) < 1e-7 and len(row) <= 4 for row in mesh['weights'])
    final = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
    names = [choice['target_roles'][r] for r in ('head', 'upperarm_L', 'shin_L')]
    kept = report.get('preserved_physics_bones')
    if kept:
        group = next((g for g in kept['groups'] if g['root'].startswith('tail_')), kept['groups'][0])
        names.append(kept['exported_bones'][group['weighted_bones'][len(group['weighted_bones'])//2]])
    probes = []
    for name in names:
        branch = {name}
        while True:
            expanded = branch | {b['name'] for b in report['target'] if b['parent'] in branch}
            if expanded == branch: break
            branch = expanded
        before = [evaluated(o) for o in meshes]
        bone = final.pose.bones[name]; bone.rotation_mode = 'QUATERNION'
        bone.rotation_quaternion = Quaternion((1, 0, 0), .2)
        after = [evaluated(o) for o in meshes]
        moved = 0
        for a, b, mesh in zip(before, after, report['meshes']):
            assert np.isfinite(b).all()
            amount = np.asarray([sum(w for n, w in row if n in branch) for row in mesh['weights']])
            np.testing.assert_allclose(b[amount == 0], a[amount == 0], atol=1e-5, rtol=0)
            moved += int(np.count_nonzero(np.linalg.norm(b-a, axis=1) > 1e-5))
        assert moved > 0, name
        probes.append({'bone': name, 'moved_vertices': moved, 'unrelated_vertices_unchanged': True})
        bone.matrix_basis = Matrix.Identity(4)
        for obj, before_points in zip(meshes, before):
            np.testing.assert_allclose(evaluated(obj), before_points, atol=1e-5, rtol=0)
    write(directory/'result.json', report)
    correction = directory/'correction'; correction.mkdir(exist_ok=True)
    new_meshes, new_rigs = import_source(report['blend'])
    correction_choice = choice | {'source_roles': choice['target_roles'], 'mapping': {b['name']: b['name'] for b in report['target']}}
    correction_choice.pop('mapping_anchors', None); correction_choice.pop('follow_motion_reviewed', None)
    corrected = prepare({'mode': 'rigged_humanoid', 'correction': True, 'target': report['target'],
                         'options': correction_choice, 'workspace': str(correction),
                         'hero_root': 'models/test/proportions'}, new_meshes, new_rigs)
    assert corrected['target'] == report['target']
    max_correction_error = 0.
    for a, b in zip(report['meshes'], corrected['meshes']):
        np.testing.assert_allclose(a['positions'], b['positions'], atol=5e-6, rtol=1e-6)
        assert a['faces'] == b['faces'] and a['uvs'] == b['uvs'] and a['colors'] == b['colors']
        max_correction_error = max(max_correction_error, float(np.max(np.abs(np.asarray(a['positions'])-b['positions']))))
    write(correction/'result.json', corrected)
    case = {'case': label, 'passed': True, 'vertices': sum(len(m['positions']) for m in report['meshes']),
            'original_hero_bones': len(target), 'target_binds_unchanged': True,
            'preserved_bones': len(kept['exported_bones']) if kept else 0, 'preserved_groups': len(kept['groups']) if kept else 0,
            'max_rest_evaluation_error_inches': max_rest_error, 'max_correction_error_inches': max_correction_error,
            'topology_uvs_colors_unchanged': True, 'probes': probes,
            'max_joint_offset_mm': report['rest_alignment']['max_joint_offset_mm'],
            'missing_texture_images_disconnected_for_geometry_test': sorted(set(missing_images))}
    cases.append(case); print(json.dumps(case), flush=True)


if args.source:
    test_case('tangtang')
    assert sha(args.source) == original_hash
else:
    test_case('generated-rigged', True)
    test_case('generated-unrigged', False)
summary = {'blender': bpy.app.version_string, 'source_sha256': original_hash, 'source_unchanged': True,
           'cases': cases, 'texture_recovery_qualified': False, 'geometry_only': args.geometry_only,
           'native_compiler_validated': False, 'in_game_validated': False}
write(OUT/'results.json', summary)
