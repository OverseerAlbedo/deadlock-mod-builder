# Model preparation and gun customization

How a character file becomes a Deadlock hero replacement, and what the
preparation step does automatically so that almost any rigged humanoid works.

## Where preparation sits in the app

The sidebar leads through five numbered steps (a check mark shows a finished
step); Motion and Extras are optional, Step timings, Logs and Settings are tools.

**Simple and Expert** (the switch in the top bar, stored on the PC in
`localStorage` `dmb.uiMode`, default Simple): Simple hides Motion, Extras and
Step timings in the sidebar, Tool diagnostics, About and the compiler backend
choices in Settings, Build's developer tools and raw receipts, the motion preset
picker, and on Prepare model the joint and bone matching tabs, the preparation
mode and the raw inspection, unless something needs a choice (unmatched bones or
joint problems open them; **Edit matches** opens them on request). Page
components take `expert` (default `true`, so component tests see everything);
only `App.tsx` passes the real mode. Required choices (unmatched joints, shape
key handling, red tool paths) are never hidden.

**Themes** (Settings → Appearance): *Original*, *Violet D* and *Mint D*. The
choice is `Settings.theme` (serde default `original`), saved at once by the
`set_theme` command rather than with the tool paths, so it survives restarts and
reinstalls like the other settings. The Theme Manager is `src/theme/themes.ts`:
each theme is one `ThemeTokens` set (background, surfaces, text, chrome, the
primary button, the accent family, the studio card and border, and `hue`), and
`applyTheme` writes them to `<html>` as the CSS variables the stylesheets read
(`VARIABLES` maps token → variable) plus `data-theme`. Every tinted shade in the
four stylesheets is an `hsl(calc(var(--hue) ± n) s% l%)` relative to the theme's
hue (153 reproduces the original exactly), so a theme recolors the tinted
panels, borders and viewport frames without listing them; the plain greys were
snapped to the neutral tokens (`--background` … `--text`) by lightness. Every
page shares `--surface` as its background and `--studio-card` /
`--studio-border` for its cards (`.studio-workspace .section` and
`.prep-card` alike). Success, warning and error colors (`--ok*`, `--good*`,
Completed, matched bones, red errors) are not themed. The sidebar mark is
`BrandMark.tsx` (the icon's crossed D in the theme's gradient) and the window
icon is switched with `WebviewWindow::set_icon` from
`src-tauri/icons/icon-violet.png` / `icon-mint.png` (also at start-up in the
Tauri setup hook). A taskbar shortcut keeps the installer's icon. Custom
themes (`ThemeCustomizer.tsx`, Settings → Appearance → Customize) are
`ThemePreset`s in `Settings.custom_themes` (saved at once by
`save_theme_presets`): three base colors go through `deriveTokens` (surfaces
step up from the background, text greys mix toward it, the accent family keeps
the accent's hue, the primary button takes whichever text reads on the accent)
plus any token set by hand under Advanced, and the chosen built-in D as the
window icon (`icon_for` in `desktop.rs`). `presetToTheme` fills missing tokens
from the original.

Prepare model has one **3D view** on the right with three modes: *Your model*
(the source file and its own animation), *Joint check* (the mapping viewer) and
*Result* (the converted DMX). Rare fit options (arm skinning, arm alignment,
finger bend, skin influences) are under **Customize fit** in Fit & review.

| Step | Where | What happens |
| --- | --- | --- |
| Setup | `src/setup`, `src-tauri/src/setup.rs` | Game VPK, Source2Viewer CLI, compilers, Blender. |
| 1. Character & model | `src/components/ProjectOverview.tsx` | Pick the hero model to replace (`project.original_model`), your model file and, optionally, a gun file. The picker labels models from the installed game's hero definitions (`scripts/heroes.vdata`, `m_strModelName`): the model a hero definition loads is that hero's main target, and roster models the game does not load are marked *Not used in game* (Rem loads `familiar_wip.vmdl`). Each entry has a tag: *In game*, *Alternate*, *Not playable yet* (roster `"playable": false`: Nurse Harrow and Violet, whose files are in the game though they cannot be picked; the game's definitions do not mark them) or *In development* (only `m_bDisabled` hero definitions load the model, `GameTarget::Disabled`). Analyze warns when a project targets such a model. A Blender, FBX or glTF model goes straight to step 2 and is prepared automatically (`dmb-model-source` event with `auto: true`); a DMX is analysed as before. |
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

**Heroes with extreme proportions** (`retarget.py hero_extremes`,
`HERO_FIT_POLICY`): ordinary heroes have arms 0.80–0.96 and legs 1.49–1.82 times
their pelvis-to-head length. Shiv's right arm is 1.19 (a long forearm; the left is
0.96) and Wraith's legs are 1.37. With **Keep source proportions** the source used
to keep its own leg length and stand on the floor, which lifted the body above a
short-legged hero, and the arms (or Shiv's long forearm alone) stretched to reach
the hero's hands. When an arm is longer than 1.05, the arms differ by more than
10%, or the legs are shorter than 1.45, the torso is scaled evenly so the hips and
neck land on the hero's, the legs follow the hero's leg joints (source
cross-section kept), and with **Align arms to hero joints** the arm length is
spread over the whole arm: the elbow may sit up to 8% of the arm's length from the
hero's elbow, the hand still lands on the hero's hand. The audit records
`hero_fit` (proportions, torso scale, per-arm scales) and preparation warns that
the hero has unusual proportions. `scripts/test_hero_proportions.py` covers an
ordinary, a Shiv-like and a Wraith-like hero. Other heroes are unchanged.

### Textures (`material_textures.py`)

Each material's base colour is found from its node graph: the image feeding
the Principled BSDF base colour, or, in game rips, an image wired straight into
the Material Output or into a Diffuse, Emission or Toon colour (also through a
mix that only tints one image). A material with a single unsuffixed image uses
it as the base colour. Channel suffixes (`_n`, `_nrm`, `_orm`, `_mask` ...)
choose the other slots. Prepared material names have the form
`Mesh__Material`; the Appearance planner also matches an image named like the
source material inside that name.

MMD materials (mmd_tools) feed three images into the MMD shader group: the base
texture (`mmd_base_tex`), a toon ramp (`mmd_toon_tex`) and a sphere (shine) map
(`mmd_sphere_tex`). Inside the group they meet in a chain of color mixes
(base × toon, then × sphere and + sphere); followed through it, base and toon
made one branch ambiguous and the sphere map became the color (SiViolet's MMD
version: skin.png on the body, a .bmp sphere map, which cannot be copied, on the
clothes). Toon and sphere image nodes (`MMD_SHADING_NODE`) are never a color
source, and such materials carry the review note that toon effects are not
baked. `scripts/test_blender_material_recovery.py` has the mmd_tools group case.

**Face layers** (`material_textures.py source_hidden`, `face_layer`;
`deadlock-materials face_layers.rs`). A material the source shows fully
transparent (the MMD shader's Alpha, else a single Principled BSDF's Alpha, else
the viewport alpha, at 0 and not linked) is a layer an expression fades in:
MMD blush (赤面, 照れ, 頬, cheek), tears (涙, tear) or other (sweat, effect
quads). Preparation records it as the material's `face_layer` and notes it. A
game build has no expressions, so when material files are written
(`patch_face_layers`, after the appearance changes) each such material is fully
transparent (`F_TRANSLUCENT 1`, `TextureTranslucency1 [0 0 0 1]`) unless its kind
is shown in `Customization.face_layers` (Appearance > Face details). A shown
layer gets a grayscale mask written from its color image's alpha
(`<image>_a.png` beside the staged copy) as its translucency, so the blush keeps
its soft edges; an image without transparency is drawn as it is. Before,
SiViolet's MMD version drew its blush and tears solid (a red face). Switching
needs no new preparation; finding the layers does. Re-preparing always takes
`face_layer` from the new preparation (`refreshed_material`).

When the same hero is prepared again, a material keeps what the user changed,
but a texture or tint that is still the previous preparation's automatic pick
(the same prepared file name, which carries the image's hash) takes the new
automatic pick (`service.rs refreshed_material`); the computed candidate lists
always come from the new preparation.

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
the hash to the new path (`service.rs renamed_materials`). Non-English names are
spelled in Latin letters first (`readable_names.py`, `material_textures.py
prepared_slug`): Chinese characters and Japanese kanji by their Mandarin reading
without tones from the Unicode Unihan data (`cjk_readings.txt`, 150 KB, rebuilt
by `scripts/make_cjk_readings.py`), kana in Hepburn romaji, Hangul by syllable,
accents dropped, so `头发` becomes `toufa_…` and `スカート` `sukaato_…` instead of
`__…`. ASCII names keep exactly the name they had. Bone names are not changed
(the compiler's alias rules depend on them). `scripts/test_readable_names.py`
checks the spellings with plain Python. Staged texture copies
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
form a mirrored pair in front of the chest in the upper torso: above the spine and
above 35% of the way from the pelvis to the neck, never hanging from the pelvis and
never named as a butt. "In front" uses the facing direction from the feet to the
toes; only each foot's lowest child counts, because boot straps, heels and pendants
hanging from a foot can point backwards (that flipped the facing and let a butt
pass as breasts). Turning it on keeps them as moving parts and records
`physics_kinds: {root: "breast"}`.

**Butt motion** is separate from breasts and off by default: a new model's first
inspection sets `butt_motion: false`, while projects from before butt motion
existed (unset) keep their saved groups unchanged. Butt bones are the same kind of short torso children named
butt/buttock/glute/gluteus/booty/bum/ass/pigu/tunbu (whole name parts, so Button
is not one), 臀, 屁股, お尻, ケツ, or 尻 outside 尻尾 (tail), or a mirrored pair low on
the hips (below a quarter of the way from the pelvis to the neck) that is not in
front of the body. Breasts win over butts. While butt motion is off, butt bones stay
on the hips even when the source gives them physics, and saved butt groups are
dropped from the selection instead of failing as stale. Butt-named groups in older
projects (MMD `尻1.L`/`尻2.L` with rigid bodies, as in janeVen) get the Butt part
type; the Physics tab's switch turns them off without converting again. Model
preparation's **Keep butt bones** (`buttMotionOptions`) keeps them as moving parts
with `physics_kinds: {root: "butt"}`; the Physics tab's **Butt motion** switch and
amount work like the breast ones. Build gives them the **Butt** part type: the
breast proxy (pairs linked per anchor and kind) with goal 0.6, drag 0.15, no
gravity and travel about a quarter of the radius (0.25–0.8 in); Stiffer is goal
0.75 and travel ×0.7, More movement goal 0.45 and travel ×1.3. Needs an in-game check.
All three detectors share these rules and `hanging-chains.json` (`butt_bones`,
`expected_roots_with_butt_motion`, `butt_names`).

**Fine-tune movement** (Physics tab, Simple and Expert): four sliders per part
type found in the model (hair, streamers, tails, ears, cloth, long skirts,
accessories, breasts, butt, gun charms), stored as
`customization.secondary_motion.tuning: {kind: {stiffness, damping, gravity,
swing}}`, each from -1 to 1 with 0 (or missing) the automatic value. They are
keyed by part type, not by moving part, so a saved preset fits any model. Build
applies them in `secondary.rs generate_inner` right after `secondary_auto::append`
to that part's moving nodes only (`secondary_auto::tune`; fixed anchors keep
their values): stiffness moves the pose pull toward 1 or down to a quarter,
damping toward 0.5 or to none, gravity toward 1.5 or to none, swing scales a
travel limit between ×0.25 and ×2.5, and a part without a travel limit (0, free)
stays free. The per-part Movement choice (Stiffer, More movement) is applied
first. TangTang presets ignore tuning, and any tuning stops the accepted
TangTang reference from being reused for Automatic. **Saved presets**
(`physics_presets.rs`, `state/physics-presets.json`, up to 50, names up to 40
characters) hold only the tuning; applying one replaces the project's tuning.
Before the first Analyze every common part type is listed.

**Source rig springs** (item 21): Inspect reads the spring settings a rig already
has (`resources/blender/source_springs.py`): VRM spring bones from the VRM
add-on's data in a .blend (read as stored ID properties, the add-on is not
needed) or from a .vrm/.glb/.gltf file (`VRMC_springBone` joints, VRM 0.x
`secondaryAnimation` groups over whole chains), and MMD rigid bodies bound to a
bone (angular damping, rotation springs of their GENERIC_SPRING joints). They are
converted to the builder's scale (VRM: goal = 0.45 × stiffness, drag = 0.5 ×
dragForce, gravity = gravityPower; MMD: goal = 0.05 + 0.15 × log10(1 + spring)
(springs run from 0 to 100,000 for a locked joint), drag = 1 − (1 − angular
damping)^(1/30) (Bullet damping is per second), gravity 1; checked on janeVen:
152 bones, median spring 0, median damping 0.99), averaged per kept moving part (`springs.ts
partSprings`) into `preparation_options.physics_springs` and used only when the
Physics tab's **Start from your model's own spring settings** is on
(`secondary_motion.use_source_springs`, off by default, so Yvonne's accepted hair
is unchanged). `generate_inner` then replaces the automatic goal, drag and
gravity of that part's moving points (pinned points stay pinned) before Fine-tune.
`scripts/test_source_springs.py` checks the conversion with plain Python.

**Gravity in the game's unit** (October 17, 2026). Valve's own cloth
(`ClothChain`, `gravity_z = 1`) compiles to 360 per node in every hero, while
the compiler copies proxy streams as they are. Automatic motion keeps its 0–1.5
gravity scale everywhere (motion profiles, Fine-tune, source springs, the
movement preview) and `secondary_auto::write_maps` writes it multiplied by
`GAME_GRAVITY` (360). Before, automatic parts had about 1/360 of normal gravity
in game: tails looked static, sleeves only moved with sharp arm motion. Hair,
ears, charms and breasts have gravity 0 by design and stay so; the Fine-tune
gravity slider of each part type now works in game. The TangTang presets keep
their accepted proxies unchanged. Read a build's values with the development
example `deadlock-resource/examples/fe_stats.rs` (or `fe_parts.rs` per bone).

**Leg colliders** (`body_colliders.rs`, automatic motion only). Analyze measures
a capsule along each thigh (`leg_upper_*` to `leg_lower_*`, starting 20% below
the hip so a waistband never sits inside it) and shin (`leg_lower_*` to
`ankle_*`) from the character's own leg: the 95th percentile distance of the
leg-dominant vertices. Each radius is reduced to clear every moving proxy node
at rest by `CLEARANCE`, and a capsule thinner than `MIN_RADIUS` is left out.
They are stored in `Setup.body_colliders` and written as `ClothShapeCapsule`
nodes beside the proxy (`proxy_source::author_direct_proxy`), as the cloth
authoring flow did for the Vindicta reference; the compiled cloth gets matching
`m_TaperedCapsuleRigids` (`examples/fe_capsules.rs`). Skirts, coat tails and
tails then stop at the legs instead of passing through the body.

Up to two **hip capsules** on the pelvis (`hip_colliders`, at `HIP_LEVELS`: hip
joint height and 0.15 thigh lengths lower) run from hip joint to hip joint
behind the hips. The butt's depth is the 95th percentile of how far the
hip- and thigh-dominant vertices around hip height reach behind the hip joints
(back is the opposite of side × up). Each capsule tries 17 set-backs from the
hip line to half that depth, takes at most half the depth as its radius,
shrinks it to clear every moving node, and keeps the placement whose back
surface reaches furthest. Skirt bones usually rest inside the butt (SiViolet's
back panel bones sit 1.7 in behind the hip joints), so the capsule ends up
just in front of them: a back panel cannot move further into the body than it
rests. `examples/parts_report.rs` prints each collider and, as FREE lines,
its unshrunk radius and the moving nodes that limit it.

**Skirt travel stops at the body** (`long_skirt.rs`, `body_colliders.rs
fabric_gaps`). A skirt joint's travel limit (`torso × (0.02 + 0.30 t)`) let the
fabric of SiViolet's back panel reach her body: row 3 could travel 2.5 in with
its fabric 1.45 in from the body. Each drawn vertex is fabric of the moving bone
that weights it most (moving bones weight it at least half) or body (bones
outside every moving part weight it at least half); a joint's gap is the
nearest body point to any of its fabric (`BodyGrid`, searched up to 0.35 torso
lengths), a joint without fabric takes the gap of the nearest joint of its
panel that has some, and the travel becomes `min(travel, max(gap − CLEARANCE,
BODY_TRAVEL_FLOOR))` (`body_capped_skirt_travel`). The body is measured in the
bind pose: this covers fabric over pelvis-skinned skin such as the butt, while
thigh swing is the leg capsules' job. Fine-tune swing still scales the capped
values. `parts_report.rs` with `SKIRTGAP=<name part>` prints each joint's travel
and fabric-to-body gap.

**Head and neck colliders** (`body_colliders.rs head_collider`, `NECK`). The
neck gets a capsule from `neck_0` to `head`, measured like the legs (95th
percentile of the neck vertices, shrunk to clear every moving node). The head
gets a short upright capsule (`HEAD_HALF_LENGTH` 1 in above and below its
centre). Its radius is the distance to the nearest moving node less
`CLEARANCE`, at most the median distance of the vertices weighted at least 90%
to the head (static hair included); its centre is the best of a grid up to 2 in
up or down and 1.5 in forward or back from those vertices' centre, so a helper
node inside the head (SiViolet's strandA weights at 1.35 in) does not shrink it
to nothing. SiViolet's MMD version gets a 4.45 in head and a 1.41 in neck (her
collar pieces sit at the neck), the Blender version 3.25 and 2.22 in; Valve's
Violet has a 4.0 in head capsule. Shoulders and upper back have none yet.

**Parts hanging on one bone** (`secondary_auto::simulated_bones`). A part with
exactly one weighted bone (a hair bun) gets proxy points only on that bone and
the group bones above it; mesh-less helper bones below it stay unsimulated and
follow it. SiViolet's buns (`L/R_hairD_01_jnt`) each carry an unweighted
`wingB` chain of three bones joined to the bun's 0.78 in ring by rods; swinging
freely, it twisted the right bun's ring (the two rings sit at different angles,
since MMD bones have no orientation) and flung the 5 in bun around. Parts with
mesh on several bones keep every bone, mesh or not (their helper bones shape
the motion). If a bun still bounces, its root ring is the next suspect: hair
gives it attraction 0.5 with no damping, a spring like the ears were.

**Shorter skirt panels** (`long_skirt.rs`). A panel qualifies as a long skirt
on its own when its weighted joints and its visible fabric reach `SKIRT_REACH`
(0.45) torso lengths below the pelvis and span `SKIRT_SPAN` (0.6); until
October 2026 that was 0.5 and 0.65, which SiViolet's MMD version (a skirt ending
around mid-thigh) just missed in all 16 panels. A panel that reaches only
`JOINED_REACH` (0.4) and spans `JOINED_SPAN` (0.5), in its joints or in its
visible fabric, joins when it shares at least three rendered triangles with the
skirt on the same anchor, panel by panel (a ring of panels joins through its
neighbors). SiViolet's back center panel (8 weighted joints, the others 10)
used to stay plain cloth with no travel limit and swung into the butt when she
walked backwards.

**The hero's own cloth names** (`Setup.hero_vertex_sets`, automatic motion
only). A hero's animation clips can carry legacy `AE_CL_CLOTH_STIFFEN` events
(Value, SpeedIn, SpeedOut, Length, VertexSet) that pull cloth toward the
animation by vertex map name. Violet has 27 such clips: the run blend's center
clip `out_of_combat_run_center` (so while she speeds up from standing or slows
to a stop), jumps, dashes, slides, zipline and rope clips, on `pony_tail_vm`,
`hair_front_vm`, `tassel_vm` and `apron_{inner,outer}_{l,r}_vm`. Haze has none.
A replacement's cloth had no map of those names and the whole cloth stood still
for about two seconds on every start and stop (inferred from the game's
behavior: a name it cannot find seems to mean every node). Analyze reads the
names from the game's compiled model (`secondary::hero_vertex_sets`, the
`m_VertexMaps` `sName`s) and `secondary_auto::write_maps` adds an all-zero
`cloth_vertex_set_<name>$0` stream for each; the compiler turns each into a
map of that name and hash with no nodes (`nVertexCount` 0), so the events find
their map and stiffen nothing. The rest-constraint audit accepts such empty
maps only (`deadlock-resource constraints.rs`). Read a clip's events with
`deadlock-resource/examples/clip_events.rs` and a model's maps with
`fe_field.rs MODEL m_VertexMaps`; `deadlock-vpk/examples/vpk_dump.rs`
extracts a hero's clips from the game VPK.

**Parts that stay still** (`Setup.still_roots`, automatic motion only). The
compiler turns a pose pull of 1 into an attraction of 0.5 with no damping,
which is a spring, never a lock (ears with Ear motion off bounced). Parts of
kind Rigid (Ear motion off, gun charms off, a Rigid choice) therefore get no
proxy points at all (`generate_inner` skips them) and their bones follow the
body like any skinned bone; every group is attached to a body bone, so nothing
moving sits above them. `Setup::simulated_groups` leaves them out of the
solver's selected and weighted bones (`deadlock-pipeline secondary_motion.rs
selected`), which the compiled-model check needs, and out of the roll-lock
chains. A model whose every part is Rigid is refused with a message to turn
movement off.

**Streamer bend limits and drag.** The compiler links a streamer's joints
only to their neighbors, and its lengthwise rods may shrink to 80%, so a
long streamer folded up like an accordion on quick turns (SiViolet's 30-joint
sleeves). Valve's own cloth has extra rods two and three joints apart.
`Setup.strand_bends` lists every streamer chain (3 or more weighted joints)
and `sheet_stability::apply` (`Plan.strands`) adds one rod from each joint's
position node to the one two joints further: no shorter than the chord when
the joint bends `STRAND_BEND_DEGREES` (35) beyond its rest bend, no longer than
both links laid straight. It runs after compile and again at install from the
native snapshot, like the fabric rods (`Proof.strand_bend_rods`). The same
pass tethers every joint to the static point the streamer hangs from (the one
next to its root ring, closest to the root's position node): no longer than
the streamer's rest length up to that joint, 1% of it at least, with the anchor
weighted 0 (`Proof.strand_tethers`). A streamer can bend and swing freely but
not stretch; SiViolet's 30-joint sleeves grew visibly longer on quick starts,
since the compiler gives them only neighbor rods. A streamer with no static
point next to its root is refused. New rods never share a node within one SIMD
pack, as the compiler's own packs never do. Drag slows
every node, so streamers with more than `LONG_STREAMER_JOINTS` (10) weighted
joints get their drag multiplied by 10 / joints
(`secondary_auto::streamer_drag_multiplier`, Automatic setting only). It was
added for SiViolet's sleeves hanging still for a second, whose real cause was
Violet's cloth-stiffen events (above); revert it if long streamers swing too
loosely. Valve's own long cloth has no drag. Read a build's rods with `examples/fe_chain.rs`.

**Hair buns** (`parts.rs hair_bun`). A Hair part that is one round piece on one
weighted bone (no axis more than 1.6 times the next) or has a bun name (the word
bun, 団子, 丸子, 髻, odango) is Rigid, so it is not simulated and follows the
head; an explicit part type still wins. SiViolet's buns (`L/R_hairD_01_jnt`,
about 1.5 x 1.4 x 0.8 in) bounced; the user wants buns still.

**Long sleeves.** A sleeve name (`sleeve`, 袖) on one thin, flat chain of at
least 6 weighted joints (`parts.rs long_strip`) is a streamer, even where its
mesh joins the rest of the sleeve; so is any such strip whose mesh only joins
groups with at most a quarter of its joints and length, like a cuff or a band
(`joined_only_to_small_parts`). Strips joined to panels of their own size (a
skirt) stay cloth. SiViolet's MMD version names her floor-length sleeves 袖1.L to
袖24.L; both join their cuffs and stayed stiff cloth.

**Part types that pair up.** A left/right pair (`parts.rs mirror_key`: L/R,
Left/Right as a word, 左/右) where one side became cloth only because its mesh
touches a moving group the other side lacks takes its streamer twin's type
(`harmonize_mirrors`; SiViolet's left sleeve and its forearm cord). A ribbon
or tassel name on a long, slender but round chain (a cord or tassel bundle) is
a streamer, not stiff cloth; tassel names (tassel, タッセル, 流苏) are streamer
words. On the Physics tab, **Ear motion** turns ear groups off (part type
Rigid; anything hanging from them, such as an earring in the same group, stays
still too) and back to automatic.

**Movement preview** (items 10 and 18; Physics → Your character → Movement):
`deadlock_pipeline::motion_preview::build` reads the prepared DMX skeleton and the
hero's own clips (`stance::find_ranked`, candidates best first: the gun-ready
idle, `weapon_run_n`, and a jump of `jump_ground` (≤1 s) + an in-air clip (0.6 s)
+ a landing (≤1.2 s) + 0.7 s of the idle to settle). A candidate whose joints lie
far below the rest skeleton is an additive layer (Haze's `landing_impact_idle`,
pelvis at the floor) and is skipped for the next one. Parts of a jump are blended
over 6 frames (quaternion slerp of the joint transforms). Clips are sampled at
60 fps (`stance::frames`, linear keys, the clip's own joint tree) into a binary
packet of rest-to-pose 3x4 matrices per bone and frame, with each part's frame
count in `segments`. Analyze records each moving bone's motion values before
Fine-tune in `Setup.preview_nodes` (the average of its proxy points). The page
(`src/preview/MovementViewport.tsx`, GPU skinning on one three.js skeleton whose
bone inverses are identity) blends between the two nearest frames at any speed
(`sampleSkins`; speeds 1×, 0.75×, 0.5×, 0.25×) and simulates the moving parts in
`src/preview/motion.ts`: each moving bone swings a point (its child, the middle
of its mesh for a breast, or a short way on) as a spring-damper on a pendulum,
120 Hz substeps: the pose pull is a spring of frequency `40 × goal` rad/s toward
the animated (unswung) point, the drag a per-second damping from the per-frame
share (at least damping ratio 0.18 so swings settle), gravity 386 in/s² × gravity,
the pendulum length from the bone's head (which follows the parent's swing) and
the travel limit from the animated point; goal ≥ 0.98 is rigid. The bone then
turns about its pivot and children follow. The simulation runs in the
character's own space: the bone's head and the animated point move smoothly
across the substeps between frames (`pivotBefore`, `tipBefore`), the drag acts
on the part's own motion through that space (not on the character's speed,
which pinned hair back like a storm when running; not relative to the carrying
bone either, which whipped long chains), the damping ratio is at least 0.55
(the game's solver damps hair that has no drag and no gravity of its own, so
the automatic hair values would ring freely otherwise), and the character's
travel enters only through its changes of speed (`inertia`: a take-off or
landing kicks the parts by at most `KICK_MAX`, a smooth arc counts for one
gravity at most, the first measured speed of a clip is no kick, and a jump of
the offset over 60 in is a clip looping round and is ignored). `OMEGA_MAX` is
22 rad/s at full pose pull. `motion.ts tune` must stay identical to Rust
`secondary_auto::tune` (same test numbers). The run moves the character
forward at about 260 in/s (the grid floor scrolls under it, the character stays
in view) and a jump rises about 55 in between the end of the take-off and the
start of the landing (`jumpArc`), shown as height. It approximates the game's
cloth solver for tuning by eye.

**Hero cloth in decompiled sources** (October 2026 game files): Source 2 Viewer
now writes the hero's own cloth as a `Softbody` and, for proxy-mesh cloth, a
`ClothProxyMeshList` with the proxy file, and no longer lists the compiler's cloth
bones (`$cloth_m0p…`, cloth flag `0x400008`; 17 heroes, Yamato 387 of 509 bones)
in the Skeleton node. Older decompiles listed them as bones named `_cloth_…`,
which the build compiles as plain bones and renames back when the game's
compiled cloth is restored (`canonical_aliases`, `deadlock_cloth::restore`). Build
keeps that flow: `modeldoc::with_compiled_cloth_bones` lists the missing bones
from the compiled skeleton (`source_audit().runtime_motion.skeleton`) the old way
(parent-relative origin and Source angles via `angles_of`, `do_not_discard`, at
the end of the parent or the Skeleton for roots, compiled order),
`with_compiled_bone_transforms` gives listed bones whose origin or angles drift
from the compiled skeleton the compiled values (the new decompiles write some
physics bones about 0.002 inches off the game's model, Lady Geist's fur among
them; the compiler takes the Skeleton node's transforms over the DMX's, and
restoring the game's cloth needs the exact bind pose: "Bind transform outside
original-physics transfer budget"), and `without_decompiled_cloth` takes the
Softbody and cloth lists out, all before `merge_template` / `patch`.
`deadlock-pipeline/tests/cloth_bones.rs` checks this on a decompiled hero (run
with --ignored).

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
breast, such as Yvonne's `clotch_piyi_*_a` strips on `胸上2.L/R`) get their own
rings and swing like charms from the breast ring (no gravity, bounded travel):
as accessories with gravity and unbounded travel they sagged and flapped through
the body. Without rings the native fit leaves those bones unfitted and Build stops.
Settings: goal 0.45, drag 0.1, no gravity, no tether,
travel about 45% of the breast radius (0.4–1.5 in), and never more than 30% of
the distance to the other breast. Stiffer is goal 0.62 and
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

**Long hair and tails.** Unnamed strands on the head get the Hair part type (names
in other scripts are often stripped to underscores, so a 19-bone strand called
`F_0_1 … F_18_1` used to become cloth and barely moved), and unnamed long, round
strands on the hips (four or more weighted bones, not flat, not joined to other
parts) get the Tail part type. Every node of a strand is pulled back to its pose,
so a strand with more weighted bones than `DENSE_CHAIN_NODES` (6) has its pose
pull multiplied by 6/bones: a 19-bone strand moves like a 5–6 bone one (Yvonne's
accepted ponytails have 5 and do not change). Dense tails and accessory strands
also get tethers (no stretching under gravity) and a travel limit of 60% of their
distance along the strand. Dense hair, tail and accessory nodes cannot travel
further than their distance to the body (`Body` in `secondary_auto.rs`: vertices
skinned to bones outside every moving part), with a floor of an inch or 15% of
their free travel so a strand resting on the back still moves. This reduces
clipping; the moving parts still have no collision shapes.

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
from before the shapes show point samples. **Pose: Holding / Rest** (Holding by
default) shows the hero as it holds the gun in game: `stance.rs` reads the first
frame of the hero's `weapon_stand_idle.dmx` (then `primary_stand_idle`,
`stand_idle`, `idle`, then the best other `*stand_idle*`: with a gun word first,
ability/item/reload stances last). Clips are `.dmx` files next to a `.vnmclip`
of the same name, wherever the sources keep them (`clips`, Warden's `clip`,
Sinclair's `dmx/animation/ag2`; Lady Geist's clips are in heroes_staging/ghost),
the hero's own model folder first, older-style animations last. It walks the clip's own joint tree (it can differ from the compiled skeleton: Haze's
pelvis hangs under `root_motion` in clips), and stores a rest-to-stance matrix per
bone. `Reference.stance` keeps the posed body (`body_shape`, skinned with the
same weights) and each hand's matrix (the bone carrying most of that gun's
weight); the viewer multiplies the fit matrix, the original gun and the fire
arrows by the hand's matrix. Preview only: the fit is still measured in the rest
pose. Cloth-simulated bones (skeleton flag `0x400000`) follow their nearest
unsimulated ancestor (`follow_unsimulated`). The posed body leaves out other body
group choices (`optional_meshes`), parts mostly weighted under a `*_scale` bone,
weights whose bone lies on average more than a third of the hero's height from
its vertices (`repair_weights`: decompiled meshes of the October 2026 files name
the wrong bone, Abrams' coat on `muzzle_fx`) and triangles the pose stretches past
three times their rest size. `tests/hero_audit.rs` runs every roster hero
through this (decompiled sources per hero) and checks it stands upright with each
gun following a hand; all 44 pass. Beginner buttons (Flip direction, Turn 90°, Smaller/Bigger,
Back/Forward/Up/Down/Left/Right, Reset) adjust the selected hand (**Both guns**
moves a dual-wield hero's two guns together); **Advanced gun
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

## Hero colors (Extras → Colors)

Not part of preparation: Colors recolors the hero's **own** compiled materials
for a project without a replacement model (`Customization.hero_materials`,
`deadlock-components materials.rs`). The list is the `.vmat` references of the
hero's compiled model; each material's DATA block (KV3) is read for its shader
and parameters, and every other `.vmdl_c` in the pak is scanned (its reference
table only, about 4 s for the 6,419 models of October 2026) for other models that
use it (`used_by`, recolored only with `allow_shared`).

The build edits the compiled material in place, never a decompiled copy:
DATA is decoded, existing `m_vectorParams` values change (the tint
`g_vColorTint1` is replaced; `g_vAlbedoContrastSaturationBrightness1` y and z are
multiplied by the saturation and brightness factors), DATA is encoded again
(`deadlock_resource::binary::encode`, KV3 v1) and the resource is rebuilt with
every other block byte exact (`container::rebuild`). Only `pbr.vfx` is accepted,
and a parameter the material lacks is refused, never added. The package holds
just the recolored `.vmat_c` files; their textures stay the game's. The receipt
(`ComponentReceipt.materials`) records each original hash and its output, and
snapshot and install checks recolor the current game material again and compare
bytes.

Why not decompile and recompile: Source 2 Viewer splits packed textures into PNGs
whose names collide (Haze's `hazev2` got `haze_v2_color.png` from two different
textures), its `.vmat` carries a nested `Compiled Textures` block, the material's
inputs live in the game's `core` search path, and re-encoding BC7 textures loses
quality. Roughness and metalness are baked into textures (`g_tNormalRoughness`,
`g_tColor` alpha), opacity and emission need static combos (`F_TRANSLUCENT`,
`F_SELF_ILLUM`) these materials lack, so none of them is offered; a hue shift
needs a new texture and is a color range (below). A material whose tint mask (`g_tTintMaskRimLightMask`) is not a
`materials/default/` texture tints only its masked areas; the page says so.
Checked on October 18, 2026: all 770 hero `pbr.vfx` materials have
`g_vColorTint1` and 679 have the albedo vector; every decodable game material
re-encodes to equal values.

### Color ranges

A hero color's `ranges` (`HeroColorRange`, up to 8) recolor only the base-color
pixels they select. The pixel function is `deadlock-components color_ranges.rs`
and, for the preview, `src/extras/heroColorRanges.ts`; both check the shared
fixture `deadlock-components/tests/fixtures/hero-color-ranges.json`. A range
selects by hue (centre and half width, wrapping around red), saturation and
brightness (HSV value), each faded by the softness (30° and 0.15 at softness 1),
then shifts the hue and scales saturation and brightness like the replacement
model color editor. Every range selects by the original color; several ranges
add up in order. Hue alone is not enough: Haze's orange clothes (hue 25,
saturation 0.73) and her tan and beige skin (hue 20–27, saturation 0.16–0.47)
share a hue.

The build (`texture_recolor.rs`) rewrites a copy of the game's `g_tColor`
texture in place: the header, flags, format, mip count, editor data (RED2) and
display rectangle stay byte exact. Every mip is decoded and recolored from its
own texels (no mip is filtered again), and only BC7 blocks whose texels change
are encoded again (`block_refit.rs`). A refit keeps the block's mode, partition,
rotation, index selection, p-bits, alpha endpoints and every index bit that feeds
alpha, and fits only the color endpoints by least squares (modes 4 and 5 also
choose their color indices again, from the old indices and from the colors'
bounding box), so the decoded alpha, where Deadlock keeps metalness, stays byte
exact; modes 0–3 (alpha 255) become mode 5 with a constant alpha. Every refit
block is decoded with `bcdec_rs` and a changed alpha is an error. The fallback
image (extra data kind 1, the texture at 32×32 in BC3) is recolored by refitting
the color half of each block and keeping its alpha half. The four header floats
after the flags (the mean linear color) move by the change of the top mip's mean.
Only BC7 textures are supported (721 of the 723 hero base-color textures of
October 2026; one DXT5 and one RGBA8888 are refused).

The copy goes to `materials/dmb_hero_colors/<texture name>_<16 hex>.vtex_c`, a
fingerprint of the policy, the material, the game texture and its hash and the
ranges, and must not exist in the game. The material's `g_tColor` names it, and
its reference table (RERL, ids `deadlock_vpk::resource_id`, MurmurHash64B of the
lowercase name without `_c`, in name order) drops the old texture unless another
parameter still uses it; all 774 hero material tables of October 2026 re-encode
byte exact with these ids. The receipt entry records the texture (`RangeTexture`);
snapshot and install checks read the current game texture and build it again.
The packaged texture is not a game file: the dependency closure and original
hashes use only the replaced game files, and install and package checks take
the texture paths from the receipt. Haze's 2048² texture (10 mips): about 92,000
of 350,000 blocks change for her orange clothes, 1.6 s. Not recolored: the
alternate reveal texture `g_tAltColor_Reveal` (a different image, shown only when
the game raises `g_flAltTextureAmount`, 0 for Haze).

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
