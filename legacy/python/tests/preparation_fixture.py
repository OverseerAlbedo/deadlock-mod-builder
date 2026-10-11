"""Generated fixtures, not claims about real imported character quality."""
import numpy as np
from deadlock_builder.preparation.blender_scripts.algorithms import ROLES

def humanoid(style='target'):
    positions={'pelvis':(0,0,1),'spine':(0,0,1.2),'chest':(0,0,1.5),'neck':(0,0,1.7),'head':(0,0,1.85)}
    parents={'pelvis':None,'spine':'pelvis','chest':'spine','neck':'chest','head':'neck'}
    for side,sign in [('L',1),('R',-1)]:
        for base,pos,parent in [('upperarm',(.3,0,1.5),'chest'),('forearm',(.6,0,1.5),'upperarm_'+side),('hand',(.9,0,1.5),'forearm_'+side),
                                ('thigh',(.15,0,.95),'pelvis'),('shin',(.15,0,.5),'thigh_'+side),('foot',(.15,-.1,.08),'shin_'+side)]:
            role=base+'_'+side;positions[role]=(sign*pos[0],pos[1],pos[2]);parents[role]=parent
    base_names={'pelvis':'Hips','spine':'Spine','chest':'Spine2','neck':'Neck','head':'Head','upperarm':'Arm','forearm':'ForeArm','hand':'Hand','thigh':'UpLeg','shin':'Leg','foot':'Foot'}
    def rename(role):
        if style=='target':return role
        base,_,side=role.partition('_');name=base_names[base]
        if style=='mixamo':return 'mixamorig:'+('Left' if side=='L' else 'Right' if side=='R' else '')+name
        if style=='bip':return 'Bip01 '+(side+' ' if side else '')+('Pelvis' if base=='pelvis' else name)
        return role
    return [{'name':rename(r),'parent':rename(parents[r]) if parents[r] else None,'position':list(positions[r]),
             'matrix':(np.eye(4)+np.array([[0,0,0,positions[r][0]],[0,0,0,positions[r][1]],[0,0,0,positions[r][2]],[0,0,0,0]])).tolist()} for r in ROLES]

def skeleton():
    b=humanoid();names=[x['name'] for x in b]
    return {'m_boneName':names,'m_nParent':[names.index(x['parent']) if x['parent'] else -1 for x in b],
            'm_bonePosParent':[(np.array(x['position'])-(np.array(b[names.index(x['parent'])]['position']) if x['parent'] else 0)).tolist() for x in b],
            'm_boneRotParent':[[0,0,0,1] for x in b],'m_boneScaleParent':[1.]*len(b),'m_nFlag':[0]*len(b)}

def mesh_report():
    return {'rest_alignment':{'policy':'anatomical-rest-alignment-v1'},'meshes':[{'name':'body','positions':[[0,0,1],[1,0,1],[0,0,2]],'normals':[[0,1,0]]*3,
            'uvs':[[[0,0],[1,0],[0,1]]],'colors':[],
            'weights':[[['pelvis',.4],['spine',.6]]]*3,
            'faces':[{'vertices':[0,1,2],'corners':[0,1,2],'material':'models/test/materials/cloth.vmat'}],
            'deformation':[]}], 'materials':{'models/test/materials/cloth.vmat':{'preset':'generic','mode':'generate','confirmed':False,'textures':{},'color':[.2,.4,.8]}},
            'weights':[],'warnings':['Secondary-motion geometry may require manual setup.']}

def surface_points(scale=1.,apose=False):
    parts=[((0,0,.65),(.16,.09,.18)),((0,0,.95),(.09,.09,.08))]
    for sign in [-1,1]:
        parts += [((sign*.12,0,.25),(.055,.06,.25)),((sign*.4,0,.64 if apose else .76),(.22,.045,.045))]
    points=[]
    for (x,y,z),(sx,sy,sz) in parts:
        for a in np.linspace(-1,1,12):
            for b in [-1,1]:
                for c in np.linspace(-1,1,12):points.append([scale*(x+a*sx),scale*(y+b*sy),scale*(z+c*sz)])
    return points
