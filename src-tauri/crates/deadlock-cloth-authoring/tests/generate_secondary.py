"""Independent Python frame-proxy oracle; development fixtures only."""
from pathlib import Path
import sys,json,hashlib,tempfile
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_secondary_proxy import fixture
from deadlock_builder.customization.secondary_proxy import generate
from deadlock_builder.customization.secondary_tuning import retune
from deadlock_builder.customization.cloth_motion import MotionProfile
OUT=Path(__file__).parent/'fixtures/secondary';OUT.mkdir(exist_ok=True)
for case,kwargs,roots in [('chain',{},['garment_root']),('independent',{'singleton':True},['garment_root','garment_mid','garment_tip']),('collapsed',{'zero_length':True},['garment_root']),('unweighted',{'unweighted':True},['garment_root'])]:
 with tempfile.TemporaryDirectory() as t:
  raw=fixture(Path(t),**kwargs);(OUT/(case+'.dmx')).write_bytes(raw)
  if case in ['collapsed','unweighted']:continue
  proxy,receipt=generate(raw,roots,source_sha256=hashlib.sha256(raw).hexdigest())
  (OUT/(case+'.proxy.dmx')).write_bytes(proxy);(OUT/(case+'.json')).write_text(json.dumps(receipt,indent=2)+'\n')
