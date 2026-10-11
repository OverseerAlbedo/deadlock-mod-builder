"""Test-only Python oracle for Rust material, appearance and source ZIP parity."""
from pathlib import Path
import hashlib
import json
import sys
import tempfile
import zipfile

REPO=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(REPO))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder import materials, source_package
from deadlock_builder.customization import appearance
from tests.fixtures import resource
OUT=Path(__file__).resolve().parent/'fixtures'; OUT.mkdir(exist_ok=True)
configurations=[{'preset':preset} for preset in materials.PRESETS]
configurations += [{'preset':'bare metal','color':[1,.2,.1],'roughness':None,'metalness':.8}, {'color':[-0.,.123456789,.0000001]}]
text_cases=[]
for config in configurations:
    for paths in ({},dict(sorted({'color':'models/test/téxture😀.png','normal':'models/test/n.tga','emission':'models/test/em.exr'}.items()))):
        text_cases.append({'config':config,'paths':paths,'expected':materials.material_text(config,paths)})
base='// authored\n"Layer0"\n{\n "shader" "pbr.vfx"\n "g_vColorTint1" "[1 1 1 1]"\n "custom_future_field" "literal value"\n}\n'
patch_cases=[]
for row in ({},{'color':[.2,.3,.4],'roughness':.25,'metalness':.7,'opacity':.5,'emission':2},{'roughness':1e-8,'emission':0},{'color':[-0.,.12345678949,.99999999999],'opacity':1e-4}):
    patch_cases.append({'text':base,'row':row,'expected':appearance.patch_text(base,row)})
texture_names=['body_color.png','body_color_edited.png','body_normal.png','nobody_color.png','body_normal_roughness.png','body.unlabeled.tga','body_ao.jpg','nested/body_metallic.png','nested/body_emissive.exr','body_opacity.tif','body_color.txt']
textures=OUT/'textures';textures.mkdir(exist_ok=True)
for name in texture_names:
    path=textures/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b'texture fixture')
source=OUT/'source';(source/'models/test').mkdir(parents=True,exist_ok=True);(source/'models/test/existing.vmat').write_text('"Layer0" { "shader" "pbr.vfx" }')
plans=materials.plan(['models/test/body.vmat','models/test/existing.vmat'],source,textures,{'models/test/body.vmat':{'textures':{'color':''},'confirmed':True}})
def normalized(value):
    if isinstance(value,dict):return {k:normalized(v) for k,v in value.items()}
    if isinstance(value,list):return [normalized(v) for v in value]
    if isinstance(value,str):return value.replace(str(textures.resolve()),'$TEXTURES')
    return value
refs_text='"Layer0" { "TextureColor1" "models/test/body.png" // "nested/child.vmat"\n "constant" "[1 1 1 1]" "TextureNormal1" "models/test/n.TGA" }'
engine={'compiler':'a'*64,'C:/Tools/éngine.dll':[123,1234567890000000000]};sources={'models/test/body.vmat':'b'*64,'models/test/téxture.png':'c'*64}
cache_key=hashlib.sha256(json.dumps({'source':sources,'engine':engine},sort_keys=True).encode()).hexdigest()
expected={'material_text':text_cases,'appearance':patch_cases,'plans':normalized(plans),'references':{'text':refs_text,'expected':source_package.texture_references(refs_text)},'cache_key':{'source':sources,'engine':engine,'expected':cache_key}}
(OUT/'expected.json').write_text(json.dumps(expected,indent=2,ensure_ascii=False)+'\n')
# Compiled material graph: two VMATs can share an unchanged texture dependency.
(OUT/'compiled-material.bin').write_bytes(resource(['materials/shared.vtex'],with_data=True))
(OUT/'compiled-texture.bin').write_bytes(resource(with_data=True))
files={'model.dmx':b'fixture opaque DMX','template.vmdl':b'{}','source_materials/models/a.vmat':b'Layer0 {}','material_mapping.json':b'{}','material_remap.json':b'{}'}
manifest=[{'path':n,'bytes':len(v),'sha256':hashlib.sha256(v).hexdigest()}for n,v in files.items()]
for suffix,method in [('stored',zipfile.ZIP_STORED),('deflate',zipfile.ZIP_DEFLATED),('bzip2',zipfile.ZIP_BZIP2),('lzma',zipfile.ZIP_LZMA)]:
    with zipfile.ZipFile(OUT/f'source-{suffix}.zip','w',compression=method)as archive:
        for n,v in files.items():archive.writestr('fixture/'+n,v)
        archive.writestr('fixture/manifest.json',json.dumps(manifest))
with tempfile.TemporaryDirectory()as temporary:
    selection=source_package.import_zip(OUT/'source-deflate.zip',Path(temporary))
    assert Path(selection['dmx']).read_bytes()==files['model.dmx']
(OUT/'provenance.json').write_text(json.dumps({'python':sys.version,'sources_sha256':{name:hashlib.sha256((REPO/name).read_bytes()).hexdigest()for name in ['legacy/python/deadlock_builder/materials.py','legacy/python/deadlock_builder/customization/appearance.py','legacy/python/deadlock_builder/source_package.py']},'fixtures_sha256':{str(p.relative_to(OUT)):hashlib.sha256(p.read_bytes()).hexdigest()for p in sorted(OUT.rglob('*'))if p.is_file()and p.name!='provenance.json'}},indent=2)+'\n')
