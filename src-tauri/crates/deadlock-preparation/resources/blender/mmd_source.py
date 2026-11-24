"""MMD models (.pmx, .pmd) through the MMD Tools extension installed in the
Blender the app uses. The add-on is never part of this app: it is the copy the
user added in Blender (Edit > Preferences > Get Extensions).

Blender runs with --factory-startup, so no add-on is on. `enable` turns the
installed extension on for this process only: `refresh_handled` keeps Blender
from synchronising the user's extension wheels (which writes to the user's
Blender folder), and the add-on's own wheels are unpacked into the job
workspace instead. `default_set` gives the add-on its default preferences (its
shared toon folder), as in a normal Blender session; preferences are never
saved in a factory-startup session.

The import settings are the add-on's defaults (MMD Tools 4.5), written out so
a change of the add-on's defaults never changes the app's results silently.
Also used by deadlock-preview's preview_export.py (one copy).
"""
import glob
import logging
import os
import sys
import zipfile

SUFFIXES = ('.pmx', '.pmd')
ADDON_ID = 'mmd_tools'
ADDON_NAME = 'MMD Tools'
# troubleshooting.rs matches these phrases (tests read them from this file).
MISSING = 'MMD Tools is not installed for Blender {version}'
NOT_ENABLED = 'MMD Tools is installed for Blender {version} but could not be turned on'
NOT_READ = 'MMD Tools could not read'
# The add-on's page, which explains installing it (also in toolLinks.ts).
PAGE = 'https://extensions.blender.org/add-ons/mmd-tools/'

IMPORT_SETTINGS = {
    'types': {'MESH', 'ARMATURE', 'PHYSICS', 'DISPLAY', 'MORPHS'},
    'scale': 0.08,
    'clean_model': True,
    'remove_doubles': False,
    'import_adduv2_as_vertex_colors': False,
    'fix_bone_order': True,
    'fix_ik_links': False,
    'ik_loop_factor': 5,
    'apply_bone_fixed_axis': False,
    'rename_bones': True,
    'use_underscore': False,
    'dictionary': 'DISABLED',
    'bone_disp_mode': 'OCTAHEDRAL',
    'use_mipmap': True,
    'sph_blend_factor': 1.0,
    'spa_blend_factor': 1.0,
    'log_level': 'INFO',
    'save_log': False,
}


def is_mmd(path):
    return os.path.splitext(str(path))[1].lower() in SUFFIXES


def _version(bpy):
    return '{}.{}'.format(*bpy.app.version[:2])


def _registered(bpy):
    # `hasattr(bpy.ops.mmd_tools, ...)` is always true; the RNA type is not.
    try:
        bpy.ops.mmd_tools.import_model.get_rna_type()
        return True
    except (KeyError, AttributeError, RuntimeError):
        return False


def installed(bpy):
    """(module name, folder) of the extension in each enabled repository."""
    found = []
    for repo in bpy.context.preferences.extensions.repos:
        if not repo.enabled or not repo.directory:
            continue
        folder = os.path.join(repo.directory, ADDON_ID)
        if os.path.isfile(os.path.join(folder, 'blender_manifest.toml')) and os.path.isfile(os.path.join(folder, '__init__.py')):
            found.append(('bl_ext.' + repo.module + '.' + ADDON_ID, folder))
    return found


def _wheels(folder, workspace):
    """The add-on's bundled wheels, unpacked into the workspace (never into Blender)."""
    wheels = sorted(glob.glob(os.path.join(folder, 'wheels', '*.whl')))
    if not wheels:
        return
    target = os.path.join(str(workspace), 'mmd_tools_wheels')
    os.makedirs(target, exist_ok=True)
    for wheel in wheels:
        with zipfile.ZipFile(wheel) as archive:
            for name in archive.namelist():
                parts = name.replace('\\', '/').split('/')
                if name.startswith('/') or '..' in parts or ':' in parts[0]:
                    raise ValueError('MMD Tools wheel holds an unsafe path: ' + name)
            archive.extractall(target)
    if target not in sys.path:
        sys.path.insert(0, target)


def enable(bpy, workspace):
    """Turn the installed MMD Tools on, or stop with a plain message."""
    import addon_utils
    version = _version(bpy)
    if _registered(bpy):
        return
    found = installed(bpy)
    if not found:
        raise ValueError(MISSING.format(version=version) + ', the Blender set in Settings. '
                         '.pmx and .pmd files are imported with this Blender add-on. In Blender ' + version +
                         ', open Edit > Preferences > Get Extensions, search for MMD Tools and click Install, then prepare again. '
                         'The MMD Tools page explains the installation: ' + PAGE)
    errors = []
    module, folder = found[0]
    try:
        _wheels(folder, workspace)
        result = addon_utils.enable(module, default_set=True, persistent=False, refresh_handled=True,
                                    handle_error=lambda ex: errors.append(str(ex)))
    except Exception as ex:  # an add-on can fail in any way while loading
        result = None
        errors.append(str(ex))
    if result is None or not _registered(bpy):
        detail = errors[-1].strip().splitlines()[-1] if errors and errors[-1].strip() else 'no import operator'
        raise ValueError(NOT_ENABLED.format(version=version) + ' (' + detail + '). '
                         'Update or reinstall MMD Tools in Blender ' + version + ' (Edit > Preferences > Get Extensions), then prepare again.')


class _Errors(logging.Handler):
    """Exceptions the add-on logged. Its plain error lines (a PMD without a
    base morph) are not fatal; a cut-off file ("Corrupted file") is: the
    add-on would go on with part of the model."""
    def __init__(self):
        super().__init__(logging.ERROR)
        self.messages = []

    def emit(self, record):
        if not record.exc_info or record.exc_info[1] is None:
            return
        error = record.exc_info[1]
        text = str(error).strip() or type(error).__name__
        if type(error).__name__ == 'error' and type(error).__module__ == 'struct':
            text = 'the file ends early or is damaged (' + text + ')'
        self.messages.append(text.splitlines()[-1])


def import_model(bpy, path):
    """Import one .pmx/.pmd with the add-on's default settings.

    The add-on's operator reports read errors and still returns FINISHED, so
    exceptions are read from its log and the result must hold a mesh."""
    name = os.path.basename(str(path))
    before = set(bpy.data.objects)
    errors = _Errors()
    root = logging.getLogger()
    root.addHandler(errors)
    try:
        bpy.ops.mmd_tools.import_model(filepath=str(path), **IMPORT_SETTINGS)
    except RuntimeError as ex:
        # A background operator's ERROR report: "Error: Failed to import model from ...".
        text = str(ex).strip().splitlines()[-1] if str(ex).strip() else 'import failed'
        errors.messages.append(text[len('Error: '):] if text.startswith('Error: ') else text)
    finally:
        root.removeHandler(errors)
    added = [obj for obj in bpy.data.objects if obj not in before]
    if errors.messages or not any(obj.type == 'MESH' for obj in added):
        # The first error is the cause (a cut-off file); later ones follow from it.
        detail = errors.messages[0] if errors.messages else 'no mesh was imported'
        raise ValueError(NOT_READ + ' ' + name + ': ' + detail + '. Check that the file opens in MikuMikuDance or PMX Editor, '
                         'or import it in Blender with MMD Tools and choose the saved .blend file instead.')
    if bpy.context.object and bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')
    return added
