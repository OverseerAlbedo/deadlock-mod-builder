"""Real Blender rest-alignment regressions; no Valve or live-game claims.

Optionally uses a supplied source GLB and analysis JSON with a real hero skeleton.
Without those it uses the same generated humanoid fixtures as the bridge suite.
"""
import os,sys,json,copy
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];sys.path[:0]=[str(ROOT),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender'),str(ROOT/'scripts')]
import bpy,numpy as np
from mathutils import Matrix
from worker import import_source,describe,prepare,auto_weight
from algorithms import suggest_mapping,identify
from test_blender_preparation import fixture
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid,skeleton
from deadlock_builder.preparation.export import export_dmx
from deadlock_builder.preparation.blender_scripts.retarget import POLICY

out=Path(os.environ['DMB_PREP_TEST_OUTPUT'])/'alignment';out.mkdir(parents=True,exist_ok=True)
source_env=os.environ.get('DMB_ALIGNMENT_SOURCE');analysis_env=os.environ.get('DMB_ALIGNMENT_ANALYSIS');options_env=os.environ.get('DMB_ALIGNMENT_OPTIONS')
if source_env:
    source=Path(source_env);analysis=json.loads(Path(analysis_env).read_text(encoding='utf-8'))
    target=analysis['target'];target_sk=analysis['target_skeleton'];opts=json.loads(Path(options_env).read_text(encoding='utf-8'))
else:
    fixture();source=out/'source.glb';bpy.ops.export_scene.gltf(filepath=str(source),export_format='GLB')
    target=humanoid();target_sk=skeleton();meshes,rigs=import_source(str(source));s=suggest_mapping(describe(meshes,rigs)['bones'],target)
    opts={'reviewed':True,'mapping':s['mapping'],'source_roles':s['source']['roles'],'target_roles':s['target']['roles'],'influence_limit':4}
results=[];reports={}
for units in (1., 100.):
    meshes,rigs=import_source(str(source));info=describe(meshes,rigs)
    positions={b['name']:b['position'] for b in info['bones']}
    landmarks={role:(np.array(positions[name])*units).tolist() for role,name in opts['source_roles'].items()}
    for obj in meshes:
        world=obj.matrix_world.copy();obj.parent=None;obj.matrix_world=Matrix.Scale(units,4)@world;obj.modifiers.clear();obj.vertex_groups.clear()
    for rig in rigs:bpy.data.objects.remove(rig,do_unlink=True)
    fitted=auto_weight(meshes,target,opts['target_roles'],landmarks)
    height=np.linalg.norm(np.array(landmarks['head'])-landmarks['foot_L'])
    maximum=0.
    for role,name in opts['target_roles'].items():
        b=fitted.data.bones[name];length=float(b.length);maximum=max(maximum,length/height)
        np.testing.assert_allclose(b.head_local,landmarks[role],atol=1e-4)
        assert .0001 < length/height < .65, ('Temporary fitted bone length in wrong units',role,length/height)
    results.append({'case':'unrigged-fitted-bone-units-'+str(units),'passed':True,'max_primary_length_height_ratio':maximum})
for mode,roll in [('rigged_humanoid',False),('rigged_humanoid',True),('unrigged_humanoid',False)]:
    meshes,rigs=import_source(str(source));options=copy.deepcopy(opts);case=mode+('-rolled' if roll else '')
    before={o.name:{'faces':[(list(p.vertices),p.material_index) for p in o.data.polygons],
                     'uvs':[[list(v.uv) for v in layer.data] for layer in o.data.uv_layers]} for o in meshes}
    if roll:
        rig=rigs[0];bpy.context.view_layer.objects.active=rig;rig.select_set(True);bpy.ops.object.mode_set(mode='EDIT')
        for i,b in enumerate(rig.data.edit_bones):b.roll+=(i+1)*.217
        bpy.ops.object.mode_set(mode='OBJECT')
    if mode=='unrigged_humanoid':
        info=describe(meshes,rigs);positions={b['name']:b['position'] for b in info['bones']}
        options['landmarks']={role:positions[name] for role,name in options['source_roles'].items()}
        for o in meshes:
            world=o.matrix_world.copy();o.parent=None;o.matrix_world=world;o.modifiers.clear();o.vertex_groups.clear()
        for rig in rigs:bpy.data.objects.remove(rig,do_unlink=True)
        rigs=[]
    dest=out/case;dest.mkdir(exist_ok=True)
    report=prepare({'mode':mode,'target':target,'options':options,'workspace':str(dest),'hero_root':'models/test/hero'},meshes,rigs)
    export=export_dmx(report,target_sk,dest/'prepared.dmx')
    assert report['rest_alignment']['policy']==POLICY
    # The final Blender armature must not deform the exported rest surface.
    bpy.context.view_layer.update();deps=bpy.context.evaluated_depsgraph_get();delta=0.
    for obj,m in zip(meshes,report['meshes']):
        assert [(list(p.vertices),p.material_index) for p in obj.data.polygons]==before[obj.name]['faces']
        assert [[list(v.uv) for v in l.data] for l in obj.data.uv_layers]==before[obj.name]['uvs']
        evaluated=obj.evaluated_get(deps);mesh=evaluated.to_mesh()
        pts=np.array([list(evaluated.matrix_world@v.co) for v in mesh.vertices]);evaluated.to_mesh_clear()
        error=float(np.max(np.linalg.norm(pts-np.array(m['positions']),axis=1)));delta=max(delta,error)
        assert error<.001,('Rest bind mismatch',error)
        assert all(abs(sum(w for n,w in row)-1)<1e-6 for row in m['weights'])
        assert np.isfinite(pts).all()
    if roll:
        for m,reference in zip(report['meshes'],reports['rigged_humanoid']['meshes']):
            np.testing.assert_allclose(m['positions'],reference['positions'],atol=.0001,rtol=1e-6)
            np.testing.assert_allclose(m['normals'],reference['normals'],atol=.0001,rtol=1e-6)
            assert m['weights']==reference['weights']
    (dest/'report.json').write_text(json.dumps(report,allow_nan=False),encoding='utf-8');reports[case]=report
    results.append({'case':case,'passed':True,'vertices':sum(len(m['positions']) for m in report['meshes']),
                    'triangles':export['geometry']['source_triangles'],'rest_bind_max_distance':delta,
                    'pose_probe_status':report['deformation_status'],'all_vertices_weighted':True,'topology_and_uvs_unchanged':True,
                    'source_axis_roll_invariant':True if roll else None})
# Correction must be idempotent when the target rig is already in its own rest pose.
previous=reports['rigged_humanoid'];meshes,rigs=import_source(previous['blend']);dest=out/'correction';dest.mkdir(exist_ok=True)
options=copy.deepcopy(opts);options['source_roles']=options['target_roles'];options['mapping']={b['name']:b['name'] for b in target}
report=prepare({'correction':True,'mode':'rigged_humanoid','target':target,'options':options,'workspace':str(dest),'hero_root':'models/test/hero'},meshes,rigs)
for m,old in zip(report['meshes'],previous['meshes']):np.testing.assert_allclose(m['positions'],old['positions'],atol=.0001,rtol=1e-6)
results.append({'case':'correction-idempotence','passed':True})
(out/'results.json').write_text(json.dumps({'blender':bpy.app.version_string,'source_kind':'provided reference' if source_env else 'generated fixture','native_compiler_tested':False,'live_game_tested':False,'cases':results},indent=2),encoding='utf-8')
print(json.dumps(results,indent=2))
