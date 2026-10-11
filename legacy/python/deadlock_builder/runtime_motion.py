"""Read-only runtime cloth checks using Source2Viewer's decoded PHYS and DATA.

This module does not write compiled resources or infer simulation from weights.
Flag meanings: ValveResourceFormat 20.0, ModelSkeletonBoneFlags.cs, commit
 a06886f7d06049052d32a7381ec05523064a2ca0 (Cloth=0x8, Procedural=0x400000).
Unknown/changed solver layouts fail closed for heroes requiring original cloth.
"""
from __future__ import annotations
import hashlib
import json
import math
import re
from .bone_validation import BinaryBlob, skeleton_identity
from .safety import BuildError

CLOTH = 0x8
PROCEDURAL = 0x400000
MOTION_MASK = CLOTH | PROCEDURAL
AUDIT_VERSION = 2


def unavailable(reason):
    return {"status": "unavailable", "reason": reason, "in_game_motion_tested": False}


def _solver_fingerprint(solver):
    """Preserve binary types without changing existing blob-free fingerprints.

    A plain ``{"$binary": "..."}`` object is not a binary literal. If any
    actual blob occurs, tag every node and use a separate hash domain so an
    authored object/array cannot imitate the blob encoding. The marker emitted
    by the text reader and the native reader's bytes share the same encoding.
    """
    has_blob = False

    def typed(value):
        nonlocal has_blob
        if isinstance(value, (bytes, BinaryBlob)):
            has_blob = True
            if isinstance(value, bytes):
                data = value.hex()
            else:
                data = value.get("$binary")
                if (set(value) != {"$binary"} or not isinstance(data, str) or
                        re.fullmatch(r"(?:[0-9a-f]{2})*", data) is None):
                    raise ValueError("invalid binary literal evidence")
            return ["blob", data]
        if isinstance(value, dict):
            return ["object", [[key, typed(item)] for key, item in sorted(value.items())]]
        if isinstance(value, list):
            return ["array", [typed(item) for item in value]]
        if value is None:
            return ["null"]
        if isinstance(value, bool):
            return ["bool", value]
        if isinstance(value, int):
            return ["integer", value]
        if isinstance(value, float):
            return ["number", value]
        if isinstance(value, str):
            return ["string", value]
        raise TypeError("unsupported solver value: " + type(value).__name__)

    encoded = typed(solver)
    payload = json.dumps(encoded if has_blob else solver, sort_keys=True,
                         separators=(',', ':'), allow_nan=False).encode()
    if has_blob:
        payload = b"deadlock-builder:solver-binary-v1\0" + payload
    return hashlib.sha256(payload).hexdigest()


def _integer(value):
    # The read-only textual KV3 parser preserves numeric tokens as strings.
    if type(value) is int:
        return value
    if isinstance(value, str) and re.fullmatch(r"[+-]?(?:[0-9]+|0[xX][0-9a-fA-F]+)", value):
        return int(value, 16 if "x" in value.lower() else 10)
    raise ValueError("invalid integer encoding")


def _finite(value):
    if isinstance(value, bool) or not isinstance(value, (int, float, str)):
        raise ValueError("invalid numeric encoding")
    result = float(value)
    if not math.isfinite(result):
        raise ValueError("nonfinite numeric value")
    return result


def _control_bindings(controls, names, solver):
    """Resolve FE controls through authored offsets, without inventing bones.

    Real original models use offset-derived proxy controls that are absent from
    DATA.m_modelSkeleton. Source 2's FeCtrlOffset_t/FeCtrlOsOffset_t/
    FeCtrlSoftOffset_t address the FE control table, not the model bone table.
    Soft offsets can blend several parent controls. Every dependency branch
    must reach a model bone; naming patterns prove nothing.
    """
    direct = set(controls) & set(names)
    parents = {}
    for key in ("m_CtrlOffsets", "m_CtrlOsOffsets", "m_CtrlSoftOffsets"):
        relations = solver.get(key, [])
        if not isinstance(relations, list):
            raise ValueError(key + " must be a control-offset list")
        for relation in relations:
            if not isinstance(relation, dict):
                raise ValueError(key + " contains an invalid relation")
            parent, child = _integer(relation.get("nCtrlParent")), _integer(relation.get("nCtrlChild"))
            if any(not 0 <= i < len(controls) for i in (parent, child)):
                raise ValueError(key + " has an out-of-range control index")
            if key != "m_CtrlOsOffsets":
                offset = relation.get("vOffset")
                if not isinstance(offset, list) or len(offset) != 3:
                    raise ValueError(key + " has an invalid three-component offset")
                for v in offset:
                    _finite(v)
            if key == "m_CtrlSoftOffsets":
                _finite(relation.get("flAlpha"))
            parents.setdefault(child, []).append((parent, key))
    # Check all offset edges, even for directly bound children. The iterative
    # walk also avoids recursion limits on large cloth-control tables.
    visited = set()
    for start in parents:
        stack, active = [(start, False)], set()
        while stack:
            at, leaving = stack.pop()
            if leaving:
                active.remove(at)
                visited.add(at)
            elif at in active:
                raise ValueError("cycle in authored control offsets: " + controls[at])
            elif at not in visited:
                active.add(at)
                stack.append((at, True))
                stack.extend((parent, False) for parent, _ in parents.get(at, []))
    derived, missing = [], []
    for i, name in enumerate(controls):
        if name in direct:
            continue
        pending, seen, anchors, relations, unbound = [i], set(), set(), [], False
        while pending:
            at = pending.pop()
            if at in seen:
                continue
            seen.add(at)
            if controls[at] in direct:
                anchors.add(controls[at])
            elif at not in parents:
                unbound = True
            else:
                for parent, relation in parents[at]:
                    relations.append({"control": controls[at], "parent": controls[parent], "relation": relation})
                    pending.append(parent)
        if unbound or not anchors:
            missing.append(name)
        else:
            derived.append({"control": name, "anchors": sorted(anchors), "offset_relations": relations})
    if missing:
        raise ValueError("solver controls not present in model skeleton and not derived from a bound control: " +
                         ", ".join(repr(name) for name in sorted(missing)) +
                         f" ({len(missing)} of {len(controls)} controls; {len(names)} skeleton bones). "
                         "This control layout needs a compatibility audit against the original hero; "
                         "it does not by itself establish that the game model is invalid")
    return {"skeleton_controls": sorted(direct), "derived_controls": derived}


def summarize(physics, data=None):
    """Summarize the entire solver, not just its node count. Never mutate inputs."""
    if not isinstance(physics, dict):
        raise BuildError("Runtime cloth audit: PHYS is not a readable object.")
    solver = physics.get("m_pFeModel")
    if solver is None or solver == "null":
        result = {"status": "audited", "cloth_nodes": 0, "static_nodes": 0, "ropes": 0,
                  "solver_sha256": None, "controls": [], "in_game_motion_tested": False}
        return _with_skeleton(result, data) if data is not None else result
    if not isinstance(solver, dict):
        raise BuildError("Runtime cloth audit: unsupported m_pFeModel layout.")
    try:
        count = int(solver["m_nNodeCount"])
        static = int(solver["m_nStaticNodes"])
        ropes = int(solver["m_nRopeCount"])
        controls = solver["m_CtrlName"]
        if (count < 0 or not 0 <= static <= count or ropes < 0 or
                not isinstance(controls, list) or len(set(controls)) != len(controls) or
                any(not isinstance(n, str) or not n for n in controls) or (count and not controls)):
            raise ValueError("invalid solver counts/controls")
        if "m_CtrlHash" in solver:
            hashes = solver["m_CtrlHash"]
            if (not isinstance(hashes, list) or len(hashes) != len(controls) or
                    any(not 0 <= _integer(v) <= 0xffffffff for v in hashes)):
                raise ValueError("incomplete/invalid control hash table")
    except (KeyError, TypeError, ValueError) as ex:
        raise BuildError(f"Runtime cloth audit: unsupported/incomplete solver metadata: {ex}") from ex
    try:
        solver_hash = _solver_fingerprint(solver)
    except (TypeError, ValueError) as ex:
        raise BuildError(f"Runtime cloth audit: unsupported solver payload: {ex}") from ex
    result = {"status": "audited", "cloth_nodes": count, "static_nodes": static, "ropes": ropes,
              "controls": controls, "solver_sha256": solver_hash, "in_game_motion_tested": False}
    if count or data is not None:
        return _with_skeleton(result, data, solver)
    return result


def _with_skeleton(result, data, solver=None):
    try:
        skeleton = data["m_modelSkeleton"]
        names, flags = skeleton["m_boneName"], skeleton["m_nFlag"]
        if len(names) != len(set(names)) or len(flags) != len(names):
            raise ValueError("duplicate bones or incomplete flags")
        result["control_bindings"] = _control_bindings(result["controls"], names, solver or {})
        result["skeleton"] = skeleton
        result["bone_motion_flags"] = {name: int(flag) & MOTION_MASK for name, flag in zip(names, flags)}
        # Audit the source itself before using it as a compatibility reference.
        skeleton_identity(skeleton, skeleton)
    except (KeyError, TypeError, ValueError, IndexError) as ex:
        raise BuildError(f"Runtime cloth audit: incomplete skeleton identity/flag data: {ex}") from ex
    return result


def comparison(original, rebuilt):
    result = {"audit_version": AUDIT_VERSION, "original": original, "rebuilt": rebuilt,
              "passed": False, "errors": [], "in_game_motion_tested": False,
              "compiled_resources_modified": False}
    errors = result["errors"]
    if original.get("status") != "audited":
        errors.append("Original runtime simulation could not be audited: " + original.get("reason", "unknown"))
    elif not original["cloth_nodes"]:
        result.update(passed=True, required=False)
        return result
    elif rebuilt.get("status") != "audited":
        errors.append("Rebuilt runtime simulation could not be audited: " + rebuilt.get("reason", "unknown"))
    else:
        result["required"] = True
        for key in ("cloth_nodes", "static_nodes", "ropes"):
            if rebuilt.get(key) != original[key]:
                errors.append(f"{key}: original {original[key]}, rebuilt {rebuilt.get(key, 'unknown')}")
        if rebuilt.get("solver_sha256") != original["solver_sha256"]:
            errors.append("Original cloth solver payload is missing or changed (all decoded solver fields compared).")
        if rebuilt.get("control_bindings") != original.get("control_bindings"):
            errors.append("Original skeleton/derived-control bindings are missing or changed.")
        if rebuilt.get("skeleton"):
            try:
                result["skeleton_identity"] = skeleton_identity(rebuilt["skeleton"], original["skeleton"])
            except (BuildError, KeyError, ValueError, TypeError, IndexError) as ex:
                errors.append("Skeleton compatibility failed: " + str(ex))
            differences = [name for name, flags in original["bone_motion_flags"].items()
                           if rebuilt.get("bone_motion_flags", {}).get(name) != flags]
            result["changed_motion_flag_bones"] = differences
            if differences:
                errors.append("Cloth/procedural bone flags differ by bone name: " + ", ".join(differences))
    result["passed"] = not errors
    return result


def warning(audit):
    if audit.get("status") != "audited":
        return "Runtime cloth preservation is unverified: " + audit.get("reason", "original model unavailable")
    if audit["cloth_nodes"]:
        return (f"Original runtime cloth: {audit['cloth_nodes']} nodes, {audit['static_nodes']} static, "
                f"{audit['ropes']} ropes. Bone weights and AG2 do not supply this solver. "
                "Rebuild checks PHYS and cloth/procedural bone flags before packaging/install; loss blocks publication. "
                "Build/Rebuild automatically restores compatible original cloth after compilation, with name/parent/bind checks. "
                "Imported scripts are never executed. In-game motion still requires testing. "
                "See docs/HAIR_MOTION_INVESTIGATION.md.")
    return None
