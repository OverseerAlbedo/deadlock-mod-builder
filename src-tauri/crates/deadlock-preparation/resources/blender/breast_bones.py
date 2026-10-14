"""Add breast bones where a character's chest has breasts but its rig has none.

Runs only on the temporary imported scene (inspection and conversion repeat it
with the same request, so both see the same bones); the user's file is never
changed. The breast shapes come from the skin: on each side of the chest, the
most forward point between the chest and the neck is compared with the front of
the ribcage below it and with the middle of the chest. A flat chest gets no
bones. Skin weight near each breast moves from the torso bones to the new bone
with a smooth falloff, keeping every vertex's total weight unchanged.
"""
import math
import bpy
import numpy as np
from mathutils import Vector

POLICY = 'generated-breast-bones-v1'
NAMES = ('DMB_Breast_L', 'DMB_Breast_R')
# Smallest breast protrusion, as a share of the pelvis-to-head length (about
# 2 cm on an adult). Male and flat chests stay below it.
MIN_PROTRUSION = 0.035


def _receipt(status, reason=None, **values):
    receipt = {'policy': POLICY, 'status': status}
    if reason:
        receipt['reason'] = reason
    receipt.update(values)
    return receipt


def _torso_rows(rig, meshes, torso, low, high):
    """Vertices skinned mostly to torso bones between the two heights."""
    bones = rig.data.bones
    rows = []
    for obj in meshes:
        names = {g.index: g.name for g in obj.vertex_groups}
        world = obj.matrix_world
        for vertex in obj.data.vertices:
            total = part = 0.
            for g in vertex.groups:
                name = names.get(g.group)
                if name is None or name not in bones or not bones[name].use_deform or g.weight <= 0:
                    continue
                total += g.weight
                if name in torso:
                    part += g.weight
            if total <= 0 or part / total < 0.5:
                continue
            co = world @ vertex.co
            if low <= co.z <= high:
                rows.append((obj, vertex.index, (co.x, co.y, co.z)))
    return rows


def find_breasts(rig, meshes, request):
    """Breast centers and size from the skin, or a receipt explaining why none."""
    bones = rig.data.bones
    for key in ('chest', 'pelvis', 'head', 'high'):
        if request.get(key) not in bones:
            return None, _receipt('unavailable', 'The torso bones needed to place breast bones were not found.')
    world = rig.matrix_world
    pos = lambda name: np.array(world @ bones[name].head_local)
    pelvis, head, chest = pos(request['pelvis']), pos(request['head']), pos(request['chest'])
    torso_length = float(np.linalg.norm(head - pelvis))
    if torso_length <= 1e-6:
        return None, _receipt('unavailable', 'The torso has no height.')
    forward = np.array([request['forward'][0], request['forward'][1], 0.])
    if np.linalg.norm(forward) < 1e-6:
        forward = np.array([0., -1., 0.])
    forward /= np.linalg.norm(forward)
    up = np.array([0., 0., 1.])
    left = np.cross(up, forward)
    high = float(pos(request['high'])[2])
    rows = _torso_rows(rig, meshes, set(request['torso']), chest[2] - 0.3 * torso_length, high)
    if len(rows) < 30:
        return None, _receipt('flat', 'Too few chest vertices are skinned to the torso.')
    points = np.array([r[2] for r in rows])
    rel = points - chest
    f, l, z = rel @ forward, rel @ left, points[:, 2]
    t = torso_length
    # Front surface over the chest (sideways x height): the most forward skin
    # in overlapping windows, then the median of each 3x3 neighbourhood, so
    # small ornaments, bows, straps and gaps in low-poly meshes do not count.
    across = np.arange(-0.3, 0.3001, 0.02) * t
    heights = np.arange(chest[2] - 0.3 * t, high + 1e-9, 0.015 * t)
    raw = np.full((len(heights), len(across)), np.nan)
    for i, height in enumerate(heights):
        row = np.abs(z - height) <= 0.015 * t
        if not row.any():
            continue
        for j, side in enumerate(across):
            window = row & (np.abs(l - side) <= 0.02 * t)
            if window.any():
                raw[i, j] = f[window].max()
    surface = np.full_like(raw, np.nan)
    for i in range(len(heights)):
        for j in range(len(across)):
            block = raw[max(i - 1, 0):i + 2, max(j - 1, 0):j + 2]
            if np.count_nonzero(~np.isnan(block)) >= 5:
                surface[i, j] = np.nanmedian(block)
    # Breasts are in the upper torso: never search the hips (a butt seen from a
    # wrongly guessed facing direction must not become a breast).
    floor = max(chest[2] - 0.12 * t, pelvis[2] + 0.35 * (high - pelvis[2]))
    rows_ok = (heights >= floor) & (heights <= high - 0.04 * t)
    middle = np.abs(across) <= 0.021 * t
    sides = {}
    for side, sign in (('L', 1.), ('R', -1.)):
        cols = (sign * across >= 0.04 * t) & (sign * across <= 0.3 * t)
        region = np.where(rows_ok[:, None] & cols[None, :], surface, np.nan)
        if np.all(np.isnan(region)):
            return None, _receipt('flat', 'No chest surface on both sides.')
        i, j = np.unravel_index(np.nanargmax(region), region.shape)
        az, al, af = float(heights[i]), float(across[j]), float(surface[i, j])
        peak = (i, j)
        # Lowest front below the apex: the ribcage under the breast.
        below = (heights >= az - 0.25 * t) & (heights <= az - 0.05 * t)
        under = surface[below][:, max(j - 1, 0):j + 2]
        if np.all(np.isnan(under)):
            return None, _receipt('flat', 'The chest has no surface below the breasts to compare with.')
        under = float(np.nanmin(under))
        cleavage = surface[i, middle]
        dip = af - float(np.nanmax(cleavage)) if not np.all(np.isnan(cleavage)) else 0.
        # The bulge: surface standing at least halfway out near the peak. Its
        # centre (not the single most forward point) places the breast, which
        # keeps tops that bridge the cleavage from pulling it to the middle.
        level = af - 0.5 * min(af - under, 0.12 * t)
        near = (np.abs(heights - az) <= 0.12 * t)[:, None] & (np.abs(across - al) <= 0.12 * t)[None, :] & cols[None, :]
        bulge = np.where(near & (np.nan_to_num(surface, nan=-np.inf) >= level), surface - level, 0.)
        if bulge.sum() > 0:
            az = float((bulge.sum(axis=1) * heights).sum() / bulge.sum())
            al = float((bulge.sum(axis=0) * across).sum() / bulge.sum())
        width_cols = np.where(bulge[peak[0]] > 0)[0]
        width = (across[width_cols.max()] - across[width_cols.min()]) / 2 + 0.02 * t if len(width_cols) else 0.
        sides[side] = (az, al, af, under, dip, abs(width))
    lft, rgt = sides['L'], sides['R']
    rises = [s[2] - s[3] for s in (lft, rgt)]
    # Breasts stand well out from the ribcage, or clearly out with a cleavage
    # between them. Male chests, children and flat torsos stay below this.
    breasts = all(rise >= 0.06 * t or (rise >= MIN_PROTRUSION * t and s[4] >= 0.015 * t)
                  for rise, s in zip(rises, (lft, rgt)))
    if not breasts:
        return None, _receipt('flat', 'The chest is too flat for breast motion.',
                              protrusion_ratio=round(min(rises) / t, 4), measured=_measured(sides, t))
    if (abs(lft[0] - rgt[0]) > 0.045 * t or abs(lft[1] + rgt[1]) > 0.045 * t
            or abs(lft[2] - rgt[2]) > 0.03 * t):
        return None, _receipt('flat', 'The chest shape is not a mirrored pair of breasts.', measured=_measured(sides, t))
    # Mirror the two sides so the bones form an exact pair.
    az = (lft[0] + rgt[0]) / 2
    al = (lft[1] - rgt[1]) / 2
    apex_f = (lft[2] + rgt[2]) / 2
    depth = min(apex_f - (lft[3] + rgt[3]) / 2, 0.12 * t)
    base = apex_f - depth
    radius = min(max((lft[5] + rgt[5]) / 2, depth * 0.9, 0.04 * t), 0.15 * t)
    # Like most rigs, the bone sits near the chest wall, where a breast pivots.
    center_f = base + 0.2 * depth
    centers = {side: chest + forward * center_f + left * (sign * al) + up * (az - chest[2])
               for side, sign in (('L', 1.), ('R', -1.))}
    plan = {'rows': rows, 'f': f, 'l': l, 'z': z, 'centers': centers, 'radius': radius,
            'forward': forward, 'base': base, 'depth': depth, 'al': al, 'az': az, 'center_f': center_f}
    return plan, None


def _measured(sides, t):
    """Per-side measurements as shares of the torso length, for the receipt."""
    return {side: {'height': round(v[0] / t, 3), 'across': round(v[1] / t, 3), 'rise': round((v[2] - v[3]) / t, 3),
                   'cleavage': round(v[4] / t, 3)} for side, v in sides.items()}


def _falloff(plan, index, sign):
    """Weight share for the breast on `sign`: 1 at its center, 0 at its edge,
    nothing behind the chest wall."""
    radius, depth = plan['radius'], max(plan['depth'], 1e-9)
    f, l, z = plan['f'][index], plan['l'][index], plan['z'][index]
    if f < plan['base'] - 0.15 * depth:
        return 0.
    across = ((l - sign * plan['al']) ** 2 + (z - plan['az']) ** 2) / radius ** 2
    behind = (max(0., plan['center_f'] - f) / (depth * 1.2)) ** 2
    t = math.sqrt(across + behind)
    return 0. if t >= 1. else (1. - t * t) ** 2


def add_breast_bones(rig, meshes, request):
    """Create DMB_Breast_L/R under the chest and move nearby torso skin weight
    to them. Returns a receipt."""
    if any(name in rig.data.bones for name in NAMES):
        return _receipt('present', 'Breast bones were already added.')
    plan, failure = find_breasts(rig, meshes, request)
    if failure:
        return failure
    torso = set(request['torso'])
    inverse = rig.matrix_world.inverted()
    bpy.context.view_layer.objects.active = rig
    for obj in bpy.context.view_layer.objects:
        obj.select_set(obj == rig)
    bpy.ops.object.mode_set(mode='EDIT')
    try:
        parent = rig.data.edit_bones[request['chest']]
        for name, side in zip(NAMES, ('L', 'R')):
            center = plan['centers'][side]
            bone = rig.data.edit_bones.new(name)
            bone.head = inverse @ Vector(center.tolist())
            bone.tail = inverse @ Vector((center + plan['forward'] * plan['radius'] * 0.6).tolist())
            bone.parent = parent
            bone.use_connect = False
            bone.use_deform = True
    finally:
        bpy.ops.object.mode_set(mode='OBJECT')
    moved = {name: 0 for name in NAMES}
    groups = {}
    for index, (obj, vertex_index, _) in enumerate(plan['rows']):
        shares = [(name, _falloff(plan, index, sign)) for name, sign in zip(NAMES, (1., -1.))]
        shares = [(name, w) for name, w in shares if w > 1e-4]
        if not shares:
            continue
        vertex = obj.data.vertices[vertex_index]
        names = {g.index: g.name for g in obj.vertex_groups}
        torso_groups = [(g.group, g.weight) for g in vertex.groups
                        if names.get(g.group) in torso and g.weight > 0]
        available = sum(w for _, w in torso_groups)
        if available <= 0:
            continue
        taken = min(1., sum(w for _, w in shares))
        for group, weight in torso_groups:
            obj.vertex_groups[group].add([vertex_index], weight * (1. - taken), 'REPLACE')
        for name, share in shares:
            key = (obj.name, name)
            if key not in groups:
                groups[key] = obj.vertex_groups.get(name) or obj.vertex_groups.new(name=name)
            groups[key].add([vertex_index], available * share / max(1., sum(w for _, w in shares)), 'REPLACE')
            moved[name] += 1
    if min(moved.values()) == 0:
        return _receipt('flat', 'No skin near the breast shapes could be moved to breast bones.')
    t = float(np.linalg.norm(np.array(rig.matrix_world @ rig.data.bones[request['head']].head_local)
                              - np.array(rig.matrix_world @ rig.data.bones[request['pelvis']].head_local)))
    return _receipt('added', bones=list(NAMES), parent=request['chest'],
                    radius_ratio=round(plan['radius'] / t, 4), protrusion_ratio=round(plan['depth'] / t, 4),
                    vertices=moved)
