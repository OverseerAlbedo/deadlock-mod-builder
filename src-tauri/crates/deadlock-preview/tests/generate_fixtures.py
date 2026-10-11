"""Reference outputs from the existing Python preview; never a runtime dependency."""
import copy
import json
from pathlib import Path
import struct
import sys
root=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(root))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.preparation.export import export_dmx
from deadlock_builder.preview.dmx import load_dmx
from deadlock_builder.dmx import Dmx
from tests.preparation_fixture import mesh_report,skeleton
out=Path(__file__).parent/'fixtures'
for name in ('basic','scaled','translated','unskinned','generated_normals','concave'):
    report,target=mesh_report(),skeleton()
    if name=='scaled':
        target['m_boneScaleParent'][0]=1.0002
        target['m_boneScaleParent'][1]=.9998
        target['m_boneRotParent'][0]=[0,0,0.7071067811865476,0.7071067811865476]
    if name=='concave':
        mesh=report['meshes'][0]
        mesh['positions']=[[0,0,1],[2,0,1],[2,2,1],[1,1,1],[0,2,1]]
        mesh['normals']=[[0,0,1]]*5
        mesh['uvs']=[[[0,0],[1,0],[1,1],[.5,.5],[0,1]]]
        mesh['weights']=[[['pelvis',.4],['spine',.6]]]*5
        mesh['faces'][0].update(vertices=list(range(5)),corners=list(range(5)))
    path=out/(name+'.dmx');export_dmx(report,target,path)
    doc=Dmx.load(path)
    if name in ('translated','unskinned'):
        dag=next(e for e in doc.elements if e.type=='DmeDag')
        tr=doc.elements[dag.get('transform')]
        attr=next(a for a in tr.attributes if a.name=='position')
        attr.raw=attr.value=struct.pack('<3f',5.,-2.,3.)
        if name=='unskinned':
            for elem in doc.elements:
                if elem.type=='DmeVertexData':
                    elem.attributes=[a for a in elem.attributes if a.name not in ('jointWeights','jointIndices','jointCount','blendweights$0','blendindices$0')]
    if name=='generated_normals':
        for elem in doc.elements:
            if elem.type=='DmeVertexData':elem.attributes=[a for a in elem.attributes if a.name not in ('normal$0','normal$0Indices','normals','normalsIndices')]
    path.write_bytes(doc.rewritten_binary9())
    scene=load_dmx(path);pose=scene.pose()
    data={'meshes':[{'name':m.name,'source_name':m.source_name,'material':m.material.name,'positions':m.positions.tolist(),'normals':m.normals.tolist(),'indices':m.triangles.flatten().tolist(),'uvs':m.uvs.tolist() if m.uvs is not None else None} for m in pose.meshes],
          'bones':[{'name':n,'position':p.tolist(),'parent':int(parent)} for n,p,parent in zip(pose.bone_names,pose.bone_positions,pose.bone_parents)]}
    (out/(name+'.json')).write_text(json.dumps(data,indent=2))
