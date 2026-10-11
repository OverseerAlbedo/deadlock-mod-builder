"""Real Blender imports: unused rigs, shared rest rigs and genuine ambiguity."""
from pathlib import Path
import sys,os,json,hashlib
import numpy as np
ROOT=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(ROOT),str(ROOT/'scripts'),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
import bpy
from test_blender_preparation import fixture
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid
from worker import import_source,describe,prepare
from algorithms import suggest_mapping
OUT=Path(os.environ['DMB_PREP_TEST_OUTPUT']);OUT.mkdir(parents=True,exist_ok=True)
results=[]

def capture(meshes):
    return {o.name:{'points':[list(o.matrix_world@v.co) for v in o.data.vertices],
        'faces':[list(f.vertices) for f in o.data.polygons],
        'weights':[[[o.vertex_groups[g.group].name,g.weight] for g in v.groups] for v in o.data.vertices]} for o in meshes}

for case in ('unused-different-rig','equivalent-outfit-rig','two-characters','unbound-ambiguous-weights','unrigged-with-authoring-rigs'):
    meshes=fixture(rigged=case!='unrigged-with-authoring-rigs')
    if case=='unrigged-with-authoring-rigs':
        rigs=[]
        for n in ('guide-a','guide-b'):
            o=bpy.data.objects.new(n,bpy.data.armatures.new(n));bpy.context.collection.objects.link(o);rigs.append(o)
    else:
        rig=next(o for o in bpy.context.scene.objects if o.type=='ARMATURE')
        other=rig.copy();other.data=rig.data.copy();bpy.context.collection.objects.link(other)
        if case=='unused-different-rig':
            other.location.x=7
            for i,b in enumerate(other.data.bones):b.name='control_'+str(i)
        if case in ('equivalent-outfit-rig','two-characters'):
            for obj in list(meshes):
                duplicate=obj.copy();duplicate.data=obj.data.copy();bpy.context.collection.objects.link(duplicate)
                duplicate.parent=other
                for m in duplicate.modifiers:
                    if m.type=='ARMATURE':m.object=other
                meshes.append(duplicate)
            if case=='two-characters':other.location.x+=3
        if case=='unbound-ambiguous-weights':
            for o in meshes:o.parent=None;o.modifiers.clear()
    bpy.context.view_layer.update()
    before=capture(meshes)
    source=OUT/(case+'.blend');bpy.ops.wm.save_as_mainfile(filepath=str(source))
    digest=hashlib.sha256(source.read_bytes()).hexdigest()
    try:
        receipts=[];imported,selected=import_source(str(source),helper_reports=receipts)
    except ValueError as e:
        assert case in ('two-characters','unbound-ambiguous-weights'),str(e)
        assert 'multiple armatures' in str(e),str(e)
        results.append({'case':case,'passed':True,'rejected':str(e)})
        continue
    assert case not in ('two-characters','unbound-ambiguous-weights')
    assert capture(imported)==before
    assert len(selected)==(0 if case=='unrigged-with-authoring-rigs' else 1)
    assert receipts and receipts[-1]['policy']=='render-bound-source-rig-v1'
    if selected:
        info=describe(imported,selected);mapping=suggest_mapping(info['bones'],humanoid())
        work=OUT/case;work.mkdir(exist_ok=True)
        options={'reviewed':True,'source_roles':mapping['source']['roles'],'target_roles':mapping['target']['roles'],'mapping':mapping['mapping']}
        result=prepare({'source':str(source),'mode':'rigged_humanoid','options':options,'target':humanoid(),'workspace':str(work),'hero_root':'models/test'},imported,selected)
        assert len(result['meshes'])==len(before)
        for mesh in result['meshes']:
            np.testing.assert_allclose(mesh['positions'],before[mesh['name']]['points'],atol=1e-5)
        corrected,rigs=import_source(result['blend']);assert len(corrected)==len(before) and len(rigs)==1
    assert hashlib.sha256(source.read_bytes()).hexdigest()==digest
    results.append({'case':case,'passed':True,'meshes_retained':len(imported),'receipt':receipts[-1]})
(OUT/'rig-results.json').write_text(json.dumps({'blender':bpy.app.version_string,'cases':results},indent=2)+'\n')
print(json.dumps({'passed':len(results),'cases':[r['case'] for r in results]}))
