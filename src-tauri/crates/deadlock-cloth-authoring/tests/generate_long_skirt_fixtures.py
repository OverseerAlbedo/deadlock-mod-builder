"""Independent sewn panels, an open slit, a branch, and non-skirt controls."""
from pathlib import Path
import json
import sys

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.preparation.export import export_dmx

out = Path(__file__).parent / 'fixtures/long_skirt'
out.mkdir(exist_ok=True)
names, parents, positions = ['pelvis', 'head'], [-1, 0], [[0, 0, 0], [0, 24, 0]]
world = {'pelvis': [0, 0, 0], 'head': [0, 24, 0]}
roots, meshes, columns = [], [], {}

def column(root, x, z=4, top=0, step=6, anchor='pelvis'):
    roots.append(root)
    bones = []
    for level in range(5):
        n = 'dmb_secondary_' + (root if level == 0 else root + '_' + str(level))
        parent = bones[-1] if bones else anchor
        p = [x, top - level * step, z]
        names.append(n); parents.append(names.index(parent))
        positions.append([a-b for a,b in zip(p, world[parent])]); world[n] = p
        bones.append(n)
    columns[root] = bones
    return bones

def panel(label, a, b):
    verts, weights, faces = [], [], []
    for level in range(5):
        for bone in [a[level], b[level]]:
            verts.append(world[bone]); weights.append([[bone, 1.]])
    for row in range(4):
        face = [2*row, 2*row+1, 2*row+3, 2*row+2]
        # Split into triangles exactly as a real rendered surface would be.
        for tri in [face[:3], [face[0], face[2], face[3]]]:
            faces.append({'vertices': tri, 'corners': tri, 'material': 'materials/fabric.vmat'})
    # Intermediate samples ensure the geometry gate uses visible fabric and
    # enough supporting triangles rather than an artificial tiny triangle.
    for level,bone in enumerate(a+b):
        base = len(verts); p=world[bone]
        verts += [p, [p[0]+.02,p[1]+.02,p[2]], [p[0]-.02,p[1]-.02,p[2]]]
        weights += [[[bone,1.]]]*3
        # Non-collinear satellite points, attached with existing panel weights.
        verts[-1] = [p[0]-.02,p[1]+.02,p[2]]
        faces.append({'vertices':[base,base+1,base+2], 'corners':[base,base+1,base+2], 'material':'materials/fabric.vmat'})
    meshes.append({'name': label, 'positions':verts, 'normals':[[0,0,1]]*len(verts),
        'uvs':[[[0,0]]*len(verts)],'colors':[],'weights':weights,'faces':faces,'deformation':[]})

# The names give no garment semantics. Two separate panels sit close to one
# another, but the source has no face across the middle slit.
for i,x in enumerate([-6,-2,2,6]): column('spring'+str(i),x)
panel('left_panel',columns['spring0'],columns['spring1'])
panel('right_panel',columns['spring2'],columns['spring3'])
# A small branch on the first column must not prevent the main garment joining.
branch='dmb_secondary_side_branch';parent=columns['spring0'][2]
names.append(branch);parents.append(names.index(parent));positions.append([-.5,0,.5]);world[branch]=[-6.5,-12,4.5]
m=meshes[0];base=len(m['positions']);p=world[branch]
m['positions'] += [p,[p[0]+.1,p[1],p[2]],[p[0],p[1]+.1,p[2]]]
m['normals'] += [[0,0,1]]*3;m['uvs'][0] += [[0,0]]*3;m['weights'] += [[[branch,1.]]]*3
m['faces'].append({'vertices':[base,base+1,base+2],'corners':[base,base+1,base+2],'material':'materials/fabric.vmat'})
for label,z,top,step,anchor in [('skirt_short',10,0,2,'pelvis'),('cape',14,12,8,'pelvis'),
        ('hair',18,24,12,'head'),('tail',22,0,6,'pelvis'),('ornament',26,0,6,'pelvis')]:
    a=column(label+'_a',-2,z,top,step,anchor);b=column(label+'_b',2,z,top,step,anchor)
    panel(label,a,b)
# A detached long garment column is not sewn to another just because it is near.
a=column('skirt_detached',8,4)
panel('detached',a,a)
skeleton={'m_boneName':names,'m_nParent':parents,'m_bonePosParent':positions,
    'm_boneRotParent':[[0,0,0,1]]*len(names),'m_boneScaleParent':[1.]*len(names),'m_nFlag':[0]*len(names)}
for mesh in meshes:
    offset=0
    for face in mesh['faces']:
        face['corners']=list(range(offset,offset+len(face['vertices'])))
        offset += len(face['vertices'])
    mesh['normals']=[[0,0,1]]*offset
    mesh['uvs']=[[[0,0]]*offset]
export_dmx({'meshes':meshes,'materials':{}},skeleton,out/'panels.dmx')
(out/'oracle.json').write_text(json.dumps({'roots':roots,'long_skirts':['spring0','spring1','spring2','spring3'],
    'connected_pairs':[['spring0','spring1'],['spring2','spring3']]},indent=2)+'\n')
