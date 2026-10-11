"""Repeatable corrected-DMX checks, separate from original/compiler validation.

These numeric checks do not establish animation quality or bind-pose correctness.
They intentionally produce a separate receipt; they never grant build acceptance.
"""
from __future__ import annotations
import math
from pathlib import Path
from . import model
from .dmx import Dmx
from .safety import digest, write_json


def skinning(dmx: Dmx):
    summary = dmx.summary()
    palettes = {tuple(e.get("jointList", [])) for e in dmx.elements if e.type == "DmeModel" and e.get("jointList")}
    palette = next(iter(palettes)) if len(palettes) == 1 else None
    meshes = []
    for mesh in summary["meshes"]:
        element = dmx.elements[mesh["element"]]
        ref = element.get("bindState", element.get("currentState", -1))
        state = dmx.elements[ref] if isinstance(ref, int) and ref >= 0 else None
        errors, pending = [], []
        weights = state.get("blendweights$0", state.get("jointWeights", [])) if state else []
        indices = state.get("blendindices$0", state.get("jointIndices", [])) if state else []
        slots = state.get("jointCount", 0) if state else 0
        if not weights or not indices:
            errors.append("Skin weights/joint indices are missing")
        elif not isinstance(slots, int) or not 1 <= slots <= 64:
            errors.append("Invalid or unsupported influences-per-vertex jointCount")
        else:
            if len(weights) != mesh["vertices"] * slots or len(indices) != len(weights):
                errors.append("Weight/index counts do not match vertices × jointCount; inspect exporter/indexed layout")
            invalid = [w for w in weights if not isinstance(w, (int, float)) or not math.isfinite(w) or w < 0]
            if invalid:
                errors.append("Weights contain negative or non-finite values")
            if not invalid and len(weights) == mesh["vertices"] * slots:
                bad_sums = sum(abs(sum(weights[i:i + slots]) - 1) > 0.02 for i in range(0, len(weights), slots))
                if bad_sums:
                    errors.append(f"{bad_sums} vertices have weights that do not sum to 1 within 0.02")
            if palette is None:
                pending.append("No unique DmeModel joint palette; index-to-bone validation needs review")
            else:
                bad_indices = sum(not isinstance(i, int) or i < 0 or i >= len(palette) for i, w in zip(indices, weights) if isinstance(w, (int, float)) and w > 0)
                if bad_indices:
                    errors.append(f"{bad_indices} positive-weight influences point outside the joint palette")
                else:
                    nonbones = sum(palette[i] < 0 or palette[i] >= len(dmx.elements) or dmx.elements[palette[i]].type != "DmeJoint" for i, w in zip(indices, weights) if w > 0)
                    if nonbones:
                        errors.append(f"{nonbones} positive-weight influences reference non-bone palette entries")
        meshes.append({"name": mesh["name"], "vertices": mesh["vertices"], "joint_count": slots,
                       "errors": errors, "pending": pending, "passed": not errors and not pending})
    return {"meshes": meshes, "joint_palette_entries": len(palette) if palette else None,
            "numeric_checks_passed": all(m["passed"] for m in meshes)}


def corrected_dmx_receipt(pipeline, project, output: Path):
    """Read-only regression check; do not save acceptance flags or change sources."""
    report = pipeline.inspect(project)
    dmx = Dmx.load(Path(project.dmx))
    skin = skinning(dmx)
    selected = model.selected_meshes(report["mappings"])
    selected_checks = [m for m in skin["meshes"] if m["name"] in selected]
    splits = []
    for name in sorted(selected) if not project.mesh_template else []:
        split = Dmx(dmx.single_mesh(name))
        split_skin = skinning(split)
        source_mesh = next(m for m in skin["meshes"] if m["name"] == name)
        if split_skin["meshes"] != [source_mesh] or split.summary()["bones"] != report["dmx"]["bones"]:
            from .safety import BuildError
            raise BuildError("Isolating the corrected DMX changed its skinning metadata.")
        splits.append({"mesh": name, "skin_metadata_unchanged": True, "skeleton_names_unchanged": True})
    passed = bool(selected_checks) and all(m["passed"] for m in selected_checks) and not report["unresolved_meshes"]
    receipt = {"dmx_sha256": digest(Path(project.dmx)), "dmx": project.dmx,
               "numeric_and_split_checks_passed": passed, "skinning": skin, "isolated_mesh_checks": splits,
               "mesh_mappings": report["mappings"], "unresolved_meshes": report["unresolved_meshes"],
               "filtered_source_used_byte_identically": bool(project.mesh_template),
               "geometry": report.get("geometry"), "source_materials": report.get("source_materials"),
               "source_bones": report.get("source_bones"),
               "material_review_needed": [name for name, m in report["materials"].items() if m["ambiguous"] or not m["confirmed"]],
               "deferred": ["Valve import and compile of the corrected DMX",
                            "Skeleton bind pose, bone palette and deformation in the compiled model",
                            "Idle, locomotion, combat/weapon and ability animations in Deadlock",
                            "UI graph animation, effects, attachments, hitboxes and cloth behavior",
                            "All selected LODs, material appearance and rendering cost"],
               "gameplay_validated": False, "compiled": False, "installed": False}
    write_json(output, receipt)
    return receipt
