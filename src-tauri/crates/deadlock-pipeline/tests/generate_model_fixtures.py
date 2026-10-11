"""Test-only Python reference; never called by the application."""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.pipeline import Pipeline
from deadlock_builder.project import Project, Settings

OUT = Path(__file__).with_name('fixtures') / 'model'
OUT.mkdir(parents=True, exist_ok=True)
MODEL = 'models/heroes/test/test.vmdl'
TEXT = '''{ rootNode = { _class = "RootNode" children = [
{ _class = "RenderMeshList" children = [{ _class = "RenderMeshFile" name = "body" filename = "models/heroes/test/old.dmx" }] }
{ _class = "AttachmentList" children = [{ _class = "Attachment" name = "muzzle" }] }
] } }'''

with tempfile.TemporaryDirectory() as temporary:
    folder = Path(temporary)
    imported = folder / 'imported'
    source = imported / MODEL
    source.parent.mkdir(parents=True)
    source.write_text(TEXT)
    material = imported / 'models/unusual/agent/body.vmat'
    material.parent.mkdir(parents=True)
    material.write_text('Layer0 { shader = "citadel_character.vfx" }')
    dmx = folder / 'replacement.dmx'
    shutil.copyfile(ROOT / 'src-tauri/crates/deadlock-dmx/tests/fixtures/basic-v9.dmx', dmx)
    p = Project(original_model=MODEL+'_c', imported_source=str(imported), dmx=str(dmx), generate_materials=False)
    pipe = Pipeline(Settings(workspace=str(folder/'work')))
    report = pipe.inspect(p)
    fields = ['model','dmx','geometry','mappings','source_mappings','project_mappings','unresolved_meshes','unskinned_meshes','needs_review']
    (OUT/'inspection.json').write_text(json.dumps({key: report[key] for key in fields}, indent=2)+'\n')
    content, game, generated = pipe.prepare(p, report, offline=True)
    (OUT/'original.vmdl').write_text(TEXT)
    (OUT/'prepared.vmdl').write_bytes((content/MODEL).read_bytes())
    prepared = json.loads((pipe.root(p)/'prepared.json').read_text())
    (OUT/'prepared.json').write_text(json.dumps({'generated_materials':generated,'mesh_sources':prepared['mesh_sources'],'dmx_sha256':prepared['dmx_sha256']}, indent=2)+'\n')
    for path in prepared['mesh_sources'].values():
        (OUT/Path(path).name).write_bytes((content/path).read_bytes())
sources = ['pipeline.py','model.py','project.py','materials.py','dmx.py','source_identity.py']
(OUT/'provenance.json').write_text(json.dumps({'generator':'generate_model_fixtures.py','source_sha256':{name:hashlib.sha256((ROOT/'legacy/python/deadlock_builder'/name).read_bytes()).hexdigest() for name in sources},'native_compiler_executed':False},indent=2)+'\n')
