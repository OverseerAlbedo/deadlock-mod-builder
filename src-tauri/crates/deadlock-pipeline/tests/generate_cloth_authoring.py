"""Development fixture capture: synthetic source geometry, never playable output."""
import sys,json,shutil
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_skirt_authoring import SkirtAuthoringTests
out=Path(__file__).parent/'fixtures/cloth-authoring';out.mkdir(exist_ok=True)
f=SkirtAuthoringTests();f.setUp()
try:
    shutil.copyfile(f.project.dmx,out/'source.dmx');shutil.copyfile(f.project.original_compiled,out/'original.vmdl_c');(out/'original.vmdl').write_text(f.model_text)
    (out/'project.json').write_text(json.dumps({'original_model':f.project.original_model,'meshes':f.project.meshes,'materials':f.project.materials}))
    from deadlock_builder import compiled_kv3 as kv
    from deadlock_builder.motion_restore import container,payload
    from tests.test_compiled_bone_aliases import resource
    raw=(out/'original.vmdl_c').read_bytes();blocks=[]
    for b in container(raw):
        data=payload(raw,b)
        if b['name']=='PHYS':
            doc=kv.decode(data);doc['m_pFeModel']={'m_nNodeCount':1,'m_nStaticNodes':1,'m_nRopeCount':0,'m_CtrlName':['pelvis']};data=kv.encode(doc,data[4:20])
        blocks.append((b['name'],data))
    (out/'original.vmdl_c').write_bytes(resource(blocks))
    mesh=f.fixture.meshes['meshes'][0]
    mesh['positions']=[[3.,0.,10.],[3.5,0.,10.],[3.,0.,6.],[3.5,0.,6.]]
    mesh['normals']=[[0,1,0]]*4;mesh['uvs']=[[[0,0],[1,0],[0,1],[1,1]]]
    mesh['weights']=[[['garment_top',1.]]]*2+[[['garment_end',1.]]]*2
    mesh['faces']=[{'vertices':[0,1,3,2],'corners':[0,1,3,2],'material':'models/test/materials/cloth.vmat'}]
    f.fixture.write_source();shutil.copyfile(f.project.dmx,out/'automatic-source.dmx')
    # Exact Python workspace assembly output is qualified separately at surface
    # and source module boundaries; this service fixture exercises all glue.
finally:f.doCleanups()
