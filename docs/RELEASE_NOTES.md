# Deadlock Mod Builder 0.13.0-beta.3

The third beta brings themes with a customizer, a movement preview that behaves like the game, redrawn fine-tuning sliders, and fixes for the build page, the new-user guide and the Blender preview. Everything new is listed in `CHANGELOG.md`.

## Themes

- **Settings → Appearance**: **Original**, **Violet D** and **Mint D**. Every page, the mark in the sidebar and the window icon (title bar and taskbar) follow the choice; it is saved with your settings and survives restarts and reinstalls. Success stays green and errors red in every theme.
- **Customize…** makes your own theme from any of them: three colors (accent, background, text) with a color picker and a hex code each, **Advanced** for every color, a live preview of the whole interface, your choice of window icon, and **Save as preset** to add it to the theme cards (edit or delete it later).
- **Show New-user setup in the sidebar** hides or brings back the Getting started entry.

## Movement

- The **Movement** preview (Physics → Your character) no longer flings hair, cloth and tails about while running or jumping: the parts swing relative to the body, settle within a swing or two, take-off and landing push them gently, and the jump is one movement instead of a float.
- Movement runs **Analyze project** by itself when the moving parts have not been analyzed yet, and the moving parts are recorded by every Analyze — also before Mesh mapping is complete and for the TangTang presets.
- **Fine-tune movement** sliders redrawn: automatic in the middle, **−** / **+** in 5% steps, the value beside each slider, a one-line explanation, double-click to reset, and **Reset all…** with a warning.

## Build & install and setup

- After a successful build the checklist no longer asks you to analyze again; **Completed** shows under Output and beside the page title.
- **Analyze project** checks the VPK slot and offers **Choose free slot** when another mod uses it.
- The new-user guide stays in the sidebar until **Done**; its Hair and cloth step explains automatic movement instead of an old reference setup.
- Prepare model's **Your model** and **Joint check** views no longer fail when a Blender file points at a texture that is not on this PC.

## Privacy

- The app makes no network requests of its own. Builds made from source keep the builder's Windows user folder out of the program, and command receipts leave out the Steam account name given to DepotDownloader.

## Checked in game

- Shiv and Wraith with Keep source proportions look right. Haze and Lady Geist projects compile with heroes decompiled by the newer Source 2 Viewer and the game's cloth is restored (beta 2).
- Not yet checked in game: the movement amounts from Fine-tune, the source spring values, and the finger bend, charm swing and dual-gun aim from beta 1. Please report what you see.

## After updating

- Open your project and run **Analyze project** once; the movement preview and the fine-tuning use it. Material choices and prepared models are kept.
- The theme is Original until you pick another in Settings → Appearance.
