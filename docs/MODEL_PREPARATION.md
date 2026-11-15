# Model preparation and gun customization

This guide explains how a character file becomes a Deadlock hero replacement. It
covers what each of the five steps does, what the preparation step fixes by
itself so that almost any rigged humanoid works, which choices are yours, and
where the limits are.

## The five steps

The sidebar leads through five numbered steps, and a check mark shows a finished
step. Motion and Extras are optional; Step timings, Logs and Settings are tools.

| Step | What happens |
| --- | --- |
| Setup | Deadlock, Blender, Source 2 Viewer CLI and Reduced CSDK 13 (once, in Settings or the new-user guide). |
| 1. Character & model | Pick the hero model to replace, your model file and, optionally, a gun file. |
| 2. Prepare model | Turn your file into a DMX bound to the hero's exact skeleton. |
| 3. Appearance & weapons | Materials, textures, mesh mapping, equipment and the gun. |
| 4. Physics & preview | Secondary motion (hair, cloth, breasts, gun charms) and a 3D view of the prepared character. |
| 5. Build & install | Inspect the project, compile the model and materials, pack the VPK and install it. |

Blender runs in the background as a geometry engine. Your own file is never
written.

### Simple and Expert

The switch in the top bar chooses Simple (the default) or Expert. Simple hides
advanced pages and tools, including the joint and bone matching tabs on Prepare
model. Those tabs open by themselves when something needs a choice, and **Edit
matches** opens them on request. Required choices, such as unmatched joints,
shape key handling and red tool paths, are never hidden.

### 1. Character & model

The hero picker labels models from the installed game's hero definitions. The
model a hero definition loads is that hero's main target, and roster models the
game does not load are marked *Not used in game*. Each entry has a tag: *In
game*, *Alternate*, *Not playable yet* or *In development*. Analyze warns when a
project targets an *In development* model.

A Blender, FBX or glTF model goes straight to step 2 and is prepared
automatically. A DMX is analysed as it is.

### 2. Prepare model

**Prepare automatically** inspects your file, applies the suggestions, checks
the result, converts it and uses the model in one go, then continues to step 3.
It stops only where a choice is needed. **Prepare model** does the same from the
review tabs: the click counts as your review, and the check runs again right
before converting. Choices that need a new inspection (the gun file, Scene
objects) run it again by themselves. A project that is already prepared, or that
started from a DMX, says so and offers to continue.

Prepare model has one **3D view** with three modes: *Your model* (the source
file and its own animation), *Joint check* (the mapping viewer) and *Result*
(the converted model). The Result view draws its grid at the game's floor, not
at the model's lowest point. Rare fit options (arm skinning, arm alignment,
finger bend, skin influences) are under **Customize fit** in **Fit & review**.

Preparation has two phases:

1. **Inspect.** The app copies your file into the project workspace (a `.blend`
   with its textures packed), reads the hero's skeleton and gun from the game
   files, lists your bones, meshes, shape keys, scene objects and gun
   candidates, and proposes a body role and a target for every weighted bone. A
   model that lies down is rotated upright and inspected again.
2. **Convert.** Each source bone is fitted to the hero from the reviewed body
   roles, the vertices are moved into the hero's rest pose, the weights are
   mapped to hero bones, and the result is exported and validated.

### Steps 3 to 5

Steps 3 and 4 are covered in the sections below. In step 3, **Apply gun
changes** prepares the model again in one click. In step 5, Build checks the
compiler's paths before compiling and says whether preparing again or a shorter
CSDK folder fixes a path that is too long.

## What the app handles in awkward source files

### Scene objects

The importer keeps the character and sets everything else aside. **Scene
objects** lists every object with a status and a reason; override any choice
there and inspect again.

| Found in the file | What happens |
| --- | --- |
| Cameras, lights, empties, light probes | Ignored. |
| Curves, text, metaballs, surfaces | Converted to meshes when attached to the character rig; otherwise set aside. |
| Grease pencil, hair curves, volumes, point clouds | Set aside (cannot be exported as character geometry). |
| Objects hidden from rendering or in excluded collections | Set aside. Included again if nothing visible remains. |
| Several characters | The most prominent one is kept; choose another **Character rig** to override. |
| Meshes bound to another rig but touching the character | Frozen at rest and attached to the character. |
| Part rigs of a modular character (separate face or hand rig) | Combined into the character rig. |
| Detached props on their own rig, away from the character | Set aside; keep one to attach it rigidly to the nearest bone. |
| Ground planes, backdrops, far-away props | Set aside. |
| Loose props held by the character | Rigidly attached to the nearest bone (finger and toe bones hand them to the hand or foot). |
| Loose accessories hugging the body | Weights copied from the nearest body surface. |
| Bone-parented or constraint-attached objects | Rigidly weighted to that bone at its rest-pose placement. |
| Subdivision, mirror, solidify and other generative modifiers | Applied. |
| Cloth, soft body, particle and other simulation modifiers | Removed; the rest shape is kept. |
| Mirrored (negative scale) objects | Transform baked into the geometry, normals recomputed. |
| Empty material slots | A placeholder material is assigned. |
| Vertices without bone weights | Copy the nearest weighted vertex. |
| Collection instances, linked libraries | Made real and local in the workspace copy. |
| Multiple scenes in a `.blend` | The active scene is used. |
| Missing, generated or sequence textures | Warning; the material keeps a flat color. |

When your model has shape keys, **Shape key handling** is a required choice.

### FBX textures and long paths

An FBX brings only the images it names, found by file name below its folder; an
FBX the app cannot read (ASCII or damaged) brings every image there. Blender
cannot open a path of 260 characters or more: staging stops with a message
naming the file when the model needs it, and skips the file otherwise. The app
itself has no 260-character limit.

### Bone names and body roles

Bone names are tried first (Mixamo, Unreal, Biped, VRM, MMD, DAZ, Character
Creator, Rigify DEF and common variants). When names are missing, ambiguous or
contradictory, the body is recognised from the shape of the skeleton; skirts or
hair that reach the floor are not taken for legs. Left and right come from side
tokens in the names (`.L`, `_l`, `Left`, `lShldr`, `左` and so on) when they
agree, otherwise from the direction the toes point. An MMD rig with every bone
renamed `node0` to `nodeN` is recognised the same as its named version. Your
choices in **Body roles** always win.

### Bone mapping

Every other weighted bone follows its body joint (or the nearest one), so
helper, face or accessory bones never block a conversion. Hair, skirts and tails
detected as moving parts stay independent bones when **Moving parts** is on.
Everything stays editable in **Bone mapping**, and the review confirmation
covers the loss of independent motion.

## Fitting choices

### Model fitting

**Model fitting** offers *Match hero proportions* and *Preserve source
proportions*. Shiv (a long right forearm) and Wraith (short legs) have unusual
proportions, and preparation warns about them. For such heroes, Preserve source
proportions scales the torso evenly so the hips and neck land on the hero's, and
the legs follow the hero's leg joints. With **Align arms to hero joints**, the
elbow may sit up to 8% of the arm's length from the hero's elbow, and the hand
still lands on the hero's hand. Other heroes are unchanged.

### Bend fingers like the hero

**Bend fingers like the hero** (under Customize fit, on by default) bends mapped
finger segments to the hero's resting direction, so they close around the gun. A
segment that would turn more than 100° is left as it is and reported (a finger
mapped to the wrong hero finger).

### Feet on the floor

Both fits line up your character's ankles with the hero's, and the hero's soles
stand at the game's floor. Feet taller than the hero's (heels, thick soles)
therefore reach below the floor. **Feet on the floor** in Fit & review chooses
the correction:

- **Line up the ankles (default):** nothing changes. The receipt notes how far
  the feet reach below or above the floor.
- **Shorten or lengthen the legs:** everything below the hips scales vertically
  about the hips. The hips, body and arms stay on the hero's joints, but your
  model's knees and ankles move off the hero's.
- **Move the whole character up or down:** nothing is stretched, but the body,
  arms and hands sit that much off the hero's joints (and the gun).

Moving parts kept on a scaled leg stay rigid and follow their attachment point.
Within 0.05 in nothing is corrected. A scale outside 0.2 to 3 leaves the fit
alone with a note. Changing this choice needs a new conversion.

## Materials and textures

### How base colors are found

Each material's base color comes from its node graph: the image feeding the
Principled BSDF base color, or, in game rips, an image wired straight into the
Material Output or into a Diffuse, Emission or Toon color. A material with a
single unsuffixed image uses it as the base color. Channel suffixes (`_n`,
`_nrm`, `_orm`, `_mask` and so on) choose the other slots.

A texture connected to the base color is not tinted: the material gets a white
tint, because Blender ignores the color behind a connected socket. Materials
without a base-color image keep their color.

The game textures with the first UV map, so the UV map a part's materials
actually use is moved to the front before export. Baked atlases often live on a
second map such as `BakeUV`.

### MMD materials

MMD materials feed a base texture, a toon ramp and a sphere (shine) map into the
MMD shader. The toon ramp and the sphere map are never used as the color, and
such materials carry a review note that toon effects are not baked.

### Face details

A material the source shows fully transparent is a layer that an expression
fades in: blush, tears or other (sweat, effect quads). A game build has no
expressions, so these layers are hidden by default. **Appearance → Face
details** shows them by kind. A shown layer uses its color image's alpha as its
transparency, so the blush keeps its soft edges; an image without transparency
is drawn as it is. Switching a layer needs no new preparation; finding the
layers does.

### Preparing the same hero again

A material keeps what you changed. A texture or tint that is still the previous
preparation's automatic pick takes the new automatic pick.

### Material names

A prepared material's file name keeps at most 24 readable characters of the
Blender material name plus an 8-digit code from the full name, because Windows
tools fail on paths of 260 characters or more. Your material choices carry over
when a model prepared with longer names is prepared again. Non-English names are
spelled in Latin letters, so `头发` becomes `toufa_…` instead of `__…`. ASCII
names keep exactly the name they had. Bone names are not changed.

## Mesh mapping

Mesh mapping is on the Build page. Every source mesh is a checkbox under each of
the original model's render nodes. Prepared characters name their meshes after
your source (Body, Face), not after the hero's render nodes, so the page offers
a **suggested mapping**: all your meshes on the main body node and its lower
detail levels, the original's other parts hidden, and its gun kept. Nothing is
applied until you choose it. Source meshes that no render node uses are named in
a warning (and by Analyze), because they would be missing in game, with a button
that adds them to the main node.

## Moving parts

### What counts as a moving part

A bone chain becomes a selectable moving part when it has existing physics
bindings, when it has a recognisable name (hair, ponytail, braid, ribbon, skirt,
cape, tail, robe, sash, scarf, shawl, mantle, cloak, tassel, veil, and Japanese
and Chinese equivalents), or when it is a long unnamed chain: at least three
weighted bones in a single line, hanging from the pelvis, spine, chest, neck or
head, spanning a quarter of the torso. Fingers and toes never qualify.

New models keep every detected group. In projects with automatic motion, groups
detected for the first time are added, and groups you were shown and left off
stay off.

### Part types

Build gives each moving part a type. You can change it under **Adjust moving
parts**; an explicit choice always wins.

- **Hair.** Named hair, unnamed strands on the head, and braids (including
  "plait"). Hair has no gravity and no drag; hair longer than 2.5 torso lengths
  gains gravity and drag and hangs like a streamer from 3. Hair buns (one round
  piece on one bone, or named bun, 団子, 丸子, 髻 or odango) are Rigid and follow the
  head.
- **Tail.** Named tails, and unnamed long, round strands on the hips (four or
  more weighted bones, not flat, not joined to other parts).
- **Ear.** Ear groups. The Physics tab's **Ear motion** switch turns them off
  (anything hanging from them stays still too) and back to automatic.
- **Streamer.** Ribbons, tassels, cords and long thin strips. A sleeve named
  sleeve or 袖 on one thin, flat chain of at least 6 weighted joints is a
  streamer. Streamers can bend and swing freely but do not stretch or fold up.
  Streamers with more than 10 weighted joints get less drag.
- **Cloth.** Panels, for example a cloak. Long strips that hang from a cloak or
  cape (at least 0.6 torso lengths, at least twice as long as wide) move like
  streamers while the shared top keeps cloth values.
- **Long skirt.** A panel that reaches 0.45 torso lengths below the pelvis and
  spans 0.6. A shorter panel (reaching 0.4, spanning 0.5) joins the skirt when
  it shares mesh with it, panel by panel. Each skirt joint's travel is capped by
  the gap between its fabric and the body.
- **Charm.** Names with charm, keychain, keyring, pendant, lanyard or dangle
  (and Japanese and Chinese equivalents) move wherever they hang. A charm word
  beats the rigid accessory words, so a dangle earring moves.
- **Rigid.** Halo, hat, cap, crown, tiara, horn, helmet, glasses, mask, earring,
  hairpin, clip, hair band, ornament and accessory (also 飾, 冠, 帽, 角, 眼鏡, 輪)
  never become moving parts from their names, so `HairPin` or `髪飾り` stay on the
  head. Other motion words still win: a ribbon on a hat moves. Rigid parts are
  not simulated; they follow their bone. A model whose every part is Rigid is
  refused with a message to turn movement off.
- **Breast** and **Butt.** See [Breast and butt
  motion](#breast-and-butt-motion).

Strands with more than 6 weighted bones move like a 5 to 6 bone strand.

### Body colliders

With automatic motion, Analyze measures capsules on the character's own thighs,
shins, upper arms, forearms, hips, neck and head, so skirts, coat tails, capes
and tails stop at the body. Shoulders and upper back have none. Colliders act on
the part's joint line, not on the visible panel around it. Strips anchored on an
arm track the arm closely.

### Breast and butt motion

Breast motion and butt motion are separate options, and both are off for new
models.

- **In preparation:** **Keep or add breast bones** decides whether breast bones
  are in the converted model, and **Keep butt bones** does the same for butt
  bones. If the rig has no breast bones, the app adds a left and a right one
  under the chest when the chest clearly has breasts that mirror each other;
  flat, male and child chests and asymmetric clothing get none, and the receipt
  says why. A rig that already has weighted breast-named bones never gets extra
  bones.
- **On the Physics tab:** the **Breast motion** and **Butt motion** switches
  decide whether those parts move, with one amount control for both sides.
  Turning a switch off makes the parts Rigid but leaves them in the model. This
  needs no new conversion. Breast bones from a rig's own physics count even when
  the preparation option is off.
- **Amounts:** the motion is a short bounce, not a dangling strand, and the two
  sides move together so they cannot swing into each other. **Stiffer** and
  **More movement** reduce or increase it.

A breast-named bone that carries a long part (more than half a torso, such as a
cape) moves as that part instead. The Breast motion switch leaves such groups
out and says why; choosing Breast for them under Adjust moving parts still
works.

### Source rig springs

Inspect reads the spring settings a rig already has: VRM spring bones (from a
`.blend` or from a `.vrm`, `.glb` or `.gltf` file; the VRM add-on is not needed)
and MMD rigid bodies. They are converted to the app's scale and used only when
the Physics tab's **Start from your model's own MMD spring settings** (or VRM)
is on. It is off by default.

## Fine-tune and the movement preview

### Fine-tune movement

The Physics tab shows four sliders per part type found in the model (hair,
streamers, tails, ears, cloth, long skirts, accessories, breasts, butt, gun
charms). Each runs from -1 to 1, and 0 is the automatic value.

| Slider | Effect |
| --- | --- |
| Stiffness | Moves the pull back to the pose up toward full or down to a quarter. |
| Damping | Moves damping up toward 0.5 or down to none. |
| Gravity | Moves gravity up toward 1.5 or down to none. |
| Swing range | Scales the travel limit between ×0.25 and ×2.5. A part that swings freely stays free. |

The per-part Movement choice (**Stiffer**, **More movement**) is applied first.
**Reset all** puts every slider back to automatic. Before the first Analyze,
every common part type is listed.

The sliders are keyed by part type, not by moving part, so a saved preset fits
any model. **Saved presets** hold only the tuning (up to 50, names up to 40
characters), and applying one replaces the project's tuning. The TangTang
presets ignore tuning.

### Movement preview

**Physics → Your character → Movement** plays the hero's own idle, run and jump
clips on your prepared character at 1×, 0.75×, 0.5× or 0.25× and simulates the
moving parts with your Fine-tune values. It approximates the game's cloth
solver, for tuning by eye.

## Guns

Deadlock heroes carry their gun in one of three ways:

- **Part of the hero model:** render meshes skinned to weapon bones. The app
  finds them in the hero model together with their lower detail levels.
- **A gun in each hand** (Haze): every gun mesh is weighted to one hand's bones,
  and each hand's gun is measured on its own.
- **A separate weapon model:** the hero model has no gun mesh; the gun is its
  own `.vmdl`.

### The Weapon tab

The Weapon tab summarises the hero's gun, your gun and what will happen, then
offers one choice:

| Choice | Result |
| --- | --- |
| Use my gun(s) instead of the hero's (recommended, preselected) | Your gun is fitted onto the original and replaces its meshes (all detail levels) in the mesh mapping. |
| Keep the hero's gun(s) | Your gun is left out of the character. |
| More options → Keep my gun in the hand as an accessory | It follows the hand like any accessory, and the hero's gun stays too. |

For a dual-wield hero, **Right hand** and **Left hand** pick your gun for each.
The left hand can also use *A copy of the right-hand gun* or keep the hero's own
left gun. **Swap hands** exchanges the two with their adjustments.

### Fit preview and adjustments

A 3D preview updates by itself after every change: your right gun in amber, your
left gun teal, the hero's guns see-through, and arrows along the line of fire.
View buttons (Angled, Front, Side, Top) and Show buttons (the hand, **Both
guns**, **Whole body**) frame it.

**Pose: Holding / Rest** (Holding by default) shows the hero as it holds the gun
in game. The fit itself is measured in the rest pose.

The adjust buttons (**Flip direction**, **Turn ⟲ 90°** and **Turn ⟳ 90°**,
**Smaller**/**Bigger**, Back/Forward/Up/Down/Left/Right, **Reset**) change the
selected hand. **Both guns** moves a dual-wield hero's two guns together. Roll
turns the gun around its barrel; pitch tilts the muzzle up and yaw turns it to
the gun's right, both around the grip. **Advanced gun settings** holds size
mode, skinning, the weapon bone, exact numbers per hand, the parts list and
**Join each gun's parts into one mesh**.

### How the fit works

Both guns are measured the same way: barrel axis, muzzle end, up direction, grip
and length. Part names count as whole words: Muzzle, Barrel, Nose, Front,
Emitter, Suppressor, Silencer and Compensator for the front; Rear, Back, Stock
and Butt for the rear; Grip, Handle and Handgrip for the grip. The fit line
shows how the front was found. **Flip direction** moves the muzzle to the other
end, and the app points out a flip that goes against the part names.

Your gun is rotated to match the original, scaled to its length (or kept at the
character's scale) and moved so the grips coincide. It takes the original gun's
skin weights, so it follows the same weapon bones (recoil, slide, magazine).
Muzzle attachments and hit detection stay on the original bones. Heroes without
gun meshes but with weapon or muzzle bones use those bones for the fit with
lower confidence; check the roll in the preview.

**Join each gun's parts into one mesh** is on by default; parts with shape keys
are left as they are. Gun parts that use identical materials share one game
material, so a gun baked to one atlas becomes one material instead of one per
part color.

### Gun from another file

The Weapon tab of a character project, and step 1, offer an optional **Gun from
another file** (Blender, FBX, GLB or glTF). Choosing, changing or clearing it
runs the inspection again by itself.

- Hidden objects, the gun's own rig, ground planes, backdrops and parts away
  from the gun are set aside, and each can be kept in **Scene objects**. Flat
  gun parts such as grip panels, rims or decals are never taken for the floor.
- The gun's grip is moved to the middle of the right hand unless the gun already
  sits there, and the gun is weighted rigidly to the hand. A rig without a right
  hand attaches it to the nearest weighted bone; an unrigged character keeps it
  where it is.
- **A pair of guns** in one file gives a right and a left gun. Parts are sorted
  by their hand from the object name (tags `[L]`/`[R]`, `(L)`, `.L`, `_R`, then
  the words left/right, l/r, 左/右) or from the gun rig bone they are weighted to.
  Mirrored halves of one gun stay one gun. A real pair posed overlapping in the
  file is taken for one gun; set one side aside in **Scene objects** to split
  it.
- Guns from the gun file are suggested before any gun in the character file. A
  gun-file gun that no hand uses is left out. A gun in the character file stays
  on the character unless you set it aside in **Scene objects**.
- Charm chains on the gun file's rig keep their bones and hang from the hand
  holding that gun, so they swing like charms on the character's own gun.
- A changed, cleared or newly chosen gun file needs a new inspection. A moved
  but identical file is accepted.

### Separate weapon models

When the hero's gun is a separate model, the Weapon tab's **Use my gun instead
of the hero's** creates a gun project next to your project, prepares the gun and
links it, so one VPK holds the character and the gun. The same preview and
adjust buttons work on the gun project, and **Apply changes** makes the gun
model again. **Appearance → Equipment → Weapon model** (*Find this hero's weapon
models*, *Create weapon project*) remains for manual setups.

A weapon project's source may be a gun-only file. When no object is recognisable
as a gun (one unnamed, unrigged object such as `Cube`), the whole scene except
ground planes and backdrops is offered as the gun. Without a rig, parts touching
each other in a chain (barrel, suppressor) stay together.

### Charms on the gun

A chain hanging from a gun bone counts as a gun charm when it has two to four
bones in one line, its tip hangs below its top by at least 40% of its length,
and no bone in it is named as a gun part (slide, magazine, trigger, barrel and
so on). A strap hanging from a gun bone counts in any shape. A charm made of one
weighted bone has nothing to swing around and stays rigid. A chain hanging
straight from the hand bone is not a gun charm unless it is named as a charm.

| Gun choice | Charm |
| --- | --- |
| Replace the hero's gun | The charm's bones are kept, hang from the hero bone the fitted gun follows, and are placed like the gun. |
| Keep it in the hand | The charm hangs from the hand like any accessory. |
| Leave it out | The charm is left out with the gun, and offered again when the gun comes back. |

A charm's pivot stays on the gun and the rest swings with no gravity, so it
never sags into the gun when the gun is held sideways. The Physics tab's **Gun
charm motion** switch turns the swing on or off (off keeps the charm still on
the gun) and sets one amount for all charms, without converting again. A gun
that copies the right-hand gun keeps the copied charm rigid.

Separate weapon models cannot carry charm motion. A gun from a separate gun file
keeps its charm bones when it replaces a gun that is part of the hero model.

## Hero colors

**Extras → Colors** is not part of preparation. It recolors the hero's **own**
compiled materials for a project without a replacement model. Materials that
other models also use are recolored only when you allow it.

The build changes only the tint, saturation and brightness values the material
already has, and only on `pbr.vfx` materials. Roughness, metalness, opacity and
emission are not offered, and a hue shift needs a color range. A material whose
tint mask is not a default texture tints only its masked areas; the page says
so.

### Color ranges

A hero color can have up to 8 color ranges. Each selects base-color pixels by
hue, saturation and brightness, faded by the softness, then shifts the hue and
scales saturation and brightness. Every range selects by the original color, and
several ranges add up in order. Hue alone is not enough for every material:
Haze's orange clothes and her skin share a hue but differ in saturation.

The build recolors a copy of the game's base-color texture and packages it with
the mod; the alpha channel, where Deadlock keeps metalness, stays exact. Only
BC7 textures are supported. The alternate reveal texture is not recolored.

## Limits

- Automatic results still need an in-game check: grip, aim and muzzle position,
  elbow and knee bending, and clipping (for dual-wield heroes, also the two-gun
  animations).
- Non-humanoid bodies (quadrupeds, characters without two legs and two arms),
  models whose limbs are not separated in the rest pose, and live rig controls
  that change the rest shape still need a manual source copy.
- A separately modelled gun replaces the weapon model for every hero that shares
  that model.
- Gun charms move only on guns that are part of the hero model.
- Hero colors work only without a replacement model, only on `pbr.vfx`
  materials, and color ranges only on BC7 textures.
