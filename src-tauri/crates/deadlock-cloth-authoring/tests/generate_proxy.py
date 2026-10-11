"""Development-only differential fixture capture, never imported by native app."""
import sys,json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_cloth_proxy_source import source,proposal,skeleton,ROOT as DATA
from deadlock_builder.customization.cloth_proxy_source import author_direct_proxy,configure_joint_driven_proxy
out=Path(__file__).parent/'fixtures';bones=skeleton();analysis=proposal(bones)
(out/'proxy-inputs.json').write_text(json.dumps({'skeleton':bones,'analysis':analysis}))
rows=[]
cases=[('empty28',source(version=28)),('empty40',source()),('empty41',source(version=41)),('native28',(DATA/'legacy28-cloth-source.vmdl').read_text()),('native40',(DATA/'r3.vmdl').read_text()),('unicode',source('{ _class="Arbitrary" name="\u8863\U0001f469" }, // comma, in comment\n'))]
for name,text in cases:
    authored,receipt=author_direct_proxy(text,'models/custom/garment_proxy.dmx',analysis,bones)
    (out/(name+'-before.vmdl')).write_text(text);(out/(name+'-after.vmdl')).write_text(authored);(out/(name+'-receipt.json')).write_text(json.dumps(receipt));rows.append(name)
    if name=='empty28':
        joint,proof=configure_joint_driven_proxy(authored,'builder_skirt_proxy');(out/'joint-after.vmdl').write_text(joint);(out/'joint-receipt.json').write_text(json.dumps(proof))
(out/'proxy-cases.json').write_text(json.dumps(rows))
