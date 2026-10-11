"""Real Blender regressions for character parts and detached equipment.

Run with Blender --background --factory-startup --disable-autoexec --python,
or with the official bpy wheel. DMB_MODULAR_SOURCE optionally tests a user
source read-only; it is never copied into the repository or saved in place.
"""
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT/'scripts'), str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
import bpy
import numpy as np
from mathutils import Matrix, Vector
from test_blender_preparation import fixture
from worker import import_source, describe, prepare
from source_rigs import POLICY
from algorithms import suggest_mapping
from blend_source import snapshot_blend
from static_face import apply_static_shapes
from tests.preparation_fixture import humanoid

OUT = Path(os.environ['DMB_PREP_TEST_OUTPUT'])
OUT.mkdir(parents=True, exist_ok=True)


def mesh(name, center, rig, groups, radius=.06):
    points = [tuple(Vector(center)+Vector((x, y, z))*radius)
              for x, y, z in [(-1,-1,-1),(1,-1,-1),(1,1,-1),(-1,1,-1),(-1,-1,1),(1,-1,1),(1,1,1),(-1,1,1)]]
    data = bpy.data.meshes.new(name)
    data.from_pydata(points, [], [(0,3,2,1),(4,5,6,7),(0,1,5,4),(3,7,6,2),(0,4,7,3),(1,2,6,5)])
    data.uv_layers.new(name='UVMap')
    for i, uv in enumerate(data.uv_layers[0].data):
        uv.uv = ((i%4)//2, (i%4)%2)
    obj = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(obj)
    obj.data.materials.append(bpy.data.materials.new(name+' material'))
    obj.parent = rig
    obj.matrix_world = Matrix.Identity(4)
    for name, weight in groups:
        group = obj.vertex_groups.new(name=name)
        group.add(list(range(len(data.vertices))), weight, 'REPLACE')
    obj.modifiers.new('Skin', 'ARMATURE').object = rig
    return obj


def armature(name, bones):
    rig = bpy.data.objects.new(name, bpy.data.armatures.new(name))
    bpy.context.collection.objects.link(rig)
    bpy.ops.object.select_all(action='DESELECT')
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')
    for name, parent, point in bones:
        bone = rig.data.edit_bones.new(name)
        bone.head = point
        bone.tail = Vector(point)+Vector((0, .04, 0))
        if parent:
            bone.parent = rig.data.edit_bones[parent]
    bpy.ops.object.mode_set(mode='OBJECT')
    return rig


def capture(objects):
    return {o.name:{'points':[list(o.matrix_world@v.co) for v in o.data.vertices],
                   'faces':[(list(p.vertices),p.material_index) for p in o.data.polygons],
                   'weights':[[(o.vertex_groups[g.group].name,g.weight) for g in v.groups] for v in o.data.vertices],
                   'uvs':[[list(v.uv) for v in layer.data] for layer in o.data.uv_layers],
                   'materials':[s.material.name if s.material else None for s in o.material_slots],
                   'shapes':{s.name:[list(p.co) for p in s.data] for s in o.data.shape_keys.key_blocks} if o.data.shape_keys else {}}
            for o in objects}


def assert_geometry(before, objects, renames):
    after = capture(objects)
    for name, current in after.items():
        inverse = {v:k for k,v in renames.get(name, {}).items()}
        current['weights'] = [[(inverse.get(n,n),w) for n,w in row] for row in current['weights']]
        np.testing.assert_allclose(current.pop('points'), before[name]['points'], rtol=1e-7, atol=1e-6)
        assert current == {k:v for k,v in before[name].items() if k!='points'}, name


def facial_motion(rig, obj, anchor):
    # Every retained facial weight lies below the character's head. A head turn
    # must move the complete face rigidly with its original offset, without gaps.
    bpy.context.view_layer.update()
    baseline = np.asarray([obj.matrix_world@v.co for v in obj.data.vertices])
    bone = rig.pose.bones[anchor]
    rest = rig.matrix_world@rig.data.bones[anchor].matrix_local
    delta = Matrix.Translation(rest.translation) @ Matrix.Rotation(.3,4,'Z') @ Matrix.Translation(-rest.translation)
    bone.matrix = rig.matrix_world.inverted() @ delta @ rest
    bpy.context.view_layer.update()
    evaluated = obj.evaluated_get(bpy.context.evaluated_depsgraph_get())
    actual = [evaluated.matrix_world@v.co for v in evaluated.data.vertices]
    expected = (np.c_[baseline,np.ones(len(baseline))]@np.asarray(delta).T)[:,:3]
    np.testing.assert_allclose(actual, expected, rtol=1e-6, atol=2e-6)
    bone.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()


def generated(case):
    objects = fixture()
    body = objects[0]
    primary = next(o for o in bpy.context.scene.objects if o.type=='ARMATURE')
    primary.name = 'Z character'
    head = primary.matrix_world@primary.data.bones['head'].matrix_local
    offset = Vector((0, .014 if case!='exact-subset' else 0, 0))
    part = armature('Face parts', [('head',None,head.translation+offset), ('detail','head',head.translation+Vector((.02,.01,.02))+offset)])
    face = mesh('Face', head.translation+Vector((0,-.04,.04)), part, [('head',.4),('detail',.6)])
    face.shape_key_add(name='Basis')
    face.shape_key_add(name='Smile')
    objects.append(face)
    if case=='nested-parent':
        empty=bpy.data.objects.new('Face container',None)
        bpy.context.collection.objects.link(empty)
        empty.parent=part
        world=face.matrix_world.copy();face.parent=empty;face.matrix_world=world
    if case=='group-name-collision':
        group=body.vertex_groups.new(name='detail')
        group.add([0],.125,'REPLACE')
    if case=='parent-controlled':
        empty=bpy.data.objects.new('Face control',None)
        bpy.context.collection.objects.link(empty)
        empty.constraints.new('LIMIT_LOCATION')
        empty.parent=part
        face.parent=empty
    if case=='live-rig-controls':
        part.constraints.new('LIMIT_LOCATION')
    if case=='physics-bound-part':
        root=bpy.data.objects.new('MMD part',None);bpy.context.collection.objects.link(root)
        root['mmd_type']='ROOT';root['mmd_tools_version']='4.3.0';part.parent=root
        group=bpy.data.objects.new('rigidbodies',None);bpy.context.collection.objects.link(group)
        group.parent=root;group['mmd_type']='RIGID_GRP_OBJ'
        bpy.ops.mesh.primitive_cube_add(size=.03)
        collider=bpy.context.object;collider.parent=group;collider.hide_render=True
        collider['mmd_type']='RIGID_BODY';collider['mmd_rigid']={'shape':'BOX','bone':'detail'}
        bpy.ops.rigidbody.object_add();collider.rigid_body.collision_shape='BOX'
        constraint=collider.constraints.new('CHILD_OF');constraint.name='mmd_tools_rigid_parent'
        constraint.target=part;constraint.subtarget='detail';constraint.mute=True
    if case=='scaled-part':
        part.scale.x=2
    if case=='detached-face':
        part.location.x=3
    if case in ('offset-with-props','same-rig-names-renamed'):
        # More bones than the body and a name that sorts first must not make
        # equipment the selected character.
        prop=armature('A equipment', [('tool_'+str(i),None,(3,0,1+i*.001)) for i in range(180)])
        objects.append(mesh('Equipment', (3,0,1),prop,[('tool_0',1)]))
    if case=='nearby-unattached':
        prop=armature('Unattached', [('gadget',None,(.1,0,1))])
        objects.append(mesh('Nearby',(.1,0,1),prop,[('gadget',1)]))
    if case=='two-characters':
        second=primary.copy();second.data=primary.data.copy();second.location.x=3
        bpy.context.collection.objects.link(second)
        duplicate=body.copy();duplicate.data=body.data.copy();bpy.context.collection.objects.link(duplicate)
        duplicate.parent=second
        duplicate.modifiers[0].object=second
        objects.append(duplicate)
    if case=='same-rig-names-renamed':
        primary.name='arbitrary 713';part.name='arbitrary 992'
    primary.hide_set(True)
    bpy.context.view_layer.update()
    before=capture(objects)
    source=OUT/(case+'.blend')
    bpy.ops.wm.save_as_mainfile(filepath=str(source))
    digest=hashlib.sha256(source.read_bytes()).hexdigest()
    bad={'parent-controlled','live-rig-controls','physics-bound-part','scaled-part','detached-face','nearby-unattached','two-characters'}
    try:
        receipts=[]
        meshes,rigs=import_source(str(source),allow_shape_keys=True,helper_reports=receipts)
    except ValueError as error:
        assert case in bad,(case,str(error))
        if case=='physics-bound-part':assert 'physics helper bindings' in str(error),str(error)
        assert hashlib.sha256(source.read_bytes()).hexdigest()==digest
        return {'case':case,'passed':True,'rejected':str(error)}
    assert case not in bad,case
    receipt=next(r for r in receipts if r['policy']==POLICY)
    assert len(rigs)==1 and set(o.name for o in meshes)=={'body','Face'}
    assert len(rigs[0].data.bones)<180
    rename=next(iter(receipt['part_bones'].values()))
    assert_geometry(before,meshes,{'Face':rename})
    if case=='group-name-collision':
        assert 'detail' not in rigs[0].data.bones
    if case=='exact-subset':assert rename['head']=='head'
    else:
        assert rename['head']!='head'
        assert rigs[0].data.bones[rename['head']].parent.name=='head'
    facial_motion(rigs[0],next(m for m in meshes if m.name=='Face'),'head')
    staged=OUT/(case+'-staged.blend')
    snapshot_blend(bpy,source,staged)
    repeated=[]
    meshes,rigs=import_source(str(staged),allow_shape_keys=True,helper_reports=repeated)
    assert next(r for r in repeated if r['policy']==POLICY)==receipt
    assert_geometry(before,meshes,{'Face':rename})
    # Confirm the real preparation/corrected-source path can consume the union.
    apply_static_shapes(meshes,'basis_static')
    info=describe(meshes,rigs);target=humanoid();suggested=suggest_mapping(info['bones'],target)
    mapping=suggested['mapping'];following={}
    for name in info['weighted_bones']:
        if name not in mapping:mapping[name]='head';following[name]='head'
    options={'reviewed':True,'source_roles':suggested['source']['roles'],'target_roles':suggested['target']['roles'],
             'mapping':mapping,'mapping_anchors':following,'follow_motion_reviewed':True}
    work=OUT/case;work.mkdir(exist_ok=True)
    report=prepare({'source':str(staged),'mode':'rigged_humanoid','options':options,'target':target,'workspace':str(work),'hero_root':'models/test'},meshes,rigs)
    assert {m['name'] for m in report['meshes']}=={'body','Face'}
    for entry in report['meshes']:
        np.testing.assert_allclose(entry['positions'],before[entry['name']]['points'],rtol=1e-6,atol=2e-6)
    corrected,rigs=import_source(report['blend'],allow_shape_keys=True)
    assert len(rigs)==1 and {o.name for o in corrected}=={'body','Face'}
    assert hashlib.sha256(source.read_bytes()).hexdigest()==digest
    return {'case':case,'passed':True,'retained':receipt['retained_meshes'],'excluded':receipt['excluded_meshes']}


def real_source(path):
    digest=hashlib.sha256(path.read_bytes()).hexdigest()
    bpy.ops.wm.open_mainfile(filepath=str(path),load_ui=False,use_scripts=False)
    before=capture([o for o in bpy.context.scene.objects if o.type=='MESH'])
    source_rigs={o.name:{b.name:np.asarray(o.matrix_world@b.matrix_local) for b in o.data.bones}
                 for o in bpy.context.scene.objects if o.type=='ARMATURE'}
    owners={o.name:next(m.object.name for m in o.modifiers if m.type=='ARMATURE')
            for o in bpy.context.scene.objects if o.type=='MESH'}
    receipts=[];meshes,rigs=import_source(str(path),allow_shape_keys=True,helper_reports=receipts)
    info=describe(meshes,rigs)
    receipt=next(r for r in receipts if r['policy']==POLICY)
    renames={m.name:receipt['part_bones'].get(owners[m.name],{}) for m in meshes}
    assert_geometry(before,meshes,renames)
    maximum=0.
    for rig,names in receipt['part_bones'].items():
        for old,new in names.items():
            actual=np.asarray(rigs[0].matrix_world@rigs[0].data.bones[new].matrix_local)
            expected=source_rigs[rig][old]
            np.testing.assert_allclose(actual[:3,3],expected[:3,3],rtol=1e-7,atol=1e-6)
            np.testing.assert_allclose(actual[:3,:3],expected[:3,:3],rtol=1e-7,atol=4e-6)
            maximum=max(maximum,float(abs(actual-expected).max()))
    facial_motion(rigs[0],next(m for m in meshes if m.name=='Face'),'Bip001_Head')
    saved=OUT/'uploaded-selected-character.blend'
    bpy.ops.wm.save_as_mainfile(filepath=str(saved),copy=True)
    repeated=[];reloaded,reloaded_rigs=import_source(str(saved),allow_shape_keys=True,helper_reports=repeated)
    assert_geometry(before,reloaded,renames)
    assert len(reloaded_rigs)==1 and next(r for r in repeated if r['policy']==POLICY)==receipt
    assert hashlib.sha256(path.read_bytes()).hexdigest()==digest
    return {'case':'uploaded-model','passed':True,'source_sha256':digest,'vertices':info['vertices'],'bones':len(info['bones']),
            'maximum_copied_matrix_roundoff':maximum,'receipt':receipt,'source_file_unchanged':True,
            'external_textures_present':all(Path(bpy.path.abspath(i.filepath)).is_file() or i.packed_file for i in bpy.data.images if i.source=='FILE')}


if __name__=='__main__':
    bpy.context.preferences.filepaths.use_scripts_auto_execute=False
    cases=['exact-subset','offset-with-props','nested-parent','group-name-collision','same-rig-names-renamed',
           'parent-controlled','live-rig-controls','physics-bound-part','scaled-part','detached-face','nearby-unattached','two-characters']
    results=[generated(case) for case in cases]
    if os.environ.get('DMB_MODULAR_SOURCE'):results.append(real_source(Path(os.environ['DMB_MODULAR_SOURCE'])))
    (OUT/'modular-rig-results.json').write_text(json.dumps({'blender':bpy.app.version_string,'cases':results},indent=2)+'\n')
    print(json.dumps({'blender':bpy.app.version_string,'passed':len(results),'cases':[r['case'] for r in results]},indent=2))
