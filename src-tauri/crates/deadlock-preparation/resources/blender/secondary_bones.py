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
BREAST_HINTS = ('breast','bust','boob','pectoral','oppai','おっぱい','乳')
# Butt bones move only when butt motion is turned on (off by default). ASCII
# words match whole name parts (Butt_L, glute_r; not Button); 尻 also counts on
# its own, but never inside 尻尾 (tail). Keep in sync with secondary_bones.rs
# and src/preparation/secondary-bones.ts.
BUTT_HINTS = ('butt','buttock','buttocks','glute','glutes','gluteus','booty','bum','ass','pigu','tunbu',
              '臀','屁股','お尻','おしり','ケツ')
# Charms and other small hanging pieces, matched without separators (Key_Chain
# is keychain). They move anywhere and override the rigid accessory words (a
# dangle earring moves). Keep in sync with secondary_bones.rs (CHARM_HINTS) and
# src/preparation/secondary-bones.ts.
CHARM_HINTS = ('charm','keychain','keyring','pendant','lanyard','dangle',
               'チャーム','キーホルダー','ペンダント','根付','挂件','掛件','吊坠','吊墜','挂坠','掛墜','吊饰','吊飾')
# Straps count as charms only when they hang from a gun bone.
GUN_STRAP_HINTS = ('strap','ストラップ')
# Moving gun parts (slide, magazine, trigger ...) are never charms.
GUN_PART_HINTS = ('slide','mag','magazine','clip','bullet','bullets','shell','shells','ammo','round','rounds','cartridge',
                  'casing','trigger','hammer','bolt','barrel','muzzle','cylinder','chamber','breech','sight','sights','scope',
                  'stock','grip','handle','safety','lever','pump','drum','receiver','rail','laser','silencer','suppressor',
                  'bayonet','blade','spring','guard','hopper','弾','弹','マガジン','スライド','トリガー','引き金','銃口','枪口')


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
    moving = any(h in n for h in HINTS if h not in ('hair','髪','发','髮','耳')) or charm_named(name)
    return rigid and not moving


def charm_named(name):
    n = ''.join(c for c in name.lower() if c.isalnum())
    return any(h in n for h in CHARM_HINTS)


def gun_part_named(name):
    words, n = tokens(name), name.lower()
    return any(h in words if h.isascii() else h in n for h in GUN_PART_HINTS)


def gun_marks(bone):
    """Bones of a gun candidate carry the candidate ids under 'weapon'."""
    marks = bone.get('weapon') if bone else None
    return [m for m in marks if isinstance(m, str)] if isinstance(marks, list) else []


def gun_charm(bone, byname, children, source_roles):
    """Charms on a gun: a chain hanging from a bone of a gun candidate. Two to
    four bones in one line whose tip hangs below their top by at least 40% of
    their length, without gun-part names; a strap on the gun qualifies in any shape."""
    roles = set(source_roles.values())
    parent = byname.get(bone.get('parent'))
    if parent is None or bone['name'] in roles or parent['name'] in roles or not gun_marks(parent):
        return False
    if any(h in bone['name'].lower() for h in GUN_STRAP_HINTS):
        return True
    distance = lambda a, b: math.sqrt(sum((a[k]-b[k])**2 for k in range(3)))
    chain, cursor = [bone], bone['name']
    while children.get(cursor):
        following = children[cursor]
        if len(following) != 1 or len(chain) > 4 or following[0] not in byname:
            return False
        cursor = following[0]
        chain.append(byname[cursor])
    path = sum(distance(a['position'], b['position']) for a, b in zip(chain, chain[1:]))
    return (2 <= len(chain) <= 4 and path > 1e-6
            and not any(b['name'] in roles or gun_part_named(b['name']) for b in chain)
            and chain[0]['position'][2] - chain[-1]['position'][2] >= 0.4*path)


def on_gun(root, bones, source_roles, gun):
    """Whether the moving part at root hangs from the gun candidate gun: a bone
    between the root and its body anchor belongs to that gun."""
    byname = {b['name']: b for b in bones}
    roles, steps = set(source_roles.values()), 0
    cursor = byname.get((byname.get(root) or {}).get('parent'))
    while cursor is not None:
        if cursor['name'] in roles or steps >= MAX_BONES:
            return False
        if gun in gun_marks(cursor):
            return True
        cursor, steps = byname.get(cursor.get('parent')), steps + 1
    return False


def named_secondary(name):
    """Named as a moving part (HINTS, ear, charm). Same as secondary_bones.rs named_secondary."""
    n = name.lower()
    return any(h in n for h in HINTS) or any(t in ('ear','ears') for t in re.split('[^a-z0-9]+', n)) or charm_named(name)


def named_breast(name):
    n = name.lower()
    return not named_butt(name) and (any(h in n for h in BREAST_HINTS)
            or ('胸' in n and ('左' in n or '右' in n or any(t in ('l','r','left','right') for t in tokens(name)))))


def named_butt(name):
    words, n = tokens(name), name.lower()
    return (any(h in words if h.isascii() else h in n for h in BUTT_HINTS)
            or ('尻' in n and '尻尾' not in n))


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
    """Facing direction in the ground plane, from the feet to the toes (default -Y).
    Only each foot's lowest child counts: the toes are on the ground, while boot
    straps, heels and pendants hanging from the foot can point anywhere."""
    f = [0., 0.]
    for side in ('foot_L', 'foot_R'):
        foot = byname.get(source_roles.get(side))
        if foot is None:
            continue
        toe = None
        for child in children.get(foot['name'], []):
            if child in byname and (toe is None or byname[child]['position'][2] < toe['position'][2]):
                toe = byname[child]
        if toe is not None:
            f[0] += toe['position'][0] - foot['position'][0]
            f[1] += toe['position'][1] - foot['position'][1]
    n = math.sqrt(f[0]**2 + f[1]**2)
    return [0., -1.] if n < 1e-6 else [f[0]/n, f[1]/n]


def torso_chain(byname, source_roles):
    """Torso bones between the pelvis and the neck (or head), where breast bones hang."""
    chain = []
    start = byname.get(source_roles.get('neck') or source_roles.get('head'))
    cursor = byname.get(start.get('parent')) if start else None
    while cursor is not None and len(chain) < 64:
        chain.append(cursor['name'])
        if cursor['name'] == source_roles.get('pelvis'):
            break
        cursor = byname.get(cursor.get('parent'))
    return chain


def body_pair_options(bones, byname, children, weighted, source_roles):
    """Short, weighted, non-role children of torso bones: where breast and butt
    bones hang. Returns (options, torso length, pelvis height, shoulder height)."""
    role_names = set(source_roles.values())
    pelvis, head = byname.get(source_roles.get('pelvis')), byname.get(source_roles.get('head'))
    if pelvis is None or head is None:
        return None
    distance = lambda a, b: math.sqrt(sum((a[k]-b[k])**2 for k in range(3)))
    torso = distance(pelvis['position'], head['position'])
    if torso <= 1e-6:
        return None
    parents = torso_chain(byname, source_roles)
    high = byname.get(source_roles.get('neck'), head)['position'][2]

    def short(bone):
        branch, at = [bone['name']], 0
        while at < len(branch):
            if len(branch) > 3:
                return False
            branch.extend(children.get(branch[at], []))
            at += 1
        return len(branch) <= 3 and all(n not in role_names and n in byname
                                        and distance(byname[n]['position'], bone['position']) <= 0.17*torso
                                        for n in branch)
    options = [b for b in bones if b.get('parent') in parents and b['name'] not in role_names
               and b['name'] in weighted and short(b)]
    return options, torso, pelvis['position'][2], high


def mirrored_pairs(options, byname, children, source_roles, torso, keep):
    """Mirrored pairs among options: same parent, same height and depth, opposite
    sides. keep(forward offset, height) filters by placement."""
    forward = facing(byname, children, source_roles)
    lateral = [-forward[1], forward[0]]

    def place(b):
        p = byname[b['parent']]
        d = [b['position'][0]-p['position'][0], b['position'][1]-p['position'][1]]
        return (d[0]*forward[0] + d[1]*forward[1], d[0]*lateral[0] + d[1]*lateral[1], b['position'][2])
    output = set()
    for i, a in enumerate(options):
        fa, la, za = place(a)
        partners = 0
        for j, b in enumerate(options):
            if j == i or b.get('parent') != a.get('parent'):
                continue
            fb, lb, zb = place(b)
            if (keep(fa, za) and keep(fb, zb) and abs(fa-fb) <= 0.03*torso and abs(za-zb) <= 0.03*torso
                    and la*lb < 0 and abs(la+lb) <= 0.04*torso and 0.06*torso <= abs(la-lb) <= 0.5*torso):
                partners += 1
        if partners == 1 and not rigid_named(a['name']):
            output.add(a['name'])
    return output


def breast_roots(bones, byname, children, weighted, source_roles):
    """Breast bones: weighted, short, non-role children of a torso bone that are
    named as breasts, or form a mirrored pair in front of the chest in the upper
    torso (never on the pelvis, never a butt)."""
    found = body_pair_options(bones, byname, children, weighted, source_roles)
    if found is None:
        return set()
    options, torso, pelvis, high = found
    spine = byname.get(source_roles.get('spine'))
    low = spine['position'][2] if spine is not None else pelvis
    floor = max(low, pelvis + 0.35*(high - pelvis))
    output = {b['name'] for b in options if named_breast(b['name'])}
    # An unnamed pair whose own name says it is another moving part (chest
    # ribbons over the breasts, hair, a cape) is never a breast.
    above = [b for b in options if b.get('parent') != source_roles.get('pelvis') and not named_butt(b['name']) and not named_secondary(b['name'])]
    output |= mirrored_pairs(above, byname, children, source_roles, torso,
                             lambda forward, z: forward >= 0.02*torso and floor < z < high)
    return output


def butt_roots(bones, byname, children, weighted, source_roles):
    """Butt bones: short weighted children of a torso bone named as a butt, or a
    mirrored pair low on the hips that is not in front of the body."""
    found = body_pair_options(bones, byname, children, weighted, source_roles)
    if found is None:
        return set()
    options, torso, pelvis, high = found
    breasts = breast_roots(bones, byname, children, weighted, source_roles)
    output = {b['name'] for b in options if named_butt(b['name'])}
    hips = pelvis + 0.25*(high - pelvis)
    output |= mirrored_pairs(options, byname, children, source_roles, torso,
                             lambda forward, z: forward <= 0.02*torso and pelvis - 0.25*torso <= z <= hips)
    return {n for n in output if n not in breasts and not named_breast(n)}


def butt_bones(bones, weighted, source_roles):
    byname = {b['name']: b for b in bones}
    children = {}
    for bone in bones:
        children.setdefault(bone.get('parent'), []).append(bone['name'])
    return sorted(butt_roots(bones, byname, children, set(weighted), source_roles))


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


def physics_groups(bones, weighted, source_roles, target_roles, breasts=None, butts=None):
    """breasts: True adds breast bones as moving parts, False leaves them on the
    body even when the source gives them physics, None keeps the earlier behavior.
    butts: True adds butt bones as moving parts, False leaves them on the hips
    even when the source gives them physics, None keeps the earlier behavior."""
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
    butt_set = butt_roots(bones, byname, children, weighted_set, source_roles)
    if breasts is False:
        physical -= breast_set
    if butts is False:
        physical -= butt_set
    verified = set(physical)
    for bone in bones:
        if not (named_secondary(bone['name'])
                or hanging_chain(bone, byname, children, weighted_set, source_roles)
                or gun_charm(bone, byname, children, source_roles)):
            continue
        if (rigid_named(bone['name']) or ring_chain(bone, byname, children, source_roles)
                or bone['name'] in breast_set or (butts is False and bone['name'] in butt_set)):
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
    # A breast (or butt) is usually one weighted bone; it moves as a whole.
    for enabled, found in ((breasts is True, breast_set), (butts is True, butt_set)):
        if not enabled:
            continue
        for root in found:
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


def gun_hands(gun):
    """The plan of each hand: the right (or only) gun, then a dual-wield hero's left gun."""
    if not gun:
        return []
    return [gun] + ([gun['left']] if gun.get('left') else [])


def removed_guns(gun):
    """Candidate ids of the guns left out: left-out hands and unused gun-file guns."""
    ids = ['+'.join(hand.get('objects', [])) for hand in gun_hands(gun) if hand.get('action') == 'remove']
    return ids + ['+'.join(objects) for objects in (gun or {}).get('drop', [])]


def selection(options, bones, weighted, source_roles, target_roles, target, gun=None):
    """gun: the weapon plan of this conversion. Moving parts hanging from a
    replaced gun hang from its main weapon bone (each hand's plan anchor);
    those on a gun that is left out are left out with it. Keep in sync with
    secondary_bones.rs."""
    enabled = options.get('preserve_physics_bones', False)
    if not isinstance(enabled, bool):
        raise ValueError('Keep existing physics bones must be on or off.')
    if not enabled:
        return None
    roots = options.get('physics_roots', [])
    if not isinstance(roots, list) or not roots or any(not isinstance(n, str) for n in roots) or len(set(roots)) != len(roots):
        raise ValueError('Choose the detected physics groups to keep.')
    removed = removed_guns(gun)
    if removed:
        roots = [r for r in roots if not any(on_gun(r, bones, source_roles, gun_id) for gun_id in removed)]
        if not roots:
            return None
    breasts = options.get('breast_motion')
    if breasts is not None and not isinstance(breasts, bool):
        raise ValueError('Breast motion must be on or off.')
    butts = options.get('butt_motion')
    if butts is not None and not isinstance(butts, bool):
        raise ValueError('Butt motion must be on or off.')
    if butts is False:
        # Butt groups saved with butt motion on stay on the hips once it is off.
        hips = set(butt_bones(bones, weighted, source_roles))
        roots = [r for r in roots if r not in hips]
        if not roots:
            return None
    choices = {g['root']: g for g in physics_groups(bones, weighted, source_roles, target_roles, breasts, butts)}
    if any(root not in choices for root in roots):
        raise ValueError('Saved physics groups are stale or unsupported. Inspect the original source again.')
    groups = [choices[n] for n in sorted(roots)]
    # Each fitted gun carries its own charms (a copied gun's charms stay on the original gun).
    for hand in gun_hands(gun):
        if hand.get('action') != 'replace' or hand.get('copy'):
            continue
        hand_id = '+'.join(hand.get('objects', []))
        for group in groups:
            if on_gun(group['root'], bones, source_roles, hand_id):
                if not hand.get('anchor'):
                    raise ValueError('The fitted gun has no weapon bone for its charms. Inspect the source again.')
                group['target_anchor'] = hand['anchor']
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
            orientation = mapped[:3, :3]
            size = abs(np.linalg.det(orientation)) ** (1. / 3.)
            if abs(size - 1.) > 1e-6:
                orientation = orientation / size  # a charm on a gun fitted at another scale
            # Reconstruct a proper frame from bpy float32 source orientations.
            world = _local_matrix(mapped[:3, 3], _quaternion(orientation), 1.)
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
