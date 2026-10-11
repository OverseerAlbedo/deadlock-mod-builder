"""Real Blender fitted-garment integration; no native compiler/game claims.

Run with Blender --background --factory-startup --disable-autoexec
--python-exit-code 23 --python scripts/test_blender_clothing.py.
"""
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))
sys.path.insert(0,str(ROOT/'scripts'))
sys.path.insert(0,str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender'))
import bpy
from worker import import_source,import_garment,describe,prepare
from blend_source import snapshot_blend
from test_blender_preparation import fixture
from algorithms import suggest_mapping,identify
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid,skeleton
from deadlock_builder.preparation.export import export_dmx

OUT=Path(os.environ.get('DMB_PREP_TEST_OUTPUT',str(ROOT/'build/blender-clothing-test'))).resolve()
OUT.mkdir(parents=True,exist_ok=True)
results=[]

for rigged in (True,False):
    directory=OUT/('rigged' if rigged else 'unrigged')
    directory.mkdir(exist_ok=True)
    source=directory/'character.blend'
    garment=directory/'clothing.blend'
    fixture('target',rigged=rigged)
    bpy.ops.wm.save_as_mainfile(filepath=str(source))
    fixture('target',rigged=rigged,clothes=True)
    bpy.data.objects.remove(bpy.data.objects['body'],do_unlink=True)
    bpy.context.view_layer.update()
    coat = bpy.data.objects['coat']
    garment_positions = [coat.matrix_world @ vertex.co for vertex in coat.data.vertices]
    bpy.ops.wm.save_as_mainfile(filepath=str(garment))
    before={str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in (source,garment)}
    meshes,rigs=import_source(str(source))
    meshes,rigs=import_garment(str(garment),meshes,rigs)
    coat = next(obj for obj in meshes if obj.name == 'coat')
    assert all((coat.matrix_world @ vertex.co - expected).length < 1e-6
               for vertex, expected in zip(coat.data.vertices, garment_positions))
    assert len(meshes)==2
    assert len(rigs)==int(rigged)
    info=describe(meshes,rigs)
    target=humanoid()
    mapping=suggest_mapping(info['bones'],target) if rigs else {'target':identify(target)}
    options={'reviewed':True,'target_roles':mapping['target']['roles'],'influence_limit':4}
    if rigs:options.update(mapping=mapping['mapping'],source_roles=mapping['source']['roles'])
    else:options['landmarks']=info['humanoid']['landmarks']
    report=prepare({'source':str(source),'correction':False,
                    'mode':'rigged_humanoid' if rigged else 'unrigged_humanoid',
                    'target':target,'options':options,'workspace':str(directory),
                    'hero_root':'models/test/hero'},meshes,rigs)
    receipt=export_dmx(report,skeleton(),directory/'prepared.dmx')
    assert len(report['meshes'])==2
    assert receipt['skinning']['numeric_checks_passed']
    assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest()==sha for path,sha in before.items())
    results.append({'case':directory.name,'passed':True,'meshes':len(report['meshes']),
                    'skinning_checks':True,'original_files_unchanged':True})

# The source-relative texture must survive moving the staged Blender snapshot.
fixture('target',rigged=True)
image=bpy.data.images.new('Generated texture fixture',width=8,height=8)
image.filepath_raw=str(OUT/'fabric.png');image.file_format='PNG';image.save()
bpy.data.images.remove(image)
image=bpy.data.images.load(str(OUT/'fabric.png'));image.name='Fabric'
material=bpy.data.objects['body'].data.materials[0]
material.use_nodes=True
texture=material.node_tree.nodes.new('ShaderNodeTexImage');texture.image=image
material.node_tree.links.new(texture.outputs['Color'],material.node_tree.nodes.get('Principled BSDF').inputs['Base Color'])
image.filepath='//fabric.png'
source=OUT/'relative-texture.blend'
bpy.ops.wm.save_as_mainfile(filepath=str(source))
before=hashlib.sha256(source.read_bytes()).hexdigest()
import_source(str(source))
snapshot=OUT/'relocated/source.blend'
proof=snapshot_blend(bpy,source,snapshot)
assert proof['packed_images']
(OUT/'fabric.png').unlink()
import_source(str(snapshot))
assert bpy.data.images['Fabric'].packed_file
assert hashlib.sha256(source.read_bytes()).hexdigest()==before
results.append({'case':'relative-texture-packing','passed':True})

(OUT/'clothing-results.json').write_text(json.dumps({'blender':bpy.app.version_string,
    'native_compiler_tested':False,'live_game_tested':False,'cases':results},indent=2),encoding='utf-8')
print(json.dumps(results,indent=2))
