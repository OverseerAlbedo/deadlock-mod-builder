# Troubleshooting and compatibility

## Icons shown as view-only or too dark

Install Alpha 47 or newer, then use **Extras → Reload assets**. Earlier builds
rejected ordinary compiled images when they carried fallback artwork, Panorama
processing flags, padded display dimensions, or custom mip counts. A separate
resizing bug could also darken previews and fitted replacement images. These
were application limitations, not a broken Windows or compiler installation.

Drop a PNG or JPG onto the selected icon and analyze/build again. Existing
prepared models can be reused. The expanded path handles ordinary 2D color
icons across heroes without a texture compiler; it preserves visible dimensions,
GPU padding and mip count, with lossless RGBA storage for GPU textures. This can
increase texture size. Embedded PNG textures keep PNG storage.

An atlas, cubemap, array, volume, virtual texture, unknown format or malformed
file still requires different handling. **File details** explains a remaining
restriction. Sound and particle replacements continue to use their existing
format and compatibility checks; this does not make arbitrary files interchangeable.

## Setup

Save Settings before checking them. Select `blender.exe`, the original game archive, the material compiler, and the separate model toolset where applicable. Use the app's model-toolset guide for acquisition; do not mix unrelated compiler DLLs.

The Blender bridge accepts 4.2–4.5, 5.0 and 5.2 LTS. Newer source files may still depend on features or add-ons absent from the installed version. An import error should be diagnosed from the final Python exception, not only Blender's configured exit code 23.

## Models and mapping

Automatic roles and weighted-bone suggestions need review on unfamiliar rigs. Rigs with generic or unusual bone names are recognised from their skeleton structure; sources exported lying down are rotated upright. Extra characters, cameras, lights, hidden objects, scenery, missing materials and partly weighted vertices are handled automatically - open **Source inspection → Scene objects** to see what was kept or set aside and to override it, then click **Inspect again with these choices**. Unsupported live controls that change the rest shape may still need a baked/exported character. A successfully prepared preview does not prove native compilation or in-game compatibility. See [model preparation](MODEL_PREPARATION.md) for the full list.

Separate body and face/part rigs can be imported when there is one clear character and the part has a verified attachment. The importer preserves offset rest bones instead of forcing differently placed bones together, and reports any clearly detached props excluded from character preparation. Rig names and bone counts do not decide which object is the character. Import and inspect the original file again after updating from an earlier rejected import.

If attachment, relative scale, live controls or independent part physics still prevent merging, use a source copy with one shared character armature. Do not delete face/outfit meshes or apply arbitrary joins just to bypass the error. Keep external texture folders next to an unpacked `.blend`; the importer cannot recover image files absent from the download.

### Guns

If the source contains a gun, the **Weapon** tab offers to fit it onto the hero's gun, leave it out, or keep it in the hand. *Replace* is only available when the hero's gun is part of the hero model. When the hero's gun is a separate model, use **Appearance → Equipment → Weapon model** to create a weapon project from the same file. Use **Preview gun fit**; if the gun points backwards or is upside down, use *Flip barrel direction* or *Roll*. Shots and muzzle flashes come from the original muzzle bones, so check aim and grip in game.

Choose the main hero model unless an alternate variant is intended. A valid VPK targeting an unused model path can compile without visibly replacing the character being tested. The character picker reads the installed game's hero definitions: the model the game loads is listed under the hero's name, and roster models it does not load are marked *Not used in game* (for example Rem loads `familiar_wip.vmdl`, not `familiar.vmdl`). Analyze warns when a saved project still targets such a model; choose the game's model, then prepare and build again. Check the installation slot and disable conflicting mods when diagnosing this.

## Physics

Automatic mode operates on preserved secondary groups and available mesh evidence. Narrow straps, long streamers, broad skirt panels and upper attachments have different treatment. An explicit override can intentionally prevent automatic classification. Re-analyze a changed project and rebuild to apply a new plan.

Halos, hats, crowns, hairpins and other rigid accessories stay on the head instead of receiving hair motion. Breast motion is switched on and off on the **Physics** tab (Automatic motion → **Breast motion**), together with its amount (**More movement** for a bigger bounce, **Stiffer** for less). When the prepared model has breast bones, that applies at the next Analyze and Build without converting again. When it has none, the switch says so: in Model preparation, **Keep or add breast bones** must be on; click **Inspect again to add breast bones** (they are added where the chest has breasts, or the reason is shown), convert, and click **Use prepared model**.

Vindicta, Daki, TangTang and Graves have accepted user-tested configurations. This is not certification of every combination of source character, target hero, motion, and game version. Native test fixtures, standalone previews and compiler success do not replace gameplay verification.

## Disk space

Projects are built in the workspace folder (by default `%LOCALAPPDATA%DeadlockModBuilderworkspaces`), and each project also has working copies in the CSDK and model toolset folders. **Settings → Storage** shows how much each project uses and clears data the builder can recreate: old model preparations, older backups, build caches, compiler working folders and leftovers of interrupted operations. Data a saved or open project still uses, built VPKs and workspaces with a build lock are kept. **Remove…** deletes all stored data of one project (its saved project file stays; installed mods are not touched). Retention choices are saved with the other settings.

## Logs

Use **Logs** to save complete application/tool output and **Step timings** to see the failing stage. Include the app version, selected hero/model path, source format, exact error, reproduction steps, and relevant logs with a bug report. Remove private paths/account information before posting publicly. Do not upload a complete game dump to a bug report.

## Portable build

Extract all files before launching. Keep `native/cloth-reader`, `physics-references`, and `licenses` beside the EXE. Install WebView2 Runtime if absent. The ZIP version still saves settings and workspaces in the Windows user profile.

For a CI failure, download `DeadlockModBuilder-Windows-validation` from that run and inspect the first failed command. The workflow tests/builds only; it has no release-publishing permission.

## The camera is sideways or too low with a model mod (game update of October 16, 2026)

The October 16, 2026 game update changed how attachments parented to the `root_motion` bone are stored in compiled models: the bone's rotation is no longer baked into them. The game's own hero models were recompiled; a mod compiled before that date still carries the old values, so its camera pivots (`standing_pivot`, `crouching_pivot`) and a few other root attachments are rotated by 90 degrees in a match. Every hero-model VPK built before the update is affected, whichever tool made it; VPKs that only replace materials, sounds or icons are not.

Two ways to fix it:

- **Build & install → Installed mods → Check installed mods** lists every VPK in the game's addons folder with its state. **Update** rewrites the affected hero models in place (the attachments take the game's current values; custom attachments on the root bone are converted by the same rule) and keeps the previous file as a backup. This works for any single-file VPK, including mods made elsewhere.
- **Rebuild** the project: a build always decompiles the current game model and, since this version, takes the attachments from the game's compiled model regardless of the decompiler.
