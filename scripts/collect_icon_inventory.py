"""Collect a compact, reproducible icon audit from decompiled game definitions.

Usage: python scripts/collect_icon_inventory.py HEROES ABILITIES PAK_LIST OUTPUT
Inputs are the text files produced by Source 2 Viewer / GameTracking-Deadlock.
Only model identities, image paths and ability links are retained, not artwork.
"""
import hashlib
import json
import re
import sys
from pathlib import Path


def collect(hero_file, ability_file, listing_file):
    def records(path):
        text = path.read_text(encoding='utf-8-sig')
        return dict(re.findall(r'^\t(\w+) =\s*\n\t\{\n(.*?)^\t\}', text, re.M | re.S))

    heroes, abilities = records(hero_file), records(ability_file)
    if len(heroes) < 20 or len(abilities) < 100:
        raise ValueError('Expected complete decompiled game definition tables.')
    def images(body):
        return sorted(set(re.findall(r'"(file://\{images\}/[^"\n]+)"', body)))
    out_heroes = {}
    for key, body in heroes.items():
        model = re.search(r'm_strModelName = (?:resource_name:)?"([^"]+)"', body)
        if not model or key == 'hero_base':
            continue
        bound = re.search(r'\t\tm_mapBoundAbilities =\s*\n\t\t\{(.*?)\n\t\t\}', body, re.S)
        links = re.findall(r'(\w+) = "([^"]+)"', bound.group(1)) if bound else []
        out_heroes[key] = {'m_strModelName': model.group(1), 'm_mapBoundAbilities': dict(links), 'images': images(body)}
    out_abilities = {key: {'images': images(body), 'links': sorted(set(re.findall(r'"([^"\n]+)"', body)) & abilities.keys())}
                     for key, body in abilities.items()}
    paths = sorted({line.split(' CRC:')[0] for line in listing_file.read_text().splitlines()
                    if line.startswith('panorama/images/')})
    return {'source_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in (hero_file, ability_file, listing_file)},
            'heroes': out_heroes, 'abilities': out_abilities, 'images': paths}


if __name__ == '__main__':
    if len(sys.argv) != 5:
        raise SystemExit(__doc__)
    result = collect(*(Path(x) for x in sys.argv[1:4]))
    Path(sys.argv[4]).write_text(json.dumps(result, indent=2) + '\n')
    print(f"Collected {len(result['heroes'])} hero definitions and {len(result['images'])} Panorama image paths.")
