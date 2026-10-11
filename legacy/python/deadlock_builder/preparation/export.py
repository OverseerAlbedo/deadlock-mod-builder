"""Narrow prepared-DMX exporter using the existing binary writer and validator.

Only streams explicitly collected by the Blender bridge are accepted. No compile
or VPK behavior lives here. Output is experimental until native import validation.
"""
import struct
import uuid
from pathlib import Path
import numpy as np
from ..dmx import Dmx, Element, Attribute
from ..validation import skinning
from ..safety import BuildError, atomic_write
from .target_skeleton import prepare_target
from .dmx_bind import audit_dmx_bind


def export_dmx(report, skeleton, path):
    _, scale_audit = prepare_target(skeleton)  # Also enforce the policy for direct exporters.
    dm=object.__new__(Dmx);dm.version=9;dm.header=b'<!-- dmx encoding binary 9 format model 22 -->\n\0';dm.strings=[];dm.elements=[]
    def string(s):
        if s not in dm.strings:dm.strings.append(s)
        return dm.strings.index(s)
    def element(kind,name):
        string(kind);string(name);index=len(dm.elements)
        dm.elements.append(Element(kind,name,uuid.uuid5(uuid.NAMESPACE_URL,f'dmb-preparation/{index}/{kind}/{name}').bytes_le,[]))
        return index
    def attr(index,name,base,value,array=False):
        string(name);values=value if array else [value]
        raw=struct.pack('<i',len(values)) if array else b''
        for item in values:
            if base==5:raw+=item.encode('utf-8')+b'\0' if array else struct.pack('<i',string(item))
            elif base in (1,2):raw+=struct.pack('<i',item)
            elif base==3:raw+=struct.pack('<f',item)
            elif base==4:raw+=struct.pack('<?',item)
            else:raw+=struct.pack('<'+'f'*len(item),*item)
        dm.elements[index].attributes.append(Attribute(name,base+32 if array else base,value,raw,array))
    def transform(name,pos=(0,0,0),rot=(0,0,0,1),scale=1.0):
        idx=element('DmeTransform',name);attr(idx,'position',10,pos);attr(idx,'orientation',13,rot)
        attr(idx,'scale',3,scale);return idx
    root=element('DmElement','root');model=element('DmeModel','prepared');attr(root,'model',1,model);attr(root,'skeleton',1,model)
    attr(model,'transform',1,transform('prepared'))
    names=skeleton['m_boneName'];joints=[element('DmeJoint',n) for n in names];trans=[]
    for i,idx in enumerate(joints):
        t=transform(names[i],skeleton['m_bonePosParent'][i],skeleton['m_boneRotParent'][i],skeleton['m_boneScaleParent'][i]);trans.append(t);attr(idx,'transform',1,t)
        attr(idx,'children',1,[joints[j] for j,p in enumerate(skeleton['m_nParent']) if p==i],True)
    children=[joints[i] for i,p in enumerate(skeleton['m_nParent']) if p==-1];palette=joints.copy()
    for mesh in report['meshes']:
        positions=mesh['positions'];corners=[i for face in mesh['faces'] for i in face['vertices']]
        corner_ids=[i for face in mesh['faces'] for i in face['corners']]
        if sorted(corner_ids)!=list(range(len(corner_ids))):raise BuildError('Unsupported reused/missing polygon loop indices.')
        count=len(positions)
        for face in mesh['faces']:
            if len(face['vertices'])<3 or any(i<0 or i>=count for i in face['vertices']):raise BuildError('Invalid generated polygon.')
        streams={'position$0':(10,positions,corners),'normal$0':(10,mesh['normals'],corner_ids)}
        if mesh.get('tangents'):streams['tangent$0']=(11,mesh['tangents'],corner_ids)
        for i,uv in enumerate(mesh['uvs']):streams['texcoord$'+str(i)]=(9,uv,corner_ids)
        for i,color in enumerate(mesh['colors']):streams['color$'+str(i)]=(11,color,corner_ids)
        dag=element('DmeDag',mesh['name']);shape=element('DmeMesh',mesh['name']);state=element('DmeVertexData','bind')
        tr=transform(mesh['name']);children.append(dag);palette.append(dag);trans.append(tr)
        attr(dag,'transform',1,tr);attr(dag,'shape',1,shape);attr(dag,'visible',4,True)
        attr(shape,'bindState',1,state);attr(shape,'currentState',1,state);attr(shape,'baseStates',1,[state],True)
        attr(state,'vertexFormat',5,list(streams)+['blendweights$0','blendindices$0'],True)
        attr(state,'flipVCoordinates',4,True)
        for stream,(typ,values,indices) in streams.items():
            if not np.isfinite(np.asarray(values)).all():raise BuildError('Non-finite generated attribute: '+stream)
            if any(i<0 or i>=len(values) for i in indices):raise BuildError('Generated attribute index outside stream.')
            attr(state,stream,typ,values,True);attr(state,stream+'Indices',2,indices,True)
        slots=max(len(row) for row in mesh['weights']);attr(state,'jointCount',2,slots)
        weights=[];indices=[]
        for row in mesh['weights']:
            for n,w in row:weights.append(w);indices.append(names.index(n))
            weights.extend([0.]*(slots-len(row)));indices.extend([0]*(slots-len(row)))
        attr(state,'blendweights$0',3,weights,True);attr(state,'blendindices$0',2,indices,True)
        faces=[]
        # Polygon loops are reindexed in flattened original polygon order.
        offset=0;grouped={}
        for face in mesh['faces']:
            grouped.setdefault(face['material'],[]).extend(list(range(offset,offset+len(face['vertices'])))+[-1]);offset+=len(face['vertices'])
        for material,values in grouped.items():
            mat=element('DmeMaterial',material);attr(mat,'mtlName',5,material)
            fs=element('DmeFaceSet',material);attr(fs,'material',1,mat);attr(fs,'faces',2,values,True);faces.append(fs)
        attr(shape,'faceSets',1,faces,True)
    attr(model,'children',1,children,True);attr(model,'jointList',1,palette,True)
    bind=element('DmeTransformList','bind');attr(bind,'transforms',1,trans,True);attr(model,'baseStates',1,[bind],True)
    raw=dm.rewritten_binary9();parsed=Dmx(raw);skin=skinning(parsed)
    if not skin['numeric_checks_passed']:raise BuildError('Prepared DMX failed the existing numeric skin validator: '+str(skin))
    expected_triangles=sum(sum(len(f['vertices'])-2 for f in m['faces']) for m in report['meshes'])
    if parsed.geometry_counts()['source_triangles']!=expected_triangles:raise BuildError('Prepared topology count changed during DMX export.')
    if parsed.summary()['bones']!=names:raise BuildError('Prepared target bone palette mismatch.')
    bind_audit=audit_dmx_bind(parsed,skeleton,require_explicit_scale=True)
    identity=bind_audit['bind_identity'];scale_audit=bind_audit['target_scale_audit']
    atomic_write(Path(path),raw)
    return {'bind_identity':identity,'target_scale_audit':scale_audit,'target_bind_audit':bind_audit,'skinning':skin,'geometry':parsed.geometry_counts(),'dmx':parsed.summary(),'native_compiler_validated':False}
