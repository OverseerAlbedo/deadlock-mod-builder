"""Actual Blender API failure guards, correction round trip and texture copy."""
import sys,json,os,hashlib
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];sys.path[:0]=[str(ROOT),str(ROOT/'scripts'),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
import bpy
from test_blender_preparation import fixture,OUT
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid,skeleton
from worker import import_source,describe,prepare
from algorithms import identify,suggest_mapping
from deadlock_builder.preparation.export import export_dmx
results=[]

def blocked(name,setup,expected):
    try:
        setup();path=OUT/(name+'.glb');bpy.ops.export_scene.gltf(filepath=str(path),export_format='GLB')
        m,r=import_source(str(path));info=describe(m,r)
        errors=info['humanoid'].get('errors',[])
        if info['humanoid'].get('status')=='Unsupported' or errors:
            results.append({'case':name,'passed':True,'reason':str(errors)});return
        raise AssertionError('Unsupported fixture was not rejected')
    except ValueError as ex:
        assert expected in str(ex),(expected,str(ex))
        results.append({'case':name,'passed':True,'reason':str(ex)})

def cube():
    bpy.ops.wm.read_factory_settings(use_empty=True);bpy.ops.mesh.primitive_cube_add()
    bpy.context.object.data.materials.append(bpy.data.materials.new('box'))

def flat():
    objects=fixture(rigged=False)
    for o in objects:
        for v in o.data.vertices:v.co.z*=.15

def missing():
    objects=fixture(rigged=False)
    for o in objects:
        for v in o.data.vertices:
            if v.co.x>.2 and v.co.z>1.2:v.co.x=.18

def multi():
    objects=fixture();rig=next(o for o in bpy.context.scene.objects if o.type=='ARMATURE');other=rig.copy();other.data=rig.data.copy();bpy.context.collection.objects.link(other)
    other.location.x+=3
    for obj in objects:
        duplicate=obj.copy();duplicate.data=obj.data.copy();bpy.context.collection.objects.link(duplicate);duplicate.parent=other
        for m in duplicate.modifiers:
            if m.type=='ARMATURE':m.object=other
    bpy.context.view_layer.update()

def nonhuman():
    fixture();rig=next(o for o in bpy.context.scene.objects if o.type=='ARMATURE')
    for i,b in enumerate(rig.data.bones):b.name='creature_'+str(i)

def morph():
    o=fixture()[0];o.shape_key_add(name='Basis');o.shape_key_add(name='Smile')

for name,setup,expected in [('arbitrary-object',cube,'Unsupported'),('quadruped-proportions',flat,'Unsupported'),
                           ('missing-arm',missing,'Unsupported'),('multiple-characters',multi,'multiple armatures'),
                           ('rigged-nonhumanoid',nonhuman,'Unsupported'),('morph-blocked',morph,'Morph/flex')]:
    blocked(name,setup,expected)

# Genuine image pixels are copied without display-transform rendering.
o=fixture('mixamo')[0];mat=o.data.materials[0];mat.use_nodes=True
image=bpy.data.images.new('fixture_color',width=4,height=4);image.pixels=[.1,.3,.6,1]*16
image.filepath_raw=str(OUT/'fixture_texture.png');image.file_format='PNG';image.save();image.pack()
node=mat.node_tree.nodes.new('ShaderNodeTexImage');node.image=image
mat.node_tree.links.new(node.outputs['Color'],mat.node_tree.nodes.get('Principled BSDF').inputs['Base Color'])
source=OUT/'textured.glb';bpy.ops.export_scene.gltf(filepath=str(source),export_format='GLB')
m,r=import_source(str(source));info=describe(m,r);mapping=suggest_mapping(info['bones'],humanoid())
packed=next(i for i in bpy.data.images if i.packed_file).packed_file.data
before=hashlib.sha256(source.read_bytes()).hexdigest()
options={'reviewed':True,'mapping':mapping['mapping'],'source_roles':mapping['source']['roles'],'target_roles':mapping['target']['roles']}
work=OUT/'texture-and-correction';work.mkdir(exist_ok=True)
report=prepare({'mode':'rigged_humanoid','options':options,'target':humanoid(),'workspace':str(work),'hero_root':'models/test/hero'},m,r)
texture=next(iter(report['materials'].values()))['textures']['color']
assert Path(texture).read_bytes()==packed
assert hashlib.sha256(source.read_bytes()).hexdigest()==before
export_dmx(report,skeleton(),work/'prepared.dmx')
results.append({'case':'texture-byte-preservation-and-source-untouched','passed':True})
# Re-open the generated workspace Blender file and re-export after a user's save.
m,r=import_source(report['blend']);info=describe(m,r);mapping=suggest_mapping(info['bones'],humanoid())
options.update(mapping=mapping['mapping'],source_roles=mapping['source']['roles'])
corrected=OUT/'correction';corrected.mkdir(exist_ok=True)
report=prepare({'source':report['blend'],'correction':True,'mode':'rigged_humanoid','options':options,'target':humanoid(),'workspace':str(corrected),'hero_root':'models/test/hero'},m,r)
export_dmx(report,skeleton(),corrected/'prepared.dmx')
results.append({'case':'prepared-blend-correction-roundtrip','passed':True})
# A foreign model measured in meters must fit an inch-based target, including its thickness.
m,r=import_source(str(source));info=describe(m,r)
target=humanoid();factor=39.37007874
for bone in target:
    bone['position']=[v*factor for v in bone['position']]
    for axis in range(3):bone['matrix'][axis][3]*=factor
mapping=suggest_mapping(info['bones'],target)
options.update(mapping=mapping['mapping'],source_roles=mapping['source']['roles'])
scaled=OUT/'unit-normalization';scaled.mkdir(exist_ok=True)
report=prepare({'mode':'rigged_humanoid','options':options,'target':target,'workspace':str(scaled),'hero_root':'models/test/hero'},m,r)
assert abs(report['unit_scale']-factor)<.01
ref=skeleton()
ref['m_bonePosParent']=[[v*factor for v in row] for row in ref['m_bonePosParent']]
receipt=export_dmx(report,ref,scaled/'prepared.dmx')
assert receipt['bind_identity']['name_and_parent_identity_passed']
ys=[p[1] for mesh in report['meshes'] for p in mesh['positions']]
assert max(ys)-min(ys)>3, 'Model thickness failed to scale into target units'
results.append({'case':'meter-source-to-inch-target','passed':True})
# Blender 5 packs custom normals into a named attribute. It is semantic normal
# data, not an unknown stream. Check a non-default normal through blend import,
# preparation and the actual DMX bytes, including the correction round trip.
import numpy as np, struct
from mathutils import Vector
from deadlock_builder.dmx import Dmx
o=fixture()[0]
normal=Vector((.2,.4,.8)).normalized()
o.data.normals_split_custom_set([normal]*len(o.data.loops))
normal_source=OUT/'custom-normal-source.blend'
bpy.ops.wm.save_as_mainfile(filepath=str(normal_source))
m,r=import_source(str(normal_source))
expected=[list(v.vector) for v in m[0].data.corner_normals]
info=describe(m,r);mapping=suggest_mapping(info['bones'],humanoid())
options={'reviewed':True,'mapping':mapping['mapping'],'source_roles':mapping['source']['roles'],'target_roles':mapping['target']['roles']}
normal_work=OUT/'custom-normal-roundtrip';normal_work.mkdir(exist_ok=True)
report=prepare({'mode':'rigged_humanoid','options':options,'target':humanoid(),'workspace':str(normal_work),'hero_root':'models/test/hero'},m,r)
np.testing.assert_allclose(report['meshes'][0]['normals'],expected,atol=1e-5)
export_dmx(report,skeleton(),normal_work/'prepared.dmx')
dmx=Dmx.load(normal_work/'prepared.dmx')
shape=next(e for e in dmx.elements if e.type=='DmeMesh')
state=dmx.elements[shape.get('bindState')]
np.testing.assert_allclose([struct.unpack('<fff',v) for v in state.get('normal$0')],expected,atol=1e-5)
results.append({'case':'custom-normals-blend-preparation-dmx','passed':True})

# Unknown user attributes and a same-name attribute of the wrong type remain
# blocked; enabling Blender 5 must not make the stream guard permissive.
for attr_name in ('user_required_stream','custom_normal'):
    o=fixture()[0]
    if o.data.attributes.get(attr_name):o.data.attributes.remove(o.data.attributes[attr_name])
    o.data.attributes.new(name=attr_name,type='FLOAT',domain='POINT')
    path=OUT/(attr_name+'.blend');bpy.ops.wm.save_as_mainfile(filepath=str(path))
    try:import_source(str(path))
    except ValueError as ex:
        assert attr_name in str(ex),str(ex)
    else:raise AssertionError('Unsupported attribute was silently accepted: '+attr_name)
    results.append({'case':'reject-'+attr_name,'passed':True})
(OUT/'guard-results.json').write_text(json.dumps({'blender':bpy.app.version_string,'cases':results},indent=2),encoding='utf-8')
print(json.dumps(results,indent=2))
