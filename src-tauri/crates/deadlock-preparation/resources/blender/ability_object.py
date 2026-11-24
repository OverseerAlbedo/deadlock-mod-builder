"""Ability object preparation: the mesh objects of a file as one neutral packet.

Executed only by the configured Blender with autoexec disabled, as its own
--python entry (never through worker.py). The staged file (source.rs staging:
manifests, shorter image copies and stand-ins through staged_images.load) is
imported, every visible mesh object is evaluated (modifiers and any pose
applied), triangulated and written in world space as
the app's prepared-mesh form: positions, corner normals, the first UV map and
each polygon's material path. Materials are read by material_textures.py
(texture_and_materials, imported, never changed): each material's colour image
and basic values, copied beside the packet. A small preview copy (at most
PREVIEW_TRIANGLES) is added when the object is larger.

The job (job.json) holds: source, root (the object's folder in the game, the
material paths' prefix), packet (where to write it), result, workspace (the
material images are copied to its textures folder).
"""
import sys
import json
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
import bpy
import bmesh
import numpy as np
import mesh_export as mx
import staged_images
from material_textures import texture_and_materials

VERSION = 'ability-object-packet-v2'
PREVIEW_TRIANGLES = 20000
MAX_VERTICES = 2_000_000
DEFAULT_MATERIAL = 'dmb_object_default'


def import_file(path):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    suffix = Path(path).suffix.lower()
    if suffix in {'.glb', '.gltf'}:
        staged_images.load(bpy, path, lambda: bpy.ops.import_scene.gltf(filepath=path, merge_vertices=False))
    elif suffix == '.fbx':
        staged_images.load(bpy, path, lambda: bpy.ops.import_scene.fbx(filepath=path, use_anim=False))
    elif suffix == '.blend':
        bpy.ops.wm.open_mainfile(filepath=path, load_ui=False, use_scripts=False)
        if any(lib for lib in bpy.data.libraries):
            raise ValueError('This Blender file uses linked libraries. In a separate copy, make the object and its materials local, then choose it again.')
    else:
        raise ValueError('An ability object comes from a Blender, FBX, GLB or glTF file.')
    if bpy.context.object and bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')


def baked_objects():
    """Each visible mesh object as a new object holding its evaluated,
    triangulated mesh in world space (modifiers and pose applied)."""
    sources = [o for o in bpy.context.scene.objects if o.type == 'MESH' and o.visible_get() and not o.hide_render]
    if not sources:
        raise ValueError('The file has no visible mesh object to use as the ability object.')
    # Armatures deforming the meshes: their bone names and the meshes' weights on them are kept
    # (for a game object with several bones). The meshes are evaluated as before (pose applied):
    # a file for the same-skeleton way should hold its armature at rest.
    rigs = []
    for obj in sources:
        for modifier in obj.modifiers:
            if modifier.type == 'ARMATURE' and modifier.object is not None and modifier.object.type == 'ARMATURE' and modifier.object not in rigs:
                rigs.append(modifier.object)
    graph = bpy.context.evaluated_depsgraph_get()
    bone_names = {b.name for rig in rigs for b in rig.data.bones}
    baked = []
    notes = []
    fallback = None
    for obj in sources:
        mesh = bpy.data.meshes.new_from_object(obj.evaluated_get(graph), preserve_all_data_layers=True, depsgraph=graph)
        if not mesh.polygons:
            bpy.data.meshes.remove(mesh)
            continue
        matrix = obj.matrix_world.copy()
        if matrix.to_3x3().determinant() <= 0:
            raise ValueError('Mirrored or flat object transform on ' + obj.name + ': apply its transforms in a copy of the file first.')
        mesh.transform(matrix)
        bm = bmesh.new()
        bm.from_mesh(mesh)
        bmesh.ops.triangulate(bm, faces=bm.faces[:])
        bm.to_mesh(mesh)
        bm.free()
        # The object's own slot materials (object-linked slots too); empty slots get one plain material.
        slots = [s.material for s in obj.material_slots]
        if not slots:
            slots = [None]
        # Clearing the materials sets every polygon's material index to 0: keep
        # them, clamped to the last slot (a one-slot file stays all 0).
        indices = [min(p.material_index, len(slots) - 1) for p in mesh.polygons]
        mesh.materials.clear()
        for material in slots:
            if material is None:
                if fallback is None:
                    fallback = bpy.data.materials.get(DEFAULT_MATERIAL) or bpy.data.materials.new(DEFAULT_MATERIAL)
                    fallback.diffuse_color = (0.8, 0.8, 0.8, 1.0)
                    notes.append('Polygons without a material use a plain grey one.')
                material = fallback
            mesh.materials.append(material)
        mesh.polygons.foreach_set('material_index', indices)
        copy = bpy.data.objects.new(obj.name, mesh)
        # The copy's name gets a suffix beside its source; the part keeps the source's own name.
        copy['dmb_part'] = obj.name
        if bone_names:
            names = [g.name for g in obj.vertex_groups]
            copy['dmb_weights'] = json.dumps([[[names[g.group], g.weight] for g in v.groups if g.group < len(names) and names[g.group] in bone_names and g.weight > 0] for v in mesh.vertices])
        bpy.context.scene.collection.objects.link(copy)
        baked.append(copy)
    if not baked:
        raise ValueError('The file has no polygons to use as the ability object.')
    if sum(len(o.data.vertices) for o in baked) > MAX_VERTICES:
        raise ValueError('An ability object is limited to two million vertices.')
    if len(rigs) > 1:
        notes.append('The file has several armatures; their bones are listed together.')
    armature = [{'name': b.name, 'parent': b.parent.name if b.parent else None} for rig in rigs for b in rig.data.bones]
    return baked, notes, armature


def exported(objects, paths, with_uvs=True):
    out = []
    for obj in objects:
        mesh = obj.data
        normals = mx.transformed_normals(mx.corner_normals(mesh), np.eye(3), 'Invalid normal on ' + obj.name, 0.)
        uvs = mx.uvs(mesh)[:1] if with_uvs else []
        entry = {'name': obj.name, 'positions': mx.positions(mesh).tolist(), 'normals': normals,
                 'uvs': uvs, 'faces': mx.faces(mesh, mx.material_paths(obj, paths))}
        if 'dmb_part' in obj:
            entry['part'] = obj['dmb_part']
        if 'dmb_weights' in obj:
            entry['weights'] = json.loads(obj['dmb_weights'])
        out.append(entry)
    return out


def triangles(objects):
    return sum(len(o.data.polygons) for o in objects)


def preview(objects, paths):
    """A lighter copy for the fit view, when the object is large."""
    total = triangles(objects)
    if total <= PREVIEW_TRIANGLES:
        return None
    ratio = PREVIEW_TRIANGLES / total
    graph = bpy.context.evaluated_depsgraph_get()
    light = []
    for obj in objects:
        modifier = obj.modifiers.new('dmb preview', 'DECIMATE')
        modifier.ratio = ratio
        bpy.context.view_layer.update()
        graph = bpy.context.evaluated_depsgraph_get()
        mesh = bpy.data.meshes.new_from_object(obj.evaluated_get(graph), depsgraph=graph)
        obj.modifiers.remove(modifier)
        bm = bmesh.new()
        bm.from_mesh(mesh)
        bmesh.ops.triangulate(bm, faces=bm.faces[:])
        bm.to_mesh(mesh)
        bm.free()
        copy = bpy.data.objects.new(obj.name + ' preview', mesh)
        for i, slot in enumerate(obj.material_slots):
            if i < len(copy.material_slots):
                copy.material_slots[i].material = slot.material
        light.append(copy)
    return exported(light, paths, with_uvs=False)


def main():
    job = json.loads(Path(sys.argv[sys.argv.index('--') + 1]).read_text(encoding='utf-8'))
    try:
        import_file(job['source'])
        unit = float(bpy.context.scene.unit_settings.scale_length or 1.0)
        objects, notes, armature = baked_objects()
        materials, paths, warnings = texture_and_materials(objects, Path(job['workspace']) / 'textures', job['root'])
        packet = {'version': VERSION, 'unit_metres': unit, 'meshes': exported(objects, paths), 'materials': materials,
                  'warnings': notes + warnings}
        if armature:
            packet['armature'] = armature
        light = preview(objects, paths)
        if light is not None:
            packet['preview'] = light
        Path(job['packet']).write_text(json.dumps(packet, allow_nan=False), encoding='utf-8')
        Path(job['result']).write_text(json.dumps({'status': 'ok', 'version': VERSION, 'meshes': len(objects),
                                                   'vertices': sum(len(o.data.vertices) for o in objects),
                                                   'triangles': triangles(objects), 'materials': len(materials),
                                                   'warnings': packet['warnings']}, allow_nan=False), encoding='utf-8')
    except Exception as error:
        Path(job['result']).write_text(json.dumps({'status': 'error', 'error': str(error)}), encoding='utf-8')
        raise


main()
