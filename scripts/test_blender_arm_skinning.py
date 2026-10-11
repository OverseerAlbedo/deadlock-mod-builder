"""Compare source/smooth skinning on an actual source without rewriting it.

Run in Blender with --python-exit-code 23 and -- followed by this script's
arguments. Missing external image nodes may be disconnected only with the
explicit geometry-only flag. This does not run Valve's compiler or the game.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import sys
import bpy
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender'))
import worker

p = argparse.ArgumentParser(__doc__)
for name in ('source', 'target', 'options', 'output'):
    p.add_argument('--'+name, type=Path, required=True)
p.add_argument('--geometry-only', action='store_true')
a = p.parse_args(sys.argv[sys.argv.index('--')+1:])
a.output.mkdir(parents=True, exist_ok=True)
digest = hashlib.sha256(a.source.read_bytes()).hexdigest()
target = json.loads(a.target.read_text())
options = json.loads(a.options.read_text())
reports, missing = {}, set()
for mode in ('source', 'smooth_twist'):
    meshes, rigs = worker.import_source(str(a.source), allow_shape_keys=True)
    worker.apply_static_shapes(meshes, 'current_static')
    if a.geometry_only:
        for mat in bpy.data.materials:
            if mat.node_tree:
                for node in mat.node_tree.nodes:
                    image = getattr(node, 'image', None)
                    if image and image.source == 'FILE' and not image.packed_file and not Path(bpy.path.abspath(image.filepath)).is_file():
                        missing.add(image.name); node.image = None
    out = a.output/mode; out.mkdir(exist_ok=True)
    choice = {**copy.deepcopy(options), 'arm_skinning': mode}
    report = worker.prepare({'mode': 'rigged_humanoid', 'options': choice, 'target': copy.deepcopy(target),
                             'workspace': str(out), 'hero_root': 'models/test/arm_skinning'}, meshes, rigs)
    reports[mode] = report
    assert all(len(row) <= 4 and abs(sum(w for _, w in row)-1.) < 1e-7
               for m in report['meshes'] for row in m['weights'])

old, new = reports['source'], reports['smooth_twist']
assert old['target'] == new['target'] and old['rest_alignment'] == new['rest_alignment']
assert old['preserved_physics_bones'] == new['preserved_physics_bones']
arm_names = {options['target_roles'][part+'_'+side] for part in ('upperarm', 'forearm') for side in ('L', 'R')}
arm_names.update(n for s in new['arm_skinning']['segments'] for n in s['twist_joints'])
physics = set((new['preserved_physics_bones'] or {}).get('exported_bones', {}).values())
changed = untouched = physics_rows = 0
max_unrelated = max_tangent = 0.
for before, after in zip(old['meshes'], new['meshes']):
    for key in ('name', 'positions', 'normals', 'uvs', 'colors', 'faces'):
        assert before[key] == after[key], key
    # Blender's tangent calculation can differ by a float32 rounding step
    # between identical scene loads; positions, normals and UVs are exact.
    np.testing.assert_allclose(before['tangents'], after['tangents'], atol=2e-6, rtol=0)
    if before['tangents']:
        max_tangent = max(max_tangent, float(np.max(np.abs(np.array(before['tangents'])-after['tangents']))))
    for x, y in zip(before['weights'], after['weights']):
        x, y = dict(x), dict(y)
        difference = max(abs(x.get(n, 0.)-y.get(n, 0.)) for n in set(x)|set(y))
        changed += difference > 1e-7
        untouched += difference <= 1e-7
        for n in (set(x)|set(y))-arm_names:
            delta = abs(x.get(n, 0.)-y.get(n, 0.))
            assert delta < 1e-7, (n, delta)
            max_unrelated = max(max_unrelated, delta)
        if physics.intersection(x):
            physics_rows += 1
            assert difference < 1e-7
assert changed > 0, 'This actual source did not exercise the new skinning'

# Correction files already contain the reviewed final skin. Re-export must
# retain those weights, including the existing custom physics branches.
meshes, rigs = worker.import_source(new['blend'])
out = a.output/'correction'; out.mkdir(exist_ok=True)
choice = {**options, 'arm_skinning': 'smooth_twist', 'source_roles': options['target_roles'],
          'mapping': {b['name']: b['name'] for b in new['target']}, 'mapping_anchors': {}}
corrected = worker.prepare({'mode': 'rigged_humanoid', 'options': choice, 'target': new['target'], 'correction': True,
                            'workspace': str(out), 'hero_root': 'models/test/arm_skinning'}, meshes, rigs)
assert corrected['arm_skinning']['correction_weights_preserved']
assert not corrected['arm_skinning']['segments']
for before, after in zip(new['meshes'], corrected['meshes']):
    np.testing.assert_allclose(before['positions'], after['positions'], atol=1e-4, rtol=1e-6)
    for x, y in zip(before['weights'], after['weights']):
        x, y = dict(x), dict(y)
        assert set(x) == set(y)
        assert max(abs(x[n]-y[n]) for n in x) < 1e-7
assert hashlib.sha256(a.source.read_bytes()).hexdigest() == digest
summary = {'blender': bpy.app.version_string, 'source_sha256': digest, 'source_unchanged': True,
           'vertices': changed+untouched, 'changed_arm_vertices': changed,
           'unchanged_vertices': untouched, 'physics_vertices_preserved': physics_rows,
           'max_unrelated_weight_delta': max_unrelated, 'geometry_and_target_binds_unchanged': True,
           'max_tangent_rounding_delta': max_tangent,
           'correction_weights_preserved': True, 'arm_skinning': new['arm_skinning'],
           'missing_images': sorted(missing), 'geometry_only': a.geometry_only, 'in_game_tested': False}
(a.output/'verification.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2)+'\n')
print(json.dumps({k: v for k, v in summary.items() if k not in ('arm_skinning', 'missing_images')}))
