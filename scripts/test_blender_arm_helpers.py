"""Verify exported helper skin weights respond to their own animated joints.

Run with Blender via run_script.py, after test_blender_proportions.py. These
isolated joint probes are not the game's animation graph or artistic approval.
"""
import argparse
import json
from pathlib import Path
import bpy
import numpy as np
from mathutils import Matrix,Quaternion

p=argparse.ArgumentParser(__doc__);p.add_argument('--checks',type=Path,required=True);a=p.parse_args()
folder=a.checks/'blender/tangtang';report=json.loads((folder/'result.json').read_text())
bpy.ops.wm.open_mainfile(filepath=str(folder/'prepared.blend'),load_ui=False,use_scripts=False)
rig=next(o for o in bpy.context.scene.objects if o.type=='ARMATURE')
objects={o.name:o for o in bpy.context.scene.objects if o.type=='MESH'}
def evaluated(o):
 bpy.context.view_layer.update();e=o.evaluated_get(bpy.context.evaluated_depsgraph_get());m=e.to_mesh()
 try:return np.array([e.matrix_world@v.co for v in m.vertices])
 finally:e.to_mesh_clear()
baseline={n:evaluated(o) for n,o in objects.items()};probes=[]
for name in sorted({v['target'] for v in report['rest_alignment']['arm_deformation'].values()}):
 branch={name}
 while True:
  expanded=branch|{b['name'] for b in report['target'] if b['parent'] in branch}
  if expanded==branch:break
  branch=expanded
 rig.pose.bones[name].rotation_mode='QUATERNION';rig.pose.bones[name].rotation_quaternion=Quaternion((0,1,0),.25)
 moved=weighted=0
 for mesh in report['meshes']:
  current=evaluated(objects[mesh['name']]);old=baseline[mesh['name']]
  assert np.isfinite(current).all()
  mask=np.array([sum(w for n,w in row if n in branch)>0 for row in mesh['weights']])
  np.testing.assert_allclose(current[~mask],old[~mask],atol=1e-5,rtol=0)
  moved+=int((np.linalg.norm(current-old,axis=1)>1e-5).sum());weighted+=int(mask.sum())
 assert weighted>0 and moved>0,name
 rig.pose.bones[name].matrix_basis=Matrix.Identity(4)
 for n,o in objects.items():np.testing.assert_allclose(evaluated(o),baseline[n],atol=1e-5,rtol=0)
 probes.append({'target':name,'weighted_vertices':weighted,'moved_vertices':moved,'unrelated_vertices_unchanged':True})
result={'blender':bpy.app.version_string,'helper_probes':probes,'all_passed':True,'in_game_tested':False}
(a.checks/'blender-arm-probes.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
