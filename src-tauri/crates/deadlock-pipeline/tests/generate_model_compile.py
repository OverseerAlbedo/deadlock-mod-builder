"""Development-only generated resources: structurally typed, not playable."""
from pathlib import Path
import sys,json
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_compiled_bone_aliases import fixture,resource
from tests.fixtures import resource as ref_resource
from deadlock_builder.modeldoc import header
OUT=Path(__file__).parent/'fixtures/model-compile';OUT.mkdir(exist_ok=True)
refs=['models/test/rig.vnmskel','animgraphs/test/game.vnmgraph','animgraphs/test/ui.vnmgraph','models/test/cloth.vmat']
blocks=fixture(canonical=True);data=blocks[0][1];data.update(m_vecNmSkeletonRefs=[refs[0]],m_animGraph2Refs=[{'m_sIdentifier':'default','m_hGraph':refs[1]},{'m_sIdentifier':'UI','m_hGraph':refs[2]}]);blocks.append(('RERL',ref_resource(refs)[28:]));(OUT/'model.vmdl_c').write_bytes(resource(blocks))
(OUT/'reference.json').write_text(json.dumps({'sha256':'synthetic','skeleton':data['m_modelSkeleton']},indent=2))
text=header(28)+'''\n{ rootNode = { _class = "RootNode" children = [
{ _class = "NmSkeletonList" children = [{ _class = "NmSkeletonReference" filename = "models/test/rig.vnmskel" }] }
{ _class = "AnimGraph2List" children = [ { _class = "DefaultAnimGraph2" filename = "animgraphs/test/game.vnmgraph" } { _class = "AnimGraph2" name = "UI" filename = "animgraphs/test/ui.vnmgraph" } ] }
{ _class = "RenderMeshList" children = [ { _class = "RenderMeshFile" name = "body" filename = "models/test/body.dmx" } { _class = "RenderMeshFile" name = "cape" filename = "models/test/cape.dmx" } ] }
] } }\n'''
(OUT/'model.vmdl').write_text(text)
