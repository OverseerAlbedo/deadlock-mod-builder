"""Replay reviewed mapping on a supplied model without changing the source.

Required environment: DMB_MAPPING_SOURCE, DMB_MAPPING_FIXTURE (target bones),
DMB_MAPPING_OPTIONS, DMB_MAPPING_OUTPUT. Set DMB_MAPPING_GEOMETRY_ONLY=1 only
for a diagnostic when external textures were not supplied. This exception is
confined to this test script; production imports still report missing assets.
"""
from pathlib import Path
import hashlib
import json
import os
import sys
import bpy

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src-tauri/crates/deadlock-preparation/resources/blender'))
import worker

source = Path(os.environ['DMB_MAPPING_SOURCE'])
output = Path(os.environ['DMB_MAPPING_OUTPUT'])
output.mkdir(parents=True, exist_ok=True)
fixture = json.loads(Path(os.environ['DMB_MAPPING_FIXTURE']).read_text())
options = json.loads(Path(os.environ['DMB_MAPPING_OPTIONS']).read_text())
digest = hashlib.sha256(source.read_bytes()).hexdigest()
helpers = []
meshes, rigs = worker.import_source(str(source), allow_shape_keys=True, helper_reports=helpers)
info = worker.describe(meshes, rigs)
shapes = worker.apply_static_shapes(meshes, 'current_static')
missing = []
geometry_only = os.environ.get('DMB_MAPPING_GEOMETRY_ONLY') == '1'
if geometry_only:
    for material in bpy.data.materials:
        if material.node_tree:
            for node in list(material.node_tree.nodes):
                if (node.type == 'TEX_IMAGE' and node.image and not node.image.packed_file
                        and node.image.source == 'FILE'
                        and not Path(bpy.path.abspath(node.image.filepath)).is_file()):
                    missing.append(bpy.path.abspath(node.image.filepath))
                    material.node_tree.nodes.remove(node)
result = worker.prepare({'source': str(source), 'mode': 'rigged_humanoid',
    'options': options, 'target': fixture['target'], 'workspace': str(output),
    'hero_root': 'models/test/captured'}, meshes, rigs)
(output / 'result.json').write_text(json.dumps(result, ensure_ascii=False))
assert hashlib.sha256(source.read_bytes()).hexdigest() == digest
summary = {'blender': bpy.app.version_string, 'source_sha256': digest,
    'source_unchanged': True, 'input_weighted_bones': len(info['weighted_bones']),
    'meshes': len(result['meshes']),
    'vertices': sum(len(m['positions']) for m in result['meshes']),
    'preserved': len(result.get('preserved_physics_bones', {}).get('exported_bones', {})),
    'static_shapes': shapes, 'helper_reports': helpers,
    'missing_external_textures': sorted(set(missing)), 'geometry_only_diagnostic': geometry_only}
(output / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps(summary, ensure_ascii=False))
