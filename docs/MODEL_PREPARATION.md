# Model preparation and gun customization

How a character file becomes a Deadlock hero replacement, and what the
preparation step does automatically so that almost any rigged humanoid works.

## Where preparation sits in the app

The sidebar leads through five numbered steps (a check mark shows a finished
step); Motion and Extras are optional, Step timings, Logs and Settings are tools.

| Step | Where | What happens |
| --- | --- | --- |
| Setup | `src/setup`, `src-tauri/src/setup.rs` | Game VPK, Source2Viewer CLI, compilers, Blender. |
| 1. Character & model | `src/components/ProjectOverview.tsx` | Pick the hero model to replace (`project.original_model`), your model file and, optionally, a gun file. The picker labels models from the installed game's hero definitions (`scripts/heroes.vdata`, `m_strModelName`): the model a hero definition loads is that hero's main target, and roster models the game does not load are marked *Not used in game* (Rem loads `familiar_wip.vmdl`). Analyze warns when a project targets such a model. A Blender, FBX or glTF model goes straight to step 2 and is prepared automatically (`dmb-model-source` event with `auto: true`); a DMX is analysed as before. |
| **2. Prepare model** | `src/preparation`, `deadlock-preparation` crate, embedded Blender scripts | Turn the source file into a DMX bound to the hero's exact skeleton. **Prepare automatically** inspects, applies the suggestions, checks (`preparation_review`), converts and uses the model in one go and continues to step 3; it stops where a choice is needed. **Prepare model** does the same from the review tabs: the click is the review (`reviewed: true`, plus `follow_motion_reviewed` when joints follow the body), and the check runs again right before converting. Choices that need a new inspection (the gun file, Scene objects) re-run it automatically. A prepared project, or one that started from a DMX, says so and offers to continue. |
| 3. Appearance & weapons | `src/components/Appearance.tsx`, `src/preparation/WeaponsSection.tsx`, `deadlock-materials` | Materials, textures, mesh mapping, equipment, and the gun of the prepared model (the saved preparation is reopened with `preparation_resume`; **Apply gun changes** prepares again in one click and refreshes the 3D preview of the character). |
| 4. Physics & preview | `src/components/CustomizationPages.tsx` (`PhysicsPage`), `deadlock-cloth*` | Secondary motion (hair, cloth, breasts, gun charms) and a 3D view of the prepared character. |
| 5. Build & install | `deadlock-pipeline` | Inspect the project, compile model/materials, pack the VPK, install. |

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

Mesh data crosses the Blender boundary through `mesh_export.py` (bulk
`foreach_get`/`foreach_set` readers, one cached pass over the vertex groups per
object, vectorized blend matrices and normals); the per-element loops it
replaced took most of a conversion. Once the DMX is written, the receipt
(`preparation-report.json`, compact JSON) keeps each mesh as `PreparedMesh::slim`
(name, vertex and polygon counts, materials, bones and audit fields) and
`blender-result.json` is removed, so a preparation folder holds the DMX, the
`.blend`, textures and small JSON files rather than three copies of the
geometry. Older receipts with full geometry still load (the geometry fields
default to empty).

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

Mapped finger segments are bent to the hero's resting direction
(`retarget.py finger_curl`, option `fit_finger_curl`, on by default; **Bend
fingers like the hero** in Conversion review). Hero rest poses have bent fingers
(Haze about 45–55° per finger) and the game adds the grip to that pose, so
straight source fingers stayed open around a gun. Each segment keeps its source
length and turns about its joint (the hero joint in *Match hero proportions*,
otherwise where its bent parent puts it); a segment that would turn more than
100° is left as it is and reported (a finger mapped to the wrong hero finger).

### Textures (`material_textures.py`)

Each material's base colour is found from its node graph: the image feeding
the Principled BSDF base colour, or, in game rips, an image wired straight into
the Material Output or into a Diffuse, Emission or Toon colour (also through a
mix that only tints one image). A material with a single unsuffixed image uses
it as the base colour. Channel suffixes (`_n`, `_nrm`, `_orm`, `_mask` ...)
choose the other slots. Prepared material names have the form
`Mesh__Material`; the Appearance planner also matches an image named like the
source material inside that name.

A texture connected to the Principled Base Color is not tinted: the material
gets a white tint, since Blender ignores the colour behind a connected socket
(materials without a base-color image keep their colour, MMD materials their
diffuse colour). The game textures with the first UV map, so before export the
UV map a part's materials read their images with (a UV Map node, or the render
UV map) is moved to the front (`scene_isolation.py sampled_uv_first`; baked
atlases often live on a second map such as `BakeUV`). Gun parts whose materials
the conversion reads identically (same images on the same inputs, same values)
share the most used of them (`weapon_fit.py share_materials`), so a gun baked
to one atlas becomes one game material instead of one per part colour, each with
its own combined normal/roughness texture.

A prepared material's file name keeps at most 24 readable characters of the
Blender material name plus an 8-digit hash of the full name
(`optional_frost___translu_2362886d.vmat`). The compiler names the textures it
makes after the material (`<material>_vmat_g_tnormalroughness_<hash>.vtex`), and
Windows tools fail on paths of 260 characters or more, which a deep CSDK folder
reached with long names. The hash is the identity: when a model prepared with
longer names is prepared again, its material choices and surface changes follow
the hash to the new path (`service.rs renamed_materials`). Staged texture copies
keep at most 32 characters of the image name. Before compiling, Build checks the
compiler's paths for every material and its images (`deadlock-materials
native_paths.rs`) and says whether preparing again or a shorter CSDK folder fixes
an overflow.

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

**Charms** move wherever they hang: names with charm, keychain (also `Key_Chain`;
names are compared without separators), keyring, pendant, lanyard or dangle (also
チャーム, キーホルダー, ペンダント, 根付, 挂件/掛件, 吊坠/吊墜, 挂坠/掛墜, 吊饰/吊飾). A
charm word beats the rigid accessory words, so a dangle earring moves.

**Gun charms** (`gun_charm` in the three detectors): Inspect marks the bones of
every gun candidate it offers (the candidate's attach bone and every bone its
objects are weighted to) with the candidate id (`weapon: ["Pistol"]` on the bone,
`weapon::mark_gun_bones`). A chain hanging from such a bone (not a body role) is a
moving part when it has two to four bones in one line, its tip hangs below its top
by at least 40% of its length, and no bone in it is named as a gun part (slide,
mag/magazine, clip, bullet, shell, ammo, trigger, hammer, bolt, barrel, muzzle,
cylinder, sight, scope, stock, grip, handle, rail, spring, guard ... and 弾,
マガジン, スライド, トリガー, 引き金, 銃口). A strap (`strap`, ストラップ) hanging from a gun
bone counts in any shape and length. Like every automatic moving part a charm
needs two weighted bones at different positions: a charm made of one weighted
bone has nothing to swing around and stays rigid on the gun. A chain hanging
straight from the hand bone is not a gun charm (unless it is named as a charm).

**Breast motion** is optional and off by default (`breast_motion` in the
preparation options; unset keeps earlier projects unchanged). Breast bones are
weighted, short (at most three bones), non-role children of a torso bone (pelvis
to neck) that are named breast/bust/boob/pectoral/oppai/乳 (or 胸 with a side), or
form a mirrored pair in front of the chest between the spine and the neck.
Turning it on keeps them as moving parts and records `physics_kinds: {root: "breast"}`.

**Models without breast bones** (`resources/blender/breast_bones.py`): with breast
motion on, Inspect asks Blender to add `DMB_Breast_L/R` under the chest when the
chest has breasts. The front of the torso skin is measured on a smoothed grid;
each side's bulge must stand well out from the ribcage below it (or clearly out
with a cleavage), and the two sides must mirror each other, so flat, male and
child chests and asymmetric clothing get none (the receipt says why). The bones
sit near the chest wall at the bulge centres, and skin weight near each breast
moves from the torso bones to them with a smooth falloff, keeping every vertex's
total. Only the temporary scene changes; Convert repeats the same request
(`Analysis.breast_generation`). Turning breast motion off maps the added bones
back to the chest until the next inspection. A rig with weighted breast-named
bones (`breast_named_bones`, including ones that carry ribbons or cape strips,
such as Yvonne's `胸上2.L/R`) is never given extra bones. Tested on Daki and
TangTang (added), Ghist and Yvonne with their own breast bones removed (added
close to the real ones), and Paimon and Endmin (none); Daki's added bones passed
conversion and native DMX validation.

Build gives breast groups the **Breast** part type, a bounce in the style of
stylized action games rather than a dangling strand (`secondary_auto.rs`,
`append_breast`): one moving ring sized to the breast (so small node offsets do
not become large rotations), a fixed ring straight behind it toward the chest
(the sideways part toward the midline is removed using the other breast), and a
link between the left and right rings so the pair moves together and cannot
swing into each other. Other bones in a breast group (parts hanging from the
breast) get their own rings and move like accessories from the breast ring;
without them the native fit leaves those bones unfitted and Build stops.
Settings: goal 0.45, drag 0.1, no gravity, no tether,
travel about 45% of the breast radius (0.4–1.5 in). Stiffer is goal 0.62 and
travel ×0.7; More movement is goal 0.3 and travel ×1.35. The first tuning (goal
0.8, drag 0.5, travel 0.3 in) barely moved in game. Amounts need an in-game check.

Two places, two jobs: Model preparation's **Keep or add breast bones** (the
`breast_motion` option) decides whether breast bones are in the converted model.
The **Physics** tab's **Breast motion** switch (`BreastMotion` in
`SecondaryMotion.tsx`) decides whether they bounce and how much: off sets the
breast groups' part type to Rigid (they stay in the model, so the next
conversion's saved groups stay valid), on sets Breast, and one amount control
sets both sides. That needs no conversion. Breast groups there are found by name,
by `physics_kinds` or by the last Analyze, so breast bones from a rig's own
physics (MMD-style `胸上2.L/R`) count even when the preparation option is off.

Rings follow each bone's own X/Z frame, except when the chain runs within that
plane (rigs whose bone Y is not along the bone, such as MMD tip bones like
`尻2.L` → `尻2錘.L`): then the ring is drawn across the chain (`ring_axes`), and
`connect` never picks a pairing with a collapsed face. Without this, coplanar
rings made a degenerate face that the native float32 check rejects.

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

Deadlock heroes carry their gun in one of three ways:

* **Part of the hero model**: render meshes skinned to weapon bones such as
  `weaponPivot → weapon → … → muzzle_fx`. The app finds them in the decompiled
  hero model by their skin weights (and names) together with their lower LODs.
* **A gun in each hand** (Haze: `haze_gun_l`, `haze_gun_r`): every gun mesh is
  weighted to one hand's bones (`weapon_hand_L`, `magazine_L`, `cocking_handle_L`
  …). `reference_from_sources` then splits the reference into `sides` (right and
  left, each with its meshes of all LODs, points and weights); `Reference::hand`
  gives one hand's reference, and `target_frame` measures each hand's gun from its
  own grip and muzzle hints (bones of the other hand never count).
* **A separate weapon model**: the hero model has no gun mesh; the gun is its
  own `.vmdl`.

Decompiled gun meshes can lack a named bind list or have a null `bindState`
(Haze's guns); `deadlock-preview` then uses the joints' own transforms and the
mesh's `currentState`, so such guns are measured instead of being taken for a
separate model. Gun meshes that still cannot be read are listed in
`Reference.unreadable` and explained in the Weapon tab.

The **Weapon** tab (`WeaponPanel.tsx`, helpers in `gun.ts`) starts with a plain
summary of the hero's gun, your gun(s) and what will happen, then one choice:

| Choice | Result |
| --- | --- |
| Use my gun(s) instead of the hero's (recommended, preselected) | Your gun is fitted onto the original and replaces those meshes (all LODs) in the mesh mapping. A dual-wield hero gets a gun in each hand: **Right hand** and **Left hand** pick your gun for each (the left hand can also use *a copy of the right-hand gun* or keep *the hero's own left gun*), and **Swap hands** exchanges them with their adjustments. |
| Keep the hero's gun(s) | Your gun is left out of the converted character. |
| More options → Keep my gun in the hand as an accessory | Old behaviour: it follows the hand like any accessory; the hero's gun stays too. |

While fitting, a 3D preview updates by itself after every change (no Blender run,
`preparation_weapon_preview`): your right gun as a solid amber shape and your left
gun teal, the hero's original guns see-through and the hero's body faintly, with
arrows along the line of fire (white from the original muzzle, coloured from your
gun's muzzle). The shapes are simplified meshes saved with the inspection: the
hero's guns and body from the decompiled DMX (`Reference.shape`, `Side.shape`,
`Reference.body_shape`, `weapon::simplify`) and each candidate gun from Blender
(`Candidate.shape`, `weapon_fit.py _shape`); both use vertex clustering to a
triangle budget. The preview places your gun with the fit matrix itself. View
buttons (Angled, Front, Side, Top) and Show buttons (this hand, both guns, whole
body) frame it; it opens on the hand you adjust, keeps the camera while you adjust,
and keeps the last result on screen while the next one is computed. Inspections
from before the shapes show point samples. Beginner buttons (Flip direction, Turn 90°, Smaller/Bigger,
Back/Forward/Up/Down/Left/Right, Reset) adjust the selected hand; **Advanced gun
settings** keeps size mode, skinning, the weapon bone, exact numbers per hand,
the parts list and **Join each gun's parts into one mesh**.

`weapon::plan_choice` is the one planner (the service and the e2e probe use it):
`Choice.left` holds the left hand's gun and adjustments (`objects` empty = copy the
right gun, `keep` = the hero's own left gun); the plan for the right (or only) gun
carries `left` (the left hand's plan, `mesh_prefix: weapon_l_`, `copy` when it
duplicates the right gun's objects), `merge`, and `drop` (gun-file guns no hand
uses, left out). `suggest_choice` puts the gun file's right gun in the right hand
and its left gun in the left hand. Blender (`weapon_fit.split_hands`) separates
each hand's objects (duplicating them for a copy before anything is fitted),
removes dropped guns and fits each hand with its own matrix and skin. The weapon
receipt lists `hands` (side, meshes); `record_replacements` adds each hand's
replaced original meshes and `receipt_mappings` maps each hand's fitted gun onto
that hand's original gun only.

**Merging** (`weapon_fit.merge_parts`, on by default): a gun's parts are joined into
one mesh (`weapon_gun`, `weapon_l_gun`; chunks stay under 200,000 corners, far
below the engine's 524,288). Vertex groups merge by name (bone weights and charm
weights stay), materials by slot, UV maps by position (renamed by index first);
each part's world-space corner normals are stored in a `dmb_normal` corner
attribute before joining and used for the export, and the joined counts are
checked. Parts with shape keys are left as they are. The harness compares a merged
and an unmerged conversion corner by corner (`gun_merge_is_lossless`).

Faces with more than four corners (n-gons) are split into triangles right before
export when the mesh has UVs (`scene_isolation.triangulate_ngons`): Blender
computes tangents only for triangles and quads, and the game triangulates anyway.
Triangles and quads, vertices, weights, UVs and custom normals stay; meshes without
n-gons (or with shape keys) are not touched. This applies to gun meshes, body
meshes, attached props and weapon-model corrections.

For a **separate weapon model**, the Weapon tab's **Use my gun instead of the
hero's** (`SeparateGun.tsx`) finds the hero's gun model (particle and effect models
are never offered), saves the project if it was never saved, creates
`<project>.gun-<model>.dmb.json` next to it (preparation mode **Weapon model**,
source = the gun file or the character file), inspects, picks the gun, checks,
converts and accepts it, links it as an equipment variant (one VPK holds the
character and the gun) and sets the character's own gun to *leave out*. The same
preview and adjust buttons work on the gun project; **Apply changes** makes the gun
model again. **Appearance → Equipment → Weapon model** (*Find this hero's weapon
models*, *Create weapon project*) remains for manual setups.

A weapon project's source may be a gun-only file. Inspect is told the project
is a weapon model (`mode: weapon`): the gun may hold most of the scene's
vertices, and when no object is recognisable as a gun (one unnamed, unrigged
object such as `Cube`), the whole scene except ground planes and backdrops is
offered as the gun (`attach_kind: whole_file`) and suggested. Without a rig, the
scene is measured by its longest side and parts touching each other in a chain
(barrel, suppressor) stay together.

### Gun from a separate file

The **Weapon** tab of a character project (and step 1, Character & model) has an
optional **Gun from another file** (`preparation_options.weapon_source`; Blender,
FBX, GLB or glTF). Choosing, changing or clearing it re-runs the inspection by
itself.

* Inspect stages the file like the source (`<preparation>/weapon-file`, a packed
  copy for `.blend`) and records `Analysis.weapon_file`: the user's path with the
  SHA-256 of the file and its textures, the staged copy and its SHA-256, and the
  hand bone. After the normal inspection passes, one more Blender pass adds the
  gun with the right hand the native recognition found (`hand_R`). Convert sends
  the same request (`weapon_source`); corrections already hold the gun.
* Blender (`weapon_source.py`) appends the file's objects after the character was
  isolated, rotated and given breast bones, and renames them `gun_file_<name>`.
  Cameras, lights and empties are ignored; hidden objects, the gun's own rig, bone
  shapes, ground planes and backdrops (flat and gun-sized, or much larger than
  the gun) and parts away from the gun are set aside, and each can be kept in
  **Scene objects**. Shape keys, modifiers, constraints and mirrored transforms
  are baked. With a hand bone the gun's grip (the handle below the barrel,
  measured like `weapon.rs`) is moved to the middle of the hand unless the gun
  already sits there, and the gun is rigidly weighted to the hand, so *Keep it in
  the hand* works (its orientation stays as in the file). A rig without a right
  hand attaches it to the nearest weighted bone; an unrigged character keeps it
  where it is.
* A file with **a pair of guns** gives a right and a left gun: parts are sorted by
  their hand, read from the original object name (tags `[L]`/`[R]`, `(L)`, `.L`,
  `_R` first, then the words left/right, l/r, 左/右; `weapon_source.side_of`, the
  same rules as `weapon.rs side_of`) or from the gun rig bone they are weighted to,
  before the parts are frozen. Each gun is its largest part and what touches it;
  parts without a hand join the gun they touch. The right gun goes to the right
  hand and the left gun to the left hand (the request carries `hand` and
  `left_hand`); receipt rows and candidates carry `side`, and the summary lists
  `guns`.
* The gun-file objects are always offered, one candidate per gun (`gun_file:
  true`), and are suggested before any gun in the character file (the right-hand
  gun first). A gun that was moved into the hand (`attach_kind: placed`) is fitted
  from its own handle, not from the hand. A gun-file gun that no hand uses is left
  out (`Plan.drop`). A gun in the character file stays on the character unless you
  set it aside in **Scene objects**.
* Ground planes and backdrops are found by comparing each flat part with
  everything else in the file (`weapon_source.backdrops`), so flat gun parts such
  as grip panels, rims or decals are never taken for the floor.
* **Charms in the gun file keep their bones**: before the parts are frozen,
  `capture_charms` reads the gun rig's charm chains (a bone named as a charm,
  keychain, pendant, strap, tassel or ribbon whose parent is not one, with the bones
  below it) and each part's charm weights. `keep_charms` then adds the chain to the
  character rig under a non-deforming holder bone (the gun's own bone above the
  charm, e.g. `gun_file_weapon_root.L`) that hangs from the hand holding that gun,
  moved like the gun, and gives the charm vertices their charm weights back (the
  rest stays on the hand). The holder is listed as a gun bone (`gun_bones` on the
  receipt rows), so the charm is recognised as hanging from that gun and follows
  the gun fit and charm motion like a charm on the character's own gun. A part
  whose vertex count changed while freezing keeps its charm rigid.
* The preparation receipt keeps `weapon_file_selection`. Convert, the review
  check and Analyze/Build compare it with the current gun file: a changed (or
  changed-texture), cleared or newly chosen gun file needs a new inspection; a
  moved but identical file is accepted.

### Charms on the gun

Charms, keychains, pendants, straps and tassels hanging from the character's gun
are found as moving parts (see **Gun charms** under Moving parts) and kept with
the other moving parts. What happens to them follows the gun choice:

| Gun choice | Charm |
| --- | --- |
| Replace the hero's gun | The charm's bones are kept (`dmb_secondary_*`) and hang from the hero bone the fitted gun mainly follows (`Plan.anchor`: the rigid bone, or the bone with the most of the original gun's weight). They are placed with the same rotation, scale and offset as the gun, and the charm's vertices keep their charm-bone weights (`weapon_fit.apply`); the rest of each gun vertex follows the gun's planned skin. |
| Keep it in the hand | Unchanged path: the charm hangs from the hand like any accessory. |
| Leave it out | The charm is left out with the gun (its groups are dropped from the kept moving parts; the Weapon tab removes them and offers them again when the gun comes back). |

Rust and Blender compute the same plan (`secondary_bones::selection_for` and
`selection(..., gun)`, with `Plan::hands`/`Plan::removed_ids` and
`gun_hands`/`removed_guns`); Blender receives the inspection's gun-bone marks as
`gun_bones` and counts the gun objects' own weights, and Convert checks that both
plans agree. Each fitted gun carries its own charms (its hand's anchor and fit);
a gun that copies the right gun keeps the copied charm rigid; charms on left-out
or unused guns are left out (the UI mirror is `removedGuns` in
`secondary-bones.ts`). Corrections keep the gun of their conversion.

During Build a part hanging from a hero weapon bone, a part with
`physics_kinds: {root: "charm"}` (set for gun charms when they are first kept) or
a charm-named part gets the **Charm** part type (`secondary_auto.rs`,
`charm_parameters`): its pivot stays on the gun, the rest swings with goal
0.5→0.35 along the charm, drag 0.3 so it settles within a few swings, no gravity
so it never sags into the gun when the gun is held sideways, and a travel limit of
about a third of the charm's reach (0.2–1.2 in). Stiffer is goal 0.7, drag 0.4,
travel ×0.6; More movement is goal 0.35, drag 0.2, travel ×1.4. **These amounts
are untested in game; they need an in-game check.** The **Physics** tab's **Gun
charm motion** switch (`CharmMotion` in `SecondaryMotion.tsx`) turns the swing on
or off (off sets the part type to Rigid, so the charm stays still on the gun) and
sets one amount for all charms, without converting again.

Separate weapon models cannot carry charm motion. None of the game's separate
weapon models has cloth (`astro_gun`, `priest_gun_crossbow`: no physics block;
`abrams_gun`, `werewolf/weapon_brk1`: `m_pFeModel = null`), weapon projects export
only the gun against the weapon skeleton without extra bones, and whether the game
simulates cloth on a weapon entity is unverified. A gun from a separate gun file
keeps its charm bones (see above) when it replaces a gun that is part of the hero
model.

### How the fit works (`weapon.rs`)

Both guns are measured the same way: the barrel axis (principal axis refined
on the front of the gun), the muzzle end (muzzle bones/attachments, part names,
the end away from the hand, or the thinner end), the up direction (the grip
hangs below the barrel), the grip (the hand holding it, parts named as the grip,
or the handle) and the length. Part names count as whole words: Muzzle, Barrel,
Nose, Front, Emitter, Suppressor, Silencer and Compensator for the front; Rear,
Back, Stock and Butt for the rear; Grip, Handle and Handgrip for the grip
(`weapon_fit.py _hints` gives their centres, charms are skipped; `weapon.rs fit`
uses front minus rear). `Frame.direction` records how the front was found and the
fit line shows it; **Flip direction** moves the muzzle to the other end, and a
flip against the front the part names gave is pointed out.
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
FBX/GLB round trips, gun-only weapon models, a gun in a second file used by a
character project (replace and keep) and as a weapon project's GLB source,
a rigged two-part gun file with a curve strap, a hidden part and a floor (merged,
and compared corner by corner with an unmerged conversion; `DMB_PROBE_MERGE=0`),
a pistol on its own gun bone carrying an unnamed two-link charm (replace and
keep, with moving parts kept; `DMB_PROBE_PHYSICS`), a dual gun file with an `[L]`
and an `[R]` pistol each carrying a two-bone charm on a dual-wield hero
(`DMB_PROBE_DUAL`; a gun and a swinging charm in each hand), on a one-gun hero
(the left gun left out) and as one gun copied into both hands
(`DMB_PROBE_LEFT=copy`), an unrigged statue) and runs each
through inspect → plan → prepare → native DMX validation against a captured
196-bone hero skeleton.

## Limits

Automatic results still need the in-game check: grip, aim and muzzle
position, elbow and knee bending, and clipping. Non-humanoid bodies
(quadrupeds, characters without two legs and two arms), models whose limbs
are not separated in the rest pose, and live rig controls that change the rest
shape still need a manual source copy. A separately modelled gun replaces the
weapon model for every hero that shares that model. Gun charms move only on guns
that are part of the hero model, and their swing amount is not yet checked in
game. Dual-wield fits (Haze) are checked against synthetic and decompiled data;
grip, aim and the two-gun animations still need an in-game check.
