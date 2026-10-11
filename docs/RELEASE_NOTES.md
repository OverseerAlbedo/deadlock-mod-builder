# Deadlock Mod Builder 0.13.0-alpha.51

This update fixes missing character images in **Extras** and makes model preparation work with far more source files.

## Model preparation

- **Almost any rigged humanoid.** Game rips, cluttered Blender scenes and rigs with unusual bone names are recognised. **Scene objects** lists every object and what happened to it; you can keep or leave out any of them. A detached prop on its own rig can be kept and is attached to the nearest bone.
- **Gun customization.** Replace the hero's gun with one from your file, leave it out, or keep it in hand. Separate weapon models can be built as gun-only projects.
- **Textures.** Base-colour images from game rips (wired straight into the material output, or into a Diffuse, Emission or Toon colour) now reach **Appearance** automatically.
- **Moving parts.** Robes, sashes, scarves, cloaks and similar chains are recognised as moving parts, as are long unnamed chains hanging from the body.
- **Mesh mapping.** Checkboxes replace the Ctrl+click list. A suggested mapping can be applied with one click, and source meshes that no render node uses are named with a one-click fix, because they would be missing in game.

## Extras icons

- Recognize the game's older image names for Abrams, Lady Geist, Dynamo, Vyper, Seven and Mo & Krill.
- Follow the selected hero's image and ability references in the installed game, so artwork no longer needs the character's name in its filename to be found. Retain alternate and unused images discovered by name.
- Show compiled vector artwork, including hero-name logos, with previews. Vector artwork is view-only; normal texture replacement remains available.
- Label potentially shared images and require **Include this shared replacement** before replacing them.
- Explain when a game definition refers to an image that is absent from the selected archive.

## Test this build

Install the Windows x64 setup EXE. Open your project, select a character, go to **Extras**, click **Reload assets** (or **Load character assets** if the catalog is empty), and select **Icons**. Test Abrams and Lady Geist first, then switch characters and reload their assets. Use the original game's `pak01_dir.vpk` configured in Settings.

Projects prepared before this update keep working. To pick up the new texture and moving-part detection, press **Inspect source** again on the **Prepare** page and prepare once more, then check **Mesh mapping** on **Build**. The accepted cloth/hair implementation is preserved.

## Coverage and limits

The inventory check covered all 44 heroes and 3 alternate model entries in the app against a game snapshot dated October 11, 2026. No available image explicitly referenced by their hero and reachable ability definitions was omitted. Abrams has 18 textures and 3 vectors; Lady Geist has 19 textures and 5 vectors in that snapshot. Counts may change with game updates.

Three portrait references have no corresponding file in that game inventory: Deadman Danny (`deadpack_card`), Nurse Harrow (`nurse_card`) and Violet (`artist_card`). Their available images are still listed, and the missing references produce a notice.

The inventory audit verifies filenames and reference discovery. It does not certify every image payload or Windows rendering. Frontend tests, component tests, real compiled vector/definition fixtures and installer checks are documented in **RELEASE_VALIDATION.md**. Windows and Deadlock were not executed here. No public release or Git tag was published.
