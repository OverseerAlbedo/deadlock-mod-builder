"""Python differential cloth/audit fixtures; development only."""
from pathlib import Path
from copy import deepcopy
import json,math,sys
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder import compiled_kv3 as kv,runtime_motion
from deadlock_builder.motion_restore import container,payload,unique
from deadlock_builder.customization.tested_physics import _solver_structure
from deadlock_builder.customization.cloth_compiled_capacity import audit_rods,audit_remapping
from deadlock_builder.customization.cloth_constraint_audit import audit_constraint_feasibility
from tests.test_cloth_constraint_audit import fixture,set_rod
from tests.test_compiled_kv3_compression import GUID
DEST=Path(__file__).parent/'fixtures'
cases=[]
def save(name,physics):
    (DEST/('cloth-'+name+'.kv3')).write_bytes(kv.encode(physics,GUID))
    cases.append({'name':name,'rods':audit_rods(physics),'constraints':audit_constraint_feasibility(physics)})
save('no-solver',{'m_pFeModel':None});save('missing-solver',{});save('malformed-solver',{'m_pFeModel':[]})
for length in (1.,2.,1.999995,2.000005,1.99,2.01,2.1,3.):
    p=fixture();set_rod(p['m_pFeModel'],maximum=length);save('maximum-'+str(length),p)
mutations={
 'mask-excluded':lambda f:f.__setitem__('m_TreeCollisionMasks',[2]),
 'outside-anchor':lambda f:f['m_InitPose'][0].__setitem__(slice(0,3),[0.,3.,0.]),
 'quaternion-zero':lambda f:f['m_InitPose'][1].__setitem__(7,0.),
 'pose-short':lambda f:f['m_InitPose'][1].pop(),
 'mass-static':lambda f:f['m_NodeInvMasses'].__setitem__(0,1.),
 'mass-dynamic':lambda f:f['m_NodeInvMasses'].__setitem__(2,0.),
 'extra-mask':lambda f:f['m_TreeCollisionMasks'].append(1),
 'bool-mask':lambda f:f['m_TreeCollisionMasks'].__setitem__(0,True),
 'static-overflow':lambda f:f.__setitem__('m_nStaticNodes',4),
 'collider-index':lambda f:f['m_TaperedCapsuleRigids'][0].update(nNode=3),
 'negative-radius':lambda f:f['m_TaperedCapsuleRigids'][0]['vSphere'][0].__setitem__(3,-1.),
 'missing-rod':lambda f:f['m_Rods'].clear(),
 'vertex-map':lambda f:f['m_VertexMaps'].append({}),
 'flags':lambda f:f['m_TaperedCapsuleRigids'][0].update(nFlags=1),
 'dynamic-capsule':lambda f:f['m_TaperedCapsuleRigids'][0].update(nNode=2),
 'unknown-capsule':lambda f:f['m_TaperedCapsuleRigids'][0].update(new_field=0),
 'tapered':lambda f:f['m_TaperedCapsuleRigids'][0]['vSphere'][0].__setitem__(3,3.),
 'non-unit-slot':lambda f:f['m_InitPose'][1].__setitem__(3,2.),
 'outside-quat-envelope':lambda f:f['m_InitPose'][1].__setitem__(7,1.001),
 'missing-exclusion':lambda f:f.pop('m_DynNodeVertexSet'),
 'near-unit-slot':lambda f:f['m_InitPose'][1].__setitem__(3,1.0000002),
}
for name,mutate in mutations.items():
    p=fixture();mutate(p['m_pFeModel']);save(name,p)
for maximum in (1.,3.):
    p=fixture();f=p['m_pFeModel'];f['m_nNodeCount']=4;f['m_InitPose'].append(deepcopy(f['m_InitPose'][2]));f['m_CtrlName'].append('second_dynamic');f['m_NodeInvMasses'].append(1.);f['m_TreeCollisionMasks']=[1,2,3];set_rod(f,maximum=maximum);save('mixed-'+str(maximum),p)
p=fixture();f=p['m_pFeModel'];f['m_InitPose'][1][3]=1.000009;set_rod(f,maximum=2.000005);save('slot-ambiguous',p)
p=fixture();f=p['m_pFeModel'];f['m_InitPose'][1][4:]=[0.,0.,math.sqrt(.5)*(1+2e-7),math.sqrt(.5)*(1+2e-7)];f['m_TaperedCapsuleRigids'][0]['vSphere'][0][0]=-20.;f['m_TaperedCapsuleRigids'][0]['vSphere'][1][0]=20.;set_rod(f,maximum=2.000001);save('quaternion-ambiguous',p)
native=[]
for path in ['physics-references/Vindicta-R12/cloth_supported_hair.vmdl_c','physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c']:
    raw=(ROOT/path).read_bytes();blocks=container(raw)
    def read(name):return kv.normal(kv.decode(payload(raw,unique(blocks,name))))
    physics,data,ctrl=read('PHYS'),read('DATA'),read('CTRL')
    mdats=[kv.normal(kv.decode(payload(raw,b))) for b in blocks if b['name']=='MDAT']
    native.append({'path':path,'structure':_solver_structure(physics),'rods':audit_rods(physics),
                   'remapping':audit_remapping(data,ctrl,mdats,blocks),'constraints':audit_constraint_feasibility(physics),
                   'control_bindings':runtime_motion._control_bindings(physics['m_pFeModel']['m_CtrlName'],data['m_modelSkeleton']['m_boneName'],physics['m_pFeModel'])})
(DEST/'cloth-manifest.json').write_text(json.dumps({'cases':cases,'native':native},indent=2)+'\n')
print(json.dumps({'cases':len(cases),'native':len(native)}))
