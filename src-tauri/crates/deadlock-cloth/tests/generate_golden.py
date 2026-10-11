"""Development-only Python oracle fixture capture; not a runtime dependency."""
import sys,json,hashlib,copy,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder import motion_restore,compiled_kv3 as kv
from deadlock_builder.customization import tested_physics
from deadlock_builder.project import Project
from tests.test_motion_restore import model
from tests.test_runtime_motion import skeleton,physics
from tests.test_tested_physics import fixtures
from tests.test_compiled_bone_aliases import resource
from tests.test_skeleton_extensions import authority,extension_fixture
OUT=Path(__file__).parent/'fixtures';OUT.mkdir(exist_ok=True)
def write(name,raw):
    (OUT/name).write_bytes(raw);return hashlib.sha256(raw).hexdigest()
manifest={'restorations':[],'tested':{},'rejects':[]}
a,b=model(),model(True)
for name,original,candidate,source in [('simple',a,b,None),('unchanged',a,a,None),('not-required',b,b,None)]:
    output,receipt=motion_restore.restore_bytes(original,candidate,source)
    write(name+'-original.vmdl_c',original);write(name+'-candidate.vmdl_c',candidate)
    manifest['restorations'].append({'name':name,'output_sha256':write(name+'-output.vmdl_c',output),'status':receipt['status']})
s=skeleton();s={k:list(reversed(v)) for k,v in s.items()};s['m_nParent']=[1,-1];s['m_nFlag']=[0x80,0x20]
original=a;candidate=model(True,bones=s);output,receipt=motion_restore.restore_bytes(original,candidate)
write('reordered-original.vmdl_c',original);write('reordered-candidate.vmdl_c',candidate)
manifest['restorations'].append({'name':'reordered','output_sha256':write('reordered-output.vmdl_c',output),'status':receipt['status']})
a,b=extension_fixture();source=authority(resource(a),b)
output,receipt=motion_restore.restore_bytes(resource(a),resource(b),source)
write('extended-original.vmdl_c',resource(a));write('extended-candidate.vmdl_c',resource(b));(OUT/'extended-source.json').write_text(json.dumps(source))
manifest['restorations'].append({'name':'extended','output_sha256':write('extended-output.vmdl_c',output),'status':receipt['status'],'source':'extended-source.json'})
a,b,c=fixtures();original,candidate,reference=map(resource,(a,b,c));source=authority(original,b)
write('tested-original.vmdl_c',original);write('tested-candidate.vmdl_c',candidate);write('tested-reference.vmdl_c',reference);(OUT/'tested-source.json').write_text(json.dumps(source))
with tempfile.TemporaryDirectory() as temp:
    ref=Path(temp)/'ref.vmdl_c';ref.write_bytes(reference)
    project=Project(original_model='models/example.vmdl',customization={'enabled':True,'physics':{'enabled':True,'source':str(ref),'hero_path':'models/example.vmdl'}})
    proof=tested_physics.inspect(project,original,source);output,receipt=tested_physics.apply(project,original,candidate,proof,source)
    manifest['tested']={'output_sha256':write('tested-output.vmdl_c',output),'node_count':proof['structure']['node_count'],'extra_names':proof['target']['extra_names']}
    for kind in ('rest','scale','inverse','animation','unknown-solver'):
        changed=copy.deepcopy(b)
        if kind=='rest':changed[0][1]['m_modelSkeleton']['m_bonePosParent'][-1][0]+=.001
        if kind=='scale':changed[0][1]['m_modelSkeleton']['m_boneScaleParent'][-1]=1.
        if kind=='inverse':changed[2][1]['m_skeleton']['m_bones'][-1]['m_invBindPose'][0]+=.001
        if kind=='animation':changed[4][1]['m_decodeKey']['m_boneArray'][-1]['m_pos'][0]+=.002
        if kind=='unknown-solver':changed[7]=copy.deepcopy(c[7]);changed[7][1]['m_pFeModel']['m_unrecognizedRetainedField']['coefficients'][0]=99.
        raw=resource(changed)
        try:tested_physics.apply(project,original,raw,proof,source)
        except Exception as error:manifest['rejects'].append({'name':kind,'stage':'apply','python_error':str(error)})
        else:raise AssertionError(kind)
        write('reject-'+kind+'.vmdl_c',raw)
    for kind in ('array','tree','rods','fit','physics-binding','uint16','duplicate-ctrl'):
        changed=copy.deepcopy(c);fe=changed[7][1]['m_pFeModel']
        if kind=='array':fe['m_NodeInvMasses'].pop()
        if kind=='tree':fe['m_TreeChildren'][-1]['nChild'][0]=6
        if kind=='rods':fe['m_Rods'].pop()
        if kind=='fit':fe['m_FitMatrices']=[None]
        if kind=='physics-binding':changed[1][1]['embedded_physics']['phys_data_block']=0
        if kind=='uint16':changed[0][1]['m_remappingTableStarts'][-1]=65536
        if kind=='duplicate-ctrl':changed[1][1]['embedded_meshes'][-1]=copy.deepcopy(changed[1][1]['embedded_meshes'][0])
        raw=resource(changed)
        try:tested_physics.qualify(raw)
        except Exception as error:manifest['rejects'].append({'name':kind,'stage':'qualify','python_error':str(error)})
        else:raise AssertionError(kind)
        write('reject-'+kind+'.vmdl_c',raw)
    blocks=[(block['name'],motion_restore.payload(output,block)) for block in motion_restore.container(output)]
    for kind in ('solver','body','flags'):
        changed=copy.deepcopy(blocks);ordinal=0 if kind=='flags' else 7;value=kv.decode(changed[ordinal][1])
        if kind=='solver':value['m_pFeModel']['m_unrecognizedRetainedField']['coefficients'][0]=.9
        if kind=='body':value['m_bodySentinel']['joints'][0]=5
        if kind=='flags':value['m_modelSkeleton']['m_nFlag'][-1]&=~0x400008
        changed[ordinal]=(changed[ordinal][0],kv.encode(value,changed[ordinal][1][4:20]));raw=resource(changed)
        try:tested_physics.audit(project,original,raw,proof,source)
        except Exception as error:manifest['rejects'].append({'name':kind,'stage':'validate','python_error':str(error)})
        else:raise AssertionError(kind)
        write('reject-'+kind+'.vmdl_c',raw)
(OUT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Captured',len(manifest['restorations']),'restorations and',len(manifest['rejects']),'rejection fixtures')
from tests.fixtures import resource as references_resource
blocks=[(b['name'],motion_restore.payload(model(),b)) for b in motion_restore.container(model()) if b['name']!='PHYS']
write('no-phys.vmdl_c',resource(blocks))
ref=references_resource(['physics/external.vphys'])
rer=('RERL',motion_restore.payload(ref,motion_restore.unique(motion_restore.container(ref),'RERL')))
write('external-phys.vmdl_c',resource(blocks+[rer]))
phys=next((b['name'],motion_restore.payload(model(),b)) for b in motion_restore.container(model()) if b['name']=='PHYS')
write('multiple-phys.vmdl_c',resource(blocks+[phys,phys]))
