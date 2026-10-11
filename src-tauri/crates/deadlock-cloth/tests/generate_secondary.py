"""Synthetic joint-fitting gates; these bytes are never native-motion evidence."""
from pathlib import Path
import sys,json,copy
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_tested_physics import fixtures
from tests.test_compiled_bone_aliases import resource
from tests.test_skeleton_extensions import authority
OUT=Path(__file__).parent/'fixtures/secondary';OUT.mkdir(exist_ok=True)
a,b,c=fixtures();sk=c[0][1]['m_modelSkeleton'];fe=c[7][1]['m_pFeModel'];n=len(sk['m_boneName']);extra=sk['m_boneName'][-2:]
sk['m_nFlag']=[(v&~0x400008)|(0x400008 if name in extra else 0) for name,v in zip(sk['m_boneName'],sk['m_nFlag'])]
fe['m_nNodeCount']=n+6;fe['m_nStaticNodes']=n;fe['m_nRotLockStaticNodes']=n;fe['m_nRopeCount']=0
fe['m_CtrlName']+=['proxy_'+str(i) for i in range(6)];fe['m_CtrlHash']=list(range(n+6));fe['m_SkelParents']+=[n-2]*3+[n-1]*3
fe['m_NodeInvMasses']=[0.]*n+[1.]*6
for i in range(6):fe['m_InitPose'].append([[0,0,1],[1,0,1],[0,1,1]][i%3]+[1.,0.,0.,0.,1.])
fe['m_NodeIntegrator'] += [copy.deepcopy(fe['m_NodeIntegrator'][-1]) for _ in range(6)]
fe['m_CtrlOffsets']=[{'nCtrlParent':n-2+i//3,'nCtrlChild':n+i,'vOffset':[0.,0.,0.]} for i in range(6)]
fe['m_FitMatrices']=[{'nNode':n-2,'nBeginDynamic':0,'nEnd':3},{'nNode':n-1,'nBeginDynamic':3,'nEnd':6}]
fe['m_FitWeights']=[{'nNode':n+i,'flWeight':1/3,'nDummy':0} for i in range(6)]
fe['m_Rods']=[];fe['m_SimdRods']=[]
parents=[65535]*11;children=[];queue=list(range(6))
while len(queue)>1:
 left,right=queue.pop(0),queue.pop(0);at=6+len(children);parents[left]=parents[right]=at;children.append({'nChild':[left,right]});queue.append(at)
fe['m_TreeChildren']=children;fe['m_TreeParents']=parents;fe['m_TreeCollisionMasks']=[0]*11
orig=resource(a);native=resource(c);src=authority(orig,b)
(OUT/'original.vmdl_c').write_bytes(orig);(OUT/'native.vmdl_c').write_bytes(native);(OUT/'source.json').write_text(json.dumps(src,indent=2));(OUT/'groups.json').write_text(json.dumps(extra))
print('Synthetic controls',n+6,'selected',extra)
