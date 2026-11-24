"""Executed ONLY by external Blender 4.2-5.3 (imported by worker.py at Convert).

Writes what making a clip from an animation needs from this preparation
(`animation-fit.json` beside the prepared model), so a clip is retargeted
exactly the way the model was fitted at rest without running the preparation
again: per mapped bone the rest map after Feet on the floor (unweighted bones
too), the source rest matrices and the unit scale, the body alignment, roles,
mapping, following bones, the arm-deformation kinds and the hero's bones.
Every number is a float64 (json writes the shortest repr that round-trips).

Nothing here changes a preparation output: the worker calls `extra_maps`
before Feet on the floor and `write` after it, and both are guarded there,
so a failure leaves no fit file and the preparation as it was.
"""
import json
import math
from pathlib import Path

import numpy as np

# Must equal Rust `clip_making::FIT_POLICY`.
POLICY = 'animation-fit-v1'
FILE = 'animation-fit.json'


def _m(matrix):
    return [[float(x) for x in row] for row in np.asarray(matrix, dtype=float)]


def _rotation(matrix):
    """The nearest rotation (polar decomposition) of a 3x3 or the 3x3 of a 4x4."""
    m = np.asarray(matrix, dtype=float)[:3, :3]
    u, _, vh = np.linalg.svd(m)
    r = u @ vh
    if np.linalg.det(r) < 0:
        u[:, -1] *= -1
        r = u @ vh
    return r


def extra_maps(rest_maps, source, target_matrices, mapping, weighted_maps, source_roles, target_roles, parents,
               following, **fit):
    """Rest maps of mapped bones that carry no weights (preparation computes
    weighted ones only), from the same fit over the whole mapping. Weighted
    maps are never taken from here. An error leaves them out."""
    full = {n: d for n, d in mapping.items() if n in source and d in target_matrices}
    if not set(full) - set(weighted_maps):
        return {}
    try:
        maps, _ = rest_maps(source, target_matrices, full, source_roles, target_roles, parents, following, mapping, **fit)
    except Exception:
        return {}
    return {n: np.asarray(m, dtype=float) for n, m in maps.items() if n not in weighted_maps}


def body_alignment(audit, source, target_matrices, source_roles, target_roles, hips):
    """(rotation, even scale) of the body alignment: Preserve source
    proportions records the matrix it used; Match hero proportions has none,
    so it is built from the same landmark frames (body_basis), scale 1."""
    recorded = audit.get('body_alignment')
    if recorded is not None:
        m = np.asarray(recorded, dtype=float)[:3, :3]
        return _rotation(m), float(np.linalg.norm(m[:, 0]))
    from retarget import body_basis
    sp = {n: np.asarray(m, dtype=float)[:3, 3] for n, m in source.items()}
    tp = {n: np.asarray(m, dtype=float)[:3, 3] for n, m in target_matrices.items()}
    return body_basis(tp, target_roles, hips=hips) @ body_basis(sp, source_roles, hips=hips).T, 1.


def write(job, rig, source, parents, source_roles, target_roles, mapping, following, maps, extra, weighted,
          unit_scale, audit, target):
    """`animation-fit.json` in the job's workspace (rigged, non-correction
    preparations only; the caller decides)."""
    options = job.get('options', {})
    hips = (options.get('fit_leg_joints') or 'pelvis') != 'pelvis'
    target_matrices = {b['name']: np.asarray(b['matrix'], dtype=float) for b in target}
    rotation, scale = body_alignment(audit, source, target_matrices, source_roles, target_roles, hips)
    every = dict(extra)
    every.update(maps)
    fit = {
        'policy': POLICY,
        'rig': rig.name,
        'rotation_degrees': [float(v) for v in (job.get('rotation') or [0., 0., 0.])],
        'unit_scale': float(unit_scale),
        'fit_mode': options.get('fit_mode', 'match_target'),
        'fit_floor': options.get('fit_floor', 'ankles'),
        'fit_leg_joints': options.get('fit_leg_joints', 'pelvis'),
        'source': {n: _m(m) for n, m in source.items()},
        'parents': dict(parents),
        'weighted': sorted(weighted),
        'source_roles': dict(source_roles),
        'target_roles': dict(target_roles),
        'mapping': dict(mapping),
        'following': dict(following),
        'maps': {n: _m(m) for n, m in every.items()},
        'weighted_maps': sorted(maps),
        'body_rotation': _m(rotation),
        'body_scale': float(scale),
        'arm_deformation': audit.get('arm_deformation') or {},
        'target': [{'name': b['name'], 'parent': b.get('parent'), 'matrix': _m(b['matrix'])} for b in target],
    }
    if not all(math.isfinite(x) for m in fit['maps'].values() for row in m for x in row):
        raise ValueError('non-finite rest map')
    Path(job['workspace'], FILE).write_text(json.dumps(fit, allow_nan=False), encoding='utf-8')
