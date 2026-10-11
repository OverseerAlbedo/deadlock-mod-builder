"""Regenerate reference fixtures using the existing Python behavior; not production code."""
from pathlib import Path
import sys,json
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_toolchain import pe_bytes
from tests.fixtures import resource
from deadlock_builder.pe import imports
HERE=Path(__file__).parent/'fixtures'
for name,args in [('imports',{}),('delay',{'names':('resourcecompiler.exe','KERNEL32.dll'),'delay':True})]:
    path=HERE/(name+'.bin');path.write_bytes(pe_bytes(**args))
    (HERE/(name+'.json')).write_text(json.dumps(imports(path),indent=2)+'\n')
(HERE/'model-resource.bin').write_bytes(resource([],with_data=True))
