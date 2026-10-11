"""Different hair rigs with independently specified equivalent profile locations."""
from pathlib import Path
import json
import sys

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.preparation.export import export_dmx

OUT = Path(__file__).parent / 'fixtures/hair_motion'
OUT.mkdir(exist_ok=True)
prefix = 'dmb_secondary_'
names, parents, positions = ['head'], [-1], [[0, 0, 0]]
world = {'head': [0, 0, 0]}
meshes, roots, expected = [], [], {}

def group(rows):
    roots.append(rows[0][0])
    for name, parent, position, t in rows:
        # The proxy ring uses local X/Z; use a Y-directed chain like Blender.
        position = [position[0], position[2], position[1]]
        full = prefix + name
        parent = prefix + parent if parent != 'head' else parent
        names.append(full)
        parents.append(names.index(parent))
        positions.append([a-b for a, b in zip(position, world[parent])])
        world[full] = position
        if t is None:
            continue
        expected[full] = t
        # Each weighted joint owns a distinct triangle, independent of helpers.
        meshes.append({'name': name, 'positions': [position, [position[0]+.2, *position[1:]],
                       [position[0], position[1]+.2, position[2]]],
            'normals': [[0, 0, 1]]*3, 'uvs': [[[0, 0]]*3], 'colors': [],
            'weights': [[[full, 1.]]]*3,
            'faces': [{'vertices': [0, 1, 2], 'corners': [0, 1, 2], 'material': 'materials/hair.vmat'}],
            'deformation': []})

group([('hair_short_0', 'head', [0, 0, 12], 0.),
       ('hair_short_1', 'hair_short_0', [0, 0, 10], 1.)])
group([('bang_short_0', 'head', [4, 0, 12], 0.),
       ('bang_short_1', 'bang_short_0', [4, 0, 11], .5),
       ('bang_short_2', 'bang_short_1', [4, 0, 10], 1.)])
# Generic imported root, leading/intermediate/trailing helpers, 10x longer.
group([('spring_imported', 'head', [8, 0, 14], None),
       ('helper_leading', 'spring_imported', [8, 0, 13], None),
       ('ponytail_0', 'helper_leading', [8, 0, 12], 0.),
       ('helper_between_0', 'ponytail_0', [8, 0, 7], None),
       ('ponytail_1', 'helper_between_0', [8, 0, 2], .5),
       ('helper_between_1', 'ponytail_1', [8, 0, -3], None),
       ('ponytail_2', 'helper_between_1', [8, 0, -8], 1.),
       ('helper_end', 'ponytail_2', [8, 0, -80], None)])
# Tip curls toward the root: straight-line distance gives the wrong ordering.
group([('hair_curved_0', 'head', [12, 0, 12], 0.),
       ('hair_curved_1', 'hair_curved_0', [12, 0, 2], .5),
       ('hair_curved_2', 'hair_curved_1', [12, 6, 10], 1.)])
group([('hair_dense_0', 'head', [16, 0, 12], 0.),
       ('hair_dense_1', 'hair_dense_0', [16, 0, 7], .25),
       ('hair_dense_2', 'hair_dense_1', [16, 0, 2], .5),
       ('hair_dense_3', 'hair_dense_2', [16, 0, -3], .75),
       ('hair_dense_4', 'hair_dense_3', [16, 0, -8], 1.)])
group([('hair_single', 'head', [20, 0, 12], 0.)])
group([('hair_branch', 'head', [24, 0, 12], 0.),
       ('hair_branch_left', 'hair_branch', [23, 0, 10], 1.),
       ('hair_branch_right', 'hair_branch', [25, 0, 10], 1.)])
skeleton = {'m_boneName': names, 'm_nParent': parents, 'm_bonePosParent': positions,
            'm_boneRotParent': [[0, 0, 0, 1]]*len(names),
            'm_boneScaleParent': [1.]*len(names), 'm_nFlag': [0]*len(names)}
export_dmx({'meshes': meshes, 'materials': {}}, skeleton, OUT/'mixed.dmx')
(OUT/'oracle.json').write_text(json.dumps({'roots': roots, 'weighted_coordinates': expected}, indent=2)+'\n')
