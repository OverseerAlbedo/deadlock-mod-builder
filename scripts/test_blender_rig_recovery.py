"""Native recovery and conversion proof on an optional private source.

DMB_RECOVERY_SOURCE is read only. DMB_RECOVERY_MAPPING is mapping_report's
native Rust output for that inspected source and the positional Haze fixture.
Missing external textures are replaced only in disposable test scenes. This
does not execute a Valve compiler or establish game/texture acceptance.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
import numpy as np
import bpy

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT/'scripts'),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
from worker import import_source,describe,prepare
from source_compatibility import normalize, POLICY
from secondary_bones import physics_groups
from static_face import apply_static_shapes

source = Path(os.environ['DMB_RECOVERY_SOURCE'])
mapping_path = Path(os.environ['DMB_RECOVERY_MAPPING'])
out = Path(os.environ['DMB_PREP_TEST_OUTPUT']);out.mkdir(parents=True,exist_ok=True)
digest = hashlib.sha256(source.read_bytes()).hexdigest()
suggestions = json.loads(mapping_path.read_text())
assert not suggestions['source']['errors'] and not suggestions['target']['errors']


def capture(meshes,rig):
    return {'bones':{b.name:{'matrix':[list(r) for r in b.matrix_local],'parent':b.parent.name if b.parent else None,'deform':b.use_deform} for b in rig.data.bones},
            'meshes':{o.name:{'points':[list(v.co) for v in o.data.vertices],
                             'weights':[[(o.vertex_groups[g.group].name,g.weight) for g in v.groups] for v in o.data.vertices],
                             'uvs':[[list(v.uv) for v in u.data] for u in o.data.uv_layers],
                             'faces':[list(p.vertices) for p in o.data.polygons]} for o in meshes}}


def target_fixture():
    rows=json.loads((ROOT/'src-tauri/crates/deadlock-preparation/tests/fixtures/tangtang-rig.json').read_text())['target']
    worlds={}
    for b in rows:
        m=np.eye(4);m[:3,3]=b['position'];worlds[b['name']]=m
    for b in rows:
        world=worlds[b['name']];local=np.linalg.inv(worlds[b['parent']])@world if b['parent'] else world
        b.update(local_position=local[:3,3].tolist(),local_rotation=[0.,0.,0.,1.],local_scale=1.,
                 matrix=world.tolist(),inverse_bind_matrix=np.linalg.inv(world).tolist(),length=1.)
    return rows


receipts=[]
meshes,rigs=import_source(str(source),allow_shape_keys=True,helper_reports=receipts)
before=capture(meshes,rigs[0]);info=describe(meshes,rigs)
assert capture(meshes,rigs[0])==before,'Inspection must preserve exact Blender data'
assert not info['humanoid']['errors'] and info['humanoid']['roles']==suggestions['source']['roles']
recovery=next(r for r in receipts if r['policy']==POLICY)
assert recovery['original_bind_matrices_unchanged']
assert len(recovery['reconnected_roots'])==14 and len(recovery['promoted_helpers'])==5
saved=out/'recovered-intake.blend'
bpy.ops.wm.save_as_mainfile(filepath=str(saved),copy=True)
meshes,rigs=import_source(str(saved),allow_shape_keys=True)
assert capture(meshes,rigs[0])==before
assert describe(meshes,rigs)['bones']==info['bones']

# Live source controls keep the unresolved original hierarchy instead of
# guessing how it should behave. Removing the temporary control restores recovery.
c=rigs[0].pose.bones[suggestions['source']['roles']['head']].constraints.new('LIMIT_ROTATION')
assert normalize(rigs[0],meshes) is None
rigs[0].pose.bones[suggestions['source']['roles']['head']].constraints.remove(c)
assert normalize(rigs[0],meshes) is not None

results=[]
for fit in ['match_target','preserve_proportions']:
    meshes,rigs=import_source(str(source),allow_shape_keys=True)
    info=describe(meshes,rigs)
    options={'source_roles':suggestions['source']['roles'],'target_roles':suggestions['target']['roles'],
             'mapping':copy.deepcopy(suggestions['mapping']),'mapping_anchors':copy.deepcopy(suggestions['assistance']['anchors']),
             'reviewed':True,'follow_motion_reviewed':True,'fit_mode':fit,'fit_arm_joints':True,'arm_skinning':'smooth_twist'}
    groups=physics_groups(info['bones'],info['weighted_bones'],options['source_roles'],options['target_roles'])
    options.update(preserve_physics_bones=bool(groups),physics_roots=[g['root'] for g in groups])
    kept={n for g in groups for n in g['source_bones']}
    for entry in suggestions['assistance']['entries']:
        name=entry['source']
        if name not in kept and name not in options['mapping']:
            follow=entry['follow'];options['mapping'][name]=follow['target'];options['mapping_anchors'][name]=follow['anchor']
    assert all(n in options['mapping'] or n in kept for n in info['weighted_bones'])
    # This uploaded source omits its external texture folder. Test skeleton and
    # real geometry conversion using temporary generated material colors only.
    for material in bpy.data.materials:
        if material.node_tree:material.node_tree.nodes.clear()
        material.use_nodes=False
    apply_static_shapes(meshes,'current_static')
    work=out/fit;work.mkdir(exist_ok=True)
    target=target_fixture()
    result=prepare({'source':str(source),'mode':'rigged_humanoid','options':options,'target':target,
                    'workspace':str(work),'hero_root':'models/test/recovery'},meshes,rigs)
    assert result['target'][:len(target)]==target,'Hero bind skeleton changed'
    assert {m['name'] for m in result['meshes']}=={'Body','Face'}
    for mesh in result['meshes']:
        assert np.isfinite(mesh['positions']).all()
        assert all(abs(sum(w for _,w in row)-1)<1e-8 for row in mesh['weights'])
        assert all(c['finite'] for c in mesh['deformation'])
    (work/'result.json').write_text(json.dumps(result,allow_nan=False))
    # The correction handoff has the target's real hierarchy and exact bind
    # receipt; no source recovery runs on this already prepared model.
    corrected,corrected_rigs=import_source(result['blend'],allow_shape_keys=True)
    assert normalize(corrected_rigs[0],corrected) is None
    correction=copy.deepcopy(options)
    correction.update(source_roles=options['target_roles'],mapping={b['name']:b['name'] for b in result['target']},mapping_anchors={},preserve_physics_bones=False,physics_roots=[])
    corrected_out=work/'correction';corrected_out.mkdir(exist_ok=True)
    replay=prepare({'source':result['blend'],'mode':'rigged_humanoid','correction':True,'options':correction,
                    'target':result['target'],'workspace':str(corrected_out),'hero_root':'models/test/recovery'},corrected,corrected_rigs)
    assert replay['target']==result['target']
    results.append({'fit':fit,'vertices':sum(len(m['positions']) for m in result['meshes']),
                    'weighted_bones':len(info['weighted_bones']),'preserved_secondary_bones':len(kept),
                    'followed_bones':len(options['mapping_anchors']),'hero_binds_unchanged':True,
                    'correction_handoff_passed':True,'warnings':result['warnings']})
assert hashlib.sha256(source.read_bytes()).hexdigest()==digest
evidence={'blender':bpy.app.version_string,'source_sha256':digest,'source_unchanged':True,
          'inspection_exact_data_unchanged':True,'saved_intake_reimport_passed':True,'live_controls_not_repaired':True,
          'recovery':recovery,'conversions':results,'materials':'temporary generated colors; external textures not supplied',
          'target':'positional Haze regression fixture; not a current compiled hero','native_game_compiler_run':False}
(out/'rig-recovery-results.json').write_text(json.dumps(evidence,indent=2)+'\n')
print(json.dumps(evidence,indent=2))
