"""Distribute existing arm influence over qualified hero twist joints.

No geometry or skeleton edits. This is optional skinning, after anatomical
fitting and before the existing influence-limit/normalization pass. Body,
finger and retained physics influences are kept outside the redistribution.
"""
import numpy as np

POLICY = 'spatial-arm-twist-skinning-v1'
MODES = {'source', 'smooth_twist'}


def plan(target, roles, source_roles, mapping, matched):
    bones = {b['name']: b for b in target}
    primary = set(roles.values())
    result = []
    for side in ('L', 'R'):
        for start, end in (('upperarm', 'forearm'), ('forearm', 'hand')):
            role = start+'_'+side
            a, b = roles[role], roles[end+'_'+side]
            origin = np.asarray(bones[a]['matrix'], dtype=float)[:3, 3]
            axis = np.asarray(bones[b]['matrix'], dtype=float)[:3, 3]-origin
            length2 = float(axis@axis)
            if not np.isfinite(length2) or length2 < 1e-12:
                continue
            helpers = []
            for name, bone in bones.items():
                compact = ''.join(c.lower() for c in name if c.isalnum())
                # Only side branches on the reviewed segment. End markers,
                # unrelated controls and the next anatomical joint cannot act
                # as extra deformation joints merely because of their names.
                if (name in primary or bone.get('parent') != a or bone.get('deform') is False
                        or not ('twist' in compact or '捩' in compact or compact.endswith('roll'))
                        or any(x in compact for x in ('end', 'notwist', 'weapon', 'prop', 'cloth', 'hair', 'ctrl', 'control'))):
                    continue
                delta = np.asarray(bone['matrix'], dtype=float)[:3, 3]-origin
                t = float(delta@axis)/length2
                if .1 <= t <= .9 and float((delta-axis*t)@(delta-axis*t))/length2 <= .08**2:
                    helpers.append((t, name))
            helpers.sort()
            if not helpers or any(y[0]-x[0] < 1e-4 for x, y in zip(helpers, helpers[1:])):
                continue
            names = {a, *(n for _, n in helpers)}
            eligible = {source_roles[role]} if mapping.get(source_roles[role]) == a else set()
            eligible.update(n for n, item in matched.items() if item['kind'] == 'twist'
                            and item['role'] == role and mapping.get(n) in names)
            if eligible:
                result.append({'role': role, 'base': a, 'origin': origin, 'axis': axis,
                               'length2': length2, 'helpers': helpers, 'sources': eligible,
                               'upper': start == 'upperarm'})
    return result


def distribution(segment, t):
    """Continuous weights in normalized segment coordinates, not bone indices."""
    base = segment['base']
    helpers = segment['helpers']
    # The upper arm keeps its shoulder rotation as the dominant influence.
    # The forearm transitions to the distal twist joint near the wrist, as in
    # the supplied reference; existing hand/finger weights remain untouched.
    anchors = [(0., base), *helpers, (1., base if segment['upper'] else helpers[-1][1])]
    for (left, a), (right, b) in zip(anchors, anchors[1:]):
        if t <= right:
            u = min(1., max(0., (t-left)/(right-left)))
            u = u*u*(3.-2.*u)
            values = {a: 1.-u}
            values[b] = values.get(b, 0.)+u
            if segment['upper']:
                values = {n: w*.35 for n, w in values.items()}
                values[base] = values.get(base, 0.)+.65
            return {n: w for n, w in values.items() if w > 1e-12}
    return {base: 1.}


def redistribute(rows, points, mapping, segments, limit=4, protected_sources=()):
    """Move arm-only weight mass; source data and unrelated weights are read-only."""
    if len(rows) != len(points) or not np.isfinite(points).all():
        raise ValueError('Arm skinning requires one finite position per skin row.')
    by_source = {name: i for i, segment in enumerate(segments) for name in segment['sources']}
    protected_sources = set(protected_sources)
    result, changed, outside, protected, limited = [], 0, 0, 0, 0
    per_segment = {segment['role']: 0 for segment in segments}
    for row, point in zip(rows, points):
        merged, groups = {}, {}
        for name, weight in row:
            if weight <= 0:
                continue
            dest = mapping[name]
            merged[dest] = merged.get(dest, 0.)+weight
            if name in by_source:
                group = groups.setdefault(by_source[name], {})
                group[dest] = group.get(dest, 0.)+weight
        before = merged.copy()
        if any(n in protected_sources and w > 0 for n, w in row):
            protected += 1
            result.append(list(before.items()))
            continue
        if len(before) > limit:
            limited += 1
            result.append(list(before.items()))
            continue
        total = sum(merged.values())
        adjusted = []
        for index, group in groups.items():
            segment = segments[index]
            mass = sum(group.values())
            if mass < .25*total:
                continue
            delta = np.asarray(point)-segment['origin']
            t = float(delta@segment['axis'])/segment['length2']
            radial2 = float((delta-t*segment['axis'])@(delta-t*segment['axis']))/segment['length2']
            # Do not turn remote arm-following cloth/attachments into arm skin.
            if not 0. <= t <= 1. or radial2 > .30**2:
                outside += 1
                continue
            for name, weight in group.items():
                merged[name] -= weight
            # Fade into the authored wrist crease. This keeps the spatial
            # boundary continuous when a vertex lies just beyond the segment.
            fade = min(1., (1.-t)/.08)
            fade = fade*fade*(3.-2.*fade)
            for name, weight in group.items():
                merged[name] = merged.get(name, 0.)+(1.-fade)*weight
            for name, weight in distribution(segment, t).items():
                merged[name] = merged.get(name, 0.)+fade*mass*weight
            adjusted.append(segment['role'])
        merged = {n: max(0., w) for n, w in merged.items() if w > 1e-12}
        # Keep the existing seam when smoothing would exceed its influence
        # budget. Never prune a finger/torso/other-region influence to make
        # room for extra arm helpers, or renormalize that unrelated weight.
        if len(merged) > limit:
            limited += 1
            result.append(list(before.items()))
            continue
        if abs(sum(merged.values())-total) > max(1., total)*1e-10:
            raise ValueError('Arm skinning changed total vertex influence.')
        changed += any(abs(merged.get(n, 0.)-before.get(n, 0.)) > 1e-8 for n in set(merged)|set(before))
        for role in adjusted:
            per_segment[role] += 1
        result.append(list(merged.items()))
    return result, {'policy': POLICY, 'adjusted_vertices': changed,
                    'segment_vertices': per_segment, 'outside_limb_skipped': outside,
                    'physics_vertices_preserved': protected, 'influence_budget_preserved': limited,
                    'target_binds_modified': False, 'geometry_modified': False}
