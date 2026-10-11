"""Skin mass, spatial continuity, invariance and isolation regressions."""
import copy
from pathlib import Path
import sys
import unittest
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'src-tauri/crates/deadlock-preparation/resources/blender'))
from arm_skinning import plan, redistribute
from algorithms import normalize_weights


def fixture():
    bones, roles = [], {}
    for side, y in [('L', 4.), ('R', -4.)]:
        for i, role in enumerate(('upperarm', 'forearm', 'hand')):
            name = role+'_'+side
            roles[name] = name
            matrix = np.eye(4); matrix[:3, 3] = [10.*i, y, 0.]
            bones.append({'name': name, 'parent': None if i == 0 else ('upperarm' if i == 1 else 'forearm')+'_'+side,
                          'matrix': matrix.tolist()})
            if i < 2:
                for j in (1, 2):
                    m = matrix.copy(); m[0, 3] += 10.*j/3.
                    bones.append({'name': name+'_TWIST'+str(j), 'parent': name, 'matrix': m.tolist()})
    mapping = {n: n for n in roles}
    mapping.update(sleeve='upperarm_L', hair='head', finger='finger_L')
    return bones, roles, mapping


class ArmSkinningTests(unittest.TestCase):
    def setUp(self):
        self.bones, self.roles, self.mapping = fixture()
        self.plan = plan(self.bones, self.roles, self.roles, self.mapping, {})

    def test_mass_and_unrelated_influences_survive_without_geometry_mutation(self):
        rows = [[('forearm_L', .7), ('hair', .1), ('finger', .2)], [('sleeve', 1.)]]
        points = [[15., 4., 1.], [5., 4., 1.]]
        original = copy.deepcopy((rows, points, self.bones))
        result, audit = redistribute(rows, points, self.mapping, self.plan)
        self.assertEqual(audit['adjusted_vertices'], 1)
        self.assertAlmostEqual(dict(result[0])['head'], .1)
        self.assertAlmostEqual(dict(result[0])['finger_L'], .2)
        self.assertEqual(result[1], [('upperarm_L', 1.)])
        self.assertAlmostEqual(sum(w for _, w in result[0]), 1.)
        self.assertEqual((rows, points, self.bones), original)

    def test_remote_arm_following_geometry_and_minor_influences_are_untouched(self):
        rows = [[('forearm_L', 1.)], [('forearm_L', .1), ('hair', .9)]]
        result, audit = redistribute(rows, [[15., 20., 0.], [15., 4., 0.]], self.mapping, self.plan)
        self.assertEqual(result, [[('forearm_L', 1.)], [('forearm_L', .1), ('head', .9)]])
        self.assertEqual(audit['adjusted_vertices'], 0)

    def test_spatial_weights_have_no_nearest_helper_steps_or_wrist_boundary_jump(self):
        points = [[10.+t, 4., .5] for t in np.linspace(0., 10.1, 2021)]
        rows = [[('forearm_L', 1.)] for _ in points]
        result, _ = redistribute(rows, points, self.mapping, self.plan)
        names = sorted({n for row in result for n, _ in row})
        weights = np.array([[dict(row).get(n, 0.) for n in names] for row in result])
        self.assertLess(float(np.abs(np.diff(weights, axis=0)).max()), .012)
        np.testing.assert_allclose(weights.sum(1), 1., atol=1e-12)
        self.assertEqual(result[-1], [('forearm_L', 1.)])
        # A source with no helper groups still uses the hero's two twist joints.
        self.assertIn('forearm_L_TWIST1', names)
        self.assertIn('forearm_L_TWIST2', names)

    def test_world_rotation_translation_scale_and_bone_order_do_not_change_weights(self):
        points = np.array([[5., 4., .2], [15., -4., .2]])
        rows = [[('upperarm_L', 1.)], [('forearm_R', 1.)]]
        expected, _ = redistribute(rows, points, self.mapping, self.plan)
        rotation = np.array([[0., -1., 0.], [0., 0., 1.], [-1., 0., 0.]])
        bones = copy.deepcopy(self.bones)[::-1]
        for bone in bones:
            m = np.array(bone['matrix']); m[:3, 3] = 7.*rotation@m[:3, 3]+[9., 2., -3.]
            bone['matrix'] = m.tolist()
        actual, _ = redistribute(rows, 7.*points@rotation.T+[9., 2., -3.], self.mapping,
                                  plan(bones, self.roles, self.roles, self.mapping, {}))
        for a, b in zip(actual, expected):
            self.assertEqual(set(dict(a)), set(dict(b)))
            np.testing.assert_allclose([dict(a)[n] for n, _ in b], [w for _, w in b], atol=1e-12)

    def test_ambiguous_helpers_end_markers_and_other_limb_branches_are_excluded(self):
        bones = copy.deepcopy(self.bones)
        helper = copy.deepcopy(next(b for b in bones if b['name'] == 'forearm_L_TWIST1'))
        helper['name'] = 'duplicate_TWIST'; bones.append(helper)
        found = plan(bones, self.roles, self.roles, self.mapping, {})
        self.assertNotIn('forearm_L', [p['role'] for p in found])
        for b in bones:
            if 'TWIST' in b['name']: b['name'] += '_end'
        self.assertEqual(plan(bones, self.roles, self.roles, self.mapping, {}), [])

    def test_only_reviewed_source_helpers_are_redistributed(self):
        mapping = {**self.mapping, 'foreign_helper': 'forearm_L_TWIST2'}
        rows = [[('foreign_helper', 1.)]]
        untouched, _ = redistribute(rows, [[11., 4., .1]], mapping, self.plan)
        self.assertEqual(untouched, [[('forearm_L_TWIST2', 1.)]])
        matched = {'foreign_helper': {'kind': 'twist', 'role': 'forearm_L', 'target': 'forearm_L_TWIST2'}}
        p = plan(self.bones, self.roles, self.roles, mapping, matched)
        changed, _ = redistribute(rows, [[11., 4., .1]], mapping, p)
        self.assertGreater(dict(changed[0])['forearm_L'], .5)

    def test_existing_influence_limit_still_applies(self):
        rows = [[('upperarm_L', .8), ('hair', .1), ('finger', .1)]]
        raw, _ = redistribute(rows, [[5., 4., .2]], self.mapping, self.plan)
        targets = {b['name'] for b in self.bones}|{'head', 'finger_L'}
        final, audit = normalize_weights(raw, {n: n for n in targets}, targets, 4)
        self.assertLessEqual(len(final[0]), 4)
        self.assertAlmostEqual(sum(w for _, w in final[0]), 1.)
        self.assertEqual(audit['unweighted'], 0)
        self.assertEqual(dict(raw[0]), {'upperarm_L': .8, 'head': .1, 'finger_L': .1})

    def test_mixed_physics_seams_keep_every_original_weight(self):
        mapping = {**self.mapping, 'cloth': 'retained_cloth'}
        rows = [[('forearm_L', .6), ('cloth', .4)]]
        result, audit = redistribute(rows, [[15., 4., .2]], mapping, self.plan, protected_sources={'cloth'})
        self.assertEqual(result, [[('forearm_L', .6), ('retained_cloth', .4)]])
        self.assertEqual(audit['physics_vertices_preserved'], 1)

    def test_missing_or_nonfinite_positions_are_rejected(self):
        for points in ([], [[float('nan'), 0., 0.]]):
            with self.assertRaises(ValueError):
                redistribute([[('forearm_L', 1.)]], points, self.mapping, self.plan)


if __name__ == '__main__':
    unittest.main()
