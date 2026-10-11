"""Synthetic Blender sources for the "almost any humanoid" probes.

Development-only. Builds a skinned T-pose humanoid with configurable bone
naming conventions, plus the kinds of extra scene content that real downloads
contain (cameras, lights, curves, hidden alternates, loose props, a second
character, a gun held in the hand, generative modifiers, missing materials).
Nothing here imports the retired legacy helpers.
"""
import math
import bpy
from mathutils import Matrix, Vector

# Canonical layout: Z up, character faces -Y, left side is +X, height ~1.8.
LAYOUT = [
    # key, parent, head, tail, deform
    ('root', None, (0, 0, 0), (0, .15, 0), False),
    ('pelvis', 'root', (0, 0, .95), (0, 0, 1.05), True),
    ('spine', 'pelvis', (0, 0, 1.05), (0, 0, 1.25), True),
    ('chest', 'spine', (0, 0, 1.25), (0, 0, 1.48), True),
    ('neck', 'chest', (0, 0, 1.48), (0, 0, 1.58), True),
    ('head', 'neck', (0, 0, 1.58), (0, 0, 1.80), True),
    ('jaw', 'head', (0, -.03, 1.62), (0, -.09, 1.6), True),
    ('hair_0', 'head', (0, .09, 1.74), (0, .13, 1.55), True),
    ('hair_1', 'hair_0', (0, .13, 1.55), (0, .15, 1.38), True),
    ('hair_2', 'hair_1', (0, .15, 1.38), (0, .16, 1.22), True),
]
for side, sign in (('L', 1), ('R', -1)):
    s = lambda x: sign * x
    LAYOUT += [
        ('clavicle_' + side, 'chest', (s(.03), 0, 1.44), (s(.17), 0, 1.45), True),
        ('upperarm_' + side, 'clavicle_' + side, (s(.17), 0, 1.45), (s(.45), 0, 1.45), True),
        ('upperarm_twist_' + side, 'upperarm_' + side, (s(.31), 0, 1.45), (s(.40), 0, 1.45), True),
        ('forearm_' + side, 'upperarm_' + side, (s(.45), 0, 1.45), (s(.70), 0, 1.45), True),
        ('hand_' + side, 'forearm_' + side, (s(.70), 0, 1.45), (s(.78), 0, 1.45), True),
        ('thumb_' + side, 'hand_' + side, (s(.72), -.04, 1.44), (s(.76), -.07, 1.43), True),
        ('index_' + side, 'hand_' + side, (s(.78), -.02, 1.45), (s(.84), -.02, 1.45), True),
        ('middle_' + side, 'hand_' + side, (s(.78), 0, 1.45), (s(.85), 0, 1.45), True),
        ('pinky_' + side, 'hand_' + side, (s(.77), .03, 1.45), (s(.82), .03, 1.45), True),
        ('thigh_' + side, 'pelvis', (s(.10), 0, .93), (s(.10), 0, .50), True),
        ('shin_' + side, 'thigh_' + side, (s(.10), 0, .50), (s(.10), 0, .09), True),
        ('foot_' + side, 'shin_' + side, (s(.10), 0, .09), (s(.10), -.12, .03), True),
        ('toe_' + side, 'foot_' + side, (s(.10), -.12, .03), (s(.10), -.20, .03), True),
    ]

L, R = '.L', '.R'
STYLES = {
    # Already supported conventions (sanity).
    'mixamo': {'root': 'Armature_root', 'pelvis': 'mixamorig:Hips', 'spine': 'mixamorig:Spine', 'chest': 'mixamorig:Spine2',
               'neck': 'mixamorig:Neck', 'head': 'mixamorig:Head', 'jaw': 'mixamorig:Jaw',
               'hair_0': 'mixamorig:Hair0', 'hair_1': 'mixamorig:Hair1', 'hair_2': 'mixamorig:Hair2',
               **{f'{k}_{s}': f'mixamorig:{side}{v}' for s, side in (('L', 'Left'), ('R', 'Right')) for k, v in
                  (('clavicle', 'Shoulder'), ('upperarm', 'Arm'), ('upperarm_twist', 'ArmTwist'), ('forearm', 'ForeArm'),
                   ('hand', 'Hand'), ('thumb', 'HandThumb1'), ('index', 'HandIndex1'), ('middle', 'HandMiddle1'),
                   ('pinky', 'HandPinky1'), ('thigh', 'UpLeg'), ('shin', 'Leg'), ('foot', 'Foot'), ('toe', 'ToeBase'))}},
    # Generic names carry no anatomy at all.
    'generic': None,
    # Blender Rigify deform layer.
    'rigify': {'root': 'root', 'pelvis': 'DEF-spine', 'spine': 'DEF-spine.001', 'chest': 'DEF-spine.003',
               'neck': 'DEF-spine.004', 'head': 'DEF-spine.006', 'jaw': 'DEF-jaw',
               'hair_0': 'DEF-hair', 'hair_1': 'DEF-hair.001', 'hair_2': 'DEF-hair.002',
               **{f'{k}_{s}': f'DEF-{v}{x}' for s, x in (('L', L), ('R', R)) for k, v in
                  (('clavicle', 'shoulder'), ('upperarm', 'upper_arm'), ('upperarm_twist', 'upper_arm.001'.replace('.001', '_twist')),
                   ('forearm', 'forearm'), ('hand', 'hand'), ('thumb', 'thumb.01'), ('index', 'f_index.01'),
                   ('middle', 'f_middle.01'), ('pinky', 'f_pinky.01'), ('thigh', 'thigh'), ('shin', 'shin'),
                   ('foot', 'foot'), ('toe', 'toe'))}},
    # DAZ Genesis 8.
    'daz': {'root': 'Genesis8Female', 'pelvis': 'hip', 'spine': 'abdomenLower', 'chest': 'chestUpper', 'neck': 'neckLower',
            'head': 'head', 'jaw': 'lowerJaw', 'hair_0': 'hairA', 'hair_1': 'hairB', 'hair_2': 'hairC',
            **{f'{k}_{s}': f'{p}{v}' for s, p in (('L', 'l'), ('R', 'r')) for k, v in
               (('clavicle', 'Collar'), ('upperarm', 'ShldrBend'), ('upperarm_twist', 'ShldrTwist'), ('forearm', 'ForearmBend'),
                ('hand', 'Hand'), ('thumb', 'Thumb1'), ('index', 'Index1'), ('middle', 'Mid1'), ('pinky', 'Pinky1'),
                ('thigh', 'ThighBend'), ('shin', 'Shin'), ('foot', 'Foot'), ('toe', 'Toe'))}},
    # Unreal mannequin.
    'unreal': {'root': 'root', 'pelvis': 'pelvis', 'spine': 'spine_01', 'chest': 'spine_03', 'neck': 'neck_01', 'head': 'head',
               'jaw': 'jaw', 'hair_0': 'hair_01', 'hair_1': 'hair_02', 'hair_2': 'hair_03',
               **{f'{k}_{s}': f'{v}_{s.lower()}' for s in ('L', 'R') for k, v in
                  (('clavicle', 'clavicle'), ('upperarm', 'upperarm'), ('upperarm_twist', 'upperarm_twist_01'),
                   ('forearm', 'lowerarm'), ('hand', 'hand'), ('thumb', 'thumb_01'), ('index', 'index_01'),
                   ('middle', 'middle_01'), ('pinky', 'pinky_01'), ('thigh', 'thigh'), ('shin', 'calf'),
                   ('foot', 'foot'), ('toe', 'ball'))}},
    # Character Creator / iClone.
    'cc': {'root': 'CC_Base_BoneRoot', 'pelvis': 'CC_Base_Hip', 'spine': 'CC_Base_Waist', 'chest': 'CC_Base_Spine02',
           'neck': 'CC_Base_NeckTwist01', 'head': 'CC_Base_Head', 'jaw': 'CC_Base_JawRoot',
           'hair_0': 'Hair_Root', 'hair_1': 'Hair_01', 'hair_2': 'Hair_02',
           **{f'{k}_{s}': f'CC_Base_{s}_{v}' for s in ('L', 'R') for k, v in
              (('clavicle', 'Clavicle'), ('upperarm', 'Upperarm'), ('upperarm_twist', 'UpperarmTwist01'),
               ('forearm', 'Forearm'), ('hand', 'Hand'), ('thumb', 'Thumb1'), ('index', 'Index1'), ('middle', 'Mid1'),
               ('pinky', 'Pinky1'), ('thigh', 'Thigh'), ('shin', 'Calf'), ('foot', 'Foot'), ('toe', 'ToeBase'))}},
    # Hand-made rig with idiosyncratic names.
    'odd': {'root': 'Master', 'pelvis': 'Hip_Ctrl_DEF', 'spine': 'Torso_Low', 'chest': 'Torso_Up', 'neck': 'Nape',
            'head': 'Skull', 'jaw': 'Chin', 'hair_0': 'Tress1', 'hair_1': 'Tress2', 'hair_2': 'Tress3',
            **{f'{k}_{s}': f'{v}{x}' for s, x in (('L', '.L'), ('R', '.R')) for k, v in
               (('clavicle', 'Collarbone'), ('upperarm', 'Bicep'), ('upperarm_twist', 'Bicep_Roll'),
                ('forearm', 'Elbow'), ('hand', 'Palm'), ('thumb', 'Digit0'), ('index', 'Digit1'), ('middle', 'Digit2'),
                ('pinky', 'Digit4'), ('thigh', 'Femur'), ('shin', 'Tibia'), ('foot', 'Heel'), ('toe', 'Toes'))}},
}


def names(style):
    table = STYLES[style]
    if table is None:  # Bone, Bone.001, ... in creation order
        return {key: ('Bone' if i == 0 else 'Bone.%03d' % i) for i, (key, *_rest) in enumerate(LAYOUT)}
    return table


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def material(name, color=(.8, .6, .5, 1)):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = color
    return mat


def box(center, axis, length, width, depth=None):
    """Eight corners of a box along an axis."""
    axis = Vector(axis).normalized()
    up = Vector((0, 0, 1)) if abs(axis.z) < .9 else Vector((1, 0, 0))
    side = axis.cross(up).normalized()
    up = side.cross(axis).normalized()
    depth = width if depth is None else depth
    center = Vector(center)
    points = []
    for a in (-.5, .5):
        for b in (-.5, .5):
            for c in (-.5, .5):
                points.append(center + axis * (a * length) + side * (b * width) + up * (c * depth))
    faces = [(0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)]
    return points, faces


def mesh_object(name, parts, mat=None, collection=None):
    """parts: list of (points, faces, {group: weight})"""
    verts, faces, groups = [], [], []
    for points, quads, weights in parts:
        base = len(verts)
        verts += [tuple(p) for p in points]
        faces += [tuple(base + i for i in q) for q in quads]
        groups += [weights] * len(points)
    data = bpy.data.meshes.new(name)
    data.from_pydata(verts, [], faces)
    data.update()
    obj = bpy.data.objects.new(name, data)
    (collection or bpy.context.scene.collection).objects.link(obj)
    if mat is not None:
        data.materials.append(mat)
    for index, weights in enumerate(groups):
        for group, weight in (weights or {}).items():
            vg = obj.vertex_groups.get(group) or obj.vertex_groups.new(name=group)
            vg.add([index], weight, 'REPLACE')
    return obj


def humanoid(style='mixamo', name='Armature', offset=(0, 0, 0), scale=1.0, with_hair=True, body_name='Body'):
    table = names(style)
    data = bpy.data.armatures.new(name)
    rig = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')
    off = Vector(offset)
    for key, parent, head, tail, deform in LAYOUT:
        if not with_hair and key.startswith('hair'):
            continue
        bone = data.edit_bones.new(table[key])
        bone.head = Vector(head) * scale + off
        bone.tail = Vector(tail) * scale + off
        bone.use_deform = deform
        if parent:
            bone.parent = data.edit_bones[table[parent]]
    bpy.ops.object.mode_set(mode='OBJECT')
    parts = []
    for key, parent, head, tail, deform in LAYOUT:
        if not deform or (not with_hair and key.startswith('hair')):
            continue
        h, t = Vector(head) * scale + off, Vector(tail) * scale + off
        length = (t - h).length
        width = (.12 if key in ('pelvis', 'spine', 'chest') else .1 if key == 'head' else .035 if key.startswith(('thumb', 'index', 'middle', 'pinky')) else .06) * scale
        points, quads = box((h + t) / 2, t - h, length * .98, width)
        weights = {table[key]: 1.0}
        if key.startswith('upperarm_twist'):
            weights = {table[key]: .6, table['upperarm_' + key[-1]]: .4}
        parts.append((points, quads, weights))
    body = mesh_object(body_name, parts, material(body_name + '_mat'))
    body.parent = rig
    modifier = body.modifiers.new('Armature', 'ARMATURE')
    modifier.object = rig
    return rig, body, table


def pistol(name='Pistol', grip=(-.74, 0, 1.45), forward=(0, -1, 0), up=(0, 0, 1), scale=1.0):
    """A recognizable pistol: long barrel/slide forward of the grip, grip hanging below."""
    grip, forward, up = Vector(grip), Vector(forward).normalized(), Vector(up).normalized()
    slide_center = grip + forward * .06 * scale + up * .05 * scale
    parts = [box(slide_center, forward, .24 * scale, .035 * scale, .045 * scale) + ({},),
             box(grip - up * .02 * scale, up, .11 * scale, .03 * scale, .05 * scale) + ({},),
             box(slide_center + forward * .13 * scale, forward, .03 * scale, .02 * scale) + ({},)]
    return mesh_object(name, parts, material(name + '_mat', (.2, .2, .22, 1)))


def bone_parent(obj, rig, bone):
    world = obj.matrix_world.copy()
    obj.parent = rig
    obj.parent_type = 'BONE'
    obj.parent_bone = bone
    bpy.context.view_layer.update()
    obj.matrix_world = world


def clutter(rig, body, table):
    """Typical extra scene content from downloaded character files."""
    scene = bpy.context.scene
    cam = bpy.data.objects.new('Camera', bpy.data.cameras.new('Camera'))
    scene.collection.objects.link(cam)
    light = bpy.data.objects.new('Key light', bpy.data.lights.new('Key light', 'AREA'))
    scene.collection.objects.link(light)
    curve = bpy.data.curves.new('Hair guide', 'CURVE')
    spline = curve.splines.new('POLY'); spline.points.add(2)
    curve_obj = bpy.data.objects.new('Hair guide', curve); scene.collection.objects.link(curve_obj)
    text = bpy.data.objects.new('Credits', bpy.data.curves.new('Credits', 'FONT')); scene.collection.objects.link(text)
    empty = bpy.data.objects.new('Empty_ctrl', None); scene.collection.objects.link(empty)
    # Hidden alternate body (disabled for render) bound to the same rig.
    alt = body.copy(); alt.data = body.data.copy(); alt.name = 'Body_old'; scene.collection.objects.link(alt)
    alt.hide_render = True; alt.hide_set(True)
    # A ground plane far away with no weights.
    floor = mesh_object('Floor', [box((0, 0, -.02), (1, 0, 0), 6, 6, .02) + ({},)], material('Floor_mat'))
    # Accessory parented to the armature object without weights (follows the body only at rest).
    badge = mesh_object('Badge', [box((.06, -.07, 1.35), (1, 0, 0), .04, .04, .01) + ({},)])  # no material
    badge.parent = rig
    # Shared mesh data between two objects (instances).
    earring_l = mesh_object('Earring.L', [box((.07, 0, 1.62), (0, 0, 1), .03, .01) + ({table['head']: 1.},)], material('Gold'))
    earring_l.parent = rig; earring_l.modifiers.new('Armature', 'ARMATURE').object = rig
    earring_r = earring_l.copy(); earring_r.name = 'Earring.R'; scene.collection.objects.link(earring_r)
    earring_r.location.x = -.14
    # Generative modifiers before the armature modifier.
    body.modifiers.new('Subdivision', 'SUBSURF').levels = 1
    body.modifiers.move(len(body.modifiers) - 1, 0)
    # Mirrored accessory (negative scale) skinned to the chest.
    patch = mesh_object('Patch', [box((.08, -.07, 1.3), (1, 0, 0), .05, .05, .01) + ({table['chest']: 1.},)], material('Patch_mat'))
    patch.parent = rig; patch.modifiers.new('Armature', 'ARMATURE').object = rig
    patch.scale.x = -1
    # Partially weighted mesh: half the vertices have no weight.
    scarf = mesh_object('Scarf', [box((0, -.02, 1.5), (1, 0, 0), .2, .06) + ({table['neck']: 1.},),
                                  box((0, -.02, 1.43), (1, 0, 0), .2, .06) + ({},)], material('Scarf_mat'))
    scarf.parent = rig; scarf.modifiers.new('Armature', 'ARMATURE').object = rig
    # Second, unrelated character standing to the side.
    other_rig, other_body, _ = humanoid('unreal', name='NPC_Rig', offset=(2.5, 0, 0), with_hair=False, body_name='NPC_Body')
    bpy.context.view_layer.update()
    return {'camera': cam, 'light': light, 'curve': curve_obj, 'text': text, 'alt': alt, 'floor': floor,
            'badge': badge, 'earrings': (earring_l, earring_r), 'patch': patch, 'scarf': scarf,
            'npc': (other_rig, other_body)}


def save(path):
    bpy.context.view_layer.update()
    bpy.ops.wm.save_as_mainfile(filepath=str(path))


# ---------------------------------------------------------------- harder cases
def pose_bone(rig, name, degrees=35, axis='X'):
    from mathutils import Euler
    bone = rig.pose.bones[name]
    bone.rotation_mode = 'XYZ'
    angles = [0., 0., 0.]
    angles['XYZ'.index(axis)] = math.radians(degrees)
    bone.rotation_euler = Euler(angles, 'XYZ')
    bpy.context.view_layer.update()


def child_of(obj, rig, bone):
    """Attach with a Child Of constraint at the current (posed) placement."""
    world = obj.matrix_world.copy()
    constraint = obj.constraints.new('CHILD_OF')
    constraint.target = rig
    constraint.subtarget = bone
    bpy.context.view_layer.update()
    pose = rig.matrix_world @ rig.pose.bones[bone].matrix
    constraint.inverse_matrix = pose.inverted()
    bpy.context.view_layer.update()
    obj.matrix_world = world
    return constraint


def lay_down(objects=None):
    """Simulate a Y-up export opened without axis conversion (character lies along +Y)."""
    rot = Matrix.Rotation(math.radians(-90), 4, 'X')
    for obj in objects or [o for o in bpy.context.scene.objects if o.parent is None]:
        obj.matrix_world = rot @ obj.matrix_world
    bpy.context.view_layer.update()


def shape_keys_and_subsurf(body):
    body.shape_key_add(name='Basis')
    smile = body.shape_key_add(name='Smile')
    for point in smile.data:
        point.co.x *= 1.02
    smile.value = .5
    modifier = body.modifiers.new('Subdivision', 'SUBSURF')
    modifier.levels = 1
    body.modifiers.move(len(body.modifiers) - 1, 0)


def curve_hair(rig, bone):
    curve = bpy.data.curves.new('Hair curve', 'CURVE')
    curve.dimensions = '3D'
    curve.bevel_depth = .01
    spline = curve.splines.new('POLY')
    spline.points.add(2)
    for i, z in enumerate((1.78, 1.70, 1.62)):
        spline.points[i].co = (.05, .08, z, 1)
    obj = bpy.data.objects.new('Hair curve', curve)
    bpy.context.scene.collection.objects.link(obj)
    obj.data.materials.append(material('Hair_mat', (.1, .05, .02, 1)))
    bone_parent(obj, rig, bone)
    return obj


def into_collection_instance(objects, name='CharacterAsset'):
    """Move objects into an excluded collection and instance it with an empty."""
    collection = bpy.data.collections.new(name)
    bpy.context.scene.collection.children.link(collection)
    for obj in objects:
        for owner in list(obj.users_collection):
            owner.objects.unlink(obj)
        collection.objects.link(obj)
    layer = bpy.context.view_layer.layer_collection.children[collection.name]
    layer.exclude = True
    empty = bpy.data.objects.new(name + '_instance', None)
    empty.instance_type = 'COLLECTION'
    empty.instance_collection = collection
    bpy.context.scene.collection.objects.link(empty)
    return empty


def second_scene():
    scene = bpy.data.scenes.new('Lighting')
    light = bpy.data.objects.new('Rim', bpy.data.lights.new('Rim', 'POINT'))
    scene.collection.objects.link(light)
    return scene


def unrigged_character():
    """A static T-pose body (no armature) with scenery around it."""
    parts = []
    for key, parent, head, tail, deform in LAYOUT:
        if not deform or key.startswith(('hair', 'jaw', 'thumb', 'index', 'middle', 'pinky', 'upperarm_twist')):
            continue
        h, t = Vector(head), Vector(tail)
        width = .14 if key in ('pelvis', 'spine', 'chest') else .12 if key == 'head' else .07
        parts.append(box((h + t) / 2, t - h, (t - h).length, width) + ({},))
    body = mesh_object('Statue', parts, material('Statue_mat'))
    mesh_object('Floor', [box((0, 0, -.02), (1, 0, 0), 6, 6, .02) + ({},)], material('Floor_mat'))
    cam = bpy.data.objects.new('Camera', bpy.data.cameras.new('Camera'))
    bpy.context.scene.collection.objects.link(cam)
    return body


def export(path):
    path = str(path)
    if path.endswith('.fbx'):
        bpy.ops.export_scene.fbx(filepath=path, use_selection=False, add_leaf_bones=False, bake_anim=False)
    else:
        bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', export_animations=False)
