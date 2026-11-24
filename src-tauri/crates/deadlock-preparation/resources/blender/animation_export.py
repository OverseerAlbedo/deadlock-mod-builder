"""Executed ONLY by external Blender 4.2-5.3, with autoexec disabled, as its own
entry (never through worker.py):

    blender --background --factory-startup --disable-autoexec --python animation_export.py -- <job.json>

Reads the actions of the user's file and samples one of them as motion
of the hero's bones, retargeted the way the preparation fitted that rig at
rest. The fit comes from the prepared model's `animation-fit.json`
(animation_fit.py), so the preparation never runs again.

Job: {"operation": "list" | "sample", "source": the .blend, .fbx, .glb or
.gltf (FBX and glTF are imported with the preparation's settings plus their
animation; each FBX take or glTF animation is one action), "fit": the fit
file, "result": reply path, "workspace": job folder}; "sample" adds "action",
"mode" ("as_it_looks" or "from_rest"), "packet" (where the packet goes) and
optional "step" (frames between samples, default 1).

The file's armature must be the one the model was prepared from: the rig with
the saved name (else the one armature holding every driving bone), and every
bone that drives a hero bone at the rest it had (rotation within
REST_ROTATION, position within REST_INCHES after the unit scale). Otherwise
the file is refused with one sentence naming a bone that differs.

Two modes. Source poses are in the normalized source space preparation uses:
rotation without scale, positions times the unit scale.
  dR_b(f)  = rot(P_b(f)) rot(R_b)^T        source world rotation change from rest
  Q_h      = polar rotation of b's rest map (exact when the map is rigid;
             Preserve source proportions scales limbs along their length)
  B, s     = the body alignment's rotation and even scale
  a        = nearest hero ancestor of h that has a driver ("anchor")

"as_it_looks" (default): the hero bone takes the source bone's world
orientation through the fixed source-to-hero bone relation of the rest fit,
in the body-aligned world:
  D_h(f)   = B dR_b(f) Q_h^T              rotation = D_h rot(T_h)
  y_h(f)   = D_a(f)^T D_h(f)              (D_h when h has no anchor)
Why: preparation puts the mesh around b at Q_h v (v: source rest offset from
the joint); the game draws it at D_h Q_h v; Blender draws it at dR_b v.
Equal after the body alignment B gives the rule. The source posed like the
prepared mesh (dR_b = B^T Q_h) gives D = I: the hero's rest. A source at its
own rest gives D_h = B Q_h^T: the hero in the source's rest pose (a T-pose
source gives a T-posed hero). Limb maps take the source limb's anatomical
frame onto the hero's, so a limb segment points along B times the source
segment and its roll about its length is the source's. A shoulder bridge
(arm-deformation kind "shoulder": the chest's map plus a shear) uses the
chest's map, so the source's rest clavicle stands for the hero's rest
clavicle.
  p_h(f)   = p_T(h) + s B (x_b(f) - x_b(rest))   bones without an anchor (the
             pelvis): the hero joint moves as the source joint moves.

"from_rest": the change from each one's rest pose.
  Y_h(f)   = Q_h (dR_a^T dR_b) Q_h^T     (Q_h dR_b Q_h^T when h has no anchor)
  D_h(f)   = D_a(f) Y_h(f)
  p_h(f)   = M_b D'_b(f) M_b^-1 p_T(h)    bones without an anchor only
  A rest frame gives D = I: the prepared rest exactly.

Source bones that follow another bone (no independent motion in the prepared
model) never drive a hero bone. Several source bones on one hero bone: the
role bone wins, then a weighted bone, then the first by name. Every number is
a float64 (mathutils works in float32, so quaternions are taken in numpy).
"""
import json
import math
import sys
from pathlib import Path

import numpy as np
import bpy
from mathutils import Matrix, Euler

# Must equal Rust `clip_making::PACKET_POLICY` (clip_author::POLICY).
POLICY = 'source-action-retarget-v2'
# Must equal Rust `clip_making::FIT_POLICY`.
FIT_POLICY = 'animation-fit-v1'
MODES = ('as_it_looks', 'from_rest')
REST_ROTATION = 1e-4
REST_INCHES = 1e-3
# Sentences Rust and the page compare (clip_making.rs).
SKELETON_DIFFERS = "This file's skeleton differs from your prepared character"
SKELETON_ADVICE = 'Animate the file you prepared, or a copy of it.'
NO_RIG = 'This file has no armature with the bones of your prepared character.'
NO_ACTION = 'The file has no animation named '
MAX_SAMPLES = 20000


def _rotation(matrix):
    """The nearest rotation (polar decomposition) of a 3x3 or the 3x3 of a 4x4."""
    m = np.asarray(matrix, dtype=float)[:3, :3]
    u, _, vh = np.linalg.svd(m)
    r = u @ vh
    if np.linalg.det(r) < 0:
        u[:, -1] *= -1
        r = u @ vh
    return r


def _quat(r):
    """x, y, z, w of a rotation matrix, in double precision."""
    m = np.asarray(r, dtype=float)[:3, :3]
    t = m[0, 0] + m[1, 1] + m[2, 2]
    if t > 0.:
        s = math.sqrt(t + 1.) * 2.
        q = [(m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s, s / 4.]
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = math.sqrt(1. + m[0, 0] - m[1, 1] - m[2, 2]) * 2.
        q = [s / 4., (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s, (m[2, 1] - m[1, 2]) / s]
    elif m[1, 1] > m[2, 2]:
        s = math.sqrt(1. + m[1, 1] - m[0, 0] - m[2, 2]) * 2.
        q = [(m[0, 1] + m[1, 0]) / s, s / 4., (m[1, 2] + m[2, 1]) / s, (m[0, 2] - m[2, 0]) / s]
    else:
        s = math.sqrt(1. + m[2, 2] - m[0, 0] - m[1, 1]) * 2.
        q = [(m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, s / 4., (m[1, 0] - m[0, 1]) / s]
    n = math.sqrt(sum(x * x for x in q))
    return [x / n for x in q]


def load_fit(path):
    raw = json.loads(Path(path).read_text(encoding='utf-8'))
    if raw.get('policy') != FIT_POLICY:
        raise ValueError('The prepared model has no fit for making clips. Prepare the model again.')
    arr = lambda m: np.asarray(m, dtype=float)
    return {
        'raw': raw,
        'source': {n: arr(m) for n, m in raw['source'].items()},
        'maps': {n: arr(m) for n, m in raw['maps'].items()},
        'weighted': set(raw['weighted']),
        'roles': raw['source_roles'],
        'mapping': raw['mapping'],
        'following': raw['following'],
        'unit_scale': float(raw['unit_scale']),
        'B': arr(raw['body_rotation']),
        'scale': float(raw['body_scale']),
        'arm_deformation': raw.get('arm_deformation') or {},
        'target': raw['target'],
        'rotation': [float(v) for v in raw.get('rotation_degrees') or (0., 0., 0.)],
        'rig': raw.get('rig'),
    }


def drivers(fit):
    """Hero bone -> the source bone that drives it."""
    roles = set(fit['roles'].values())
    by_hero = {}
    for name, hero in fit['mapping'].items():
        if name in fit['following'] or name not in fit['source'] or name not in fit['maps']:
            continue
        by_hero.setdefault(hero, []).append(name)
    return {hero: sorted(names, key=lambda n: (n not in roles, n not in fit['weighted'], n))[0]
            for hero, names in by_hero.items()}


def context(fit):
    """Everything per hero bone that does not change from frame to frame."""
    target = {b['name']: b for b in fit['target']}
    T = {n: np.asarray(b['matrix'], dtype=float) for n, b in target.items()}
    hero_parent = {n: b.get('parent') for n, b in target.items()}
    drive = {h: b for h, b in drivers(fit).items() if h in target}
    order, seen = [], set()

    def visit(n):
        if n in seen or n not in target:
            return
        if hero_parent[n]:
            visit(hero_parent[n])
        seen.add(n)
        order.append(n)
    for n in target:
        visit(n)
    anchor = {}
    for h in drive:
        a = hero_parent[h]
        while a is not None and a not in drive:
            a = hero_parent[a]
        anchor[h] = a
    Q = {h: _rotation(fit['maps'][drive[h]]) for h in drive}
    look, bridged = dict(Q), []
    for h, b in drive.items():
        info = fit['arm_deformation'].get(b) or {}
        anchor_bone = fit['roles'].get(info.get('role'))
        if info.get('kind') == 'shoulder' and anchor_bone in fit['maps']:
            look[h] = _rotation(fit['maps'][anchor_bone])
            bridged.append(h)
    return {'target': target, 'T': T, 'hero_parent': hero_parent, 'drive': drive, 'order': order,
            'anchor': anchor, 'Q': Q, 'look': look, 'bridged': bridged, 'B': fit['B'], 'scale': fit['scale']}


# FBX and glTF files are imported with the settings the preparation's import
# uses (worker.py import_source), so the armature and its rest are the ones
# the fit was saved from; only the animation is read in addition. A Rust test
# (clip_making.rs) reads both files and keeps these equal.
FBX_IMPORT = {'use_anim': True, 'automatic_bone_orientation': False}
GLTF_IMPORT = {'merge_vertices': False}
SUFFIXES = ('.blend', '.fbx', '.glb', '.gltf')


def open_file(path):
    suffix = Path(path).suffix.lower()
    if suffix not in SUFFIXES:
        raise ValueError('Unsupported animation file: ' + Path(path).name)
    if suffix == '.blend':
        bpy.ops.wm.open_mainfile(filepath=path, load_ui=False, use_scripts=False)
    elif suffix == '.fbx':
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.import_scene.fbx(filepath=path, **FBX_IMPORT)
    else:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        properties = bpy.ops.import_scene.gltf.get_rna_type().properties
        optional = {'disable_bone_shape': True} if 'disable_bone_shape' in properties else {}
        bpy.ops.import_scene.gltf(filepath=path, **GLTF_IMPORT, **optional)
    if bpy.context.object and bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')


def find_rig(fit, ctx):
    needed = set(ctx['drive'].values())
    armatures = [o for o in bpy.context.scene.objects if o.type == 'ARMATURE']
    named = [o for o in armatures if o.name == fit['rig']]
    if named:
        return named[0]
    holding = [o for o in armatures if needed <= set(o.data.bones.keys())]
    if len(holding) == 1:
        return holding[0]
    if len(armatures) == 1:
        return armatures[0]
    raise ValueError(NO_RIG)


def turn(fit):
    """The rotation the preparation applied to the file's root objects."""
    return Euler(tuple(math.radians(v) for v in fit['rotation']), 'XYZ').to_matrix().to_4x4()


def skeleton_check(fit, ctx, rig):
    """(None, bones) when every driving bone is at its prepared rest, else (sentence, bones)."""
    u, R = fit['unit_scale'], turn(fit)
    world = R @ rig.matrix_world
    for b in sorted(set(ctx['drive'].values())):
        bone = rig.data.bones.get(b)
        if bone is None:
            return SKELETON_DIFFERS + ' (the bone ' + b + ' is missing). ' + SKELETON_ADVICE, len(rig.data.bones)
        rest = np.asarray(world @ bone.matrix_local, dtype=float)
        saved = fit['source'][b]
        if (float(np.abs(_rotation(rest) - saved[:3, :3]).max()) > REST_ROTATION
                or float(np.abs(rest[:3, 3] * u - saved[:3, 3]).max()) > REST_INCHES):
            return SKELETON_DIFFERS + ' (the bone ' + b + ' is not where it was when you prepared). ' + SKELETON_ADVICE, len(rig.data.bones)
    return None, len(rig.data.bones)


def _curves(action):
    """The action's F-curves (layered actions of Blender 4.4+ too)."""
    curves = []
    if hasattr(action, 'layers') and len(action.layers):
        for layer in action.layers:
            for strip in layer.strips:
                for bag in getattr(strip, 'channelbags', []):
                    curves.extend(bag.fcurves)
    if not curves and hasattr(action, 'fcurves'):
        try:
            curves = list(action.fcurves)
        except (AttributeError, RuntimeError):
            curves = []
    return curves


def _fcurve_bones(action):
    """Bone names the action has F-curves for."""
    names = set()
    for c in _curves(action):
        p = c.data_path
        if p.startswith('pose.bones["'):
            names.add(p[len('pose.bones["'):].split('"]')[0])
    return names


def assign(rig, name):
    action = bpy.data.actions.get(name)
    if action is None:
        raise ValueError(NO_ACTION + name + '.')
    data = rig.animation_data_create()
    for track in data.nla_tracks:
        track.mute = True
    # Bones the action has no keys for keep whatever pose values they had (an
    # earlier action, a pose left in the file): put every bone at rest first.
    data.action = None
    for pb in rig.pose.bones:
        pb.matrix_basis = Matrix.Identity(4)
    data.action = action
    if hasattr(data, 'action_slot') and data.action_slot is None:
        slots = [s for s in getattr(action, 'slots', []) if getattr(s, 'target_id_type', 'OBJECT') == 'OBJECT']
        if slots:
            data.action_slot = slots[0]
    return action


def fps():
    scene = bpy.context.scene
    return scene.render.fps / scene.render.fps_base


def frame_bones(fit, ctx, posed, mode):
    """Hero bone rows of one frame from the posed source worlds."""
    u = fit['unit_scale']
    drive, anchor, Q, B, T = ctx['drive'], ctx['anchor'], ctx['Q'], ctx['B'], ctx['T']
    dR = {b: _rotation(p) @ fit['source'][b][:3, :3].T for b, p in posed.items()}
    delta, bones = {}, {}
    for h in ctx['order']:
        if h not in drive:
            continue
        b, a = drive[h], anchor[h]
        if mode == 'as_it_looks':
            delta[h] = B @ dR[b] @ ctx['look'][h].T
            y = delta[h] if a is None else delta[a].T @ delta[h]
        else:
            change = dR[b] if a is None else dR[drive[a]].T @ dR[b]
            y = Q[h] @ change @ Q[h].T
            delta[h] = y if a is None else delta[a] @ y
        row = {'y': _quat(y), 'delta': _quat(delta[h])}
        if a is None:
            if mode == 'as_it_looks':
                moved = posed[b][:3, 3] * u - fit['source'][b][:3, 3]
                row['position'] = (T[h][:3, 3] + ctx['scale'] * (B @ moved)).tolist()
            else:
                d = np.eye(4)
                d[:3, :3] = dR[b]
                d[:3, 3] = posed[b][:3, 3] * u - dR[b] @ fit['source'][b][:3, 3]
                m = fit['maps'][b]
                row['position'] = (m @ d @ np.linalg.inv(m) @ np.r_[T[h][:3, 3], 1.])[:3].tolist()
        bones[h] = row
    return bones


def list_actions(fit, ctx, rig):
    drive = set(ctx['drive'].values())
    bones = set(rig.data.bones.keys())
    rate = fps()
    out = []
    for action in sorted(bpy.data.actions, key=lambda a: a.name):
        keyed = _fcurve_bones(action) & bones
        start, end = float(action.frame_range[0]), float(action.frame_range[1])
        out.append({'name': action.name, 'frame_start': start, 'frame_end': end, 'fps': rate,
                    'length_seconds': (end - start) / rate if rate > 0 else 0.,
                    'keyed_bones': sorted(keyed), 'driving_keyed_bones': len(keyed & drive)})
    return out


def sample(fit, ctx, rig, job):
    mode = job.get('mode') or 'as_it_looks'
    if mode not in MODES:
        raise ValueError('Unknown mode ' + str(mode) + '; choose one of ' + ', '.join(MODES))
    action = assign(rig, job['action'])
    keyed = _fcurve_bones(action)
    start, end = float(action.frame_range[0]), float(action.frame_range[1])
    step = float(job.get('step') or 1)
    rate = fps()
    if not end > start or rate <= 0:
        raise ValueError('The action ' + action.name + ' has fewer than two frames to sample.')
    if (end - start) / step + 1 > MAX_SAMPLES:
        raise ValueError('The action ' + action.name + ' is too long to sample (at most ' + str(MAX_SAMPLES) + ' frames).')
    scene, R = bpy.context.scene, turn(fit)
    drive = ctx['drive']
    names = sorted(set(drive.values()))
    frames, frame = [], start
    while frame <= end + 1e-6:
        frames.append(frame)
        frame += step
    if job.get('keys'):
        # An imported file's keys need not sit on the scene's frames (a glTF
        # file keyed at 30 per second lands on a 24 per second scene): its
        # key times are sampled too, so no key is passed over.
        keys = {round(float(k.co[0]), 6) for c in _curves(action) for k in c.keyframe_points}
        frames = sorted(set(round(f, 6) for f in frames) | {k for k in keys if start - 1e-6 <= k <= end + 1e-6})
        if len(frames) > MAX_SAMPLES:
            raise ValueError('The action ' + action.name + ' is too long to sample (at most ' + str(MAX_SAMPLES) + ' frames).')
    samples = []
    for frame in frames:
        whole = math.floor(frame)
        scene.frame_set(int(whole), subframe=float(frame - whole))
        world = R @ rig.matrix_world
        posed = {b: np.asarray(world @ rig.pose.bones[b].matrix, dtype=float) for b in names}
        samples.append({'frame': frame, 'time': (frame - start) / rate, 'bones': frame_bones(fit, ctx, posed, mode)})
    rows = []
    for h in ctx['order']:
        if h in drive:
            b = drive[h]
            rows.append({'hero': h, 'source': b, 'anchor': ctx['anchor'][h], 'keyed': b in keyed,
                         'weighted': b in fit['weighted'], 'shoulder_bridge': h in ctx['bridged'],
                         'bind_rotation': _quat(_rotation(ctx['T'][h]))})
    roles = fit['raw'].get('target_roles') or {}
    return {'policy': POLICY, 'mode': mode, 'action': action.name, 'fps': rate, 'frame_start': start, 'frame_end': end,
            'step': step, 'length': (end - start) / rate, 'bones': rows,
            'unmatched_hero_bones': [n for n in ctx['order'] if n not in drive],
            'keyed_source_bones': sorted(keyed), 'feet': [roles[r] for r in ('foot_L', 'foot_R') if r in roles],
            'samples': samples}


def main():
    job = json.loads(Path(sys.argv[sys.argv.index('--') + 1]).read_text(encoding='utf-8'))
    try:
        fit = load_fit(job['fit'])
        ctx = context(fit)
        open_file(job['source'])
        rig = find_rig(fit, ctx)
        error, count = skeleton_check(fit, ctx, rig)
        reply = {'status': 'ok', 'rig': rig.name, 'bones': count,
                 'driving_bones': len(set(ctx['drive'].values())), 'skeleton_error': error}
        if job['operation'] == 'list':
            reply['actions'] = list_actions(fit, ctx, rig)
        elif job['operation'] == 'sample':
            if error:
                raise ValueError(error)
            if Path(job['source']).suffix.lower() != '.blend':
                job = dict(job, keys=True)
            packet = sample(fit, ctx, rig, job)
            Path(job['packet']).write_text(json.dumps(packet, allow_nan=False), encoding='utf-8')
            reply.update(samples=len(packet['samples']), length=packet['length'], fps=packet['fps'])
        else:
            raise ValueError('Unknown request.')
        Path(job['result']).write_text(json.dumps(reply, allow_nan=False), encoding='utf-8')
    except Exception as error:
        Path(job['result']).write_text(json.dumps({'status': 'error', 'error': str(error)}), encoding='utf-8')
        raise


if __name__ == '__main__':
    main()
