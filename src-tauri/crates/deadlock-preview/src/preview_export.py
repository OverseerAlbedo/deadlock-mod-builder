"""Executed by Blender only: export a disposable preview, never save the source."""
import hashlib
import json
from pathlib import Path
import sys
import os
import stat
sys.path.insert(0, str(Path(__file__).parent))
from source_helpers import mmd_helpers, helper_receipt


def checked_path(path):
    """Reject links before canonicalization, including Windows reparse points."""
    path = Path(os.path.abspath(path))
    for item in (*reversed(path.parents), path):
        info = item.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
            raise RuntimeError('Linked preview input is not supported: ' + str(item))
    if not path.is_file() or path.stat().st_size > 512 * 1024 * 1024:
        raise RuntimeError('Preview dependency is missing or exceeds 512 MiB: ' + str(path))
    return path


def sha(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024*1024), b''):
            result.update(block)
    return result.hexdigest()


def export_glb(exporter, output, **settings):
    r"""Keep Windows verbatim paths out of the glTF add-on's slash joining.

    Rust/Python may canonicalize an output to \\?\ paths. The add-on appends
    '/' to dirname(filepath), which is invalid in that namespace. Export from
    the existing private output directory using './name.glb'; retain the full
    canonical path for our own I/O and integrity checks. Do not strip prefixes
    (which breaks long paths and UNC shares) or change the original .blend.
    """
    previous = os.getcwd()
    try:
        os.chdir(output.parent)
        result = exporter(filepath=os.path.join('.', output.name), export_format='GLB', **settings)
        if result != {'FINISHED'}:
            raise RuntimeError('Blender did not finish the preview GLB export.')
    finally:
        os.chdir(previous)


def main():
    import bpy
    job = json.loads(Path(sys.argv[sys.argv.index('--')+1]).read_text(encoding='utf-8'))
    source, output = checked_path(job['source']), Path(job['output']).resolve()
    if output == source or output.suffix.lower() != '.glb':
        raise RuntimeError('Preview output must be a separate GLB.')
    before = sha(source)
    bpy.context.preferences.filepaths.use_scripts_auto_execute = False
    if source.suffix.lower() == '.blend':
        bpy.ops.wm.open_mainfile(filepath=str(source), load_ui=False, use_scripts=False)
    elif source.suffix.lower() == '.fbx':
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.import_scene.fbx(filepath=str(source), use_anim=True)
    else:
        raise RuntimeError('Unsupported preview conversion source.')
    dependencies = {str(source): before}
    warnings = ['Blender preview is a GLB snapshot; Blender-specific shaders, constraints and effects may differ.']
    helpers = mmd_helpers(bpy.context.scene.objects)
    helper_info = helper_receipt(helpers)
    if helpers:
        warnings.append(helper_info['warning'])
        shapes = {bone.custom_shape for obj in bpy.context.scene.objects if obj.type == 'ARMATURE'
                  for bone in obj.pose.bones if bone.custom_shape}
        for obj in bpy.context.scene.objects:
            obj.select_set(obj not in helpers and obj not in shapes)
    for library in bpy.data.libraries:
        path = checked_path(bpy.path.abspath(library.filepath, library=library.parent))
        if not path.is_file():
            raise RuntimeError('Missing linked Blender library: ' + str(path))
        dependencies[str(path)] = sha(path)
    for image in bpy.data.images:
        if image.source != 'FILE' or image.packed_file:
            continue
        path = checked_path(bpy.path.abspath(image.filepath, library=image.library))
        if not path.is_file():
            raise RuntimeError('Missing external Blender texture: ' + str(path))
        dependencies[str(path)] = sha(path)
    if any(mod.type in ('CLOTH', 'SOFT_BODY', 'FLUID') for obj in bpy.data.objects for mod in obj.modifiers):
        warnings.append('Simulation modifiers are shown at the saved frame only; this viewer does not replay Blender cloth or fluids.')
    if any(getattr(obj, 'animation_data', None) and obj.animation_data.drivers for obj in bpy.data.objects):
        warnings.append('Automatic Python execution is disabled; scripted drivers may not match the Blender source.')
    export_glb(bpy.ops.export_scene.gltf, output, use_selection=bool(helpers),
                              export_animations=True, export_skins=True, export_morph=True,
                              export_all_influences=True, export_rest_position_armature=True,
                              export_yup=True, export_apply=False)
    if sha(source) != before or any(sha(path) != value for path, value in dependencies.items()):
        raise RuntimeError('A source model or linked resource changed during preview export.')
    if sum(checked_path(path).stat().st_size for path in dependencies) > 1024 * 1024 * 1024:
        raise RuntimeError('Preview dependencies exceed 1 GiB.')
    result = {'status': 'ok', 'source_sha256': before, 'output_sha256': sha(output),
              'dependencies': dependencies, 'warnings': warnings, 'source_helpers': helper_info,
              'blender_version': bpy.app.version_string}
    Path(job['receipt']).write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding='utf-8')


if __name__ == '__main__':
    main()
