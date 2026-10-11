"""End-to-end probe for "almost any rigged humanoid" and gun customization.

Builds deliberately awkward source files (odd bone names, cluttered scenes,
posed rigs, constraint-attached guns, Y-up exports, shape keys under
modifiers, collection instances, curve hair, FBX/GLB round trips), then runs
the real bridge: inspect -> native orientation/role/mapping/weapon planning ->
prepare -> native DMX export validation against a captured 196-bone hero.

Run with Blender:
    blender --background --factory-startup --python scripts/test_blender_any_humanoid.py -- OUTPUT_DIR
or with the bpy module:
    python scripts/test_blender_any_humanoid.py OUTPUT_DIR
First build the native probe:
    cargo build --manifest-path src-tauri/Cargo.toml -p deadlock-preparation --example e2e_probe
Optional: DMB_E2E_PROBE=path/to/e2e_probe, DMB_BLENDER=path/to/blender (bridge runs in that Blender).
"""
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import bpy  # noqa: E402
import blender_fixtures as F  # noqa: E402

WORKER = ROOT / 'src-tauri/crates/deadlock-preparation/resources/blender/worker.py'
TARGET = ROOT / 'src-tauri/crates/deadlock-preparation/tests/fixtures/daki-rig.json'
args = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else sys.argv[1:]
OUT = Path(args[0] if args else os.environ.get('DMB_PREP_TEST_OUTPUT', 'dmb-any-humanoid')).resolve()
OUT.mkdir(parents=True, exist_ok=True)
PROBE = Path(os.environ.get('DMB_E2E_PROBE') or ROOT / 'src-tauri/target/debug/examples' / ('e2e_probe.exe' if os.name == 'nt' else 'e2e_probe'))
if not PROBE.is_file():
    raise SystemExit('Build the native probe first: cargo build --manifest-path src-tauri/Cargo.toml -p deadlock-preparation --example e2e_probe')


def bridge(job_path):
    """Run the embedded worker exactly as the app does (separate process)."""
    blender = os.environ.get('DMB_BLENDER') or (bpy.app.binary_path if Path(bpy.app.binary_path or '').name.lower().startswith('blender') else '')
    if blender:
        command = [blender, '--background', '--factory-startup', '--disable-autoexec', '--python-exit-code', '23',
                   '--python', str(WORKER), '--', str(job_path)]
    else:
        runner = OUT / 'run_worker.py'
        runner.write_text('import runpy,sys\njob,worker=sys.argv[1],sys.argv[2]\nsys.argv=["blender","--",job]\nrunpy.run_path(worker,run_name="__main__")\n')
        command = [sys.executable, str(runner), str(job_path), str(WORKER)]
    log = Path(job_path).with_name('blender.log')
    with open(log, 'w', encoding='utf-8') as handle:
        subprocess.run(command, stdout=handle, stderr=subprocess.STDOUT, check=False)
    return json.loads(Path(job_path).with_name('result.json').read_text(encoding='utf-8'))


def probe(*arguments):
    return subprocess.run([str(PROBE), *map(str, arguments)], capture_output=True, text=True, check=True).stdout.strip()


def inspect(source, folder, rotation=(0, 0, 0)):
    folder.mkdir(parents=True, exist_ok=True)
    job = folder / 'job.json'
    job.write_text(json.dumps({'operation': 'inspect', 'source': str(source), 'rotation': list(rotation),
                               'result': str(folder / 'result.json'), 'workspace': str(folder)}))
    return bridge(job)


def run_case(name, source, mode='hero', action='none', fit='match_target', shape='', expect_rotation=None):
    folder = OUT / name
    report = inspect(source, folder / 'inspect')
    assert report['status'] == 'ok', (name, report.get('error'))
    rotation = json.loads(probe('orient', folder / 'inspect' / 'result.json'))
    if expect_rotation is not None:
        assert rotation == expect_rotation, (name, rotation)
    if rotation:
        report = inspect(source, folder / 'upright', rotation)
        assert report['status'] == 'ok', (name, report.get('error'))
        inspected = folder / 'upright' / 'result.json'
    else:
        inspected = folder / 'inspect' / 'result.json'
    workspace = folder / 'prepare'
    job = probe('plan', inspected, TARGET, workspace, mode, action, fit, shape)
    prepared = bridge(job)
    assert prepared['status'] == 'ok', (name, prepared.get('error'))
    dmx = workspace / 'prepared.dmx'
    if dmx.exists():
        dmx.unlink()
    validated = probe('validate', inspected, TARGET, workspace / 'options.json', workspace / 'result.json', dmx)
    return report, prepared, validated


cases = []


def build(name, make):
    F.clear()
    make()
    path = OUT / 'sources' / (name + '.blend')
    path.parent.mkdir(parents=True, exist_ok=True)
    F.save(path)
    return path


def messy_gun(style):
    rig, body, table = F.humanoid(style)
    F.clutter(rig, body, table)
    gun = F.pistol()
    F.bone_parent(gun, rig, table['hand_R'])


def posed_constraint_gun():
    rig, body, table = F.humanoid('mixamo')
    F.pose_bone(rig, table['forearm_R'], 40, 'Z')
    hand = rig.matrix_world @ rig.pose.bones[table['hand_R']].matrix
    gun = F.pistol(grip=tuple(hand.translation))
    F.child_of(gun, rig, table['hand_R'])


def laid_down():
    F.humanoid('generic')
    F.lay_down()


def shapes():
    rig, body, table = F.humanoid('mixamo')
    F.shape_keys_and_subsurf(body)


def hair_curve():
    rig, body, table = F.humanoid('rigify')
    F.curve_hair(rig, table['head'])


def instanced():
    rig, body, table = F.humanoid('cc')
    F.into_collection_instance([rig, body])


def scenes():
    F.humanoid('unreal')
    F.second_scene()


results = []
for style in ('generic', 'rigify', 'daz', 'cc', 'odd', 'mixamo', 'unreal'):
    source = build('clean_' + style, lambda style=style: F.humanoid(style))
    report, prepared, validated = run_case('clean_' + style, source)
    results.append({'case': 'clean_' + style, 'validated': validated})

source = build('messy_odd_gun', lambda: messy_gun('odd'))
report, prepared, validated = run_case('messy_odd_gun', source, action='replace')
aside = {r['name'] for r in report['scene']['objects'] if r['status'] == 'excluded'}
assert {'Body_old', 'Floor', 'NPC_Body', 'Credits', 'Hair guide'} <= aside, aside
assert [m['name'] for m in prepared['meshes'] if m['name'].startswith('weapon_')] == ['weapon_Pistol']
assert prepared['weapon']['action'] == 'replace'
results.append({'case': 'messy_odd_gun', 'set_aside': sorted(aside), 'validated': validated})

source = build('posed_constraint_gun', posed_constraint_gun)
report, prepared, validated = run_case('posed_constraint_gun', source, action='replace')
pistol = next(w for w in report['weapons'] if 'Pistol' in w['objects'])
assert pistol['attach_bone'] == 'mixamorig:RightHand', pistol
results.append({'case': 'posed_constraint_gun', 'validated': validated})

source = build('laid_down_generic', laid_down)
report, prepared, validated = run_case('laid_down_generic', source, expect_rotation=[90.0, 0.0, 0.0])
results.append({'case': 'laid_down_generic', 'validated': validated})

source = build('shape_keys_subsurf', shapes)
report, prepared, validated = run_case('shape_keys_subsurf', source, shape='current_static')
assert len(prepared['meshes'][0]['positions']) > 280, 'Subdivision was not applied after static shape baking'
results.append({'case': 'shape_keys_subsurf', 'validated': validated})

source = build('curve_hair', hair_curve)
report, prepared, validated = run_case('curve_hair', source)
assert any(m['name'].startswith('Hair curve') for m in prepared['meshes']), [m['name'] for m in prepared['meshes']]
results.append({'case': 'curve_hair', 'validated': validated})

source = build('collection_instance', instanced)
report, prepared, validated = run_case('collection_instance', source)
results.append({'case': 'collection_instance', 'validated': validated})

source = build('two_scenes', scenes)
report, prepared, validated = run_case('two_scenes', source)
results.append({'case': 'two_scenes', 'validated': validated})

for suffix in ('fbx', 'glb'):
    F.clear()
    messy_gun('mixamo')
    exported = OUT / 'sources' / ('messy_gun.' + suffix)
    F.export(exported)
    report, prepared, validated = run_case('messy_gun_' + suffix, exported, action='replace')
    assert any(m['name'].startswith('weapon_') for m in prepared['meshes']), suffix
    results.append({'case': 'messy_gun_' + suffix, 'validated': validated})

source = build('gun_only_weapon_model', lambda: F.bone_parent(F.pistol(), F.humanoid('mixamo')[0], 'mixamorig:RightHand'))
report, prepared, validated = run_case('gun_only_weapon_model', source, mode='weapon', action='replace')
assert [m['name'] for m in prepared['meshes']] == ['weapon_Pistol']
results.append({'case': 'gun_only_weapon_model', 'validated': validated})

source = build('unrigged_statue', F.unrigged_character)
report = inspect(source, OUT / 'unrigged_statue' / 'inspect')
assert report['status'] == 'ok' and report['suggested_mode'] == 'unrigged_humanoid', report.get('error')
assert {r['name'] for r in report['scene']['objects'] if r['status'] == 'excluded'} >= {'Floor'}
assert 'landmarks' in report['humanoid'], report['humanoid']
results.append({'case': 'unrigged_statue', 'inspected': True})

(OUT / 'any-humanoid-results.json').write_text(json.dumps({'blender': bpy.app.version_string, 'cases': results}, indent=2) + '\n')
print(json.dumps({'passed': len(results), 'cases': [r['case'] for r in results]}))
