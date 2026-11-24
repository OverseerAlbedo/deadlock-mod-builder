# Deadlock Mod Builder 0.14.0

A big Extras release: **recolor effects**, **replace ability objects**, **rename items** and give them new icons, **edit and silence sounds**, and a rebuilt **Colors** page. The **Motion** page can make a clip from your own animation or take a move from another hero, and Prepare model, Appearance & weapons and Physics & preview have new layouts, with a movement preview that simulates the game's own cloth. Your project's choices can be undone and redone. From this version the app is licensed under the GNU AGPL v3.0 or later. See `CHANGELOG.md` for everything.

## New

### Motion

- **Motion, rebuilt:** the hero's animations by group with readable names and a search, played on your prepared character, with a check that your replacement fits before you build.
- **Make it from my animation:** animate your character in Blender (or save an FBX or glTF from another program) and the app turns the animation into the hero's clip, with the game's length, events and root motion kept.
- **Another hero’s animation:** take a move from another hero in the game. Choose the hero and the animation (the same move is suggested first), the **Timing** and whether the **Arms and hands** come from the other hero or stay this hero's own. A move made of several animations, such as a dash, is taken in one click with **Take all**.

### Extras

- **Effects:** recolor a hero's ability and weapon effects with one color pick, or shift exact values per part under Advanced. Change Size, Brightness and Opacity, hide single parts, and give 3D parts and lights their own settings. The selected effect plays as you edit it, beside the game's version if you like.
- **Objects:** replace the objects an ability throws or shows (Haze's Sleep Dagger, Holliday's Powder Keg, McGinnis's turret and wall, Graves's gravestone and many more) with a model of your own. The game's collision, animations, cloth and glow stay as the game has them. Needs Blender and Reduced CSDK 13.
- **Items:** give any shop item a new icon, or a new name: type it in the name field at the top of the side panel and click **Save**. The shop's card, tooltip and search follow your name.
- **Colors, rebuilt:** your character in 3D beside the materials. Click a color on the character and choose its new color; grey, white and black parts can be recolored too, and **This part only** limits a change to the piece you clicked instead of **Everywhere this color**. Colors also edits your replacement model's materials, and a Weapon filter shows the weapon's materials.
- **Sounds:** tags (Hero, Ability, Weapon, Voice) and readable names, an editor that picks the part of your file that plays (songs up to 20 minutes) with volume, fades and loudness matching, and **Silence this sound** for game sounds you want gone.

### Other pages

- **New layouts** for Prepare model, Appearance & weapons and Physics & preview, each with its 3D view beside the tabs.
- **Physics & preview:** the Movement view simulates the game's own cloth model, so hair, cloth and tails move much as they do in game, and it plays running in every direction, a stop and a long fall. The preview shows your model's texture colors.
- **Model fitting → Leg joints**, for knees that stretch when the character jumps or crouches.
- **Build & install → Installed mods** says what every VPK in the addons folder is and which heroes it changes, with labels of your own.
- **Undo and Redo** (Ctrl+Z / Ctrl+Y) for your project's choices.
- **Make a copy for another hero** on Character & model puts the same character on a second hero as a new project.
- **MikuMikuDance models (.pmx, .pmd)** can be chosen directly, with the free MMD Tools add-on for Blender.
- **Load 3D previews automatically** in Settings → Appearance.

## Changed

- **Preserve source proportions** is the default fit for a model checked for the first time in a project. Settings → Appearance → Default fit for new models can switch back to Match hero proportions.
- Automatic movement: breasts have a slow, soft bounce; hair no longer floats up in long falls and its roots stay on the head; a tail sticking straight out hangs under gravity.
- Better rig detection: bone names with a studio prefix, numbered spines and more breast and butt bone names.
- Textures with very long names, or saved under another file type than the model asks for, are found and used.
- Mesh mapping's suggested mapping puts every unused mesh on the body in one click and keeps the hero's weapon parts and the things a hero carries.
- Build & install keeps the list of things to review from the last Analyze, so you can build once it is done without analyzing again.

## Fixed

- A hero part named like one of the hero's bones (Rem's `head`, for example) could not be switched off or given several meshes in Mesh mapping: the build stopped with "Cannot safely expand unknown parent_bone reference to head." or "Disabling head leaves an unsupported parent_bone reference" (issue #10).
- An effect that draws a hero's model (such as Haze's Bullet Dance figures) stopped the build with "Character component dependency is missing".
- Physics & preview: Movement no longer answers "Another operation is running" while the character is still loading, and the jump lands properly instead of twisting the body over at the landing.
- Some of the game's icons and item pictures were previewed dark and in wrong colors.
- Your own gun is drawn once on heroes whose model shows several gun parts at the same time.

## License

From 0.14.0 Deadlock Mod Builder is licensed under the GNU Affero General Public License v3.0 or later (see `LICENSE` and `NOTICE`). Versions up to and including 0.13.0 stay under the MIT License. Notices for every bundled component ship with the app.

## After updating the app

- Projects, settings and themes are kept.
- To see the new movement preview, run **Analyze project** again on projects analyzed with an earlier version.
- **Make it from my animation** needs **Prepare model** once more on projects prepared with an earlier version.
- Mods you already installed keep their old movement until you build and install them again.
- If you had changed the Breasts, Hair or Ears sliders in Fine-tune, set them back to Automatic first: the automatic movement of those parts changed, and saved slider values apply on top of it.
- To pick up textures saved under another file type than the model asks for, choose the model file again in an existing project.
- A project that uses the new Colors options (recoloring grey, white or black, or This part only) cannot be opened by 0.13.0 or older.
