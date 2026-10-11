"""Real Blender integration for static facial baking and scaled rigid props.

Run: blender --background --factory-startup --disable-autoexec
     --python-exit-code 23 --python scripts/test_blender_character_equipment.py

Generated fixtures only. Does not claim Valve compiler/in-game validation.
"""
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(ROOT),str(ROOT/'scripts'),str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
import bpy
import numpy as np
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import skeleton
from test_blender_preparation import fixture
from worker import import_source, describe, prepare
from algorithms import suggest_mapping, identify
from static_face import apply_static_face
from deadlock_builder.preparation.target_skeleton import prepare_target
from deadlock_builder.preparation.export import export_dmx
from deadlock_builder.customization.equipment import prop_matrix

OUT=Path(os.environ.get('DMB_EQUIPMENT_TEST_OUTPUT',str(ROOT/'build/blender-equipment-test'))).resolve()
OUT.mkdir(parents=True,exist_ok=True)

def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    results=[]
    for rigged in (True,False):
        directory=OUT/('rigged' if rigged else 'unrigged');directory.mkdir(exist_ok=True)
        bpy.ops.wm.read_factory_settings(use_empty=True)
        data=bpy.data.meshes.new('charm');data.from_pydata([(0,0,0),(.1,0,0),(0,.1,0)],[],[(0,1,2)])
        obj=bpy.data.objects.new('charm',data);bpy.context.collection.objects.link(obj)
        obj.data.materials.append(bpy.data.materials.new('charm metal'))
        prop_source=directory/'charm.blend';bpy.ops.wm.save_as_mainfile(filepath=str(prop_source))
        prop_hash=sha(prop_source)
        meshes=fixture(rigged=rigged);body=meshes[0]
        basis=body.shape_key_add(name='Basis');smile=body.shape_key_add(name='Smile')
        for point in smile.data:
            if point.co.z>1.7:point.co.y += .025
        source=directory/'character.blend';bpy.ops.wm.save_as_mainfile(filepath=str(source));source_hash=sha(source)
        meshes,rigs=import_source(str(source),allow_shape_keys=True);info=describe(meshes,rigs)
        assert info['morphs']==[{'name':'body/Smile','mesh':'body','shape':'Smile'}]
        face=apply_static_face(meshes,{'enabled':True,'mode':'static','weights':{'body/Smile':.5}})
        assert face and not meshes[0].data.shape_keys
        sk=skeleton();sk['m_boneScaleParent'][0]=1.001;sk['m_boneScaleParent'][1]=.9998
        target,_=prepare_target(sk)
        suggestion=suggest_mapping(info['bones'],target) if rigged else {'target':identify(target)}
        options={'reviewed':True,'target_roles':suggestion['target']['roles'],'influence_limit':4}
        if rigged:options.update(mapping=suggestion['mapping'],source_roles=suggestion['source']['roles'])
        else:options['landmarks']=info['humanoid']['landmarks']
        prop={'source':str(prop_source),'bone':'head','translation':[.2,0,0],'rotation':[0,0,45],'scale':1.5}
        job={'mode':'rigged_humanoid' if rigged else 'unrigged_humanoid','target':target,'options':options,
             'hero_root':'models/test','workspace':str(directory),'props':[prop]}
        report=prepare(job,meshes,rigs)
        attached=[mesh for mesh in report['meshes'] if mesh.get('attachment')]
        assert len(attached)==1 and attached[0]['weights']==[[('head',1.)]]*3
        expected=(prop_matrix(target,prop)@np.array([0,0,0,1]))[:3]
        np.testing.assert_allclose(attached[0]['positions'][0],expected,atol=1e-7)
        export=export_dmx(report,sk,directory/'prepared.dmx')
        assert export['target_bind_audit']['world_inverse_bind_consistent']
        assert sha(source)==source_hash and sha(prop_source)==prop_hash
        results.append({'mode':job['mode'],'status':'PASS','static_face':face,
                        'prop_meshes':[mesh['name'] for mesh in attached], 'target_bind_audit':export['target_bind_audit'],
                        'native_compiler_validated':False,'in_game_validated':False})
    (OUT/'result.json').write_text(json.dumps({'cases':results},indent=2),encoding='utf-8')
    print('PASS: static facial baking and rigid props with inherited target bone scales, rigged and unrigged.')

if __name__=='__main__':main()
