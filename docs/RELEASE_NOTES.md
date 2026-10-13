# Deadlock Mod Builder 0.13.0-beta.1

The first beta. The app now leads through five steps, prepares a model automatically, handles guns from separate files (including a gun in each hand and swinging charms), and is faster and far smaller on disk. This release was tested with Yvonne on Haze using the Artzy Tyrannical dual gun file; see *Checked in game* at the end.

## Five steps

- The sidebar leads through **1 Character & model → 2 Prepare model → 3 Appearance & weapons → 4 Physics & preview → 5 Build & install**, with a check mark on finished steps.
- A Blender, FBX or glTF model chosen in step 1 (or dropped on the window) is checked, matched and prepared automatically; the app stops only where a choice is needed and offers a button to that spot.
- **Prepare model** does the check, the conversion and *use this model* in one click.
- The new-user setup and the model compiler setup use the same step layout, with plain explanations of what each stage does.

## Guns

- **Gun from a separate file** (step 1, optional): the file's gun is put in the hero's hand and fitted to the hero's own gun. A file with two guns ([L]/[R], .L/.R or left/right in the names) gives one per hand on dual-wield heroes such as Haze; the left hand can also use a copy of the right gun or keep the hero's.
- **Live 3D fit preview** with solid shapes: your guns in amber and teal over the hero's see-through guns and body, arrows along the line of fire, View and Show buttons, and simple Flip / Turn / Bigger / Smaller / move buttons. The gun's front is read from its part names (Muzzle, Nose, Emitter, Rear, Grip …).
- **Gun charms** (keychains, pendants, straps) keep their bones and swing in game; **Physics → Gun charm motion** sets the amount.
- A gun's parts are joined into one mesh per gun, and parts that share one texture set become one material (a 15-material baked gun becomes one), so builds are smaller and faster. Textures on a second UV map (baked atlases) line up in game, and textured parts are no longer darkened by a hidden flat colour.
- Heroes whose gun is a separate model get that model built from your gun in the same tab, into the same mod.
- **Fingers bend like the hero's** at rest, so the hero's grip closes them around the gun (on by default in Conversion review).

## Materials and build

- Prepared material names are shortened so compiled texture paths stay under the Windows limit; Build checks path lengths before compiling and says what to do if one is too long.
- A free VPK slot is detected from the game's addons folder at every check, and the chosen slot's state is shown on the Build page. The project's own earlier output no longer counts as another mod.

## Faster and smaller

- Preparation is about 40% faster (Yvonne with two guns: 26 s instead of 44 s), and a preparation uses about 100 MB instead of 900 MB. Opening, accepting, analyzing and building no longer read a 470 MB file.
- Builds copy and verify files with fewer reads and rehash only what the compiler changed.

## Also

- Fixes for Haze's guns being reported as a separate model, particle models offered as weapon models, flat gun parts set aside as backdrops, n-gon faces failing to convert, and a false "Your gun file changed" warning.
- The full list is in `CHANGELOG.md`.

## Checked in game

- Yvonne on Haze with the Artzy Tyrannical dual guns: both guns in hand, textures correct, mod installs and loads.
- Not yet checked in this build: the finger bend around the gun, the charm swing amount and the dual-gun aim during animations. Please report what you see.

## After updating

- Open your project and run **Prepare model → Prepare automatically** once: it applies the finger bend, the shorter material names and the smaller preparation format. Material choices are kept.
- Older preparation folders (about 900 MB each) stay until **Settings → Storage** clears them.
