"""Real Blender API tests for explicit static shape conversion.

Uses generated fixtures; exercises the same worker entry point as the desktop
bridge. Does not qualify native Windows, the user's TangTang file, or the game.
"""
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(ROOT),str(ROOT/'scripts'),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
import bpy
import numpy as np
from test_blender_preparation import fixture
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import skeleton
from worker import import_source, describe, main as worker_main
from static_face import apply_static_shapes
from algorithms import suggest_mapping, identify
from deadlock_builder.preparation.target_skeleton import prepare_target
from deadlock_builder.preparation.export import export_dmx

OUT=Path(os.environ.get('DMB_SHAPES_TEST_OUTPUT',str(ROOT/'build/blender-static-shapes'))).resolve()
OUT.mkdir(parents=True,exist_ok=True)
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
results=[]

def points(data):return np.asarray([list(v.co) for v in data],dtype=float)

def streams(obj):
    return {'faces':[(tuple(p.vertices),p.material_index) for p in obj.data.polygons],
            'uvs':[[list(d.uv) for d in layer.data] for layer in obj.data.uv_layers],
            'groups':[[ (g.group,g.weight) for g in v.groups] for v in obj.data.vertices],
            'materials':[s.material.name for s in obj.material_slots]}

def source(rigged=True):
    body=fixture(rigged=rigged)[0]
    basis=body.shape_key_add(name='Basis');a=body.shape_key_add(name='Smile')
    for v in a.data:v.co.y+=.025
    a.value=.4
    b=body.shape_key_add(name='Relative detail');b.relative_key=a
    for dst,src in zip(b.data,a.data):dst.co=src.co;dst.co.x+=.01
    b.value=.6
    muted=body.shape_key_add(name='Muted');muted.value=1;muted.mute=True
    for v in muted.data:v.co.z+=.05
    mask=body.vertex_groups.new(name='shape_mask')
    mask.add([0],.5,'REPLACE');b.vertex_group=mask.name
    return body

for mode in ['current_static','basis_static']:
    body=source();before=streams(body)
    expected=points(body.data.shape_keys.reference_key.data)
    if mode=='current_static':
        expected[:,1]+=.025*.4;expected[0,0]+=.01*.6*.5
    rig=next(o for o in bpy.context.scene.objects if o.type=='ARMATURE')
    rig.pose.bones[0].rotation_mode='XYZ';rig.pose.bones[0].rotation_euler.y=.5
    report=apply_static_shapes([body],mode)
    np.testing.assert_allclose(points(body.data.vertices),expected,atol=1e-6)
    assert streams(body)==before and body.data.shape_keys is None
    assert report[0]['shape_keys_baked']==3 and not report[0]['runtime_morphs_exported']
    assert body.modifiers[0].object==rig and abs(rig.pose.bones[0].rotation_euler.y-.5)<1e-6
    results.append({'case':'masked-relative-muted-'+mode,'passed':True})

body=source();key=body.data.shape_keys.key_blocks['Smile'];key.driver_add('value').driver.expression='0.25'
bpy.context.scene.frame_set(7);bpy.context.view_layer.update()
expected=points(body.data.shape_keys.reference_key.data);expected[:,1]+=.025*.25;expected[0,0]+=.01*.6*.5
apply_static_shapes([body],'current_static')
np.testing.assert_allclose(points(body.data.vertices),expected,atol=1e-6)
results.append({'case':'current-evaluated-driver-value','passed':True})

# Native absolute-key interpolation must match Blender's evaluated rest mesh.
body=source(rigged=False);keys=body.data.shape_keys;keys.use_relative=False;keys.eval_time=5
bpy.context.view_layer.update()
evaluated=body.evaluated_get(bpy.context.evaluated_depsgraph_get());mesh=evaluated.to_mesh()
expected=points(mesh.vertices);evaluated.to_mesh_clear()
apply_static_shapes([body],'current_static')
np.testing.assert_allclose(points(body.data.vertices),expected,atol=1e-6)
results.append({'case':'absolute-key-current-mix','passed':True})

for rigged in [True,False]:
    for mode in ['current_static','basis_static']:
        directory=OUT/(('rigged' if rigged else 'unrigged')+'-'+mode);directory.mkdir(exist_ok=True)
        body=source(rigged);path=directory/'source.blend'
        bpy.ops.wm.save_as_mainfile(filepath=str(path));original=sha(path)
        meshes,rigs=import_source(str(path),allow_shape_keys=True);info=describe(meshes,rigs)
        assert len(info['morphs'])==3
        target,_=prepare_target(skeleton());suggest=suggest_mapping(info['bones'],target) if rigged else {'target':identify(target)}
        options={'reviewed':True,'target_roles':suggest['target']['roles'],'shape_key_mode':mode}
        if rigged:options.update(mapping=suggest['mapping'],source_roles=suggest['source']['roles'])
        else:options['landmarks']=info['humanoid']['landmarks']
        job={'operation':'prepare','source':str(path),'mode':'rigged_humanoid' if rigged else 'unrigged_humanoid',
             'target':target,'options':options,'hero_root':'models/test','workspace':str(directory),
             'result':str(directory/'result.json'),'correction':False}
        job_path=directory/'job.json';job_path.write_text(json.dumps(job),encoding='utf-8')
        previous=sys.argv;sys.argv=['worker.py','--',str(job_path)]
        try:worker_main()
        finally:sys.argv=previous
        report=json.loads((directory/'result.json').read_text())
        assert report['status']=='ok' and report['static_shapes'][0]['mode']==mode
        assert any('Animated morphs' in warning for warning in report['warnings'])
        exported=export_dmx(report,skeleton(),directory/'prepared.dmx')
        assert exported['target_bind_audit']['world_inverse_bind_consistent']
        assert sha(path)==original
        bpy.ops.wm.open_mainfile(filepath=report['blend'],load_ui=False,use_scripts=False)
        assert all(not obj.data.shape_keys for obj in bpy.context.scene.objects if obj.type=='MESH')
        # Re-export the accepted correction without reapplying a morph delta.
        if rigged:
            corrected=dict(job,source=report['blend'],correction=True)
            corrected['options']=dict(options,mapping={bone['name']:bone['name'] for bone in target},source_roles=options['target_roles'])
            job_path.write_text(json.dumps(corrected),encoding='utf-8');sys.argv=['worker.py','--',str(job_path)]
            try:worker_main()
            finally:sys.argv=previous
            correction=json.loads((directory/'result.json').read_text())
            for first,second in zip(report['meshes'],correction['meshes']):
                np.testing.assert_allclose(first['positions'],second['positions'],atol=1e-5)
        results.append({'case':directory.name+'-worker-dmx-correction','passed':True,'source_unchanged':True})

body=source();path=OUT/'unreviewed.blend';bpy.ops.wm.save_as_mainfile(filepath=str(path));before=sha(path)
try:import_source(str(path))
except ValueError as error:assert 'Shape key handling' in str(error)
else:raise AssertionError('Unreviewed morphs were silently discarded')
assert sha(path)==before
results.append({'case':'unreviewed-source-stays-blocked-and-unchanged','passed':True})
(OUT/'results.json').write_text(json.dumps({'blender':bpy.app.version_string,'cases':results},indent=2)+'\n')
print('PASS:',len(results),'static shape cases on',bpy.app.version_string)
