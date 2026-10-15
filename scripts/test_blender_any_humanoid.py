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
# Captured hero rig: private assets (a separate repository cloned to private-assets/, or DMB_PRIVATE_ASSETS); see docs/DEVELOPMENT.md.
PRIVATE = Path(os.environ.get('DMB_PRIVATE_ASSETS') or ROOT / 'private-assets')
TARGET = PRIVATE / 'fixtures/daki-rig.json'
if not TARGET.is_file():
    raise SystemExit(f'{TARGET} is missing: clone the private assets repository to private-assets/ (docs/DEVELOPMENT.md).')
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


def inspect(source, folder, rotation=(0, 0, 0), **extra):
    folder.mkdir(parents=True, exist_ok=True)
    job = folder / 'job.json'
    job.write_text(json.dumps({'operation': 'inspect', 'source': str(source), 'rotation': list(rotation),
                               'result': str(folder / 'result.json'), 'workspace': str(folder), **extra}))
    return bridge(job)


def run_case(name, source, mode='hero', action='none', fit='match_target', shape='', expect_rotation=None, gun_file=None):
    folder = OUT / name
    # Weapon projects tell the worker the source may be a gun-only file.
    extra = {'mode': 'weapon'} if mode == 'weapon' else {}
    report = inspect(source, folder / 'inspect', **extra)
    assert report['status'] == 'ok', (name, report.get('error'))
    rotation = json.loads(probe('orient', folder / 'inspect' / 'result.json'))
    if expect_rotation is not None:
        assert rotation == expect_rotation, (name, rotation)
    if rotation:
        report = inspect(source, folder / 'upright', rotation, **extra)
        assert report['status'] == 'ok', (name, report.get('error'))
        inspected = folder / 'upright' / 'result.json'
    else:
        inspected = folder / 'inspect' / 'result.json'
    if gun_file:
        # As the app does: a last pass adds the gun file, held by the right
        # hand the native recognition found (a pair's left gun by the left
        # hand); conversion repeats the request.
        hands = json.loads(probe('hand', inspected))
        report = inspect(source, folder / 'gun', rotation or (0, 0, 0), weapon_source={'source': str(gun_file), **hands}, **extra)
        assert report['status'] == 'ok', (name, report.get('error'))
        inspected = folder / 'gun' / 'result.json'
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
    # Mapped fingers are bent to the hero's resting curl (none skipped).
    curl = prepared['rest_alignment'].get('finger_curl') or {}
    fingers = [d for d in json.loads((OUT / ('clean_' + style) / 'prepare' / 'options.json').read_text(encoding='utf-8')).get('mapping', {}).values() if d.startswith('finger_')]
    assert not curl.get('skipped') and (curl.get('segments') or not fingers), (style, curl)
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

# A gun in a file of its own: one unrigged object called "Cube" with no
# weapon words, on a floor with a camera.
gun_blend = build('gun_file', F.gun_file)
F.clear()
F.gun_file()
gun_glb = OUT / 'sources' / 'gun_file.glb'
F.export(gun_glb)

# Character project: the gun file replaces the hero's in-model gun.
character = build('gun_file_character', lambda: F.humanoid('mixamo'))
report, prepared, validated = run_case('gun_file_character', character, action='replace', gun_file=gun_blend)
rows = {r['name']: r for r in report['scene']['objects']}
assert rows['gun_file_Cube']['status'] == 'bound' and rows['gun_file_Cube']['bone'] == 'mixamorig:RightHand', rows.get('gun_file_Cube')
assert rows['gun_file_Plane']['status'] == 'excluded' and rows['gun_file_Camera']['status'] == 'ignored', rows
gun = next(w for w in report['weapons'] if w.get('gun_file'))
assert gun['objects'] == ['gun_file_Cube'] and gun['attach_kind'] == 'placed', gun
assert report['weapon_file']['placed'] and report['weapon_file']['objects'] == ['gun_file_Cube'], report['weapon_file']
assert [m['name'] for m in prepared['meshes'] if m['name'].startswith('weapon_')] == ['weapon_gun_file_Cube']
assert prepared['weapon']['action'] == 'replace'
results.append({'case': 'gun_file_character', 'validated': validated})

# Keep it in the hand instead: the gun stays rigidly on the right hand.
report, prepared, validated = run_case('gun_file_character_keep', character, action='keep', gun_file=gun_blend)
kept = next(m for m in prepared['meshes'] if m['name'] == 'gun_file_Cube')
assert len({name for row in kept['weights'] for name, _ in row}) == 1, kept['weights'][:3]
results.append({'case': 'gun_file_character_keep', 'validated': validated})

# Weapon project whose source is the gun file (GLB round trip).
report, prepared, validated = run_case('gun_file_weapon_model', gun_glb, mode='weapon', action='replace')
assert [w['objects'] for w in report['weapons']] == [['Cube']] and report['weapons'][0]['attach_kind'] == 'whole_file', report['weapons']
assert {r['name'] for r in report['scene']['objects'] if r['status'] == 'excluded'} >= {'Plane'}, report['scene']['objects']
assert [m['name'] for m in prepared['meshes']] == ['weapon_Cube'], [m['name'] for m in prepared['meshes']]
results.append({'case': 'gun_file_weapon_model', 'validated': validated})

# A gun file with its own rig, two skinned parts and a dense curve strap: one
# gun at rest shape; the rig, a hidden old part and the floor are set aside.
rigged_gun = build('rigged_gun_file', F.rigged_gun_file)
report, prepared, validated = run_case('gun_file_rigged_parts', build('gun_file_rigify', lambda: F.humanoid('rigify')), action='replace', gun_file=rigged_gun)
rows = {r['name']: r['status'] for r in report['scene']['objects'] if r['name'].startswith('gun_file_')}
assert rows == {'gun_file_Frame': 'bound', 'gun_file_Slide': 'bound', 'gun_file_Strap_mesh': 'bound', 'gun_file_Strap': 'converted',
                'gun_file_GunRig': 'excluded', 'gun_file_Old': 'excluded', 'gun_file_Plane': 'excluded'}, rows
assert [w['objects'] for w in report['weapons'] if w.get('gun_file')] == [['gun_file_Frame', 'gun_file_Slide', 'gun_file_Strap_mesh']], report['weapons']
# The gun's three parts are joined into one mesh; every part's material stays.
assert sorted(m['name'] for m in prepared['meshes'] if m['name'].startswith('weapon_')) == ['weapon_gun']
merged = next(m for m in prepared['meshes'] if m['name'] == 'weapon_gun')
assert len({face['material'] for face in merged['faces']}) >= 2, {face['material'] for face in merged['faces']}
results.append({'case': 'gun_file_rigged_parts', 'validated': validated})


def gun_corners(meshes):
    """Every corner of the fitted gun: position, normal, first UV, material and weights.
    A part without UVs reads texture coordinate (0, 0), as it gets when merged."""
    out = []
    for mesh in meshes:
        if not mesh['name'].startswith('weapon_'):
            continue
        uv = mesh['uvs'][0] if mesh['uvs'] else None
        for face in mesh['faces']:
            for vertex, corner in zip(face['vertices'], face['corners']):
                out.append((tuple(round(v, 3) for v in mesh['positions'][vertex]), tuple(round(v, 3) for v in mesh['normals'][corner]),
                            tuple(round(v, 4) for v in uv[corner]) if uv else (0., 0.), face['material'],
                            tuple(sorted((name, round(w, 3)) for name, w in mesh['weights'][vertex]))))
    return sorted(out)


# Merging is lossless: the same gun converted with its parts kept separate has
# exactly the same corners (positions, normals, UVs, materials and weights).
os.environ['DMB_PROBE_MERGE'] = '0'
try:
    _, unmerged, _ = run_case('gun_file_rigged_parts_unmerged', build('gun_file_rigify', lambda: F.humanoid('rigify')), action='replace', gun_file=rigged_gun)
finally:
    os.environ.pop('DMB_PROBE_MERGE', None)
assert len([m for m in unmerged['meshes'] if m['name'].startswith('weapon_')]) == 3
assert gun_corners(prepared['meshes']) == gun_corners(unmerged['meshes'])
results.append({'case': 'gun_merge_is_lossless', 'compared': len(gun_corners(prepared['meshes']))})

# Gun charm: a pistol on its own gun bone carries an unnamed two-link charm
# (part of the pistol mesh). Replacing the hero's gun keeps the charm's bones on
# the fitted gun's weapon bone with its charm weights; keeping the gun in the
# hand hangs the charm from the hand. Moving parts are kept as the UI does.
def charmed_gun():
    rig, body, table = F.humanoid('mixamo')
    F.charmed_pistol(rig, table['hand_R'])


os.environ['DMB_PROBE_PHYSICS'] = '1'
try:
    source = build('charmed_pistol', charmed_gun)
    report, prepared, validated = run_case('charmed_pistol', source, action='replace')
    pistol = next(w for w in report['weapons'] if 'Pistol' in w['objects'])
    assert {'Gun', 'Gun_Link_0', 'Gun_Link_1'} <= set(pistol['bones']), pistol
    assert prepared['weapon']['charms'] == ['Gun_Link_0'], prepared['weapon']
    anchor = json.loads((OUT / 'charmed_pistol' / 'prepare' / 'job.json').read_text(encoding='utf-8'))['weapon']['anchor']
    bones = {b['name']: b for b in prepared['target']}
    assert bones['dmb_secondary_Gun_Link_0']['parent'] == anchor, (bones['dmb_secondary_Gun_Link_0']['parent'], anchor)
    assert bones['dmb_secondary_Gun_Link_1']['parent'] == 'dmb_secondary_Gun_Link_0'
    gun_mesh = next(m for m in prepared['meshes'] if m['name'] == 'weapon_Pistol')
    links = {name for row in gun_mesh['weights'] for name, weight in row if name.startswith('dmb_secondary_Gun_Link')}
    assert links == {'dmb_secondary_Gun_Link_0', 'dmb_secondary_Gun_Link_1'}, links
    # Bones and charm geometry were fitted with the same transform: each link's
    # vertices sit next to its bone, not left behind at the source hand.
    for link in ('dmb_secondary_Gun_Link_0', 'dmb_secondary_Gun_Link_1'):
        head = bones[link]['position']
        step = sum((a - b) ** 2 for a, b in zip(bones['dmb_secondary_Gun_Link_1']['position'], bones['dmb_secondary_Gun_Link_0']['position'])) ** .5
        own = [p for p, row in zip(gun_mesh['positions'], gun_mesh['weights']) if dict(row).get(link, 0) > .5]
        assert own and min(sum((a - b) ** 2 for a, b in zip(p, head)) ** .5 for p in own) <= 1.5 * step, (link, head, own[:2])
    results.append({'case': 'charmed_pistol', 'validated': validated})

    report, prepared, validated = run_case('charmed_pistol_keep', source, action='keep')
    bones = {b['name']: b for b in prepared['target']}
    hand = json.loads((OUT / 'charmed_pistol_keep' / 'prepare' / 'options.json').read_text(encoding='utf-8'))['target_roles']['hand_R']
    assert bones['dmb_secondary_Gun_Link_0']['parent'] == hand, (bones['dmb_secondary_Gun_Link_0']['parent'], hand)
    results.append({'case': 'charmed_pistol_keep', 'validated': validated})

    # A pair of guns in one file (a dual weapon rig, each gun with a charm).
    dual_blend = build('dual_gun_file', F.dual_gun_file)
    character = build('dual_gun_character', lambda: F.humanoid('mixamo'))

    def charm_anchors(prepared):
        kept = prepared.get('preserved_physics_bones') or {'groups': []}
        return {g['root']: g['target_anchor'] for g in kept['groups'] if 'charm' in g['root']}

    def weapon_meshes(prepared):
        return sorted(m['name'] for m in prepared['meshes'] if m['name'].startswith('weapon_'))

    # A dual-wield hero (Haze): each gun goes to its own hand and replaces the
    # hero's gun in that hand; each gun's parts are joined into one mesh, and
    # each charm keeps its bones and hangs from its own gun's weapon bone.
    os.environ['DMB_PROBE_DUAL'] = '1'
    try:
        report, prepared, validated = run_case('dual_gun_character', character, action='replace', gun_file=dual_blend)
    finally:
        os.environ.pop('DMB_PROBE_DUAL', None)
    guns = report['weapon_file']['guns']
    assert [(g['side'], g['bone']) for g in guns] == [('right', 'mixamorig:RightHand'), ('left', 'mixamorig:LeftHand')], guns
    assert sorted(guns[0]['charm_bones'] + guns[1]['charm_bones']) == sorted(
        'gun_file_charm_' + part + '.' + tag for part in ('pivot', 'body') for tag in 'LR'), guns
    assert sorted(w.get('side') for w in report['weapons'] if w.get('gun_file')) == ['left', 'right'], report['weapons']
    for gun in (w for w in report['weapons'] if w.get('gun_file')):
        # Each gun has a solid preview shape, and its named front (the
        # emitter) at the barrel end tells the fit which way it points.
        shape, hints = gun['shape'], gun.get('hints', {})
        assert shape['triangles'] and len(shape['triangles']) % 3 == 0 and max(shape['triangles']) < len(shape['positions']) // 3, gun['id']
        assert hints.get('front') and hints['front'][0] > (gun['bounds'][0][0] + gun['bounds'][1][0]) / 2, hints
    assert weapon_meshes(prepared) == ['weapon_gun', 'weapon_l_gun'], weapon_meshes(prepared)
    # The baked frames and slides of both guns read one texture: one material,
    # not tinted with the colour behind its connected Base Color, textured with
    # the UV map the material reads ("BakeUV", exported first).
    baked = {f['material'] for m in prepared['meshes'] if m['name'].startswith('weapon_') for f in m['faces'] if 'baked' in f['material']}
    assert len(baked) == 1, baked
    material = next(iter(baked))
    assert prepared['materials'][material]['color'] == [1.0, 1.0, 1.0] and prepared['materials'][material]['textures'].get('color'), prepared['materials'][material]
    for mesh in (m for m in prepared['meshes'] if m['name'].startswith('weapon_')):
        corners = [c for f in mesh['faces'] if f['material'] == material for c in f['corners']]
        assert corners and all(tuple(round(v, 3) for v in mesh['uvs'][0][c]) == (.25, .75) for c in corners), mesh['name']
    assert any('became one material' in w for w in prepared['warnings']), prepared['warnings']
    assert [h['side'] for h in prepared['weapon']['hands']] == ['right', 'left'], prepared['weapon']
    assert charm_anchors(prepared) == {'gun_file_charm_pivot.R': 'weaponHand_R', 'gun_file_charm_pivot.L': 'weaponHand_L'}, charm_anchors(prepared)
    for side, mesh in (('R', 'weapon_gun'), ('L', 'weapon_l_gun')):
        exported = next(m for m in prepared['meshes'] if m['name'] == mesh)
        charm = {name for row in exported['weights'] for name, _ in row if 'charm' in name}
        assert charm == {'dmb_secondary_gun_file_charm_pivot.' + side, 'dmb_secondary_gun_file_charm_body.' + side}, (mesh, charm)
    results.append({'case': 'dual_gun_character', 'validated': validated})

    # A hero with one gun: the right gun of the pair is used and the left gun
    # (with its charm) is left out.
    report, prepared, validated = run_case('dual_gun_single_hero', character, action='replace', gun_file=dual_blend)
    assert weapon_meshes(prepared) == ['weapon_gun'], weapon_meshes(prepared)
    assert not any(m['name'].startswith('gun_file_') for m in prepared['meshes']), [m['name'] for m in prepared['meshes']]
    assert charm_anchors(prepared) == {'gun_file_charm_pivot.R': 'weapon'}, charm_anchors(prepared)
    results.append({'case': 'dual_gun_single_hero', 'validated': validated})

    # One gun in both hands: the left hand gets a copy of the right gun (its
    # charm stays rigid on the copy); the file's left gun is left out.
    os.environ['DMB_PROBE_DUAL'] = '1'
    os.environ['DMB_PROBE_LEFT'] = 'copy'
    try:
        report, prepared, validated = run_case('dual_gun_copy', character, action='replace', gun_file=dual_blend)
    finally:
        os.environ.pop('DMB_PROBE_DUAL', None)
        os.environ.pop('DMB_PROBE_LEFT', None)
    assert weapon_meshes(prepared) == ['weapon_gun', 'weapon_l_gun'], weapon_meshes(prepared)
    assert prepared['weapon']['hands'][1]['copy'], prepared['weapon']
    assert charm_anchors(prepared) == {'gun_file_charm_pivot.R': 'weaponHand_R'}, charm_anchors(prepared)
    results.append({'case': 'dual_gun_copy', 'validated': validated})
finally:
    os.environ.pop('DMB_PROBE_PHYSICS', None)

source = build('unrigged_statue', F.unrigged_character)
report = inspect(source, OUT / 'unrigged_statue' / 'inspect')
assert report['status'] == 'ok' and report['suggested_mode'] == 'unrigged_humanoid', report.get('error')
assert {r['name'] for r in report['scene']['objects'] if r['status'] == 'excluded'} >= {'Floor'}
assert 'landmarks' in report['humanoid'], report['humanoid']
results.append({'case': 'unrigged_statue', 'inspected': True})

(OUT / 'any-humanoid-results.json').write_text(json.dumps({'blender': bpy.app.version_string, 'cases': results}, indent=2) + '\n')
print(json.dumps({'passed': len(results), 'cases': [r['case'] for r in results]}))
