"""Development-only Python reference exporter; Rust runtime never executes this."""
from pathlib import Path
import hashlib
import json
import random
import struct
import sys

ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder import compiled_kv3 as kv
from deadlock_builder._vendor.sourceio_kv3 import types as t
from deadlock_builder._vendor.sourceio_kv3.enums import Specifier
from deadlock_builder.vpk import resource_blocks
from deadlock_builder.kv3 import Document,field_edit
from tests.test_compiled_kv3_compression import v5_blob_array,GUID,FOOTER
import lz4.block
import numpy as np
import zstandard

DEST=Path(__file__).parent/'fixtures';DEST.mkdir(exist_ok=True)
def save(name,data):
    decoded=kv.decode(data)
    (DEST/(name+'.kv3')).write_bytes(data)
    encoded=kv.encode(decoded,GUID)
    (DEST/(name+'.v1')).write_bytes(encoded)
    return {'name':name,'sha256':hashlib.sha256(data).hexdigest(),'encoded_sha256':hashlib.sha256(encoded).hexdigest()}

resource=t.String('materials/é/body.vmat');resource.specifier=Specifier.RESOURCE
subclass=t.String('example');subclass.specifier=Specifier.SUBCLASS
value=t.Object({'null':None,'bool':t.Bool(True),'false':t.Bool(False),
 'negative':t.Int64(-(2**63)),'unsigned':t.UInt64(2**64-1),'float32':t.Float(float(np.float32(.1))),
 'negative_zero':t.Double(-0.0),'tiny':t.Double(float.fromhex('0x0.0000000000001p-1022')),
 'resource':resource,'subclass':subclass,'blob':t.BinaryBlob(b'\x00\xff\x0a'),
 'array':t.Array([t.Int64(1),t.String('with space'),t.Object({'z':t.Double(3.25)})])})
raw=kv.encode(value,GUID)
rows=[save('all-values',raw),save('all-values-lz4',raw[:20]+struct.pack('<I',1)+raw[24:40]+lz4.block.compress(raw[40:],store_size=False))]
seed=random.Random(42).randbytes(3000);chunks=[seed,seed,seed[:900]];joined=b''.join(chunks)
blobs=[joined[:1100],b'',joined[1100:4200],joined[4200:]]
chain=v5_blob_array(blobs,chunks);rows.append(save('v5-chained-blobs',chain))

# Minimal independently laid out v5 array with an f32 value and both short types.
def v5_scalars(method):
    # buffer0: strings byte lane then aligned one int string-table count.
    zero=struct.pack('<I',0)
    # buffer1: shorts[0]=-2, shorts[1]=65535; ints[0]=array count, ints[1]=float32 bits.
    one=struct.pack('<hHIf',-2,65535,3,.1)+bytes([8,20,21,19])+FOOTER
    if method==0: pack0,pack1=zero,one
    elif method==1: pack0,pack1=[lz4.block.compress(b,store_size=False) for b in (zero,one)]
    else: pack0,pack1=[zstandard.ZstdCompressor(write_checksum=True).compress(b) for b in (zero,one)]
    header=b'\x053VK'+GUID+struct.pack('<IHH',method,0,16384 if method==1 else 0)
    header+=struct.pack('<3I',0,1,0)+struct.pack('<IHH',4,0,1)
    header+=struct.pack('<6I',len(zero)+len(one),len(pack0)+len(pack1),0,0,0,0)
    header+=struct.pack('<4I',len(zero),len(pack0),len(one),len(pack1))
    header+=struct.pack('<4I',0,2,2,0)+bytes(16)
    return header+pack0+pack1
for method in (0,1,2):rows.append(save('v5-scalars-'+str(method),v5_scalars(method)))

source='''<!-- kv3 encoding:text:version{1} format:modeldoc28:version{2} -->\n{
  // Keep this comment and spacing exactly.
  root = { _class = "RootNode" children = [
    { _class = "RenderMeshFile" name = "étoile" filename = "old.dmx" opaque = strange:token }
  ] blob = #[ 00ff 0a ] text = """line with spaces  \nnext""" }
}\n'''
doc=Document(source);node=doc.objects('RenderMeshFile')[0]
expected=doc.apply([field_edit(node,'filename','新しい model.dmx'),field_edit(node,'added','value')])
(DEST/'lossless.vmdl').write_text(source,encoding='utf-8')
(DEST/'lossless-edited.vmdl').write_text(expected,encoding='utf-8')

native=[]
for path in [ROOT/'physics-references/Vindicta-R12/cloth_supported_hair.vmdl_c',ROOT/'physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c']:
    raw=path.read_bytes();items=[]
    for index,block in enumerate(resource_blocks(raw)['blocks']):
        if block['name'] not in ('DATA','PHYS','CTRL','MDAT','AGRP'):continue
        data=raw[block['offset']:block['offset']+block['size']]
        # Every supported ordinal is an independent golden, including repeated MDAT.
        decoded=kv.decode(data);encoded=kv.encode(decoded,data[4:20])
        items.append({'ordinal':index,'name':block['name'],'version':data[0],
                      'sha256':hashlib.sha256(data).hexdigest(),'encoded_sha256':hashlib.sha256(encoded).hexdigest()})
    native.append({'path':path.relative_to(ROOT).as_posix(),'sha256':hashlib.sha256(raw).hexdigest(),'blocks':items})
(DEST/'manifest.json').write_text(json.dumps({'fixtures':rows,'native':native},indent=2)+'\n')
print(json.dumps({'synthetic_fixtures':len(rows),'native_resources':len(native),'native_blocks':sum(len(x['blocks']) for x in native)}))
