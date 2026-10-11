"""Native Blender target-only gate, including real parent/roll quantization.

Run Blender --background --python-exit-code 23 --python this_file.py --
    --targets targets.json --output output_directory
The input is a list of {resource, target} using production prepare_target data.
This gate does not import replacement meshes or claim native compilation.
"""
import argparse
import copy
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src-tauri/crates/deadlock-preparation/resources/blender'))
import bpy
import numpy as np
from mathutils import Matrix
from worker import make_rig
from exact_bind import (METADATA_KEY, audit_display_bind, canonical_bones,
                        load_correction_bind, store_bind_metadata)

parser = argparse.ArgumentParser()
parser.add_argument('--targets', required=True, type=Path)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:])
args.output.mkdir(parents=True, exist_ok=True)
rows = []
for index, case in enumerate(json.loads(args.targets.read_text())):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    rig = make_rig(case['target'])
    receipt = store_bind_metadata(rig, case['target'])
    metadata = rig[METADATA_KEY]
    path = args.output / (str(index).zfill(2) + '.blend')
    bpy.ops.wm.save_as_mainfile(filepath=str(path))
    bpy.ops.wm.open_mainfile(filepath=str(path))
    rig = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
    assert rig[METADATA_KEY] == metadata
    restored, proof = load_correction_bind(rig, case['target'])
    for bone in case['target']:
        np.testing.assert_array_equal(restored[bone['name']], bone['matrix'])
    # Ensure the physical audit rejects damaged display transforms instead of
    # allowing SVD to conceal them; canonical data never changes in this test.
    canonical = canonical_bones(case['target'])
    display = json.loads(metadata)['display_bones']
    name = canonical[0]['name']
    for change in ('rotation', 'position', 'stretch', 'reflection'):
        damaged = copy.deepcopy(display)
        matrix = np.array(damaged[name]['matrix'])
        if change == 'rotation':
            matrix[:3, :3] = np.asarray(Matrix.Rotation(np.deg2rad(.2), 3, 'X')) @ matrix[:3, :3]
        elif change == 'position':
            matrix[0, 3] += .6 / 25.4
        elif change == 'stretch':
            matrix[:3, :3] *= 1.005
        else:
            matrix[:3, 0] *= -1
        damaged[name]['matrix'] = matrix.tolist()
        try:
            audit_display_bind(damaged, canonical)
        except ValueError:
            pass
        else:
            raise AssertionError('Display audit accepted ' + change)
    audit = receipt['display_audit']
    rows.append({'resource': case['resource'], 'bones': len(case['target']), 'passed': True,
                 'saved_reopened_metadata_exact': True, 'correction_rest_snapshot_exact': True,
                 'display_guards': 4, 'display_audit': {k: v for k, v in audit.items() if k != 'bones'}})
    print(case['resource'], rows[-1]['display_audit'], flush=True)
result = {'blender': bpy.app.version_string, 'cases': rows, 'passed': len(rows),
          'bone_count': sum(r['bones'] for r in rows), 'native_compiler_validated': False,
          'in_game_validated': False}
(args.output / 'results.json').write_text(json.dumps(result, indent=2))
