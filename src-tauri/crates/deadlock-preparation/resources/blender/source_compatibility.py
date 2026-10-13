"""Recover a rest-only skin rig whose exporter omitted primary/control joints.

This is a bounded repair of disconnected roots, not a nearest-bone remapper.
Roles require a complete anatomical layout. Only missing links are added;
existing parents, world rest matrices, vertices and weights are never replaced.
Healthy rigs take the old path. Unknown/ambiguous rigs remain reviewable.
"""
import re
import numpy as np
from algorithms import identify, name_role, ROLES

POLICY = 'rest-skin-rig-recovery-v1'


def distance(a, b):
    return float(np.linalg.norm(np.asarray(a)-np.asarray(b)))


def helper_role(name):
    # Exported twist roots sometimes are the only surviving joint at a pivot.
    # Do not recognize numbered twist segments or IK/control bones as primaries.
    name = name.split(':')[-1]
    if not name.lower().endswith('twist'):
        return None
    name = re.sub(r'^(Bip\d+[_ .-]+)B_', r'\1', name, flags=re.I)
    name = re.sub(r'[_ .-]*twist$', '', name, flags=re.I)
    role = name_role(name)
    return role if role in {'neck','upperarm_L','upperarm_R','thigh_L','thigh_R'} else None


def side(name):
    value = name.lower()
    if re.search(r'(?:^|[_. -])l(?:$|[_. -])', value) or 'left' in value or value.startswith('左'):
        return 'L'
    if re.search(r'(?:^|[_. -])r(?:$|[_. -])', value) or 'right' in value or value.startswith('右'):
        return 'R'
    return None


def plan(bones, weighted):
    """Pure planner, also exercised independently of Blender and bone order."""
    base = identify(bones)
    if not base['errors']:
        return None
    by_name = {b['name']:b for b in bones}
    roles = dict(base['roles'])
    used = set(weighted)
    eligible = lambda b: b.get('deform', True) and np.isfinite(b['position']).all()

    def ancestor(parent, child):
        seen = set()
        while child in by_name and child not in seen:
            if child == parent:
                return True
            seen.add(child)
            child = by_name[child].get('parent')
        return False

    # 3ds Max Biped commonly has Spine + Spine1, with Spine1 as the chest.
    # Longer numbered chains also resolve by ancestry rather than suffix order.
    if 'pelvis' in roles and ('spine' not in roles or 'chest' not in roles):
        candidates = [b for b in bones if eligible(b) and name_role(b['name']) in {'spine','chest'}
                      and ancestor(roles['pelvis'], b['name'])]
        if 2 <= len(candidates) <= 8 and all(ancestor(a['name'], b['name']) or ancestor(b['name'], a['name'])
                                            for a in candidates for b in candidates):
            first = next(b for b in candidates if all(ancestor(b['name'], c['name']) for c in candidates))
            last = next(b for b in candidates if all(ancestor(c['name'], b['name']) for c in candidates))
            if roles.get('spine', first['name']) != roles.get('chest', last['name']):
                roles.setdefault('spine', first['name'])
                roles.setdefault('chest', last['name'])

    # Names are optional for joints inside an otherwise recognized chain. A
    # unique path must have exactly the missing number of deform joints; do not
    # choose one of several twists/controls by proximity or bone order.
    chains = [('pelvis','spine','chest','neck','head')]
    for suffix in ('L','R'):
        chains += [('chest','upperarm_'+suffix,'forearm_'+suffix,'hand_'+suffix),
                   ('pelvis','thigh_'+suffix,'shin_'+suffix,'foot_'+suffix)]
    for chain in chains:
        known = [i for i,r in enumerate(chain) if r in roles]
        for first,last in zip(known,known[1:]):
            missing = chain[first+1:last]
            if not missing:
                continue
            cursor, root, path, seen = by_name[roles[chain[last]]].get('parent'), roles[chain[first]], [], set()
            while cursor in by_name and cursor != root and cursor not in seen:
                seen.add(cursor)
                bone = by_name[cursor]
                if eligible(bone):
                    path.append(bone)
                cursor = bone.get('parent')
            if cursor != root or len(path) != len(missing):
                continue
            if any(b['name'] in roles.values() or re.search(r'twist|helper|control|(?:^|[_. -])(?:ik|ctrl|mch)(?:[_. -]|$)|hair|cloth|skirt', b['name'], re.I) for b in path):
                continue
            for role,bone in zip(missing,reversed(path)):
                roles[role] = bone['name']

    promoted = []
    for role in ROLES:
        if role in roles:
            continue
        candidates = [b for b in bones if eligible(b) and b['name'] in used and helper_role(b['name']) == role]
        if len(candidates) == 1:
            roles[role] = candidates[0]['name']
            promoted.append(candidates[0]['name'])
    if set(roles) != set(ROLES) or len(set(roles.values())) != len(ROLES):
        return None
    points = {r:by_name[n]['position'] for r,n in roles.items()}
    height = points['head'][2]-min(points['foot_L'][2], points['foot_R'][2])
    if not np.isfinite(height) or height <= 1e-6:
        return None
    # A surviving arm/thigh helper needs an independent, weighted joint at its
    # pivot as evidence. A mid-segment twist alone is not enough to infer a joint.
    for role, name in roles.items():
        if name not in promoted:
            continue
        point = points[role]
        if role == 'neck':
            if not points['chest'][2] < point[2] < points['head'][2] or distance(point[:2], points['head'][:2]) > height*.08:
                return None
        elif not any(b['name'] != name and b['name'] in used and eligible(b)
                     and side(b['name']) == role[-1] and distance(point,b['position']) < height*.005
                     for b in bones):
            return None

    parents = [('spine','pelvis'),('chest','spine'),('neck','chest'),('head','neck')]
    for suffix in ('L','R'):
        parents += [(a+'_'+suffix,b+'_'+suffix if b not in {'chest','pelvis'} else b)
                    for a,b in [('upperarm','chest'),('forearm','upperarm'),('hand','forearm'),
                                ('thigh','pelvis'),('shin','thigh'),('foot','shin')]]
    changes = {}
    for child_role, parent_role in parents:
        child, parent = roles[child_role], roles[parent_role]
        if ancestor(parent, child):
            continue
        # Never overwrite a conflicting real parent or cross between rigs.
        if by_name[child].get('parent') is not None:
            return None
        changes[child] = parent
    revised = [dict(b, parent=changes.get(b['name'], b.get('parent'))) for b in bones]
    if identify(revised, roles)['errors']:
        return None

    # Static detached corrective roots at a proven pivot can follow that pivot.
    # Named breast roots must lie in the chest region. Other unknown roots stay
    # unresolved; no generic nearest-joint fallback collapses hair or clothing.
    for b in bones:
        if b.get('parent') or b['name'] in roles.values() or b['name'] not in used or not eligible(b):
            continue
        n = b['name'].lower()
        if any(t in n for t in ('hair','cloth','skirt','tail','robe','cape','braid','髪','裙')):
            continue
        candidates = [r for r in ROLES if distance(b['position'], points[r]) < height*.005
                      and (not r.endswith(('_L','_R')) or side(b['name']) == r[-1])]
        if len(candidates) == 1 and any(t in n for t in ('hip','deltoid','twist','knee','elbow','wrist')):
            changes[b['name']] = roles[candidates[0]]
        elif ('breast' in n or '胸' in n) and points['spine'][2] < b['position'][2] < points['neck'][2] and distance(b['position'], points['chest']) < height*.2:
            changes[b['name']] = roles['chest']
    revised = [dict(b, parent=changes.get(b['name'], b.get('parent'))) for b in bones]
    if identify(revised, roles)['errors']:
        return None
    return {'roles':roles,'parents':changes,'promoted':promoted}


def inspect_roles(rig):
    bones = [{'name':b.name,'parent':b.parent.name if b.parent else None,
              'position':list((rig.matrix_world@b.matrix_local).translation),'deform':b.use_deform}
             for b in rig.data.bones]
    return bones


def normalize(rig, meshes, reports=None):
    """Return an effective rest hierarchy; never edit Blender bones or binds.

    Reparenting in Edit Mode can re-round imported bone scale and axes. The
    converter already reads explicit matrices, so keep those original matrices
    and use the verified hierarchy only for role/mapping/attachment decisions.
    The final hero rig is constructed by the existing exact-bind exporter.
    """
    from source_helpers import physics_bone_bindings
    from exact_bind import METADATA_KEY
    if METADATA_KEY in rig:
        return None
    bones = inspect_roles(rig)
    from mesh_export import used_group_names
    weighted = used_group_names(meshes)
    proposal = plan(bones, weighted)
    if proposal is None:
        return None
    roles, parents = proposal['roles'], proposal['parents']
    if parents:
        if (rig.constraints or (rig.animation_data and rig.animation_data.drivers)
                or (rig.data.animation_data and rig.data.animation_data.drivers)
                or physics_bone_bindings(__import__('bpy').context.scene.objects, rig)
                or any(b.constraints or not np.allclose(np.asarray(b.matrix_basis), np.eye(4), rtol=1e-7, atol=1e-6)
                       for b in rig.pose.bones)
                or any(o.parent_type == 'BONE' and o.parent == rig for o in __import__('bpy').context.scene.objects)):
            return None
        receipt = {'policy':POLICY,'roles':roles,'reconnected_roots':parents,'promoted_helpers':proposal['promoted'],
                   'effective_rest_hierarchy_only':True,'original_bind_matrices_unchanged':True,
                   'rigid_body_meshes':0,'joint_empties':0,'objects':[],
                   'warning':'Recovered '+str(len(parents))+' disconnected links for rest conversion using surviving anatomical joints. '+
                             'Used '+str(len(proposal['promoted']))+' surviving helpers for missing body roles. '+
                             'Original rest matrices and skin weights are unchanged. Review shoulders, knees and head movement.'}
        if reports is not None:
            reports.append(receipt)
    return proposal
