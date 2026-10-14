"""Checks the readable spellings of non-English material names (readable_names.py)
and the prepared material slug built from them. Plain Python; no Blender needed:

    python scripts/test_readable_names.py
"""
import hashlib, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'src-tauri/crates/deadlock-preparation/resources/blender')]
from readable_names import readable
from material_textures import NAME_CHARS, prepared_slug

CASES = [
    # ASCII names keep exactly the prepared name they had.
    ('Hair_01', 'Hair_01'),
    ('Material #1.002', 'Material #1.002'),
    # Chinese (and kanji) by Mandarin reading, one word per run.
    ('头发', 'toufa'),
    ('衣服_02', 'yifu_02'),
    ('Hair头发', 'Hair_toufa'),
    ('皮肤2', 'pifu_2'),
    # Kana in Hepburn: long vowels, small kana and doubled consonants.
    ('スカート', 'sukaato'),
    ('シャツ', 'shatsu'),
    ('きゃく', 'kyaku'),
    ('マッチ', 'matchi'),
    ('フィルム', 'firumu'),
    ('ｽｶｰﾄ', 'sukaato'),
    ('髪の毛', 'fa_no_mao'),
    # Hangul by syllable name, accents dropped, full-width letters folded.
    ('머리', 'meori'),
    ('Pelé', 'Pele'),
    ('ＡＢＣ１', 'ABC1'),
    ('★', '_'),
]

failures = [f'{name!r}: {readable(name)!r} != {expected!r}' for name, expected in CASES if readable(name) != expected]
# The slug: readable part, cut to NAME_CHARS, then the hash of the original name.
old = lambda name: re.sub('[^a-zA-Z0-9_-]', '_', name).lower()[:NAME_CHARS] + '_' + hashlib.sha256(name.encode()).hexdigest()[:8]
for name in ['Hair_01', 'Material #1.002', 'Body.001 (skin)']:
    if prepared_slug(name) != old(name):
        failures.append(f'ASCII slug changed: {name!r}')
slug = prepared_slug('头发')
if slug != 'toufa_' + hashlib.sha256('头发'.encode()).hexdigest()[:8]:
    failures.append(f'slug {slug!r}')
if len(prepared_slug('衣服衣服衣服衣服衣服衣服衣服衣服').rsplit('_', 1)[0]) != NAME_CHARS:
    failures.append('long names are not cut to NAME_CHARS')
if failures:
    print('\n'.join(failures))
    sys.exit(1)
print(f'readable names: {len(CASES)} cases ok')
