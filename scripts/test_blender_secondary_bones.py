"""Real bpy proof of rig preservation, not Source 2 simulation qualification.

Optional DMB_SECONDARY_SOURCE points to a user-owned source; it is never saved.
The TangTang test uses an older 95-joint positional fixture with generated unit
rotations/scales, not an authoritative current compiled Haze bind. External
textures are omitted only in these disposable test scenes.
"""
import copy, hashlib, json, os, sys
from pathlib import Path
import numpy as np
import bpy
from mathutils import Matrix, Quaternion
ROOT=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(ROOT),str(ROOT/'scripts'),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
from worker import import_source, describe, prepare
from secondary_bones import physics_groups, selection, PREFIX
from exact_bind import _local_matrix, canonical_bones
from static_face import apply_static_shapes
OUT=Path(os.environ.get('DMB_SECONDARY_OUTPUT',ROOT/'build/secondary-bones')).resolve();OUT.mkdir(parents=True,exist_ok=True)
source_path=Path(os.environ['DMB_SECONDARY_SOURCE']) if os.environ.get('DMB_SECONDARY_SOURCE') else None
project_path=Path(os.environ['DMB_SECONDARY_PROJECT']) if os.environ.get('DMB_SECONDARY_PROJECT') else None
cases=[]
def target_from_positions():
    rows=json.loads((Path(os.environ.get('DMB_PRIVATE_ASSETS') or ROOT/'private-assets')/'fixtures/tangtang-rig.json').read_text())['target']
    positions={b['name']:np.asarray(b['position']) for b in rows};worlds={};result=[]
    for b in rows:
        world=np.eye(4);world[:3,3]=positions[b['name']];worlds[b['name']]=world
    for b in rows:
        n,p=b['name'],b['parent'];local=np.linalg.inv(worlds[p])@worlds[n] if p else worlds[n]
        result.append({'name':n,'parent':p,'position':positions[n].tolist(),'local_position':local[:3,3].tolist(),
            'local_rotation':[0.,0.,0.,1.],'local_scale':1.,'matrix':worlds[n].tolist(),'inverse_bind_matrix':np.linalg.inv(worlds[n]).tolist(),'length':1.})
    return result

def source_scene():
    if source_path:
        meshes,rigs=import_source(str(source_path),allow_shape_keys=True)
        apply_static_shapes(meshes,'current_static')
        # Asset delivery omitted external textures. Geometry, UVs and slot names
        # remain real; turn off nodes in memory to isolate bone conversion tests.
        for m in bpy.data.materials:
            if m.node_tree:m.node_tree.nodes.clear()
            m.use_nodes=False
        options=json.loads(project_path.read_text(encoding='utf-8-sig'))['preparation_options']
        target=target_from_positions()
        # The older positional fixture has fewer fingers than the user's
        # current hero. Only in this synthetic-target test, attach these
        # missing finger destinations to their corresponding existing hand.
        destinations={b['name'] for b in target}
        for name,destination in options['mapping'].copy().items():
            if destination not in destinations:
                assert destination.startswith('finger_') and destination[-2:] in {'_L','_R'}
                hand=options['target_roles']['hand'+destination[-2:]]
                options['mapping'][name]=hand
                options.setdefault('mapping_anchors',{})[name]=options['source_roles']['hand'+destination[-2:]]
    else:
        from test_blender_preparation import fixture
        import sys as _reference_sys
        from pathlib import Path as _ReferencePath
        _reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
        from tests.preparation_fixture import humanoid
        from algorithms import suggest_mapping
        meshes=fixture();rig=meshes[0].parent;rigs=[rig]
        bpy.context.view_layer.objects.active=rig;bpy.ops.object.mode_set(mode='EDIT')
        parent=rig.data.edit_bones['pelvis']
        for n,y in [('appendage',.15),('helper',.25),('tip',.35)]:
            b=rig.data.edit_bones.new(n);b.head=(0,y,1);b.tail=(0,y+.1,1);b.parent=parent;parent=b
        bpy.ops.object.mode_set(mode='OBJECT')
        data=bpy.data.meshes.new('appendage geometry');data.from_pydata([(0,.17,1),(.05,.17,1),(0,.37,1),(.05,.37,1)],[],[(0,1,2),(1,3,2)])
        obj=bpy.data.objects.new('appendage geometry',data);bpy.context.collection.objects.link(obj);obj.parent=rig
        mod=obj.modifiers.new('Rig','ARMATURE');mod.object=rig
        obj.vertex_groups.new(name='appendage').add([0,1],1.,'REPLACE');obj.vertex_groups.new(name='tip').add([2,3],1.,'REPLACE')
        mat=bpy.data.materials.new('plain');obj.data.materials.append(mat);meshes.append(obj)
        root=bpy.data.objects.new('MMD root',None);root['mmd_tools_version']='4.5.0';bpy.context.collection.objects.link(root);rig.parent=root
        group=bpy.data.objects.new('rigidbodies',None);bpy.context.collection.objects.link(group);group.parent=root
        for n,kinematic in [('appendage',False),('helper',False),('tip',False),('upperarm_L',True)]:
            bpy.ops.mesh.primitive_cube_add(size=.01);body=bpy.context.object;body.name='rigid '+n;body.parent=group;body.hide_render=True
            bpy.ops.rigidbody.object_add();body.rigid_body.kinematic=kinematic
            c=body.constraints.new('CHILD_OF');c.name='mmd_tools_rigid_parent';c.target=rig;c.subtarget=n;c.mute=True
        target=canonical_bones(humanoid());worlds={}
        for b in target:
            b['local_scale']=1.25 if b['name']=='pelvis' else .8 if b['name']=='chest' else 1.
            local=_local_matrix(b['local_position'],b['local_rotation'],b['local_scale'])
            world=worlds[b['parent']]@local if b['parent'] else local;worlds[b['name']]=world
            b.update(matrix=world.tolist(),inverse_bind_matrix=np.linalg.inv(world).tolist(),position=world[:3,3].tolist(),length=1.)
        suggest=suggest_mapping(describe(meshes,rigs)['bones'],target)
        options={'reviewed':True,'mapping':suggest['mapping'],'source_roles':suggest['source']['roles'],'target_roles':suggest['target']['roles'],'influence_limit':4,'mapping_anchors':{'appendage':'pelvis','tip':'pelvis'},'follow_motion_reviewed':True}
        options['mapping'].update(appendage='pelvis',tip='pelvis')
    return meshes,rigs,options,target

def evaluated(obj):
    bpy.context.view_layer.update();e=obj.evaluated_get(bpy.context.evaluated_depsgraph_get());m=e.to_mesh()
    try:return np.asarray([e.matrix_world@v.co for v in m.vertices],dtype=float)
    finally:e.to_mesh_clear()

original_hash=hashlib.sha256(source_path.read_bytes()).hexdigest() if source_path else None
meshes,rigs,options,target=source_scene();info=describe(meshes,rigs)
groups=physics_groups(info['bones'],info['weighted_bones'],options['source_roles'],options['target_roles'])
assert groups
if source_path:
    assert len(groups)==38 and sum(len(g['source_bones']) for g in groups)==262
    assert not set(options['source_roles'].values())&{n for g in groups for n in g['source_bones']}
else:
    assert len(groups)==1 and groups[0]['source_bones']==['appendage','helper','tip']
cases.append({'case':'dynamic-body-bindings-identify-groups-and-exclude-normal-body-colliders','passed':True,'groups':len(groups),'bones':sum(len(g['source_bones']) for g in groups)})
(OUT/'inspection.json').write_text(json.dumps(info,ensure_ascii=False))
(OUT/'options.json').write_text(json.dumps(options,ensure_ascii=False))
(OUT/'target.json').write_text(json.dumps(target))
for roots in [[],['not a physics group'],[options['source_roles']['upperarm_L']],[groups[0]['root'],groups[0]['root']]]:
    try:selection(options|{'preserve_physics_bones':True,'physics_roots':roots},info['bones'],info['weighted_bones'],options['source_roles'],options['target_roles'],target)
    except ValueError:pass
    else:raise AssertionError('Invalid preservation selection accepted')
cases.append({'case':'empty-stale-body-role-and-duplicate-selections-rejected','passed':True})

results={}
for label,roots in [('disabled',None),('one',[next((g['root'] for g in groups if g['root'].startswith('tail_')),groups[0]['root'])]),('all',[g['root'] for g in groups])]:
    meshes,rigs,choice,base=source_scene();original_rows={o.name: [[(o.vertex_groups[g.group].name,g.weight) for g in v.groups if g.weight>0] for v in o.data.vertices] for o in meshes}
    choice.update(preserve_physics_bones=roots is not None,physics_roots=roots or [])
    work=OUT/label;work.mkdir(exist_ok=True)
    result=prepare({'mode':'rigged_humanoid','target':base,'options':choice,'workspace':str(work),'hero_root':'models/test/haze'},meshes,rigs)
    results[label]=result
    assert result['target'][:len(base)]==base
    plan=result['preserved_physics_bones']
    if roots is None:
        assert len(result['target'])==len(base)
        continue
    assert set(plan['exported_bones'])=={n for g in plan['groups'] for n in g['source_bones']}
    assert len(result['target'])==len(base)+len(plan['exported_bones'])
    final=next(o for o in bpy.context.scene.objects if o.type=='ARMATURE')
    for obj,mesh in zip(meshes,result['meshes']):
        before=next(m for m in results['disabled']['meshes'] if m['name']==mesh['name'])
        np.testing.assert_allclose(mesh['positions'],before['positions'],rtol=0,atol=2e-6)
        assert mesh['faces']==before['faces'] and mesh['uvs']==before['uvs'] and mesh['colors']==before['colors']
        np.testing.assert_allclose(evaluated(obj),mesh['positions'],atol=5e-5,rtol=1e-6)
    cases.append({'case':label+'-keeps-original-hero-binds-and-rest-geometry-with-independent-weights','passed':True,'added_bones':len(plan['exported_bones']),'trimmed_vertices':sum(s['trimmed_vertices'] for s in result['weights'])})
    # Pose one middle weighted joint in every kept group. Its descendants can
    # move; every vertex without weight on that branch must stay fixed.
    byname={b['name']:b for b in result['target']};checks=[]
    for group in plan['groups']:
        name=plan['exported_bones'][group['weighted_bones'][len(group['weighted_bones'])//2]]
        branch={name}
        while True:
            larger=branch|{b['name'] for b in result['target'] if b['parent'] in branch}
            if larger==branch:break
            branch=larger
        delta=np.asarray(Matrix.Translation(byname[name]['position'])@Matrix.Rotation(.2,4,'X')@Matrix.Translation(-np.asarray(byname[name]['position'])))
        moved=0
        for mesh in result['meshes']:
            points=np.asarray(mesh['positions']);amount=np.asarray([sum(w for n,w in row if n in branch) for row in mesh['weights']])
            bent=points+(np.c_[points,np.ones(len(points))]@delta.T)[:,:3]*amount[:,None]-points*amount[:,None]
            assert np.isfinite(bent).all()
            np.testing.assert_allclose(bent[amount==0],points[amount==0],atol=1e-12)
            moved+=int(np.count_nonzero(np.linalg.norm(bent-points,axis=1)>1e-7))
        assert moved>0,group['root']
        checks.append({'root':group['root'],'tested_bone':name,'moved_vertices':moved})
    cases.append({'case':label+'-each-group-bends-its-weighted-region-without-moving-unrelated-vertices','passed':True,'groups':checks})
    # Real Blender evaluation of a middle tail/helper joint, plus save/reopen.
    group=next((g for g in plan['groups'] if g['root'].startswith('tail_')),plan['groups'][0]);name=plan['exported_bones'][group['weighted_bones'][len(group['weighted_bones'])//2]]
    before=[evaluated(o) for o in meshes];pose=final.pose.bones[name];pose.rotation_mode='QUATERNION';pose.rotation_quaternion=Quaternion((1,0,0),.2)
    after=[evaluated(o) for o in meshes];assert max(float(np.max(np.abs(a-b))) for a,b in zip(before,after))>1e-5
    pose.matrix_basis=Matrix.Identity(4);bpy.context.view_layer.update()
    for o,p in zip(meshes,before):np.testing.assert_allclose(evaluated(o),p,atol=1e-5)
    corr=work/'correction';corr.mkdir(exist_ok=True);new_meshes,new_rigs=import_source(result['blend'])
    corrected=prepare({'mode':'rigged_humanoid','correction':True,'target':result['target'],'options':choice|{'source_roles':choice['target_roles'],'mapping':{b['name']:b['name'] for b in result['target']}},'workspace':str(corr),'hero_root':'models/test/haze'},new_meshes,new_rigs)
    assert corrected['target']==result['target']
    for a,b in zip(result['meshes'],corrected['meshes']):np.testing.assert_allclose(a['positions'],b['positions'],atol=5e-6,rtol=1e-6)
    cases.append({'case':label+'-real-bpy-bend-reset-and-correction-roundtrip','passed':True})
    (work/'result.json').write_text(json.dumps(result,ensure_ascii=False,allow_nan=False))
if source_path:assert hashlib.sha256(source_path.read_bytes()).hexdigest()==original_hash
summary={'blender':bpy.app.version_string,'cases':cases,'source_sha256':original_hash,'source_unchanged':True,'native_compiler_validated':False,'in_game_motion_tested':False,'target_scope':'Older 95-joint positional fixture with generated unit bind rotations/scales, not the current Haze bind; absent finger targets follow the corresponding hand in this test only' if source_path else 'Generated humanoid fixture with nonunit parent scales','external_textures_used':False}
(OUT/'results.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
