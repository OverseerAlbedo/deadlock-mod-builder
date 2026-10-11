"""Real Blender API integration fixtures (run with Blender --background --python).

Alternatively DMB_BLENDER_PYTHON can run this with the official bpy wheel.
This does not execute Valve compilers or certify artistic/live-game quality.
"""
import sys, os, json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT));sys.path.insert(0,str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender'))
import bpy
from mathutils import Vector,Matrix
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid,skeleton
from worker import import_source,describe,prepare
from algorithms import suggest_mapping,identify

OUT=Path(os.environ.get('DMB_PREP_TEST_OUTPUT',str(ROOT/'build/blender-preparation-test'))).resolve()
OUT.mkdir(parents=True,exist_ok=True)


def fixture(style='target',rigged=True,scale=1.,apose=False,clothes=False):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bones=humanoid(style);names=[b['name'] for b in bones]
    if apose:
        for b in bones:
            if 'arm' in b['name'].lower() or 'hand' in b['name'].lower():b['position'][2]-=abs(b['position'][0])*.4
    points=[Vector(b['position'])*scale for b in bones]
    # Connected procedural humanoid fixture, no downloaded character data.
    mesh=bpy.data.meshes.new('body');mesh.from_pydata(points,[(i,names.index(b['parent'])) for i,b in enumerate(bones) if b['parent']],[])
    obj=bpy.data.objects.new('body',mesh);bpy.context.collection.objects.link(obj);obj.select_set(True);bpy.context.view_layer.objects.active=obj
    skin=obj.modifiers.new('fixture geometry','SKIN')
    for i,v in enumerate(mesh.skin_vertices[0].data):
        radius=.11 if i<5 else .07
        v.radius=(radius*scale,radius*scale)
    mesh.skin_vertices[0].data[0].use_root=True
    smooth=obj.modifiers.new('fixture surface','SUBSURF');smooth.levels=1
    bpy.ops.object.modifier_apply(modifier=skin.name);bpy.ops.object.modifier_apply(modifier=smooth.name)
    mat=bpy.data.materials.new('Fixture cloth');mat.diffuse_color=(.2,.4,.8,1);obj.data.materials.append(mat)
    bpy.ops.object.mode_set(mode='EDIT');bpy.ops.mesh.select_all(action='SELECT');bpy.ops.uv.smart_project();bpy.ops.object.mode_set(mode='OBJECT')
    meshes=[obj]
    if clothes:
        bpy.ops.mesh.primitive_uv_sphere_add(segments=12,ring_count=8,location=(0,0,1.3*scale),scale=(.20*scale,.15*scale,.20*scale))
        coat=bpy.context.object;coat.name='coat';coat.data.materials.append(bpy.data.materials.new('Fixture coat'));meshes.append(coat)
        bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
    if rigged:
        arm=bpy.data.armatures.new('source');rig=bpy.data.objects.new('source',arm);bpy.context.collection.objects.link(rig)
        bpy.context.view_layer.objects.active=rig;rig.select_set(True);bpy.ops.object.mode_set(mode='EDIT')
        for i,b in enumerate(bones):
            eb=arm.edit_bones.new(b['name']);eb.head=points[i];eb.tail=points[i]+Vector((0,.1*scale,0))
        for b in bones:
            if b['parent']:arm.edit_bones[b['name']].parent=arm.edit_bones[b['parent']]
        bpy.ops.object.mode_set(mode='OBJECT')
        for o in meshes:
            for name in names:o.vertex_groups.new(name=name)
            for v in o.data.vertices:
                index=min(range(len(points)),key=lambda i:(o.matrix_world@v.co-points[i]).length)
                o.vertex_groups[names[index]].add([v.index],1,'REPLACE')
            mod=o.modifiers.new('skin','ARMATURE');mod.object=rig;o.parent=rig
    return meshes

if __name__=='__main__':
    results=[]
    for style,rigged,scale,apose,clothes in [('mixamo',True,1,False,False),('bip',True,1,False,False),('target',True,1,False,True),
                                           ('target',False,1,False,False),('target',False,2,True,True),('target',False,.5,False,False)]:
        for fmt in ['glb','gltf','fbx','blend']:
            name=f'{style}-{rigged}-{scale}-{apose}-{clothes}-{fmt}';directory=OUT/name;directory.mkdir(exist_ok=True)
            try:
                fixture(style,rigged,scale,apose,clothes);path=directory/('source.'+fmt)
                if fmt=='blend':bpy.ops.wm.save_as_mainfile(filepath=str(path))
                elif fmt=='fbx':bpy.ops.export_scene.fbx(filepath=str(path),use_selection=False,add_leaf_bones=False,bake_anim=False)
                else:bpy.ops.export_scene.gltf(filepath=str(path),export_format='GLB' if fmt=='glb' else 'GLTF_SEPARATE')
                meshes,rigs=import_source(str(path));info=describe(meshes,rigs)
                target=humanoid();suggest=suggest_mapping(info['bones'],target) if rigs else {'target':identify(target)}
                options={'reviewed':True,'target_roles':suggest['target']['roles'],'influence_limit':4}
                if rigs:options.update(mapping=suggest['mapping'],source_roles=suggest['source']['roles'])
                else:options['landmarks']=info['humanoid']['landmarks']
                before_faces={o.name:[(tuple(poly.vertices),poly.material_index) for poly in o.data.polygons] for o in meshes}
                before_uv={o.name:[[list(d.uv) for d in layer.data] for layer in o.data.uv_layers] for o in meshes}
                before_material={o.name:[s.material.name for s in o.material_slots] for o in meshes}
                report=prepare({'source':str(path),'correction':False,'mode':'rigged_humanoid' if rigged else 'unrigged_humanoid','target':target,'options':options,
                                'workspace':str(directory),'hero_root':'models/test/hero'},meshes,rigs)
                for obj in meshes:
                    assert [(tuple(poly.vertices),poly.material_index) for poly in obj.data.polygons]==before_faces[obj.name]
                    assert [[list(d.uv) for d in layer.data] for layer in obj.data.uv_layers]==before_uv[obj.name]
                    assert [s.material.name for s in obj.material_slots]==before_material[obj.name]
                (directory/'result.json').write_text(json.dumps(report,allow_nan=False),encoding='utf-8')
                from deadlock_builder.preparation.export import export_dmx
                receipt=export_dmx(report,skeleton(),directory/'prepared.dmx')
                results.append({'case':name,'passed':True,'vertices':sum(x['vertices'] for x in report['weights']), 'skin':receipt['skinning']['numeric_checks_passed']})
            except Exception as ex:
                import traceback
                traceback.print_exc();results.append({'case':name,'passed':False,'error':str(ex)})
    (OUT/'results.json').write_text(json.dumps({'blender':bpy.app.version_string,'test_kind':'real Blender Python API; generated fixtures; no Valve/native-game test','cases':results},indent=2),encoding='utf-8')
    print(json.dumps(results,indent=2))
    if not all(r['passed'] for r in results):raise RuntimeError('Blender integration fixture failures; inspect results.json')
