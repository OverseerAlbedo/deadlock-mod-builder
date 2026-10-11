"""Run inside Blender: metadata groups must not become skin influences.

Uses generated models; DMB_MMD_SOURCE optionally adds a read-only user-source check.
"""
import hashlib
import json
import os
from pathlib import Path
import sys
import bpy
ROOT=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(ROOT),str(ROOT/'scripts'),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
from test_blender_preparation import fixture
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid
from worker import describe,import_source,prepare
from algorithms import suggest_mapping
OUT=Path(os.environ.get('DMB_PREP_TEST_OUTPUT',str(ROOT/'build/skin-group-test')));OUT.mkdir(parents=True,exist_ok=True)
results=[]

def convert(name,metadata=False,empty=False):
    body=fixture()[0];rig=body.parent
    if metadata:
        for group_name in ['mmd_edge_scale','mmd_vertex_order','ordinary_mesh_mask']:
            group=body.vertex_groups.new(name=group_name)
            group.add(list(range(len(body.data.vertices))),.75,'REPLACE')
        bpy.context.view_layer.objects.active=rig;bpy.ops.object.mode_set(mode='EDIT')
        control=rig.data.edit_bones.new('nondeforming_control');control.head=(0,0,0);control.tail=(0,0,.1)
        bpy.ops.object.mode_set(mode='OBJECT');rig.data.bones['nondeforming_control'].use_deform=False
        body.vertex_groups.new(name='nondeforming_control').add(list(range(len(body.data.vertices))),1.,'REPLACE')
    if empty:
        for group in body.vertex_groups:
            if group.name in rig.data.bones and rig.data.bones[group.name].use_deform:group.remove([0])
    info=describe([body],[rig]);target=humanoid();suggest=suggest_mapping(info['bones'],target)
    options={'reviewed':True,'mapping':suggest['mapping'],'source_roles':suggest['source']['roles'],'target_roles':suggest['target']['roles'],'influence_limit':4}
    if metadata:
        assert not set(info['weighted_bones']) & {'mmd_edge_scale','mmd_vertex_order','ordinary_mesh_mask','nondeforming_control'}
        assert all(n in info['warnings'][0] for n in ['mmd_edge_scale','mmd_vertex_order','ordinary_mesh_mask','nondeforming_control'])
    folder=OUT/name;folder.mkdir(exist_ok=True)
    return prepare({'mode':'rigged_humanoid','options':options,'target':target,'workspace':str(folder),'hero_root':'models/test/hero'},[body],[rig])

baseline=convert('baseline');metadata=convert('with-metadata',True)
assert baseline['meshes']==metadata['meshes']
assert baseline['weights']==metadata['weights']
results.append({'case':'metadata-and-nondeforming-groups-do-not-change-converted-geometry-or-skin','passed':True})
try:convert('metadata-only-vertex',True,True)
except ValueError as error:assert 'unweighted' in str(error).lower(),str(error)
else:raise AssertionError('Metadata concealed an unweighted vertex')
results.append({'case':'metadata-cannot-conceal-an-unweighted-vertex','passed':True})
source=os.environ.get('DMB_MMD_SOURCE')
if source:
    source=Path(source);before=hashlib.sha256(source.read_bytes()).hexdigest()
    meshes,rigs=import_source(str(source),allow_shape_keys=True);info=describe(meshes,rigs)
    actual={b['name'] for b in info['bones'] if b['deform']}
    assert set(info['weighted_bones'])<=actual
    assert 'mmd_edge_scale' not in info['weighted_bones'] and 'mmd_vertex_order' not in info['weighted_bones']
    assert before==hashlib.sha256(source.read_bytes()).hexdigest()
    (OUT/'inspection.json').write_text(json.dumps(info,ensure_ascii=False,indent=2))
    results.append({'case':'read-only-user-source-skin-group-classification','passed':True,'weighted_bones':len(info['weighted_bones']),'source_sha256':before,'warnings':info.get('warnings',[])})
(OUT/'results.json').write_text(json.dumps({'blender':bpy.app.version_string,'cases':results},indent=2,ensure_ascii=False))
print(json.dumps(results,indent=2,ensure_ascii=False))
