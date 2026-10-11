"""Synthetic typed fixtures for the deliberate no-cloth extension correction."""
import sys,copy
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_skeleton_extensions import extension_fixture
from tests.test_compiled_bone_aliases import resource
from tests.fixtures import resource as reference_resource
from deadlock_builder.motion_restore import container,payload
out=Path(__file__).parent/'fixtures/no-cloth-extension';out.mkdir(exist_ok=True)
a,b=extension_fixture();a[7]=('PHYS',{'m_pFeModel':None});a[0][1]['m_modelSkeleton']['m_nFlag']=[64]*3
refs=reference_resource(['models/test/cloth.vmat']);rer=payload(refs,next(x for x in container(refs) if x['name']=='RERL'))
for absent in (False,True):
 for name,blocks in [('original',a),('candidate',b)]:
  blocks=copy.deepcopy(blocks)
  if absent:blocks[7]=('TEST',blocks[7][1]);blocks[1][1].pop('embedded_physics')
  blocks.append(('RERL',rer));(out/(('absent-' if absent else 'empty-')+name+'.vmdl_c')).write_bytes(resource(blocks))
