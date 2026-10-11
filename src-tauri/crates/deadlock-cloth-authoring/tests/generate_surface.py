"""Development-only independent Python oracle for native source authoring."""
from pathlib import Path
import copy,json,sys,tempfile,hashlib
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_cloth_surface import report,skeleton,append_overlay,append_panel
from deadlock_builder.dmx import Dmx
from deadlock_builder.preparation.export import export_dmx
from deadlock_builder.customization.cloth_surface import author_direct_cloth
from deadlock_builder.customization.cloth_motion import MotionProfile,author_motion_maps
OUT=Path(__file__).parent/'fixtures/surface';OUT.mkdir(parents=True,exist_ok=True)
cases=[]
for name in ['base','motion','overlay','far_overlay','wrong_weights','panel','ambiguous','collapsed','unanchored','boundary','budget']:
 data=report();g=data['meshes'][0];sel={'garment':{'enabled':list(range(3,9)),'pinned':[0,1,2]}};options={'max_edge_inches':.7};motion=None
 if name=='motion':motion=MotionProfile()
 if name in ('overlay','far_overlay','wrong_weights','ambiguous'):
  if name=='ambiguous':
   new=append_panel(g,[0,.002,0]);sel['garment']['enabled']+=new['enabled'];sel['garment']['pinned']+=new['pinned']
  sel['garment']['enabled']+=append_overlay(g,offset=.2 if name=='far_overlay' else .02,wrong_weights=name=='wrong_weights')
 if name=='panel':
  new=append_panel(g,[4,0,0]);sel['garment']['enabled']+=new['enabled'];sel['garment']['pinned']+=new['pinned'];motion=MotionProfile()
 if name=='collapsed':g['positions']=[[x+1e7,0,z]for x,y,z in g['positions']]
 if name=='unanchored':sel['garment']['enabled']=list(range(9));sel['garment']['pinned']=[0]
 if name=='boundary':sel['garment']['pinned']=[0]
 if name=='budget':options['max_proxy_vertices']=9
 path=OUT/(name+'.dmx');export_dmx(data,skeleton(),path);dmx=Dmx.load(path);sha=hashlib.sha256(dmx.data).hexdigest();case={'name':name,'source_sha256':sha,'selection':sel,'options':options,'motion':motion.values()if motion else None}
 with tempfile.TemporaryDirectory()as temp:
  try:
   receipt=author_direct_cloth(dmx,sel,Path(temp)/'result',source_sha256=sha,motion_profile=motion,**options)
   for key in ['render_path','proxy_path']:
    payload=Path(receipt.pop(key)).read_bytes();(OUT/(name+('.render.dmx'if key=='render_path'else'.proxy.dmx'))).write_bytes(payload)
   case.update(passed=True,receipt=receipt)
  except Exception as e:case.update(passed=False,error=str(e))
 cases.append(case)
(OUT/'oracle.json').write_text(json.dumps(cases,indent=2)+'\n');print([(c['name'],c['passed'])for c in cases])
