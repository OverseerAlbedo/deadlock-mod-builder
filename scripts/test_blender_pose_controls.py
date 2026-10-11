"""Rest-only rig import accepts pose controls and rejects world-transform controls."""
import os,sys,json,hashlib
from pathlib import Path
import numpy as np
ROOT=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(ROOT),str(ROOT/'scripts'),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
import bpy
from test_blender_preparation import fixture
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid
import worker
from algorithms import suggest_mapping
OUT=Path(os.environ['DMB_PREP_TEST_OUTPUT']);OUT.mkdir(parents=True,exist_ok=True)
cases=[]
for case in ['pose-drivers','object-driver','object-constraint','data-driver','bone-parent','parent-driver','unresolved-driver']:
    meshes=fixture();rig=next(o for o in bpy.context.scene.objects if o.type=='ARMATURE')
    bone=rig.pose.bones['head']
    if case in ['pose-drivers','bone-parent','parent-driver']:
        curve=bone.driver_add('rotation_euler',0);curve.driver.expression='0.5'
        constraint=bone.constraints.new('COPY_ROTATION');constraint.target=rig;constraint.subtarget=bone.parent.name
        curve=constraint.driver_add('influence');curve.driver.expression='0.5'
        if case=='bone-parent':meshes[0].parent_type='BONE';meshes[0].parent_bone='head'
        if case=='parent-driver':
            parent=bpy.data.objects.new('controlled parent',None);bpy.context.collection.objects.link(parent);rig.parent=parent
            parent.driver_add('location',0).driver.expression='1'
    elif case=='unresolved-driver':
        curve=bone.driver_add('scale',0);curve.data_path='pose.bones["missing"].scale';curve.driver.expression='1'
    elif case=='object-driver':rig.driver_add('location',0).driver.expression='2'
    elif case=='object-constraint':rig.constraints.new('LIMIT_LOCATION')
    else:rig.data.bones['head'].driver_add('bbone_x').driver.expression='0.2'
    bpy.context.view_layer.update()
    saved=OUT/(case+'.blend');bpy.ops.wm.save_as_mainfile(filepath=str(saved))
    digest=hashlib.sha256(saved.read_bytes()).hexdigest()
    m,r=worker.import_source(str(saved));before={o.name:[list(o.matrix_world@v.co) for v in o.data.vertices] for o in m}
    try:info=worker.describe(m,r)
    except ValueError as e:
        assert case!='pose-drivers',str(e)
        cases.append({'case':case,'rejected':str(e),'passed':True});continue
    assert case=='pose-drivers'
    assert info['rest_rig_controls']['pose_drivers']==2
    mapping=suggest_mapping(info['bones'],humanoid());assert not mapping['source']['errors']
    options={'reviewed':True,'mapping':mapping['mapping'],'source_roles':mapping['source']['roles'],'target_roles':mapping['target']['roles']}
    out=OUT/case;out.mkdir(exist_ok=True)
    result=worker.prepare({'mode':'rigged_humanoid','options':options,'target':humanoid(),'workspace':str(out),'hero_root':'models/test/pose_controls'},m,r)
    for mesh in result['meshes']:np.testing.assert_allclose(mesh['positions'],before[mesh['name']],atol=1e-5)
    m,r=worker.import_source(result['blend']);info=worker.describe(m,r)
    assert info['rest_rig_controls']['pose_drivers']==0 and info['rest_rig_controls']['pose_constraints']==0
    assert hashlib.sha256(saved.read_bytes()).hexdigest()==digest
    cases.append({'case':case,'geometry_and_source_preserved':True,'passed':True})
(OUT/'pose-controls.json').write_text(json.dumps({'blender':bpy.app.version_string,'cases':cases},indent=2))
print(json.dumps({'passed':len(cases),'blender':bpy.app.version_string}))
