"""Real Blender regression: preview paths and helper offsets on generated models.

Never reads or changes a user model. Run with --background --factory-startup
--disable-autoexec --python-exit-code 23 --python <this script>.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import numpy as np
import bpy

ROOT=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(ROOT),str(ROOT/'scripts'),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
from test_blender_preparation import fixture
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid
from worker import describe,prepare
from algorithms import suggest_mapping,deformation_checks
from retarget import rest_maps

OUT=Path(os.environ.get('DMB_MAPPING_TEST_OUTPUT',str(ROOT/'build/mapping-offset-test'))).resolve()
OUT.mkdir(parents=True,exist_ok=True)
cases=[]

def convert(name, helper=False, preserve=False):
    body=fixture()[0];rig=body.parent
    if helper:
        bpy.context.view_layer.objects.active=rig
        bpy.ops.object.mode_set(mode='EDIT')
        anchor=rig.data.edit_bones['forearm_L'];bone=rig.data.edit_bones.new('forearm_twist')
        bone.head=anchor.head.copy();bone.head.y+=.08;bone.tail=bone.head+type(bone.head)((0,.1,0));bone.parent=anchor
        bpy.ops.object.mode_set(mode='OBJECT')
        group=body.vertex_groups['forearm_L'];indices=[v.index for v in body.data.vertices if any(g.group==group.index and g.weight>0 for g in v.groups)]
        assert indices
        extra=body.vertex_groups.new(name='forearm_twist');extra.add(indices,1.,'REPLACE');group.remove(indices)
    info=describe([body],[rig]);target=humanoid();suggest=suggest_mapping(info['bones'],target)
    options={'reviewed':True,'mapping':suggest['mapping'],'source_roles':suggest['source']['roles'],'target_roles':suggest['target']['roles'],'influence_limit':4}
    if helper:
        options['mapping']['forearm_twist']='forearm_L'
        assert next(b for b in info['bones'] if b['name']=='forearm_twist')['skin']['vertices']==len(indices)
        if preserve:options.update(mapping_anchors={'forearm_twist':'forearm_L'},follow_motion_reviewed=True)
    folder=OUT/name;folder.mkdir(exist_ok=True)
    return prepare({'mode':'rigged_humanoid','options':options,'target':target,'workspace':str(folder),'hero_root':'models/test/hero'},[body],[rig])

baseline=convert('baseline');following=convert('following',True,True);direct=convert('legacy-direct',True,False)
for key in ['positions','normals','uvs','weights','faces']:
    a,b=baseline['meshes'][0][key],following['meshes'][0][key]
    if key in ['positions','normals']:np.testing.assert_allclose(a,b,atol=1e-10)
    else:assert a==b,key
delta=float(np.max(np.abs(np.asarray(baseline['meshes'][0]['positions'])-np.asarray(direct['meshes'][0]['positions']))))
assert delta>.05
assert following['rest_alignment']['policy']=='ancestor-offset-preservation-v1'
cases.append({'case':'helper-offset-keeps-baseline-geometry-and-weights','passed':True,'legacy_direct_displacement':delta})

bones=humanoid();source={b['name']:np.asarray(b['matrix']) for b in bones};parents={b['name']:b['parent'] for b in bones};roles={n:n for n in source}
source['helper']=source['forearm_L'].copy();parents['helper']='forearm_L';mapping=roles|{'helper':'forearm_L'}
for anchors in [{'helper':'hand_L'},{'helper':'helper'},{'forearm_L':'upperarm_L'}]:
    try:rest_maps(source,source,mapping,roles,roles,parents,anchors)
    except ValueError:pass
    else:raise AssertionError('Invalid following anchor accepted: '+str(anchors))
cases.append({'case':'invalid-and-primary-anchors-rejected','passed':True})
turn=np.eye(4);turn[:2,:2]=[[0,-1],[1,0]]
probes=deformation_checks([[0,0,0],[1,0,0],[0,1,0]],[[0,1,2]],[[('parent',1.)],[('child',1.)],[('child',1.)]],{'parent':{'parent':turn,'child':turn}})
assert probes[0]['severe_edge_strain']==0
cases.append({'case':'movement-probe-moves-entire-target-branch','passed':True})

spec=importlib.util.spec_from_file_location('preview_export',ROOT/'src-tauri/crates/deadlock-preview/src/preview_export.py')
export=importlib.util.module_from_spec(spec);spec.loader.exec_module(export)
fixture();original=OUT/'original generated source.blend';bpy.ops.wm.save_as_mainfile(filepath=str(original))
before=hashlib.sha256(original.read_bytes()).hexdigest()
deep=OUT/('long directory with spaces '*3)/('long directory with spaces '*3)/('日本語 preview '*5)
if os.name=='nt':deep=Path('\\\\?\\'+str(deep))
deep.mkdir(parents=True,exist_ok=True);output=deep/'scene.glb';job=deep/'job.json';receipt=deep/'receipt.json'
job.write_text(json.dumps({'source':str(original),'output':str(output),'receipt':str(receipt)},ensure_ascii=False),encoding='utf-8')
prior=os.getcwd();sys.argv=['preview_export.py','--',str(job)];export.main()
assert output.read_bytes()[:4]==b'glTF' and json.loads(receipt.read_text(encoding='utf-8'))['status']=='ok'
assert os.getcwd()==prior and hashlib.sha256(original.read_bytes()).hexdigest()==before
seen={}
def failure(**settings):
    seen.update(settings)
    assert not os.path.isabs(settings['filepath']) and os.path.dirname(settings['filepath'])=='.'
    raise RuntimeError('expected exporter failure')
try:export.export_glb(failure,output)
except RuntimeError as error:assert str(error)=='expected exporter failure'
else:raise AssertionError('Exporter failure swallowed')
assert os.getcwd()==prior
cases.append({'case':'real-gltf-export-deep-unicode-path-source-unchanged-cwd-restored','passed':True,'output_path_length':len(str(output)),'native_windows':os.name=='nt','source_sha256':before})
(OUT/'results.json').write_text(json.dumps({'blender':bpy.app.version_string,'cases':cases},indent=2),encoding='utf-8')
print(json.dumps(cases,indent=2))
