"""Test-only Python oracle. Never shipped or invoked by the desktop app."""
import base64, hashlib, json, struct, sys
from pathlib import Path
import numpy as np
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_preview_scene import gltf_fixture
from deadlock_builder.preview import load_scene
from deadlock_builder.preview.scene import trs
from PIL import Image
OUT=Path(__file__).with_name('fixtures')/'gltf'
OUT.mkdir(exist_ok=True)

def glb(doc,data,path):
    doc['buffers']=[{'byteLength':len(data)}]
    raw=json.dumps(doc,separators=(',',':')).encode(); raw+=b' '*(-len(raw)%4)
    data=bytes(data)+b'\0'*(-len(data)%4)
    chunks=struct.pack('<II',len(raw),0x4e4f534a)+raw+struct.pack('<II',len(data),0x004e4942)+data
    path.write_bytes(b'glTF'+struct.pack('<II',2,len(chunks)+12)+chunks)

def expected(path):
    s=load_scene(path)
    return {'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'mesh_count':len(s.meshes),'vertex_count':sum(len(m.positions) for m in s.meshes),'triangle_count':sum(len(m.triangles) for m in s.meshes),'bone_count':len(set(int(j) for sk in s.skins for j in sk.joints)), 'animations':[{'name':a.name,'duration':a.duration} for a in s.animations], 'poses':[{'time':t,'positions':[m.positions.tolist() for m in s.pose(t,0 if s.animations else None).meshes], 'normals':[m.normals.tolist() for m in s.pose(t,0 if s.animations else None).meshes]} for t in [0,.25,.5,1.]]}

gltf_fixture(OUT/'basic.gltf'); gltf_fixture(OUT/'basic.glb',binary=True)
doc=gltf_fixture(OUT/'external.gltf',external=True)
Image.new('RGBA',(2,2),(100,140,200,230)).save(OUT/'cloth.png')
doc['images']=[{'uri':'cloth.png'}];doc['textures']=[{'source':0}]
doc['materials']=[{'name':'Cloth','pbrMetallicRoughness':{'baseColorTexture':{'index':0},'baseColorFactor':[.4,.6,.8,1.], 'metallicFactor':.2,'roughnessFactor':.7}}]
doc['meshes'][0]['primitives'][0]['material']=0
(OUT/'external.gltf').write_text(json.dumps(doc))
# Eight nonzero influences, nonuniform scaling, morphs and animation. Rest
# inverse binds are serialized f32 exactly, as in real exported glTF.
data=bytearray();d={'asset':{'version':'2.0'},'bufferViews':[],'accessors':[]}
def acc(values,kind='VEC3',dtype='<f4',normalized=False):
    a=np.asarray(values,dtype=dtype);data.extend(b'\0'*(-len(data)%4));offset=len(data);raw=a.tobytes();data.extend(raw)
    d['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':len(raw)})
    v={'bufferView':len(d['bufferViews'])-1,'count':len(a),'type':kind,'componentType':{'<f4':5126,'<u2':5123,'u1':5121}[dtype]}
    if normalized:v['normalized']=True
    d['accessors'].append(v);return len(d['accessors'])-1
positions=acc([[0,0,0],[1,0,0],[0,1,0]]);normals=acc([[1/np.sqrt(3)]*3]*3)
j0=acc([[0,1,2,3]]*3,'VEC4','<u2');j1=acc([[4,5,6,7]]*3,'VEC4','<u2');w0=acc([[.125]*4]*3,'VEC4');w1=acc([[.125]*4]*3,'VEC4')
nodes=[{'name':'Eight','mesh':0,'skin':0}];world=[]
for i in range(8):
    t=[i*.2,i*.1,0];q=[0,0,np.sin(i*.04),np.cos(i*.04)];sc=[1+i*.03,1-i*.02,1+i*.01]
    nodes.append({'name':f'Joint{i}','translation':t,'rotation':q,'scale':sc});world.append(trs(t,q,sc))
ibm=acc(np.linalg.inv(world).transpose(0,2,1).reshape(8,16),'MAT4')
morph=acc([[0,0,0],[0,0,.2],[0,0,1]])
d.update(nodes=nodes,scenes=[{'nodes':list(range(9))}],scene=0,skins=[{'joints':list(range(1,9)),'inverseBindMatrices':ibm}],meshes=[{'name':'Eight','weights':[0.], 'extras':{'targetNames':['Smile']},'primitives':[{'attributes':{'POSITION':positions,'NORMAL':normals,'JOINTS_0':j0,'WEIGHTS_0':w0,'JOINTS_1':j1,'WEIGHTS_1':w1},'targets':[{'POSITION':morph}]}]}])
times=acc([0,1],'SCALAR');move=acc([[0,0,0],[2,0,0]]);scale=acc([[1,1,1],[2,3,4]]);weights=acc([0,1],'SCALAR')
d['animations']=[{'name':'EightMove','samplers':[{'input':times,'output':move},{'input':times,'output':scale},{'input':times,'output':weights}], 'channels':[{'sampler':0,'target':{'node':1,'path':'translation'}},{'sampler':1,'target':{'node':1,'path':'scale'}},{'sampler':2,'target':{'node':0,'path':'weights'}}]}]
glb(d,data,OUT/'eight.glb')
# STEP and cubic fixtures preserve all legacy interpolation choices.
for interpolation in ['STEP','CUBICSPLINE']:
    dd=json.loads(json.dumps(d));bb=bytearray(data)
    if interpolation=='CUBICSPLINE':
        for samp in dd['animations'][0]['samplers']:
            src=dd['accessors'][samp['output']];view=dd['bufferViews'][src['bufferView']];width={'SCALAR':1,'VEC3':3}[src['type']]
            values=np.frombuffer(bytes(data[view['byteOffset']:view['byteOffset']+view['byteLength']]),dtype='<f4').reshape(2,width)
            cubic=np.stack([np.zeros_like(values),values,np.zeros_like(values)],axis=1).astype('<f4');bb.extend(b'\0'*(-len(bb)%4));view['byteOffset']=len(bb);view['byteLength']=cubic.nbytes;bb.extend(cubic.tobytes());src['count']=6
    for samp in dd['animations'][0]['samplers']:samp['interpolation']=interpolation
    glb(dd,bb,OUT/(interpolation.lower()+'.glb'))
# Interleaved POSITION with sparse replacement and normalized UV0.
doc=gltf_fixture(OUT/'sparse.gltf');data=bytearray(base64.b64decode(doc['buffers'][0]['uri'].split(',')[1]))
def view(raw,stride=None):
    data.extend(b'\0'*(-len(data)%4));at=len(data);data.extend(raw);v={'buffer':0,'byteOffset':at,'byteLength':len(raw)}
    if stride:v['byteStride']=stride
    doc['bufferViews'].append(v);return len(doc['bufferViews'])-1
posview=view(struct.pack('<12f',0,0,0,99,1,0,0,99,9,9,9,99),16);iv=view(bytes([2]));vv=view(struct.pack('<3f',0,1,0))
doc['accessors'][0].update(bufferView=posview,sparse={'count':1,'indices':{'bufferView':iv,'componentType':5121},'values':{'bufferView':vv}})
doc['accessors'][2].update(bufferView=view(bytes([0,0,255,0,0,255])),componentType=5121,normalized=True)
doc['buffers']=[{'byteLength':len(data),'uri':'data:application/octet-stream;base64,'+base64.b64encode(data).decode()}]
(OUT/'sparse.gltf').write_text(json.dumps(doc))
# Loader edge cases: generated normals, absent morph deltas, signed positive
# indices and sparse MAT4 accessors must retain the legacy decoded scene.
for variant in ['generated_normals','missing_morph','signed_indices','sparse_matrix']:
    path=OUT/(variant+'.gltf');doc=gltf_fixture(path)
    primitive=doc['meshes'][0]['primitives'][0]
    data=bytearray(base64.b64decode(doc['buffers'][0]['uri'].split(',')[1]))
    if variant=='generated_normals': primitive['attributes'].pop('NORMAL')
    if variant=='missing_morph':
        primitive['targets'].append({});doc['meshes'][0]['weights']=[.25,.5]
        doc['meshes'][0]['extras']['targetNames']=['Smile','Empty']
        doc['animations'][0]['channels']=doc['animations'][0]['channels'][:1]
    if variant=='signed_indices':
        data.extend(b'\0'*(-len(data)%4));offset=len(data);data.extend(struct.pack('<3h',0,1,2))
        doc['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':6})
        doc['accessors'].append({'bufferView':len(doc['bufferViews'])-1,'componentType':5122,'type':'SCALAR','count':3})
        primitive['indices']=len(doc['accessors'])-1
    if variant=='sparse_matrix':
        data.extend(b'\0'*(-len(data)%4));offset=len(data);data.extend(bytes([0,1]));values=doc['accessors'][5].pop('bufferView')
        doc['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':2})
        doc['accessors'][5]['sparse']={'count':2,'indices':{'bufferView':len(doc['bufferViews'])-1,'componentType':5121},'values':{'bufferView':values}}
    doc['buffers']=[{'byteLength':len(data),'uri':'data:application/octet-stream;base64,'+base64.b64encode(data).decode()}]
    path.write_text(json.dumps(doc))
results={p.name:expected(p) for p in OUT.iterdir() if p.suffix in ['.gltf','.glb']}
(OUT/'oracle.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps({name:{k:v for k,v in record.items() if k!='poses'} for name,record in results.items()},indent=2))
