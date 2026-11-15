# Changelog

## 0.13.0-beta.15 — 2026-11-15

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
