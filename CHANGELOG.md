# Changelog

## 0.13.0-beta.1 — 2026-10-13

- **Gun from a separate file**: Model preparation's **Weapon** tab has an optional **Gun file** (Blender, FBX, GLB or glTF). Inspect adds that file's gun to the character (cameras, lights, hidden objects, its own rig and ground planes are set aside; every object is listed in **Scene objects** as `gun_file_<name>`), puts its grip in the right hand and offers it as the gun: replace the hero's in-model gun with it, leave it out, or keep it in the hand. Convert adds it the same way. The gun file's path and SHA-256 (with its textures) are part of the preparation: a changed, cleared or newly chosen gun file asks for a new inspection, like a changed source. The file itself is never changed.
- **Weapon model projects accept a gun-only file**: a gun that is one unnamed, unrigged object (or makes up most of the file) is now offered as the gun, with ground planes and backdrops set aside, and its grip is measured from the handle when there is no hand. **Equipment → Weapon model** can create the weapon project from a separate gun file instead of the character file; it starts from the Weapon tab's gun file when one is set.
- **Gun charms**: keychains, pendants, straps and tassels hanging from the character's gun are kept as moving parts. When the gun replaces the hero's in-model gun, the charm keeps its own bones on the fitted gun (same rotation, scale and offset as the gun, charm weights kept) and gets a new **Charm** part type in Build: a light swing that settles quickly, with no gravity so it never sags into the gun and a small travel limit. Kept in the hand, the charm hangs from the hand; left out, it goes with the gun. Charms are found by name (charm, keychain, key chain, keyring, pendant, lanyard, dangle and Japanese/Chinese names) or as a short chain hanging below a bone of the gun, never a slide, magazine or trigger. **Physics → Gun charm motion** turns the swing on or off and sets its amount without converting again. The swing amount has not been checked in game yet. Separate weapon models cannot carry charm motion: none of the game's weapon models has cloth, and weapon projects keep no extra bones.
- Charm names beat rigid accessory names, so a dangle earring or a pendant on a necklace moves instead of staying rigid.
- **Five clear steps**: the sidebar now leads through **1 Character & model → 2 Prepare model → 3 Appearance & weapons → 4 Physics & preview → 5 Build & install**, with a check mark on finished steps; Motion and Extras are under *More customization*. Step 1 picks the hero, your model file and (optionally) a gun file.
- **Automatic preparation**: a Blender, FBX or glTF model chosen in step 1 (or dropped on the window) is checked, matched and prepared automatically and the app moves on to step 3; it stops only where a choice is needed. In Prepare model, **Prepare model** does the check, the conversion and *use this model* in one click (the old tick box, Check review, Convert and Use prepared model sequence is gone). A gun file or Scene objects change re-runs the check by itself. An already prepared model (or a DMX) says so and offers to continue.
- **Gun card rewritten for new users**: a plain summary of the hero's gun, your gun and what will happen; one recommended choice (**Use my gun instead of …'s**), with *Keep the hero's gun* and *keep my gun as an accessory* behind it; guns named by file and hand (“Left gun from Artzy_Tyrannical_Dual_Rig.blend · 284 parts”) instead of every part name; a live 3D preview that updates by itself, shows both hands over a faint outline of the hero, and simple **Flip / Turn 90° / Smaller / Bigger / move** buttons (exact numbers stay under Advanced).
- **Dual-wield heroes (Haze)**: each of the hero's guns is fitted on its own, and a gun file with two guns (parts tagged [L]/[R], .L/.R or left/right) puts one in each hand. The left hand can use its own gun, a copy of the right gun, or keep the hero's left gun; **Swap hands** exchanges them. Each fitted gun replaces only its own hand's original gun at every detail level.
- Fix: Haze's guns were reported as *a separate model* because her decompiled gun files have no named bind list and a null bind state; they are read now. Effect and particle models (`haze_gun_magazine_model_fx`) are no longer offered as weapon models.
- **Separate gun models in the same tab**: for a hero whose gun is its own model, **Use my gun instead of …'s** creates, prepares and links the gun project automatically (saved next to your project) and leaves your gun off the character's body; preview and adjust it there and **Apply changes**. No more Equipment detour or second project to open.
- **Appearance & weapons** shows the gun of the prepared model with the same card; **Apply gun changes** prepares the model again in one click and refreshes the 3D preview of your character. **Physics & preview** has its own 3D view of the character.
- **Gun parts are merged**: a gun's parts are joined into one mesh per gun (284 parts → 1), with materials, bone weights, charm weights, UVs and shading unchanged (checked corner by corner against an unmerged conversion). Faster builds and fewer draw calls; it can be turned off under Advanced.
- **Gun-file charms swing**: a charm on a gun from a gun file (e.g. `charm_pivot → charm_body` in the gun's own rig) keeps its bones on the fitted gun and uses the gun charm motion, like a charm on the character's own gun.
- Fix: flat gun parts in a gun file (grip panels, rims, painted lines) were thrown away as “ground or backdrop”; only real floors and backdrops are set aside now (each part is compared with everything else in the file).
- Fix: models and guns with faces of more than four corners (n-gons) failed to convert with “Tangent space can only be computed for tris/quads”; such faces are split into triangles right before export (models without n-gons are unchanged).
- **Solid gun preview**: the fit preview draws your guns as solid shapes (amber, the left gun teal) in the hero's hands, the hero's original guns see-through and the hero's body faintly, with arrows along the line of fire (white from the original muzzle, coloured from your gun's muzzle). **View** (Angled, Front, Side, Top) and **Show** (this hand, both guns, whole body) buttons; it opens on a close-up of the hand you adjust and keeps your camera while you adjust. Shapes are simplified meshes saved with the inspection (a few hundred KB); older inspections show points until the model is prepared again.
- **Gun direction from part names**: parts named for the gun's front (Muzzle, Barrel, Nose, Emitter, Front …), rear (Rear, Stock, Butt, Back) or grip tell which end is the muzzle, instead of guessing from the shape (the thinner end), which put guns with a wide emitter in backwards. The fit line says how the front was found and the gun's size in words (“the length of Haze's gun × 110%”) instead of a unit-conversion factor.
- Fix: after **Flip direction** the muzzle stayed at the old end, so the reported muzzle distance (and the muzzle marker) were wrong; the muzzle now moves to the new front. A flip against the front the part names gave is pointed out.
- Fix: Build failed with “Path Overflow, must be less than 260 characters” when a long Blender material name (“Optional Frost - Translucent Pale Ice”) met a deep CSDK folder, because the compiler names the textures it makes after the material. Prepared material names keep at most 24 readable characters plus their identity hash (choices and surface changes follow the material when the model is prepared again), staged texture copies keep at most 32, and Build checks the compiler's paths before compiling, saying whether to prepare again or move the CSDK.

- Fix: textures baked to a second UV map (an atlas on “BakeUV” next to the original “UVMap”) did not line up in game, because the game textures with the first UV map and the export kept Blender's order. The UV map a part's materials read their images with is now exported first.
- Fix: a texture connected to Base Color was tinted in game with the colour hidden behind the socket (often the material's old flat colour, e.g. nearly black), which Blender ignores. Such materials now get a white tint, as in Blender; materials without a base-color image keep their colour.
- **Fingers bend like the hero's**: the hero's fingers rest slightly bent (Haze about 45–55° per finger) and the game adds its grip to that pose, so straight source fingers (MMD rigs, most T-pose models) stayed open around the gun. Preparation now bends each mapped finger segment to the hero's resting direction, keeping its length (**Bend fingers like the hero**, on by default, in Conversion review). Prepare the model again to use it.
- Fix: **Appearance & weapons** said “Your gun file changed” (and hid **Apply gun changes**) when the gun file was the same file written with a `\\?\` prefix or other capitals; it now compares paths like the Prepare model step.
- **Faster preparation and smaller preparations**: the Blender conversion reads and writes mesh data in bulk instead of one corner at a time (Yvonne with two guns: about 30 s instead of 44 s; results identical apart from float rounding), the movement probes and the humanoid name checks no longer repeat work per bone, and a preparation folder keeps about 100 MB instead of 900 MB: the receipt lists each mesh's name, counts, materials and bones instead of a second copy of all geometry (the DMX has it), and the raw Blender result is removed once the receipt is written. Opening, accepting, analyzing and building a prepared model no longer parse a 470 MB file.
- **Faster builds**: staged source trees and model sources are read twice per file instead of five times (hashed while copied, verified once after), compiler dependencies extracted from the game archive are hashed in memory instead of read back, and material builds hash a compiled file only when it changed since the previous material, instead of the whole output folder twice per material. The gun-file inspection shares the faster mesh readers (its Python work went from 11 s to 6 s on the dual gun file).
- The Prepare model page serializes the raw inspection only when the *Native source inspection* panel is opened (it can be several MB, and was rebuilt on every change).
- **VPK slots**: the chosen slot's state (free, in use by another VPK, or used by this project's earlier build) is shown on the Build page and read from the game's addons folder again whenever the slot changes or the app window comes back, so a VPK deleted while the app is open frees its slot. The project's own earlier output no longer counts as an occupied slot (Build still asks before replacing it).
- **New-user setup and Model tool setup** use the step layout of the mod flow: numbered stages with check marks, cards, one recommended action per stage, a map of the five mod steps, and plain wording. The underlying actions (detect, save and check tools, acquire, verify, test the hero, use the toolset) are unchanged.
- **Gun materials that use the same textures become one material**: a gun baked to one texture set (one atlas shared by 15 part colours) no longer becomes 15 game materials, each with its own copy of the combined normal/roughness texture. Only gun parts are joined, and only materials the conversion reads identically (same images on the same inputs, same values); the preparation notes say which.

## 0.13.0-alpha.52 — 2026-10-11

- **Settings → Storage** shows disk use per project and per kind of data, including the per-project working copies in the CSDK and model toolset, and clears what the builder can recreate: unused model preparations, older backups, build caches and staging, compiler working folders and leftovers of interrupted operations. **Remove…** deletes all data of a project that is not open; **Find saved projects in a folder…** recognises project files saved by earlier versions. Data that a saved or open project uses, built VPKs and locked workspaces are never removed.
- Automatic retention: Inspect, Convert and Accept keep only the preparation the project uses (and, by default, the previous one); replaced VPKs and original-source backups keep the newest one per file (adjustable). Optionally clear build caches and compiler folders after each successful build (off by default).
- Original hero files extracted from the game are shared per hero and game version instead of copied into every project. Existing project copies are merged on first use.
- Optional **breast motion** (off by default): breast bones found by name (including DAZ `lPectoral`) or as a mirrored pair in front of the chest get a springy bounce that does not sag. The left and right breasts are linked so they move together and cannot swing into each other, and their spring sits straight behind each breast, which removes the glitchy sway of the first version. The bounce is now clearly visible (the first tuning barely moved in game). Adjust per part under Physics → Automatic motion → Adjust moving parts.
- Models **without breast bones**: with breast motion on, Inspect adds two breast bones where the chest has breasts and moves the nearby skin weight to them (flat, male and child chests get none, with the reason shown). Rigs that already have weighted breast bones, including breast bones that carry ribbons or cape strips, are never given extra ones. The source file is unchanged.
- Fix: a breast group with ribbons or cape strips hanging from it (for example Yvonne's `胸上2.L/R`) failed Build with "Native secondary fitting is incomplete", because only the breast bone got motion. The hanging parts now move from the breast like accessories.
- **Breast motion on the Physics tab**: one switch turns the bounce on or off and sets its amount for both breasts. It applies at the next Analyze and Build, without converting again, whenever the prepared model has breast bones (including breast bones from the rig's own physics, as in MMD-style rigs). Model preparation's checkbox is now **Keep or add breast bones** and only decides whether the bones are there.
- Fix: Analyze failed with "Native float32 precision collapses or reverses a cloth proxy face" for moving parts whose tip bone sits beside the bone instead of along it (MMD hip and breast tips such as `尻2.L` → `尻2錘.L` in janeVen). Such rings are now drawn across the chain, and ring connections never use a collapsed face. Ordinary rigs keep their exact motion setup.
- Analyze explains when moving parts or breast motion changed after the model was prepared (convert again and click **Use prepared model**) instead of reporting a missing secondary group.
- Halos, hats, crowns, hairpins, hair ornaments and other rigid accessories no longer receive hair motion; ring-shaped chains are not treated as hair strands. A new **Rigid** part type keeps a part on its bone.
- Experimental color editor: **Appearance → Surface changes → Adjust colors** shifts hue, saturation and brightness of a material's base color with an approximate preview. Build adjusts a copy of the base-color image (or plain color); source files and other materials are unchanged.
- Character targets follow the installed game's hero definitions. Rem's main target is now `familiar_wip.vmdl` (the model the game loads); `familiar.vmdl` is listed as *Not used in game*, and Analyze explains when a saved project targets a model the game does not load.
- Tests: GitHub's build machines use a short 8.3 temp path (`C:\Users\RUNNER~1\...`) that the app resolves to its long spelling, which failed path comparisons in Rust tests only there; CI now runs the build with the runner's own temp folder. UI tests got time limits suited to slower 2-core build machines (30 s per test, 5 s for on-screen waits).

## 0.13.0-alpha.51 — 2026-10-11

- Fix missing Extras images for Abrams (`bull`), Lady Geist (`spectre`), Dynamo (`sumo`) and Vyper (`kali`); include the legacy `giga` and `grappler` ability names for Seven and Mo & Krill.
- Discover images from the installed game's hero and reachable ability definitions, including images without the hero's name and compiled hash variants. Keep name discovery for alternate and unused artwork.
- Show compiled vector logos in Extras with read-only previews. Keep texture replacement and shared-resource confirmation checks in staging and build verification.
- Explain image references whose files are absent from the selected game archive. Add inventory coverage for all 44 roster heroes and 3 alternate models.
- Prepare almost any rigged humanoid: game rips, Blender scenes with cameras, lights, scenery and extra rigs, and rigs with missing or unusual bone names. Every object gets a row in **Scene objects** with what happened to it, and you can keep or leave out any of them. Detached props on their own rig (for example a held flower) can be kept and are attached to the nearest bone; finger and toe bones hand them to the hand or foot.
- Gun customization: replace the hero's gun with one from your file (fitted to grip, barrel and muzzle), leave it out, or keep it in hand. Separate weapon models can be built as gun-only weapon projects.
- Textures from game rips reach **Appearance** automatically: images wired straight into the material output or into a Diffuse/Emission/Toon colour are recognised as the base colour, and prepared `Mesh__Material` names find their images.
- Hair, cloth and other moving parts: robes, sashes, scarves, shawls, mantles, cloaks, tassels and veils (also Japanese names) are recognised, plus long unnamed chains hanging from the trunk. Projects with automatic motion pick up newly detected groups; groups you turned off stay off.
- **Mesh mapping** uses checkboxes, shows a suggested mapping you can apply, and warns about source meshes that no render node uses (they would be missing in game) with a one-click fix.
- Windows: settings keep ordinary `C:\` paths, and build output folders are published without replacing existing folders, retrying while other programs still hold files.
- Preserve Alpha 50 cloth/hair physics tuning, accepted references and existing texture replacement behavior.

## 0.13.0-alpha.50 — 2026-10-11

- Recover qualifying rest-only skin rigs with disconnected anatomical roots and missing primary joints when surviving weighted helpers, corroborating pivots, sides and the complete body layout agree. Recovery uses an effective hierarchy for conversion; original bone matrices and skin data are never rewritten.
- Identify two-spine Biped layouts by their actual hierarchy, and infer unnamed intermediate joints only on unique, fully anchored chains. Support ValveBiped, DEF and VRM J_Bip naming prefixes.
- Map complete Biped and Mixamo finger chains when joint counts and hand ancestry match the target.
- Keep both bulk follow buttons visible. Explain missing roles and stale suggestions, and allow suggestions to refresh using manual roles.
- Preserve existing successful conversions, character/weapon selection, exact hero binds, scale checks, physics tuning and presets. Ambiguous evidence and live controls still need review.

## 0.13.0-alpha.49 — 2026-10-10

- Import modular characters with separate body and face/part rigs when their attachment can be verified. Select the character from weighted anatomy or humanoid geometry, rather than armature names or bone counts.
- Preserve part rest positions, skin weights, UVs, materials and shape keys. Give offset part bones distinct identities instead of collapsing them onto differently placed body bones.
- Exclude clearly detached, unconnected non-character props from character preparation, and show the selected, merged and excluded parts in the inspection notice.
- Keep ambiguous parts, multiple characters and unsupported control/physics bindings blocked with an explanation. Original source files are unchanged.

## 0.13.0-alpha.48 — 2026-10-10

- Keep the current tab and selected asset when dragging files into the app. The window-wide drag handler no longer switches to Appearance before the pointer reaches a replacement box.
- Route image drops to the selected Extras icon, including at Windows display scaling. Accepted model drops still open their model workflow.

## 0.13.0-alpha.47 — 2026-10-10

- Fix float-channel normalization during image resizing, which could turn colorful previews and fitted replacement images nearly black.
- Fix Extras rejecting ordinary compiled hero icons as view-only because of fallback-image data, Panorama processing flags, padded display rectangles, or shortened mip chains.
- Add a compiler-independent Panorama replacement path. Embedded images retain PNG storage; GPU artwork uses RGBA storage with the original dimensions, visible rectangle, mip count, clamp/LOD behavior, and runtime metadata.
- Remove stale fallback artwork and compiler pixel-processing records from rebuilt textures; validate the replacement pixels and metadata before VPK packaging.
- Expand intensity, alpha, two-channel, and embedded JPEG texture handling. Unknown/virtual, array, volume, cube and sprite-sheet layouts still need their own editing workflow.
- Keep the accepted model preparation, cloth, sound replacement, and Windows CI behavior.

## 0.13.0-alpha.46 — 2026-10-10

### Physics and compatibility

- Accepted Graves / Endmin garment behavior, including the restored yellow strap.
- Shared-surface skirt fitting and automatic restraints for qualifying hoods and upper garment attachments.
- Preserve accepted Daki streamers and the qualified Vindicta reference.

### Interface

- Refresh Overview, Appearance, Physics, Motion, Review & build, Step timings, Logs, and Settings to match Model preparation and Extras.
- Larger controls, clearer primary actions, and layouts that use wide windows.

### Release infrastructure

- Replace obsolete Python/Qt/PyInstaller Windows CI with current React/Rust/Tauri tests and packaging.
- Check resource integrity and version consistency before building; preserve fixture bytes on Windows checkout.
- Package an installer and a complete portable ZIP with required references, native reader, and third-party notices.
- Check portable startup on the Windows runner and retain failure diagnostics.
- Rewrite user/developer documentation and exclude retired desktop build artifacts from the repository.

Release infrastructure does not publish automatically. The application version remains Alpha 46; the GitHub handoff does not retune the accepted model or cloth behavior.
