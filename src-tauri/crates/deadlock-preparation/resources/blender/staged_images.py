"""Images of a staged model whose copy sits at a shorter path.

Blender cannot open a path of 260 characters or more, so the app stages an
image whose copy would be that long directly beside the staged model
(source.rs `shorter_copies`): under its own name, where Blender's FBX
importer finds it by name, or under a short name. The staging manifest beside
the model lists each such copy: `origin` is the image's path relative to the
model's folder, as the model names it, and `relative` its copy.

`load` runs an import so that Blender uses these copies: the FBX importer's
image search is pointed at them, and an image still missing after the import
(glTF, which reads exact paths) is relinked. The copy's pixels are packed and
the image keeps the path the model asked for, so its name and the texture
detection in material_textures.py stay what they are for any other model.
A model without such copies is imported exactly as before.

An image the model names under one extension and that exists only under
another (`body.tga` asked for, `body.jpg` beside the model) is staged the same
way, as a stand-in: its `origin` has the extension asked for and `found` says
which file it is. The stand-in is used exactly like a shorter copy, so the
image keeps the name and path the model asked for while its pixels are the
file's. When several files stand in for one image, none is used and they are
offered as choices (`choices`). Images staged for materials by file name
(`materials` in the manifest, decided at staging by texture_names.rs) are
listed by `name_matches`; material_textures.py decides whether to use them.
"""
import json
import os
from contextlib import contextmanager, nullcontext
from pathlib import Path

MANIFEST = 'source-manifest.json'

# What the imports of this Blender session found, for material_textures.py.
# Images loaded from a stand-in: image pointer -> (name asked for, file used).
_USED = {}
# The same notes by the folded path asked for.
_USED_PATHS = {}
# Several stand-ins for one image: folded file name asked for -> staged copies.
_CHOICES = {}
# Images matched to materials by name: material name -> staged files.
_MATCHES = {}
# The user's file name of each staged copy (`found`), by copy path.
_FOUND = {}


def _key(path):
    return os.path.normpath(str(path)).replace('\\', '/').lower()


def _entries(model):
    folder = Path(model).parent
    try:
        entries = json.loads((folder / MANIFEST).read_text(encoding='utf-8'))
    except (OSError, ValueError):
        return folder, []
    return folder, [e for e in entries if isinstance(e, dict)] if isinstance(entries, list) else []


def _plain(relative):
    """Copies sit directly in the staging folder."""
    return (isinstance(relative, str) and relative and len(Path(relative).parts) == 1
            and not Path(relative).is_absolute() and relative not in {'.', '..'})


def _copies(model):
    """({folded path the model asks for: staged copy}, {such path: [copies]}
    for paths with several copies, which none of them stands for alone)."""
    folder, entries = _entries(model)
    found = {}
    for entry in entries:
        origin, relative = entry.get('origin'), entry.get('relative')
        if not (isinstance(origin, str) and origin and _plain(relative)):
            continue
        copy = str(folder / relative)
        found.setdefault(_key(os.path.join(str(folder), origin)), []).append(copy)
        name = entry.get('found') if isinstance(entry.get('found'), str) else origin
        _FOUND[copy] = name.replace('\\', '/').rsplit('/', 1)[-1]
    # Stand-ins (the extension differs from the copy's) are found by the file
    # name asked for, so files of that name in different folders are several
    # choices too, never one picked by where it lies.
    by_name = {}
    for key, paths in found.items():
        for copy in paths:
            if Path(copy).suffix.lower() != Path(key).suffix.lower():
                by_name.setdefault(key.rsplit('/', 1)[-1], set()).add(copy)
    shared = {name for name, paths in by_name.items() if len(paths) > 1}
    copies, several = {}, {}
    for key, paths in found.items():
        if len(paths) == 1 and key.rsplit('/', 1)[-1] not in shared:
            copies[key] = paths[0]
        else:
            several.setdefault(key, []).extend(paths)
    return copies, several


def relocated(model):
    """{folded path the model asks for: staged copy} from the manifest beside `model`."""
    return _copies(model)[0]


def name_matches(model=None):
    """{material name: [staged image files]} that staging matched by file
    name, from the manifest beside `model`, or from every model imported in
    this session when `model` is None."""
    if model is None:
        return {name: list(paths) for name, paths in _MATCHES.items()}
    folder, entries = _entries(model)
    matches = {}
    for entry in entries:
        relative, materials = entry.get('relative'), entry.get('materials')
        if not (isinstance(relative, str) and relative and isinstance(materials, list)):
            continue
        path = folder / relative
        if not path.resolve().is_relative_to(folder.resolve()):
            continue
        for name in materials:
            if isinstance(name, str) and str(path) not in matches.setdefault(name, []):
                matches[name].append(str(path))
    return matches


def stand_in(image):
    """(file name the model asks for, file used) when `image` was loaded from
    a stand-in, or is a packed copy of one (the FBX importer copies an image
    it also wires to Alpha)."""
    used = _USED.get(image.as_pointer())
    if used is None and image.packed_file and image.filepath:
        used = _USED_PATHS.get(_key(image.filepath))
    return used


def choices(image):
    """The staged files that could each stand in for `image`, which asks for
    a file that is missing; empty when there are none or `image` has pixels."""
    if not image.filepath:
        return []
    name = image.filepath.replace('\\', '/').rsplit('/', 1)[-1].lower()
    return list(_CHOICES.get(name, []))


def found_name(copy):
    """The user's file name for a staged copy."""
    return _FOUND.get(str(copy), Path(copy).name)


def _copy(bpy, copy, requested):
    """The copy as a packed image that keeps the path the model asked for."""
    try:
        image = bpy.data.images.load(copy)
    except RuntimeError:
        return None
    image.pack()
    image.filepath_raw = requested
    if Path(copy).suffix.lower() != Path(requested.replace('\\', '/')).suffix.lower():
        note = (requested.replace('\\', '/').rsplit('/', 1)[-1], found_name(copy))
        _USED[image.as_pointer()] = note
        _USED_PATHS[_key(requested)] = note
    return image


@contextmanager
def _fbx_search(bpy, copies, several=None):
    """Point the FBX importer's image search (bpy_extras.image_utils.load_image)
    at the copies: an image the model names by its path, or, when the importer
    finds nothing at that path or by its own search, the one copy of that name."""
    from bpy_extras import image_utils
    original = image_utils.load_image
    # By name only when one copy has it: two different images of that name
    # must never stand in for one another.
    names = {}
    for key, copy in copies.items():
        names.setdefault(key.rsplit('/', 1)[-1], []).append(copy)
    for key, paths in (several or {}).items():
        names.setdefault(key.rsplit('/', 1)[-1], []).extend(paths)

    def load_image(imagepath, dirname='', place_holder=False, recursive=False, **options):
        if not isinstance(imagepath, str) or not imagepath:
            return original(imagepath, dirname, place_holder=place_holder, recursive=recursive, **options)
        copy = copies.get(_key(imagepath))
        if copy is None:
            image = original(imagepath, dirname, place_holder=False, recursive=recursive, **options)
            if image is not None:
                return image
            same = names.get(_key(imagepath).rsplit('/', 1)[-1], [])
            copy = same[0] if len(same) == 1 else None
        image = _copy(bpy, copy, imagepath) if copy else None
        if image is None:
            return original(imagepath, '', place_holder=place_holder, recursive=False, **options)
        return image

    image_utils.load_image = load_image
    try:
        yield
    finally:
        image_utils.load_image = original


def _relink(bpy, copies, prior):
    """Replace each image of this import that has no pixels and asks for a
    path with a copy by that copy, keeping its name and settings."""
    for image in [i for i in bpy.data.images if i.as_pointer() not in prior]:
        if image.packed_file or image.source != 'FILE' or not image.filepath:
            continue
        requested = bpy.path.abspath(image.filepath)
        copy = copies.get(_key(requested))
        if copy is None or os.path.isfile(requested):
            continue
        replacement = _copy(bpy, copy, requested)
        if replacement is None:
            continue
        name = image.name
        replacement.alpha_mode = image.alpha_mode
        replacement.colorspace_settings.name = image.colorspace_settings.name
        image.user_remap(replacement)
        bpy.data.images.remove(image)
        replacement.name = name


def load(bpy, model, run):
    """Run the import `run` of the staged model `model` with its shorter copies
    and stand-ins."""
    copies, several = _copies(model)
    for key, paths in several.items():
        known = _CHOICES.setdefault(key.rsplit('/', 1)[-1], [])
        known.extend(p for p in paths if p not in known)
    for name, paths in name_matches(model).items():
        known = _MATCHES.setdefault(name, [])
        known.extend(p for p in paths if p not in known)
    if not copies and not several:
        return run()
    prior = {i.as_pointer() for i in bpy.data.images}
    with _fbx_search(bpy, copies, several) if Path(model).suffix.lower() == '.fbx' else nullcontext():
        result = run()
    _relink(bpy, copies, prior)
    return result
