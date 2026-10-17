"""Real bpy fixtures for read-only prepared-material texture recovery.

Generated images/graphs exercise the user's naming pattern and MMD-style links;
this is not a test of the full user TangTang asset or of Source 2 rendering.
"""
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender'))
import bpy
from material_textures import texture_and_materials
from worker import main as worker_main

OUT=Path(os.environ.get('DMB_MATERIAL_TEST_OUTPUT',str(ROOT/'build/material-recovery'))).resolve()
OUT.mkdir(parents=True,exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
results=[]
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()

def image(name,value=.5):
    img=bpy.data.images.new(name,width=4,height=4,alpha=True)
    img.pixels=[value,.2,.7,.4]*16
    img.filepath_raw=str(OUT/name);img.file_format='PNG';img.save();img.pack()
    return img

def material(name):
    mat=bpy.data.materials.new(name);mat.use_nodes=True
    mat.node_tree.nodes.clear()
    output=mat.node_tree.nodes.new('ShaderNodeOutputMaterial')
    return mat,output

def texture(tree,img,name=None):
    node=tree.nodes.new('ShaderNodeTexImage');node.image=img
    if name:node.name=name
    return node

def mesh(mat):
    data=bpy.data.meshes.new(mat.name);data.from_pydata([(0,0,0),(1,0,0),(0,0,1)],[],[(0,1,2)])
    obj=bpy.data.objects.new(mat.name,data);bpy.context.collection.objects.link(obj);obj.data.materials.append(mat)
    return obj

def extract(mat,label):
    obj=mesh(mat)
    configs,_,warnings=texture_and_materials([obj],OUT/label,'models/test/hero')
    return next(iter(configs.values())),warnings,obj

img=image('deliberately_named_N.png')
mat,out=material('body')
shader=mat.node_tree.nodes.new('ShaderNodeBsdfPrincipled');tex=texture(mat.node_tree,img)
mat.node_tree.links.new(tex.outputs['Color'],shader.inputs['Base Color']);mat.node_tree.links.new(shader.outputs[0],out.inputs['Surface'])
cfg,_,_=extract(mat,'direct')
assert set(cfg['textures'])=={'color'} and sha(cfg['textures']['color'])==sha(img.filepath)
assert not cfg['confirmed']
results.append({'case':'direct-connection-outranks-image-suffix-and-preserves-bytes','passed':True})

# A shader inside a group receives a different image in each material instance.
group=bpy.data.node_groups.new('Shared shader','ShaderNodeTree')
group.interface.new_socket(name='Tint',in_out='INPUT',socket_type='NodeSocketColor')
group.interface.new_socket(name='Surface',in_out='OUTPUT',socket_type='NodeSocketShader')
gin=group.nodes.new('NodeGroupInput');gout=group.nodes.new('NodeGroupOutput');principled=group.nodes.new('ShaderNodeBsdfPrincipled')
group.links.new(gin.outputs['Tint'],principled.inputs['Base Color']);group.links.new(principled.outputs[0],gout.inputs['Surface'])
for i in [1,2]:
    img=image('T_actor_tangtang_cloth_0'+str(i)+'_D.png',.2*i)
    mat,out=material('cloth'+str(i));tree=mat.node_tree;node=tree.nodes.new('ShaderNodeGroup');node.node_tree=group
    tex=texture(tree,img);reroute=tree.nodes.new('NodeReroute')
    tree.links.new(tex.outputs['Color'],reroute.inputs[0]);tree.links.new(reroute.outputs[0],node.inputs['Tint']);tree.links.new(node.outputs[0],out.inputs['Surface'])
    cfg,_,_=extract(mat,'group'+str(i))
    assert sha(cfg['textures']['color'])==sha(img.filepath)
    assert len(cfg['candidates']['color'])==1
    results.append({'case':'shader-group-instance-'+str(i)+'-uses-own-image','passed':True})

# An image inside a passthrough color group reaches an outside Principled shader.
inner=bpy.data.node_groups.new('Inner image','ShaderNodeTree')
inner.interface.new_socket(name='Color',in_out='OUTPUT',socket_type='NodeSocketColor')
inner_out=inner.nodes.new('NodeGroupOutput');img=image('nested.png');inner_tex=texture(inner,img)
inner.links.new(inner_tex.outputs['Color'],inner_out.inputs['Color'])
outer=bpy.data.node_groups.new('Outer image','ShaderNodeTree')
outer.interface.new_socket(name='Color',in_out='OUTPUT',socket_type='NodeSocketColor')
outer_out=outer.nodes.new('NodeGroupOutput');inner_node=outer.nodes.new('ShaderNodeGroup');inner_node.node_tree=inner
outer.links.new(inner_node.outputs['Color'],outer_out.inputs['Color'])
mat,out=material('nested');tree=mat.node_tree;group_node=tree.nodes.new('ShaderNodeGroup');group_node.node_tree=outer
shader=tree.nodes.new('ShaderNodeBsdfPrincipled');tree.links.new(group_node.outputs[0],shader.inputs['Base Color']);tree.links.new(shader.outputs[0],out.inputs['Surface'])
cfg,_,_=extract(mat,'nested');assert sha(cfg['textures']['color'])==sha(img.filepath)
results.append({'case':'nested-color-groups-and-direct-image-links','passed':True})

# Connected MMD base-image identity works with opaque/Japanese material names.
mat,out=material('瞳_1_HL');tree=mat.node_tree
base=image('opaque-image.png');toon=image('toon_D.png');sphere=image('sphere_D.png')
base_node=texture(tree,base,'mmd_base_tex');toon_node=texture(tree,toon,'mmd_toon_tex');sphere_node=texture(tree,sphere,'mmd_sphere_tex')
mix=tree.nodes.new('ShaderNodeMixRGB');mix2=tree.nodes.new('ShaderNodeMixRGB');diffuse=tree.nodes.new('ShaderNodeBsdfDiffuse')
tree.links.new(base_node.outputs['Color'],mix.inputs[1]);tree.links.new(toon_node.outputs['Color'],mix.inputs[2])
tree.links.new(mix.outputs[0],mix2.inputs[1]);tree.links.new(sphere_node.outputs['Color'],mix2.inputs[2])
tree.links.new(mix2.outputs[0],diffuse.inputs['Color']);tree.links.new(diffuse.outputs[0],out.inputs['Surface'])
cfg,notes,obj=extract(mat,'mmd');assert sha(cfg['textures']['color'])==sha(base.filepath)
assert len(cfg['candidates']['color'])==1 and notes
results.append({'case':'connected-mmd-base-keeps-opaque-material-binding-and-excludes-toon-sphere','passed':True})

# The mmd_tools shader group (MMDShaderDev): base x toon, then x sphere and
# + sphere, mixed into a Diffuse BSDF inside the group. Followed through those
# mixes, the sphere map used to become the color (SiViolet's MMD version).
mmd=bpy.data.node_groups.new('MMDShaderDev test','ShaderNodeTree')
for socket in ['Base Tex','Toon Tex','Sphere Tex']:mmd.interface.new_socket(name=socket,in_out='INPUT',socket_type='NodeSocketColor')
mmd.interface.new_socket(name='Shader',in_out='OUTPUT',socket_type='NodeSocketShader')
gin=mmd.nodes.new('NodeGroupInput');gout=mmd.nodes.new('NodeGroupOutput');diffuse=mmd.nodes.new('ShaderNodeBsdfDiffuse')
with_toon,with_sphere,plus_sphere,blend=[mmd.nodes.new('ShaderNodeMixRGB') for _ in range(4)]
mmd.links.new(gin.outputs['Base Tex'],with_toon.inputs[1]);mmd.links.new(gin.outputs['Toon Tex'],with_toon.inputs[2])
mmd.links.new(with_toon.outputs[0],with_sphere.inputs[1]);mmd.links.new(gin.outputs['Sphere Tex'],with_sphere.inputs[2])
mmd.links.new(with_toon.outputs[0],plus_sphere.inputs[1]);mmd.links.new(gin.outputs['Sphere Tex'],plus_sphere.inputs[2])
mmd.links.new(with_sphere.outputs[0],blend.inputs[1]);mmd.links.new(plus_sphere.outputs[0],blend.inputs[2])
mmd.links.new(blend.outputs[0],diffuse.inputs['Color']);mmd.links.new(diffuse.outputs[0],gout.inputs['Shader'])
mat,out=material('M_actor_mmd_body');tree=mat.node_tree;shader=tree.nodes.new('ShaderNodeGroup');shader.node_tree=mmd
base=image('T_actor_mmd_body_01.png',.3);toon=image('skintoon.png',.6);sphere=image('skin.png',.9)
for img,name,socket in [(base,'mmd_base_tex','Base Tex'),(toon,'mmd_toon_tex','Toon Tex'),(sphere,'mmd_sphere_tex','Sphere Tex')]:
    tree.links.new(texture(tree,img,name).outputs['Color'],shader.inputs[socket])
tree.links.new(shader.outputs['Shader'],out.inputs['Surface'])
cfg,notes,obj=extract(mat,'mmd-group')
assert sha(cfg['textures']['color'])==sha(base.filepath),cfg['textures']
assert len(cfg['candidates']['color'])==1 and notes
results.append({'case':'mmd-shader-group-uses-base-texture-not-sphere-map','passed':True})

mat,out=material('complex');tree=mat.node_tree
one=image('T_actor_tangtang_iris_01_D.png',.2);two=image('T_actor_tangtang_iris_02_D.png',.8)
first=texture(tree,one);second=texture(tree,two);unused=texture(tree,image('unused_D.png'))
mix=tree.nodes.new('ShaderNodeMixRGB');diffuse=tree.nodes.new('ShaderNodeBsdfDiffuse')
tree.links.new(first.outputs[0],mix.inputs[1]);tree.links.new(second.outputs[0],mix.inputs[2]);tree.links.new(mix.outputs[0],diffuse.inputs['Color']);tree.links.new(diffuse.outputs[0],out.inputs['Surface'])
cfg,_,_=extract(mat,'ambiguous')
assert not cfg['textures'] and cfg['ambiguous']==['color'] and len(cfg['candidates']['color'])==2
assert {sha(p) for p in cfg['candidates']['color']}=={sha(one.filepath),sha(two.filepath)}
results.append({'case':'multiple-connected-images-remain-choices-disconnected-image-ignored','passed':True})

# Fully transparent MMD layers (an expression fades them in) are face layers,
# named blush, tears or other; an opaque material and a linked Alpha are not.
for name,alpha,kind in [('赤面',0.0,'blush'),('照れ',0.0,'blush'),('涙2',0.0,'tears'),('汗',0.0,'other'),('肌',1.0,None)]:
    mat,out=material(name);tree=mat.node_tree;shader=tree.nodes.new('ShaderNodeGroup');shader.node_tree=mmd;shader.name='mmd_shader'
    if 'Alpha' not in [s.name for s in mmd.interface.items_tree]:
        mmd.interface.new_socket(name='Alpha',in_out='INPUT',socket_type='NodeSocketFloat')
    shader.inputs['Alpha'].default_value=alpha
    tree.links.new(texture(tree,image(name+'.png'),'mmd_base_tex').outputs['Color'],shader.inputs['Base Tex'])
    tree.links.new(shader.outputs['Shader'],out.inputs['Surface'])
    cfg,notes,obj=extract(mat,'face-'+name)
    assert cfg.get('face_layer')==kind,(name,cfg.get('face_layer'))
    assert (kind is None) or any('Face details' in n for n in notes)
mat,out=material('principled-linked-alpha');tree=mat.node_tree;shader=tree.nodes.new('ShaderNodeBsdfPrincipled')
shader.inputs['Alpha'].default_value=0.0;alpha_tex=texture(tree,image('mask_alpha.png'))
tree.links.new(alpha_tex.outputs['Alpha'],shader.inputs['Alpha']);tree.links.new(shader.outputs[0],out.inputs['Surface'])
cfg,_,_=extract(mat,'linked-alpha');assert 'face_layer' not in cfg
results.append({'case':'fully-transparent-source-layers-become-face-layers','passed':True})

mat,out=material('packed-only');tree=mat.node_tree
tex=texture(tree,image('T_actor_body_ORM.png'));diffuse=tree.nodes.new('ShaderNodeBsdfDiffuse')
tree.links.new(tex.outputs['Color'],diffuse.inputs['Color']);tree.links.new(diffuse.outputs[0],out.inputs['Surface'])
cfg,_,_=extract(mat,'packed');assert not cfg['textures']
results.append({'case':'packed-channel-image-not-guessed-or-swizzled','passed':True})

# Recover all of the above from a saved file through the actual worker entry.
source=OUT/'prepared.blend';bpy.ops.wm.save_as_mainfile(filepath=str(source));before=sha(source)
positions={o.name:[list(v.co) for v in o.data.vertices] for o in bpy.context.scene.objects if o.type=='MESH'}
workspace=OUT/'recovered';workspace.mkdir(exist_ok=True)
job={'operation':'recover_materials','source':str(source),'hero_root':'models/test/hero',
     'workspace':str(workspace),'result':str(workspace/'result.json')}
path=workspace/'job.json';path.write_text(json.dumps(job),encoding='utf-8')
previous=sys.argv;sys.argv=['worker.py','--',str(path)]
try:worker_main()
finally:sys.argv=previous
report=json.loads((workspace/'result.json').read_text())
assert report['status']=='ok' and len(report['materials'])==len(positions)
assert sha(source)==before
assert positions=={o.name:[list(v.co) for v in o.data.vertices] for o in bpy.context.scene.objects if o.type=='MESH'}
assert not (workspace/'prepared.blend').exists()
assert all(p.startswith(str(workspace/'textures')) for c in report['materials'].values() for p in c['textures'].values())
results.append({'case':'worker-recovers-existing-file-without-reconversion-or-source-geometry-writes','passed':True,'source_unchanged':True})

(OUT/'results.json').write_text(json.dumps({'blender':bpy.app.version_string,'cases':results},indent=2)+'\n')
print('PASS:',len(results),'material recovery cases on',bpy.app.version_string)
