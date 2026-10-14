"""Builds resources/blender/cjk_readings.txt from Unihan.zip (kMandarin, first
reading, tone marks removed). Lines: `reading<TAB>characters` grouped by reading.

    python scripts/make_cjk_readings.py Unihan.zip src-tauri/crates/deadlock-preparation/resources/blender/cjk_readings.txt
"""
import sys, zipfile, unicodedata
from collections import defaultdict

source, out = sys.argv[1], sys.argv[2]
text = zipfile.ZipFile(source).read('Unihan_Readings.txt').decode('utf-8')
version = next(l for l in text.splitlines() if l.startswith('# Unicode Version')).split('Version')[1].strip()


def plain(reading):
    reading = reading.replace('ü', 'v').replace('Ü', 'v')
    letters = ''.join(c for c in unicodedata.normalize('NFKD', reading) if not unicodedata.combining(c))
    return ''.join(c for c in letters.lower() if 'a' <= c <= 'z')


groups = defaultdict(list)
for line in text.splitlines():
    if not line or line.startswith('#'):
        continue
    code, field, value = line.split('\t', 2)
    if field != 'kMandarin':
        continue
    reading = plain(value.split()[0])
    if reading:
        groups[reading].append(chr(int(code[2:], 16)))

with open(out, 'w', encoding='utf-8', newline='\n') as f:
    f.write(f'# Mandarin readings (pinyin without tones) of CJK ideographs, from the Unicode Unihan database {version} (kMandarin, first reading).\n')
    f.write('# Copyright (c) 1991-2026 Unicode, Inc. Unicode License v3: licenses/Unicode-Unihan.txt. Rebuild: scripts/make_cjk_readings.py.\n')
    for reading in sorted(groups):
        f.write(reading + '\t' + ''.join(groups[reading]) + '\n')
print(version, len(groups), sum(len(v) for v in groups.values()))
