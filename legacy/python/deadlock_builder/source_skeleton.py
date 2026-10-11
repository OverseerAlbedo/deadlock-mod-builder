"""Resolve exporter spellings for source comparison, never runtime identities.

Source2Viewer exports a procedural cloth bone beginning with ``$`` using ``_``
instead (ModelExtract.GetExportBoneName). The compiled original remains the
authority: names alone cannot authorize this conversion. No source node, DMX
joint, transform, skinning palette or compiled resource is renamed here.
"""
from __future__ import annotations

import re

from .safety import BuildError


def canonical_source_hierarchy(bones, original_report=None):
    """Return an identity view retaining each original source node object.

    Only an audited skeleton tied to this original resource may authorize an
    export alias. Unavailable evidence retains the existing exact comparison.
    Parent names use the same mapping, including independent cloth roots.
    """
    original = original_report or {}
    audit = original.get("runtime_motion", {})
    if audit.get("status") != "audited":
        return bones
    skeleton = audit.get("skeleton")
    if skeleton is None and not audit.get("cloth_nodes"):
        # Originals without a PHYS block have no skeleton in the motion audit.
        return bones
    sha = original.get("compiled_sha256")
    if (not isinstance(sha, str) or re.fullmatch(r"[0-9a-f]{64}", sha) is None or
            audit.get("resource_sha256") != sha):
        raise BuildError("Source skeleton export aliases require an audit of the same original compiled model.")
    # Lazy imports avoid the model -> bone_validation -> model import cycle.
    from .bone_validation import skeleton_identity
    from .runtime_motion import CLOTH, PROCEDURAL, _integer

    try:
        names, flags = skeleton["m_boneName"], skeleton["m_nFlag"]
        if (not isinstance(names, list) or not names or
                any(not isinstance(n, str) or not n for n in names) or
                len(set(names)) != len(names) or not isinstance(flags, list) or
                len(flags) != len(names)):
            raise ValueError("incomplete or duplicate bone identity/flag data")
        flags = [_integer(flag) for flag in flags]
        if any(not 0 <= flag <= 0xffffffff for flag in flags):
            raise ValueError("invalid bone flags")
        skeleton_identity(skeleton, skeleton)
    except (KeyError, TypeError, ValueError, IndexError) as ex:
        raise BuildError(f"Source skeleton export alias audit is incomplete: {ex}") from ex
    identities = set(names)
    mask = CLOTH | PROCEDURAL
    aliases = {"_" + name[1:]: name for name, flag in zip(names, flags)
               if name.startswith("$") and flag & mask == mask and
               "_" + name[1:] not in identities}
    resolved = {}
    for name, item in bones.items():
        canonical = aliases.get(name, name)
        if canonical in resolved:
            raise BuildError(f"Ambiguous source skeleton export aliases for bone: {canonical}")
        resolved[canonical] = {**item, "parent": aliases.get(item["parent"], item["parent"])}
    return resolved
