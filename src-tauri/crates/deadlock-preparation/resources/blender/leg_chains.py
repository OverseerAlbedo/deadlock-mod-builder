"""Parallel deform-leg equivalence, mirrored by the Rust suggestion/review gate.

Every joint must match a complete same-side chain within 0.5% of leg length.
Unrelated helper, IK, shadow and secondary names do not qualify.
"""
from algorithms import name_role
import numpy as np

POLICY = 'parallel-leg-skin-chains-v1'
PARTS = ('thigh', 'shin', 'foot')


_ROLES = {}


def _role(name):
    name = name.rsplit(':', 1)[-1].removeprefix('DEF-')
    found = name_role(name)
    if found and found.split('_')[0] in PARTS:
        return found
    compact = ''.join(c.lower() if c.isascii() else c for c in name if c.isalnum())
    for side in ('L', 'R'):
        for part, base in zip(PARTS, ('足', 'ひざ', '足首')):
            for suffix in ('', 'd'):
                if compact in (base + suffix + side.lower(), ('左' if side == 'L' else '右') + base + suffix):
                    return part + '_' + side
    return None


def role(name):
    """_role, remembered per name."""
    try:
        return _ROLES[name]
    except KeyError:
        found = _ROLES[name] = _role(name)
        return found


def equivalents(bones, roles):
    names = {b['name']: b for b in bones}
    def ancestor(parent, child):
        seen = set()
        while child in names and child not in seen:
            if child == parent:
                return True
            seen.add(child)
            child = names[child].get('parent')
        return False
    def distance(a, b):
        return float(np.linalg.norm(np.asarray(a['position']) - b['position']))
    result = {}
    pelvis = roles.get('pelvis')
    if not pelvis:
        return result
    for side in ('L', 'R'):
        selected = [names.get(roles.get(p + '_' + side)) for p in PARTS]
        if any(b is None for b in selected):
            continue
        if not all(ancestor(a['name'], b['name']) for a, b in zip(selected, selected[1:])):
            continue
        length = sum(distance(a, b) for a, b in zip(selected, selected[1:]))
        if not np.isfinite(length) or length < 1e-7:
            continue
        choices = [[b for b in bones if b.get('deform', True) is not False
                    and role(b['name']) == p + '_' + side
                    and np.isfinite(b['position']).all()
                    and ancestor(pelvis, b['name'])] for p in PARTS]
        if any(len(c) > 16 for c in choices):
            continue
        for a in choices[0]:
            for b in choices[1]:
                for c in choices[2]:
                    chain = (a, b, c)
                    if not all(ancestor(x['name'], y['name']) and distance(x, y) > 1e-7
                               for x, y in zip(chain, chain[1:])):
                        continue
                    if all(distance(x, y) <= length * .005 for x, y in zip(chain, selected)):
                        result.update((x['name'], y['name']) for x, y in zip(chain, selected) if x['name'] != y['name'])
    return result
