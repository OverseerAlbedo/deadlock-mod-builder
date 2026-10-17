"""Moving-part detection rules of the Blender bridge against the fixture shared
with secondary_bones.rs and src/preparation/secondary-bones.ts.

Run with Blender (the bridge module needs Blender's NumPy):
  blender --background --factory-startup --python-exit-code 23 --python scripts/test_blender_moving_parts.py
"""
import json, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
from secondary_bones import physics_groups, breast_bones, butt_bones, named_butt, named_breast, on_gun, charm_named, rigid_named, selection

data=json.loads((ROOT/'src-tauri/crates/deadlock-preparation/tests/fixtures/hanging-chains.json').read_text(encoding='utf-8'))
bones,weighted,roles=data['bones'],data['weighted'],data['roles']

def roots(breasts):
    return [g['root'] for g in physics_groups(bones,weighted,roles,roles,breasts)]

assert roots(None)==data['expected_roots'],roots(None)
assert roots(False)==data['expected_roots'],roots(False)
assert roots(True)==data['expected_roots_with_breast_motion'],roots(True)
left=next(g for g in physics_groups(bones,weighted,roles,roles,True) if g['root']=='Bone_Breast_L_00')
assert (left['source_anchor'],len(left['source_bones']))==('Bip001_Spine1',1),left
assert breast_bones(bones,weighted,roles)==data['breast_bones']
# Without breast names the mirrored pair in front of the chest is found by position.
renamed=data['renamed_breast_bones']
generic=[{**b,'name':renamed.get(b['name'],b['name'])} for b in bones]
assert breast_bones(generic,[renamed.get(n,n) for n in weighted],roles)==[renamed.get(n,n) for n in data['breast_bones']]
# Ribbons on the chest are a mirrored pair in front of the chest too, but their
# names make them ribbons; renamed without a part word they count as breasts.
ribbons=data['chest_ribbons']
assert not any(r in data['breast_bones'] for r in ribbons)
decorated=[{**b,'name':ribbons.get(b['name'],b['name'])} for b in bones]
assert breast_bones(decorated,[ribbons.get(n,n) for n in weighted],roles)==sorted(data['breast_bones']+list(ribbons.values())),breast_bones(decorated,[ribbons.get(n,n) for n in weighted],roles)
# Butts: by name and as a low mirrored pair behind the hips, never breasts, moving only with butt motion on.
assert butt_bones(bones,weighted,roles)==data['butt_bones'],butt_bones(bones,weighted,roles)
assert [g['root'] for g in physics_groups(bones,weighted,roles,roles,None,True)]==data['expected_roots_with_butt_motion']
assert all(named_butt(n) and not named_breast(n) for n in data['butt_names']['butt'])
assert not any(named_butt(n) for n in data['butt_names']['not_butt'])
saved={'preserve_physics_bones':True,'butt_motion':False,'physics_roots':data['expected_roots_with_butt_motion']}
assert [g['root'] for g in physics_groups(bones,weighted,roles,roles,None,False)]==data['expected_roots']
assert sorted(g['root'] for g in selection(saved,bones,weighted,roles,roles,[{'name':n} for n in roles.values()])['groups'])==data['expected_roots']
print('moving-part rules: ok')

# Gun charms: chains hanging from the marked gun bones move; gun parts and parts
# on the hand do not; without the marks only named charms move.
gun,charms=data['gun'],data['gun_charm_roots']
groups=physics_groups(bones,weighted,roles,roles)
assert [g['root'] for g in groups if on_gun(g['root'],bones,roles,gun)]==charms
link=next(g for g in groups if g['root']=='Pistol_Link_00')
assert (link['source_anchor'],link['source_bones'])==('Bip001_L_Hand',['Pistol_Link_00','Pistol_Link_01','Pistol_Link_02']),link
unmarked=[{k:v for k,v in b.items() if k!='weapon'} for b in bones]
assert [g['root'] for g in physics_groups(unmarked,weighted,roles,roles)]==[r for r in data['expected_roots'] if r not in charms]
assert charm_named('Key_Chain_00') and charm_named('キーホルダー') and not charm_named('Keystone')
assert not rigid_named('Earring_Dangle_00') and rigid_named('Earring_00')
options={'preserve_physics_bones':True,'physics_roots':data['expected_roots']}
target=[{'name':n} for n in set(roles.values())|{'weapon'}]
plan={'objects':[gun],'action':'replace','anchor':'weapon'}
kept=selection(options,bones,weighted,roles,roles,target,plan)
assert all((g['target_anchor']=='weapon')==(g['root'] in charms) for g in kept['groups']),kept['groups']
left=selection(options,bones,weighted,roles,roles,target,{**plan,'action':'remove','anchor':''})
assert [g['root'] for g in left['groups']]==[r for r in data['expected_roots'] if r not in charms]
assert selection({**options,'physics_roots':charms},bones,weighted,roles,roles,target,{**plan,'action':'remove'}) is None
print('gun charm rules: ok')

# Adding breast bones to a rig without them (breast_bones.py): a synthetic torso
# with two breasts gets a mirrored pair with weights confined to the breasts and
# every vertex's total weight unchanged; the same torso without breasts gets none.
import bpy, bmesh
from mathutils import Matrix
from breast_bones import add_breast_bones

def character(with_breasts):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    mesh = bpy.data.meshes.new('body')
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=48, radius1=0.13, radius2=0.13, depth=0.6,
                          matrix=Matrix.Translation((0, 0, 1.15)))
    if with_breasts:
        for side in (1, -1):
            bmesh.ops.create_uvsphere(bm, u_segments=24, v_segments=16, radius=0.065,
                                      matrix=Matrix.Translation((side * 0.065, -0.1, 1.24)))
    bm.to_mesh(mesh); bm.free()
    body = bpy.data.objects.new('body', mesh); bpy.context.scene.collection.objects.link(body)
    arm = bpy.data.armatures.new('rig'); rig = bpy.data.objects.new('rig', arm)
    bpy.context.scene.collection.objects.link(rig); bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')
    parent = None
    for name, z in (('pelvis', 0.92), ('spine', 1.05), ('chest', 1.19), ('neck', 1.38), ('head', 1.47)):
        b = arm.edit_bones.new(name); b.head = (0, 0, z); b.tail = (0, 0, z + 0.08); b.parent = parent; parent = b
    bpy.ops.object.mode_set(mode='OBJECT')
    groups = {n: body.vertex_groups.new(name=n) for n in ('pelvis', 'spine', 'chest', 'neck')}
    for v in mesh.vertices:
        z = v.co.z
        name = 'pelvis' if z < 0.98 else 'spine' if z < 1.12 else 'chest' if z < 1.36 else 'neck'
        groups[name].add([v.index], 1.0, 'REPLACE')
    body.parent = rig
    body.modifiers.new('rig', 'ARMATURE').object = rig
    return rig, body

request = {'chest': 'chest', 'pelvis': 'pelvis', 'head': 'head', 'high': 'neck',
           'torso': ['neck', 'chest', 'spine', 'pelvis'], 'forward': [0., -1.]}
rig, body = character(True)
receipt = add_breast_bones(rig, [body], request)
assert receipt['status'] == 'added', receipt
heads = {n: rig.matrix_world @ rig.data.bones[n].head_local for n in ('DMB_Breast_L', 'DMB_Breast_R')}
assert abs(heads['DMB_Breast_L'].x + heads['DMB_Breast_R'].x) < 1e-6 and heads['DMB_Breast_L'].x > 0.03, heads
assert all(abs(h.z - 1.24) < 0.06 and h.y < -0.08 for h in heads.values()), heads
assert all(rig.data.bones[n].parent.name == 'chest' for n in heads)
names = {g.index: g.name for g in body.vertex_groups}
for v in body.data.vertices:
    weights = {names[g.group]: g.weight for g in v.groups}
    assert abs(sum(weights.values()) - 1.0) < 1e-4, weights
    breast = weights.get('DMB_Breast_L', 0) + weights.get('DMB_Breast_R', 0)
    if breast > 0.01:
        assert v.co.y < -0.06 and abs(v.co.z - 1.24) < 0.12, (v.co, weights)
assert add_breast_bones(rig, [body], request)['status'] == 'present'
rig, body = character(False)
flat = add_breast_bones(rig, [body], request)
assert flat['status'] == 'flat' and 'DMB_Breast_L' not in rig.data.bones, flat
print('breast bone generation: ok')
