"""Development oracle for native accepted baseline transfer (hashes, not copies)."""
import sys,json,hashlib,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder import compiled_kv3 as kv,motion_restore as mr
from deadlock_builder.customization import tested_physics as tp
from deadlock_builder.project import Project
original=(ROOT/'physics-references/Vindicta-R12/cloth_supported_hair.vmdl_c').read_bytes()
reference_path=ROOT/'physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c'
reference=reference_path.read_bytes();blocks=mr.container(reference)
data_block=mr.unique(blocks,'DATA');physics_block=mr.unique(blocks,'PHYS')
data=kv.decode(mr.payload(reference,data_block));physics=kv.decode(mr.payload(reference,physics_block))
data['m_modelSkeleton']['m_nFlag']=[int(f)&~0x400008 for f in data['m_modelSkeleton']['m_nFlag']];physics['m_pFeModel']=None
candidate=mr.rebuild(reference,blocks,{'DATA':kv.encode(data,mr.payload(reference,data_block)[4:20]),'PHYS':kv.encode(physics,mr.payload(reference,physics_block)[4:20])})
project=Project(original_model='models/example.vmdl',customization={'enabled':True,'physics':{'enabled':True,'source':str(reference_path),'hero_path':'models/example.vmdl'}})
proof=tp.inspect(project,original);output,receipt=tp.apply(project,original,candidate,proof)
result={'original_sha256':hashlib.sha256(original).hexdigest(),'reference_sha256':hashlib.sha256(reference).hexdigest(),'candidate_sha256':hashlib.sha256(candidate).hexdigest(),'output_sha256':hashlib.sha256(output).hexdigest(),'status':receipt['status']}
(Path(__file__).parent/'fixtures/native-transfer.json').write_text(json.dumps(result,indent=2)+'\n')
print(result)
