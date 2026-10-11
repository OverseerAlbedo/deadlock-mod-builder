"""Read-only bone identity audits. Compiler-owned index arrays are never copied.

Remap slices follow VRF ModelData/BoneRemapTable.cs; embedded mesh indices and
block indices follow Model.GetEmbeddedMeshes/ParseEmbeddedMesh2.
"""
from __future__ import annotations
import json
import math
import re
from pathlib import Path
from .kv3 import Document
from .model import template_info
from .safety import BuildError


class BinaryBlob(dict):
    """JSON-safe evidence that retains its KV3 binary type during an audit.

    An ordinary authored object may also contain a ``$binary`` member. The
    distinct in-memory type prevents that object from impersonating a blob in
    a solver fingerprint; JSON reports still display the compact hex payload.
    """
    def __init__(self, value: bytes):
        super().__init__({"$binary": value.hex()})


def plain(node):
    if node.kind == "blob":
        # Preserve binary identity in JSON reports without confusing it with
        # a scalar string or a numeric array. Hex casing/spacing is immaterial.
        return BinaryBlob(node.value)
    if node.kind == "array":
        return [plain(v) for v in node.value]
    if node.kind == "object":
        return {k: plain(v) for k, v in node.members.items()}
    if node.kind == "typed":
        return node.value[1]
    return node.value


def dumps(text):
    """Parse each CLI KV3 block, excluding the CLI diagnostic preamble."""
    chunks = re.split(r'--- Data for block "[^"\n]+" ---', text)
    result = []
    for chunk in chunks[1:] if len(chunks) > 1 else chunks:
        start, end = chunk.find("{"), chunk.rfind("}")
        header = chunk.find("-->")
        if header >= 0:
            start = chunk.find("{", header + 3)
        if start >= 0 and end > start:
            result.append(plain(Document(chunk[start:end + 1]).root))
    if not result:
        raise BuildError("Source2Viewer returned no readable KV3 block for bone validation.")
    return result


def source_bones(dmx, template, reference_file="", original_sha=None):
    _, _, required = template_info(template)
    bind_audit = None
    joints = {i: e for i, e in enumerate(dmx.elements) if e.type == "DmeJoint"}
    names = [e.name for e in joints.values()]
    if len({e.uuid for e in joints.values()}) != len(joints):
        raise BuildError("Replacement DMX has duplicate bone element identities.")
    if len(names) != len(set(names)) or set(names) != set(required):
        raise BuildError("Replacement DMX bones differ from required template bone identities.")
    palettes = {tuple(e.get("jointList", [])) for e in dmx.elements if e.type == "DmeModel" and e.get("jointList")}
    if len(palettes) != 1:
        raise BuildError("Replacement DMX must have one unambiguous joint palette.")
    palette = next(iter(palettes))
    joint_indices = [i for i in palette if i in joints]
    if len(joint_indices) != len(joints) or len(set(joint_indices)) != len(joints):
        raise BuildError("Required DMX joints are missing or duplicated in the skinning palette.")
    parents = {}
    for i, e in joints.items():
        for child in e.get("children", []):
            if child in joints:
                if joints[child].name in parents:
                    raise BuildError("DMX joint has more than one parent.")
                parents[joints[child].name] = e.name
    if any(parents.get(name) != item["parent"] for name, item in required.items()):
        raise BuildError("Replacement DMX hierarchy differs from the keep-bone template.")
    if reference_file:
        ref = json.loads(Path(reference_file).read_text(encoding="utf-8"))
        if original_sha and ref.get("sha256") != original_sha:
            raise BuildError("Skeleton reference belongs to a different original compiled model.")
        skeleton = ref["skeleton"]
        ref_names = skeleton["m_boneName"]
        if set(ref_names) != set(names):
            raise BuildError("Replacement bone names differ from the original reference.")
        for i, name in enumerate(ref_names):
            parent = int(skeleton["m_nParent"][i])
            if parents.get(name) != (ref_names[parent] if parent >= 0 else None):
                raise BuildError(f"Replacement bone parent differs from original: {name}")
        if "scale_policy" in ref:
            from .preparation.target_skeleton import SCALE_POLICY
            from .preparation.dmx_bind import audit_dmx_bind
            if ref["scale_policy"] != SCALE_POLICY:
                raise BuildError("Unknown prepared skeleton scale policy; regenerate preparation.")
            # Prepared references opt into the strict current/bind transform
            # audit at every pipeline source check, including after splitting.
            # Unmarked manual DMX references retain their existing validation.
            bind_audit = audit_dmx_bind(dmx, skeleton, require_explicit_scale=True)
    from .validation import skinning
    numeric = skinning(dmx)
    if not numeric["numeric_checks_passed"]:
        raise BuildError("Replacement skinning audit failed: " + json.dumps(numeric["meshes"]))
    return {**({"target_bind_audit": bind_audit} if bind_audit is not None else {}), "bone_count": len(names), "joint_palette_entries": len(palette),
            "joint_palette_names": [joints[i].name if i in joints else None for i in palette],
            "required_names_and_parents_match": True, "skinning": numeric}


def skeleton_identity(actual, reference):
    names, original = actual["m_boneName"], reference["m_boneName"]
    if len(names) != len(set(names)) or set(names) != set(original):
        from .skeleton_diagnostics import identity_difference, format_difference
        difference = identity_difference(names, original)
        raise BuildError("Compiled skeleton lost, duplicated or added required bone identities. "
                         + format_difference(difference))
    mapping = {name: i for i, name in enumerate(names)}
    errors = []
    max_translation = max_rotation = 0.0
    def parent_name(s, i):
        parent = int(s["m_nParent"][i])
        if parent < -1 or parent >= len(s["m_boneName"]):
            raise BuildError("Compiled skeleton has an out-of-range parent index.")
        return s["m_boneName"][parent] if parent >= 0 else None
    for i, name in enumerate(original):
        j = mapping[name]
        if parent_name(actual, j) != parent_name(reference, i):
            errors.append(name + ": parent identity changed")
        a = list(map(float, actual["m_bonePosParent"][j]))
        b = list(map(float, reference["m_bonePosParent"][i]))
        distance = math.dist(a, b)
        # Source 2 model skeleton translations are in inches.
        max_translation = max(max_translation, distance * 0.0254)
        qa = list(map(float, actual["m_boneRotParent"][j]))
        qb = list(map(float, reference["m_boneRotParent"][i]))
        norm_a, norm_b = math.sqrt(sum(x*x for x in qa)), math.sqrt(sum(x*x for x in qb))
        if not all(math.isfinite(v) for v in a+b+qa+qb) or min(norm_a, norm_b) < 0.99:
            raise BuildError(f"Invalid compiled bind transform: {name}")
        angle = math.degrees(2 * math.acos(min(1, abs(sum(x*y for x,y in zip(qa,qb))) / (norm_a * norm_b))))
        max_rotation = max(max_rotation, angle)
        scale = float(actual["m_boneScaleParent"][j])
        if distance > 0.01 or angle > 0.1 or not math.isfinite(scale) or abs(scale-float(reference["m_boneScaleParent"][i])) > 1e-4:
            errors.append(name + ": bind transform changed")
    if errors:
        raise BuildError("Compiled skeleton identity validation failed: " + "; ".join(errors[:10]))
    return {"bone_count": len(names), "original_to_compiled": [mapping[n] for n in original],
            "reordered_bones": sum(i != mapping[n] for i,n in enumerate(original)),
            "name_and_parent_identity_passed": True, "max_translation_error_m": max_translation,
            "max_rotation_error_degrees": max_rotation}


def compiled(data, ctrl, mdats, blocks, reference):
    skeleton = data["m_modelSkeleton"]
    result = skeleton_identity(skeleton, reference)
    names = skeleton["m_boneName"]
    starts = list(map(int, data.get("m_remappingTableStarts", [])))
    table = list(map(int, data.get("m_remappingTable", [])))
    if not starts or starts[0] != 0 or starts != sorted(starts) or any(i < 0 or i > len(table) for i in starts):
        raise BuildError("Compiled mesh remapping table has invalid slice boundaries.")
    if any(i < 0 or i >= len(names) for i in table):
        raise BuildError("Compiled mesh remap points outside the model skeleton.")
    indices = [i for i,b in enumerate(blocks) if b["name"] == "MDAT"]
    if len(indices) != len(mdats):
        raise BuildError("VRF did not return every embedded mesh DATA block.")
    by_block = dict(zip(indices, mdats))
    meshes = []
    entries = ctrl.get("embedded_meshes", [])
    if not entries:
        raise BuildError("Compiled-output identity audit requires embedded mesh metadata.")
    for item in entries:
        index = int(item.get("m_nMeshIndex", item.get("mesh_index", -1)))
        block = int(item.get("m_nDataBlock", item.get("data_block", -1)))
        if index < 0 or index >= len(starts) or block not in by_block:
            raise BuildError("Compiled CTRL mesh points to a missing skeleton slice or mesh block.")
        mesh = by_block[block]
        bones = mesh.get("m_skeleton", {}).get("m_bones", [])
        remap = table[starts[index]:starts[index+1] if index+1 < len(starts) else len(table)]
        if len(remap) < len(bones) or not bones:
            raise BuildError("Compiled mesh remap slice is shorter than its bone list.")
        local_names = {b["m_boneName"] for b in bones}
        for bone, model_index in zip(bones, remap):
            if bone["m_boneName"] != names[model_index]:
                raise BuildError("Compiled mesh remap bone identity mismatch: " + bone["m_boneName"])
            parent = int(skeleton["m_nParent"][model_index])
            visited = set()
            while parent >= 0 and names[parent] not in local_names:
                if parent in visited:
                    raise BuildError("Cycle in compiled model bone hierarchy.")
                visited.add(parent)
                parent = int(skeleton["m_nParent"][parent])
            if bone.get("m_parentName", "") != (names[parent] if parent >= 0 else ""):
                raise BuildError("Compiled mesh remap parent identity mismatch: " + bone["m_boneName"])
        meshes.append({"name": item.get("m_Name", item.get("name")), "mesh_index": index,
                       "remapped_bones": len(bones), "extra_remap_entries_bounds_checked": len(remap)-len(bones), "identities_match": True})
    result.update({"meshes": meshes, "compiler_arrays_modified": False,
                   "vertex_blendindices_decoded": False, "live_ag2_deformation_validated": False})
    return result
