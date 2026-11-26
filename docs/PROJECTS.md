# Projects and installed mods

A project is one character on one hero, saved as a `.dmb.json` file. This page
covers working with projects (Undo and Redo, file windows, copying a project
to another hero, two preferences in Settings) and the list of installed mods on
Build & install. The five steps themselves are in the
[model preparation guide](MODEL_PREPARATION.md).

## Undo and Redo

Undo and Redo take back the changes you make to your project: choices on every
page, the Extras and the Motion page. The buttons sit in the top bar next to the
project name; the keys are **Ctrl+Z** for Undo and **Ctrl+Y** (or
**Ctrl+Shift+Z**) for Redo. In a text field these keys edit the text as usual.
A short message names what was undone or redone.

- A slider drag is one step, and quick changes to the same setting within about
  a second are one step. Up to 100 steps are kept.
- Undo stops at the last preparation, Analyze or build, and at opening or
  starting a project: those write files the project points to, so the history
  starts again from there. Saving does not clear it.
- Undo and Redo are not available while an operation runs.
- Settings are not part of a project, so they are not undone.

## File windows

Every open and save window starts in the folder you last used for the same kind
of file: projects where you keep projects (opening and saving share it), models
where your models are, sounds where your sounds are, and so on. Saving a project
that already has a file suggests that file. The folders are remembered on this
PC only.

## Make a copy for another hero

To put the same character on a second hero, open the project, go to
**Character & model** and click **Make a copy for another hero…** under the
hero picker, then choose the hero. The copy is a new project; the original
project and its files are not changed.

The copy opens at Prepare model, is not saved yet (save it when you are ready),
gets a free VPK slot and starts with a fresh Undo history. A notice above the
page, and an entry in the sidebar's Notifications, lists what happened:

| | What |
| --- | --- |
| Carried over | Your model and gun files, body roles on your model's bones, moving-part groups, fit and shape key choices, scene object choices, material images and settings, color and surface changes of your model's materials, the texture folder, face expression and face details, physics fine-tuning and preset, item icons and renamed items, a clothing file, and output and detail-level settings |
| Not carried, because they belong to the first hero | Sounds, effect files and effect edits, ability objects, hero icons, animation clips, hero colors, equipment choices on the hero's meshes and bones, the tested cloth setup and the gun fit |
| Made again for the new hero | Joint matches, the mesh mapping, the prepared model and the VPK slot |

On Prepare model, prepare the model for the new hero: the body roles and
material choices come with it, and the joint matches are made again for the new
skeleton. A project that started from a ready-made DMX is not
carried that way, because the DMX was made for the first hero's skeleton: choose
your model file again in the copy.

## Appearance preferences in Settings

**Settings → Appearance** holds three preferences besides the theme. Each is
saved at once.

- **Language (语言):** English or Simplified Chinese (简体中文). On a first
  start the app follows the Windows display language, and the new-user guide
  asks on its first card. Switching takes effect at once, without a restart,
  and keeps unsaved work. With the Chinese interface, heroes and items are
  named as the game's own Chinese text names them. Error messages with their
  help steps, the notes under a check or a result, the log, and names made
  from the game's file names (sounds, effects, animations, ability objects)
  stay in English. Projects and settings are the same in either language.
- **Default fit for new models:** the Model fitting a model starts on when it is
  checked for the first time in a project, *Preserve source proportions* (the
  default) or *Match hero proportions*. It applies to rigged models and to
  models without a skeleton. You can still change the fit on Prepare model →
  Fit & review, and projects checked before keep their own fit. See
  [Model fitting](MODEL_PREPARATION.md#model-fitting).
- **Load 3D previews automatically** (off by default): Prepare model,
  Appearance & weapons and Physics & preview load their 3D view by themselves
  when the page is shown, once per model and only when nothing else is running.
  It never starts an Analyze. Previews of Blender, FBX and MMD files start
  Blender, which takes a while on slower PCs. Off, each view loads with its
  button.

## Installed mods

**Build & install → Installed mods** lists every VPK in the game's `addons`
folder and says what each one is. Nothing is read until you click **Check
installed mods** (**Scan again** later); **Open the addons folder** opens it in
Explorer. The scan never writes to the game folder.

What each file is comes from the file itself: the heroes it belongs to and the
kind of change are read from the paths inside it (character model, gun,
materials, sounds, icons, effects, ability objects, item icons, item names,
other files),
so the list stays right when a mod loader renames or renumbers files. Item
icons count for every hero. A file that cannot be read as a VPK is listed as
such.

- **Filter and search:** search by hero, file or label; the chips choose a
  hero, **Items** or **Other**; **Made by** chooses **Made here** (installed by
  this app) or **From elsewhere**.
- **Made here:** a file this app built and installed shows its project.
  **Details** offers **Open its project** (the same as opening it with Open…;
  shown when the project file is on this PC), **Show its project file** and
  **Show in folder**. A file installed by an
  older version of the app, before it kept a record of installs, reads as not
  made by this app.
- **Same game files:** when two VPKs change the same game files with different
  contents, both are flagged and Details lists the files. Which version the
  game uses is not decided here.
- **Labels:** **Add a label** gives a file a name of your own (for example
  "main skin"). The label is kept by the app, not in the file, and follows the
  file's contents, so it stays when the file is renamed and moves with an
  Update.
- **Update:** hero models compiled before the game's October 16, 2026 update
  show the camera 90 degrees off in a match. Their state says they need an
  update, and **Update** (or **Update N mods**) rewrites them in place,
  whichever tool made them, keeping the previous files as backups. See
  [the camera section of Troubleshooting](TROUBLESHOOTING.md#the-camera-is-sideways-or-too-low-with-a-model-mod-game-update-of-october-16-2026).
