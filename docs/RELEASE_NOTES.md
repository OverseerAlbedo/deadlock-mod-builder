# Deadlock Mod Builder 0.13.0-beta.6

A fix for the game update of October 16, 2026, which broke the camera of every hero-model mod built before it. Everything from the earlier betas is still in; see `CHANGELOG.md`.

## What happened

The update changed how the game stores attachments parented to the `root_motion` bone: that bone's rotation is no longer baked into them. The game's own heroes were recompiled, but a mod compiled before October 16 still carries the old values, so its camera pivots sit 90 degrees off in a match. This affects every VPK that replaces a hero model, whichever tool made it. VPKs that only replace materials, sounds or icons are fine.

## What to do

- Open **Build & install → Installed mods** and click **Check installed mods**. Every VPK in the game's addons folder is listed with its hero and state.
- Click **Update**. The affected hero models are rewritten in place: their attachments take the game's current values, custom attachments on the root bone are converted by the same rule, and the previous file is kept as a backup under the app's workspace (`mod-updates\backups`). The list includes mods made by other people that you have installed; updating those is your choice.
- Or rebuild your project. Builds now take the attachments from the game's compiled model after compiling, so a rebuild is correct regardless of the decompiler version.

## Checked

- A mod built on October 14 and updated this way decompiles to the same attachment list as the game's current model. Not yet checked in a match; please report what you see.

## After updating the app

- Nothing else changes. Projects, materials, prepared models and themes are kept.
