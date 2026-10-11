"""Development-only reference fixture generation. Tests/runtime never invoke Python."""
from pathlib import Path
import copy,json,sys
from types import SimpleNamespace
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.customization import automatic_cloth as automatic,cloth_inventory
from deadlock_builder.dmx import Dmx
from deadlock_builder.preparation.export import export_dmx
from deadlock_builder.project import Project
from deadlock_builder.safety import BuildError
from tests.test_cloth_surface import report,append_panel
from tests.test_skirt_physics import skeleton,SkirtPhysicsTests
from tests.test_cloth_inventory import physics
from tests.test_motion_restore import model
OUT=Path(__file__).parent/'fixtures';OUT.mkdir(exist_ok=True)
index={'surfaces':[],'inventory':[],'analysis':[]}

def save_surface(name,source,options):
 path=OUT/(name+'.dmx');export_dmx(source,skeleton(),path)
 try:
  selected,reports,_=automatic.surface_selection(Dmx.load(path),options)
  expected={'selection':selected,'surfaces':reports}
 except BuildError as e:expected={'error':str(e)}
 index['surfaces'].append({'file':path.name,'options':options,'expected':expected})
base=report()
for mesh in base['meshes']:mesh['weights']=[[['pelvis',1.]] for _ in mesh['positions']]
options={'meshes':['garment'],'attachment':'top','collision':'none'}
save_surface('automatic-top',base,options)
save_surface('automatic-bottom',base,options|{'attachment':'bottom'})
save_surface('automatic-none-selected',base,{})
save_surface('automatic-two-selected',base,options|{'meshes':['garment','rigid']})
save_surface('automatic-invalid-edge',base,options|{'attachment':'unknown'})
save_surface('automatic-missing-mesh',base,options|{'meshes':['missing']})
flat=copy.deepcopy(base);flat['meshes'][0]['positions']=[[x,z,0]for x,y,z in flat['meshes'][0]['positions']];save_surface('automatic-flat',flat,options)
disconnected=copy.deepcopy(base);append_panel(disconnected['meshes'][0],[4,0,0]);disconnected['meshes'][0]['weights']=[[['pelvis',1.]]]*len(disconnected['meshes'][0]['positions']);save_surface('automatic-disconnected',disconnected,options)
closed=copy.deepcopy(base);mesh=closed['meshes'][0];mesh.update(positions=[[0,0,0],[1,0,0],[0,1,0],[0,0,1]],weights=[[['pelvis',1.]]]*4,normals=[[0,1,0]]*12,uvs=[[[0,0]]*12],faces=[])
for n,ids in enumerate(([0,2,1],[0,1,3],[1,2,3],[2,0,3])):mesh['faces'].append({'vertices':ids,'corners':list(range(n*3,n*3+3)),'material':'materials/cloth.vmat'})
save_surface('automatic-closed',closed,options)

for case in ['capsules','none','body-fit','changed-bind']:
 f=SkirtPhysicsTests();f.setUp()
 try:
  f.source=copy.deepcopy(f.reference);f.meshes=copy.deepcopy(base);f.meshes['meshes']=f.meshes['meshes'][:1]
  f.meshes['meshes'][0]['positions']=[[x+3,y,z+7]for x,y,z in f.meshes['meshes'][0]['positions']]
  if case=='body-fit':
   f.write_original(capsules=False)
   for bone in ['leg_upper_L','leg_lower_L','leg_upper_R','leg_lower_R']:
    x=2 if bone.endswith('_L')else -2;z=7 if bone.startswith('leg_upper')else 3
    mesh=copy.deepcopy(base['meshes'][1]);mesh.update(name=bone+'body',positions=[[x+.5,0,z],[x,.6,z],[x-.7,0,z+.1]],weights=[[[bone,1.]]]*3);f.meshes['meshes'].append(mesh)
  if case=='changed-bind':f.source['m_bonePosParent'][0][0]+=.05
  f.write_source();f.report['runtime_motion']['cloth_nodes']=0
  opt=options|{'collision':'none'if case=='none'else'legs'}
  p=Project(original_model='models/test/hero.vmdl',dmx=str(f.root/'source.dmx'),customization={'enabled':True,'cloth_setup':opt})
  pipe=SimpleNamespace(root=lambda p:f.root/'work',check_cancel=lambda:None,inspect_original=lambda p:f.report)
  dmx_name='automatic-analysis-'+case+'.dmx';raw_name='automatic-original-'+case+'.vmdl_c';(OUT/dmx_name).write_bytes((f.root/'source.dmx').read_bytes());(OUT/raw_name).write_bytes((f.root/'original.vmdl_c').read_bytes())
  try:result=automatic.inspect_inputs(pipe,p)[0]['analysis']
  except BuildError as e:result={'error':str(e)}
  audit=copy.deepcopy(f.report);audit.pop('compiled_snapshot',None)
  index['analysis'].append({'dmx':dmx_name,'original':raw_name,'audit':audit,'options':opt,'expected':result})
 finally:f.doCleanups()

def inv_case(name,value):
 raw=model(phys=value);path=OUT/('inventory-'+name+'.vmdl_c');path.write_bytes(raw)
 try:expected=cloth_inventory.solver_inventory(raw)
 except BuildError as e:expected={'error':str(e)}
 index['inventory'].append({'file':path.name,'expected':expected})
inv_case('basic',physics());opaque=physics();opaque['m_pFeModel'].update(opaque=b'\x00\xff',extra=[{'bytes':b'a'}],scalar=False,floating=.1,nothing=None,string='骨');inv_case('opaque',opaque)
inv_case('absent',{});inv_case('null',{'m_pFeModel':None})
for i,(field,replacement)in enumerate([('m_nNodeCount',-1),('m_nStaticNodes',4),('m_nRotLockStaticNodes',2),('m_nNodeCount',True),('m_nFirstPositionDrivenNode',4),('m_CtrlName','root'),('m_TaperedCapsuleRigids',[2]),('m_TreeParents','not an array'),('m_SimdRods',{}),('m_TaperedCapsuleRigids',[{'nNode':-1}])]):
 value=physics();value['m_pFeModel'][field]=replacement;inv_case('bad-'+str(i),value)
(OUT/'automatic-reference.json').write_text(json.dumps(index,ensure_ascii=False,indent=2,allow_nan=False),encoding='utf8')
print({key:len(value) for key,value in index.items()})
