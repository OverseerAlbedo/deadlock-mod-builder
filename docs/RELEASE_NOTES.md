# Deadlock Mod Builder 0.13.0-beta.13

**Nurse Harrow**, clearer automatic-movement errors (issue #5), and two preparation fixes. Everything from the earlier betas is still in; see `CHANGELOG.md`.

## New

- **Nurse Harrow** was released in the game on October 20 and is tagged In game. Checked on the PC with a character prepared and built on her in every physics mode: Preserve original physics, automatic movement, and Preserve with the rig's physics bones kept.

## Fixed

- **Automatic secondary motion** stopped Analyze with "The selected replacement meshes do not contain a preserved secondary rig" for several different reasons, and its help never appeared. The error now says which step is missing: Keep existing physics bones is off, the model in use was converted without its moving-part bones (convert again and click Use prepared model), or the moving parts are in meshes left out of the build. In the last case Analyze now finishes and names those meshes, so you can add them in Build & install → Mesh mapping (it was not shown while Analyze failed).
- The Physics page says when your model's rig has no moving parts at all: automatic movement moves the hair, cloth and tail bones a model already has, and clothes weighted only to body bones cannot move. **Preserve original physics** now says that it keeps the hero's own hair and cloth; your model's own hair and clothes stay still in that mode.
- **Match hero proportions** (the default Model fitting) stopped after Blender with "Prepared geometry does not match the selected Model fitting option" for most rigs. Fixed; Preserve source proportions was not affected.
- **Automatic movement** could stop with "Bind transform outside original-physics transfer budget" on a moving part that points almost exactly along its parent bone's axis (found with a character on Nurse Harrow). Model preparation now lines such bones up exactly, by a fraction of a degree, so they compile without error. Convert and click Use prepared model once for characters prepared earlier.
- After a game update changes a hero's model, Analyze stops with "Skeleton reference belongs to different original model". It now comes with help: Convert and click Use prepared model again. Today's update changed Silver, Violet and Haze, among others.

## Checked

- Checked on the PC on October 20, 2026: the issue #5 cases on a copy of a real project, Nurse Harrow builds in every physics mode, and both Model fitting modes.
- Not yet checked in a match: Nurse Harrow mods, automatic movement on her, and Match hero proportions.

## After updating the app

- Projects prepared before today's game update need Convert and Use prepared model once (the error says so). Projects, settings and themes are kept.
