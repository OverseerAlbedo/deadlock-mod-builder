# Changelog

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
