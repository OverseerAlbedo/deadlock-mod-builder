"""Equivalent existing arm joints, with the same proof as native suggestions."""
import numpy as np

POLICY = 'anatomical-arm-deformation-v1'


def detect(source, target, sr, tr):
    def compact(n):
        return ''.join(c.lower() for c in n.rsplit(':', 1)[-1] if c.isalnum())

    def shoulder(n, side):
        return compact(n) in {v for base in ('shoulder', 'clavicle', 'clav', '肩')
            for v in (base+side.lower(), side.lower()+base,
                      ('left' if side == 'L' else 'right')+base, ('左' if side == 'L' else '右')+base)}

    def twist(n):
        n = compact(n)
        roll = n.startswith('roll') or n.endswith('roll') or any(v in n for v in ('armroll', 'armupperroll', 'armlowerroll'))
        return not any(v in n for v in ('notwist', 'weapon', 'prop', 'hair', 'cloth', 'skirt', 'tail', 'streamer', 'controller')) and not n.startswith(('ear', 'leftear', 'rightear')) and ('twist' in n or '捩' in n or roll)

    def helper(n):
        return twist(n) or any(v in compact(n) for v in ('shoulderpart', 'elbowpart', 'upperarmhelper', 'forearmhelper'))

    def side_agrees(n, side):
        n = n.rsplit(':', 1)[-1].lower()
        wrong = 'r' if side == 'L' else 'l'
        return not n.endswith(('.'+wrong, '_'+wrong)) and not n.startswith(('right', '右') if side == 'L' else ('left', '左'))

    def ancestor(bones, parent, name):
        seen = set()
        while name is not None and name not in seen:
            if name == parent:
                return True
            seen.add(name)
            name = bones.get(name, {}).get('parent')
        return False

    def coordinate(p, a, b):
        axis = np.asarray(b)-a
        length2 = float(axis @ axis)
        if not np.isfinite(length2) or length2 < 1e-12:
            return None
        delta = np.asarray(p)-a
        t = float(delta @ axis)/length2
        off = float((delta-axis*t) @ (delta-axis*t))/length2
        return t if np.isfinite(t) and .1 <= t <= .9 and off <= .08**2 else None

    source = {b['name']: b for b in source}
    target = {b['name']: b for b in target}
    out = {}
    for side in ('L', 'R'):
        upper = 'upperarm_'+side
        if all(k in r for r in (sr, tr) for k in ('chest', upper)):
            def collect(bones, r):
                return [n for n, b in bones.items() if b.get('deform') is not False and shoulder(n, side)
                        and ancestor(bones, r['chest'], n) and ancestor(bones, n, r[upper])]
            a, b = collect(source, sr), collect(target, tr)
            if len(a) == len(b) == 1:
                out[a[0]] = {'target': b[0], 'role': 'chest', 'kind': 'shoulder'}
        for start, end in (('upperarm', 'forearm'), ('forearm', 'hand')):
            role, next_role = start+'_'+side, end+'_'+side
            if not all(k in r for r in (sr, tr) for k in (role, next_role)):
                continue
            sa, sb, ta, tb = sr[role], sr[next_role], tr[role], tr[next_role]
            if not all(n in bones for bones, names in ((source, (sa, sb)), (target, (ta, tb))) for n in names):
                continue
            def eligible(bones, name, a, b):
                return name not in (a, b) and ancestor(bones, a, name) and not ancestor(bones, b, name)
            candidates = []
            for n, b in target.items():
                if b.get('deform') is False or not twist(n) or not eligible(target, n, ta, tb):
                    continue
                t = coordinate(b['position'], target[ta]['position'], target[tb]['position'])
                if t is not None:
                    candidates.append((n, t))
            for n, b in source.items():
                if b.get('deform') is False or not helper(n) or not side_agrees(n, side) or not eligible(source, n, sa, sb):
                    continue
                t = coordinate(b['position'], source[sa]['position'], source[sb]['position'])
                if t is None:
                    continue
                distances = sorted((abs(t-u), name) for name, u in candidates if abs(t-u) <= .20)
                if not distances or (len(distances)>1 and abs(distances[0][0]-distances[1][0]) < 1e-5):
                    continue
                out[n] = {'target': distances[0][1], 'role': role, 'kind': 'twist'}
    return out
