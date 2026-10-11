"""Snapshot a user Blender source without moving its relative texture base.

Called inside the external, autoexec-disabled Blender process after the usual
mesh/rig checks. The original is opened read-only; only a distinct workspace
copy is saved. Packing happens before saving to retain // texture resolution.
This imports rest geometry/materials, not animation or simulation authoring.
"""
import hashlib
from source_helpers import uses_linked_libraries
from pathlib import Path


def snapshot_blend(bpy, source, destination):
    source, destination = Path(source).resolve(), Path(destination).resolve()
    if source == destination:
        raise ValueError('The Blender source and staged copy must be different files.')
    if uses_linked_libraries(bpy):
        # Linked characters (asset libraries) are copied into the workspace
        # snapshot as local data; the original files stay untouched.
        for _ in range(4):
            linked = [block for block in getattr(bpy.data, 'all_ids', bpy.data.user_map().keys()) if block.library is not None]
            if not linked:
                break
            for block in linked:
                try:
                    block.make_local()
                except (RuntimeError, ReferenceError, AttributeError):
                    pass
        if uses_linked_libraries(bpy):
            raise ValueError('Linked Blender libraries could not be made local. In a separate source copy, use File > External Data / Make Local, then import again.')
    packed = []
    warnings = []
    for image in bpy.data.images:
        if not image.users or image.source == 'VIEWER':
            continue
        if image.source not in {'FILE', 'GENERATED'} or getattr(image, 'is_multiview', False):
            warnings.append('Skipped texture '+image.name+': image sequences, movies, tiled (UDIM) and multiview textures are not imported. Choose an image for that material in Appearance.')
            continue
        if image.packed_file:
            raw = bytes(image.packed_file.data)
            if not raw:
                warnings.append('Skipped empty packed texture '+image.name+'.')
                continue
            packed.append({'image': image.name, 'sha256': hashlib.sha256(raw).hexdigest(),
                           'origin': 'already-packed'})
            continue
        if image.source == 'GENERATED':
            warnings.append('Skipped generated (unsaved) texture '+image.name+'. Save it as PNG to use it.')
            continue
        if image.is_dirty:
            warnings.append('Texture '+image.name+' has unsaved paint changes; the saved file on disk is used.')
        path = Path(bpy.path.abspath(image.filepath, library=image.library)).resolve()
        if not path.is_file():
            warnings.append('Missing texture file '+str(path)+' for '+image.name+'. Its material keeps a flat colour; choose an image in Appearance or restore the file and import again.')
            continue
        before = hashlib.sha256(path.read_bytes()).hexdigest()
        image.reload()
        image.pack()
        if (not image.packed_file or hashlib.sha256(bytes(image.packed_file.data)).hexdigest() != before
                or hashlib.sha256(path.read_bytes()).hexdigest() != before):
            raise ValueError('Blender texture changed or could not be packed exactly: '+str(path))
        packed.append({'image': image.name, 'sha256': before, 'origin': str(path)})
    destination.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(destination), copy=True)
    if not destination.is_file():
        raise ValueError('Blender did not save the packed source copy.')
    return {'status': 'ok', 'packed_images': packed, 'warnings': warnings, 'active_scene': bpy.context.scene.name,
            'scripts_enabled': False, 'imports_animations': False, 'imports_cloth_simulation': False}
