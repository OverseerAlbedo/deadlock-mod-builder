"""Keep source proportions on heroes with extreme proportions (Shiv's long right
arm, Wraith's short legs): the torso is scaled to the hero's hips and neck, the
legs follow the hero's leg joints, and an arm difference is spread over the
whole arm. Ordinary heroes keep the source fit unchanged.

Run with Blender's Python (NumPy):
  blender --background --factory-startup --python-exit-code 23 --python scripts/test_hero_proportions.py
"""
import sys
from pathlib import Path
import numpy as np
ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
from retarget import rest_maps, hero_extremes, ELBOW_SPREAD

ROLES = ['pelvis', 'spine', 'chest', 'neck', 'head'] + [p+'_'+s for s in 'LR' for p in
         ('upperarm', 'forearm', 'hand', 'thigh', 'shin', 'foot')]
PARENTS = {'pelvis': None, 'spine': 'pelvis', 'chest': 'spine', 'neck': 'chest', 'head': 'neck',
           **{f'upperarm_{s}': 'chest' for s in 'LR'}, **{f'forearm_{s}': f'upperarm_{s}' for s in 'LR'},
           **{f'hand_{s}': f'forearm_{s}' for s in 'LR'}, **{f'thigh_{s}': 'pelvis' for s in 'LR'},
           **{f'shin_{s}': f'thigh_{s}' for s in 'LR'}, **{f'foot_{s}': f'shin_{s}' for s in 'LR'}}


def body(arm=(0.27, 0.26), legs=(0.45, 0.42), right_forearm=None, pelvis=0.95):
    """A T-posed body facing -Y, Z up (pelvis-to-head 0.55)."""
    p = {'pelvis': (0, 0, pelvis), 'spine': (0, 0, pelvis+.1), 'chest': (0, 0, pelvis+.25),
         'neck': (0, 0, pelvis+.45), 'head': (0, 0, pelvis+.55)}
    for s, x in (('L', 1), ('R', -1)):
        upper, fore = arm[0], (right_forearm if s == 'R' and right_forearm else arm[1])
        p[f'upperarm_{s}'] = (x*.17, 0, pelvis+.42)
        p[f'forearm_{s}'] = (x*(.17+upper), 0, pelvis+.42)
        p[f'hand_{s}'] = (x*(.17+upper+fore), 0, pelvis+.42)
        p[f'thigh_{s}'] = (x*.1, 0, pelvis-.05)
        p[f'shin_{s}'] = (x*.1, 0, pelvis-.05-legs[0])
        p[f'foot_{s}'] = (x*.1, 0, pelvis-.05-legs[0]-legs[1])
    return {k: np.array(v, dtype=float) for k, v in p.items()}


def matrices(points, prefix):
    out = {}
    for role, position in points.items():
        m = np.eye(4); m[:3, 3] = position
        out[prefix+role] = m
    return out


def fit(source_points, target_points):
    source, target = matrices(source_points, 'src_'), matrices(target_points, '')
    mapping = {'src_'+r: r for r in ROLES}
    parents = {'src_'+r: ('src_'+PARENTS[r] if PARENTS[r] else None) for r in ROLES}
    bones = [{'name': n, 'position': source[n][:3, 3], 'parent': parents[n]} for n in source]
    targets = [{'name': r, 'position': target[r][:3, 3], 'parent': PARENTS[r]} for r in ROLES]
    maps, audit = rest_maps(source, target, mapping, {r: 'src_'+r for r in ROLES}, {r: r for r in ROLES}, parents,
                            fit_mode='preserve_proportions', source_bones=bones, target_bones=targets,
                            fit_arm_joints=True, fit_finger_curl=False)
    placed = {r: (maps['src_'+r] @ np.r_[source_points[r], 1.])[:3] for r in ROLES}
    return placed, audit


source = body(legs=(0.46, 0.43))
hero = {'normal': body(arm=(0.24, 0.23)), 'shiv': body(arm=(0.24, 0.23), right_forearm=0.36),
        'wraith': body(arm=(0.26, 0.24), legs=(0.36, 0.34), pelvis=0.76)}
assert hero_extremes(hero['normal'], {r: r for r in ROLES})[0] == []
assert hero_extremes(hero['shiv'], {r: r for r in ROLES})[0] == ['arms']
assert hero_extremes(hero['wraith'], {r: r for r in ROLES})[0] == ['legs']

# An ordinary hero keeps the source fit: no scale, source legs, feet on the floor.
placed, audit = fit(source, hero['normal'])
fit_audit = audit['hero_fit']
assert fit_audit['extremes'] == [] and fit_audit['torso_scale'] == 1. and audit['source_leg_lengths_preserved']
assert abs(min(placed['foot_L'][2], placed['foot_R'][2]) - hero['normal']['foot_L'][2]) < 1e-9
print('ordinary hero: ok')

# Shiv: the hands still reach the hero's hands, the elbow moves at most a small
# share of the arm, and the right forearm stretches less than a per-segment fit.
placed, audit = fit(source, hero['shiv'])
spread = audit['hero_fit']['arm_spread']['R']
for side in 'LR':
    assert np.allclose(placed['hand_'+side], hero['shiv']['hand_'+side], atol=1e-9), side
    assert np.allclose(placed['upperarm_'+side], hero['shiv']['upperarm_'+side], atol=1e-9), side
arm = np.linalg.norm(hero['shiv']['forearm_R']-hero['shiv']['upperarm_R'])+np.linalg.norm(hero['shiv']['hand_R']-hero['shiv']['forearm_R'])
assert np.linalg.norm(placed['forearm_R']-hero['shiv']['forearm_R']) <= ELBOW_SPREAD*arm+1e-9
assert spread['forearm_scale'] < spread['per_segment_forearm_scale'] and spread['upper_scale'] > spread['per_segment_upper_scale']
assert np.allclose(placed['pelvis'], hero['shiv']['pelvis']) and np.allclose(placed['neck'], hero['shiv']['neck'])
for side in 'LR':
    assert np.allclose(placed['foot_'+side], hero['shiv']['foot_'+side], atol=1e-9)
print('long right arm (Shiv): ok', spread)

# Wraith: the hips stay on the hero's hips (no lift from longer source legs), the
# neck lands on the hero's neck and the feet on the hero's feet.
placed, audit = fit(source, hero['wraith'])
assert audit['hero_fit']['extremes'] == ['legs'] and not audit['source_leg_lengths_preserved']
for role in ('pelvis', 'neck', 'upperarm_L', 'upperarm_R', 'hand_L', 'hand_R', 'foot_L', 'foot_R'):
    assert np.allclose(placed[role], hero['wraith'][role], atol=1e-9), role
print('short legs (Wraith): ok', audit['hero_fit']['torso_scale'])
