"""Preserve reviewed physics-bound branches as ordinary bones, without simulation."""
import math
import re
import numpy as np
from exact_bind import _local_matrix, _quaternion

POLICY = 'preserve-source-physics-bones-v1'
BINDING_POLICY = 'verified-rigid-body-bone-binding-v1'
PREFIX = 'dmb_secondary_'
MAX_BONES = 512


# Keep in sync with secondary_bones.rs (HINTS) and src/preparation/secondary-bones.ts.
HINTS = ('hair','ponytail','braid','ribbon','streamer','skirt','cloth','cape','tail',
         'robe','sash','scarf','shawl','mantle','cloak','tassel','veil',
         '髪','发','髮','尻尾','しっぽ','耳','リボン','裙','マント','スカート','スカーフ')


# Rigid head and body accessories. Hair and ear names do not make these move
# (hairpin, hair ornament, 髪飾り, ear ornament); other motion names such as
# ribbon or tassel still win. Keep in sync with secondary_bones.rs (RIGID_HINTS)
# and src/preparation/secondary-bones.ts.
RIGID_HINTS = ('halo','hat','cap','pin','pins','clip','clips','crown','tiara','circlet','horn','horns',
               'helmet','helm','glasses','goggles','goggle','spectacles','visor','mask','earring',
               'earrings','piercing','hairpin','hairclip','barrette','hairband','headband','ornament',
               'accessory','acc','飾','冠','帽','角','眼鏡','メガネ','輪')
# Breast bones move only when breast motion is turned on.
BREAST_HINTS = ('breast','bust','boob','oppai','おっぱい','乳')


def tokens(name):
    """Split on separators and camel case: HatBrim_L -> hat, brim, l."""
    out, word, previous_lower = [], '', False
    for c in name:
        if not c.isalnum():
            if word:
                out.append(word.lower())
            word, previous_lower = '', False
            continue
        if c.isupper() and previous_lower and word:
            out.append(word.lower())
            word = ''
        previous_lower = c.islower() or (c.isascii() and c.isdigit())
        word += c
    if word:
        out.append(word.lower())
    return [t for t in (t.rstrip('0123456789') for t in out) if t]


def rigid_named(name):
    words, n = tokens(name), name.lower()
    rigid = any((h in words or (len(h) >= 6 and h in n)) if h.isascii() else h in n for h in RIGID_HINTS)
    moving = any(h in n for h in HINTS if h not in ('hair','髪','发','髮','耳'))
    return rigid and not moving


def named_breast(name):
    n = name.lower()
    return (any(h in n for h in BREAST_HINTS)
            or ('胸' in n and ('左' in n or '右' in n or any(t in ('l','r','left','right') for t in tokens(name)))))


def ring_chain(bone, byname, children, source_roles):
    """A closed loop (halo, crown band, hat brim): the single chain that contains
    the bone, from below its body anchor, ends close to where it starts."""
    distance = lambda a, b: math.sqrt(sum((a[k]-b[k])**2 for k in range(3)))
    roles, steps = set(source_roles.values()), 0
    while bone.get('parent') in byname and bone['parent'] not in roles and steps < MAX_BONES:
        bone, steps = byname[bone['parent']], steps + 1
    chain, cursor = [bone], bone['name']
    while children.get(cursor):
        following = children[cursor]
        if len(following) != 1 or len(chain) >= MAX_BONES or following[0] not in byname:
            return False
        cursor = following[0]
        chain.append(byname[cursor])
    path = sum(distance(a['position'], b['position']) for a, b in zip(chain, chain[1:]))
    return len(chain) >= 4 and path > 1e-6 and distance(chain[0]['position'], chain[-1]['position']) < 0.3*path


def facing(byname, children, source_roles):
    """Facing direction in the ground plane, from the feet to the toes (default -Y)."""
    f = [0., 0.]
    for side in ('foot_L', 'foot_R'):
        foot = byname.get(source_roles.get(side))
        if foot is None:
            continue
        for child in children.get(foot['name'], []):
            if child in byname:
                f[0] += byname[child]['position'][0] - foot['position'][0]
                f[1] += byname[child]['position'][1] - foot['position'][1]
    n = math.sqrt(f[0]**2 + f[1]**2)
    return [0., -1.] if n < 1e-6 else [f[0]/n, f[1]/n]


def breast_roots(bones, byname, children, weighted, source_roles):
    """Breast bones: weighted, short, non-role children of the chest (or spine)
    that are named as breasts, or form a mirrored pair in front of the chest
    between the spine and the neck."""
    role_names = set(source_roles.values())
    pelvis, head = byname.get(source_roles.get('pelvis')), byname.get(source_roles.get('head'))
    if pelvis is None or head is None:
        return set()
    distance = lambda a, b: math.sqrt(sum((a[k]-b[k])**2 for k in range(3)))
    torso = distance(pelvis['position'], head['position'])
    if torso <= 1e-6:
        return set()
    parents = [source_roles[r] for r in ('chest', 'spine') if r in source_roles]
    low = byname.get(source_roles.get('spine'), pelvis)['position'][2]
    high = byname.get(source_roles.get('neck'), head)['position'][2]

    def short(bone):
        branch, at = [bone['name']], 0
        while at < len(branch):
            if len(branch) > 3:
                return False
            branch.extend(children.get(branch[at], []))
            at += 1
        return len(branch) <= 3 and all(n not in role_names and n in byname
                                        and distance(byname[n]['position'], bone['position']) <= 0.15*torso
                                        for n in branch)
    options = [b for b in bones if b.get('parent') in parents and b['name'] not in role_names
               and b['name'] in weighted and short(b)]
    output = {b['name'] for b in options if named_breast(b['name'])}
    forward = facing(byname, children, source_roles)
    lateral = [-forward[1], forward[0]]

    def place(b):
        p = byname[b['parent']]
        d = [b['position'][0]-p['position'][0], b['position'][1]-p['position'][1]]
        return (d[0]*forward[0] + d[1]*forward[1], d[0]*lateral[0] + d[1]*lateral[1], b['position'][2])
    for i, a in enumerate(options):
        fa, la, za = place(a)
        partners = []
        for j, b in enumerate(options):
            if j == i or b.get('parent') != a.get('parent'):
                continue
            fb, lb, zb = place(b)
            if (fa >= 0.02*torso and fb >= 0.02*torso and abs(fa-fb) <= 0.03*torso and abs(za-zb) <= 0.03*torso
                    and la*lb < 0 and abs(la+lb) <= 0.04*torso and 0.06*torso <= abs(la-lb) <= 0.5*torso
                    and low < za < high):
                partners.append(b)
        if len(partners) == 1 and not rigid_named(a['name']):
            output.add(a['name'])
    return output


def breast_bones(bones, weighted, source_roles):
    byname = {b['name']: b for b in bones}
    children = {}
    for bone in bones:
        children.setdefault(bone.get('parent'), []).append(bone['name'])
    return sorted(breast_roots(bones, byname, children, set(weighted), source_roles))


def hanging_chain(bone, byname, children, weighted, source_roles):
    """Unnamed hanging part: a non-role chain of at least three weighted bones on
    the pelvis/spine/chest/neck/head that spans a quarter of the torso."""
    roles = set(source_roles.values())
    if bone['name'] in roles:
        return False
    parent = bone.get('parent')
    if not parent or not any(source_roles.get(r) == parent for r in ('pelvis','spine','chest','neck','head')):
        return False
    pelvis, head = byname.get(source_roles.get('pelvis')), byname.get(source_roles.get('head'))
    if pelvis is None or head is None:
        return False
    distance = lambda a, b: math.sqrt(sum((a[k]-b[k])**2 for k in range(3)))
    torso = distance(pelvis['position'], head['position'])
    if torso <= 1e-6:
        return False
    chain, cursor = [bone], bone['name']
    while children.get(cursor):
        following = children[cursor]
        if len(following) != 1 or len(chain) >= MAX_BONES or following[0] not in byname:
            return False
        cursor = following[0]
        chain.append(byname[cursor])
    return (len(chain) >= 3 and sum(1 for b in chain if b['name'] in weighted) >= 3
            and not any(b['name'] in roles for b in chain)
            and distance(chain[0]['position'], chain[-1]['position']) >= 0.25*torso)


def physics_groups(bones, weighted, source_roles, target_roles, breasts=None):
    """breasts: True adds breast bones as moving parts, False leaves them on the
    body even when the source gives them physics, None keeps the earlier behavior."""
    byname = {b['name']: b for b in bones}
    if len(byname) != len(bones):
        raise ValueError('Duplicate source bone identities.')
    physical = {b['name'] for b in bones if b.get('physics', {}).get('policy') == BINDING_POLICY
                and isinstance(b['physics'].get('dynamic_bodies'), list)
                and b['physics']['dynamic_bodies']
                and all(isinstance(n, str) and n for n in b['physics']['dynamic_bodies'])}
    children = {}
    for bone in bones:
        children.setdefault(bone.get('parent'), []).append(bone['name'])
    weighted_set = set(weighted)
    breast_set = breast_roots(bones, byname, children, weighted_set, source_roles)
    if breasts is False:
        physical -= breast_set
    verified = set(physical)
    for bone in bones:
        if not (any(h in bone['name'].lower() for h in HINTS) or any(t in ('ear','ears') for t in re.split('[^a-z0-9]+',bone['name'].lower()))
                or hanging_chain(bone, byname, children, weighted_set, source_roles)):
            continue
        if rigid_named(bone['name']) or ring_chain(bone, byname, children, source_roles) or bone['name'] in breast_set:
            continue
        branch, pending = set(), [bone['name']]
        while pending:
            name = pending.pop()
            if name in branch or len(branch) >= MAX_BONES:
                raise ValueError('Secondary chain is cyclic or exceeds 512 bones.')
            branch.add(name)
            pending.extend(children.get(name, []))
        if any(n in verified or n in source_roles.values() or
               any(byname[n].get('physics', {}).get(k) for k in ('dynamic_bodies','kinematic_bodies'))
               for n in branch):
            continue
        used = [byname[n] for n in weighted if n in branch]
        if len(used) >= 2 and any(sum((a-b)**2 for a,b in zip(b['position'],used[0]['position'])) > 1e-10 for b in used[1:]):
            physical.add(bone['name'])
    if breasts is True:
        # A breast is usually one weighted bone; it moves as a whole.
        for root in breast_set:
            if not any(byname[root].get('physics', {}).get(k) for k in ('dynamic_bodies','kinematic_bodies')):
                physical.add(root)
    groups = []
    for root in sorted(physical):
        cursor, seen, has_dynamic_parent = byname[root].get('parent'), {root}, False
        anchor = None
        while cursor is not None:
            if cursor in seen or cursor not in byname:
                raise ValueError('Invalid source physics bone ancestry.')
            seen.add(cursor)
            if cursor in physical:
                has_dynamic_parent = True
            if anchor is None and cursor in source_roles.values():
                anchor = cursor
            cursor = byname[cursor].get('parent')
        if has_dynamic_parent or anchor is None:
            continue
        names, pending = [], [root]
        while pending:
            name = pending.pop(0)
            if name in names or len(names) >= MAX_BONES:
                raise ValueError('Source physics branch is cyclic or exceeds 512 bones.')
            names.append(name)
            pending.extend(sorted(n for n, b in byname.items() if b.get('parent') == name))
        if any(n in source_roles.values() for n in names):
            continue  # Never treat body roles as secondary physics branches.
        used = sorted(set(names) & set(weighted))
        if not used:
            continue
        role = next(r for r, n in sorted(source_roles.items()) if n == anchor)
        destination = target_roles.get(role)
        if not destination:
            continue
        groups.append({'root': root, 'source_anchor': anchor, 'target_anchor': destination,
                       'source_bones': names, 'weighted_bones': used})
    return groups


def selection(options, bones, weighted, source_roles, target_roles, target):
    enabled = options.get('preserve_physics_bones', False)
    if not isinstance(enabled, bool):
        raise ValueError('Keep existing physics bones must be on or off.')
    if not enabled:
        return None
    roots = options.get('physics_roots', [])
    if not isinstance(roots, list) or not roots or any(not isinstance(n, str) for n in roots) or len(set(roots)) != len(roots):
        raise ValueError('Choose the detected physics groups to keep.')
    breasts = options.get('breast_motion')
    if breasts is not None and not isinstance(breasts, bool):
        raise ValueError('Breast motion must be on or off.')
    choices = {g['root']: g for g in physics_groups(bones, weighted, source_roles, target_roles, breasts)}
    if any(root not in choices for root in roots):
        raise ValueError('Saved physics groups are stale or unsupported. Inspect the original source again.')
    groups = [choices[n] for n in sorted(roots)]
    names = [n for g in groups for n in g['source_bones']]
    if len(names) != len(set(names)) or len(names) > MAX_BONES:
        raise ValueError('Selected physics branches overlap or exceed 512 bones.')
    target_names = {b['name'] for b in target}
    if any(g['target_anchor'] not in target_names for g in groups):
        raise ValueError('Choose a valid target attachment for source physics bones.')
    exported = {n: PREFIX + n for n in names}
    for name, output in exported.items():
        if output in target_names or len(output.encode('utf-8')) > 63 or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in output):
            raise ValueError('A preserved physics bone has a conflicting or unsupported name: ' + name)
    return {'policy': POLICY, 'groups': groups, 'exported_bones': exported,
            'new_physics': False, 'in_game_motion_tested': False}


def extend_target(target, source, parents, plan, anchor_transforms, group_transforms=None):
    """Append exact T*R*S locals; retain original target records unchanged."""
    result = list(target)
    worlds = {b['name']: np.asarray(b['matrix'], dtype=float) for b in target}
    for group in plan['groups']:
        transform = (group_transforms[group['root']] if group_transforms is not None
                     else anchor_transforms[group['source_anchor']])
        for name in group['source_bones']:
            output = plan['exported_bones'][name]
            parent = group['target_anchor'] if name == group['root'] else plan['exported_bones'][parents[name]]
            mapped = transform @ np.asarray(source[name], dtype=float)
            # Reconstruct a proper frame from bpy float32 source orientations.
            world = _local_matrix(mapped[:3, 3], _quaternion(mapped[:3, :3]), 1.)
            local = np.linalg.inv(worlds[parent]) @ world
            scale = math.sqrt(float(np.trace(local[:3, :3].T @ local[:3, :3])) / 3.)
            rotation = _quaternion(local[:3, :3] / scale)
            local = _local_matrix(local[:3, 3], rotation, scale)
            world = worlds[parent] @ local
            worlds[output] = world
            result.append({'name': output, 'parent': parent,
                           'local_position': local[:3, 3].tolist(), 'local_rotation': rotation,
                           'local_scale': scale, 'position': world[:3, 3].tolist(),
                           'matrix': world.tolist(), 'inverse_bind_matrix': np.linalg.inv(world).tolist(),
                           'length': 1.})
    return result
