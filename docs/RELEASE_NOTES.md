# Deadlock Mod Builder 0.13.0-beta.11

**Hero colors** and fixes for builds that keep the hero's own cloth (reported in issue #3). Everything from the earlier betas is still in; see `CHANGELOG.md`.

## New

- **Colors** (Extras → Colors): recolor a hero's own clothes and body without a replacement model, for example Haze's clothes black. Pick a material and change its tint, saturation or brightness; the mod contains only the changed materials.
- **Color ranges:** recolor only part of a material with a real hue shift, for example the clothes but not the skin. Click a color on the preview, narrow the selection (the white selection view shows what is picked), then set the hue shift. A material can hold up to 8 ranges.
- **Extras in Simple mode:** Extras, with Colors, now shows in Simple mode too. Motion and Step timings stay in Expert mode.

## Fixed

- **Builds that keep the hero's own cloth** (Preserve original physics, or moving parts off) stopped with "Unknown KVType for variable 'datatype': 26 (0x1A)" since the game's October 16 update. The bundled cloth reader now reads the new game data. Every hero was affected, not only Silver.
- **Preserve original physics with kept hair bones** stopped with "Bind transform outside original-physics transfer budget" or "Compiled skeleton bind transform changed" on a hair or tail bone. The compiler stores those bones slightly rounded; the build now takes the compiler's exact values for them, as automatic movement already did. No check was loosened.

## Good to know

- Recolored materials that other models also use (marked Shared) change on those models too; you confirm that per material.
- Hue shifts only move colored pixels. Black, white or grey parts cannot become a color with a range; use the tint for white or grey parts.

## Checked

- Checked on the PC on October 18, 2026: Silver with Preserve original physics built and installed; Violet built with automatic movement and with kept hair bones in Preserve mode.
- Not yet checked in a match: Colors, color ranges and Preserve builds with kept hair bones.

## After updating the app

- Nothing to redo. Projects, settings, prepared models and themes are kept.
