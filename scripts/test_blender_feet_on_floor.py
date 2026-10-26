"""Model fitting -> Feet on the floor in real bpy: every choice prepares the
same procedural humanoid, 'ankles' (the default) leaves the fit exactly as
before, and the other two stand the soles on the hero's floor (height 0).

  blender --background --factory-startup --python-exit-code 23 --python scripts/test_blender_feet_on_floor.py -- --output <folder>
"""
import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender'), str(ROOT), str(ROOT/'scripts')]
import bpy
import numpy as np
from mathutils import Vector
from worker import describe, prepare
from algorithms import suggest_mapping, identify
from secondary_bones import physics_groups
from retarget import floor_corrections, FLOOR_MODES, FLOOR_TOLERANCE
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.preparation.target_skeleton import prepare_target
from test_blender_preparation import fixture
from tests.preparation_fixture import skeleton

parser = argparse.ArgumentParser()
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else [])
OUT = args.output.resolve(); OUT.mkdir(parents=True, exist_ok=True)

# The corrections alone: a sole 2 in below the floor, hips at 36.
assert FLOOR_MODES == ('ankles', 'legs', 'lift')
none, _, scale = floor_corrections('ankles', -2., 36.)
assert np.array_equal(none, np.eye(4)) and scale == 1.
lift, legs, _ = floor_corrections('lift', -2., 36.)
assert lift[2, 3] == 2. and np.array_equal(legs, np.eye(4))
every, legs, scale = floor_corrections('legs', -2., 36.)
assert np.array_equal(every, np.eye(4)) and scale < 1.
assert abs((legs@[0, 0, -2., 1])[2]) < 1e-12 and abs((legs@[0, 0, 36., 1])[2]-36.) < 1e-12
assert floor_corrections('legs', 1.5, 36.)[2] > 1.  # floating feet: longer legs
assert floor_corrections('legs', -FLOOR_TOLERANCE/2, 36.)[2] == 1.  # within tolerance
assert floor_corrections('legs', -2., 0.)[2] == 1.  # hips at the floor: nothing
assert floor_corrections('legs', -2., -1.9)[2] == 1.  # past the 0.2-3 scale guard: nothing
for gone in ('float', 'feet'):  # 'feet' (flatten the feet) was dropped on October 25, 2026
    try:
        floor_corrections(gone, -2., 36.)
        raise AssertionError(f'unknown mode accepted: {gone}')
    except ValueError:
        pass


def add_ribbon(meshes, rig, shin):
    """A kept three-bone ribbon hanging beside the left shin (moving part)."""
    bpy.context.view_layer.objects.active = rig; bpy.ops.object.mode_set(mode='EDIT')
    knee = rig.data.edit_bones[shin].head.copy(); parent = rig.data.edit_bones[shin]
    for i, name in enumerate(('ribbon', 'ribbon_helper', 'ribbon_tip')):
        bone = rig.data.edit_bones.new(name); bone.head = knee+Vector((.1, 0, -.08*i)); bone.tail = bone.head+Vector((0, 0, -.08))
        bone.parent = parent; parent = bone
    bpy.ops.object.mode_set(mode='OBJECT')
    x, z = knee.x+.1, knee.z
    data = bpy.data.meshes.new('ribbon geometry')
    data.from_pydata([(x, 0, z), (x+.04, 0, z), (x, 0, z-.2), (x+.04, 0, z-.2)], [], [(0, 1, 2), (1, 3, 2)])
    obj = bpy.data.objects.new('ribbon geometry', data); bpy.context.collection.objects.link(obj); obj.parent = rig
    obj.modifiers.new('Rig', 'ARMATURE').object = rig
    obj.vertex_groups.new(name='ribbon').add([0, 1], 1., 'REPLACE'); obj.vertex_groups.new(name='ribbon_tip').add([2, 3], 1., 'REPLACE')
    obj.data.materials.append(bpy.data.materials.new('ribbon')); meshes.append(obj)


def prepared(mode, sink, ribbon=False):
    meshes = fixture(rigged=True)
    rigs = [o for o in bpy.context.scene.objects if o.type == 'ARMATURE']
    target, _ = prepare_target(skeleton())
    if ribbon:
        shin = identify(describe(meshes, rigs)['bones'])['roles']['shin_L']
        add_ribbon(meshes, rigs[0], shin)
    info = describe(meshes, rigs)
    suggested = suggest_mapping(info['bones'], target)
    roles = suggested['source']['roles']
    if sink:
        # Taller feet (heels): the vertices on the feet move down 15% of the body.
        body = meshes[0]
        height = np.ptp([v.co.z for v in body.data.vertices])
        on_foot = {body.vertex_groups[roles['foot_'+s]].index for s in 'LR'}
        for v in body.data.vertices:
            if any(g.group in on_foot and g.weight > 0 for g in v.groups):
                v.co.z -= .15*height
    choice = {'reviewed': True, 'influence_limit': 4, 'target_roles': suggested['target']['roles'],
              'source_roles': roles, 'mapping': suggested['mapping']}
    if ribbon:
        groups = physics_groups(info['bones'], info['weighted_bones'], roles, suggested['target']['roles'])
        assert [g['root'] for g in groups] == ['ribbon'], groups
        choice.update(preserve_physics_bones=True, physics_roots=['ribbon'], follow_motion_reviewed=True)
    if mode is not None:
        choice['fit_floor'] = mode
    directory = OUT/f"{'sink' if sink else 'float'}-{mode or 'unset'}{'-ribbon' if ribbon else ''}"; directory.mkdir(exist_ok=True)
    report = prepare({'mode': 'rigged_humanoid', 'target': target, 'options': choice,
                      'workspace': str(directory), 'hero_root': 'models/test/floor'}, meshes, rigs)
    return report, np.asarray(report['meshes'][0]['positions'], dtype=float), report['meshes'][0]['weights'], suggested['target']['roles']


def subtree(target, root):
    found = {root}
    while True:
        more = found | {b['name'] for b in target if b.get('parent') in found}
        if more == found:
            return found
        found = more


results = {}
for sink in (False, True):
    base, base_points, weights, target_roles = prepared(None, sink)
    floor = base['rest_alignment']['floor']
    assert floor['mode'] == 'ankles' and floor['measured'] and not floor['applied'], floor
    sole = floor['sole_inches']
    assert (sole < -2*FLOOR_TOLERANCE) if sink else (sole > 1.5*FLOOR_TOLERANCE), (sink, sole)
    feet_bones = subtree(base['target'], target_roles['foot_L']) | subtree(base['target'], target_roles['foot_R'])
    leg_bones = subtree(base['target'], target_roles['thigh_L']) | subtree(base['target'], target_roles['thigh_R'])
    share = lambda bones: np.asarray([sum(w for n, w in row if n in bones) for row in weights])
    on_feet, on_legs = share(feet_bones), share(leg_bones)
    lowest = lambda points: float(np.percentile(points[on_feet >= .5, 2], 1))
    case = 'sink' if sink else 'float'
    results[case] = {'unset': {'sole_inches': sole}}
    for mode in FLOOR_MODES:
        report, points, _, _ = prepared(mode, sink)
        audit = report['rest_alignment']['floor']
        assert audit['mode'] == mode and audit['measured'], audit
        results[case][mode] = {key: audit.get(key) for key in ('sole_inches', 'applied', 'vertical_scale', 'lift_inches')}
        if mode == 'ankles':
            assert not audit['applied']
            assert np.array_equal(points, base_points), 'ankles must keep the fit exactly'
            continue
        assert audit['applied'], audit
        assert abs(lowest(points)) < .01, (case, mode, lowest(points))
        assert any(w.startswith('Feet on the floor:') for w in report['warnings']), report['warnings']
        if mode == 'lift':
            # One upward (or downward) shift of every point; the receipt rounds it.
            shift = points-base_points
            np.testing.assert_allclose(shift, [shift[0]]*len(points), atol=1e-9)
            np.testing.assert_allclose(shift[0], [0, 0, -sole], atol=1e-3)
        else:
            assert (audit['vertical_scale'] < 1.) == sink, audit
            untouched = on_legs == 0
            assert untouched.any()
            np.testing.assert_allclose(points[untouched], base_points[untouched], atol=1e-9)
            assert np.abs(points[~untouched]-base_points[~untouched]).max() > 1e-3
            # Only heights change.
            np.testing.assert_allclose(points[:, :2], base_points[:, :2], atol=1e-9)

# A kept moving part on a squashed shin keeps orthonormal bones (rigid, following
# its attachment point); with the ankles fit it is exactly as before.
results['ribbon'] = {}
kept_base = None
for mode in (None, 'legs', 'lift'):
    report, points, _, _ = prepared(mode, True, ribbon=True)
    hero = len(prepare_target(skeleton())[0])
    added = report['target'][hero:]
    assert [b['name'] for b in added] == list(report['preserved_physics_bones']['exported_bones'].values()), added
    for bone in added:
        m = np.asarray(bone['matrix'], dtype=float)[:3, :3]
        np.testing.assert_allclose(m.T@m, np.eye(3), atol=1e-6, err_msg=bone['name'])
    ribbon_points = np.asarray(next(m for m in report['meshes'] if m['name'] == 'ribbon geometry')['positions'])
    if mode is None:
        kept_base = ribbon_points
    elif mode == 'legs':
        # Rigid: the strip keeps its length (0.2 before the unit scale) instead of squashing.
        assert report['rest_alignment']['floor']['vertical_scale'] < 1.
        np.testing.assert_allclose(np.linalg.norm(ribbon_points[2]-ribbon_points[0]), np.linalg.norm(kept_base[2]-kept_base[0]), rtol=1e-6)
        assert np.abs(ribbon_points-kept_base).max() > 1e-3  # it follows the shorter leg
    results['ribbon'][mode or 'unset'] = len(added)
print(json.dumps({'passed': sum(len(r) for r in results.values()), 'results': results}))
