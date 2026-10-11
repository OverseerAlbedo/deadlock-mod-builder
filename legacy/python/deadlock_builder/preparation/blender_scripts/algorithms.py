"""Conservative humanoid recognition and transfer, shared with offline tests.

No automatic artistic-acceptance score. Ambiguity is returned for human review.
Coordinates passed to these functions are world-space, Z-up.
"""
import re
import math
import numpy as np

ALIASES = {
 'pelvis': ('hips','pelvis','hip'), 'spine': ('spine','spine1','spine01'),
 'chest': ('chest','upperchest','spine2','spine3','spine02','spine03'),
 'neck': ('neck','neck1'), 'head': ('head',),
 'upperarm': ('upperarm','armupper','arm'), 'forearm': ('forearm','lowerarm','armlower'),
 'hand': ('hand','wrist'), 'thigh': ('thigh','upleg','upperleg','legupper','hip'),
 'shin': ('calf','leg','lowerleg','leglower','knee','shin'), 'foot': ('foot','ankle'),
}
ROLES = ('pelvis','spine','chest','neck','head') + tuple(r+'_'+s for s in ('L','R') for r in ('upperarm','forearm','hand','thigh','shin','foot'))
CHAINS = [('pelvis','spine','chest','neck','head')] + [(a+'_'+s,b+'_'+s,c+'_'+s) for s in ('L','R') for a,b,c in [('upperarm','forearm','hand'),('thigh','shin','foot')]]


def name_role(name):
    name = name.split(':')[-1].lower()
    name = re.sub(r'^(bip\d+|mixamorig)[_ .-]*','',name)
    compact = re.sub('[^a-z0-9]','',name)
    for role in ROLES:
        base, _, side = role.partition('_')
        for alias in ALIASES[base]:
            variants = [alias] if not side else [alias+side.lower(),side.lower()+alias,('left' if side=='L' else 'right')+alias,alias+('left' if side=='L' else 'right')]
            if compact in variants:
                return role
    return None


def identify(bones, overrides=None):
    names = {b['name']:b for b in bones}
    if len(names)!=len(bones): raise ValueError('Duplicate bone names.')
    if any(b.get('parent') is not None and b['parent'] not in names for b in bones):
        raise ValueError('Bone references a missing parent.')
    roles, ambiguous = {}, []
    for b in bones:
        role=name_role(b['name'])
        if role:
            if role in roles:
                ambiguous.append(role)
            else: roles[role]=b['name']
    for role in ambiguous: roles.pop(role,None)
    for role, name in (overrides or {}).items():
        if role not in ROLES or name not in names: raise ValueError('Invalid landmark/bone override.')
        roles[role]=name
    errors=[]
    missing=[r for r in ROLES if r not in roles]
    if missing: errors.append('Assign humanoid roles: '+', '.join(missing))
    def ancestor(child,parent):
        seen=set()
        while child is not None:
            if child in seen: raise ValueError('Cyclic bone hierarchy.')
            seen.add(child)
            if child==parent:return True
            child=names[child].get('parent')
        return False
    for b in names: ancestor(b,'')
    for chain in CHAINS:
        for a,b in zip(chain,chain[1:]):
            if a in roles and b in roles and not ancestor(roles[b],roles[a]):
                errors.append(f'Hierarchy mismatch: {b} is not below {a}.')
    if not missing:
        pts={r:np.array(names[n]['position'],dtype=float) for r,n in roles.items()}
        if not all(np.isfinite(p).all() for p in pts.values()):raise ValueError('Invalid bone positions.')
        height=pts['head'][2]-min(pts['foot_L'][2],pts['foot_R'][2])
        if height<=0:errors.append('Orient the humanoid upright (Z up).')
        else:
            if pts['head'][2]<=pts['pelvis'][2] or min(pts['thigh_L'][2],pts['thigh_R'][2])<=max(pts['foot_L'][2],pts['foot_R'][2]):
                errors.append('Head/pelvis/leg positions are not upright humanoid landmarks.')
            # Derive the horizontal lateral direction from the paired hips.
            # Valve heroes can face +X (left/right Y), imported rigs often face Y.
            lateral=pts['thigh_L']-pts['thigh_R'];lateral[2]=0
            width=float(np.linalg.norm(lateral))
            if width<height*.025:
                errors.append('Review left/right spatial separation: thigh')
            else:
                lateral/=width
            for base in ('upperarm','hand','thigh','foot'):
                l,r=pts[base+'_L'],pts[base+'_R']
                lp=float(np.dot(l-pts['pelvis'],lateral));rp=float(np.dot(r-pts['pelvis'],lateral))
                if lp<=0 or rp>=0 or np.linalg.norm(l-r)<height*.025:
                    errors.append('Review left/right spatial separation: '+base)
            for chain in CHAINS:
                for a,b in zip(chain,chain[1:]):
                    distance=np.linalg.norm(pts[a]-pts[b])/height
                    if not .005<distance<.6:errors.append('Implausible segment: '+a+' → '+b)
    return {'roles':roles,'missing':missing,'errors':errors,'status':'Needs Review' if errors else 'Supported Humanoid',
            'evidence':'Names + ancestor chains + left/right and vertical landmark tests; human confirmation required.'}


def suggest_mapping(source, target, source_roles=None, target_roles=None):
    a=identify(source,source_roles);b=identify(target,target_roles)
    mapping={name:b['roles'][role] for role,name in a['roles'].items() if role in b['roles']}
    target_names={x['name'] for x in target}
    for bone in source:
        if bone['name'] in target_names: mapping.setdefault(bone['name'],bone['name'])
    return {'source':a,'target':b,'mapping':mapping}


def normalize_weights(rows, mapping, targets, limit=4):
    if not 1<=limit<=4:raise ValueError('Influence limit must be 1–4 in this preview.')
    valid=set(targets);result=[];discarded=0.;trimmed=0
    for index,row in enumerate(rows):
        merged={}
        for name,weight in row:
            if not math.isfinite(weight) or weight<0:raise ValueError('Invalid weight at vertex '+str(index))
            if weight==0:continue
            target=mapping.get(name)
            if target not in valid:raise ValueError('Weighted source bone needs a target: '+name)
            merged[target]=merged.get(target,0.)+weight
        ordered=sorted(merged.items(),key=lambda p:(-p[1],p[0]))
        if len(ordered)>limit:
            discarded+=sum(w for _,w in ordered[limit:]);trimmed+=1
        kept=ordered[:limit];total=sum(w for _,w in kept)
        if total<=1e-12:raise ValueError('Unweighted vertex: '+str(index))
        result.append([(n,w/total) for n,w in kept])
    return result, {'vertices':len(rows),'weighted':len(result),'unweighted':0,
                    'max_influences':max((len(x) for x in result),default=0),
                    'trimmed_vertices':trimmed,'discarded_weight_sum':discarded}


def estimate_landmarks(vertices):
    p=np.array(vertices,dtype=float)
    if p.ndim!=2 or p.shape[1]!=3 or len(p)<30 or not np.isfinite(p).all():
        raise ValueError('Unsupported: too little valid humanoid geometry.')
    lo,hi=p.min(axis=0),p.max(axis=0);size=hi-lo;h=size[2]
    if h<=0 or not .22<size[0]/h<1.6 or size[1]/h>.65:
        raise ValueError('Unsupported: expected an upright conventional T/A-pose humanoid. Review orientation.')
    center=(lo+hi)*.5
    q=(p-center)/h;z=(p[:,2]-lo[2])/h
    head=p[z>.88];legs=p[(z>.12)&(z<.4)]
    if len(head)<3 or len(legs)<6 or np.ptp(head[:,0])>.4*h:
        raise ValueError('Unsupported: cannot isolate one head and two legs.')
    # Two separated occupied leg intervals, not a single arbitrary box/column.
    x=np.sort(legs[:,0]);gaps=np.diff(x);gap=int(np.argmax(gaps))
    if gaps[gap]<h*.025 or not 1<gap<len(x)-3:
        raise ValueError('Unsupported: two separated legs were not identified.')
    landmarks={'pelvis':[center[0],center[1],lo[2]+.51*h], 'spine':[center[0],center[1],lo[2]+.61*h],
               'chest':[center[0],center[1],lo[2]+.73*h], 'neck':[center[0],center[1],lo[2]+.85*h],
               'head':np.mean(head,axis=0).tolist()}
    for side,sign in [('L',1),('R',-1)]:
        arm=p[(sign*(p[:,0]-center[0])>.19*h)&(z>.48)&(z<.86)]
        leg=legs[sign*(legs[:,0]-center[0])>0]
        if len(arm)<3 or len(leg)<3:raise ValueError('Unsupported: missing arm or leg on '+side)
        wrist=arm[np.argmax(sign*(arm[:,0]-center[0]))]
        shoulder=np.array([center[0]+sign*.14*h,center[1],lo[2]+.77*h])
        if abs(wrist[0]-shoulder[0])<h*.13:raise ValueError('Needs review: arms must be separated from the torso.')
        hip=np.array([np.mean(leg[:,0]),center[1],lo[2]+.50*h])
        ankle=np.array([np.mean(leg[:,0]),center[1],lo[2]+.07*h])
        for role,point in [('upperarm',shoulder),('forearm',(shoulder+wrist)/2),('hand',wrist),
                           ('thigh',hip),('shin',(hip+ankle)/2),('foot',ankle)]:
            landmarks[role+'_'+side]=point.tolist()
    left=sum(q[:,0]>.04);right=sum(q[:,0]<-.04)
    if min(left,right)/max(left,right)<.55:raise ValueError('Unsupported: strong asymmetry or multiple characters; review in Blender.')
    return {'landmarks':landmarks,'bounds':[lo.tolist(),hi.tolist()], 'status':'Needs Review',
            'explanation':'Cross-section estimates for upright T/A poses. Confirm every landmark in Blender; no automatic anatomical certainty.'}


def deformation_checks(points, faces, weights, transforms):
    """Probe mapped bones one at a time; edge strain is diagnostic, never artistic proof."""
    p=np.asarray(points,dtype=float);checks=[]
    edges={tuple(sorted((face[i],face[(i+1)%len(face)]))) for face in faces for i in range(len(face))}
    for name,matrix in transforms.items():
        out=p.copy();branch=matrix if isinstance(matrix,dict) else {name:matrix}
        matrices={n:np.asarray(m) for n,m in branch.items()}
        for i,row in enumerate(weights):
            for n,w in row:
                if n in matrices and w:out[i]+=w*((matrices[n]@np.r_[p[i],1])[:3]-p[i])
        ratios=[]
        for a,b in edges:
            length=np.linalg.norm(p[a]-p[b])
            if length>1e-8:ratios.append(np.linalg.norm(out[a]-out[b])/length)
        extreme=int(sum(r>4 or r<.1 for r in ratios))
        checks.append({'bone':name,'status':'WARNING' if extreme else 'PASS','severe_edge_strain':extreme,
                       'finite':bool(np.isfinite(out).all())})
    return checks


def component_count(vertex_count,edges):
    parents=list(range(vertex_count))
    def find(i):
        while parents[i]!=i:
            parents[i]=parents[parents[i]];i=parents[i]
        return i
    for a,b in edges:
        if not 0<=a<vertex_count or not 0<=b<vertex_count:raise ValueError('Invalid topology edge.')
        ra,rb=find(a),find(b)
        if ra!=rb:parents[rb]=ra
    return len({find(i) for i in range(vertex_count)})
