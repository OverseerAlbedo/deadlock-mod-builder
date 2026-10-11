"""Development-only oracle; no Python dependency in Rust application runtime."""
from pathlib import Path
import copy,json,math,sys
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import skeleton
from tests.test_compiled_bone_aliases import fixture,resource
from tests.test_skeleton_extensions import extension_fixture,authority
from deadlock_builder.preparation.target_skeleton import prepare_target
from deadlock_builder.skeleton_extensions import extension_identity,_source_binding
from deadlock_builder.bone_validation import skeleton_identity
from deadlock_builder.compiled_extension_bindings import audit_bindings
from deadlock_builder import motion_restore as restore,compiled_kv3 as kv
OUT=Path(__file__).parent/'fixtures';OUT.mkdir(exist_ok=True)
cases=[]
for value in (1.,1.000075101852417,1.01,.99,2.,.5):
 s=skeleton();s['m_boneScaleParent'][0]=value;bones,audit=prepare_target(s)
 cases.append({'skeleton':s,'bones':bones,'scale_audit':audit})
q=math.sqrt(.5)
s={'m_boneName':['root','child','tip'],'m_nParent':[-1,0,1],'m_nFlag':[0,0,0],
   'm_bonePosParent':[[3.,4.,5.],[1.,0.,0.],[0.,2.,0.]],'m_boneRotParent':[[0.,0.,q,q],[0.,0.,0.,1.],[0.,0.,0.,1.]],'m_boneScaleParent':[2.,.5,1.]}
bones,audit=prepare_target(s);cases.append({'skeleton':s,'bones':bones,'scale_audit':audit})
s=skeleton();count=len(s['m_boneName']);s['m_boneScaleParent']=[1.00009]*count;s['m_nParent']=[i-1 for i in range(count)];s['m_bonePosParent']=[[1000.,0.,0.]]*count
bones,audit=prepare_target(s);cases.append({'skeleton':s,'bones':bones,'scale_audit':audit})
(OUT/'transforms.json').write_text(json.dumps(cases,indent=2)+'\n')

def bindings(raw):
 blocks=restore.container(raw)
 read=lambda name:[kv.normal(kv.decode(restore.payload(raw,b))) for b in blocks if b['name']==name]
 return audit_bindings(read('DATA')[0],read('CTRL')[0],read('MDAT'),read('AGRP'),blocks)

for name,blocks in [('canonical',fixture(canonical=True)),('export-alias',fixture()),('reordered',fixture(canonical=True,order=(2,0,1)))]:
 raw=resource(blocks);(OUT/f'{name}.vmdl_c').write_bytes(raw)
 (OUT/f'{name}-bindings.json').write_text(json.dumps(bindings(raw),indent=2)+'\n')
original,candidate=extension_fixture();raw=resource(candidate);original_raw=resource(original)
(OUT/'extension.vmdl_c').write_bytes(raw)
(OUT/'extension-original.vmdl_c').write_bytes(original_raw)
s=authority(original_raw,candidate)
report={'identity':extension_identity(candidate[0][1]['m_modelSkeleton'],original[0][1]['m_modelSkeleton']),
        'source':s,'source_binding':_source_binding(candidate[0][1]['m_modelSkeleton'],original[0][1]['m_modelSkeleton'],s,s['original_sha256']),
        'bindings':bindings(raw)}
(OUT/'extension.json').write_text(json.dumps(report,indent=2)+'\n')

for label,path in [('r12-original','physics-references/Vindicta-R12/cloth_supported_hair.vmdl_c'),('r12-balanced','physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c')]:
 (OUT/f'{label}-bindings.json').write_text(json.dumps(bindings((ROOT/path).read_bytes()),indent=2)+'\n')
from deadlock_builder.compiled_bone_aliases import repair_aliases
for label,blocks in [('alias',fixture()),('alias-reordered',fixture(order=(2,0,1)))]:
 original=resource(fixture(canonical=True));candidate=resource(blocks)
 fixed,receipt=repair_aliases(original,candidate)
 (OUT/f'{label}-input.vmdl_c').write_bytes(candidate)
 (OUT/f'{label}-expected.vmdl_c').write_bytes(fixed)
 (OUT/f'{label}-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
from tests.preparation_fixture import mesh_report
from deadlock_builder.preparation.export import export_dmx
from deadlock_builder.source_identity import inspect_identity,preflight
from deadlock_builder.validation import skinning
from deadlock_builder.dmx import Dmx
for label in ['prepared','prepared-streams','prepared-scale']:
 s=skeleton();r=mesh_report()
 if label=='prepared-scale':s['m_boneScaleParent'][0]=1.000075101852417
 if label=='prepared-streams':
  m=r['meshes'][0];m['name']='衣服_🧵';m['colors']=[[[1.,0.,0.,1.],[0.,1.,0.,1.],[0.,0.,1.,1.]]];m['tangents']=[[1.,0.,0.,-1.]]*3;m['uvs'].append([[.5,0.],[1.,.5],[.5,1.]])
 path=OUT/f'{label}.dmx';audit=export_dmx(r,s,path)
 (OUT/f'{label}.json').write_text(json.dumps({'skeleton':s,'input':r,'audit':audit},indent=2)+'\n')
from tests.test_source_identity import original,write_selected_weights
from tests.test_extension_source import extended_chain,keep_template
from deadlock_builder.extension_source import source_contract,template_contract
from deadlock_builder.preparation.target_skeleton import SCALE_POLICY
from deadlock_builder.bone_validation import source_bones
from deadlock_builder.safety import BuildError,digest
import struct
for label in ['extension-source','extension-loose','extension-unselected','extension-zero','extension-implicit','extension-alias']:
 path=OUT/f'{label}.dmx';s=extended_chain();ref=original()
 if label=='extension-loose':
  r=mesh_report();m=r['meshes'][0];m['positions'].append([20,20,20]);m['weights']=[[['root',1.]]]*3+[[['hem',1.]]];export_dmx(r,s,path)
 elif label=='extension-zero':write_selected_weights(path,s,[['root',1.],['hem',0.]])
 elif label=='extension-unselected':write_selected_weights(path,s,[['root',1.]],[['hem',1.]])
 else:write_selected_weights(path,s,[['hem',1.]])
 d=Dmx.load(path)
 if label=='extension-implicit':
  for e in d.elements:
   if e.type=='DmeTransform':e.attributes[:]=[a for a in e.attributes if a.name!='scale']
  path.write_bytes(d.rewritten_binary9());d=Dmx.load(path)
 if label=='extension-alias':
  ref['runtime_motion']['skeleton']['m_boneName'][1]='$braid'
  i=next(i for i,e in enumerate(d.elements)if e.type=='DmeJoint'and e.name=='braid')
  path.write_bytes(d.rewritten_binary9(names={i:'_braid'}));d=Dmx.load(path)
 contract=source_contract(d,ref,['body'],digest(path));out={'original':ref,'selected':['body'],'inspection':inspect_identity(d,ref),'contract':contract,'skinning':skinning(d)}
 if contract:
  template=keep_template(ref['runtime_motion']['skeleton']);out['required']={n:ref['runtime_motion']['skeleton']['m_boneName'][p]if p>=0 else None for n,p in zip(ref['runtime_motion']['skeleton']['m_boneName'],ref['runtime_motion']['skeleton']['m_nParent'])}
  out['template_audit']=template_contract(d,template,ref,contract)
  if label=='extension-source':
   reference={'sha256':ref['compiled_sha256'],'skeleton':ref['runtime_motion']['skeleton'],'scale_policy':SCALE_POLICY};rp=OUT/'extension-reference.json';rp.write_text(json.dumps(reference));out['marked_template_audit']=template_contract(d,template,ref,contract,str(rp))
 (OUT/f'{label}.json').write_text(json.dumps(out,indent=2)+'\n')

alias_bad=[]
for label in ['unknown-value','unknown-key','unknown-blob','unknown-hash','unknown-block','bad-remap','bad-parent','bad-control','bad-scale']:
 blocks=fixture()
 if label=='unknown-value':blocks[0][1]['unknown_binding']='_cloth_a'
 if label=='unknown-key':blocks[0][1]['_cloth_a']=12
 if label=='unknown-blob':blocks[5][1]['m_segments']=[b'\0_cloth_a\0']
 if label=='unknown-hash':blocks[4][1]['m_decodeKey']['m_boneNameHashes']=[11,22,33]
 if label=='unknown-block':blocks.append(('ZZZZ',{'m_boneName':'_cloth_a'}))
 if label=='bad-remap':blocks[0][1]['m_remappingTable'][2]=999
 if label=='bad-parent':blocks[3][1]['m_skeleton']['m_bones'][1]['m_parentName']='_cloth_a'
 if label=='bad-control':blocks[1][1]['embedded_meshes'][1]['m_nDataBlock']=2
 if label=='bad-scale':blocks[0][1]['m_modelSkeleton']['m_boneScaleParent'][1]+=.001
 raw=resource(blocks)
 try:repair_aliases(resource(fixture(canonical=True)),raw);raise RuntimeError(label+' unexpectedly passed')
 except BuildError:pass
 (OUT/f'alias-bad-{label}.vmdl_c').write_bytes(raw);alias_bad.append(label)
(OUT/'alias-bad.json').write_text(json.dumps(alias_bad))
# Independent malformed bind corpus is emitted by the Python writer, then rejected by both gates.
bind_cases=[]
s=skeleton();base=Dmx((OUT/'prepared.dmx').read_bytes());joint=next(e for e in base.elements if e.type=='DmeJoint');tr=joint.get('transform')
for label in ['missing-scale','changed-scale','changed-position','moved-mesh','root-away','bind-away','mesh-under-joint','duplicate-palette','shared-owner']:
 d=Dmx(base.data);edits={}
 if label=='missing-scale':d.elements[tr].attributes[:]=[a for a in d.elements[tr].attributes if a.name!='scale']
 if label=='changed-scale':edits[(tr,'scale')]=struct.pack('<f',1.00001)
 if label=='changed-position':edits[(tr,'position')]=struct.pack('<3f',5.,0.,0.)
 if label=='moved-mesh':
  dag=next(e for e in d.elements if e.type=='DmeDag');edits[(dag.get('transform'),'position')]=struct.pack('<3f',1.,0.,0.)
 if label=='root-away':edits[(0,'model')]=struct.pack('<i',0)
 if label=='bind-away':
  i,bind=next((i,e)for i,e in enumerate(d.elements)if e.type=='DmeTransformList');refs=list(bind.get('transforms'));refs[0]=refs[1];edits[(i,'transforms')]=struct.pack('<i'+'i'*len(refs),len(refs),*refs)
 if label in ['mesh-under-joint','shared-owner']:
  ji,joint=next((i,e)for i,e in enumerate(d.elements)if e.type=='DmeJoint');dag=next(i for i,e in enumerate(d.elements)if e.type=='DmeDag');children=list(joint.get('children'))+[dag];edits[(ji,'children')]=struct.pack('<i'+'i'*len(children),len(children),*children)
 if label=='duplicate-palette':
  i,m=next((i,e)for i,e in enumerate(d.elements)if e.type=='DmeModel');refs=list(m.get('jointList'));refs[-1]=refs[0];edits[(i,'jointList')]=struct.pack('<i'+'i'*len(refs),len(refs),*refs)
 raw=d.rewritten_binary9(attributes=edits)
 from deadlock_builder.preparation.dmx_bind import audit_dmx_bind
 try:audit_dmx_bind(Dmx(raw),s);raise RuntimeError(label+' unexpectedly passed')
 except BuildError:pass
 (OUT/f'bind-bad-{label}.dmx').write_bytes(raw);bind_cases.append(label)
(OUT/'bind-bad.json').write_text(json.dumps(bind_cases))
# Base source/template API goldens.
d=Dmx((OUT/'prepared.dmx').read_bytes());template=keep_template(s);r={'sha256':'a'*64,'skeleton':s,'scale_policy':SCALE_POLICY};rp=OUT/'prepared-reference.json';rp.write_text(json.dumps(r));audit=source_bones(d,template,str(rp),'a'*64)
(OUT/'prepared-source-bones.json').write_text(json.dumps({'required':{n:s['m_boneName'][p]if p>=0 else None for n,p in zip(s['m_boneName'],s['m_nParent'])},'audit':audit},indent=2))
