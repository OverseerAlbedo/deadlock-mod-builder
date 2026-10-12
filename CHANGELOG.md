# Changelog

## Unreleased

- **Gun from a separate file**: Model preparation's **Weapon** tab has an optional **Gun file** (Blender, FBX, GLB or glTF). Inspect adds that file's gun to the character (cameras, lights, hidden objects, its own rig and ground planes are set aside; every object is listed in **Scene objects** as `gun_file_<name>`), puts its grip in the right hand and offers it as the gun: replace the hero's in-model gun with it, leave it out, or keep it in the hand. Convert adds it the same way. The gun file's path and SHA-256 (with its textures) are part of the preparation: a changed, cleared or newly chosen gun file asks for a new inspection, like a changed source. The file itself is never changed.
- **Weapon model projects accept a gun-only file**: a gun that is one unnamed, unrigged object (or makes up most of the file) is now offered as the gun, with ground planes and backdrops set aside, and its grip is measured from the handle when there is no hand. **Equipment → Weapon model** can create the weapon project from a separate gun file instead of the character file; it starts from the Weapon tab's gun file when one is set.
- **Gun charms**: keychains, pendants, straps and tassels hanging from the character's gun are kept as moving parts. When the gun replaces the hero's in-model gun, the charm keeps its own bones on the fitted gun (same rotation, scale and offset as the gun, charm weights kept) and gets a new **Charm** part type in Build: a light swing that settles quickly, with no gravity so it never sags into the gun and a small travel limit. Kept in the hand, the charm hangs from the hand; left out, it goes with the gun. Charms are found by name (charm, keychain, key chain, keyring, pendant, lanyard, dangle and Japanese/Chinese names) or as a short chain hanging below a bone of the gun, never a slide, magazine or trigger. **Physics → Gun charm motion** turns the swing on or off and sets its amount without converting again. The swing amount has not been checked in game yet. Separate weapon models cannot carry charm motion: none of the game's weapon models has cloth, and weapon projects keep no extra bones.
- Charm names beat rigid accessory names, so a dangle earring or a pendant on a necklace moves instead of staying rigid.

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
