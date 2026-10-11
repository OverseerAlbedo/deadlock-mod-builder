"""Real Blender regression for local appends versus linked datablocks.

Creates only generated fixtures and verifies source hashes after each check.
Run inside Blender, or with the official bpy package.
"""
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'src-tauri/crates/deadlock-preparation/resources/blender'),
               str(ROOT / 'scripts'), str(ROOT)]
import bpy
from test_blender_preparation import fixture
from source_helpers import uses_linked_libraries
from worker import import_source
from blend_source import snapshot_blend

OUT = Path(os.environ['DMB_PREP_TEST_OUTPUT']).resolve()
OUT.mkdir(parents=True, exist_ok=True)
library = OUT / 'generated-library.blend'
fixture()
bpy.ops.wm.save_as_mainfile(filepath=str(library))
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
library_hash = sha(library)
cases = []

for kind in ['object', 'material']:
    fixture()
    with bpy.data.libraries.load(str(library), link=True) as (available, loaded):
        if kind == 'object':
            loaded.objects = ['body']
        else:
            loaded.materials = available.materials[:1]
    if kind == 'object':
        bpy.context.scene.collection.objects.link(loaded.objects[0])
    else:
        bpy.data.objects['body'].data.materials.clear()
        bpy.data.objects['body'].data.materials.append(loaded.materials[0])
    assert uses_linked_libraries(bpy)
    source = OUT / f'linked-{kind}.blend'
    bpy.ops.wm.save_as_mainfile(filepath=str(source))
    before = sha(source)
    for action in [lambda: import_source(str(source)),
                   lambda: snapshot_blend(bpy, source, OUT / f'bad-{kind}.blend')]:
        try:
            action()
        except ValueError as error:
            assert 'linked' in str(error).lower(), str(error)
        else:
            raise AssertionError('Actual linked data was accepted')
    assert sha(source) == before and sha(library) == library_hash
    cases.append({'case': f'linked-{kind}-rejected-with-source-unchanged', 'passed': True})

fixture()
meshes, rigs = import_source(str(library), clear=False)
assert meshes and rigs and not uses_linked_libraries(bpy)
assert all(block.library is None for block in bpy.data.user_map())
assert sha(library) == library_hash
cases.append({'case': 'local-append-accepted-with-source-unchanged', 'passed': True})
(OUT / 'results.json').write_text(json.dumps({'blender': bpy.app.version_string,
                                            'cases': cases}, indent=2))
print(json.dumps(cases, indent=2))
