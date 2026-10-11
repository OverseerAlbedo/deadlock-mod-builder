"""Reproduce differential fixtures from the untouched Python implementation.

TEST TOOL ONLY: no Python process or Python dependency is used by the Rust app.
Run from any working directory with the same Unicode baseline (15.0.0).
"""
from pathlib import Path
import hashlib
import json
import sys
import unicodedata
import tempfile

REPO = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(REPO))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.packer import write_vpk
from deadlock_builder.vpk import Vpk, resource_blocks, resource_references
from deadlock_builder.package_snapshot import create_snapshot, remove_snapshot
from tests.fixtures import vpk, resource
from tests.test_resource_container import compiled_resource

HERE = Path(__file__).resolve().parent
OUT = HERE / 'fixtures'
OUT.mkdir(exist_ok=True)
FILES = {
    'models/heroes_wip/arbitrary/test_model.vmdl_c': bytes(range(256)) * 17,
    'materials/UB New.vmat_c': b'opaque compiled material\0\xff',
    'rootfile': b'no extension',
    'empty.txt': b'',
    'unicode/na\u00efve.txt': 'h\u00e9llo'.encode(),
    '.hidden': b'hidden root resource',
}
for path, data in FILES.items():
    target = OUT / 'source' / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
hashes = {k: hashlib.sha256(v).hexdigest() for k, v in FILES.items()}
receipts = {}
for version in (1, 2):
    target = OUT / f'python-v{version}_dir.vpk'
    target.unlink(missing_ok=True)
    receipts[f'v{version}'] = write_vpk(OUT / 'source', list(FILES), target, expected_hashes=hashes, version=version)
    assert Vpk(target).verify_files(hashes) == hashes
    if version == 2:
        assert Vpk(target).verify_v2_hashes()
    for split in (False, True):
        target = OUT / f'preload-v{version}-{int(split)}_dir.vpk'
        vpk(target, FILES, version=version, preload=7, split=split)
        assert Vpk(target).verify_files(hashes) == hashes
refs = ['models/test/rig.vnmskel', 'animgraphs/test/hero.vnmgraph']
resource_data = compiled_resource([(b'MDAT', b'x')] * 638 + [(b'MARK', b''), (b'RERL', resource(refs)[28:])])
(OUT / 'resource.bin').write_bytes(resource_data)
metadata = {
    'generator': 'tests/generate_fixtures.py; Python baseline',
    'hashes': hashes,
    'receipts': receipts,
    'resource': resource_blocks(resource_data),
    'references': resource_references(resource_data),
    'models': Vpk(OUT / 'python-v2_dir.vpk').models(),
}
with tempfile.TemporaryDirectory() as temporary:
    stage = Path(temporary) / 'snapshot'
    metadata['snapshot_receipt'] = create_snapshot(OUT / 'source', stage, settle_seconds=0, retry_seconds=0).receipt()
    remove_snapshot(stage)
(OUT / 'expected.json').write_text(json.dumps(metadata, indent=2, ensure_ascii=False) + '\n')
# Record source/version evidence for the generated Unicode mapping. Core owns
# the shared implementation; VPK does not carry a second runtime table.
assert unicodedata.unidata_version == '15.0.0', unicodedata.unidata_version
mapping = [[chr(i), chr(i).casefold()] for i in range(0x110000) if chr(i).casefold() != chr(i)]
(OUT / 'casefold.json').write_text(json.dumps({'unicode_version': unicodedata.unidata_version, 'mapping': mapping}, ensure_ascii=False) + '\n')
(OUT / 'provenance.json').write_text(json.dumps({
    'python': sys.version,
    'unicode_version': unicodedata.unidata_version,
    'sources_sha256': {str(p): hashlib.sha256((REPO / p).read_bytes()).hexdigest() for p in (
        'legacy/python/deadlock_builder/vpk.py', 'legacy/python/deadlock_builder/packer.py', 'legacy/python/deadlock_builder/file_stability.py',
        'legacy/python/deadlock_builder/package_snapshot.py', 'tests/fixtures.py', 'tests/test_resource_container.py')},
    'fixture_sha256': {str(p.relative_to(OUT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(OUT.rglob('*')) if p.is_file() and p.name != 'provenance.json'},
}, indent=2) + '\n')
