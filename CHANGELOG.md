# Changelog

## 0.14.0 — 2026-11-24

- **License:** from this version the app is licensed under the GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later); see LICENSE and NOTICE. Versions up to and including 0.13.0 stay under the MIT License. The app now ships notices for every bundled Rust crate and web package.

### New

#### Motion

- **Motion, rebuilt.** The page lists the hero's animations by group with readable names and a search, plays the selected clip on your prepared character (the game's clip, yours, or both together), and shows whether a replacement fits the game's clip before you build. Clips that no animation graph of the hero names are folded at the end. Animation graph handling moved into a menu, and **Switch all replacements off** turns every clip replacement off at once.
- **Three ways to replace a clip**, chosen with the buttons under **Replace this clip**: **A compiled clip file**, **Make it from my animation** or **Another hero’s animation**.
- **Make it from my animation.** Animate your character in Blender (or in another program, saved as FBX or glTF), in the file you prepared or a copy of it, then choose the file and the animation: the app turns it into the hero's clip so that it looks in game as it does in your animation. You choose the part that plays, whether it stays in place, a loop blend, and whether the whole body or only the bones you animated follow; the page shows the result on your character and lists what the clip changes. The clip keeps the game's length, events and root motion, and when your animation moves the body's position the whole body is raised or lowered once so the feet reach the floor as in the game's clip. Takes .blend, .fbx, .glb and .gltf files; each take or animation in a file can be made into a clip. Projects prepared with an earlier version need Prepare model once more first. A project on a ready-made DMX or an unrigged model is told so instead of being asked to prepare again.
- **Another hero’s animation.** Take the move from another hero already in the game: pick the hero and the animation (the same move is suggested first), then choose **Timing** (**Fit it to this animation’s length** or **Play a part at its own speed**) and **Arms and hands** (`From <other hero>`, whose arms pose as if holding that hero's own weapon, or `Keep <this hero>’s own`). The hero's own length, events and travel stay the game's, so the result always fits, and the page lists what you get. A move made of several animations (a dash is five, a slide three) can be taken in one click with **Take all**. Heroes whose body cannot be carried over are greyed out with the reason. Needs neither Blender nor a prepared model.

#### Extras

- **Extras → Effects: recolor the hero's ability and weapon effects.** Pick a color and the whole effect follows, or switch to Advanced to shift exact values, leave parts out and change single colors. Size, Brightness and Opacity change every flat or glowing part of an effect. Each effect shows its parts with the game's colors and yours, and says which parts keep their own colors (3D models, ground decals) and where else the game uses the effect. The selected effect plays above its parts as you edit it, showing exactly what your mod will contain (also parts that another edited effect or your own replacement file decides, and also in a project that has nothing to build yet), with an optional side-by-side comparison with the game's effect, background and view-brightness choices, and a frame-rate choice the app remembers; on computers without OpenGL 4.6 the color strips are shown alone. Replacing an effect file with your own is still there, under Replace a file.
- **Extras → Effects, Advanced: controls per part.** Hide any part of an effect with its Shown / Hidden switch, and give a 3D part its own Size and Opacity and, when the game's file gives it no color, a Tint. A light part has its own Light brightness (0 to 50 times the game's value), which changes how strongly it shines on its surroundings; lights the game drives itself say so. Each part says what it draws (line, glow sprites, trail, 3D model, light), and 3D models and lights say how they follow your colors.
- Extras → Effects lists the hero's abilities in slot order, then the weapon, melee and the hero's own effects, and says what the list is. Effects sit under the right ability or under the hero for every hero, using low-violence versions, summoned objects, the hero's other models, an ability's internal or display name in a file name, "ult" files under the fourth ability, and the hero's animation, head and showcase effects; an effect placed only by its file name says so. Effects that no ability of the hero uses are grouped as "Used elsewhere in the game" or folded under "Other files in the hero's folders"; search and the Changed filter still find them, and edited effects always show. An ability whose effects are not files of the hero's folders has its own group with a note saying so.
- **Extras → Objects:** replace an object a hero's ability throws or shows (for example Haze's Sleep Dagger or Kelvin's Frost Grenade) with a .blend, .fbx, .glb or .gltf model of your own. The page lists the hero's objects under their abilities, fits your model to the game's object by size and direction, and shows both in a 3D view that keeps your object in view beside the game's. Your model's own colors and images show in game unless the effect draws the object in a material of its own, which the page tells you. This covers objects an effect draws and objects the game places itself (such as Holliday's Powder Keg): their collision, hit areas, attachment points and glowing outline stay the game's, and the view draws the collision so you can shape your model to it. Objects with versions an effect switches between and objects the game animates (such as McGinnis's healing canister) can be replaced too: an animated object follows the game's moving parts in one piece, by parts, or as a model you rigged on the object's own skeleton, and the view plays the game's animations on your model. Objects with custom-shaped collision (such as Viscous's Cube) and objects the game animates in its older way (such as Venator's Snap Trap) can be replaced as well: the app keeps the game's own collision and animations for them unchanged. Where a model holds a second part that shows beside the object, your model replaces the object and that part stays the game's own (McGinnis's Mini Turret keeps its range markers). An object that breaks apart when destroyed (such as Solomon's bishop) still breaks into the game's own pieces. Objects the game animates on a single bone (such as Holliday's Bounce Pad) can be replaced too. An object with several states (Graves's gravestone, whole and cracked) shows your model in each state, and an object with a glow around it (Pocket's briefcase) gets that glow made from your model in the game's own glow material. McGinnis's Spectral Wall is two models, listed separately: the wall while it travels and the wall as it stands. Where the game's effect draws an object in one of the game's own looks (the wall while it is being placed, for example), the page says that only your shape shows there. An object whose shape the game changes while its animation plays (Kelvin's ice path, Viscous's Puddle Punch) shows your model in its own shape, an object made of several pieces shown together (Doorman's door) has its first piece replaced while the others stay the game's own, and an object with cloth of its own (Rem's helper, Calico's cat form, Sinclair's assistant, the pigeon Baba turns into) keeps the game's cloth, so the parts of your model that follow it still swing. The few objects that a rebuild cannot reproduce exactly are listed with the reason they cannot be replaced yet, and so is a model the game never draws (one an ability only uses to block players, shots or sight, where what you see is an effect). For a model that is also part of the game's scenery, the page says your object may show in the map as well. A file that cannot be read, or that holds no mesh, gets help that says what to change. Needs Blender and Reduced CSDK 13.
- **Extras → Items:** give any item of the shop a new icon. Items are grouped like the in-game shop (Weapon, Vitality, Spirit, then Tier I–IV with their prices in souls), with Legendary for the Street Brawl items (those the game has switched off are greyed out at the end) and Misc for the game's other item pictures, such as seasonal ones. Item pictures sit on a dark tile in the list and in the large preview. Icons load only when you open the page. Item icons are the same for every hero, so a replaced item icon shows for all heroes while the mod is installed.
- **Extras → Items: rename an item.** The item's name is a field at the top of the side panel: type a new name and click **Save** (or press Enter); **Use the game's name** puts the game's name back. The item's card and tooltip title in the shop show your name, the shop's search finds it by the new name and the old search words, and the effect name shown while the item is working follows too. Renamed items are the same for every hero, and the panel shows the game's name under yours. Because the game takes a mod's name list in place of its own, the build writes the game's whole list with only your names changed, for every language the game supports: after a game update that adds items, build the mod again. The app does not install a list made from an older game version, and Installed mods labels such a file "Item names" and says when it is missing names the game now has. Voice lines and descriptions keep the game's names.
- **Extras → Colors, rebuilt.** The character is shown in 3D beside the material list and the editor: click a part to choose its material, switch between the original colors and yours, and point at a material to light it up on the character. The editor has three steps in the order they apply: recolor parts, whole material, tint. A new color picker takes a hex value, HSB or RGB sliders on color tracks and typed values, and keeps your recent colors. Color ranges have clearer sliders: the selected hue band, and two-handle saturation and brightness ranges. The page says when a range selects nothing or when a tint hides your recolored parts, and the Undo and Redo buttons in its top bar use the project's history. A Weapon filter lists the materials the hero's weapon (or your own gun) is drawn with and shows the weapon in the 3D view.
- **Extras → Colors: click a color on the character and choose its new color.** The page sets up the color range for you and lights up what it covers first. A color can also be picked on the image in the Texture view, which shows the whole image at every window size, with the click, the outline and the image in agreement. A pick reads a small area under the pointer rather than one pixel, and a click on a see-through layer of another material in front picks the selected material behind it. Grey, white and black parts can be recolored too: they have no hue of their own, so they are given the new color (its hue and strength are set, and their light and shade are kept). A material whose image is one flat color says so and points to Tint and Whole material.
- **Extras → Colors: This part only.** A color picked on the character can be changed on **This part only** (the connected piece of the model you clicked) or **Everywhere this color**, each showing how much of the material it selects. Pieces that share the same area of the image change together (mirrored halves, for example), and some pieces are large. A project that uses grey, white or black recolors or This part only opens only in 0.14.0 or later (0.13.0 refuses it with "Unsupported character extras version").
- **Extras → Colors edits your replacement model too.** With a replacement model the page changes your model's materials (tint, hue, saturation, brightness and color ranges), the same values as the surface changes on the Appearance page. Color ranges are new for replacement materials. While no replacement model is chosen, the page offers `Recolor the game's own <hero> instead`.
- **Extras → Sounds: tags and readable names.** Sounds are tagged Hero, Ability, Weapon and Voice (a sound can have several), named in plain words and grouped by ability or kind, with filters for voice lines such as pings, emotes, kills and lines other heroes say about this hero. Variations of one sound are one row: pick a variation to replace it on its own, or replace all of them with one file.
- **Extras → Sounds: an editor for a replaced sound.** Choose which part of your file plays by dragging a frame as long as the game's sound, set the volume in dB, and fade in and out. You can listen to the game's sound and to yours (exactly what the build writes) side by side. "Match the game's loudness" sets the volume so your sound plays about as loud as the game's, the volume never clips, and "Replace all N variations with this part" gives every variation the same file and the same edit. The editor opens songs and other long files (up to 20 minutes), and a long file chosen for a sound leads straight to it.
- Extras → Sounds: silence a game sound without a file (Silence this sound, or Silence all N variations), for example the cast sounds of an ability whose main sound you replaced.
- Extras: Mo & Krill's ability and movement sounds and their crosshair images are now listed as the hero's own. Extras remembers which tab you had open, and asks you to choose a hero first instead of showing an error when none is chosen.

#### Build & install

- **Build & install → Installed mods says what every VPK is.** For every file in the addons folder it shows the heroes it belongs to and what it changes (character model, gun, materials, sounds, icons, effects, ability objects, item icons, item names, other files), read from the files themselves, so it stays right when a mod loader renames or renumbers them. Files this app installed show their project (with **Open its project**), two files that change the same game files are flagged, and you can give any file a label of your own that stays with its contents. Filter the list by hero or by where a file comes from, search it, and open the addons folder or show a file in it.
- **Build & install keeps the list of items to review** from the last Analyze. Items tick off as you handle them, so there is no need to analyze again after each change, and once nothing is left you can build straight away: the build checks the project again before it starts.
- Build & install lists the extras a build includes, and offers **Build the extras without a replacement model** when no model is chosen.

#### Prepare model, Appearance & weapons, Physics & preview

- **Prepare model has a new layout**, like Physics & preview: a top bar with your file, its stats and menus (Model file & orientation, preparation mode, Scene objects, Notes), tabs with a status each, the 3D view beside every tab, and Fit & review as five groups of option cards. The Bone mapping and Weapon tabs are now Joint matches and Gun. Body roles shows the joints as rows with region chips, Moving parts uses switches with the detected groups always in view, Joint matches offers its quick fixes as cards above one row per bone, and the gun's fit preview sits beside its settings with the adjust buttons under it (the same panel on Appearance & weapons). Once Joint check is loaded, the joints, the selected bone and the kept moving parts are labelled on the character, and the part of your model a selected bone moves lights up.
- **Model fitting → Leg joints** (with Preserve source proportions), for knees that stretch when the character jumps or crouches: the recommended choice puts the hips, knees and ankles on the hero's joints without resizing the character. You can also only place the body by its hip joints, or resize the whole character to the hero's leg length (which can stretch the shoulders). The default keeps the fit as before. After preparing, the page shows how far the model's knees sit from the hero's and the character's size.
- **Appearance & weapons has a new layout.** Materials, Face, Gun and Accessories & props tabs sit beside the 3D view, with Model, Mesh options, Texture folder and Saved inputs menus in the top bar. Click a part of your character to choose its material, see each material's picture and status, and step through what is left with Next to review. Colors are changed in Extras → Colors; opacity, glow, roughness and metalness changes stay on the material. **Accept all** is a green button while there is something to accept.
- **Physics & preview has a new layout.** Tabs for Physics mode, Fine-tune, Moving parts and Tested cloth setup sit beside the character preview, which stays in place while you switch. Fine-tune is always open: pick a part type, then use larger sliders with a box for an exact percentage and a quick way back to Automatic. Labels on the character open each part type's sliders and glide with the model instead of jumping when you turn it, moving parts have a search and a filter, and the breast, butt, ear and gun charm switches, the preset choice and your saved presets are all within reach. The preview grows with the window. A line above the Fine-tune sliders and the Moving parts list says when the sliders cannot be seen working and offers the button that fixes it (See it move, Show my character, or Go to Build & install); changing a slider while the preview is on Still highlights that line once.
- **Physics & preview: Movement simulates the game's own cloth model.** For Automatic secondary motion the Movement view moves hair, cloth, tails and other moving parts with the same points, links, limits and colliders the game uses, so they trail and settle much closer to what you see in game, and Fine-tune changes still show right away. Movement also plays running back, left and right, a stop and a long fall beside stand, run and jump, at the hero's own speeds and with the hero's own lean, and the jump note says how high and how long the jump is. Analyze again on projects analyzed with an earlier version.
- **Physics & preview: the character preview shows your model's texture colors**, including your color edits exactly as the build makes them, in the still and the movement view. Hidden face layers such as blush and tears stay hidden. Solid is one click away. Coming back to the page shows the textures at once; only images you changed, repainted or recolored are loaded again. The page remembers Still or Movement and Solid or Textures while the app is open.

#### Projects and settings

- **Undo and Redo** for your project's choices, with buttons next to the project name and Ctrl+Z / Ctrl+Y (or Ctrl+Shift+Z). A slider drag is one step. Undo stops at the last preparation, analysis or build.
- **Make a copy for another hero** (Character & model): puts the same character on a second hero as a new project. It keeps your model and gun files, body roles, moving parts, material and color choices, face choices, physics fine-tuning, item icons, item names and fit choices, and lists what it could not bring because it belongs to the first hero (sounds, effects, hero icons, animations, hero colors, the gun fit). The original project is not changed.
- **Load 3D previews automatically** (Settings → Appearance, off by default): Prepare model, Appearance & weapons and Physics & preview load their 3D view by themselves when the page is shown, once per model, when nothing else is running.
- File windows open in the folder you last used for the same kind of file: projects where you keep projects, models where your models are, sounds where your sounds are.
- **MikuMikuDance models (.pmx, .pmd)** can be dropped in or chosen directly, as the character or as a gun file. They are imported with the free MMD Tools add-on in the Blender set in Settings, using the add-on's default import settings. Settings shows whether MMD Tools is installed for that Blender, with a link to the add-on's page when it is missing, and preparation explains how to install it.

### Changed

- **Preserve source proportions is the default fit** for a model checked for the first time in a project, rigged or without a skeleton. Settings → Appearance → Default fit for new models can switch back to Match hero proportions. Projects checked before keep their fit. The project overview says Preserve source proportions, the same name the fit card uses.
- **Automatic movement: breasts have a slow, soft bounce** that lags the body and settles, with every slider at Automatic. They are no longer braced to the chest and to each other, so they no longer bounce fast, turn around or clip at a large Swing range, and the character's own arms no longer push them (for example when shooting). Stiffer, More movement and the Fine-tune sliders still set how much they jiggle. If you had tuned the Breasts sliders before, reset them to Automatic first. Butt motion is unchanged.
- Automatic hair no longer floats up during long falls: every strand has a swing and height limit. The root of each strand stays on the head and the hair next to it stays close, so the head no longer shows through when the character moves. Short hair settles under light gravity; long hair moves as before. The base of an ear (three or more joints) also stays on the head, and the ear is a little firmer.
- Automatic movement: a tail modelled sticking straight out behind the character (as MMD models often are) hangs under gravity instead of pointing straight out. Tails modelled hanging, curved or pointing up move as before.
- Moving parts: breast bones named in Chinese pinyin (such as Xiongbu_L, Xiong_R, RuFang_L, or with the side first, L_XiongBu) and butt bones in joined pinyin (PiGu_L) are recognised, also when each breast has a base and surface bones, and a rig's own unrecognised chest bones count when the app measures the chest to add breast bones.
- Automatic body detection reads bone names that share a studio prefix, picks the legs by the skin they carry rather than floor-length ropes or ribbons, moves the head to the joint that carries the face, and gets the spine and chest from numbered spine bones (`Spine`, `Spine1`, `Spine2`), saying in the suggestion checks when a guess is doubtful.
- Prepare model says when a file's mesh has bone weights but no skeleton (the armature was deleted or not saved with it), and what to choose instead. In Simple mode such a file stops before preparing and asks whether to continue with automatic weights or choose another file.
- Prepare model: correcting a body role by hand keeps the moving parts and the bone matches in step with it, and anything left to fix shows as one item with a button that fixes it. A shape key choice in Fit & review can be cleared again.
- **Models whose textures have long names prepare without renaming anything.** A texture whose copy in the workspace would be too long for Blender is placed beside the model, under a short name if needed, and keeps its name in the app. This covers FBX and glTF models; a Blender or MMD model with such a texture says which file is too long.
- Textures saved under another file type are found: when a model asks for `Body.tga` or `Body.psd` and only `Body.jpg` is beside it, that image is used (with a note), and materials with no image take the one whose file name matches theirs, for review in Materials.
- Preparing a model again keeps the images you chose and offers the image the model links to a material as a choice ("Use it") without asking you to review the material again. Analyze materials no longer offers "another image" that is a copy of the image a material already uses.
- Build & install → Mesh mapping offers its suggested mapping also when only some of your model's meshes are named like the hero's parts. The suggestion puts every mesh no part uses on the body in one click, keeps a hero's weapon parts whatever they are named, and keeps the game's own model for things a hero carries (McGinnis's turret and canister while she places them) instead of hiding them.
- Physics & preview says when the last Analyze stopped because mesh mappings still need a choice. The movement preview loads much less data, and switching Moving parts on or off while paused updates the picture at once.
- The status bar says what is being done while work runs.
- The game's color textures load much faster in Extras.

### Fixed

- A hero part named like one of the hero's bones or body groups (Rem's `head`, for example) can be switched off or given several meshes in Mesh mapping; the build used to stop with "Cannot safely expand unknown parent_bone reference to head." or "Disabling head leaves an unsupported parent_bone reference" (issue #10).
- An effect that draws a hero's model (for example the figures of Haze's Bullet Dance) could not be built: the build stopped with "Character component dependency is missing" over a file that the game's own model names but the game does not ship.
- Physics & preview: Movement waits its turn instead of answering "Another operation is running" while the character or an analysis is still loading.
- Movement preview: the jump lands properly instead of twisting the body over at the landing (Lady Geist and other heroes whose landing is an additive clip), and a bone that only the in-air clip moves no longer jumps at take-off and landing. The eight heroes whose in-air animation is split in two (McGinnis and Graves among them) have Fall and a jump with its time in the air, McGinnis runs with her ordinary run, Ivy stands in her gun-ready stand and Drifter's jump starts with a jump instead of a dash.
- Your own gun is drawn once on heroes whose model shows several gun parts at the same time.
- A project that was never saved still asks before New or Open replaces it after a build.
- A file chosen as an icon that is not a picture is said so when you choose it and at Analyze, and the build names the file.
- Extras → Icons and Items: some of the game's pictures were previewed dark and in wrong colors; they show as they do in game.
- Icon and effect lists no longer show two rows under one name.
- Previewing a glTF exported from Blender no longer fails when a mesh has no UV map; its materials are shown without their image, with a note.
- Materials no longer show "Found by: Blender image connection" next to an image you chose yourself or for an image that was not found, and a JPEG standing in for a missing image is no longer used for opacity when Analyze materials runs after Prepare model.
- An ASCII FBX file gets help that says how to save it so the app can read it.

## 0.13.0 — 2026-11-15

- **Reduced CSDK 13** is supported and is now the default. It compiles the materials and the hero model, so no separate model compiler is needed. With the model compiler backend on auto, a chosen Reduced CSDK 13 compiles the models; a downloaded model toolset is used only without it. Setups that explicitly use a toolset keep it and get a one-button switch in Settings.
- Simpler setup: four things to install (Deadlock, Blender, Source 2 Viewer CLI and Reduced CSDK 13). The Blender path is in the main list of Settings, the download list shows only these four, and the model toolset page moved to Settings → Advanced configuration → Downloaded model toolset (older setups).
- With Reduced CSDK 12 chosen, the sidebar's Notifications show the steps to move to Reduced CSDK 13, with a **Find Reduced CSDK 13** button. Nothing switches by itself.
- **Notifications** in the sidebar, above the version: the Reduced CSDK 12 notice, every error, and the results of Analyze, Prepare model, Build, Install and Test my setup, each with a green, yellow or red badge; click one to open it. **Clear** empties the list for now, **Ignore** hides the Reduced CSDK 12 notice for good (Settings can show it again).
  An error now shows one line at the top of the page; **Show details** opens the whole error with **Copy for support**.
- Detect tools no longer picks a Counter-Strike tools folder as the model compiler, finds the `Reduced_CSDK_13` folder the ZIP extracts to, and chooses among installed Blender versions by version number.
- Blender 5.3 is accepted. Blender 5.2 LTS stays the recommended version.
- Source 2 Viewer CLI 21.1 works with the app: the gun-fit and movement previews find the right animations with either CLI version.
- Fix (issue #7): automatic preparation of an FBX model stopped with "The system cannot find the path specified. (os error 3)" when a folder below the model held a picture with a very long name. Only the images the FBX uses are copied now, long paths no longer break the copy, and a texture whose path would be too long for Blender gets a message naming the file.
- Fix: Blender closed without an error while inspecting a model whose armature has an object-level driver.
- The material cache notices changes to Reduced CSDK 13's shader folders.

## 0.13.0-beta.14 — 2026-10-25

### New

- Model preparation → Fit & review → **Feet on the floor**, for feet that sink into the floor or float: **Line up the ankles** (the default, the fit as before), **Shorten or lengthen the legs** or **Move the whole character up or down**. Prepare model says how far the soles were from the floor. The converted model's 3D view draws its grid at the game's floor.
- Model preparation → Gun: pitch (muzzle up or down) and yaw (left or right) around the grip besides roll, with 5° buttons and exact fields under Advanced, for each hand.
- Automatic movement: arm colliders, so cloth hanging from the shoulder stops at the arm instead of passing through it.
- Automatic movement: strips hanging from an arm (shoulder capes) follow the arm the way Valve's own arm cloth does, instead of lagging behind it.
- Automatic movement: long hanging strips inside a cloth part (cloak panels, ribbons, capes) move like streamers instead of stiff cloth.
- Automatic movement: hair reaching past the knees hangs under gravity.
- Build & install → Mesh mapping: the "Not used by any render node" box and its **Add it to …** button show even while a suggested mapping is waiting.

### Fixed

- A gun from its own file whose mirrored halves are named for the hands was split into two guns. It stays one gun now; prepare again to use it.
- Analyze stopped with "Preview cannot triangulate a self-intersecting polygon" (or "…this DMX polygon") on models with flat folded faces or corners at one spot. Such faces are accepted now.
- Build & install stayed blocked with "VPK slot … is in use by another mod" after the VPK was deleted while the app was open. Find free slot refreshes the slot, the page rereads the addons folder every 3 seconds, and the project's own installed build is recognized.
- Rebuilding into the same slot asked to confirm replacing the project's own earlier output. Build now replaces it (keeping a backup).
- Long braids moved as a stiff accessory. "plait" (and 辫/辮, 三つ編) counts as a hair word, so braids get hair motion and twist protection.
- Build stopped with "Missing or ambiguous generated bone" when moving-part bone names differed only in non-English characters. Such bones get distinct names; older preparations stop at Analyze with help to convert again.
- A cape hanging from a breast bone (MMD-style rigs) moved as a breast. It moves as cloth now.
- Adding a gun file whose parts carry modifiers could stop with a UnicodeDecodeError.
- "Invalid compiled mesh remapping table" now says which mesh and bone are affected, with help (issue #6).

## 0.13.0-beta.13 — 2026-10-20

- Nurse Harrow (released in the game on October 20, 2026) is tagged In game.
- Fix: automatic movement (and Preserve original physics with kept physics bones) could stop with "Bind transform outside original-physics transfer budget" on a bone pointing almost along its parent's axis. Preparation now turns such bones exactly onto the axis; convert again to get it.
- Error help for "Skeleton reference belongs to different original model": convert and Use prepared model again after a game update changed the hero.
- Fix: Match hero proportions stopped with "Prepared geometry does not match the selected Model fitting option" on rigs with body-following bones.
- Fix (issue #5): automatic movement's "do not contain a preserved secondary rig" error now has its own message and help for each cause. Kept bones only in meshes left out of the build are a Mesh mapping review instead of an Analyze error.
- Physics page: says when the rig has no moving parts at all, and explains that Preserve original physics keeps the hero's own hair and cloth while your model's stay still.

## 0.13.0-beta.12 — 2026-10-20

- Fix (issue #4): automatic movement stopped with a "rest constraints Failed" error when a body collider (head, neck, legs or hips) covered the fixed points of a moving part. Colliders now stay clear of them, and the error names the collider.
- Fix (issue #4): choosing the game's own model as the tested cloth reference now explains that it is the hero's own game cloth and points to Preserve original physics.

## 0.13.0-beta.11 — 2026-10-18

- **Colors** in Extras: recolor the hero's own materials (tint, saturation, brightness) without a replacement model, with a preview. The mod contains only the changed materials.
- **Color ranges** in Extras → Colors: recolor only part of a hero material with a hue shift, picked on the Game preview. The build writes a recolored copy of the texture; the game's own files stay unchanged.
- Extras now shows in Simple mode too.
- Fix (issue #3): builds that keep the hero's own cloth stopped with "Unknown KVType for variable 'datatype': 26" after the game's October 16, 2026 update. The bundled cloth reader reads the new value types.
- Fix (issue #3): Preserve original physics builds with kept moving-part bones stopped with "Bind transform outside original-physics transfer budget" or "Compiled skeleton bind transform changed".

## 0.13.0-beta.10 — 2026-10-18

- Fix (issue #2): with Steam mobile QR sign-in, the QR code appears on screen when Steam asks for it, renews with it and closes when sign-in is done.
- QR sign-in needs one scan for Download instead of three, when one DepotDownloader run can fetch exactly the planned files.
- **Sign in to Steam…** (Set up model compiler…) saves a sign-in for a username account in its own console window. The app never sees the password.
- Error help for Steam sign-in failures says what happened and what to click.
- Choosing the wrong folder as the model toolset is explained.
- The manual download guide is a page of five numbered steps with Copy buttons; instructions.txt has the same steps.

## 0.13.0-beta.9 — 2026-10-17

### New

- **Face details** on the Appearance page: layers a model keeps invisible until an expression shows them (MMD blush and tears) are hidden in game, with switches to show them.
- **Head and neck colliders** for automatic movement keep hair out of the face.
- Preparing a model again replaces the textures the app picked automatically; your own choices are kept.
- Long skirts ending around mid-thigh are recognized, and shorter panels join the skirt.
- Floor-length sleeves move as streamers instead of stiff cloth.
- Hair buns stay still (Rigid) and follow the head.

### Fixed

- MMD models got their sphere (shine) maps as main texture, or no texture for .bmp sphere maps. The base texture is used now.
- Helper bones under a hair bun no longer fling it around.

## 0.13.0-beta.8 — 2026-10-17

### New

- **Leg and hip colliders** for automatic movement keep skirts, coat tails and tails out of the legs and body.
- **Ear motion** switch on the Physics tab.
- Better part types: ribbons and tassels get streamer motion, left/right pairs get the same type, and ribbons over the breasts are no longer taken for breasts.

### Fixed

- Automatic movement had almost no gravity in game. Gravity is now written in the game's unit, and the Fine-tune gravity slider works in game. Rebuild to get it.
- On Violet, all moving parts stood still for about two seconds when she started or stopped running, jumped, dashed or slid. This applies to every hero whose own cloth is stiffened by name.
- Long skirts: a shorter panel between long panels moves with the skirt, and skirt joints no longer pass into the body.
- Streamers no longer fold up on quick turns or stretch when the hero starts moving.
- Parts set to stay still (Rigid, Ear motion off, gun charms off) are no longer simulated, so they cannot bounce.
- Builds since beta 6 that use the game's compiled model stopped with "Missing or ambiguous generated bone" at the attachments step.

## 0.13.0-beta.7 — 2026-10-17

- Fix: **Update** under Build & install → Installed mods stopped with "Package output must be outside its source folder".

## 0.13.0-beta.6 — 2026-10-16

- **Game update of October 16, 2026:** hero-model mods compiled before it have their camera 90 degrees off. **Build & install → Installed mods** lists every VPK in the addons folder and **Update** repairs the affected hero models in place, keeping a backup. New builds take the attachments from the game's compiled model.

## 0.13.0-beta.5 — 2026-10-16

- **Violet** is offered as a playable hero. Whether a hero is playable now comes from the installed game's hero definitions.

## 0.13.0-beta.4 — 2026-10-15

- Maintenance release. No change to how the app works.

## 0.13.0-beta.3 — 2026-10-14

### New

- **Movement** runs Analyze project by itself when the moving parts have not been analyzed yet, and has an **Analyze now** button.
- **Themes** (Settings → Appearance): Original, Violet D and Mint D, including the sidebar mark and the window icon.
- **Theme customizer** (Settings → Appearance → Customize…): build a theme from three colors with a live preview and save it as a preset.
- **Show New-user setup in the sidebar** setting; the guide stays under Getting started until **Done**.
- **Fine-tune movement**: **Reset all…**, redrawn sliders with − and + buttons in 5% steps, and a double-click to reset a slider.
- **Build & install** shows the last completed build beside the page title and a green **Completed** after a successful model compilation, and Analyze project checks the VPK slot with a **Choose free slot** button.
- **Simple and Expert interface** (Simple by default), a calmer Prepare model page with one 3D view, and Build settings grouped under **Customize build**.
- **Movement preview**: your character plays the hero's stand, run and jump with its moving parts swinging; the Fine-tune sliders change it right away.
- **Start from your model's own spring settings** (VRM spring bones, MMD rigid bodies), off by default.
- **Fine-tune movement**: Stiffness, Damping, Gravity and Swing range sliders for each part type (listed before the first Analyze too), and **saved physics presets**.
- **Progress bar** and **Time left** in the status bar.
- **Hero picker tags**: In game, Alternate, Not playable yet, In development.
- **Test my setup**: checks every path, compiles a test material and asks the model compiler which format it reads.
- **Autosave and recovery** every 3 minutes, with restore after a crash.
- **Readable material names**: Chinese, Japanese and Korean names are spelled in Latin letters.
- **Gun file first**: choosing a character asks whether the gun is in a separate file before preparation starts.
- The Simple/Expert switch sits next to New in the top bar, and Materials highlights **Accept all** while materials wait.
- **Both guns** choice for dual-wield heroes, and a **holding pose** in the gun fit preview.
- **Checks before building**: Deadlock running, low disk space, folders inside OneDrive.
- **Proportion notice** for heroes with extreme proportions, and **Shiv and Wraith with Keep source proportions** scaled to the hero.
- **What changed since the last build**, **keyboard shortcuts** (Ctrl+B, Ctrl+1 to Ctrl+5) and **private logs** with **Copy for support**.
- **Setup that checks itself**: every path is checked as you type, with **Use …** and **Fix all marked paths** fixes. The setup guide opens by itself on first start.
- **Model compiler setup finishes itself** after a passing download or verification.
- **Errors say what to do**: plain words, numbered steps and a button to the page where the fix happens.
- **Butt motion**, separate from breasts, off unless you turn it on.
- Long hair and thin tails without names move, and long strands are kept from passing into the body.

### Fixed

- The Movement preview's moving parts went wild while running or jumping; they now swing relative to the bone that carries them and settle calmly.
- After a successful build the checklist no longer says the project changed.
- Every page shares one background and card look per theme.
- Prepare model's 3D views failed when a Blender file pointed at a texture missing on this PC.
- Baba could not be used as the hero ("unknown KV3 type 27").
- After the October 2026 game files, Build stopped with "ModelDoc28 cannot mix modern Softbody with legacy cloth containers" or "Compile template bone/hierarchy does not match original hero" on many heroes.
- Turning on Butt motion made the next Analyze fail with "Choose a supported moving-part type".
- Build said "Another operation is running" after the pre-build warning.
- A new user's setup looked finished while hero models would be compiled with the Community SDK, which fails; Detect tools no longer does that.
- Detect tools picked the wrong `resourcecompiler.exe` from Reduced CSDK 12, and now searches more folders.
- A butt could be detected as breasts, and strips hanging from a breast bone could flap through the body.
- Simple mode no longer shows an empty "More customization" heading.
- Privacy: a build made on your own PC no longer carries your Windows user folder, and the Steam account name is left out of command receipts.

## 0.13.0-beta.1 — 2026-10-13

### New

- **Five clear steps** in the sidebar: Character & model, Prepare model, Appearance & weapons, Physics & preview, Build & install.
- **Automatic preparation**: a model chosen in step 1 is checked, matched and prepared in one go, stopping only where a choice is needed.
- **Gun from a separate file** on the Weapon tab, and weapon model projects that accept a gun-only file.
- **Gun charms** that swing, on the character's gun or a gun file's gun, with **Physics → Gun charm motion**. Charm names beat rigid accessory names, so a dangle earring moves.
- **Dual-wield heroes (Haze)**: each gun is fitted on its own, with **Swap hands**.
- Gun card rewritten for new users, with a live solid 3D fit preview, View and Show buttons, and simple Flip, Turn, Smaller, Bigger and move buttons.
- Separate gun models are created, prepared and linked from the same tab.
- Appearance & weapons shows the prepared model's gun with **Apply gun changes**, and Physics & preview has its own 3D view.
- Gun direction from part names (Muzzle, Barrel, Stock, Grip …), and gun parts merged into one mesh per gun.
- **Fingers bend like the hero's** around the gun (on by default).
- Faster preparation and builds, and much smaller preparation folders.
- **VPK slots**: the chosen slot's state is shown on the Build page.
- New-user setup and Model tool setup use the step layout of the mod flow.
- Gun materials that use the same textures become one material.

### Fixed

- Haze's guns were reported as a separate model.
- Flat gun parts were thrown away as ground or backdrop.
- Models with faces of more than four corners failed with "Tangent space can only be computed for tris/quads".
- After Flip direction the muzzle stayed at the old end.
- Build failed with "Path Overflow, must be less than 260 characters" on long material names; Build now checks the compiler's paths first.
- Textures baked to a second UV map did not line up in game.
- A texture connected to Base Color was tinted with the hidden socket color.
- Appearance & weapons said "Your gun file changed" for the same file written another way.

## 0.13.0-alpha.52 — 2026-10-11

- **Settings → Storage** shows disk use per project and clears data the app can recreate, with automatic retention.
- Optional **breast motion**, including breast bones added to models without them, with a switch on the Physics tab.
- Halos, hats, hairpins and other rigid accessories no longer get hair motion; new **Rigid** part type.
- Experimental color editor under Appearance → Surface changes → Adjust colors.
- Original hero files extracted from the game are shared per hero and game version instead of copied into every project.
- Rem's main target is the model the game loads.
- Fixes for Analyze and Build failures with breast groups carrying ribbons and with MMD tip bones.

## 0.13.0-alpha.51 — 2026-10-11

- Extras finds hero and ability images from the game's definitions, with fixes for missing images.
- Prepare almost any rigged humanoid, with a **Scene objects** list to keep or leave out every object.
- Gun customization and gun-only weapon projects; textures from game rips reach Appearance automatically.
- More robes, cloaks and other moving parts are recognized; **Mesh mapping** uses checkboxes and warns about unused meshes.
- Windows: settings keep ordinary `C:\` paths, and build output folders are published without replacing existing folders.

## 0.13.0-alpha.50 — 2026-10-11

- Recover rigs with disconnected roots and missing primary joints.
- Support two-spine Biped layouts and ValveBiped, DEF and VRM J_Bip names.
- Map complete Biped and Mixamo finger chains.

## 0.13.0-alpha.49 — 2026-10-10

- Import modular characters with separate body and face or part rigs.
- Leave detached props out of character preparation, with an explanation of what was merged or excluded.

## 0.13.0-alpha.48 — 2026-10-10

- Dragging files into the app keeps the current tab and selected asset.
- Image drops go to the selected Extras icon, also with Windows display scaling.

## 0.13.0-alpha.47 — 2026-10-10

- Fix image resizing that could turn previews and replacement images nearly black.
- Fix Extras refusing ordinary compiled hero icons as view-only, and add a replacement path that needs no compiler.
- More texture formats can be edited.

## 0.13.0-alpha.46 — 2026-10-10

- Skirt fitting and automatic restraints for hoods and upper garment attachments.
- Refreshed pages with larger controls and layouts that use wide windows.
- An installer and a complete portable ZIP for Windows.
