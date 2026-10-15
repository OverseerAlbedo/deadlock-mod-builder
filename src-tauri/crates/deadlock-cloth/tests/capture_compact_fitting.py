"""Capture exact native fitting blocks; never modify the input or create a mod.

Usage: python src-tauri/crates/deadlock-cloth/tests/capture_compact_fitting.py MODEL
Golden regressions are pinned to the supplied Windows compiler outputs.
"""
from pathlib import Path
import argparse
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.motion_restore import container, payload, unique


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('model', type=Path)
    parser.add_argument('--capture', choices=['alpha19', 'alpha26-ribbon'], default='alpha19')
    args = parser.parse_args()
    raw = args.model.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    expected, folder = {
        'alpha19': ('896b07db4618d3b663d73dd8364b1415f7f743691dd8465451db9796e733188a', 'compact'),
        'alpha26-ribbon': ('d46210beecf9000a40d43f5cfbfb864a705692f8b9e4b6a90b06dfec5761dd88', 'compact-ribbon'),
    }[args.capture]
    if digest != expected:
        parser.error('This golden capture requires the exact recorded native input.')
    output = Path(__file__).parent / 'fixtures' / folder
    output.mkdir(exist_ok=True)
    blocks = container(raw)
    records = {}
    for name in ['PHYS', 'DATA']:
        data = payload(raw, unique(blocks, name))
        (output / (name.lower() + '.bin')).write_bytes(data)
        records[name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    groups = json.loads((Path(os.environ.get('DMB_PRIVATE_ASSETS') or ROOT / 'private-assets') / 'cloth-authoring/tangtang-r2-groups.json').read_text())['groups']

    def compiler_name(name):
        return ''.join(chr(b) if chr(b).isascii() and (chr(b).isalnum() or b == 95)
                       else '_' for b in name.encode())

    selection = {key: sorted({compiler_name(n) for group in groups for n in group[field]})
                 for key, field in [('selected', 'bones'), ('weighted', 'weighted_bones')]}
    (output / 'groups.json').write_text(json.dumps(selection, indent=2) + '\n')
    provenance = {
        'provenance': f'Exact encoded PHYS and DATA blocks extracted from user-supplied {args.capture} Windows compiler output. No native tables changed. Not an in-game acceptance claim.',
        'resource_sha256': digest, 'resource_bytes': len(raw), 'blocks': records,
        'selected_bones': len(selection['selected']), 'weighted_bones': len(selection['weighted']),
    }
    (output / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
    print(json.dumps(provenance, indent=2))


if __name__ == '__main__':
    main()
