"""Development-only Python reference oracle; never used by production Rust."""
import copy
import json
from pathlib import Path
import struct
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_skirt_physics import SkirtPhysicsTests, add_bone
from deadlock_builder.customization.skirt_physics import analyze

ROOT = Path(__file__).parent / 'fixtures' / 'physics'
ROOT.mkdir(parents=True, exist_ok=True)

def fallback(t):
    for name,x,z in [('leg_upper_L',2.,7.),('leg_lower_L',2.,3.),('leg_upper_R',-2.,7.),('leg_lower_R',-2.,3.)]:
        mesh=copy.deepcopy(t.meshes['meshes'][0]);mesh['name']=name+'_mesh'
        mesh['positions']=[[x+.3,0.,z],[x,.3,z],[x-.3,0.,z+.1]]
        mesh['weights']=[[[name,1.]]]*3;t.meshes['meshes'].append(mesh)

def case(name, change=lambda t:None, options=None, snapshot=True):
    t=SkirtPhysicsTests();t.setUp()
    try:
        change(t)
        report=copy.deepcopy(t.report)
        if not snapshot: report.pop('compiled_snapshot')
        result=analyze(t.dmx,report,t.contract,options)
        folder=ROOT/name;folder.mkdir(exist_ok=True)
        (folder/'source.dmx').write_bytes(t.dmx.data)
        (folder/'original.vmdl_c').write_bytes((t.root/'original.vmdl_c').read_bytes())
        report.pop('compiled_snapshot',None)
        data={'audit':report,'selected':t.selected,'options':options or {},'snapshot':snapshot,'expected':result}
        (folder/'oracle.json').write_text(json.dumps(data,indent=2,sort_keys=True,allow_nan=False)+'\n')
    finally: t.doCleanups()

case('ragdoll')
def scale(t):
    t.reference['m_boneScaleParent'][0]=2.;t.source['m_boneScaleParent'][0]=2.;t.write_original(radius=.25);t.write_source()
case('scale',scale)
def rounded(t):
    n=struct.unpack('<f',struct.pack('<f',.99999994))[0]
    t.reference['m_boneScaleParent'][0]=n;t.source['m_boneScaleParent'][0]=n;t.write_original(radius=.25);t.write_source()
    t.report['runtime_motion']['skeleton']['m_boneScaleParent'][0]=.99999994
case('rounded',rounded)
def noextras(t):
    t.meshes['meshes'][0]['weights']=[[['pelvis',1.]]]*3;t.write_source()
case('noextras',noextras)
case('missing_snapshot',snapshot=False)
def ambiguous(t):
    add_bone(t.reference,'thigh_L',0,[2.,0.,-1.]);t.source=copy.deepcopy(t.reference)
    add_bone(t.source,'garment_top',0,[3.,0.,0.]);add_bone(t.source,'garment_middle',8,[0.,0.,-2.]);add_bone(t.source,'garment_end',9,[0.,0.,-2.])
    t.write_original();t.write_source()
case('ambiguous',ambiguous)
case('explicit_role',ambiguous,{'roles':{'thigh_L':'leg_upper_L'}})
def chain(t,branch):
    t.source['m_nParent'][9]=7 if branch else 8;t.source['m_bonePosParent'][9]=[0.,0.,-2. if branch else 2.]
    t.meshes['meshes'][0]['weights']=[[['garment_middle',.5],['garment_end',.5]]]*3;t.write_source()
case('branching',lambda t:chain(t,True));case('upward',lambda t:chain(t,False))
def radius(t,r): t.write_original(radius=r);t.write_source()
case('zero_radius',lambda t:radius(t,0.));case('negative_radius',lambda t:radius(t,-1.))
def fit(t,count=1): fallback(t);t.write_original(capsules=False,phys_copies=count);t.write_source()
case('fallback',fit);case('no_phys',lambda t:fit(t,0));case('ambiguous_phys',lambda t:fit(t,2))
case('explicit_empty_roots',options={'roots':[]})
case('collision_subset',fit,{'collision_meshes':['body']})
print('Generated',len(list(ROOT.glob('*/oracle.json'))),'physics oracles')
