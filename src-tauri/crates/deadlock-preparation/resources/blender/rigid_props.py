"""Pure rigid prop geometry transforms shared by the app and external Blender.

Standalone on purpose: frozen distributions ship Blender bridge scripts as data,
while application Python modules live inside PyInstaller's archive.
"""
from copy import deepcopy
import math
import numpy as np


def prop_matrix(target, prop):
    """Canonical scaled target world bind times an explicit local T*Rxyz*S.

Never derive this from Blender EditBone, which deliberately strips bone scale.
Input translations and unscaled prop geometry are in Source 2 model units.
"""
    for key in ('translation','rotation'):
        value=prop.get(key,[0,0,0])
        if len(value)!=3 or not np.isfinite(np.asarray(value,dtype=float)).all():raise ValueError('Invalid prop '+key)
    scale=prop.get('scale',1.)
    if not isinstance(scale,(int,float)) or isinstance(scale,bool) or not math.isfinite(scale) or scale<=0:raise ValueError('Prop scale must be positive and finite.')
    matches=[bone for bone in target if bone['name']==prop.get('bone')]
    if len(matches)!=1:raise ValueError('Attach the prop to an existing target skeleton bone: '+repr(prop.get('bone')))
    bind=np.asarray(matches[0]['matrix'],dtype=float)
    if bind.shape!=(4,4) or not np.isfinite(bind).all() or np.linalg.det(bind[:3,:3])<=0:raise ValueError('Prop target bind matrix is invalid.')
    x,y,z=np.radians(prop.get('rotation',[0,0,0]));cx,sx=math.cos(x),math.sin(x);cy,sy=math.cos(y),math.sin(y);cz,sz=math.cos(z),math.sin(z)
    rx=np.array([[1,0,0],[0,cx,-sx],[0,sx,cx]])
    ry=np.array([[cy,0,sy],[0,1,0],[-sy,0,cy]])
    rz=np.array([[cz,-sz,0],[sz,cz,0],[0,0,1]])
    local=np.eye(4);local[:3,:3]=(rz@ry@rx)*prop.get('scale',1.)
    local[:3,3]=prop.get('translation',[0,0,0])
    return bind@local


def attach_mesh(mesh, target, prop):
    """Transform a plain mesh report and assign exact rigid skinning to one bone."""
    result=deepcopy(mesh);matrix=prop_matrix(target,prop)
    points=np.asarray(mesh['positions'],dtype=float)
    if points.ndim!=2 or points.shape[1]!=3 or not np.isfinite(points).all():raise ValueError('Invalid prop positions.')
    result['positions']=(np.c_[points,np.ones(len(points))]@matrix.T)[:,:3].tolist()
    normals=np.asarray(mesh['normals'],dtype=float)@np.linalg.inv(matrix[:3,:3])
    lengths=np.linalg.norm(normals,axis=1)
    if not np.isfinite(normals).all() or (lengths<=1e-12).any():raise ValueError('Invalid prop normals.')
    result['normals']=(normals/lengths[:,None]).tolist()
    # Tangents should be regenerated from final UV/normal data by Blender.
    result['tangents']=[]
    result['weights']=[[(prop['bone'],1.)] for _ in points]
    result['attachment']={'bone':prop['bone'],'local_transform':{key:deepcopy(prop.get(key, default)) for key,default in [('translation',[0,0,0]),('rotation',[0,0,0]),('scale',1.)]}}
    return result

