from __future__ import annotations
import hashlib
import json
import re
from pathlib import Path
from .kv3 import Document, field_edit
from .safety import BuildError, relative_asset

KEEP, DISABLE = "@keep", "@disable"


def targets(value):
    """Backward compatible string or ordered, nonempty list of source meshes."""
    if isinstance(value, str):
        if value in {KEEP, DISABLE}:
            return []
        if value:
            return [value]
    if (isinstance(value, list) and value and all(isinstance(x, str) and x and x not in {KEEP, DISABLE} for x in value)
            and len(set(value)) == len(value)):
        return list(value)
    raise BuildError("Mesh mapping must be a mesh name, unique nonempty mesh list, Keep Original or Disable.")


def selected_meshes(mappings):
    return {target for value in mappings.values() for target in targets(value)}


def expanded_mappings(mappings):
    """Stable output names, also used when validating compiled CTRL mesh identities."""
    expanded = {}
    for name, value in mappings.items():
        pieces = targets(value)
        rows = [(name, value)] if len(pieces) <= 1 else [(f"{name}__part{i:02}", t) for i, t in enumerate(pieces, 1)]
        for output, target in rows:
            if output in expanded or (output != name and output in mappings):
                raise BuildError(f"Split mesh name collides with an original render node: {output}")
            expanded[output] = pieces[0] if len(pieces) == 1 else target
    return expanded


def canonical(name):
    # Blender exports commonly prefix mesh names with the source resource path.
    # This is a format convention, not a hero alias table.
    name = re.split(r"\.vmdl(?:_c)?\.", name, flags=re.I)[-1]
    return re.sub(r"[^a-z0-9]", "", name.casefold())


def lod_base(name):
    return re.sub(r"[_. -]?lod[_. -]?\d+$", "", name, flags=re.I)


def analyze(text):
    doc = Document(text)
    meshes = [{"name": n.get("name"), "filename": n.get("filename"),
               "lower_lod": lod_base(n.get("name", "")) != n.get("name", "")}
              for n in doc.objects("RenderMeshFile")]
    names = [m["name"] for m in meshes]
    if not names or len(set(names)) != len(names) or any(not n for n in names):
        raise BuildError("VMDL must have uniquely named RenderMeshFile nodes.")
    return {"meshes": meshes,
            "ag2": [{"class": n.get("_class"), "filename": n.get("filename"), "name": n.get("name", "")}
                    for n in doc.objects() if n.get("_class") in {"AnimGraph2", "DefaultAnimGraph2", "NmSkeletonReference"}],
            "lod_groups": len(doc.objects("LODGroup")),
            "attachments": len(doc.objects("Attachment")),
            "classes": sorted({n.get("_class") for n in doc.objects() if n.get("_class")})}


def suggest_mappings(model, replacement_names, existing=None, lod_strategy="reuse", auto=True):
    names = {n["name"] for n in model["meshes"]}
    mappings = {k: v for k, v in (existing or {}).items() if k in names and all(t in replacement_names for t in targets(v))}
    # Top-level choices must be resolved before lower LODs, regardless of source order.
    for node in sorted(model["meshes"], key=lambda n: n["lower_lod"]):
        name = node["name"]
        if node["lower_lod"] and lod_strategy == "reuse":
            bases = [n for n in names if canonical(n) == canonical(lod_base(name)) and n in mappings]
            if len(bases) == 1:
                value = mappings[bases[0]]
                mappings[name] = list(value) if isinstance(value, list) else value
                continue
        if name in mappings:
            continue
        if node["lower_lod"] and lod_strategy == "preserve":
            mappings[name] = KEEP
            continue
        if node["lower_lod"] and lod_strategy in {"disable", "single"}:
            mappings[name] = DISABLE
            continue
        if not auto:
            continue
        candidates = [r for r in replacement_names if canonical(r) == canonical(name)]
        if not candidates and lod_strategy == "reuse" and node["lower_lod"]:
            candidates = [r for r in replacement_names if canonical(r) == canonical(lod_base(name))]
        if len(candidates) == 1:
            mappings[name] = candidates[0]
    unresolved = [m["name"] for m in model["meshes"] if m["name"] not in mappings]
    return mappings, unresolved


def isolated_filename(hero_root, source_name):
    slug = re.sub(r"[^a-zA-Z0-9_-]", "_", source_name)[:55] or "mesh"
    tag = hashlib.sha256(source_name.encode()).hexdigest()[:8]
    return relative_asset(f"{hero_root}/dmx/builder_{slug}_{tag}.dmx")


def patch(text, mappings, source_paths):
    doc = Document(text)
    edits = []
    disabled = set()
    nodes = doc.objects("RenderMeshFile")
    expanded_mappings(mappings)  # Validate all lists and generated name collisions first.
    splits = {name: list(expanded_mappings({name: value})) for name, value in mappings.items() if len(targets(value)) > 1}
    array_items = set()
    def arrays(n):
        if n.kind == "array":
            array_items.update(id(x) for x in n.value)
            for x in n.value: arrays(x)
        elif n.kind == "object":
            for x in n.members.values(): arrays(x)
    arrays(doc.root)
    def selection_filter(piece):
        return '{ exclude_by_default = true exception_list = [ ' + json.dumps(piece, ensure_ascii=False) + ' ] }'
    def clone(node, changes, piece=None):
        fragment = Document(text[node.start:node.end])
        changes = [field_edit(fragment.root, k, v) for k, v in changes.items()]
        if piece is not None:
            changes.append(_raw_field(fragment.root, "import_filter", selection_filter(piece)))
        return fragment.apply(changes)
    for node in nodes:
        name = node.get("name")
        target = mappings.get(name)
        if target is None:
            raise BuildError(f"Choose a replacement, Keep Original or Disable for mesh: {name}")
        if target == KEEP:
            continue
        if target == DISABLE:
            disabled.add(name)
            # Leave an empty comma-separated slot out too.
            end = node.end
            after = re.match(r"\s*,", text[end:])
            if after:
                end += after.end()
            edits.append((node.start, end, ""))
        else:
            pieces = targets(target)
            for piece in pieces:
                if piece not in source_paths:
                    raise BuildError(f"Replacement DMX does not contain mapped mesh: {piece}")
            if name in splits:
                if id(node) not in array_items:
                    raise BuildError(f"Cannot expand non-array RenderMeshFile: {name}")
                copies = [clone(node, {"name": out, "filename": relative_asset(source_paths[piece])}, piece)
                          for out, piece in zip(splits[name], pieces)]
                edits.append((node.start, node.end, ",\n".join(copies)))
            else:
                edits.append(field_edit(node, "filename", relative_asset(source_paths[pieces[0]])))
                edits.append(_raw_field(node, "import_filter", selection_filter(pieces[0])))
    for node in doc.objects():
        if any(n.start <= node.start < n.end for n in nodes):
            continue
        mesh_ref = node.get("mesh_name")
        if mesh_ref in splits:
            if id(node) not in array_items:
                raise BuildError(f"Cannot expand a scalar mesh reference to {mesh_ref}; use a supported mesh-reference array.")
            edits.append((node.start, node.end, ",\n".join(clone(node, {"mesh_name": name}) for name in splits[mesh_ref])))
            continue
        for key, value in node.members.items():
            if key == "name" and node.get("_class") in {"BodyGroup", "BodyGroupChoice"}:
                continue  # Group/choice identity is not a reference to a render node.
            if (node.get("_class") == "BodyGroupChoice" and key == "meshes" and value.kind == "array"
                    and any(x.kind == "scalar" and x.value in disabled for x in value.value)):
                # Authored bodygroup choices can list every LOD of a mesh.
                # Keep choice identities/order (including empty choices), but
                # remove explicitly disabled meshes and expand retained pieces
                # together so this array receives only one nonoverlapping edit.
                if any(x.kind != "scalar" for x in value.value):
                    raise BuildError("Unsupported BodyGroupChoice meshes layout; keep the mesh or use a custom source.")
                names = [name for x in value.value if x.value not in disabled
                         for name in splits.get(x.value, [x.value])]
                edits.append((value.start, value.end, json.dumps(names, ensure_ascii=False)))
                continue
            if value.kind == "scalar" and value.value in splits:
                raise BuildError(f"Cannot safely expand unknown {key} reference to {value.value}.")
            if value.kind == "array" and any(x.kind == "scalar" and x.value in splits for x in value.value):
                if key not in {"meshes", "mesh_names", "mesh_references"} or any(x.kind != "scalar" for x in value.value):
                    raise BuildError(f"Cannot safely expand mesh references in {key}.")
                names = [name for x in value.value for name in splits.get(x.value, [x.value])]
                edits.append((value.start, value.end, json.dumps(names, ensure_ascii=False)))
        if node.get("mesh_name") in disabled:
            end = node.end
            after = re.match(r"\s*,", text[end:])
            if after:
                end += after.end()
            edits.append((node.start, end, ""))
    result = doc.apply(edits)
    # Unknown bodygroup reference forms must be resolved explicitly, never silently broken.
    if disabled:
        parsed = Document(result)
        for n in parsed.objects():
            for k, v in n.members.items():
                if v.kind == "scalar" and v.value in disabled and k not in {"name", "_class"}:
                    raise BuildError(f"Disabling {v.value} leaves an unsupported {k} reference; keep it or use a custom source.")
                if v.kind == "array" and any(x.kind == "scalar" and x.value in disabled for x in v.value):
                    raise BuildError(f"Disabled mesh remains referenced in {k}; keep it or use a custom source.")
    return result


def source_references(text):
    refs = set()
    for n in Document(text).objects():
        for key, val in n.members.items():
            v = val.value
            if val.kind == "typed" and v[0] == "resource":
                v = v[1]
            if isinstance(v, str) and "/" in v and re.search(r"\.(dmx|vmdl|vmat|vnmgraph|vnmskel|vnmclip|vphys|vanim|vagrp|animgraph)$", v):
                refs.add(relative_asset(v))
    return sorted(refs)


def _raw_field(node, key, raw):
    old = node.members.get(key)
    return (old.start, old.end, raw) if old else (node.end - 1, node.end - 1, f"\n {key} = {raw}\n")


def bone_hierarchy(document):
    found = {}
    def visit(node, parent=None):
        if node.kind == "object":
            if node.get("_class") == "Bone":
                name = node.get("name")
                if not name or name in found:
                    raise BuildError("Skeleton contains duplicate or unnamed Bone nodes.")
                found[name] = {"node": node, "parent": parent}
                parent = name
            for value in node.members.values():
                visit(value, parent)
        elif node.kind == "array":
            for value in node.value:
                visit(value, parent)
    for skeleton in document.objects("Skeleton"):
        visit(skeleton)
    return found


def template_info(template):
    doc = Document(template)
    filters = {}
    for node in doc.objects("RenderMeshFile"):
        f = node.members.get("import_filter")
        names = f.members.get("exception_list") if f and f.kind == "object" else None
        if not f or f.get("exclude_by_default") != "true" or not names or names.kind != "array" or len(names.value) != 1:
            raise BuildError("Compile template must explicitly include exactly one source mesh per filtered node.")
        target = names.value[0].value
        if not isinstance(target, str) or target in filters:
            raise BuildError("Compile template has ambiguous source mesh filters.")
        filters[target] = node
    if not filters:
        raise BuildError("Compile template contains no filtered RenderMeshFile nodes.")
    bones = bone_hierarchy(doc)
    if not bones or any(item["node"].get("do_not_discard") != "true" for item in bones.values()):
        raise BuildError("Compile template must explicitly retain all required bones.")
    markup = doc.objects("BoneMarkupList")
    if len(markup) != 1 or markup[0].get("bone_cull_type") != "None":
        raise BuildError("Compile template must declare BoneMarkupList bone_cull_type None.")
    return doc, filters, bones


def merge_template(text, template, mappings, source_path, lod_strategy, template_targets=None, original_report=None):
    """Merge only import filters/scales and keep-bone policy. Never replace the hero."""
    tpl, filters, required = template_info(template)
    original = Document(text)
    bones = bone_hierarchy(original)
    from .source_skeleton import canonical_source_hierarchy
    bones = canonical_source_hierarchy(bones, original_report)
    for name, item in required.items():
        if name not in bones:
            raise BuildError(f"Compile template bone/hierarchy does not match original hero: {name} (bone is missing from the source skeleton)")
        if item["parent"] != bones[name]["parent"]:
            raise BuildError(f"Compile template bone/hierarchy does not match original hero: {name} (template parent {item['parent']!r}, source parent {bones[name]['parent']!r})")
    required_source_names = {bones[name]["node"].get("name") for name in required}
    paths = source_path if isinstance(source_path, dict) else {target: source_path for target in selected_meshes(mappings)}
    result = patch(text, mappings, paths)
    # A single explicit LOD removes only later LODGroup levels. LODGroupAll
    # meshes render at every level, so their already-patched references and any
    # other authored children must remain alongside the highest-detail group.
    if lod_strategy == "single":
        d = Document(result)
        lists = d.objects("LODGroupList")
        if len(lists) != 1:
            raise BuildError("Single LOD strategy requires one original LODGroupList.")
        groups = lists[0].members.get("children")
        if not groups or not groups.value or groups.value[0].get("_class") != "LODGroup":
            raise BuildError("Cannot determine the original highest-detail LOD group.")
        first = groups.value[0]
        if float(first.get("switch_threshold", "-1")) != 0:
            raise BuildError("Single LOD requires an original first group with zero switch threshold.")
        retained = [node for node in groups.value if node is first or node.get("_class") != "LODGroup"]
        result = d.apply([(groups.start, groups.end, "[\n" + ",\n".join(
            result[node.start:node.end] for node in retained) + "\n]")])
    d = Document(result)
    edits = []
    expanded = expanded_mappings(mappings)
    origins = {output: name for name, value in mappings.items() for output in expanded_mappings({name: value})}
    for node in d.objects("RenderMeshFile"):
        target = expanded.get(node.get("name"))
        if target in {KEEP, DISABLE}:
            continue
        template_node = filters.get((template_targets or {}).get(target, target))
        if template_node is None:
            # Split pieces inherit the explicit parent-node import scale, never a
            # different mesh's settings. Only the allow-list is changed.
            parent = lod_base(origins[node.get("name")])
            candidates = [n for n in filters.values() if canonical(n.get("name", "")) == canonical(parent)]
            if len(candidates) != 1:
                raise BuildError(f"Compile template has no unambiguous parent settings for mapped mesh: {target}")
            template_node = candidates[0]
        edits.append(_raw_field(node, "import_filter", '{ exclude_by_default = true exception_list = [ ' + json.dumps(target, ensure_ascii=False) + ' ] }'))
        for key in ("import_scale",):
            value = template_node.members.get(key)
            if value:
                edits.append(_raw_field(node, key, template[value.start:value.end]))
    for name, item in bone_hierarchy(d).items():
        if name in required_source_names and item["node"].get("do_not_discard") != "true":
            edits.append(_raw_field(item["node"], "do_not_discard", "true"))
    markup = d.objects("BoneMarkupList")
    if len(markup) != 1:
        raise BuildError("Original hero requires a single BoneMarkupList for safe template merging.")
    if markup[0].get("bone_cull_type") != "None":
        edits.append(field_edit(markup[0], "bone_cull_type", "None"))
    return d.apply(edits)
