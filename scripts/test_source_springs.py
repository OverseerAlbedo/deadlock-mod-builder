"""Checks the MMD/VRM spring conversion and the .vrm/.glb spring reader
(source_springs.py). Plain Python; no Blender needed:

    python scripts/test_source_springs.py
"""
import json, struct, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'src-tauri/crates/deadlock-preparation/resources/blender')]
from source_springs import gltf_springs, mmd_values, vrm_values

failures = []
def check(label, actual, expected):
    if actual != expected:
        failures.append(f'{label}: {actual!r} != {expected!r}')

check('vrm usual', vrm_values(1.0, 0.4, 0.0), {'source': 'vrm', 'goal': 0.45, 'drag': 0.2, 'gravity': 0.0})
check('vrm limits', vrm_values(4.0, 1.0, 3.0), {'source': 'vrm', 'goal': 0.95, 'drag': 0.5, 'gravity': 1.5})
# MMD damping is per second (Bullet); springs span 0 to 100,000 (a locked joint).
check('mmd hair', mmd_values(0.5, 0.0), {'source': 'mmd', 'goal': 0.05, 'drag': 0.0228, 'gravity': 1.0})
check('mmd damped', mmd_values(0.99, 100.0), {'source': 'mmd', 'goal': 0.3506, 'drag': 0.1423, 'gravity': 1.0})
check('mmd locked', mmd_values(2.0, 100000.0)['goal'], 0.8)

def glb(doc):
    body = json.dumps(doc).encode()
    body += b' ' * (-len(body) % 4)
    return b'glTF' + struct.pack('<II', 2, 12 + 8 + len(body)) + struct.pack('<II', len(body), 0x4E4F534A) + body

doc = {'nodes': [{'name': 'Hips', 'children': [1, 3]}, {'name': 'hair_a', 'children': [2]}, {'name': 'hair_b'}, {'name': 'tail'}],
       'extensions': {
           'VRMC_springBone': {'springs': [{'joints': [{'node': 3, 'stiffness': 2.0, 'dragForce': 0.2, 'gravityPower': 1.0}]}]},
           'VRM': {'secondaryAnimation': {'boneGroups': [{'stiffiness': 0.5, 'dragForce': 0.6, 'gravityPower': 0.2, 'bones': [1]}]}}}}
with tempfile.TemporaryDirectory() as folder:
    path = Path(folder) / 'model.vrm'
    path.write_bytes(glb(doc))
    springs = gltf_springs(path)
    check('vrm1 joint', springs.get('tail'), vrm_values(2.0, 0.2, 1.0))
    # A VRM 0.x group applies to the whole chain below its bones.
    check('vrm0 chain', [springs.get('hair_a'), springs.get('hair_b')], [vrm_values(0.5, 0.6, 0.2)] * 2)
    check('no springs', 'Hips' in springs, False)
    (Path(folder) / 'broken.glb').write_bytes(b'glTF')
    check('unreadable file', gltf_springs(Path(folder) / 'broken.glb'), {})
if failures:
    print('\n'.join(failures))
    sys.exit(1)
print('source springs: ok')
