"""Moving-part detection rules of the Blender bridge against the fixture shared
with secondary_bones.rs and src/preparation/secondary-bones.ts.

Run with Blender (the bridge module needs Blender's NumPy):
  blender --background --factory-startup --python-exit-code 23 --python scripts/test_blender_moving_parts.py
"""
import json, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
from secondary_bones import physics_groups, breast_bones

data=json.loads((ROOT/'src-tauri/crates/deadlock-preparation/tests/fixtures/hanging-chains.json').read_text(encoding='utf-8'))
bones,weighted,roles=data['bones'],data['weighted'],data['roles']

def roots(breasts):
    return [g['root'] for g in physics_groups(bones,weighted,roles,roles,breasts)]

assert roots(None)==data['expected_roots'],roots(None)
assert roots(False)==data['expected_roots'],roots(False)
assert roots(True)==data['expected_roots_with_breast_motion'],roots(True)
left=next(g for g in physics_groups(bones,weighted,roles,roles,True) if g['root']=='Bone_Breast_L_00')
assert (left['source_anchor'],len(left['source_bones']))==('Bip001_Spine1',1),left
assert breast_bones(bones,weighted,roles)==data['breast_bones']
# Without breast names the mirrored pair in front of the chest is found by position.
renamed=data['renamed_breast_bones']
generic=[{**b,'name':renamed.get(b['name'],b['name'])} for b in bones]
assert breast_bones(generic,[renamed.get(n,n) for n in weighted],roles)==[renamed[n] for n in data['breast_bones']]
print('moving-part rules: ok')
