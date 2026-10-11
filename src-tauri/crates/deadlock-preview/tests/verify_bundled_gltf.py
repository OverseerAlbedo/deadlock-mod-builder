"""Compare Rust-bundled GLB playback inputs with the legacy Python pose oracle.
Run after cargo build -p deadlock-preview --example bundle_gltf.
"""
import json, struct, subprocess, sys, tempfile
from pathlib import Path
import numpy as np
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.preview import load_scene
FIXTURES=Path(__file__).with_name('fixtures')/'gltf'
ORACLE=json.loads((FIXTURES/'oracle.json').read_text())
EXE=ROOT/'src-tauri/target/debug/examples/bundle_gltf'
results=[]
with tempfile.TemporaryDirectory() as tmp:
    tmp=Path(tmp)
    for name,want in ORACLE.items():
        packet=tmp/'preview.bin';subprocess.run([str(EXE),str(FIXTURES/name),str(packet)],check=True)
        raw=packet.read_bytes();assert raw[:4]==b'DMBG';length=struct.unpack_from('<I',raw,8)[0]
        glb=tmp/'preview.glb';glb.write_bytes(raw[16+((length+3)&~3):])
        scene=load_scene(glb)
        errors=[]
        for expected in want['poses']:
            pose=scene.pose(expected['time'],0 if scene.animations else None)
            for got,p,n in zip(pose.meshes,expected['positions'],expected['normals']):
                # Native bundling does not edit any accessor values. Floating
                # quaternion canonicalization may move a final f32 by one ULP.
                np.testing.assert_allclose(got.positions,p,atol=1e-6,rtol=1e-6)
                np.testing.assert_allclose(got.normals,n,atol=1e-6,rtol=1e-6)
                errors.append(float(np.max(np.abs(got.positions-np.asarray(p)))))
                errors.append(float(np.max(np.abs(got.normals-np.asarray(n)))))
        results.append({'source':name,'poses':len(want['poses']),'maximum_absolute_difference':max(errors),'passed':True})
report={'scope':'Python scene poses of Rust self-contained bundled GLBs; no GPU or native Blender execution','cases':results}
output=ROOT/'docs/rust-migration/evidence/preview-gltf-bundling.json';output.parent.mkdir(exist_ok=True,parents=True);output.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
