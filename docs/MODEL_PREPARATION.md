# Model preparation and gun customization

How a character file becomes a Deadlock hero replacement, and what the
preparation step does automatically so that almost any rigged humanoid works.

## Where preparation sits in the app

| Stage | Where | What happens |
| --- | --- | --- |
| Setup | `src/setup`, `src-tauri/src/setup.rs` | Game VPK, Source2Viewer CLI, compilers, Blender. |
| Overview | `src/components/ProjectOverview.tsx` | Pick the hero model to replace (`project.original_model`). The picker labels models from the installed game's hero definitions (`scripts/heroes.vdata`, `m_strModelName`): the model a hero definition loads is that hero's main target, and roster models the game does not load are marked *Not used in game* (Rem loads `familiar_wip.vmdl`). Analyze warns when a project targets such a model. |
| **Model preparation** | `src/preparation`, `deadlock-preparation` crate, embedded Blender scripts | Turn the source file into a DMX bound to the hero's exact skeleton. |
| Appearance / Physics / Extras | `src/components`, `deadlock-materials`, `deadlock-cloth*`, `deadlock-components` | Materials, mesh mapping, equipment, secondary motion, icons, sounds. |
| Review & build | `deadlock-pipeline` | Inspect the project, compile model/materials, pack the VPK, install. |

The desktop app is Rust (Tauri) with a React UI. Blender is only used as a
geometry engine: the Rust crate `deadlock-preparation` writes a `job.json`, runs
Blender in the background with the Python scripts embedded in the crate
(`src-tauri/crates/deadlock-preparation/resources/blender/`), and reads back
`blender-result.json`. Everything else - recognition, mapping, review gates,
weapon fitting, DMX export and validation - is Rust.

### Inspect

1. `source.rs` stages the source into the project workspace (a `.blend` is
   saved as an exact copy with its textures packed; glTF dependencies are
   copied). The user's file is never written.
2. The app reads the hero's skeleton from the compiled model in the game VPK
   and measures the hero's gun from the decompiled original model
   (`weapon::reference_from_sources`).
3. Blender `describe()` (worker.py) reports bones, skin weights, meshes,
   morphs, the scene-object receipt and gun candidates.
4. If the source is lying down, it is rotated upright and inspected again.
5. `automap::suggest` proposes body roles and a target for every weighted bone.

### Convert

`service::prepare` re-checks every hash, builds the weapon plan, and runs
Blender `prepare()`. Each source bone gets a rigid transform from the reviewed
roles (`retarget.py`: *match hero proportions* or *preserve source
proportions*), vertices are moved into the hero's rest pose, weights are mapped
to hero bones, and the result is exported with `deadlock-skeleton`'s exact-bind
DMX exporter, which validates binds and skinning before the hand-off.

## Automatic handling of awkward source files

### Scene objects (`scene_isolation.py`)

The importer keeps the character and sets everything else aside. Every object
gets a status and a reason, shown under **Source inspection → Scene objects**,
where each choice can be overridden before inspecting again.

| Found in the file | What happens |
| --- | --- |
| Cameras, lights, empties, light probes | Ignored. |
| Curves, text, metaballs, surfaces | Converted to meshes when attached to the character rig; otherwise set aside. |
| Grease pencil, hair curves, volumes, point clouds | Set aside (cannot be exported as character geometry). |
| Objects hidden from rendering or in excluded collections | Set aside (hidden alternates, old versions). Included again if nothing visible remains. |
| Several characters | The most prominent one is kept; choose another **Character rig** to override. |
| Meshes bound to another rig but touching the character | Frozen at rest and attached to the character. |
| Part rigs of a modular character (separate face or hand rig) | Combined into the character rig; listed as included. |
| Detached props on their own rig, away from the character | Set aside and listed; keep one to attach it rigidly to the nearest bone at its current offset. |
| Ground planes, backdrops, far-away props | Set aside. |
| Loose props held by the character | Rigidly attached to the nearest bone (finger and toe bones hand them to the hand or foot). |
| Loose accessories hugging the body | Weights copied from the nearest body surface. |
| Bone-parented or constraint-attached objects | Rigidly weighted to that bone at its rest-pose placement. |
| Subdivision, mirror, solidify and other generative modifiers | Applied (after static shape-key baking when needed). |
| Cloth, soft body, particle and other simulation modifiers | Removed; the rest shape is kept. |
| Instanced (shared) mesh data | Made single-user. |
| Mirrored (negative scale) objects | Transform baked into the geometry, normals recomputed. |
| Extra mesh attributes | Dropped (not exported). |
| Empty material slots | A placeholder material is assigned. |
| Vertices without bone weights | Copy the nearest weighted vertex. |
| Collection instances (linked assets) | Made real. |
| Armature object constraints/drivers | Current placement kept, controls dropped. |
| Multiple scenes in a `.blend` | The active scene is used. |
| Missing, generated or sequence textures | Warning; the material keeps a flat colour. |
| Linked libraries | Made local in the workspace snapshot. |

### Bone names (`structure.rs`)

Names are tried first (Mixamo, Unreal, Biped, VRM, MMD, DAZ, Character
Creator, Rigify DEF and common variants). When they are missing, ambiguous or
contradictory, the body is recognised from the skin skeleton itself:

* every plausible pair of floor-contact chains is tried as the feet; skirts or
  hair that reach the floor are rejected because no spine and arms fit them;
* the pelvis is the common ancestor of the feet and the head; the chest is the
  spine joint that carries both arms (the laterally extreme branches);
* shoulder/elbow/wrist and hip/knee/ankle are chosen by segment proportions
  (clavicle roots, twist and helper bones are penalised); neck and head by
  position and skin size;
* left and right come from side tokens in the names (`.L`, `_l`, `Left`,
  `lShldr`, `左` ...) when they agree, otherwise from the toes' forward direction.

The result must pass the same `identify` hierarchy, side and spacing checks as
named rigs. Real MMD rigs with every bone renamed to `node0…nodeN` are
recognised identically to their named version (see `tests/structure.rs`).
User choices in **Body roles** always win.

### Mapping

Fresh inspections request a complete mapping: every remaining weighted bone
follows its body joint with its rest offset kept, so conversion is not blocked
by hundreds of helper, face or accessory bones. Bones with no body joint above
them (for example bones parented to the armature root) follow the nearest body
joint. Hair, skirts and tails detected as moving parts are still kept as
independent bones when **Moving parts** is enabled. Everything stays editable
in **Bone mapping**, and the review confirmation covers the loss of
independent motion.

### Textures (`material_textures.py`)

Each material's base colour is found from its node graph: the image feeding
the Principled BSDF base colour, or, in game rips, an image wired straight into
the Material Output or into a Diffuse, Emission or Toon colour (also through a
mix that only tints one image). A material with a single unsuffixed image uses
it as the base colour. Channel suffixes (`_n`, `_nrm`, `_orm`, `_mask` ...)
choose the other slots. Prepared material names have the form
`Mesh__Material`; the Appearance planner also matches an image named like the
source material inside that name.

### Moving parts (`secondary_bones.rs`, `secondary_bones.py`, `secondary-bones.ts`)

A bone chain becomes a selectable moving-part group when it has existing
physics bindings, or when it has a recognisable name (hair, ponytail, braid,
ribbon, skirt, cape, tail, robe, sash, scarf, shawl, mantle, cloak, tassel,
veil, and Japanese/Chinese equivalents) or is a long unnamed chain: at least
three weighted bones in a single line, hanging from the pelvis, spine, chest,
neck or head, spanning a quarter of the torso. Fingers and toes never qualify.
All three implementations share the fixture
`tests/fixtures/hanging-chains.json` and must agree (`scripts/test_blender_moving_parts.py`
checks the Python copy in Blender). New models keep every
detected group; projects with automatic motion add groups detected for the
first time, while groups you were shown and left off stay off.

Rigid accessories never become moving parts from their names: halo, hat, cap,
crown, tiara, horn, helmet, glasses, mask, earring, hairpin, clip, hair band,
ornament and accessory (also 飾, 冠, 帽, 角, 眼鏡, 輪) override the hair and ear
words, so `HairPin` or `髪飾り` stay on the head. Other motion words still win (a
ribbon on a hat moves). A chain that ends close to where it starts (a halo or
hat band) is a ring, not a strand. During Build, `parts.rs` gives such groups
the **Rigid** part type (they follow their bone without motion) when they come
from source physics bindings.

**Breast motion** is optional and off by default (`breast_motion` in the
preparation options; unset keeps earlier projects unchanged). Breast bones are
weighted, short (at most three bones), non-role children of the chest or spine
that are named breast/bust/boob/oppai/乳 (or 胸 with a side), or form a mirrored
pair in front of the chest between the spine and the neck. Turning it on keeps
them as moving parts and records `physics_kinds: {root: "breast"}`; Build gives
them the **Breast** part type: a damped spring (goal 0.6, drag 0.3, no gravity)
with a bounded travel radius, so they settle quickly and never sag. Amounts are
adjustable per part under Motion → Adjust moving parts and need an in-game check.

### Mesh mapping (Build page)

Every source mesh is a checkbox under each original render node. Prepared
characters name their meshes after the source (Body, Face), not after the
hero's render nodes, so the page offers a **suggested mapping**: all source
meshes on the main body node and its lower detail levels, the original's other
parts hidden, and its gun kept. Nothing is applied until you choose it. Source
meshes that no render node uses are named in a warning (and by Analyze),
because they would be missing in game, with a button that adds them to the
main node.

## Gun customization

Deadlock heroes carry their gun in one of two ways:

* **Part of the hero model**: render meshes skinned to weapon bones such as
  `weaponPivot → weapon → … → muzzle_fx`. The app finds them in the decompiled
  hero model by their skin weights (and names) together with their lower LODs.
* **A separate weapon model**: the hero model has no gun mesh; the gun is its
  own `.vmdl`.

When the character file also contains a gun, the **Weapon** tab offers:

| Choice | Result |
| --- | --- |
| Replace the hero's gun | Available when the gun is part of the hero model. Your gun is fitted onto the original and replaces those meshes (all LODs) in the mesh mapping. |
| Leave it out | Your gun is removed from the converted character. |
| Keep it in the hand | Old behaviour: it follows the hand like any accessory. |

For a separate weapon model use **Appearance → Equipment → Weapon model**:
*Find this hero's weapon models*, then *Create weapon project*. That creates a
gun-only project (preparation mode **Weapon model**) from the same source file
and links it as a bundled member, so one VPK contains the character and the gun.
Open the new project and run Model preparation once.

### How the fit works (`weapon.rs`)

Both guns are measured the same way: the barrel axis (principal axis refined
on the front of the gun), the muzzle end (muzzle bones/attachments, the end
away from the hand, or the thinner end), the up direction (the grip hangs below
the barrel), the grip (the hand holding it, or the handle) and the length.
The source gun is rotated so barrel and up match, uniformly scaled to the
original gun's length (or kept at the character's scale), and moved so the
grips coincide. The original gun's skin weights are copied by proximity, so the
new gun follows the same weapon bones (recoil, slide, magazine). Muzzle
attachments and hit detection stay on the original bones; the fit keeps your
gun on top of them. Flip, roll, scale and offset adjustments and a 3D preview
(original gun in grey, yours in amber) are in the Weapon tab.

Heroes without gun meshes but with weapon/muzzle bones use those bones for the
fit (lower confidence; check the roll in the preview).

## Verifying changes

```powershell
cargo test --manifest-path src-tauri/Cargo.toml -p deadlock-preparation
npm test -- --maxWorkers=2
cargo build --manifest-path src-tauri/Cargo.toml -p deadlock-preparation --example e2e_probe
blender --background --factory-startup --python scripts/test_blender_any_humanoid.py -- C:\DMB-tests\any-humanoid
```

The last command builds awkward sources (generic/Rigify/DAZ/CC/odd names,
cluttered scenes, posed rigs with constraint-attached guns, Y-up exports,
shape keys under subdivision, curve hair, collection instances, two scenes,
FBX/GLB round trips, gun-only weapon models, an unrigged statue) and runs each
through inspect → plan → prepare → native DMX validation against a captured
196-bone hero skeleton.

## Limits

Automatic results still need the in-game check: grip, aim and muzzle
position, elbow and knee bending, and clipping. Non-humanoid bodies
(quadrupeds, characters without two legs and two arms), models whose limbs
are not separated in the rest pose, and live rig controls that change the rest
shape still need a manual source copy. A separately modelled gun replaces the
weapon model for every hero that shares that model.
