"""Development-only golden generator. Normal Rust operation never invokes Python.

Run from the existing reference implementation root. Inputs come from independent
test encoders; expected summaries/extractions come from the preserved Python codec.
"""
from pathlib import Path
import json
import struct
import sys

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.fixtures import dmx
from tests.split_fixture import skinned
from deadlock_builder.dmx import Dmx

OUT = Path(__file__).parent / 'fixtures'
OUT.mkdir(exist_ok=True)

def older(raw, version):
    source = Dmx(raw)
    pool = {value: index for index, value in enumerate(source.strings)}
    idx = lambda value: struct.pack('<H' if version == 4 else '<i', pool.get(value, 65535 if version == 4 else -1))
    data = bytearray(f'<!-- dmx encoding binary {version} format model 22 -->\n\0'.encode())
    data += struct.pack('<i', len(source.strings))
    data += b''.join(s.encode() + b'\0' for s in source.strings)
    data += struct.pack('<i', len(source.elements))
    for e in source.elements:
        data += idx(e.type) + idx(e.name) + e.uuid
    for e in source.elements:
        data += struct.pack('<i', len(e.attributes))
        for a in e.attributes:
            data += idx(a.name) + bytes([a.type - 18 if a.array else a.type])
            data += idx(a.value) if a.type == 5 else a.raw
    return bytes(data)

inputs = {'basic-v9': dmx(), 'unicode-v9': dmx(('衣装', 'оружие'), ('模型/материал.vmat',)), 'split-v9': skinned()}
inputs.update({f'basic-v{v}': older(inputs['basic-v9'], v) for v in (4,5)})
for name, raw in inputs.items():
    source = Dmx(raw)
    (OUT / f'{name}.dmx').write_bytes(raw)
    (OUT / f'{name}.json').write_text(json.dumps({'summary': source.summary(), 'geometry': source.geometry_counts()}, ensure_ascii=False, indent=2)+'\n', encoding='utf8')
    for index, mesh in enumerate(source.summary()['meshes']):
        (OUT / f'{name}-mesh-{index}.dmx').write_bytes(source.single_mesh(mesh['name']))

# A separate independent encoder exercises every supported scalar/array wire type.
sizes = {1:4,2:4,3:4,4:1,7:4,8:4,9:8,10:12,11:16,12:12,13:16,14:64,15:8,16:1}
pool = ['DmElement', 'root', '文字列'] + [f'{kind}_{base}' for kind in ('scalar','array') for base in range(1,17)]
index = lambda value: struct.pack('<i', pool.index(value))
data = bytearray(b'<!-- dmx encoding binary 9 format model 22 -->\n\0' + struct.pack('<ii',0,len(pool)))
data += b''.join(s.encode()+b'\0' for s in pool) + struct.pack('<i',1) + index('DmElement')+index('root')+bytes(16)+struct.pack('<i',32)
for kind in ('scalar','array'):
    for base in range(1,17):
        data += index(f'{kind}_{base}') + bytes([base + (32 if kind=='array' else 0)])
        if kind=='array': data += struct.pack('<i',2)
        for _ in range(2 if kind=='array' else 1):
            if base==1: data += struct.pack('<i',-1)
            elif base==2 or base==7: data += struct.pack('<i',-123)
            elif base==3: data += struct.pack('<f',-0.125)
            elif base==4: data += b'\x01'
            elif base==5: data += '文字列'.encode()+b'\0' if kind=='array' else index('文字列')
            elif base==6: data += struct.pack('<i',3)+b'\x00\x80\xff'
            else: data += bytes((i%255 for i in range(sizes[base])))
raw = bytes(data)
Dmx(raw)
(OUT/'all-types-v9.dmx').write_bytes(raw)

source = Dmx(inputs['split-v9'])
changed = source.rewritten_binary9(names={4:'衣装-piece',5:'衣装-piece',10:'衣装-piece'}, identities={5:bytes(range(16))})
(OUT/'split-v9-rewritten.dmx').write_bytes(changed)

# Split golden files are produced by the reference's complete independent
# partition/write/validate pipeline, never by the Rust implementation under test.
import tempfile
import shutil
from deadlock_builder.dmx_split import Layout, split_file

def loose_source(count, triangles=1):
    source=Dmx(skinned());layout=Layout(source,'body')
    positions=b''.join(layout.attrs['position$0'].value[:5])
    positions+=b''.join(struct.pack('<3f',20+i,30+i,40+i) for i in range(count))
    weights=b''.join(struct.pack('<2f',.125 if i%2 else .75,.875 if i%2 else .25) for i in range(5+count))
    indices=struct.pack('<'+'i'*(2*(5+count)),*([0,1]*(5+count)))
    faces=[4,5,6,-1]*triangles
    return source.rewritten_binary9({(layout.state_index,'position$0'):struct.pack('<i',5+count)+positions,
        (layout.state_index,'blendweights$0'):struct.pack('<i',2*(5+count))+weights,
        (layout.state_index,'blendindices$0'):struct.pack('<i',2*(5+count))+indices,
        (layout.face_ids[1],'faces'):struct.pack('<i',len(faces))+struct.pack('<'+'i'*len(faces),*faces)})

variants={'normal-6':(skinned(),6,'body'), 'normal-9':(skinned(),9,'body'),
          'loose-many':(loose_source(10,4),6,'body'), 'loose-none':(loose_source(0),9,'body'),
          'unicode':(Dmx(skinned()).rewritten_binary9(names={4:'衣装😀',5:'衣装😀',10:'衣装😀'}),6,'衣装😀')}
for name,(raw,target,mesh) in variants.items():
    folder=OUT/'split'/name;folder.mkdir(parents=True,exist_ok=True)
    (folder/'source.dmx').write_bytes(raw)
    with tempfile.TemporaryDirectory() as temp:
        source=Path(temp)/'source.dmx';source.write_bytes(raw)
        report=split_file(source,[mesh],Path(temp)/'generated',target)
        directory=Path(report.pop('directory'));report.pop('source_file')
        for file in directory.iterdir():
            if file.name!='split-report.json':shutil.copy2(file,folder/file.name)
        (folder/'expected.json').write_text(json.dumps({'report':report,'cache_key':directory.name},ensure_ascii=False,indent=2)+'\n',encoding='utf8')
